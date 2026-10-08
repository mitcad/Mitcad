// SPDX-License-Identifier: MIT
#include "render/RenderMode.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <string>
#include <utility>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QTimer>
#include <QtLogging>

#include <Graphic3d_Camera.hxx>
#include <gp_Trsf.hxx>

#include "OcctViewer.hpp"
#include "framework/Diagnostics.hpp"
#include "render/RenderBatch.hpp"
#include "render/RenderClient.hpp"
#include "render/RenderDevice.hpp"
#include "render/RenderImage.hpp"
#include "render/RenderMesh.hpp"
#include "render/RenderScene.hpp"

namespace mitcad {

namespace {

// While the camera keeps moving, a new view goes to the worker at most this
// often (each restarts the render).
constexpr int kViewInterval = 30;
// How long a message stays over the view.
constexpr int kMessageMilliseconds = 10000;
// Meshes no scene shows any more that are kept for a while (a body shown
// again, an undone change): the hashes of shapes' meshes, and the meshes
// the worker holds (at most so many, of at most so many bytes).
constexpr std::size_t kKeptShapes = 64;
constexpr std::size_t kKeptWorkerMeshes = 64;
constexpr std::size_t kKeptWorkerBytes = std::size_t(256) << 20;

std::size_t bytesOf(const render::MeshData& mesh) {
  return (mesh.positions.size() + mesh.normals.size()) * sizeof(float) + mesh.indices.size() * sizeof(std::uint32_t);
}

QString joined(const QJsonValue& list) {
  QStringList items;
  for (const QJsonValue& item : list.toArray()) {
    items << item.toString();
  }
  return items.isEmpty() ? QStringLiteral("-") : items.join(QStringLiteral(", "));
}

QJsonArray vector3(const gp_XYZ& v) { return QJsonArray{v.X(), v.Y(), v.Z()}; }

render::ViewTransform viewTransformOf(const QString& name) {
  if (name == QLatin1String("filmic")) {
    return render::ViewTransform::Filmic;
  }
  if (name == QLatin1String("neutral")) {
    return render::ViewTransform::Neutral;
  }
  return render::ViewTransform::Standard;
}

QColor srgbOf(const QJsonValue& value) {
  const QJsonArray rgb = value.toArray();
  if (rgb.size() != 3) {
    return Qt::white;
  }
  return QColor::fromRgbF(static_cast<float>(rgb.at(0).toDouble()), static_cast<float>(rgb.at(1).toDouble()),
                          static_cast<float>(rgb.at(2).toDouble()));
}

// The user's lights for the log (mitcad#54): ", 2 lights (1 off)".
QString lightsText(const QJsonArray& lights) {
  int off = 0;
  for (const QJsonValue& light : lights) {
    off += light.toObject().value(QStringLiteral("enabled")).toBool(true) ? 0 : 1;
  }
  return off == 0 ? QStringLiteral(", %1 lights").arg(lights.size())
                  : QStringLiteral(", %1 lights (%2 off)").arg(lights.size()).arg(off);
}

int renderSamples() {
  const int samples = qEnvironmentVariableIntValue("MITCAD_RENDER_SAMPLES");
  return samples > 0 ? samples : 64;
}

} // namespace

RenderMode::RenderMode(OcctViewer& viewer, std::function<void(QObject*, std::function<void()>)> whenIdle,
                       QObject* parent)
    : QObject(parent), m_viewer(viewer), m_whenIdle(std::move(whenIdle)), m_client(new render::RenderClient(this)),
      m_messageTimer(new QTimer(this)), m_viewTimer(new QTimer(this)) {
  connect(m_client, &render::RenderClient::ready, this, &RenderMode::workerReady);
  connect(m_client, &render::RenderClient::failed, this, &RenderMode::workerFailed);
  connect(m_client, &render::RenderClient::sceneApplied, this, &RenderMode::sceneApplied);
  connect(m_client, &render::RenderClient::frameAvailable, this, &RenderMode::showFrame, Qt::QueuedConnection);
  // The CPU took over from the render device (mitcad#50).
  connect(m_client, &render::RenderClient::deviceFallback, this, [this](const QString& text) {
    if (m_enabled) {
      showMessage(text + QLatin1Char('.'));
    }
  });
  // The worker's complaints (an environment image it cannot read).
  connect(m_client, &render::RenderClient::message, this, [this](const QString& text) {
    if (m_enabled) {
      showMessage(tr("Rendering: %1.").arg(text));
    }
  });
  connect(m_client, &render::RenderClient::finished, this, [this](std::uint64_t view, int samples, double seconds) {
    if (view == m_view) {
      qInfo().noquote() << QStringLiteral("Render view %1: %2 samples in %3 s").arg(view).arg(samples).arg(seconds, 0, 'f', 2);
    }
  });
  m_messageTimer->setSingleShot(true);
  m_messageTimer->setInterval(kMessageMilliseconds);
  connect(m_messageTimer, &QTimer::timeout, this, [this] {
    if (m_message) {
      m_message->hide();
    }
  });
  m_viewTimer->setSingleShot(true);
  connect(m_viewTimer, &QTimer::timeout, this, &RenderMode::sendView);
  connect(&m_viewer, &OcctViewer::viewChanged, this, &RenderMode::viewChanged);
  connect(&m_viewer, &OcctViewer::bodiesChanged, this, &RenderMode::scheduleScene);
}

RenderMode::~RenderMode() { m_client->stop(); }

void RenderMode::setEnabled(bool enabled) {
  if (enabled == m_enabled) {
    return;
  }
  m_enabled = enabled;
  m_ready = false;
  m_lastView.clear();
  // A new worker holds no meshes.
  m_workerMeshes.clear();
  m_shapeMeshes.clear();
  for (const auto& [sequence, file] : m_meshFiles) {
    QFile::remove(file);
  }
  m_meshFiles.clear();
  m_sentEnvironment.clear();
  m_missingTextures.clear();
  m_frame = render::Frame();
  m_viewTimer->stop();
  if (m_message) {
    m_message->hide();
  }
  if (enabled) {
    qInfo().noquote() << QStringLiteral("Rendered view on");
    m_client->setDevice(render::renderDeviceChoice());
    m_viewer.setRenderedMode(true);
    m_client->start();
  } else {
    qInfo().noquote() << QStringLiteral("Rendered view off");
    m_client->stop();
    m_viewer.setRenderedMode(false);
  }
}

void RenderMode::deviceChanged() {
  if (!m_enabled) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Render device changed to %1: the worker starts again")
                           .arg(render::renderDeviceChoice());
  setEnabled(false);
  setEnabled(true);
}

void RenderMode::workerReady(const QString& renderer) {
  if (!m_enabled) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Render worker ready: %1").arg(renderer);
  m_ready = true;
  m_workerMeshes.clear();
  // Full-resolution previews (docs/rendering.md, "Previews"): none unless
  // asked for, as each costs a denoise of the whole view.
  QJsonArray previews;
  for (const QString& at : qEnvironmentVariable("MITCAD_RENDER_PREVIEWS").split(QLatin1Char(','), Qt::SkipEmptyParts)) {
    previews.append(at.trimmed().toInt());
  }
  m_client->send({{QStringLiteral("cmd"), QStringLiteral("samples")},
                  {QStringLiteral("count"), renderSamples()},
                  {QStringLiteral("previews"), previews}});
  sendEnvironment();
  scheduleScene();
  sendView();
}

void RenderMode::setSettings(const QJsonObject& settings, const QString& documentFolder) {
  if (settings == m_settings && documentFolder == m_documentFolder) {
    return;
  }
  m_settings = settings;
  m_documentFolder = documentFolder;
  const QJsonObject film = settings.value(QStringLiteral("film")).toObject();
  m_look.exposure = static_cast<float>(film.value(QStringLiteral("exposure")).toDouble());
  m_look.transform = viewTransformOf(film.value(QStringLiteral("view_transform")).toString());
  if (!m_enabled || !m_ready) {
    return;
  }
  const QString sent = m_sentEnvironment;
  sendEnvironment();
  if (m_sentEnvironment != sent) {
    // A render of its own: frames and "done" of a new view.
    m_lastView.clear();
    sendView();
  } else {
    displayFrame();
  }
}

QJsonObject RenderMode::environmentCommand() const {
  return render::environmentCommand(m_settings, m_documentFolder);
}

void RenderMode::sendEnvironment() {
  if (!m_enabled || !m_ready) {
    return;
  }
  const QJsonObject command = environmentCommand();
  const QString key = QString::fromUtf8(QJsonDocument(command).toJson(QJsonDocument::Compact));
  if (key == m_sentEnvironment) {
    return;
  }
  m_sentEnvironment = key;
  m_client->send(command);
  // A complaint about the last environment (its image) no longer holds.
  if (m_message) {
    m_message->hide();
  }
  const QJsonObject environment = command.value(QStringLiteral("environment")).toObject();
  const QJsonObject ground = command.value(QStringLiteral("ground")).toObject();
  qInfo().noquote() << QStringLiteral("Render environment %1, background %2, ground %3")
                           .arg(environment.value(QStringLiteral("preset")).toString(),
                                command.value(QStringLiteral("background"))
                                    .toObject()
                                    .value(QStringLiteral("mode"))
                                    .toString(),
                                !ground.value(QStringLiteral("shadows")).toBool(true) ? QStringLiteral("none")
                                : ground.value(QStringLiteral("reflections")).toBool()
                                    ? QStringLiteral("shadows and reflections")
                                    : QStringLiteral("shadows"))
                    + lightsText(command.value(QStringLiteral("lights")).toArray());
}

void RenderMode::workerFailed(const QString& reason) {
  if (!m_enabled) {
    return;
  }
  m_enabled = false;
  m_ready = false;
  m_viewer.setRenderedMode(false);
  if (m_client->incompatible()) {
    showMessage(tr("The renderer cannot be used: %1.").arg(reason));
  } else {
    showMessage(tr("The renderer stopped: %1. View > Rendered starts it again.").arg(reason));
  }
  emit stopped();
}

void RenderMode::scheduleScene() {
  if (!m_enabled || !m_ready || m_scenePending) {
    return;
  }
  m_scenePending = true;
  // The bodies' triangulations are read only while no job computes them.
  m_whenIdle(this, [this] {
    m_scenePending = false;
    sendScene();
  });
}

void RenderMode::sendScene() {
  if (!m_enabled || !m_ready) {
    return;
  }
  // A fixed scene for trying the renderer without a design.
  if (qEnvironmentVariableIntValue("MITCAD_RENDER_TEST_SCENE") == 1) {
    m_client->send({{QStringLiteral("cmd"), QStringLiteral("scene")}, {QStringLiteral("test"), true}});
    return;
  }
  ScopedTiming timing("render scene");
  const std::uint64_t sequence = ++m_scene;
  render::SceneUpdate update;
  std::vector<render::MeshData> fresh; // the meshes the worker does not hold
  std::vector<const Appearance*> looks; // the appearances shown
  QStringList sentFor;
  int meshed = 0;
  const std::vector<BodyDisplay> shown = m_viewer.shownBodies();
  for (const BodyDisplay& body : shown) {
    const std::string id = body.occurrence.empty() ? body.uid : body.uid + "@" + body.occurrence;
    const TopoDS_Shape& shape = body.shape->occt();
    // A shape meshed before keeps its hash: an unchanged body, one shown
    // again, another placement or material are not meshed again.
    render::MeshData mesh;
    bool hasMesh = false;
    auto cached = m_shapeMeshes.find(shape);
    if (cached == m_shapeMeshes.end()) {
      mesh = render::meshOf(shape);
      hasMesh = true;
      ++meshed;
      ShapeMesh entry;
      entry.key = mesh.indices.empty() ? std::string() : render::meshKey(mesh);
      entry.bytes = bytesOf(mesh);
      cached = m_shapeMeshes.emplace(shape, entry).first;
    }
    cached->second.used = sequence;
    const std::string key = cached->second.key;
    if (key.empty()) {
      continue; // nothing to render
    }
    auto held = m_workerMeshes.find(key);
    if (held == m_workerMeshes.end()) {
      if (!hasMesh) { // the worker forgot it
        mesh = render::meshOf(shape);
        ++meshed;
      }
      held = m_workerMeshes.emplace(key, WorkerMesh{bytesOf(mesh), sequence}).first;
      mesh.name = key;
      fresh.push_back(std::move(mesh));
      sentFor << QString::fromStdString(id);
    }
    held->second.used = sequence;
    render::BodyData entry;
    entry.id = id;
    entry.mesh = key;
    const gp_Trsf& placement = body.placement.Transformation();
    for (int row = 0; row < 3; ++row) {
      for (int column = 0; column < 4; ++column) {
        entry.transform[static_cast<std::size_t>(row * 4 + column)] =
            static_cast<float>(placement.Value(row + 1, column + 1));
      }
    }
    entry.material = render::materialOf(body.appearance);
    // Faces with appearances of their own (mitcad#53).
    std::vector<render::SceneFaces> faces;
    QStringList faceLog;
    looks.push_back(&body.appearance);
    for (const BodyDisplay::FaceLook& look : body.faceLooks) {
      faces.push_back({look.faces, look.appearance});
      looks.push_back(&look.appearance);
      faceLog << QStringLiteral("%1 faces %2")
                     .arg(look.faces.size())
                     .arg(look.appearance.id.isEmpty() ? QStringLiteral("default") : look.appearance.id);
    }
    entry.faces = render::faceMaterialsOf(faces);
    qDebug().noquote() << QStringLiteral("Render body %1: appearance %2%3")
                              .arg(QString::fromStdString(id),
                                   body.appearance.id.isEmpty() ? QStringLiteral("default") : body.appearance.id,
                                   faceLog.isEmpty() ? QString() : QStringLiteral(", ") + faceLog.join(QStringLiteral(", ")));
    update.bodies.push_back(std::move(entry));
  }
  // Textures whose images are missing: the base colour, and a message.
  const QStringList missing = render::missingTextures(looks);
  const QString missingText = missing.join(QStringLiteral("; "));
  if (missingText != m_missingTextures) {
    m_missingTextures = missingText;
    if (!missing.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Render textures not drawn: %1").arg(missingText);
      showMessage(tr("Rendering: a texture is not drawn, its base colour is shown instead (%1).").arg(missingText));
    }
  }
  update.released = forgetMeshes();
  // The new meshes in a file of their own: the worker reads it when the
  // command comes, and says so ("scene" event), then it is removed.
  QString path;
  if (!fresh.empty()) {
    path = m_dir.filePath(QStringLiteral("meshes-%1.glb").arg(sequence));
    std::vector<const render::MeshData*> meshes;
    for (const render::MeshData& mesh : fresh) {
      meshes.push_back(&mesh);
    }
    std::string error;
    if (!m_dir.isValid() || !render::writeMeshFile(meshes, QDir::toNativeSeparators(path).toStdString(), error)) {
      qWarning().noquote() << QStringLiteral("Render scene not written: %1").arg(QString::fromStdString(error));
      // The worker does not get them: they are sent with the next scene.
      for (const render::MeshData& mesh : fresh) {
        m_workerMeshes.erase(mesh.name);
      }
      return;
    }
    m_meshFiles.emplace(sequence, path);
  }
  QJsonObject command = render::sceneCommand(update, path);
  command.insert(QStringLiteral("sequence"), static_cast<qint64>(sequence));
  timing.setDetail(QStringLiteral("%1 bodies, %2 meshes sent, %3 meshed")
                       .arg(update.bodies.size())
                       .arg(fresh.size())
                       .arg(meshed));
  qInfo().noquote() << QStringLiteral("Render scene %1: %2 bodies, %3 meshes sent (%4), %5 meshed, %6 released")
                           .arg(sequence)
                           .arg(update.bodies.size())
                           .arg(fresh.size())
                           .arg(sentFor.isEmpty() ? QStringLiteral("-") : sentFor.join(QStringLiteral(", ")))
                           .arg(meshed)
                           .arg(update.released.size());
  m_client->send(command);
}

std::vector<std::string> RenderMode::forgetMeshes() {
  // Shapes: only their hashes are kept, but they keep old shapes alive.
  std::vector<std::pair<std::uint64_t, TopoDS_Shape>> unusedShapes;
  for (const auto& [shape, entry] : m_shapeMeshes) {
    if (entry.used != m_scene) {
      unusedShapes.emplace_back(entry.used, shape);
    }
  }
  if (unusedShapes.size() > kKeptShapes) {
    std::sort(unusedShapes.begin(), unusedShapes.end(),
              [](const auto& a, const auto& b) { return a.first < b.first; });
    for (std::size_t i = 0; i + kKeptShapes < unusedShapes.size(); ++i) {
      m_shapeMeshes.erase(unusedShapes[i].second);
    }
  }
  std::vector<std::pair<std::uint64_t, std::string>> unused;
  std::size_t unusedBytes = 0;
  for (const auto& [key, entry] : m_workerMeshes) {
    if (entry.used != m_scene) {
      unused.emplace_back(entry.used, key);
      unusedBytes += entry.bytes;
    }
  }
  std::sort(unused.begin(), unused.end());
  std::vector<std::string> released;
  for (std::size_t i = 0; i < unused.size() && (unused.size() - i > kKeptWorkerMeshes || unusedBytes > kKeptWorkerBytes);
       ++i) {
    const auto held = m_workerMeshes.find(unused[i].second);
    unusedBytes -= held->second.bytes;
    released.push_back(held->first);
    m_workerMeshes.erase(held);
  }
  return released;
}

void RenderMode::sceneApplied(const QJsonObject& event) {
  const auto sequence = static_cast<std::uint64_t>(event.value(QStringLiteral("sequence")).toInteger());
  const auto file = m_meshFiles.find(sequence);
  if (file != m_meshFiles.end()) {
    QFile::remove(file->second);
    m_meshFiles.erase(file);
  }
  qInfo().noquote()
      << QStringLiteral("Render scene %1 applied in %2 ms: received %3; added %4; changed %5; moved %6; removed %7; "
                        "kept %8; meshes %9; instanced %10")
             .arg(sequence)
             .arg(event.value(QStringLiteral("milliseconds")).toDouble(), 0, 'f', 1)
             .arg(joined(event.value(QStringLiteral("received"))), joined(event.value(QStringLiteral("added"))),
                  joined(event.value(QStringLiteral("changed"))), joined(event.value(QStringLiteral("moved"))),
                  joined(event.value(QStringLiteral("removed"))))
             .arg(event.value(QStringLiteral("kept")).toInt())
             .arg(event.value(QStringLiteral("meshes")).toInt())
             .arg(event.value(QStringLiteral("instanced")).toInt());
}

void RenderMode::viewChanged() {
  if (!m_enabled || !m_ready || m_viewTimer->isActive()) {
    return;
  }
  if (m_sinceSent.isValid() && m_sinceSent.elapsed() < kViewInterval) {
    m_viewTimer->start(kViewInterval - static_cast<int>(m_sinceSent.elapsed()));
    return;
  }
  sendView();
}

void RenderMode::sendView() {
  if (!m_enabled || !m_ready) {
    return;
  }
  const occ::handle<Graphic3d_Camera>& camera = m_viewer.viewCamera();
  const qreal ratio = m_viewer.devicePixelRatioF();
  const int width = qRound(m_viewer.width() * ratio);
  const int height = qRound(m_viewer.height() * ratio);
  const bool perspective = !camera->IsOrthographic();
  const double halfHeight = camera->ViewDimensions().Y() / 2.0;
  const QJsonArray eye = vector3(camera->Eye().XYZ());
  const QJsonArray target = vector3(camera->Center().XYZ());
  const QJsonArray up = vector3(camera->Up().XYZ());
  const QJsonArray size{width, height};
  QJsonObject view{{QStringLiteral("cmd"), QStringLiteral("view")},
                   {QStringLiteral("projection"), perspective ? QStringLiteral("perspective") : QStringLiteral("orthographic")},
                   {QStringLiteral("eye"), eye},
                   {QStringLiteral("target"), target},
                   {QStringLiteral("up"), up},
                   {QStringLiteral("half_height"), halfHeight},
                   {QStringLiteral("size"), size}};
  const QString key = QString::fromUtf8(QJsonDocument(view).toJson(QJsonDocument::Compact));
  if (key == m_lastView) {
    return;
  }
  m_lastView = key;
  view.insert(QStringLiteral("sequence"), static_cast<qint64>(++m_view));
  m_client->reserve(width, height);
  m_client->send(view);
  // The last frame shows another camera: the bodies are drawn until the
  // first frame of this one comes.
  m_viewer.setRenderedImage(QImage());
  m_sinceView.start();
  m_sinceSent.start();
  m_firstFrameLogged = false;
  m_firstDenoisedLogged = false;
  qDebug().noquote() << QStringLiteral("Render view %1: %2 %3 x %4").arg(m_view).arg(view.value(QStringLiteral("projection")).toString()).arg(width).arg(height);
}

void RenderMode::showFrame() {
  render::Frame frame;
  if (!m_enabled || !m_client->takeFrame(frame)) {
    return;
  }
  // A frame of an older camera or size would not fit the view.
  if (frame.info.view != m_view) {
    return;
  }
  const auto width = static_cast<int>(frame.info.width);
  const auto height = static_cast<int>(frame.info.height);
  if (frame.info.denoised != 0 && !m_firstDenoisedLogged) {
    m_firstDenoisedLogged = true;
    qInfo().noquote() << QStringLiteral("Render view %1: first denoised frame (%2 x %3, %4 samples) after %5 ms")
                             .arg(m_view)
                             .arg(width)
                             .arg(height)
                             .arg(frame.info.samples)
                             .arg(m_sinceView.elapsed());
  }
  if (!m_firstFrameLogged) {
    m_firstFrameLogged = true;
    qInfo().noquote() << QStringLiteral("Render view %1: first frame (%2 x %3) after %4 ms")
                             .arg(m_view)
                             .arg(width)
                             .arg(height)
                             .arg(m_sinceView.elapsed());
  }
  m_frame = std::move(frame);
  displayFrame();
  // What the frame covers, in the view's logical pixels (UI tests compare
  // it with the shaded view).
  const QRect covered = render::coverage(m_frame.pixels.data(), width, height);
  const double scaleX = static_cast<double>(m_viewer.width()) / width;
  const double scaleY = static_cast<double>(m_viewer.height()) / height;
  qDebug().noquote() << QStringLiteral("Render frame view %1 %2x%3 samples %4%5 covers %6,%7 %8,%9")
                            .arg(m_frame.info.view)
                            .arg(width)
                            .arg(height)
                            .arg(m_frame.info.samples)
                            .arg(m_frame.info.denoised != 0 ? QStringLiteral(" denoised") : QString())
                            .arg(qRound(covered.left() * scaleX))
                            .arg(qRound(covered.top() * scaleY))
                            .arg(qRound((covered.right() + 1) * scaleX))
                            .arg(qRound((covered.bottom() + 1) * scaleY));
}

void RenderMode::displayFrame() {
  if (!m_enabled || m_frame.pixels.empty() || m_frame.info.view != m_view) {
    return;
  }
  // The view's own background, or the settings' colour; a frame of the
  // environment covers it all.
  QColor top;
  QColor bottom;
  const QJsonObject background = m_settings.value(QStringLiteral("background")).toObject();
  if (background.value(QStringLiteral("mode")).toString() == QLatin1String("color")) {
    top = bottom = srgbOf(background.value(QStringLiteral("color")));
  } else {
    m_viewer.backgroundColors(top, bottom);
  }
  m_viewer.setRenderedImage(render::displayImage(m_frame.pixels.data(), static_cast<int>(m_frame.info.width),
                                                 static_cast<int>(m_frame.info.height), top, bottom, m_look,
                                                 m_frame.catcher.empty() ? nullptr : m_frame.catcher.data()));
}

void RenderMode::showMessage(const QString& text) {
  qWarning().noquote() << QStringLiteral("View message: %1").arg(text);
  if (!m_message) {
    m_message = new QLabel(&m_viewer);
    m_message->setObjectName(QStringLiteral("renderMessage"));
    m_message->setWordWrap(true);
    m_message->setStyleSheet(QStringLiteral(
        "QLabel { background: rgba(40, 40, 40, 210); color: white; border-radius: 6px; padding: 8px 12px; }"));
  }
  m_message->setText(text);
  const int width = std::min(520, m_viewer.width() - 40);
  m_message->setFixedWidth(width);
  m_message->adjustSize();
  m_message->move((m_viewer.width() - width) / 2, 16);
  m_message->show();
  m_message->raise();
  m_messageTimer->start();
}

} // namespace mitcad
