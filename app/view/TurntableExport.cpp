// SPDX-License-Identifier: MIT
// File › Export Turntable (mitcad#2523; app/COMMANDS.md "Export
// Turntable"): the camera turned about an axis through the model, a frame
// per step, as PNG frames and an MP4 or WebM video made of them by the
// FFmpeg program. The frames come from the view rendered offscreen
// through the turned camera (OcctViewer::snapshotFrom, the view's visual
// style), or from the render worker (Rendered: one final render a frame,
// the scene written once). The progress dialog keeps the design as it is
// until the frames are done; Cancel stops at the next frame or ends the
// encoding.
#include <array>
#include <cmath>
#include <memory>
#include <optional>
#include <string>
#include <utility>

#include <QColor>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QProcess>
#include <QProgressDialog>
#include <QTemporaryDir>
#include <QTimer>
#include <QtLogging>

#include <Bnd_Box.hxx>
#include <gp.hxx>

#include "../browser/DocumentHost.hpp"
#include "../framework/DialogFolders.hpp"
#include "render/RenderBatch.hpp"
#include "render/RenderClient.hpp"
#include "render/RenderDevice.hpp"
#include "render/ShownScene.hpp"
#include "view/Turntable.hpp"
#include "view/TurntableDialog.hpp"
#include "ViewController.hpp"

namespace mitcad {
namespace {

QString vectorText(const gp_XYZ& v, int decimals) {
  return QStringLiteral("(%1, %2, %3)")
      .arg(QString::number(v.X(), 'f', decimals), QString::number(v.Y(), 'f', decimals),
           QString::number(v.Z(), 'f', decimals));
}

// A turntable's frames and video, after the dialog, step by step on the
// event loop; deletes itself when it ends.
class TurntableJob : public QObject {
public:
  TurntableJob(OcctViewer& viewer, DocumentHost& host, QWidget* window, TurntableChoice choice, const gp_Ax1& axis,
               occ::handle<Graphic3d_Camera> start)
      : QObject(window), m_viewer(viewer), m_host(host), m_window(window), m_choice(std::move(choice)), m_axis(axis),
        m_start(std::move(start)), m_step(turntableStep(m_choice.angle, m_choice.frames)) {}

  void run() {
    m_folder = m_choice.framesFolder;
    if (m_folder.isEmpty()) {
      m_temporary = std::make_unique<QTemporaryDir>();
      if (!m_temporary->isValid()) {
        fail(QObject::tr("Could not export the turntable: no temporary folder (%1).").arg(m_temporary->errorString()),
             QStringLiteral("no temporary folder: %1").arg(m_temporary->errorString()));
        return;
      }
      m_folder = m_temporary->path();
    } else if (!QDir().mkpath(m_folder)) {
      fail(QObject::tr("Could not export the turntable: the folder %1 cannot be made.").arg(QDir::toNativeSeparators(m_folder)),
           QStringLiteral("the folder %1 cannot be made").arg(m_folder));
      return;
    }
    // Frames of an earlier export after the last one of this one (the
    // dialog asked before replacing them).
    if (!m_choice.framesFolder.isEmpty()) {
      const QDir folder(m_folder);
      for (const QString& name : folder.entryList({QStringLiteral("frame-*.png")}, QDir::Files)) {
        bool number = false;
        const int index = name.mid(6, name.size() - 10).toInt(&number);
        if (number && index > m_choice.frames && name == turntableFrameName(index)) {
          QFile::remove(folder.filePath(name));
        }
      }
    }
    const bool video = turntableHasVideo(m_choice.output);
    m_progress = new QProgressDialog(QObject::tr("Making the frames…"), QObject::tr("Cancel"), 0, m_choice.frames + (video ? 1 : 0),
                                     m_window);
    m_progress->setObjectName(QStringLiteral("turntableProgress"));
    m_progress->setWindowTitle(QObject::tr("Export Turntable"));
    m_progress->setWindowModality(Qt::WindowModal);
    m_progress->setAutoClose(false);
    m_progress->setAutoReset(false);
    m_progress->setMinimumDuration(0);
    m_progress->setValue(0);
    connect(m_progress, &QProgressDialog::canceled, this, [this] { cancel(); });
    qInfo().noquote() << QStringLiteral("Turntable started: %1 frames of %2 deg about %3 through %4 along %5, step %6 "
                                        "deg, %7 x %8, %9")
                             .arg(m_choice.frames)
                             .arg(m_choice.angle)
                             .arg(m_choice.axis, vectorText(m_axis.Location().XYZ(), 3),
                                  vectorText(m_axis.Direction().XYZ(), 4), QString::number(m_step, 'f', 4))
                             .arg(m_choice.size.width())
                             .arg(m_choice.size.height())
                             .arg(m_choice.rendered ? QStringLiteral("rendered") : QStringLiteral("view style"));
    if (m_choice.rendered && !prepareRender()) {
      return;
    }
    next();
  }

private:
  QString framePath(int index) const { return QDir(m_folder).filePath(turntableFrameName(index)); }

