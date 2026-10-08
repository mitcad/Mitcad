// SPDX-License-Identifier: MIT
#pragma once

// The final render to an image file (mitcad#48, docs/rendering.md "Final
// render"), as File > Render Image and `mitcad-cli render` ask for it: the
// scene, the camera framed for the image, the job file and the render
// worker in its batch mode (mitcad-render --batch). Qt Core and Gui and
// OCCT, no widgets, so that the command line tool shares it.

#include <array>
#include <string>
#include <vector>

#include <QColor>
#include <QJsonObject>
#include <QObject>
#include <QRectF>
#include <QSize>
#include <QString>
#include <QStringList>

#include <TopoDS_Shape.hxx>
#include <gp_Trsf.hxx>

#include "framework/Appearances.hpp"
#include "render/Renderer.hpp"

class QProcess;
class QTimer;

namespace mitcad::render {

// Faces of a body with an appearance of their own (mitcad#53): their
// indices in the shape (geometry::Shape's order, as meshOf meshes them).
struct SceneFaces {
  std::vector<int> faces;
  Appearance appearance;
};

// A body as the final render sees it: its shape, where it is placed, its
// appearance (an empty id: the renderer's default grey) and its faces'.
struct SceneBody {
  std::string name;
  TopoDS_Shape shape;
  gp_Trsf placement;
  Appearance appearance;
  std::vector<SceneFaces> faces = {};
};

// A body's appearance as the renderer's material (mitcad#46); the default
// look keeps the renderer's default material. A texture whose image file
// was found (AppearanceTexture::file, resolveTexture) gives the base
// colour (mitcad#53).
MaterialData materialOf(const Appearance& appearance);

// The materials of faces with appearances of their own.
std::vector<FaceMaterial> faceMaterialsOf(const std::vector<SceneFaces>& faces);

// Why textures of the appearances are not drawn ("Wood: the image ... is
// missing"), each once; empty when all are.
QStringList missingTextures(const std::vector<const Appearance*>& appearances);

// The bodies' triangles (their display triangulations; faces without one
// are meshed) by content hash, and the bodies with their placements and
// materials (writeSceneFile writes it). Reads the shapes' triangulations:
// in the application only while no job computes them.
SceneUpdate sceneOf(const std::vector<SceneBody>& bodies);

// The worker's "environment" command (RenderProtocol.hpp) for the
// document's render settings: the environment with an image path made
// absolute against the design's folder, the ground, and whether the camera
// sees the environment (not when `transparent`: an image with an alpha
// channel shows only the bodies and their shadows).
QJsonObject environmentCommand(const QJsonObject& settings, const QString& documentFolder,
                               bool transparent = false);

// A camera: eye, target and up (mm), its projection and half the height
// it shows at the target, seen in a view of `aspect` (width / height).
struct Camera {
  std::array<double, 3> eye{0, -1, 0};
  std::array<double, 3> target{0, 0, 0};
  std::array<double, 3> up{0, 0, 1};
  bool perspective = false;
  double halfHeight = 100.0;
  double aspect = 1.0;
};

// The camera of a named view (the `named_views` query's entry) for an
// image of `aspect`: the view's height spans the image's shorter side, as
// the 3D view shows a named view.
Camera namedViewCamera(const QJsonObject& view, double aspect);

// The image's size in pixels for the render settings' `output` section:
// width x height, or with aspect `view` as high as `viewAspect` makes it.
QSize outputSize(const QJsonObject& output, double viewAspect);

// The part of a view of `view` size the image of `image` size shows: the
// largest rectangle of the image's aspect in the middle of the view (the
// frame the view shows for a fixed aspect).
QRectF imageFrame(const QSizeF& view, const QSizeF& image);

// The worker's "view" for an image of `size` through the camera: what the
// image's frame (imageFrame) shows of the camera's view.
QJsonObject finalView(const Camera& camera, const QSize& size);

// The file name extension of an `output.format` (png, png16, jpeg, exr).
QString outputExtension(const QString& format);

// The batch job (mitcad-render --batch, docs/rendering.md "Final render")
// for the document's render settings: the scene file, the environment,
// the view (finalView), the output section with the image's `path`, the
// film, the background behind the bodies (sRGB, a gradient from `top` to
// `bottom`: the view's or the settings' colour) and, when `previewPath` is
// not empty, a small preview image there.
QJsonObject finalJob(const QJsonObject& settings, const QString& documentFolder, const QString& scenePath,
                     const QJsonObject& view, const QString& imagePath, const QColor& top, const QColor& bottom,
                     const QString& previewPath);

// The worker's executable: MITCAD_RENDER_WORKER, else mitcad-render next to
// the running executable; empty when there is none.
QString workerExecutable();

// A final render in the render worker (mitcad-render --batch --control):
// writes nothing itself; starts the process with a job file, checks its
// protocol version and reports its progress, previews and result. The
// worker stops when it is cancelled or this object is gone.
class FinalRender : public QObject {
  Q_OBJECT

public:
  explicit FinalRender(QObject* parent = nullptr);
  ~FinalRender() override;

  // Writes `job` to `jobPath` and starts `worker` on it; false (and
  // failed() later) when the job cannot be written or there is no worker.
  bool start(const QString& worker, const QJsonObject& job, const QString& jobPath);
  // Asks the worker to stop, and ends it if it does not at once;
  // cancelled() follows once it is gone.
  void cancel();
  bool isRunning() const;
  qint64 processId() const;
  // The device the worker renders on (its "ready", then any "device": the
  // CPU took over; mitcad#50).
  QString deviceLabel() const { return m_deviceLabel; }

signals:
  void ready(const QString& renderer);
  // Samples so far of `total`, seconds since the render started, and an
  // estimate of the seconds left (negative: not known yet).
  void progress(int samples, int total, double seconds, double remaining);
  // The image so far as a small PNG at `path`.
  void preview(const QString& path, int samples);
  void warning(const QString& message);
  // The image file is written.
  void finished(const QString& path, const QSize& size, int samples, double seconds);
  void failed(const QString& reason);
  void cancelled();

private:
  void readOutput();
  void handle(const QJsonObject& event);
  void ended(int exitCode, bool crashed);

  QProcess* m_process = nullptr;
  QTimer* m_killTimer;
  QByteArray m_pending;
  QString m_lastError;
  QString m_deviceLabel;
  bool m_greeted = false;
  bool m_done = false;
  bool m_cancelling = false;
};

} // namespace mitcad::render
