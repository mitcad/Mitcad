// SPDX-License-Identifier: MIT
#include "render/RenderBatch.hpp"

#include <algorithm>
#include <cmath>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QProcess>
#include <QSaveFile>
#include <QTimer>
#include <QtLogging>

#include "render/RenderDevice.hpp"
#include "render/RenderMesh.hpp"
#include "render/RenderProtocol.hpp"
#include "render/RenderScene.hpp"

namespace mitcad::render {

namespace {

float linear(double srgb) {
  return static_cast<float>(srgb <= 0.04045 ? srgb / 12.92 : std::pow((srgb + 0.055) / 1.055, 2.4));
}

std::array<float, 3> linear(const QColor& color) {
  return {linear(color.redF()), linear(color.greenF()), linear(color.blueF())};
}

std::array<double, 3> vectorOf(const QJsonValue& value) {
  const QJsonArray array = value.toArray();
  return {array.at(0).toDouble(), array.at(1).toDouble(), array.at(2).toDouble()};
}

QJsonArray json(const std::array<double, 3>& v) { return QJsonArray{v[0], v[1], v[2]}; }

// How long a worker that was asked to stop may take before it is ended.
constexpr int kStopMilliseconds = 2000;

} // namespace

MaterialData materialOf(const Appearance& a) {
  MaterialData m;
  if (a.id.isEmpty()) {
    return m;
  }
  m.baseColor = linear(a.baseColor);
  m.metallic = static_cast<float>(a.metalness);
  m.roughness = static_cast<float>(a.roughness);
  m.specular = static_cast<float>(a.specular);
  m.transmission = static_cast<float>(a.transmission);
  m.ior = static_cast<float>(a.ior);
  m.coat = static_cast<float>(a.coat);
  m.coatRoughness = static_cast<float>(a.coatRoughness);
  m.emissionColor = linear(a.emissionColor);
  m.emission = static_cast<float>(a.emission);
  m.opacity = static_cast<float>(a.opacity);
  if (a.hasTexture && !a.texture.file.isEmpty()) {
    m.texture = QDir::toNativeSeparators(a.texture.file).toStdString();
    m.textureSize = {static_cast<float>(a.texture.width), static_cast<float>(a.texture.height)};
    m.textureRotation = static_cast<float>(a.texture.rotation);
    m.texturePlanar = a.texture.planar;
  }
  return m;
}

std::vector<FaceMaterial> faceMaterialsOf(const std::vector<SceneFaces>& faces) {
  std::vector<FaceMaterial> materials;
  for (const SceneFaces& group : faces) {
    FaceMaterial material;
    for (const int face : group.faces) {
      if (face >= 0) {
        material.faces.push_back(static_cast<std::uint32_t>(face));
      }
    }
    material.material = materialOf(group.appearance);
    if (!material.faces.empty()) {
      materials.push_back(std::move(material));
    }
  }
  return materials;
}

QStringList missingTextures(const std::vector<const Appearance*>& appearances) {
  QStringList missing;
  for (const Appearance* a : appearances) {
    if (a != nullptr && a->hasTexture && a->texture.file.isEmpty()) {
      const QString line = QStringLiteral("%1: %2").arg(a->name, a->texture.missing);
      if (!missing.contains(line)) {
        missing << line;
      }
    }
  }
  return missing;
}

SceneUpdate sceneOf(const std::vector<SceneBody>& bodies) {
  SceneUpdate scene;
  for (const SceneBody& body : bodies) {
    MeshData mesh = meshOf(body.shape);
    if (mesh.indices.empty()) {
      continue;
    }
    BodyData entry;
    entry.id = body.name;
    entry.mesh = meshKey(mesh);
    for (int row = 0; row < 3; ++row) {
      for (int column = 0; column < 4; ++column) {
        entry.transform[static_cast<std::size_t>(row * 4 + column)] =
            static_cast<float>(body.placement.Value(row + 1, column + 1));
      }
    }
    entry.material = materialOf(body.appearance);
    entry.faces = faceMaterialsOf(body.faces);
    // Bodies with the same triangles share their mesh.
    const bool held = std::any_of(scene.meshes.begin(), scene.meshes.end(),
                                  [&entry](const auto& known) { return known.first == entry.mesh; });
    if (!held) {
      mesh.name = entry.mesh;
      scene.meshes.emplace_back(entry.mesh, std::move(mesh));
    }
    scene.bodies.push_back(std::move(entry));
  }
  return scene;
}

QJsonObject environmentCommand(const QJsonObject& settings, const QString& documentFolder, bool transparent) {
  QJsonObject environment = settings.value(QStringLiteral("environment")).toObject();
  // An image relative to the document's folder.
  const QString image = environment.value(QStringLiteral("image")).toString();
  if (!image.isEmpty() && QFileInfo(image).isRelative() && !documentFolder.isEmpty()) {
    environment.insert(QStringLiteral("image"), QDir::cleanPath(QDir(documentFolder).filePath(image)));
  }
  // The worker only needs to know whether the camera sees the environment:
  // the view's background and a colour are both composited by the display.
  const bool seen = !transparent && settings.value(QStringLiteral("background"))
                                            .toObject()
                                            .value(QStringLiteral("mode"))
                                            .toString() == QLatin1String("environment");
  return {{QStringLiteral("cmd"), QStringLiteral("environment")},
          {QStringLiteral("environment"), environment},
          {QStringLiteral("background"),
           QJsonObject{{QStringLiteral("mode"), seen ? QStringLiteral("environment") : QStringLiteral("view")}}},
          {QStringLiteral("ground"), settings.value(QStringLiteral("ground")).toObject()},
          {QStringLiteral("lights"), settings.value(QStringLiteral("lights")).toArray()}};
}

Camera namedViewCamera(const QJsonObject& view, double aspect) {
  Camera camera;
  camera.eye = vectorOf(view.value(QStringLiteral("eye")));
  camera.target = vectorOf(view.value(QStringLiteral("target")));
  camera.up = vectorOf(view.value(QStringLiteral("up")));
  camera.perspective = view.value(QStringLiteral("perspective")).toBool();
  const double height = view.value(QStringLiteral("height")).toDouble(250.0);
  // The 3D view's scale is its shorter side (Graphic3d_Camera::SetScale).
  camera.aspect = aspect > 0.0 ? aspect : 1.0;
  camera.halfHeight = height / 2.0 * (camera.aspect < 1.0 ? 1.0 / camera.aspect : 1.0);
  return camera;
}

QSize outputSize(const QJsonObject& output, double viewAspect) {
  const int width = std::max(1, output.value(QStringLiteral("width")).toInt(1920));
  if (output.value(QStringLiteral("aspect")).toString() == QLatin1String("fixed") || !(viewAspect > 0.0)) {
    return {width, std::max(1, output.value(QStringLiteral("height")).toInt(1080))};
  }
  return {width, std::max(1, static_cast<int>(std::lround(width / viewAspect)))};
}

QRectF imageFrame(const QSizeF& view, const QSizeF& image) {
  if (view.isEmpty() || image.isEmpty()) {
    return QRectF(QPointF(0, 0), view);
  }
  const double viewAspect = view.width() / view.height();
  const double imageAspect = image.width() / image.height();
  QSizeF frame = view;
  if (imageAspect < viewAspect) {
    frame.setWidth(view.height() * imageAspect); // narrower: the full height
  } else {
    frame.setHeight(view.width() / imageAspect); // wider: the full width
  }
  return QRectF(QPointF((view.width() - frame.width()) / 2.0, (view.height() - frame.height()) / 2.0), frame);
}

QJsonObject finalView(const Camera& camera, const QSize& size) {
  // The frame's share of the camera's height (1 unless the image is wider
  // than the camera's view).
  const QRectF frame = imageFrame(QSizeF(camera.aspect, 1.0), QSizeF(size));
  return {{QStringLiteral("projection"), camera.perspective ? QStringLiteral("perspective") : QStringLiteral("orthographic")},
          {QStringLiteral("eye"), json(camera.eye)},
          {QStringLiteral("target"), json(camera.target)},
          {QStringLiteral("up"), json(camera.up)},
          {QStringLiteral("half_height"), camera.halfHeight * frame.height()},
          {QStringLiteral("size"), QJsonArray{size.width(), size.height()}}};
}

QString outputExtension(const QString& format) {
  if (format == QLatin1String("jpeg")) {
    return QStringLiteral("jpg");
  }
  if (format == QLatin1String("exr")) {
    return QStringLiteral("exr");
  }
  return QStringLiteral("png");
}

QJsonObject finalJob(const QJsonObject& settings, const QString& documentFolder, const QString& scenePath,
                     const QJsonObject& view, const QString& imagePath, const QColor& top, const QColor& bottom,
                     const QString& previewPath) {
  QJsonObject output = settings.value(QStringLiteral("output")).toObject();
  // JPEG has no alpha channel: the background is in the image.
  const bool transparent = output.value(QStringLiteral("transparent")).toBool() &&
                           output.value(QStringLiteral("format")).toString() != QLatin1String("jpeg");
  output.insert(QStringLiteral("transparent"), transparent);
  output.insert(QStringLiteral("path"), imagePath);
  QJsonObject environment = environmentCommand(settings, documentFolder, transparent);
  environment.remove(QStringLiteral("cmd"));
  const auto rgb = [](const QColor& c) { return QJsonArray{c.redF(), c.greenF(), c.blueF()}; };
  QJsonObject job{{QStringLiteral("scene"), QJsonObject{{QStringLiteral("path"), scenePath}}},
                  {QStringLiteral("environment"), environment},
                  {QStringLiteral("view"), view},
                  {QStringLiteral("output"), output},
                  {QStringLiteral("film"), settings.value(QStringLiteral("film")).toObject()},
                  {QStringLiteral("background"),
                   QJsonObject{{QStringLiteral("top"), rgb(top)}, {QStringLiteral("bottom"), rgb(bottom)}}}};
  if (!previewPath.isEmpty()) {
    job.insert(QStringLiteral("preview"), QJsonObject{{QStringLiteral("path"), previewPath},
                                                      {QStringLiteral("size"), QJsonArray{480, 360}}});
  }
  return job;
}

QString workerExecutable() {
  QString path = qEnvironmentVariable("MITCAD_RENDER_WORKER");
  if (path.isEmpty()) {
    QString name = QString::fromLatin1(kWorkerName);
#ifdef _WIN32
    name += QStringLiteral(".exe");
#endif
    QString folder = QCoreApplication::applicationDirPath();
#ifdef __linux__
    // Qt names the AppImage file as the application for an AppImage; the
    // worker is next to the executable inside it.
    const QString self = QFileInfo(QStringLiteral("/proc/self/exe")).symLinkTarget();
    if (!self.isEmpty()) {
      folder = QFileInfo(self).absolutePath();
    }
#endif
    path = QDir(folder).filePath(name);
  }
  const QFileInfo file(path);
  return file.isFile() && file.isExecutable() ? file.absoluteFilePath() : QString();
}

// ---------------------------------------------------------------------------
// FinalRender

FinalRender::FinalRender(QObject* parent) : QObject(parent), m_killTimer(new QTimer(this)) {
  m_killTimer->setSingleShot(true);
  connect(m_killTimer, &QTimer::timeout, this, [this] {
    if (m_process != nullptr) {
      m_process->kill();
    }
  });
}

FinalRender::~FinalRender() {
  if (m_process != nullptr) {
    m_cancelling = true;
    m_process->disconnect(this);
    m_process->closeWriteChannel(); // the worker stops at the end of stdin
    if (!m_process->waitForFinished(kStopMilliseconds)) {
      m_process->kill();
      m_process->waitForFinished(1000);
    }
  }
}

bool FinalRender::start(const QString& worker, const QJsonObject& job, const QString& jobPath) {
  m_pending.clear();
  m_lastError.clear();
  m_greeted = false;
  m_done = false;
  m_cancelling = false;
  QString problem;
  if (worker.isEmpty()) {
    problem = tr("there is no %1 next to Mitcad").arg(QString::fromLatin1(kWorkerName));
  } else {
    QSaveFile file(jobPath);
    if (!file.open(QIODevice::WriteOnly) || file.write(QJsonDocument(job).toJson()) < 0 || !file.commit()) {
      problem = tr("the render job cannot be written to %1: %2").arg(jobPath, file.errorString());
    }
  }
  if (!problem.isEmpty()) {
    QMetaObject::invokeMethod(this, [this, problem] { emit failed(problem); }, Qt::QueuedConnection);
    return false;
  }
  if (m_process != nullptr) {
    // The last render's worker, done and ending by itself.
    QProcess* last = m_process;
    last->disconnect(this);
    last->closeWriteChannel();
    connect(last, &QProcess::finished, last, &QObject::deleteLater);
    if (last->state() == QProcess::NotRunning) {
      last->deleteLater();
    }
  }
  m_process = new QProcess(this);
  m_process->setProcessChannelMode(QProcess::SeparateChannels);
  QProcess* process = m_process;
  connect(m_process, &QProcess::readyReadStandardOutput, this, &FinalRender::readOutput);
  connect(m_process, &QProcess::readyReadStandardError, this, [this, process] {
    for (const QByteArray& line : process->readAllStandardError().split('\n')) {
      if (!line.trimmed().isEmpty()) {
        qDebug().noquote() << QStringLiteral("Render worker: %1").arg(QString::fromUtf8(line.trimmed()));
      }
    }
  });
  connect(m_process, &QProcess::finished, this, [this, process](int exitCode, QProcess::ExitStatus status) {
    if (process == m_process) {
      ended(exitCode, status == QProcess::CrashExit);
    }
  });
  connect(m_process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError error) {
    if (process != m_process || error != QProcess::FailedToStart) {
      return;
    }
    m_lastError = tr("the render process could not start: %1").arg(process->errorString());
    ended(-1, false);
  });
  m_process->start(worker, {QStringLiteral("--batch"), jobPath, QStringLiteral("--control")});
  return true;
}