  // The next frame, once the model is idle.
  void next() {
    if (m_done) {
      return;
    }
    if (m_frame >= m_choice.frames) {
      framesDone();
      return;
    }
    QTimer::singleShot(0, this, [this] {
      m_host.whenIdle(this, [this] {
        if (m_choice.rendered) {
          renderFrame();
        } else {
          viewFrame();
        }
      });
    });
  }

  occ::handle<Graphic3d_Camera> cameraOf(int frame) const {
    return turntableCamera(m_start, m_axis, frame * m_step);
  }

  // The frame's camera direction logged with its turn from the frame
  // before, about the axis.
  void logFrame(const gp_Dir& direction) {
    const std::optional<double> step =
        m_previous ? turnAbout(*m_previous, direction, m_axis.Direction()) : std::optional<double>();
    qDebug().noquote() << QStringLiteral("Turntable frame %1 of %2: camera direction %3, step %4")
                              .arg(m_frame + 1)
                              .arg(m_choice.frames)
                              .arg(vectorText(direction.XYZ(), 4),
                                   step ? QStringLiteral("%1 deg").arg(QString::number(*step, 'f', 2))
                                        : QStringLiteral("-"));
    m_previous = direction;
  }

  void frameWritten() {
    ++m_frame;
    if (m_progress) {
      m_progress->setValue(m_frame);
    }
    next();
  }

  void viewFrame() {
    if (m_done) {
      return;
    }
    gp_Dir direction;
    const QImage image = m_viewer.snapshotFrom(cameraOf(m_frame), m_choice.size, &direction);
    const QString path = framePath(m_frame + 1);
    if (image.isNull() || !image.save(path, "PNG")) {
      fail(QObject::tr("Could not write the frame %1.").arg(QDir::toNativeSeparators(path)),
           QStringLiteral("frame %1 not written: %2").arg(path,
                                                         image.isNull() ? QStringLiteral("the view rendered no picture")
                                                                        : QStringLiteral("the file cannot be written")));
      return;
    }
    logFrame(direction);
    frameWritten();
  }

