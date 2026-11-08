// SPDX-License-Identifier: MIT
// model_render's ray-traced picture on the headless MCP server
// (mitcad#1489, docs/mcp.md "Preview images: model_render"). The Rust
// server (core/ffi/src/mcp/raytrace.rs) chooses the camera and the bodies
// as the fast mode does; here they are rendered as `mitcad-cli render`
// renders a design: the same scene (render-scene.hpp) and job
// (render::finalJob) in the render worker mitcad-render's batch mode. The
// call's samples, time limit, denoising and background replace the
// design's render settings for this picture only; the document does not
// change. No worker, or one that fails, is an error with its error_code,
// never a blank image.

#include "mcp-raytrace.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <set>
#include <string>
#include <utility>

#include <QBuffer>
#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSize>
#include <QTemporaryDir>
#include <QTimer>

#include "mitcad_bridge/lib.h"
#include "render-scene.hpp"
#include "render/RenderBatch.hpp"
#include "render/RenderScene.hpp"

namespace mitcad::cli {

namespace {

// The time the worker may take beyond the render's own limit: starting,
// reading the scene, denoising and writing the image.
constexpr int kStartSeconds = 60;
// A picture scaled down for the byte budget: at most so many times, never
// below so many pixels on its longer side (as the fast mode's).
constexpr int kMaxDownscales = 8;
constexpr int kMinSide = 64;

const char* const kMissing =
    "The render worker mitcad-render is not installed next to mitcad-cli (or MITCAD_RENDER_WORKER names no "
    "executable): ray-traced pictures need it.";

QJsonObject refusal(const char* code, const QString& message) {
  return {{QStringLiteral("error"), message}, {QStringLiteral("error_code"), QString::fromLatin1(code)}};
}

std::array<double, 3> vectorOf(const QJsonValue& value) {
  const QJsonArray items = value.toArray();
  return {items.at(0).toDouble(), items.at(1).toDouble(), items.at(2).toDouble()};
}

QByteArray pngOf(const QImage& image) {
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  image.save(&buffer, "PNG");
  return bytes;
}

QJsonObject renderPicture(const mitcad::Document& document, const QJsonObject& request) {
  if (QCoreApplication::instance() == nullptr) {
    return refusal("render_worker_failed", QStringLiteral("the raytracer runs without a Qt application"));
  }
  const QString worker = render::workerExecutable();
  if (worker.isEmpty()) {
    return refusal("render_worker_missing", QString::fromLatin1(kMissing));
  }
  const QJsonArray size = request.value(QStringLiteral("size")).toArray();
  const QSize pixels(std::max(1, size.at(0).toInt()), std::max(1, size.at(1).toInt()));
  const QString folder = request.value(QStringLiteral("folder")).toString();
  const QJsonValue background = request.value(QStringLiteral("background"));
  const bool transparent = background.toString() == QLatin1String("transparent");
  const double timeLimit = request.value(QStringLiteral("time_limit_s")).toDouble(30.0);

  // The design's render settings with this picture's output.
  QJsonObject settings = renderQuery(document, QStringLiteral("render_settings")).toObject();
  QJsonObject output = settings.value(QStringLiteral("output")).toObject();
  output.insert(QStringLiteral("width"), pixels.width());
  output.insert(QStringLiteral("height"), pixels.height());
  output.insert(QStringLiteral("aspect"), QStringLiteral("fixed"));
  output.insert(QStringLiteral("format"), QStringLiteral("png"));
  output.insert(QStringLiteral("samples"), request.value(QStringLiteral("samples")).toInt(64));
  output.insert(QStringLiteral("time_limit"), timeLimit);
  output.insert(QStringLiteral("denoise"), request.value(QStringLiteral("denoise")).toBool(true));
  output.insert(QStringLiteral("transparent"), transparent);
  settings.insert(QStringLiteral("output"), output);

  // The placements the server chose (every one is in `instances` with
  // `hidden`).
  std::set<std::pair<QString, QString>> wanted;
  for (const QJsonValue& value : request.value(QStringLiteral("instances")).toArray()) {
    const QJsonObject entry = value.toObject();
    wanted.insert({entry.value(QStringLiteral("body")).toString(), entry.value(QStringLiteral("occurrence")).toString()});
  }
  const QJsonDocument all = QJsonDocument::fromJson(
      QByteArray::fromStdString(std::string(document.query(R"({"query": "instances", "hidden": true})"))));
  QJsonArray instances;
  for (const QJsonValue& value : all.array()) {
    const QJsonObject instance = value.toObject();
    if (wanted.count({instance.value(QStringLiteral("body")).toString(),
                      instance.value(QStringLiteral("occurrence")).toString()}) != 0) {
      instances.append(instance);
    }
  }
  const DocumentScene scene = documentScene(document, folder, instances);
  QJsonArray warnings;
  for (const QString& line : scene.missingTextures) {
    warnings.append(QStringLiteral("a texture image is not drawn: %1").arg(line));
  }

  QTemporaryDir dir;
  if (!dir.isValid()) {
    return refusal("render_worker_failed", QStringLiteral("no temporary folder: %1").arg(dir.errorString()));
  }
  const QString scenePath = dir.filePath(QStringLiteral("scene.glb"));
  std::string error;
  if (!render::writeSceneFile(render::sceneOf(scene.bodies), QDir::toNativeSeparators(scenePath).toStdString(),
                              error)) {
    return refusal("render_worker_failed",
                   QStringLiteral("the scene cannot be written: %1").arg(QString::fromStdString(error)));
  }

  // The server's camera: its height spans the picture's.
  const QJsonObject view = request.value(QStringLiteral("camera")).toObject();
  render::Camera camera;
  camera.eye = vectorOf(view.value(QStringLiteral("eye")));
  camera.target = vectorOf(view.value(QStringLiteral("target")));
  camera.up = vectorOf(view.value(QStringLiteral("up")));
  camera.perspective = view.value(QStringLiteral("projection")).toString() == QLatin1String("perspective");
  camera.halfHeight = view.value(QStringLiteral("height")).toDouble(100.0) / 2.0;
  camera.aspect = static_cast<double>(pixels.width()) / pixels.height();

  QColor top;
  QColor bottom;
  renderBackground(settings, top, bottom);
  if (background.isArray()) {
    const QJsonArray rgb = background.toArray();
    top = bottom = QColor(rgb.at(0).toInt(), rgb.at(1).toInt(), rgb.at(2).toInt());
  }
  const QString imagePath = dir.filePath(QStringLiteral("picture.png"));
  QJsonObject job = render::finalJob(settings, folder, scenePath, render::finalView(camera, pixels), imagePath, top,
                                     bottom, QString());
  job.insert(QStringLiteral("device"), QStringLiteral("auto"));

  render::FinalRender final;
  QEventLoop loop;
  QString failure;
  bool done = false;
  int samples = 0;
  double seconds = 0.0;
  QObject::connect(&final, &render::FinalRender::warning, &loop,
                   [&warnings](const QString& message) { warnings.append(message); });
  QObject::connect(&final, &render::FinalRender::finished, &loop,
                   [&](const QString&, const QSize&, int rendered, double took) {
                     done = true;
                     samples = rendered;
                     seconds = took;
                     loop.quit();
                   });
  QObject::connect(&final, &render::FinalRender::failed, &loop, [&](const QString& reason) {
    failure = reason;
    loop.quit();
  });
  QObject::connect(&final, &render::FinalRender::cancelled, &loop, [&] {
    failure = QStringLiteral("the render did not finish within %1 s").arg(timeLimit + kStartSeconds);
    loop.quit();
  });
  // A worker that hangs is stopped: the render's limit and its start.
  QTimer guard;
  guard.setSingleShot(true);
  QObject::connect(&guard, &QTimer::timeout, &final, [&final] { final.cancel(); });
  guard.start(static_cast<int>((timeLimit + kStartSeconds) * 1000.0));
  final.start(worker, job, dir.filePath(QStringLiteral("job.json")));
  loop.exec();
  guard.stop();
  if (!done) {
    return refusal("render_worker_failed", QStringLiteral("the render worker failed: %1").arg(failure));
  }
  QFile file(imagePath);
  if (!file.open(QIODevice::ReadOnly)) {
    return refusal("render_worker_failed", QStringLiteral("the render worker wrote no image"));
  }
  QByteArray png = file.readAll();
  file.close();
  QImage image;
  if (!image.loadFromData(png, "PNG") || image.isNull()) {
    return refusal("render_worker_failed", QStringLiteral("the render worker's image is no PNG"));
  }
  // At full size for the server to write to its path.
  QJsonObject answer;
  if (request.value(QStringLiteral("full")).toBool()) {
    answer.insert(QStringLiteral("full_base64"), QString::fromLatin1(png.toBase64()));
  }
  // The byte budget, as ui_screenshot's and the fast mode's: scaled down
  // until the PNG fits.
  const qint64 budget = static_cast<qint64>(request.value(QStringLiteral("max_bytes")).toDouble(5.0 * 1024 * 1024));
  bool downscaled = false;
  for (int attempt = 0; png.size() > budget; ++attempt) {
    const double factor = std::sqrt(static_cast<double>(budget) / static_cast<double>(png.size())) * 0.9;
    const QSize smaller(static_cast<int>(image.width() * factor), static_cast<int>(image.height() * factor));
    if (attempt >= kMaxDownscales || std::max(smaller.width(), smaller.height()) < kMinSide) {
      QJsonObject tooLarge = refusal(
          "render_too_large", QStringLiteral("the picture takes more than max_bytes (%1) even at %2 x %3 pixels (%4 "
                                             "bytes); allow more bytes, ask for a smaller size, or write it to a "
                                             "file with path")
                                  .arg(budget)
                                  .arg(image.width())
                                  .arg(image.height())
                                  .arg(png.size()));
      tooLarge.insert(QStringLiteral("smallest_bytes"), static_cast<double>(png.size()));
      tooLarge.insert(QStringLiteral("smallest_width"), image.width());
      tooLarge.insert(QStringLiteral("smallest_height"), image.height());
      return tooLarge;
    }
    image = image.scaled(smaller, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    png = pngOf(image);
    downscaled = true;
  }
  answer.insert(QStringLiteral("image_base64"), QString::fromLatin1(png.toBase64()));
  answer.insert(QStringLiteral("width"), image.width());
  answer.insert(QStringLiteral("height"), image.height());
  answer.insert(QStringLiteral("bytes"), static_cast<double>(png.size()));
  answer.insert(QStringLiteral("downscaled_for_bytes"), downscaled);
  answer.insert(QStringLiteral("samples"), samples);
  answer.insert(QStringLiteral("render_ms"), std::round(seconds * 1000.0));
  answer.insert(QStringLiteral("device"), final.deviceLabel());
  answer.insert(QStringLiteral("warnings"), warnings);
  return answer;
}

class CliRaytracer final : public McpRaytracer {
public:
  rust::String render(const Document& document, rust::Str request) const override {
    QJsonObject answer;
    try {
      const QJsonDocument parsed =
          QJsonDocument::fromJson(QByteArray(request.data(), static_cast<qsizetype>(request.size())));
      answer = renderPicture(document, parsed.object());
    } catch (const std::exception& error) {
      answer = refusal("render_worker_failed", QStringLiteral("the picture failed: %1").arg(error.what()));
    } catch (...) {
      answer = refusal("render_worker_failed", QStringLiteral("the picture failed"));
    }
    const QByteArray text = QJsonDocument(answer).toJson(QJsonDocument::Compact);
    return rust::String(text.constData(), static_cast<std::size_t>(text.size()));
  }
};

} // namespace

std::unique_ptr<McpRaytracer> makeMcpRaytracer() { return std::make_unique<CliRaytracer>(); }

} // namespace mitcad::cli