void FinalRender::cancel() {
  if (!isRunning() || m_cancelling) {
    return;
  }
  m_cancelling = true;
  m_process->write("{\"cmd\":\"stop\"}\n");
  m_process->closeWriteChannel();
  m_killTimer->start(kStopMilliseconds);
}

bool FinalRender::isRunning() const {
  return m_process != nullptr && m_process->state() != QProcess::NotRunning && !m_done;
}

qint64 FinalRender::processId() const { return m_process != nullptr ? m_process->processId() : 0; }

void FinalRender::readOutput() {
  if (m_process == nullptr) {
    return;
  }
  m_pending += m_process->readAllStandardOutput();
  qsizetype end = 0;
  while ((end = m_pending.indexOf('\n')) >= 0) {
    const QByteArray line = m_pending.left(end).trimmed();
    m_pending.remove(0, end + 1);
    const QJsonDocument document = QJsonDocument::fromJson(line);
    if (document.isObject()) {
      handle(document.object());
    } else if (!line.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Render worker: %1").arg(QString::fromUtf8(line));
    }
    if (m_process == nullptr) {
      return; // stopped while handling it
    }
  }
}

void FinalRender::handle(const QJsonObject& event) {
  const QString name = event.value(QStringLiteral("event")).toString();
  if (!m_greeted) {
    // The first event says which protocol the worker speaks.
    const int version = event.value(QStringLiteral("protocol")).toInt(0);
    if (name == QLatin1String("hello") && version == static_cast<int>(kProtocolVersion)) {
      m_greeted = true;
      return;
    }
    if (name != QLatin1String("error")) {
      m_lastError = tr("%1 does not match this Mitcad (its protocol version is %2, Mitcad's is %3); "
                       "install them together")
                        .arg(QString::fromLatin1(kWorkerName))
                        .arg(version)
                        .arg(kProtocolVersion);
      m_process->kill();
      return;
    }
  }
  if (name == QLatin1String("ready")) {
    m_deviceLabel = RenderDevice::fromJson(event.value(QStringLiteral("device")).toObject()).label();
    emit ready(event.value(QStringLiteral("renderer")).toString());
  } else if (name == QLatin1String("device")) {
    // The CPU took over (mitcad#50); a "warning" with the reason follows.
    m_deviceLabel = RenderDevice::fromJson(event.value(QStringLiteral("device")).toObject()).label();
  } else if (name == QLatin1String("progress")) {
    const QJsonValue remaining = event.value(QStringLiteral("remaining"));
    emit progress(event.value(QStringLiteral("samples")).toInt(), event.value(QStringLiteral("total")).toInt(),
                  event.value(QStringLiteral("seconds")).toDouble(),
                  remaining.isDouble() ? remaining.toDouble() : -1.0);
  } else if (name == QLatin1String("preview")) {
    emit preview(event.value(QStringLiteral("path")).toString(), event.value(QStringLiteral("samples")).toInt());
  } else if (name == QLatin1String("warning")) {
    emit warning(event.value(QStringLiteral("message")).toString());
  } else if (name == QLatin1String("done")) {
    m_done = true;
    const QJsonArray size = event.value(QStringLiteral("size")).toArray();
    emit finished(event.value(QStringLiteral("path")).toString(), QSize(size.at(0).toInt(), size.at(1).toInt()),
                  event.value(QStringLiteral("samples")).toInt(), event.value(QStringLiteral("seconds")).toDouble());
  } else if (name == QLatin1String("error")) {
    m_lastError = event.value(QStringLiteral("message")).toString();
  } else if (name == QLatin1String("status")) {
    qDebug().noquote() << QStringLiteral("Render worker: %1").arg(event.value(QStringLiteral("text")).toString());
  }
}

void FinalRender::ended(int exitCode, bool crashed) {
  readOutput();
  m_killTimer->stop();
  if (m_process != nullptr) {
    m_process->deleteLater();
    m_process = nullptr;
  }
  if (m_done) {
    return;
  }
  if (m_cancelling) {
    emit cancelled();
    return;
  }
  QString reason = m_lastError;
  if (reason.isEmpty()) {
    reason = crashed ? tr("the render process crashed") : tr("the render process ended (exit code %1)").arg(exitCode);
  }
  emit failed(reason);
}

} // namespace mitcad::render