  // The scene and the job's settings for the rendered frames.
  bool prepareRender() {
    m_renderFolder = std::make_unique<QTemporaryDir>();
    if (!m_renderFolder->isValid()) {
      fail(QObject::tr("Could not export the turntable: no temporary folder (%1).").arg(m_renderFolder->errorString()),
           QStringLiteral("no temporary folder: %1").arg(m_renderFolder->errorString()));
      return false;
    }
    m_settings = m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("render_settings")}});
    // PNG frames of the turntable's size, opaque (a video has no alpha).
    QJsonObject output = m_settings.value(QStringLiteral("output")).toObject();
    output.insert(QStringLiteral("format"), QStringLiteral("png"));
    output.insert(QStringLiteral("transparent"), false);
    output.insert(QStringLiteral("width"), m_choice.size.width());
    output.insert(QStringLiteral("height"), m_choice.size.height());
    m_settings.insert(QStringLiteral("output"), output);
    const QJsonObject background = m_settings.value(QStringLiteral("background")).toObject();
    if (background.value(QStringLiteral("mode")).toString() == QLatin1String("color")) {
      const QJsonArray rgb = background.value(QStringLiteral("color")).toArray();
      m_top = m_bottom = QColor::fromRgbF(static_cast<float>(rgb.at(0).toDouble()),
                                          static_cast<float>(rgb.at(1).toDouble()),
                                          static_cast<float>(rgb.at(2).toDouble()));
    } else {
      m_viewer.backgroundColors(m_top, m_bottom);
    }
    m_scenePath = m_renderFolder->filePath(QStringLiteral("scene.glb"));
    render::ShownScene written;
    std::string error;
    if (!render::writeShownScene(m_viewer, m_renderFolder->path(), m_scenePath, written, error)) {
      fail(QObject::tr("Could not export the turntable: the scene cannot be written (%1).").arg(QString::fromStdString(error)),
           QStringLiteral("the scene cannot be written: %1").arg(QString::fromStdString(error)));
      return false;
    }
    const render::WorkerChoice worker = render::RenderClient::workerChoice();
    m_worker = worker.path;
    if (m_worker.isEmpty()) {
      fail(QObject::tr("Could not export the turntable: there is no render worker."), QStringLiteral("no render worker"));
      return false;
    }
    m_render = new render::FinalRender(this);
    connect(m_render, &render::FinalRender::finished, this, [this](const QString&, const QSize&, int, double) {
      logFrame(m_renderDirection);
      frameWritten();
    });
    connect(m_render, &render::FinalRender::failed, this, [this](const QString& reason) {
      fail(QObject::tr("Could not render the frame %1: %2.").arg(m_frame + 1).arg(reason),
           QStringLiteral("frame %1 not rendered: %2").arg(m_frame + 1).arg(reason));
    });
    qInfo().noquote() << QStringLiteral("Turntable render scene: %1 bodies, worker %2").arg(written.bodies).arg(m_worker);
    return true;
  }

  void renderFrame() {
    if (m_done) {
      return;
    }
    const occ::handle<Graphic3d_Camera> turned = cameraOf(m_frame);
    render::Camera camera;
    const auto xyz = [](const gp_XYZ& v) { return std::array<double, 3>{v.X(), v.Y(), v.Z()}; };
    camera.eye = xyz(turned->Eye().XYZ());
    camera.target = xyz(turned->Center().XYZ());
    camera.up = xyz(turned->Up().XYZ());
    camera.perspective = !turned->IsOrthographic();
    camera.halfHeight = turned->ViewDimensions().Y() / 2.0;
    // The frame's aspect: its height shows the view's height, as the view
    // style's frames do.
    camera.aspect = static_cast<double>(m_choice.size.width()) / m_choice.size.height();
    m_renderDirection = turned->Direction();
    QJsonObject job = render::finalJob(m_settings, m_host.documentFolder(), m_scenePath,
                                       render::finalView(camera, m_choice.size), framePath(m_frame + 1), m_top,
                                       m_bottom, QString());
    job.insert(QStringLiteral("device"), render::renderDeviceChoice());
    if (!m_render->start(m_worker, job, m_renderFolder->filePath(QStringLiteral("job.json")))) {
      return; // failed() follows
    }
    if (m_progress) {
      m_progress->setLabelText(QObject::tr("Rendering frame %1 of %2…").arg(m_frame + 1).arg(m_choice.frames));
    }
  }

  void framesDone() {
    if (!m_choice.framesFolder.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Turntable frames saved %1: %2 frames, %3 x %4")
                               .arg(m_folder)
                               .arg(m_choice.frames)
                               .arg(m_choice.size.width())
                               .arg(m_choice.size.height());
    }
    if (!turntableHasVideo(m_choice.output)) {
      m_host.showStatus(QObject::tr("Turntable frames saved in %1.").arg(QDir::toNativeSeparators(m_folder)),
                        NoticeLevel::Information);
      qInfo().noquote() << QStringLiteral("Turntable done: %1 frames").arg(m_choice.frames);
      finish();
      return;
    }
    encode();
  }

  // The video from the frames by the FFmpeg program.
  void encode() {
    QProcess listing;
    listing.start(m_choice.ffmpeg, {QStringLiteral("-hide_banner"), QStringLiteral("-encoders")});
    listing.waitForFinished(10000);
    m_encoder = turntableEncoder(m_choice.output, QString::fromUtf8(listing.readAllStandardOutput()));
    if (m_encoder.isEmpty()) {
      fail(QObject::tr("Could not encode the video: the FFmpeg program has no encoder for %1.")
               .arg(turntableExtension(m_choice.output).toUpper()),
           QStringLiteral("no %1 encoder in %2").arg(turntableExtension(m_choice.output), m_choice.ffmpeg));
      return;
    }
    if (m_progress) {
      m_progress->setLabelText(QObject::tr("Encoding the video…"));
    }
    m_ffmpeg = new QProcess(this);
    m_ffmpeg->setWorkingDirectory(m_folder);
    m_ffmpeg->setProcessChannelMode(QProcess::MergedChannels);
    const QStringList arguments = turntableFfmpegArguments(m_choice.output, m_encoder, m_choice.fps, m_choice.size,
                                                           m_choice.frames, m_choice.video);
    connect(m_ffmpeg, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
      const QString said = QString::fromUtf8(m_ffmpeg->readAll()).trimmed();
      if (m_done) {
        return;
      }
      if (status != QProcess::NormalExit || code != 0) {
        const QString last = said.section(QLatin1Char('\n'), -1).trimmed();
        fail(QObject::tr("Could not encode the video: %1.").arg(last.isEmpty() ? QObject::tr("FFmpeg ended with code %1").arg(code) : last),
             QStringLiteral("FFmpeg ended with code %1: %2").arg(code).arg(said));
        return;
      }
      encoded();
    });
    connect(m_ffmpeg, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
      if (error == QProcess::FailedToStart && !m_done) {
        fail(QObject::tr("Could not start the FFmpeg program %1.").arg(QDir::toNativeSeparators(m_choice.ffmpeg)),
             QStringLiteral("FFmpeg did not start: %1").arg(m_choice.ffmpeg));
      }
    });
    qInfo().noquote() << QStringLiteral("Turntable encoding: %1 %2").arg(m_choice.ffmpeg, arguments.join(QLatin1Char(' ')));
    m_ffmpeg->start(m_choice.ffmpeg, arguments);
  }

  // The written video checked by its container: its frames and size.
  void encoded() {
    const TurntableVideo video = readTurntableVideo(m_choice.video);
    const QString shown = QDir::toNativeSeparators(m_choice.video);
    if (!video.ok) {
      fail(QObject::tr("Could not read the video %1 back: %2.").arg(shown, video.error),
           QStringLiteral("video %1 not read back: %2").arg(m_choice.video, video.error));
      return;
    }
    qInfo().noquote() << QStringLiteral("Turntable video saved %1: %2 frames, %3 x %4 (%5, %6 fps)")
                             .arg(m_choice.video)
                             .arg(video.frames)
                             .arg(video.width)
                             .arg(video.height)
                             .arg(m_encoder)
                             .arg(m_choice.fps);
    if (video.frames != m_choice.frames || video.width != m_choice.size.width() ||
        video.height != m_choice.size.height()) {
      m_host.showStatus(QObject::tr("The video %1 has %2 frames of %3 × %4 pixels, not %5 of %6 × %7.")
                            .arg(shown)
                            .arg(video.frames)
                            .arg(video.width)
                            .arg(video.height)
                            .arg(m_choice.frames)
                            .arg(m_choice.size.width())
                            .arg(m_choice.size.height()),
                        NoticeLevel::Warning);
      qWarning().noquote() << QStringLiteral("Turntable video differs: %1 frames of %2 x %3 written, %4 of %5 x %6 asked")
                                  .arg(video.frames)
                                  .arg(video.width)
                                  .arg(video.height)
                                  .arg(m_choice.frames)
                                  .arg(m_choice.size.width())
                                  .arg(m_choice.size.height());
    } else {
      m_host.showStatus(QObject::tr("Turntable saved as %1.").arg(shown), NoticeLevel::Information);
    }
    qInfo().noquote() << QStringLiteral("Turntable done: %1 frames, video %2").arg(m_choice.frames).arg(m_choice.video);
    finish();
  }

  void cancel() {
    if (m_done) {
      return;
    }
    if (m_ffmpeg != nullptr && m_ffmpeg->state() != QProcess::NotRunning) {
      m_ffmpeg->kill();
    }
    if (m_render != nullptr && m_render->isRunning()) {
      m_render->cancel();
    }
    qInfo().noquote() << QStringLiteral("Turntable cancelled after %1 of %2 frames").arg(m_frame).arg(m_choice.frames);
    m_host.showStatus(QObject::tr("Export Turntable canceled."), NoticeLevel::Information);
    finish();
  }

  void fail(const QString& text, const QString& log) {
    if (m_done) {
      return;
    }
    qWarning().noquote() << QStringLiteral("Turntable failed: %1").arg(log);
    m_host.showStatus(text, NoticeLevel::Error);
    finish();
  }

  void finish() {
    m_done = true;
    if (m_progress) {
      m_progress->hide();
      m_progress->deleteLater();
    }
    deleteLater();
  }

  OcctViewer& m_viewer;
  DocumentHost& m_host;
  QWidget* m_window;
  TurntableChoice m_choice;
  gp_Ax1 m_axis;
  occ::handle<Graphic3d_Camera> m_start;
  double m_step = 0.0;
  QString m_folder;
  std::unique_ptr<QTemporaryDir> m_temporary;
  QPointer<QProgressDialog> m_progress;
  int m_frame = 0; // frames written
  std::optional<gp_Dir> m_previous;
  bool m_done = false;
  // Rendered frames.
  std::unique_ptr<QTemporaryDir> m_renderFolder;
  QJsonObject m_settings;
  QString m_scenePath;
  QString m_worker;
  QColor m_top;
  QColor m_bottom;
  render::FinalRender* m_render = nullptr;
  gp_Dir m_renderDirection;
  // The video.
  QProcess* m_ffmpeg = nullptr;
  QString m_encoder;
};

// The selection's first item that can have an axis: a round face, a
// circle or an arc, a construction or origin axis.
std::optional<SelectionItem> axisItemOf(const Selection& selection) {
  for (const SelectionItem& item : selection) {
    const bool round = item.geometry == QLatin1String("cylinder") || item.geometry == QLatin1String("cone") ||
                       item.geometry == QLatin1String("torus") || item.geometry == QLatin1String("circle") ||
                       item.geometry == QLatin1String("arc");
    if (item.kind == SelectKind::Axis || ((item.kind == SelectKind::Face || item.kind == SelectKind::Edge) && round)) {
      return item;
    }
  }
  return std::nullopt;
}

} // namespace

void ViewController::exportTurntable() {
  const std::optional<SelectionItem> axisItem = currentSelection ? axisItemOf(currentSelection()) : std::nullopt;
  TurntableDialogInput input;
  const QString name = m_documentName.isEmpty() ? tr("Turntable") : m_documentName;
  const QString folder = dialogStartFolder(DialogGroup::Export, m_host.documentFolder());
  input.start = folder.isEmpty() ? name + QStringLiteral(".mp4") : QDir(folder).filePath(name + QStringLiteral(".mp4"));
  input.view = m_viewer.size();
  input.renderer = m_render != nullptr;
  if (axisItem) {
    input.selectedAxis = SelectionItem::kindText(axisItem->kind);
  }
  const std::optional<TurntableChoice> choice = askTurntable(m_window, input);
  if (!choice) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export,
                       choice->video.isEmpty() ? QDir(choice->framesFolder).filePath(QStringLiteral("frame"))
                                               : choice->video);
  const TurntableChoice chosen = *choice;
  // The view's shapes and the model are read only while no job runs.
  m_host.whenIdle(this, [this, chosen, axisItem] {
    // From where the view is going: a camera animation ended, a pending
    // fit made.
    m_viewer.settleCamera();
    const occ::handle<Graphic3d_Camera> start = new Graphic3d_Camera(m_viewer.viewCamera());
    // Through the middle of the shown bodies, else the view's target.
    gp_Pnt centre = start->Center();
    const Bnd_Box box = m_viewer.bodiesBox();
    if (!box.IsVoid()) {
      double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
      box.Get(x0, y0, z0, x1, y1, z1);
      centre = gp_Pnt((x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0);
    }
    gp_Ax1 axis(centre, gp::DZ());
    if (chosen.axis == QLatin1String("y")) {
      axis = gp_Ax1(centre, gp::DY());
    } else if (chosen.axis == QLatin1String("x")) {
      axis = gp_Ax1(centre, gp::DX());
    } else if (chosen.axis == QLatin1String("up")) {
      axis = gp_Ax1(centre, start->Up());
    } else if (chosen.axis == QLatin1String("selection")) {
      // The item's axis as the measure query gives it (commands.md,
      // "Measure with axes"), in the design's coordinates.
      QJsonObject answer;
      if (axisItem) {
        QJsonObject reference = axisItem->reference().toObject();
        if (!axisItem->occurrence.isEmpty()) {
          reference.insert(QStringLiteral("occurrence"), axisItem->occurrence);
        }
        try {
          answer = m_host.model()
                       .queryObject({{QStringLiteral("query"), QStringLiteral("measure")}, {QStringLiteral("a"), reference}})
                       .value(QStringLiteral("a"))
                       .toObject()
                       .value(QStringLiteral("axis"))
                       .toObject();
        } catch (const std::exception& error) {
          qWarning().noquote() << QStringLiteral("Turntable axis not measured: %1").arg(QString::fromUtf8(error.what()));
        }
      }
      const QJsonArray origin = answer.value(QStringLiteral("origin")).toArray();
      const QJsonArray direction = answer.value(QStringLiteral("direction")).toArray();
      const gp_XYZ along(direction.at(0).toDouble(), direction.at(1).toDouble(), direction.at(2).toDouble());
      if (origin.size() != 3 || direction.size() != 3 || along.Modulus() < 1e-12) {
        m_host.showStatus(tr("Could not export the turntable: the selection has no axis."), NoticeLevel::Error);
        qWarning().noquote() << "Turntable failed: the selection has no axis";
        return;
      }
      axis = gp_Ax1(gp_Pnt(origin.at(0).toDouble(), origin.at(1).toDouble(), origin.at(2).toDouble()), gp_Dir(along));
    }
    auto* job = new TurntableJob(m_viewer, m_host, m_window, chosen, axis, start);
    job->run();
  });
}

} // namespace mitcad
