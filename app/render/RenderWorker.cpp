// SPDX-License-Identifier: MIT
// mitcad-render, the render worker (docs/rendering.md): an executable of
// its own without windows that renders what the application asks on stdin
// into shared memory (RenderProtocol.hpp), so that a crash or an
// out-of-memory in the renderer ends only it, and so that only it loads
// the renderer's libraries. The application starts it for View > Rendered,
// and with --batch for a final render to an image file (Render Image,
// mitcad-cli render; mitcad#48).

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <initializer_list>
#include <iostream>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QCommandLineOption>
#include <QCommandLineParser>
#include <QCoreApplication>
#include <QDir>
#include <QImage>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QFile>
#include <QSaveFile>
#include <QString>

#include "platform/MacChrome.hpp"
#include "render/FrameMemory.hpp"
#include "render/RenderDevices.hpp"
#include "render/RenderImage.hpp"
#include "render/RenderOutput.hpp"
#include "render/RenderProtocol.hpp"
#include "render/RenderScene.hpp"
#include "render/Renderer.hpp"
#ifdef MITCAD_RENDER_XPC_ENTRY
#include "render/RenderSession.hpp"
#endif
#include "report/CrashHandler.hpp"

namespace mitcad {

using render::FrameHeader;

namespace {

// Events go to stdout from the main thread and the renderer's.
std::mutex g_output;

void emitEvent(const QJsonObject& event) {
  const QByteArray line = QJsonDocument(event).toJson(QJsonDocument::Compact) + '\n';
  const std::lock_guard<std::mutex> lock(g_output);
  std::fwrite(line.constData(), 1, static_cast<std::size_t>(line.size()), stdout);
  std::fflush(stdout);
}

void emitError(const QString& message) {
  emitEvent({{QStringLiteral("event"), QStringLiteral("error")}, {QStringLiteral("message"), message}});
}

// ---------------------------------------------------------------------------
// Devices (mitcad#50, docs/rendering.md "Devices")

// Cycles' runtime resource root: Resources/cycles inside a macOS bundle,
// lib/mitcad/cycles beside bin in other packages, or cycles beside the
// worker in a build tree. Metal reads source/{kernel,util} below the root.
std::string kernelFolder() {
  const QDir executable(QCoreApplication::applicationDirPath());
  for (const QString& candidate : {QStringLiteral("../Resources/cycles"), QStringLiteral("../lib/mitcad/cycles"),
#ifdef __APPLE__
                                    QStringLiteral("../../../../Resources/cycles"),
#endif
                                    QStringLiteral("cycles")}) {
    if (QDir(executable.filePath(candidate)).exists()) {
      return QDir::cleanPath(executable.filePath(candidate)).toStdString();
    }
  }
  return executable.path().toStdString();
}

QJsonObject deviceJson(const render::DeviceInfo& device) {
  return {{QStringLiteral("id"), QString::fromStdString(device.id)},
          {QStringLiteral("type"), QString::fromStdString(device.type)},
          {QStringLiteral("name"), QString::fromStdString(device.name)},
          {QStringLiteral("denoiser"), !device.denoiser.executed ? QStringLiteral("pending")
                                      : device.denoisesOnDevice ? QStringLiteral("device")
                                      : device.denoiser.type == "CPU" ? QStringLiteral("cpu") : QStringLiteral("gpu")},
          {QStringLiteral("denoiser_available"), device.denoiserAvailable},
          {QStringLiteral("denoiser_device"),
           QJsonObject{{QStringLiteral("id"), QString::fromStdString(device.denoiser.id)},
                       {QStringLiteral("type"), QString::fromStdString(device.denoiser.type)},
                       {QStringLiteral("name"), QString::fromStdString(device.denoiser.name)},
                       {QStringLiteral("executed"), device.denoiser.executed},
                       {QStringLiteral("message"), QString::fromStdString(device.denoiser.message)}}},
          {QStringLiteral("metalrt_available"), device.metalRTAvailable},
          {QStringLiteral("metalrt_default"), device.metalRTDefault},
          {QStringLiteral("metalrt"), device.metalRT}};
}

QJsonArray devicesJson(const std::vector<render::DeviceInfo>& devices) {
  QJsonArray list;
  for (const render::DeviceInfo& device : devices) {
    list.append(deviceJson(device));
  }
  return list;
}

// The renderer on the chosen device ("auto", "cpu", a device's id or
// type), falling back to the CPU (RenderDevices.hpp); `fellBack` hears why,
// also when the choice names no device. The worker's
// MITCAD_RENDER_TEST_DEVICE_FAILURE makes the first device fail (tests).
std::unique_ptr<render::Renderer> makeRenderer(render::FrameSink& sink, bool interactive, const QString& choice,
                                               const std::vector<render::DeviceInfo>& devices,
                                               const render::DeviceReport& fellBack, std::string& error,
                                               const QJsonObject* configuration = nullptr) {
  std::string message;
  const render::DeviceInfo device = render::chooseDevice(choice.toStdString(), devices, message);
  if (!message.empty()) {
    fellBack(device, message);
  }
  const std::string folder = kernelFolder();
  const std::string denoising = (configuration ? configuration->value(QStringLiteral("denoiser")).toString() :
      qEnvironmentVariable("MITCAD_RENDER_TEST_DENOISER")).toLower().toStdString();
  const std::string denoisingFailure = (configuration ? configuration->value(QStringLiteral("denoiser_failure")).toString() :
      qEnvironmentVariable("MITCAD_RENDER_TEST_DENOISER_FAILURE")).toStdString();
  const bool nativePassComparison = configuration && configuration->value(QStringLiteral("native_pass_compare")).toBool();
  const auto factory = [interactive, folder, denoising, denoisingFailure, nativePassComparison](render::FrameSink& target,
                                             const render::DeviceInfo& on, bool cpuDenoising,
                                             std::string& why) {
    render::RendererOptions options;
    options.interactive = interactive;
    options.device = on;
    options.kernelFolder = folder;
    options.denoisingDevice = denoising;
    options.testDenoisingFailure = denoisingFailure;
    options.testNativePassComparison = nativePassComparison;
    options.cpuDenoisingFallback = cpuDenoising;
    return render::makeCyclesRenderer(target, options, why);
  };
  return render::makeDeviceRenderer(sink, device, factory, fellBack,
                                    (configuration ? configuration->value(QStringLiteral("device_failure")).toString() :
                                      qEnvironmentVariable("MITCAD_RENDER_TEST_DEVICE_FAILURE")).toStdString(), error);
}

// Frames into the application's shared memory (RenderProtocol.hpp).
class SharedFrames : public render::FrameSink {
public:
  void denoisingDevice(const render::DeviceInfo& device) override {
    emitEvent({{QStringLiteral("event"), QStringLiteral("denoiser")},
               {QStringLiteral("device"), deviceJson(device)}});
  }
#ifndef _WIN32
  // The socket the application's memory comes through.
  void setChannel(int descriptor) { m_channel.adopt(descriptor); }
#endif

  // The "memory" command: the application's new segment replaces the old.
  bool attach(const QJsonObject& command, QString& error) {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_memory.reset();
    m_memoryId = command.value(QStringLiteral("id")).toInt();
    auto memory = std::make_unique<render::FrameMemory>();
    std::string why;
#ifdef _WIN32
    const bool attached = memory->attach(command.value(QStringLiteral("key")).toString().toStdString(), why);
#else
    const int descriptor = m_channel.receive(why);
    const bool attached = descriptor >= 0 && memory->attach(descriptor, why);
#endif
    if (!attached) {
      error = QString::fromStdString(why);
      return false;
    }
    const auto* header = static_cast<const FrameHeader*>(memory->data());
    if (memory->size() < render::kPixelsOffset || header->magic != render::kFrameMagic ||
        header->version != render::kProtocolVersion ||
        memory->size() < render::segmentBytes(header->capacityWidth, header->capacityHeight)) {
      error = QStringLiteral("the shared memory is not a frame buffer of this version");
      return false;
    }
    m_memory = std::move(memory);
    return true;
  }

  render::FrameBuffers beginFrame(int width, int height, int fullWidth, int fullHeight, bool withCatcher) override {
    m_mutex.lock();
    auto* header = m_memory ? static_cast<FrameHeader*>(m_memory->data()) : nullptr;
    if (header == nullptr || width <= 0 || height <= 0 || static_cast<std::uint32_t>(width) > header->capacityWidth ||
        static_cast<std::uint32_t>(height) > header->capacityHeight) {
      m_mutex.unlock();
      return {};
    }
    // A slot the application neither shows next nor reads now.
    const std::uint32_t published = header->published.load();
    const std::uint32_t reading = header->reading.load();
    m_slot = 0;
    while (m_slot == published || m_slot == reading) {
      ++m_slot;
    }
    render::FrameInfo& info = header->frames[m_slot];
    info.width = static_cast<std::uint32_t>(width);
    info.height = static_cast<std::uint32_t>(height);
    info.fullWidth = static_cast<std::uint32_t>(fullWidth);
    info.fullHeight = static_cast<std::uint32_t>(fullHeight);
    info.catcher = withCatcher ? 1 : 0;
    return {reinterpret_cast<std::uint16_t*>(render::slotPixels(header, m_slot)),
            withCatcher ? reinterpret_cast<std::uint16_t*>(render::slotPixels(header, m_slot, 1)) : nullptr};
  }

  void endFrame(std::uint64_t view, int samples, bool denoised) override {
    auto* header = static_cast<FrameHeader*>(m_memory->data());
    render::FrameInfo& info = header->frames[m_slot];
    info.samples = static_cast<std::uint32_t>(samples);
    info.denoised = denoised ? 1 : 0;
    info.view = view;
    header->published.store(m_slot);
    const QJsonObject event{{QStringLiteral("event"), QStringLiteral("frame")},
                            {QStringLiteral("memory"), m_memoryId},
                            {QStringLiteral("slot"), static_cast<int>(m_slot)},
                            {QStringLiteral("view"), static_cast<qint64>(view)},
                            {QStringLiteral("size"), QJsonArray{static_cast<int>(info.width), static_cast<int>(info.height)}},
                            {QStringLiteral("samples"), samples},
                            {QStringLiteral("denoised"), denoised}};
    m_mutex.unlock();
    emitEvent(event);
  }

  void finished(std::uint64_t view, int samples, double seconds) override {
    emitEvent({{QStringLiteral("event"), QStringLiteral("done")},
               {QStringLiteral("view"), static_cast<qint64>(view)},
               {QStringLiteral("samples"), samples},
               {QStringLiteral("seconds"), seconds}});
  }

private:
  // Held from beginFrame to endFrame: a new segment waits for the frame.
  std::mutex m_mutex;
  std::unique_ptr<render::FrameMemory> m_memory;
#ifndef _WIN32
  render::FrameChannel m_channel;
#endif
  int m_memoryId = 0; // the "memory" command's, which frames name
  std::uint32_t m_slot = 0;
};

// The last frame in memory, for --bench, and when each view's first frame
// came and its samples were done.
class KeptFrame : public render::FrameSink {
public:
  void denoisingDevice(const render::DeviceInfo& device) override {
    std::printf("render-denoiser %s %s, executed %s\n", device.denoiser.type.c_str(),
                device.denoiser.name.c_str(), device.denoiser.executed ? "yes" : "no");
    std::fflush(stdout);
  }
  using Clock = std::chrono::steady_clock;

  render::FrameBuffers beginFrame(int width, int height, int, int, bool withCatcher) override {
    m_mutex.lock();
    width_ = width;
    height_ = height;
    pixels.resize(static_cast<std::size_t>(width) * height * 4);
    catcher.resize(withCatcher ? pixels.size() : 0);
    return {pixels.data(), withCatcher ? catcher.data() : nullptr};
  }
  void endFrame(std::uint64_t view, int frameSamples, bool denoised) override {
    samples = frameSamples;
    if (firstFrame.find(view) == firstFrame.end()) {
      firstFrame[view] = Clock::now();
    }
    if (trace) {
      std::printf("render-frame view %llu at %.0f ms: %d x %d, %d samples%s%s\n",
                  static_cast<unsigned long long>(view),
                  std::chrono::duration<double, std::milli>(Clock::now() - traceStart).count(), width_, height_,
                  frameSamples, denoised ? ", denoised" : "", catcher.empty() ? "" : ", ground");
      // Each frame as shown, for a look at how a render refines.
      if (!frameFolder.isEmpty()) {
        render::displayImage(pixels.data(), width_, height_, QColor(247, 248, 250), QColor(223, 227, 234),
                             render::Look(), catcher.empty() ? nullptr : catcher.data())
            .save(QDir(frameFolder).filePath(QStringLiteral("frame-%1.png").arg(++traced, 3, 10, QLatin1Char('0'))));
      }
    }
    if (denoised && firstDenoised.find(view) == firstDenoised.end()) {
      firstDenoised[view] = Clock::now();
      firstDenoisedSize[view] = {width_, height_, frameSamples};
    }
    if (captureDenoised && denoised && view <= 2) {
      const QString key = QStringLiteral("view-%1-%2x%3-samples-%4")
                              .arg(view).arg(width_).arg(height_).arg(frameSamples);
      denoisedFrames[key] = render::displayImage(
          pixels.data(), width_, height_, QColor(247, 248, 250), QColor(223, 227, 234),
          render::Look(), catcher.empty() ? nullptr : catcher.data()).convertToFormat(QImage::Format_RGB32);
    }
    m_mutex.unlock();
    m_changed.notify_all();
  }
  void finished(std::uint64_t view, int, double) override {
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      done[view] = Clock::now();
    }
    m_changed.notify_all();
  }
  // Waits until the view's samples are done (at most a minute).
  bool waitDone(std::uint64_t view) {
    std::unique_lock<std::mutex> lock(m_mutex);
    return m_changed.wait_for(lock, std::chrono::minutes(1), [&] { return done.count(view) != 0; });
  }

  std::vector<std::uint16_t> pixels;
  std::vector<std::uint16_t> catcher; // the ground's catcher factors, or none
  bool trace = false; // prints every frame
  QString frameFolder; // and saves it there as PNG
  int traced = 0;
  Clock::time_point traceStart = Clock::now();
  int width_ = 0;
  int height_ = 0;
  int samples = 0;
  std::map<std::uint64_t, Clock::time_point> firstFrame;
  std::map<std::uint64_t, Clock::time_point> firstDenoised;
  std::map<std::uint64_t, std::array<int, 3>> firstDenoisedSize; // width, height, samples
  std::map<std::uint64_t, Clock::time_point> done;
  bool captureDenoised = false; // Only compare benchmarks retain stable-view previews.
  std::map<QString, QImage> denoisedFrames;

private:
  std::mutex m_mutex;
  std::condition_variable m_changed;
};

QJsonArray stringsOf(const std::vector<std::string>& strings) {
  QJsonArray array;
  for (const std::string& string : strings) {
    array.append(QString::fromStdString(string));
  }
  return array;
}

// What a "scene" command changed: the bodies whose meshes came with it
// ("received"), and the bodies added, changed (another mesh or material),
// moved and removed.
QJsonObject sceneEvent(int sequence, const render::SceneUpdate& update, const render::SceneChanges& changes,
                       double milliseconds) {
  std::vector<std::string> received;
  for (const render::BodyData& body : update.bodies) {
    for (const auto& [key, mesh] : update.meshes) {
      if (key == body.mesh) {
        received.push_back(body.id);
        break;
      }
    }
  }
  return {{QStringLiteral("event"), QStringLiteral("scene")},
             {QStringLiteral("sequence"), sequence},
             {QStringLiteral("bodies"), static_cast<int>(update.bodies.size())},
             {QStringLiteral("received"), stringsOf(received)},
             {QStringLiteral("meshes_received"), static_cast<int>(update.meshes.size())},
             {QStringLiteral("added"), stringsOf(changes.added)},
             {QStringLiteral("changed"), stringsOf(changes.changed)},
             {QStringLiteral("moved"), stringsOf(changes.moved)},
             {QStringLiteral("removed"), stringsOf(changes.removed)},
             {QStringLiteral("kept"), changes.kept},
             {QStringLiteral("meshes"), changes.meshes},
             {QStringLiteral("instanced"), changes.instanced},
             {QStringLiteral("restarted"), changes.restarted},
             {QStringLiteral("milliseconds"), milliseconds}};
}

void emitSceneEvent(int sequence, const render::SceneUpdate& update, const render::SceneChanges& changes,
                    double milliseconds) {
  emitEvent(sceneEvent(sequence, update, changes, milliseconds));
}

std::array<double, 3> vectorOf(const QJsonValue& value) {
  const QJsonArray array = value.toArray();
  return {array.at(0).toDouble(), array.at(1).toDouble(), array.at(2).toDouble()};
}

render::ViewData viewOf(const QJsonObject& command) {
  render::ViewData view;
  view.sequence = static_cast<std::uint64_t>(command.value(QStringLiteral("sequence")).toInteger());
  view.perspective = command.value(QStringLiteral("projection")).toString() == QLatin1String("perspective");
  view.eye = vectorOf(command.value(QStringLiteral("eye")));
  view.target = vectorOf(command.value(QStringLiteral("target")));
  view.up = vectorOf(command.value(QStringLiteral("up")));
  view.halfHeight = command.value(QStringLiteral("half_height")).toDouble();
  const QJsonArray size = command.value(QStringLiteral("size")).toArray();
  view.width = size.at(0).toInt();
  view.height = size.at(1).toInt();
  return view;
}

// The "environment" command (and --environment): the document's render
// settings as the model has them (core/model-api/src/commands.md, "Render
// settings"), the image's path made absolute by the application.
render::EnvironmentData environmentOf(const QJsonObject& command) {
  using Preset = render::EnvironmentData::Preset;
  render::EnvironmentData data;
  const QJsonObject environment = command.value(QStringLiteral("environment")).toObject();
  const QString preset = environment.value(QStringLiteral("preset")).toString();
  data.preset = preset == QLatin1String("studio_white")  ? Preset::StudioWhite
                : preset == QLatin1String("studio_dark") ? Preset::StudioDark
                : preset == QLatin1String("outdoor")     ? Preset::Outdoor
                : preset == QLatin1String("image")       ? Preset::Image
                                                         : Preset::Studio;
  const auto number = [](const QJsonObject& object, const char* key, float fallback) {
    return static_cast<float>(object.value(QLatin1String(key)).toDouble(fallback));
  };
  data.strength = number(environment, "strength", data.strength);
  data.rotation = number(environment, "rotation", data.rotation);
  data.sunElevation = number(environment, "sun_elevation", data.sunElevation);
  data.sunAzimuth = number(environment, "sun_azimuth", data.sunAzimuth);
  data.image = environment.value(QStringLiteral("image")).toString().toStdString();
  data.background = command.value(QStringLiteral("background")).toObject().value(QStringLiteral("mode")).toString() ==
                    QLatin1String("environment");
  const QJsonObject ground = command.value(QStringLiteral("ground")).toObject();
  data.groundShadows = ground.value(QStringLiteral("shadows")).toBool(true);
  data.groundReflections = ground.value(QStringLiteral("reflections")).toBool(false);
  const QJsonValue height = ground.value(QStringLiteral("height"));
  data.groundAtHeight = height.isDouble();
  data.groundHeight = height.toDouble();
  // The user's lights (mitcad#54), those that are on; colours sRGB.
  const auto linear = [](double srgb) {
    return static_cast<float>(srgb <= 0.04045 ? srgb / 12.92 : std::pow((srgb + 0.055) / 1.055, 2.4));
  };
  for (const QJsonValue& value : command.value(QStringLiteral("lights")).toArray()) {
    const QJsonObject light = value.toObject();
    if (!light.value(QStringLiteral("enabled")).toBool(true)) {
      continue;
    }
    using Kind = render::LightData::Kind;
    render::LightData entry;
    const QString kind = light.value(QStringLiteral("type")).toString();
    entry.kind = kind == QLatin1String("spot")   ? Kind::Spot
                 : kind == QLatin1String("area") ? Kind::Area
                 : kind == QLatin1String("sun")  ? Kind::Sun
                                                 : Kind::Point;
    entry.camera = light.value(QStringLiteral("space")).toString() == QLatin1String("camera");
    if (light.value(QStringLiteral("position")).toArray().size() == 3) {
      entry.position = vectorOf(light.value(QStringLiteral("position")));
    }
    if (light.value(QStringLiteral("direction")).toArray().size() == 3) {
      entry.direction = vectorOf(light.value(QStringLiteral("direction")));
    }
    const QJsonArray color = light.value(QStringLiteral("color")).toArray();
    if (color.size() == 3) {
      entry.color = {linear(color.at(0).toDouble()), linear(color.at(1).toDouble()), linear(color.at(2).toDouble())};
    }
    entry.power = number(light, "power", entry.power);
    entry.size = number(light, "size", entry.size);
    entry.sizeY = number(light, "size_y", entry.sizeY);
    entry.disc = light.value(QStringLiteral("shape")).toString() == QLatin1String("disc");
    entry.spotAngle = number(light, "spot_angle", entry.spotAngle);
    entry.spotBlend = number(light, "spot_blend", entry.spotBlend);
    entry.angle = number(light, "angle", entry.angle);
    data.lights.push_back(entry);
  }
  return data;
}

double millisecondsBetween(KeptFrame::Clock::time_point from, KeptFrame::Clock::time_point to) {
  return std::chrono::duration<double, std::milli>(to - from).count();
}

// When the view's first denoised frame came, its size and samples.
QString firstDenoised(KeptFrame& frame, std::uint64_t view, KeptFrame::Clock::time_point from) {
  const auto at = frame.firstDenoised.find(view);
  if (at == frame.firstDenoised.end()) {
    return QStringLiteral("no denoised frame");
  }
  const std::array<int, 3>& size = frame.firstDenoisedSize[view];
  return QStringLiteral("first denoised frame %1 ms (%2 x %3, %4 samples)")
      .arg(millisecondsBetween(from, at->second), 0, 'f', 0)
      .arg(size[0])
      .arg(size[1])
      .arg(size[2]);
}

QString listed(const std::vector<std::string>& ids) {
  QStringList items;
  for (const std::string& id : ids) {
    items << QString::fromStdString(id);
  }
  return items.isEmpty() ? QStringLiteral("-") : items.join(QLatin1Char(','));
}

// Both transports and the regression decode the same protocol-9 command.
void applyPauseCommand(render::Renderer& renderer, const QJsonObject& command) {
  renderer.setPaused(command.value(QStringLiteral("paused")).toBool());
}

// The real interactive CPU renderer pauses between batches. Request the
// pause at the first completed full-resolution frame, before the next batch
// can finish the view, instead of racing a machine-dependent sleep against it.
// Preview sample counts configure denoising, not mandatory display boundaries;
// Cycles can coalesce those updates for a small, quickly rendered scene.
class PauseFrame final : public KeptFrame {
public:
  static constexpr int kSamples = 16;
  render::Renderer* renderer = nullptr;
  std::atomic<bool> requested{false};
  std::atomic<bool> resumed{false};
  std::atomic<int> publications{0};
  std::atomic<int> firstResumedSample{0};

  void endFrame(std::uint64_t view, int count, bool denoised) override {
    std::printf("render-update pause frame: view %llu, %d x %d, samples %d, denoised %d\n",
                static_cast<unsigned long long>(view), width_, height_, count, int(denoised));
    const bool fullPartialFrame = view == 1 && width_ == 160 && height_ == 120 && count > 0 && count < kSamples;
    KeptFrame::endFrame(view, count, denoised);
    publications.fetch_add(1);
    if (resumed.load()) {
      int empty = 0;
      firstResumedSample.compare_exchange_strong(empty, count);
    }
    if (fullPartialFrame && !requested.exchange(true)) {
      applyPauseCommand(*renderer, {{QStringLiteral("cmd"), QStringLiteral("pause")},
                                    {QStringLiteral("paused"), true}});
    }
  }
};

bool runPauseUpdateTest(const render::RendererOptions& options) {
  PauseFrame frame;
  std::string error;
  const auto renderer = render::makeCyclesRenderer(frame, options, error);
  if (!renderer) {
    std::fprintf(stderr, "render-update pause: cannot start CPU renderer: %s\n", error.c_str());
    return false;
  }
  if (renderer->device().type != "CPU") {
    std::fprintf(stderr, "render-update pause: the synthetic check requires the CPU renderer\n");
    return false;
  }
  frame.renderer = renderer.get();
  renderer->setSamples(PauseFrame::kSamples);
  renderer->setPreviews({4});
  const render::SceneUpdate scene = render::testScene();
  renderer->updateScene(scene);
  render::ViewData view;
  view.sequence = 1;
  view.width = 160;
  view.height = 120;
  view.perspective = true;
  view.eye = {120, -160, 110};
  view.target = {40, 0, 10};
  view.up = {0, 0, 1};
  view.halfHeight = 70;
  renderer->setView(view);
  const auto deadline = KeptFrame::Clock::now() + std::chrono::minutes(1);
  while (!renderer->isPaused() && KeptFrame::Clock::now() < deadline && renderer->deviceError().empty()) {
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
  }
  const int samples = renderer->currentSample();
  if (!frame.requested.load() || !renderer->isPaused() || samples <= 0 || samples >= PauseFrame::kSamples) {
    std::fprintf(stderr, "render-update pause: no acknowledged partial render (requested %d, paused %d, samples %d)\n",
                 int(frame.requested.load()), int(renderer->isPaused()), samples);
    return false;
  }
  const int publications = frame.publications.load();
  // This is a quiet-period invariant after acknowledgement, not a render
  // speed requirement. Neither samples nor published frames may advance.
  std::this_thread::sleep_for(std::chrono::milliseconds(150));
  const bool frozen = renderer->isPaused() && renderer->currentSample() == samples &&
                      frame.publications.load() == publications;
  render::SceneUpdate same;
  same.bodies = scene.bodies;
  const auto changes = renderer->updateScene(same);
  const bool retained = changes.added.empty() && changes.changed.empty() && changes.moved.empty() &&
                        changes.removed.empty() && changes.meshes == 1 &&
                        renderer->isPaused() && renderer->currentSample() == samples;
  frame.resumed.store(true);
  applyPauseCommand(*renderer, {{QStringLiteral("cmd"), QStringLiteral("pause")},
                                {QStringLiteral("paused"), false}});
  const bool accumulationKept = renderer->currentSample() >= samples;
  const bool finished = frame.waitDone(view.sequence);
  const bool continued = finished && accumulationKept && frame.firstResumedSample.load() > samples &&
                         frame.samples == PauseFrame::kSamples && renderer->currentSample() == PauseFrame::kSamples &&
                         !renderer->isPaused() && !frame.pixels.empty() &&
                         !render::coverage(frame.pixels.data(), frame.width_, frame.height_).isEmpty();
  const bool ok = frozen && retained && continued;
  const int finalSamples = finished ? frame.samples : renderer->currentSample();
  std::printf("render-update pause: samples %d, publications %d frozen %d, scene retained %d, "
              "first resumed sample %d, final %d/%d%s\n", samples, publications, int(frozen), int(retained),
              frame.firstResumedSample.load(), finalSamples, PauseFrame::kSamples, ok ? "" : " (FAILED)");
  return ok;
}

// --update-test: scene updates as the application sends them, each checked
// against what it must change (docs/rendering.md, "Scene updates"), then a
// render of the last scene. Prints a line per update; 0 when all hold.
int runUpdateTest() {
  KeptFrame frame;
  std::string error;
  render::RendererOptions options; // on the CPU
  options.interactive = true;
  options.kernelFolder = kernelFolder();
  const std::unique_ptr<render::Renderer> renderer = render::makeCyclesRenderer(frame, options, error);
  if (!renderer) {
    std::fprintf(stderr, "render-failed %s\n", error.c_str());
    return 3;
  }
  renderer->setSamples(4);
  render::ViewData view;
  view.sequence = 1;
  view.width = 160;
  view.height = 120;
  view.perspective = true;
  view.eye = {120, -160, 110};
  view.target = {40, 0, 10};
  view.up = {0, 0, 1};
  view.halfHeight = 70;
  renderer->setView(view);
  const render::SceneUpdate first = render::testScene();
  const render::BodyData block = first.bodies.front();
  int failures = 0;
  // apply: an update, and what it must change ("-" for nothing).
  const auto apply = [&](const char* name, const render::SceneUpdate& update, const char* added,
                         const char* changed, const char* moved, const char* removed, int meshes, int instanced) {
    const render::SceneChanges changes = renderer->updateScene(update);
    const QString got = QStringLiteral("added %1 changed %2 moved %3 removed %4 meshes %5 instanced %6")
                            .arg(listed(changes.added), listed(changes.changed), listed(changes.moved),
                                 listed(changes.removed))
                            .arg(changes.meshes)
                            .arg(changes.instanced);
    const QString wanted = QStringLiteral("added %1 changed %2 moved %3 removed %4 meshes %5 instanced %6")
                               .arg(QLatin1String(added), QLatin1String(changed), QLatin1String(moved),
                                    QLatin1String(removed))
                               .arg(meshes)
                               .arg(instanced);
    const bool ok = got == wanted;
    failures += ok ? 0 : 1;
    std::printf("render-update %s: %s%s\n", name, qPrintable(got), ok ? "" : qPrintable(QStringLiteral(" (wanted %1)").arg(wanted)));
  };
  apply("first", first, "test block", "-", "-", "-", 1, 0);
  // The same bodies again, without their mesh: nothing changes.
  render::SceneUpdate same;
  same.bodies = {block};
  apply("same", same, "-", "-", "-", "-", 1, 0);
  // A second occurrence of the block: the same mesh, placed elsewhere.
  render::BodyData copy = block;
  copy.id = "test block@copy";
  copy.transform[3] = 80.0f;
  render::SceneUpdate twice;
  twice.bodies = {block, copy};
  apply("instance", twice, "test block@copy", "-", "-", "-", 1, 2);
  twice.bodies[1].transform[7] = 30.0f;
  apply("move", twice, "-", "-", "test block@copy", "-", 1, 2);
  // Another material: another Cycles mesh, the same mesh data.
  twice.bodies[1].material.baseColor = {0.6f, 0.1f, 0.1f};
  twice.bodies[1].materialKey = "red";
  apply("material", twice, "-", "test block@copy", "-", "-", 1, 0);
  // Hidden and shown again: no mesh is sent again.
  apply("hide", same, "-", "-", "-", "test block@copy", 1, 0);
  apply("show", twice, "test block@copy", "-", "-", "-", 1, 0);
  // Another mesh for the copy (a changed feature); the old one released
  // once no body uses it.
  render::SceneUpdate edited = render::testScene();
  edited.meshes.front().second.positions[0] += 1.0f;
  const std::string otherKey = render::meshKey(edited.meshes.front().second);
  edited.meshes.front().first = otherKey;
  edited.bodies = {block, twice.bodies[1]};
  edited.bodies[1].mesh = otherKey;
  apply("edit", edited, "-", "test block@copy", "-", "-", 2, 0);
  render::SceneUpdate released;
  released.bodies = {edited.bodies[1]};
  released.released = {block.mesh};
  apply("release", released, "-", "-", "-", "test block", 1, 0);
  // A face with a material of its own (mitcad#53): the same mesh, another
  // Cycles mesh for the body; another material of the face changes only
  // it again, and without the face's material it is as before.
  render::BodyData painted = released.bodies.front();
  render::FaceMaterial face;
  face.faces = {0};
  face.material.baseColor = {0.8f, 0.05f, 0.05f};
  face.materialKey = "face red";
  painted.faces = {face};
  released.bodies = {painted};
  released.released.clear();
  apply("face", released, "-", "test block@copy", "-", "-", 1, 0);
  released.bodies.front().faces.front().materialKey = "face blue";
  released.bodies.front().faces.front().material.baseColor = {0.05f, 0.05f, 0.8f};
  apply("face material", released, "-", "test block@copy", "-", "-", 1, 0);
  apply("face again", released, "-", "-", "-", "-", 1, 0);
  released.bodies.front().faces.clear();
  apply("face cleared", released, "-", "test block@copy", "-", "-", 1, 0);
  // The last scene renders.
  view.sequence = 2;
  renderer->setView(view);
  if (!frame.waitDone(2) || frame.pixels.empty()) {
    std::fprintf(stderr, "render-failed the last scene did not render\n");
    return 3;
  }
  // An orthographic view whose eye stands inside the block renders it as
  // one from far outside does (mitcad#215): the camera stands behind the
  // scene, nothing behind the eye is cut off.
  renderer->updateScene(render::testScene()); // the block alone again
  render::ViewData ortho;
  ortho.width = 160;
  ortho.height = 120;
  ortho.perspective = false;
  ortho.target = {0, 0, 10};
  ortho.up = {0, 0, 1};
  ortho.halfHeight = 30;
  const auto orthographicImage = [&](std::uint64_t sequence, const std::array<double, 3>& eye, QImage& image) {
    ortho.sequence = sequence;
    ortho.eye = eye;
    renderer->setView(ortho);
    if (!frame.waitDone(sequence) || frame.pixels.empty() ||
        render::coverage(frame.pixels.data(), frame.width_, frame.height_).isEmpty()) {
      return false;
    }
    image = render::displayImage(frame.pixels.data(), frame.width_, frame.height_, QColor(247, 248, 250),
                                 QColor(223, 227, 234), render::Look(), frame.catcher.empty() ? nullptr : frame.catcher.data())
                .convertToFormat(QImage::Format_RGB32);
    return true;
  };
  QImage inside;
  QImage outside;
  if (!orthographicImage(3, {0, -5, 10}, inside) || !orthographicImage(4, {0, -500, 10}, outside) ||
      inside.size() != outside.size()) {
    std::fprintf(stderr, "render-failed the orthographic views did not render\n");
    return 3;
  }
  double difference = 0.0;
  for (int y = 0; y < inside.height(); ++y) {
    const auto* a = reinterpret_cast<const QRgb*>(inside.constScanLine(y));
    const auto* b = reinterpret_cast<const QRgb*>(outside.constScanLine(y));
    for (int x = 0; x < inside.width(); ++x) {
      difference += std::abs(qRed(a[x]) - qRed(b[x])) + std::abs(qGreen(a[x]) - qGreen(b[x])) +
                    std::abs(qBlue(a[x]) - qBlue(b[x]));
    }
  }
  difference /= 3.0 * inside.width() * inside.height();
  const bool unchanged = difference < 2.0;
  failures += unchanged ? 0 : 1;
  std::printf("render-update orthographic eye inside the block: mean difference %.2f from outside%s\n", difference,
              unchanged ? "" : " (wanted below 2: the block is cut off)");
  // Views from below the ground (mitcad#320), the test scene's ground on:
  // the shadow catcher's matte would meet the camera's rays first and leave
  // only the background. The block's pixels are opaque (not the background)
  // in a perspective view from under the ground, an orthographic one
  // looking up, and above the ground again.
  const auto blockShare = [&](const char* name, const render::ViewData& below) {
    renderer->setView(below);
    if (!frame.waitDone(below.sequence) || frame.pixels.empty()) {
      std::printf("render-update %s: no frame\n", name);
      ++failures;
      return;
    }
    const QRect covered = render::coverage(frame.pixels.data(), frame.width_, frame.height_);
    const double share = covered.isEmpty() ? 0.0
                                           : static_cast<double>(covered.width()) * covered.height() /
                                                 (static_cast<double>(frame.width_) * frame.height_);
    const bool seen = share > 0.1;
    failures += seen ? 0 : 1;
    std::printf("render-update %s: the block covers %.2f of the image%s\n", name, share,
                seen ? "" : " (wanted above 0.1: only the background)");
  };
  render::ViewData under = view;
  under.sequence = 5;
  under.eye = {120, -160, -110};
  under.target = {0, 0, 10};
  under.halfHeight = 40;
  blockShare("perspective from below the ground", under);
  render::ViewData up = ortho;
  up.sequence = 6;
  up.eye = {0, 0, -500};
  up.target = {0, 0, 10};
  up.up = {0, 1, 0};
  up.halfHeight = 40;
  blockShare("orthographic looking up at the ground", up);
  render::ViewData above = view;
  above.sequence = 7;
  above.target = {0, 0, 10};
  above.halfHeight = 40;
  blockShare("perspective above the ground again", above);
  // Glass given to a body while the session renders (mitcad#319: the
  // application swaps only the material of a body it keeps) renders as
  // glass given before the render starts (a design opened with it): the
  // body's Cycles mesh and shader are made again, and the image is not
  // black. Without the ground, so that what the frame covers is the block.
  const auto glassOf = [](render::BodyData body) {
    body.material = render::MaterialData();
    body.material.baseColor = {1.0f, 1.0f, 1.0f};
    body.material.roughness = 0.0f;
    body.material.transmission = 1.0f;
    body.material.ior = 1.45f;
    body.materialKey = "glass";
    return body;
  };
  render::SceneUpdate opaque = render::testScene();
  opaque.ground = false;
  renderer->setSamples(16);
  renderer->updateScene(opaque);
  render::ViewData glassView = above;
  glassView.sequence = 8;
  renderer->setView(glassView);
  render::SceneUpdate swapped;
  swapped.bodies = {glassOf(opaque.bodies.front())};
  swapped.ground = false;
  const render::SceneChanges swap = frame.waitDone(8) ? renderer->updateScene(swapped) : render::SceneChanges();
  const bool swappedLive = swap.changed == std::vector<std::string>{"test block"} && swap.added.empty();
  failures += swappedLive ? 0 : 1;
  std::printf("render-update glass while rendering: changed %s%s\n", qPrintable(listed(swap.changed)),
              swappedLive ? "" : " (wanted test block)");
  glassView.sequence = 9;
  renderer->setView(glassView);
  int liveCovered = 0;
  double live = 0.0;
  if (frame.waitDone(9) && !frame.pixels.empty()) {
    live = render::coveredBrightness(frame.pixels.data(), frame.width_, frame.height_, &liveCovered);
  }
  KeptFrame openedFrame;
  std::string openedError;
  const std::unique_ptr<render::Renderer> opened = render::makeCyclesRenderer(openedFrame, options, openedError);
  int openedCovered = 0;
  double fromStart = 0.0;
  if (opened) {
    opened->setSamples(16);
    render::SceneUpdate glassScene = render::testScene();
    glassScene.ground = false;
    glassScene.bodies.front() = glassOf(glassScene.bodies.front());
    opened->updateScene(glassScene);
    render::ViewData openedView = glassView;
    openedView.sequence = 1;
    opened->setView(openedView);
    if (openedFrame.waitDone(1) && !openedFrame.pixels.empty()) {
      fromStart = render::coveredBrightness(openedFrame.pixels.data(), openedFrame.width_, openedFrame.height_,
                                            &openedCovered);
    }
  }
  const bool sameGlass = fromStart > 0.05 && live > 0.05 && std::abs(live - fromStart) <= 0.25 * fromStart + 0.02 &&
                         std::abs(liveCovered - openedCovered) <= openedCovered / 10 + 10;
  failures += sameGlass ? 0 : 1;
  std::printf("render-update glass given while rendering: brightness %.3f over %d pixels, given before the render "
              "%.3f over %d%s\n",
              live, liveCovered, fromStart, openedCovered,
              sameGlass ? "" : " (wanted alike and above 0.05: the live swap renders it wrong)");
  failures += runPauseUpdateTest(options) ? 0 : 1;
  std::printf("render-update %s\n", failures == 0 ? "ok" : "FAILED");
  return failures == 0 ? 0 : 1;
}

// --maps-test <folder> (mitcad#314): the maps of a texture set on the test
// block, each rendered and compared with the same material without it or
// with the parameter it replaces (docs/rendering.md, "Materials"). The
// images are generated into the folder, and so are the renders
// (maps-<name>.png). A map that matters changes the image; a map that is
// the same as the parameter it replaces (a mid-grey roughness map for
// roughness 0.502: read as data, not as sRGB; a white metalness map for
// metalness 1), a flat normal map, a white occlusion map, and a map that
// cannot be read (a warning; the rest still renders) do not; a normal map tilted
// along the image's u turned by half a turn renders as one tilted the
// other way (the tangent frame follows the texture's rotation). A base
// colour image of one sRGB colour renders as that colour as a plain base
// colour (read as sRGB once, not washed out: mitcad#386). Prints a line
// per comparison; 0 when all hold.
int runMapsTest(const QString& folderPath) {
  QDir folder(folderPath);
  if (!folder.mkpath(QStringLiteral("."))) {
    std::fprintf(stderr, "render-failed cannot make the folder %s\n", qPrintable(folderPath));
    return 3;
  }
  // The images: 64 x 64 pixels, the block's texture repeats every 20 mm.
  static constexpr int kSize = 64;
  static constexpr double kPi = 3.14159265358979323846;
  const auto write = [&folder](const QString& name, const std::function<QRgb(int, int)>& pixel) {
    QImage image(kSize, kSize, QImage::Format_RGB32);
    for (int y = 0; y < kSize; ++y) {
      for (int x = 0; x < kSize; ++x) {
        image.setPixel(x, y, pixel(x, y));
      }
    }
    const QString path = folder.absoluteFilePath(name);
    return image.save(path) ? path.toStdString() : std::string();
  };
  const auto grey = [](int level) { return [level](int, int) { return qRgb(level, level, level); }; };
  // A tangent-space normal (x along u, y along v, z out) as OpenGL's colour.
  const auto normalColor = [](double x, double y, double z) {
    const double length = std::sqrt(x * x + y * y + z * z);
    const auto channel = [length](double c) { return static_cast<int>(std::lround((c / length * 0.5 + 0.5) * 255.0)); };
    return qRgb(channel(x), channel(y), channel(z));
  };
  const std::string base = write(QStringLiteral("base.png"), grey(120));
  const std::string flat = write(QStringLiteral("normal-flat.png"), [&](int, int) { return normalColor(0, 0, 1); });
  // Ridges across u, four per image: the normal leans along u and back.
  const std::string ridges = write(QStringLiteral("normal-ridges.png"), [&](int x, int) {
    return normalColor(1.5 * std::cos(2.0 * kPi * 4.0 * (x + 0.5) / kSize), 0, 1);
  });
  const std::string tiltPlus = write(QStringLiteral("normal-tilt-plus-u.png"), [&](int, int) { return normalColor(0.8, 0, 1); });
  const std::string tiltMinus =
      write(QStringLiteral("normal-tilt-minus-u.png"), [&](int, int) { return normalColor(-0.8, 0, 1); });
  const std::string roughMid = write(QStringLiteral("roughness-128.png"), grey(128));
  const std::string roughLow = write(QStringLiteral("roughness-low.png"), grey(13));
  const std::string metal = write(QStringLiteral("metalness-1.png"), grey(255));
  const std::string white = write(QStringLiteral("occlusion-white.png"), grey(255));
  const std::string checker = write(QStringLiteral("occlusion-checker.png"), [](int x, int y) {
    const int level = ((x / 16 + y / 16) % 2 == 0) ? 255 : 40;
    return qRgb(level, level, level);
  });
  // A dark brown, far from its sRGB encoding applied twice.
  static constexpr int kBrown[3] = {84, 74, 66};
  const std::string brown =
      write(QStringLiteral("base-brown.png"), [](int, int) { return qRgb(kBrown[0], kBrown[1], kBrown[2]); });
  for (const std::string* file : {&base, &flat, &ridges, &tiltPlus, &tiltMinus, &roughMid, &roughLow, &metal, &white,
                                  &checker, &brown}) {
    if (file->empty()) {
      std::fprintf(stderr, "render-failed cannot write the test images into %s\n", qPrintable(folderPath));
      return 3;
    }
  }
  const std::string missing = folder.absoluteFilePath(QStringLiteral("no-such-normal.png")).toStdString();

  KeptFrame frame;
  std::string error;
  render::RendererOptions options; // on the CPU
  options.interactive = true;
  options.kernelFolder = kernelFolder();
  const std::unique_ptr<render::Renderer> renderer = render::makeCyclesRenderer(frame, options, error);
  if (!renderer) {
    std::fprintf(stderr, "render-failed %s\n", error.c_str());
    return 3;
  }
  renderer->setSamples(16);
  render::ViewData view;
  view.width = 160;
  view.height = 120;
  view.perspective = true;
  view.eye = {90, -120, 110};
  view.target = {0, 0, 10};
  view.up = {0, 0, 1};
  view.halfHeight = 40;
  render::SceneUpdate scene = render::testScene();
  // Without the ground: its shadow's noise would differ between shaders.
  scene.ground = false;
  render::MaterialData plain = scene.bodies.front().material;
  plain.texture = base;
  plain.textureSize = {20.0f, 20.0f};
  plain.roughness = 0.502f;
  // Renders the block in a material; the image as the view shows it.
  std::uint64_t sequence = 0;
  int failures = 0;
  const auto renderOf = [&](const char* name, const render::MaterialData& material, QImage& image,
                            std::vector<std::string>& warnings) {
    render::BodyData& body = scene.bodies.front();
    body.material = material;
    body.materialKey = QJsonDocument(render::materialJson(material)).toJson(QJsonDocument::Compact).toStdString();
    warnings = renderer->updateScene(scene).warnings;
    scene.meshes.clear(); // sent once
    view.sequence = ++sequence;
    renderer->setView(view);
    if (!frame.waitDone(sequence) || frame.pixels.empty()) {
      return false;
    }
    image = render::displayImage(frame.pixels.data(), frame.width_, frame.height_, QColor(247, 248, 250),
                                 QColor(223, 227, 234), render::Look(),
                                 frame.catcher.empty() ? nullptr : frame.catcher.data())
                .convertToFormat(QImage::Format_RGB32);
    image.save(folder.absoluteFilePath(QStringLiteral("maps-%1.png").arg(QLatin1String(name))));
    return true;
  };
  // The mean difference of the 8-bit channels.
  const auto difference = [](const QImage& a, const QImage& b) {
    if (a.size() != b.size()) {
      return 255.0;
    }
    double sum = 0.0;
    for (int y = 0; y < a.height(); ++y) {
      const auto* pa = reinterpret_cast<const QRgb*>(a.constScanLine(y));
      const auto* pb = reinterpret_cast<const QRgb*>(b.constScanLine(y));
      for (int x = 0; x < a.width(); ++x) {
        sum += std::abs(qRed(pa[x]) - qRed(pb[x])) + std::abs(qGreen(pa[x]) - qGreen(pb[x])) +
               std::abs(qBlue(pa[x]) - qBlue(pb[x]));
      }
    }
    return sum / (3.0 * a.width() * a.height());
  };
  // Below this the images are the same (noise and rounding), above
  // `kChanged` a map changed the block.
  constexpr double kSame = 0.6;
  constexpr double kChanged = 2.0;
  // The renders by name, and the warnings of the last one.
  std::map<std::string, QImage> images;
  std::vector<std::string> warnings;
  const auto renderAs = [&](const char* name, const render::MaterialData& material) {
    QImage image;
    if (!renderOf(name, material, image, warnings)) {
      std::fprintf(stderr, "render-failed %s did not render\n", name);
      return false;
    }
    for (const std::string& warning : warnings) {
      std::printf("render-maps %s: warning %s\n", name, warning.c_str());
    }
    images[name] = image;
    return true;
  };
  // Whether two renders differ (`changes`) or are the same.
  const auto compare = [&](const char* a, const char* b, bool changes) {
    const double d = difference(images[a], images[b]);
    const bool ok = changes ? d > kChanged : d < kSame;
    failures += ok ? 0 : 1;
    std::printf("render-maps %s against %s: mean difference %.2f (wanted %s %.1f)%s\n", a, b, d,
                changes ? "above" : "below", changes ? kChanged : kSame, ok ? "" : " FAILED");
  };
  render::MaterialData metallic = plain; // a polished metal shows its roughness
  metallic.metallic = 1.0f;
  render::MaterialData m = plain;
  bool rendered = renderAs("none", plain) && renderAs("metal", metallic);
  // Normal maps: a flat one changes nothing, ridges do.
  m.normalMap = flat;
  rendered = rendered && renderAs("normal-flat", m);
  m.normalMap = ridges;
  rendered = rendered && renderAs("normal-ridges", m);
  // Roughness: a mid-grey map is roughness 0.502 (read as data), a dark one
  // the same as the parameter 0.05.
  m = metallic;
  m.roughnessMap = roughMid;
  rendered = rendered && renderAs("metal-roughness-map-0.502", m);
  m.roughnessMap = roughLow;
  rendered = rendered && renderAs("metal-roughness-map-0.05", m);
  m = metallic;
  m.roughness = 13.0f / 255.0f;
  rendered = rendered && renderAs("metal-roughness-0.05", m);
  // Metalness: a white map is metal.
  m = plain;
  m.metalnessMap = metal;
  rendered = rendered && renderAs("metalness-map-1", m);
  // Occlusion: white changes nothing, a checker darkens its squares.
  m = plain;
  m.occlusionMap = white;
  rendered = rendered && renderAs("occlusion-white", m);
  m.occlusionMap = checker;
  rendered = rendered && renderAs("occlusion-checker", m);
  // A map that cannot be read: a warning, the rest renders as without it.
  m = plain;
  m.normalMap = missing;
  rendered = rendered && renderAs("normal-missing", m);
  const bool warned = warnings.size() == 1 && warnings.front().find("no-such-normal.png") != std::string::npos &&
                      warnings.front().find("the map is left out") != std::string::npos;
  // The tangent frame follows the rotation: +u turned by half a turn is -u.
  m = plain;
  m.normalMap = tiltPlus;
  rendered = rendered && renderAs("normal-tilt-plus-u", m);
  m.textureRotation = static_cast<float>(kPi);
  rendered = rendered && renderAs("normal-tilt-plus-u-turned", m);
  m.normalMap = tiltMinus;
  m.textureRotation = 0.0f;
  rendered = rendered && renderAs("normal-tilt-minus-u", m);
  // The base colour image: the same as its colour without a texture (the
  // material's colours are linear, as RenderBatch makes them).
  m = plain;
  m.texture = brown;
  rendered = rendered && renderAs("base-brown-image", m);
  m.texture.clear();
  for (int i = 0; i < 3; ++i) {
    const float srgb = static_cast<float>(kBrown[i]) / 255.0f;
    m.baseColor[static_cast<std::size_t>(i)] =
        srgb <= 0.04045f ? srgb / 12.92f : std::pow((srgb + 0.055f) / 1.055f, 2.4f);
  }
  rendered = rendered && renderAs("base-brown-colour", m);
  if (!rendered) {
    return 3;
  }
  compare("normal-flat", "none", false);
  compare("normal-ridges", "none", true);
  compare("metal", "none", true);
  compare("metal-roughness-map-0.502", "metal", false);
  compare("metal-roughness-map-0.05", "metal", true);
  compare("metal-roughness-map-0.05", "metal-roughness-0.05", false);
  compare("metalness-map-1", "none", true);
  compare("metalness-map-1", "metal", false);
  compare("occlusion-white", "none", false);
  compare("occlusion-checker", "none", true);
  compare("normal-missing", "none", false);
  failures += warned ? 0 : 1;
  std::printf("render-maps normal-missing: %s\n", warned ? "warned" : "no warning that the map is left out FAILED");
  compare("normal-tilt-plus-u", "none", true);
  compare("normal-tilt-plus-u", "normal-tilt-minus-u", true);
  compare("normal-tilt-plus-u-turned", "normal-tilt-minus-u", false);
  compare("base-brown-image", "base-brown-colour", false);
  std::printf("render-maps %s\n", failures == 0 ? "ok" : "FAILED");
  return failures == 0 ? 0 : 1;
}

// The mean linear colour of the pixels a frame covers (alpha above one
// half), the premultiplied alpha taken off, and how many those are; black
// when it covers nothing.
std::array<double, 3> coveredColor(const std::uint16_t* pixels, int width, int height, int& count) {
  std::array<double, 3> sum{0.0, 0.0, 0.0};
  count = 0;
  const std::size_t size = static_cast<std::size_t>(width) * static_cast<std::size_t>(height);
  for (std::size_t i = 0; i < size; ++i) {
    const std::uint16_t* pixel = pixels + i * 4;
    const float alpha = render::halfValue(pixel[3]);
    if (alpha > 0.5f) {
      for (std::size_t c = 0; c < 3; ++c) {
        sum[c] += render::halfValue(pixel[c]) / alpha;
      }
      ++count;
    }
  }
  for (double& channel : sum) {
    channel = count > 0 ? channel / count : 0.0;
  }
  return sum;
}

// A 64 x 64 test image, a colour per pixel, as a PNG file in the folder:
// its absolute path, or empty when it cannot be written.
constexpr int kTestImageSize = 64;
std::string writeTestImage(const QDir& folder, const QString& name, const std::function<QRgb(int, int)>& pixel) {
  QImage image(kTestImageSize, kTestImageSize, QImage::Format_RGB32);
  for (int y = 0; y < kTestImageSize; ++y) {
    for (int x = 0; x < kTestImageSize; ++x) {
      image.setPixel(x, y, pixel(x, y));
    }
  }
  const QString path = folder.absoluteFilePath(name);
  return image.save(path, "PNG") ? path.toStdString() : std::string();
}

// Two shades of a colour in squares of 16 pixels.
std::function<QRgb(int, int)> testChecker(QRgb a, QRgb b) {
  return [a, b](int x, int y) { return ((x / 16 + y / 16) % 2 == 0) ? a : b; };
}

// --write-test-images <folder> (mitcad#538): the texture set of
// tools/ui-render-materials-test.sh, so that the UI test needs no other
// tool to make it: ply.png (an orange checker, the base colour),
// ply_nor_gl.png (a flat normal map), ply_rough.png (mid grey) and
// ply_ao.png (light grey occlusion), 64 x 64 PNGs. Prints their paths; 0
// when all are written.
int writeTestImages(const QString& folderPath) {
  const QDir folder(folderPath);
  if (!QDir().mkpath(folderPath)) {
    std::fprintf(stderr, "render-failed cannot make the folder %s\n", qPrintable(folderPath));
    return 3;
  }
  const std::pair<const char*, std::function<QRgb(int, int)>> images[] = {
      {"ply.png", testChecker(qRgb(230, 130, 40), qRgb(200, 110, 30))},
      {"ply_nor_gl.png", [](int, int) { return qRgb(128, 128, 255); }},
      {"ply_rough.png", [](int, int) { return qRgb(140, 140, 140); }},
      {"ply_ao.png", [](int, int) { return qRgb(225, 225, 225); }},
  };
  for (const auto& [name, pixel] : images) {
    const std::string path = writeTestImage(folder, QLatin1String(name), pixel);
    if (path.empty()) {
      std::fprintf(stderr, "render-failed cannot write %s into %s\n", name, qPrintable(folderPath));
      return 3;
    }
    std::printf("render-test-image %s\n", path.c_str());
  }
  return 0;
}

// --swap-test <folder> (mitcad#538): the test block's material swapped in
// one renderer session as the application swaps a body's appearance while
// the view renders (Set Appearance, Edit Appearance, Undo and Redo): from
// an untextured material to a textured one (a base colour, a normal, a
// roughness and an occlusion image, as the application writes an embedded
// set for the worker), back and forth, to another textured one, to the
// textured one's parameters without its texture (Edit Appearance with
// "texture": null) and back (its undo). After every change the block's
// pixels have the material's colours, as a renderer that starts with the
// material renders them, and are not black (a shader Cycles had compiled
// while no mesh used it rendered black). Last, an image that cannot be read
// any more when its material comes back gives the warning, as at the
// start, and the block renders in the material's base colour. The images
// are generated into the folder. Prints a line per change; 0 when all hold.
int runSwapTest(const QString& folderPath, const QString& choice, bool requireMetal) {
  std::string missing;
  const render::DeviceInfo selected = render::chooseDevice(
      choice.toStdString(), render::cyclesDevices(kernelFolder()), missing);
  if (!missing.empty()) {
    std::fprintf(stderr, "%s %s\n", requireMetal ? "render-failed" : "render-skip", missing.c_str());
    return requireMetal ? 3 : 77;
  }
  if (requireMetal && selected.type != "METAL") {
    std::fprintf(stderr, "render-failed Required Metal device is unavailable for %s\n", qPrintable(choice));
    return 3;
  }
  QDir folder(folderPath);
  if (!folder.mkpath(QStringLiteral("."))) {
    std::fprintf(stderr, "render-failed cannot make the folder %s\n", qPrintable(folderPath));
    return 3;
  }
  // The images: 64 x 64 pixels; the block's textures repeat every 20 mm.
  static constexpr double kPi = 3.14159265358979323846;
  const auto write = [&folder](const QString& name, const std::function<QRgb(int, int)>& pixel) {
    return writeTestImage(folder, name, pixel);
  };
  const std::function<QRgb(int, int)> orangeChecker = testChecker(qRgb(230, 130, 40), qRgb(200, 110, 30));
  const std::string orange = write(QStringLiteral("swap-orange.png"), orangeChecker);
  const std::string green = write(QStringLiteral("swap-green.png"), testChecker(qRgb(60, 170, 70), qRgb(40, 140, 50)));
  // Gentle ridges across u (OpenGL's convention: x along u, z out).
  const std::string normal = write(QStringLiteral("swap-normal.png"), [](int x, int) {
    const double lean = 0.3 * std::cos(2.0 * kPi * 4.0 * (x + 0.5) / kTestImageSize);
    const double length = std::sqrt(lean * lean + 1.0);
    const auto channel = [length](double c) { return static_cast<int>(std::lround((c / length * 0.5 + 0.5) * 255.0)); };
    return qRgb(channel(lean), channel(0.0), channel(1.0));
  });
  const std::string roughness = write(QStringLiteral("swap-roughness.png"), [](int, int) { return qRgb(140, 140, 140); });
  const std::string occlusion = write(QStringLiteral("swap-occlusion.png"), [](int, int) { return qRgb(225, 225, 225); });
  // The image that cannot be read at the end (written again on every run).
  const std::string vanishing = write(QStringLiteral("swap-vanishing.png"), orangeChecker);
  for (const std::string* file : {&orange, &green, &normal, &roughness, &occlusion, &vanishing}) {
    if (file->empty()) {
      std::fprintf(stderr, "render-failed cannot write the test images into %s\n", qPrintable(folderPath));
      return 3;
    }
  }

  // The materials (linear colours): an untextured blue; the textured one,
  // whose own base colour is a light grey; another textured one (green);
  // the textured one without its texture; and one whose base colour image
  // is the vanishing one, blue without it.
  render::MaterialData plain;
  plain.baseColor = {0.05f, 0.12f, 0.45f};
  render::MaterialData textured;
  textured.baseColor = {0.8f, 0.8f, 0.8f};
  textured.texture = orange;
  textured.textureSize = {20.0f, 20.0f};
  textured.normalMap = normal;
  textured.roughnessMap = roughness;
  textured.occlusionMap = occlusion;
  render::MaterialData other = textured;
  other.texture = green;
  render::MaterialData bare = textured;
  bare.texture.clear();
  bare.normalMap.clear();
  bare.roughnessMap.clear();
  bare.occlusionMap.clear();
  render::MaterialData lost = plain;
  lost.texture = vanishing;
  lost.textureSize = {20.0f, 20.0f};
  render::MaterialData lostBare = plain; // what `lost` renders without its image
  // As the application keys materials: their JSON text.
  const auto keyOf = [](const render::MaterialData& material) {
    return QJsonDocument(render::materialJson(material)).toJson(QJsonDocument::Compact).toStdString();
  };

  static constexpr int kSamples = 16;
  render::RendererOptions options;
  options.device = selected;
  options.interactive = true;
  options.kernelFolder = kernelFolder();
  const auto checkDevice = [&](const render::Renderer& renderer) {
    const render::DeviceInfo actual = renderer.device();
    std::printf("render-swap-device %s %s, MetalRT %s\n", actual.type.c_str(), actual.name.c_str(),
                actual.metalRT ? "on" : "off");
    std::string failure = renderer.deviceError();
    if (failure.empty() && actual.id != selected.id) {
      failure = "Material swap did not execute on the selected device";
    }
    if (failure.empty() && requireMetal) {
      failure = render::requiredMetalError(actual, false);
    }
    if (!failure.empty()) {
      std::fprintf(stderr, "render-failed %s\n", failure.c_str());
      return false;
    }
    return true;
  };
  render::ViewData view;
  view.width = 160;
  view.height = 120;
  view.perspective = true;
  view.eye = {90, -120, 110};
  view.target = {0, 0, 10};
  view.up = {0, 0, 1};
  view.halfHeight = 40;
  // The block alone, without the ground: what a frame covers is the block.
  const auto sceneWith = [&keyOf](const render::MaterialData& material) {
    render::SceneUpdate scene = render::testScene();
    scene.ground = false;
    scene.bodies.front().material = material;
    scene.bodies.front().materialKey = keyOf(material);
    return scene;
  };

  // The block's colour in a renderer that starts with the material, as a
  // design opened with it (each in a session of its own).
  struct Expected {
    std::array<double, 3> color{0.0, 0.0, 0.0};
    int pixels = 0;
  };
  std::map<std::string, Expected> expected;
  const auto fromStart = [&](const char* name, const render::MaterialData& material) {
    KeptFrame fresh;
    std::string why;
    const std::unique_ptr<render::Renderer> opened = render::makeCyclesRenderer(fresh, options, why);
    if (!opened) {
      std::fprintf(stderr, "render-failed %s\n", why.c_str());
      return false;
    }
    opened->setSamples(kSamples);
    opened->updateScene(sceneWith(material));
    render::ViewData first = view;
    first.sequence = 1;
    opened->setView(first);
    if (!fresh.waitDone(1) || fresh.pixels.empty()) {
      std::fprintf(stderr, "render-failed %s did not render from the start\n", name);
      return false;
    }
    if (!checkDevice(*opened)) {
      return false;
    }
    Expected& entry = expected[name];
    entry.color = coveredColor(fresh.pixels.data(), fresh.width_, fresh.height_, entry.pixels);
    std::printf("render-swap %s from the start: colour %.3f %.3f %.3f over %d pixels\n", name, entry.color[0],
                entry.color[1], entry.color[2], entry.pixels);
    return true;
  };
  if (!fromStart("plain", plain) || !fromStart("textured", textured) || !fromStart("other", other) ||
      !fromStart("bare", bare) || !fromStart("lost", lost) || !fromStart("lost-bare", lostBare)) {
    return 3;
  }

  // One session: each change of the block's material, then a view (as the
  // application's next frame), and the block's colour.
  KeptFrame frame;
  std::string error;
  const std::unique_ptr<render::Renderer> renderer = render::makeCyclesRenderer(frame, options, error);
  if (!renderer) {
    std::fprintf(stderr, "render-failed %s\n", error.c_str());
    return 3;
  }
  renderer->setSamples(kSamples);
  int failures = 0;
  std::uint64_t sequence = 0;
  bool meshSent = false;
  // The hue the material's colours have: the textured one orange (red far
  // above blue), the other green, the untextured ones blue, the textured
  // one's own base colour grey.
  enum class Hue { Orange, Green, Blue, Grey };
  const auto hueHolds = [](Hue hue, const std::array<double, 3>& c) {
    switch (hue) {
    case Hue::Orange:
      return c[0] > 2.0 * c[2] && c[0] > c[1];
    case Hue::Green:
      return c[1] > 1.3 * c[0] && c[1] > 1.3 * c[2];
    case Hue::Blue:
      return c[2] > 1.5 * c[0];
    case Hue::Grey:
      return std::max({c[0], c[1], c[2]}) < 1.3 * std::min({c[0], c[1], c[2]});
    }
    return false;
  };
  // swapTo: the block in `material` (expected[name] from the start), and
  // the warnings of the update.
  const auto swapTo = [&](const char* step, const char* name, const render::MaterialData& material, Hue hue,
                        std::vector<std::string>& warnings) {
    render::SceneUpdate update = sceneWith(material);
    if (meshSent) {
      update.meshes.clear(); // the worker holds it
    }
    meshSent = true;
    const render::SceneChanges changes = renderer->updateScene(update);
    warnings = changes.warnings;
    view.sequence = ++sequence;
    renderer->setView(view);
    if (!frame.waitDone(sequence) || frame.pixels.empty()) {
      std::printf("render-swap %s: no frame FAILED\n", step);
      ++failures;
      return;
    }
    if (!checkDevice(*renderer)) {
      ++failures;
      return;
    }
    int pixels = 0;
    const std::array<double, 3> color = coveredColor(frame.pixels.data(), frame.width_, frame.height_, pixels);
    const Expected& wanted = expected[name];
    bool alike = std::abs(pixels - wanted.pixels) <= wanted.pixels / 10 + 10;
    for (std::size_t c = 0; c < 3; ++c) {
      alike = alike && std::abs(color[c] - wanted.color[c]) <= 0.15 * wanted.color[c] + 0.01;
    }
    const double brightness = (color[0] + color[1] + color[2]) / 3.0;
    const bool lit = brightness > 0.02;
    const bool ok = alike && lit && hueHolds(hue, color);
    failures += ok ? 0 : 1;
    std::printf("render-swap %s: %s, colour %.3f %.3f %.3f over %d pixels; from the start %.3f %.3f %.3f over %d%s\n",
                step, name, color[0], color[1], color[2], pixels, wanted.color[0], wanted.color[1], wanted.color[2],
                wanted.pixels,
                ok      ? ""
                : !lit  ? " FAILED (black)"
                : alike ? " FAILED (not the material's colours)"
                        : " FAILED (not as from the start)");
  };
  // Each change gives no warning, but for the image that cannot be read.
  const auto quiet = [&failures](const char* step, const std::vector<std::string>& warnings) {
    for (const std::string& warning : warnings) {
      std::printf("render-swap %s: unexpected warning %s FAILED\n", step, warning.c_str());
      ++failures;
    }
  };
  std::vector<std::string> warnings;
  swapTo("untextured", "plain", plain, Hue::Blue, warnings);
  quiet("untextured", warnings);
  swapTo("textured", "textured", textured, Hue::Orange, warnings);
  quiet("textured", warnings);
  swapTo("untextured again", "plain", plain, Hue::Blue, warnings);
  quiet("untextured again", warnings);
  swapTo("textured again", "textured", textured, Hue::Orange, warnings);
  quiet("textured again", warnings);
  swapTo("another textured", "other", other, Hue::Green, warnings);
  quiet("another textured", warnings);
  swapTo("back to the first textured", "textured", textured, Hue::Orange, warnings);
  quiet("back to the first textured", warnings);
  swapTo("texture removed", "bare", bare, Hue::Grey, warnings);
  quiet("texture removed", warnings);
  swapTo("texture removal undone", "textured", textured, Hue::Orange, warnings);
  quiet("texture removal undone", warnings);
  swapTo("untextured once more", "plain", plain, Hue::Blue, warnings);
  quiet("untextured once more", warnings);
  swapTo("an image that is there", "lost", lost, Hue::Orange, warnings);
  quiet("an image that is there", warnings);
  swapTo("untextured before the image goes", "plain", plain, Hue::Blue, warnings);
  quiet("untextured before the image goes", warnings);
  // The image can no longer be read (its file is no image now).
  QSaveFile broken(QString::fromStdString(vanishing));
  if (!broken.open(QIODevice::WriteOnly) || broken.write("not an image\n") < 0 || !broken.commit()) {
    std::fprintf(stderr, "render-failed cannot overwrite %s\n", vanishing.c_str());
    return 3;
  }
  swapTo("the image gone", "lost-bare", lost, Hue::Blue, warnings);
  const bool warned = warnings.size() == 1 && warnings.front().find("swap-vanishing.png") != std::string::npos &&
                      warnings.front().find("the base colour is shown") != std::string::npos;
  failures += warned ? 0 : 1;
  std::printf("render-swap the image gone: %s\n",
              warned ? "warned" : "no warning that the image cannot be read FAILED");
  std::printf("render-swap %s\n", failures == 0 ? "ok" : "FAILED");
  return failures == 0 ? 0 : 1;
}

// The options of --bench.
struct Bench {
  QString size;
  int samples = 64;
  std::vector<int> previews;
  bool interactive = false;
  bool orthographic = false;
  bool trace = false;
  bool requireMetal = false;
  QString frames; // a folder for every frame shown (implies trace)
  QString output;
  QString environment;
  QString texture; // --texture (mitcad#53)
  QString device;  // the device's choice (mitcad#50)
  QString compare; // a second device whose image it is compared with
  // The largest differences of the images accepted ("mean:p99"); empty: any.
  QString tolerance;
};

// One --bench render: its frame, time and device.
struct BenchRun {
  std::vector<std::uint16_t> pixels;
  std::vector<std::uint16_t> catcher;
  int width = 0;
  int height = 0;
  int samples = 0;
  double seconds = 0.0;
  render::DeviceInfo device;
  bool fellBack = false;
  std::map<QString, QImage> denoisedFrames;
};

// The test scene at the bench's size on the chosen device, all samples,
// then the time. Interactive: as the application renders (a lower
// resolution first, denoised along the way), then once more after the
// camera moved, with the time to the first frame. 0, or the exit status of
// a failure.
int benchOnce(const Bench& bench, const QString& choice, const std::vector<render::DeviceInfo>& devices,
              BenchRun& run) {
  const QString& size = bench.size;
  const int samples = bench.samples;
  const bool interactive = bench.interactive;
  const QString& environment = bench.environment;
  const QStringList parts = size.split(QLatin1Char('x'));
  KeptFrame frame;
  frame.trace = bench.trace || !bench.frames.isEmpty();
  frame.frameFolder = bench.frames;
  frame.captureDenoised = interactive && !bench.compare.isEmpty();
  std::string error;
  std::atomic<bool> recovered{false};
  const auto fellBack = [&recovered](const render::DeviceInfo& device, const std::string& message) {
    recovered.store(true);
    std::printf("render-fallback %s\n", message.c_str());
    std::printf("render-backend %s, MetalRT %s\n", device.type.c_str(), device.metalRT ? "on" : "off");
    std::fflush(stdout);
  };
  const std::unique_ptr<render::Renderer> renderer =
      makeRenderer(frame, interactive, choice, devices, fellBack, error);
  if (!renderer) {
    std::fprintf(stderr, "render-failed %s\n", error.c_str());
    return 3;
  }
  render::ViewData view;
  view.sequence = 1;
  view.width = parts.value(0).toInt();
  view.height = parts.value(1).toInt();
  if (view.width <= 0 || view.height <= 0) {
    std::fprintf(stderr, "render-failed invalid size %s\n", qPrintable(size));
    return 2;
  }
  view.perspective = !bench.orthographic;
  view.eye = {120, -160, 110};
  view.target = {0, 0, 10};
  view.up = {0, 0, 1};
  view.halfHeight = 45;
  std::printf("%s\n", renderer->description().c_str());
  const auto start = KeptFrame::Clock::now();
  renderer->setPreviews(bench.previews);
  renderer->setSamples(samples);
  if (!environment.isEmpty()) {
    QJsonParseError parsed;
    const QJsonDocument document = QJsonDocument::fromJson(environment.toUtf8(), &parsed);
    if (!document.isObject()) {
      std::fprintf(stderr, "render-failed --environment is no JSON object: %s\n", qPrintable(parsed.errorString()));
      return 2;
    }
    const std::string warning = renderer->setEnvironment(environmentOf(document.object()));
    if (!warning.empty()) {
      std::printf("render-environment %s\n", warning.c_str());
    }
  }
  // The view first, as in the application, whose scene comes when the
  // model is idle.
  renderer->setView(view);
  render::SceneUpdate scene = render::testScene();
  if (!bench.texture.isEmpty()) {
    // --texture image[,size[,box|planar]] on the block (mitcad#53).
    const QStringList fields = bench.texture.split(QLatin1Char(','));
    render::MaterialData& material = scene.bodies.front().material;
    material.texture = QFileInfo(fields.value(0)).absoluteFilePath().toStdString();
    const float repeat = fields.size() > 1 ? fields.at(1).toFloat() : 20.0f;
    material.textureSize = {repeat, repeat};
    material.texturePlanar = fields.value(2) == QLatin1String("planar");
    scene.bodies.front().materialKey =
        QJsonDocument(render::materialJson(material)).toJson(QJsonDocument::Compact).toStdString();
  }
  for (const std::string& warning : renderer->updateScene(scene).warnings) {
    std::printf("render-warning %s\n", warning.c_str());
  }
  if (!interactive) {
    renderer->wait();
    run.seconds = std::chrono::duration<double>(KeptFrame::Clock::now() - start).count();
    // The renderer counts the last samples after it delivered the image.
    std::printf("render-bench %dx%d samples %d seconds %.3f\n", frame.width_, frame.height_,
                renderer->currentSample(), run.seconds);
  } else {
    if (!frame.waitDone(1)) {
      std::fprintf(stderr, "render-failed the samples were not done in a minute\n");
      return 3;
    }
    std::printf("render-bench %dx%d samples %d interactive: first frame %.0f ms, %s, done %.3f s\n", view.width,
                view.height, samples, millisecondsBetween(start, frame.firstFrame[1]),
                qPrintable(firstDenoised(frame, 1, start)), millisecondsBetween(start, frame.done[1]) / 1000.0);
    // The camera orbits a little: the render starts again.
    view.sequence = 2;
    view.eye = {140, -140, 110};
    const auto moved = KeptFrame::Clock::now();
    frame.traceStart = moved; // the render of view 1 is done
    renderer->setView(view);
    if (!frame.waitDone(2)) {
      std::fprintf(stderr, "render-failed the samples were not done in a minute\n");
      return 3;
    }
    run.seconds = millisecondsBetween(moved, frame.done[2]) / 1000.0;
    std::printf("render-bench camera change: first frame %.0f ms, %s, done %.3f s\n",
                millisecondsBetween(moved, frame.firstFrame[2]), qPrintable(firstDenoised(frame, 2, moved)),
                millisecondsBetween(moved, frame.done[2]) / 1000.0);
    // Navigation: the camera orbits in 20 steps 40 ms apart (as the
    // application sends views while the mouse drags), then stops.
    std::vector<KeptFrame::Clock::time_point> sent;
    for (int step = 0; step < 20; ++step) {
      view.sequence = 3 + static_cast<std::uint64_t>(step);
      const double angle = 0.02 * (step + 1);
      view.eye = {140 * std::cos(angle) + 140 * std::sin(angle), -140 * std::cos(angle) + 140 * std::sin(angle), 110};
      sent.push_back(KeptFrame::Clock::now());
      renderer->setView(view);
      std::this_thread::sleep_for(std::chrono::milliseconds(40));
    }
    const std::uint64_t last = view.sequence;
    if (!frame.waitDone(last)) {
      std::fprintf(stderr, "render-failed the samples were not done in a minute\n");
      return 3;
    }
    int framed = 0;
    double latency = 0.0;
    for (std::uint64_t v = 3; v < last; ++v) {
      const auto at = frame.firstFrame.find(v);
      if (at != frame.firstFrame.end()) {
        ++framed;
        latency += millisecondsBetween(sent[static_cast<std::size_t>(v - 3)], at->second);
      }
    }
    const auto stopped = sent.back();
    std::printf("render-bench navigation: %d of 19 views framed (first frame after %.0f ms on average); "
                "after it stopped: first frame %.0f ms, %s, done %.3f s\n",
                framed, framed > 0 ? latency / framed : 0.0, millisecondsBetween(stopped, frame.firstFrame[last]),
                qPrintable(firstDenoised(frame, last, stopped)), millisecondsBetween(stopped, frame.done[last]) / 1000.0);
  }
  run.device = renderer->device();
  run.fellBack = recovered.load();
  std::printf("render-device %s %s\n", run.device.type.c_str(), run.device.name.c_str());
  std::printf("render-denoiser %s %s, executed %s\n", run.device.denoiser.type.c_str(),
              run.device.denoiser.name.c_str(), run.device.denoiser.executed ? "yes" : "no");
  if (!run.device.denoiser.message.empty()) {
    std::printf("render-denoiser-note %s\n", run.device.denoiser.message.c_str());
  }
  std::printf("render-backend %s, MetalRT %s (available %s, default %s)\n", run.device.type.c_str(),
              run.device.metalRT ? "on" : "off", run.device.metalRTAvailable ? "yes" : "no",
              run.device.metalRTDefault ? "on" : "off");
  const std::string failure = renderer->deviceError();
  if (!failure.empty()) {
    std::fprintf(stderr, "render-failed %s\n", failure.c_str());
    return 3;
  }
  run.pixels = frame.pixels;
  run.catcher = frame.catcher;
  run.width = frame.width_;
  run.height = frame.height_;
  run.samples = renderer->currentSample();
  run.denoisedFrames = std::move(frame.denoisedFrames);
  return run.pixels.empty() ? 3 : 0;
}

QImage benchImage(const BenchRun& run) {
  return render::displayImage(run.pixels.data(), run.width, run.height, QColor(247, 248, 250), QColor(223, 227, 234),
                              render::Look(), run.catcher.empty() ? nullptr : run.catcher.data());
}

// Equal-size RGB32 images: mean and 99th-percentile absolute sRGB channel error.
std::pair<double, int> imageDifference(const QImage& a, const QImage& b) {
  std::vector<int> differences;
  differences.reserve(static_cast<std::size_t>(a.width()) * a.height() * 3);
  double sum = 0.0;
  for (int y = 0; y < a.height(); ++y) {
    const auto* rowA = reinterpret_cast<const QRgb*>(a.constScanLine(y));
    const auto* rowB = reinterpret_cast<const QRgb*>(b.constScanLine(y));
    for (int x = 0; x < a.width(); ++x) {
      for (const int d : {qRed(rowA[x]) - qRed(rowB[x]), qGreen(rowA[x]) - qGreen(rowB[x]),
                          qBlue(rowA[x]) - qBlue(rowB[x])}) {
        differences.push_back(std::abs(d));
        sum += std::abs(d);
      }
    }
  }
  const auto percentile = differences.begin() + static_cast<std::ptrdiff_t>(differences.size() * 99 / 100);
  std::nth_element(differences.begin(), percentile, differences.end());
  return {sum / static_cast<double>(differences.size()), *percentile};
}

// --bench WxH [--device D] [--compare E]: the test scene's time on a device;
// with --compare also on another device, and how far apart the images are
// (their 8-bit sRGB channels: the mean and the 99th percentile of the
// differences). Exit status 77 (a skipped test) when --compare is given and
// there is no device of the --device choice.
int runBenchmark(const Bench& bench) {
  if (bench.requireMetal) {
    const QStringList limits = bench.tolerance.split(QLatin1Char(':'));
    bool meanValid = false;
    bool percentileValid = false;
    const double mean = limits.value(0).toDouble(&meanValid);
    const int percentile = limits.value(1).toInt(&percentileValid);
    if (bench.compare.isEmpty() || limits.size() != 2 || !meanValid || !percentileValid ||
        !std::isfinite(mean) || mean < 0 || percentile < 0 || percentile > 255) {
      std::fprintf(stderr, "render-failed --require-metal needs --compare and a valid --tolerance mean:p99\n");
      return 2;
    }
  }
  const std::vector<render::DeviceInfo> devices = render::cyclesDevices(kernelFolder());
  if (!bench.compare.isEmpty()) {
    std::string missing;
    const render::DeviceInfo device = render::chooseDevice(bench.device.toStdString(), devices, missing);
    if (bench.requireMetal && (!missing.empty() || device.type != "METAL")) {
      std::fprintf(stderr, "render-failed Required Metal device is unavailable for %s\n", qPrintable(bench.device));
      return 3;
    }
    if (!missing.empty() || (device.type == "CPU" && bench.device.compare(QLatin1String(render::kDeviceCpu),
                                                                          Qt::CaseInsensitive) != 0)) {
      std::printf("render-skip no %s device\n", qPrintable(bench.device.isEmpty() ? QStringLiteral("GPU") : bench.device));
      return 77;
    }
  }
  BenchRun run;
  if (const int status = benchOnce(bench, bench.device, devices, run); status != 0) {
    return status;
  }
  if (bench.requireMetal) {
    const std::string failure = render::requiredMetalError(run.device, run.fellBack);
    if (!failure.empty()) {
      std::fprintf(stderr, "render-failed %s\n", failure.c_str());
      return 3;
    }
  }
  if (!bench.output.isEmpty() && !benchImage(run).save(bench.output)) {
    std::fprintf(stderr, "render-failed cannot write %s\n", qPrintable(bench.output));
    return 3;
  }
  if (bench.compare.isEmpty()) {
    return 0;
  }
  BenchRun other;
  const QByteArray firstDenoiser = qgetenv("MITCAD_RENDER_TEST_DENOISER");
  const QByteArray comparisonDenoiser = qgetenv("MITCAD_RENDER_TEST_COMPARE_DENOISER");
  if (!comparisonDenoiser.isEmpty()) {
    qputenv("MITCAD_RENDER_TEST_DENOISER", comparisonDenoiser);
  }
  const int comparisonStatus = benchOnce(bench, bench.compare, devices, other);
  if (!comparisonDenoiser.isEmpty()) {
    if (firstDenoiser.isNull()) {
      qunsetenv("MITCAD_RENDER_TEST_DENOISER");
    } else {
      qputenv("MITCAD_RENDER_TEST_DENOISER", firstDenoiser);
    }
  }
  if (comparisonStatus != 0) {
    return comparisonStatus;
  }
  if (bench.requireMetal) {
    const std::string failure = other.device.type == "METAL" ?
        render::requiredMetalError(other.device, other.fellBack) :
        (other.fellBack ? "Required Metal comparison encountered a fallback" : "");
    if (!failure.empty()) {
      std::fprintf(stderr, "render-failed %s\n", failure.c_str());
      return 3;
    }
  }
  const QImage a = benchImage(run).convertToFormat(QImage::Format_RGB32);
  const QImage b = benchImage(other).convertToFormat(QImage::Format_RGB32);
  if (a.size() != b.size()) {
    std::fprintf(stderr, "render-failed the images differ in size\n");
    return 3;
  }
  const auto [mean, percentile] = imageDifference(a, b);
  std::printf("render-compare %s %.3f s, %s %.3f s (%.2f times), image difference mean %.2f, 99%% %d\n",
              run.device.type.c_str(), run.seconds, other.device.type.c_str(), other.seconds,
              other.seconds / std::max(run.seconds, 1e-6), mean, percentile);
  if (!bench.tolerance.isEmpty()) {
    const QStringList limits = bench.tolerance.split(QLatin1Char(':'));
    if (mean > limits.value(0).toDouble() || percentile > limits.value(1).toInt()) {
      std::fprintf(stderr, "render-failed the images differ more than %s\n", qPrintable(bench.tolerance));
      return 4;
    }
  }
  if (bench.interactive) {
    const QStringList limits = bench.tolerance.split(QLatin1Char(':'));
    int compared = 0;
    for (const auto& [key, preview] : run.denoisedFrames) {
      const auto found = other.denoisedFrames.find(key);
      if (found == other.denoisedFrames.end()) {
        continue; // Adaptive batches can reach different sample counts.
      }
      const auto [previewMean, previewPercentile] = imageDifference(preview, found->second);
      std::printf("render-compare-denoised %s image difference mean %.2f, 99%% %d\n",
                  qPrintable(key), previewMean, previewPercentile);
      if (!bench.tolerance.isEmpty() &&
          (previewMean > limits.value(0).toDouble() || previewPercentile > limits.value(1).toInt())) {
        std::fprintf(stderr, "render-failed the denoised preview images differ more than %s\n",
                     qPrintable(bench.tolerance));
        return 4;
      }
      ++compared;
    }
    if (compared == 0) {
      std::fprintf(stderr, "render-failed no matching denoised previews to compare\n");
      return 4;
    }
  }
  return 0;
}


// The final render's frames (--batch): the newest one, and when all
// samples are done.
class BatchFrames : public render::FrameSink {
public:
  void denoisingDevice(const render::DeviceInfo& device) override {
    emitEvent({{QStringLiteral("event"), QStringLiteral("denoiser")},
               {QStringLiteral("device"), deviceJson(device)}});
  }
  render::FrameBuffers beginFrame(int width, int height, int, int, bool withCatcher) override {
    m_mutex.lock();
    m_width = width;
    m_height = height;
    m_pixels.resize(static_cast<std::size_t>(width) * height * 4);
    m_catcher.resize(withCatcher ? m_pixels.size() : 0);
    return {m_pixels.data(), withCatcher ? m_catcher.data() : nullptr};
  }
  void endFrame(std::uint64_t, int samples, bool) override {
    m_samples = samples;
    m_fresh = true;
    m_mutex.unlock();
    m_changed.notify_all();
  }
  void finished(std::uint64_t, int, double) override {}

  void setDone() {
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_done = true;
    }
    m_changed.notify_all();
  }
  // Waits until a new frame comes, the render is done, or the time is up;
  // true when it is done.
  bool wait(std::chrono::milliseconds time) {
    std::unique_lock<std::mutex> lock(m_mutex);
    m_changed.wait_for(lock, time, [this] { return m_done || m_fresh; });
    return m_done;
  }
  // A copy of the newest frame if it was not taken yet (or `any`), with
  // its catcher factors (empty: none).
  bool take(std::vector<std::uint16_t>& pixels, std::vector<std::uint16_t>& catcher, int& width, int& height,
            int& samples, bool any = false) {
    const std::lock_guard<std::mutex> lock(m_mutex);
    if ((!m_fresh && !any) || m_pixels.empty()) {
      return false;
    }
    m_fresh = false;
    pixels = m_pixels;
    catcher = m_catcher;
    width = m_width;
    height = m_height;
    samples = m_samples;
    return true;
  }

private:
  std::mutex m_mutex;
  std::condition_variable m_changed;
  std::vector<std::uint16_t> m_pixels;
  std::vector<std::uint16_t> m_catcher;
  int m_width = 0;
  int m_height = 0;
  int m_samples = 0;
  bool m_fresh = false;
  bool m_done = false;
};

QColor srgbOf(const QJsonValue& value, const QColor& fallback) {
  const QJsonArray rgb = value.toArray();
  if (rgb.size() != 3) {
    return fallback;
  }
  return QColor::fromRgbF(static_cast<float>(rgb.at(0).toDouble()), static_cast<float>(rgb.at(1).toDouble()),
                          static_cast<float>(rgb.at(2).toDouble()));
}

QJsonObject batchEvent(const char* name) { return {{QStringLiteral("event"), QLatin1String(name)}}; }

// --batch job.json (mitcad#48, docs/rendering.md "Final render"): renders
// the job's scene, environment and view with its output settings to an
// image file, reporting on stdout as JSON lines: "hello", "ready",
// "progress" (samples so far, of how many, seconds, an estimate of the
// seconds left), "preview" (the image so far as a small PNG, when the job
// asks for one), "done" (the file is written) or "error". With --control
// it reads stdin: "stop" or its end cancels the render ("cancelled", exit
// status 4), so that the application can cancel it and it never outlives
// the application. Exit status 0 when the file is written.
int runBatch(const QString& jobPath, bool control) {
  emitEvent({{QStringLiteral("event"), QStringLiteral("hello")},
             {QStringLiteral("protocol"), static_cast<int>(render::kProtocolVersion)},
             {QStringLiteral("version"), QStringLiteral(MITCAD_VERSION)},
             {QStringLiteral("mode"), QStringLiteral("batch")}});
  QFile jobFile(jobPath);
  QJsonParseError parsed;
  const QJsonDocument document =
      jobFile.open(QIODevice::ReadOnly) ? QJsonDocument::fromJson(jobFile.readAll(), &parsed) : QJsonDocument();
  if (!document.isObject()) {
    emitError(QStringLiteral("cannot read the render job %1").arg(jobPath));
    return 2;
  }
  const QJsonObject job = document.object();
  const QJsonObject output = job.value(QStringLiteral("output")).toObject();
  render::OutputFile file;
  file.path = output.value(QStringLiteral("path")).toString().toStdString();
  if (file.path.empty() ||
      !render::outputFormatOf(output.value(QStringLiteral("format")).toString(QStringLiteral("png")).toStdString(),
                              file.format)) {
    emitError(QStringLiteral("the render job has no output file or an unknown format"));
    return 2;
  }
  file.quality = output.value(QStringLiteral("quality")).toInt(90);
  file.transparent = output.value(QStringLiteral("transparent")).toBool();
  const QJsonObject film = job.value(QStringLiteral("film")).toObject();
  file.look.exposure = static_cast<float>(film.value(QStringLiteral("exposure")).toDouble());
  const QString transform = film.value(QStringLiteral("view_transform")).toString();
  file.look.transform = transform == QLatin1String("filmic")    ? render::ViewTransform::Filmic
                        : transform == QLatin1String("neutral") ? render::ViewTransform::Neutral
                                                                : render::ViewTransform::Standard;
  const QJsonObject background = job.value(QStringLiteral("background")).toObject();
  file.top = srgbOf(background.value(QStringLiteral("top")), Qt::white);
  file.bottom = srgbOf(background.value(QStringLiteral("bottom")), file.top);
  render::ViewData view = viewOf(job.value(QStringLiteral("view")).toObject());
  view.sequence = 1;
  if (view.width <= 0 || view.height <= 0) {
    emitError(QStringLiteral("the render job has no image size"));
    return 2;
  }
  const int samples = std::max(1, output.value(QStringLiteral("samples")).toInt(128));
  const double timeLimit = std::max(0.0, output.value(QStringLiteral("time_limit")).toDouble());
  const QJsonObject preview = job.value(QStringLiteral("preview")).toObject();
  const QString previewPath = preview.value(QStringLiteral("path")).toString();
  const QJsonArray previewSize = preview.value(QStringLiteral("size")).toArray();
  const QSize previewBox(previewSize.at(0).toInt(480), previewSize.at(1).toInt(360));

  // Stops the render when the application asks or is gone.
  if (control) {
    std::thread([] {
      std::string line;
      while (std::getline(std::cin, line)) {
        const QJsonObject command = QJsonDocument::fromJson(QByteArray::fromStdString(line)).object();
        if (command.value(QStringLiteral("cmd")).toString() == QLatin1String("stop")) {
          break;
        }
      }
      emitEvent(batchEvent("cancelled"));
      // At once: Cycles stops a sample only when it is done, and nothing of
      // the render is kept.
      std::fflush(nullptr);
      std::_Exit(4);
    }).detach();
  }

  BatchFrames frames;
  std::string error;
  // The job's device (mitcad#50): a device that fails gives way to the CPU,
  // which the application and the command line show as a warning.
  const std::vector<render::DeviceInfo> devices = render::cyclesDevices(kernelFolder());
  const auto fellBack = [](const render::DeviceInfo& device, const std::string& message) {
    emitEvent({{QStringLiteral("event"), QStringLiteral("device")},
               {QStringLiteral("device"), deviceJson(device)},
               {QStringLiteral("message"), QString::fromStdString(message)}});
    emitEvent({{QStringLiteral("event"), QStringLiteral("warning")},
               {QStringLiteral("message"), QString::fromStdString(message)}});
  };
  const std::unique_ptr<render::Renderer> renderer = makeRenderer(
      frames, false, job.value(QStringLiteral("device")).toString(QLatin1String(render::kDeviceAutomatic)), devices,
      fellBack, error);
  if (!renderer) {
    emitError(QString::fromStdString(error));
    return 3;
  }
  emitEvent({{QStringLiteral("event"), QStringLiteral("ready")},
             {QStringLiteral("protocol"), static_cast<int>(render::kProtocolVersion)},
             {QStringLiteral("renderer"), QString::fromStdString(renderer->description())},
             {QStringLiteral("device"), deviceJson(renderer->device())},
             {QStringLiteral("devices"), devicesJson(devices)}});
  render::SceneUpdate scene;
  const QJsonObject sceneJob = job.value(QStringLiteral("scene")).toObject();
  if (sceneJob.value(QStringLiteral("test")).toBool()) {
    scene = render::testScene();
  } else if (!render::readSceneFile(sceneJob.value(QStringLiteral("path")).toString().toStdString(), scene,
                                    error)) {
    emitError(QString::fromStdString(error));
    return 1;
  }
  renderer->setSamples(samples);
  renderer->setFinal(output.value(QStringLiteral("denoise")).toBool(true), timeLimit);
  const std::string warning = renderer->setEnvironment(environmentOf(job.value(QStringLiteral("environment")).toObject()));
  if (!warning.empty()) {
    emitEvent({{QStringLiteral("event"), QStringLiteral("warning")},
               {QStringLiteral("message"), QString::fromStdString(warning)}});
  }
  for (const std::string& textureWarning : renderer->updateScene(scene).warnings) {
    emitEvent({{QStringLiteral("event"), QStringLiteral("warning")},
               {QStringLiteral("message"), QString::fromStdString(textureWarning)}});
  }
  emitEvent({{QStringLiteral("event"), QStringLiteral("status")},
             {QStringLiteral("text"), QStringLiteral("scene of %1 bodies, %2 meshes").arg(scene.bodies.size()).arg(scene.meshes.size())}});
  const auto start = KeptFrame::Clock::now();
  renderer->setView(view);
  std::thread waiter([&renderer, &frames] {
    renderer->wait();
    frames.setDone();
  });

  const auto seconds = [&start] {
    return std::chrono::duration<double>(KeptFrame::Clock::now() - start).count();
  };
  std::vector<std::uint16_t> pixels;
  std::vector<std::uint16_t> catcher;
  int width = 0;
  int height = 0;
  int frameSamples = 0;
  int previews = 0;
  // The image so far as a small PNG, replaced whole.
  const auto writePreview = [&] {
    if (previewPath.isEmpty()) {
      return;
    }
    const QImage image =
        render::displayImage(pixels.data(), width, height, file.top, file.bottom, file.look,
                             catcher.empty() ? nullptr : catcher.data())
            .scaled(previewBox, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    QSaveFile out(previewPath);
    if (!out.open(QIODevice::WriteOnly) || !image.save(&out, "PNG") || !out.commit()) {
      return;
    }
    emitEvent({{QStringLiteral("event"), QStringLiteral("preview")},
               {QStringLiteral("path"), previewPath},
               {QStringLiteral("number"), ++previews},
               {QStringLiteral("samples"), frameSamples}});
  };
  int reported = -1;
  auto lastReport = KeptFrame::Clock::now() - std::chrono::seconds(1);
  for (bool done = false; !done;) {
    done = frames.wait(std::chrono::milliseconds(250));
    const int sample = std::min(renderer->currentSample(), samples);
    const auto now = KeptFrame::Clock::now();
    if (!done && (sample != reported || now - lastReport >= std::chrono::seconds(1))) {
      reported = sample;
      lastReport = now;
      const double elapsed = seconds();
      double remaining = sample > 0 ? elapsed / sample * (samples - sample) : -1.0;
      if (timeLimit > 0.0) {
        remaining = remaining < 0.0 ? timeLimit - elapsed : std::min(remaining, timeLimit - elapsed);
      }
      emitEvent({{QStringLiteral("event"), QStringLiteral("progress")},
                 {QStringLiteral("samples"), sample},
                 {QStringLiteral("total"), samples},
                 {QStringLiteral("seconds"), elapsed},
                 {QStringLiteral("remaining"), remaining < 0.0 ? QJsonValue() : QJsonValue(std::max(0.0, remaining))}});
    }
    if (!done && frames.take(pixels, catcher, width, height, frameSamples)) {
      writePreview();
    }
  }
  waiter.join();
  const double elapsed = seconds();
  if (const std::string failure = renderer->deviceError(); !failure.empty()) {
    emitError(QStringLiteral("the render failed: %1").arg(QString::fromStdString(failure)));
    return 1;
  }
  if (!frames.take(pixels, catcher, width, height, frameSamples, true)) {
    emitError(QStringLiteral("the renderer gave no image"));
    return 1;
  }
  // The renderer counts a frame's last samples after it delivered it.
  frameSamples = std::max(frameSamples, renderer->currentSample());
  if (!render::writeOutputFile(pixels.data(), catcher.empty() ? nullptr : catcher.data(), width, height, file,
                               error)) {
    emitError(QStringLiteral("cannot write %1: %2")
                  .arg(QString::fromStdString(file.path), QString::fromStdString(error)));
    return 1;
  }
  writePreview();
  emitEvent({{QStringLiteral("event"), QStringLiteral("done")},
             {QStringLiteral("path"), QString::fromStdString(file.path)},
             {QStringLiteral("size"), QJsonArray{width, height}},
             {QStringLiteral("samples"), frameSamples},
             {QStringLiteral("seconds"), elapsed},
             {QStringLiteral("device"), deviceJson(renderer->device())}});
  return 0;
}

} // namespace

#ifdef MITCAD_RENDER_XPC_ENTRY
namespace render {
QJsonObject renderDeviceJson(const DeviceInfo& device) { return deviceJson(device); }
struct RenderSession::Impl {
  FrameSink& sink;
  Event event;
  QJsonObject configuration;
  std::unique_ptr<Renderer> renderer;
  Impl(FrameSink& target, Event output, QJsonObject config) : sink(target), event(std::move(output)), configuration(std::move(config)) {}
  void error(const QString& text) { event({{QStringLiteral("event"), QStringLiteral("error")},
                                          {QStringLiteral("message"), text}}); }
};
RenderSession::RenderSession(FrameSink& sink, Event event, const QJsonObject& configuration)
    : m_impl(std::make_unique<Impl>(sink, std::move(event), configuration)) {}
RenderSession::~RenderSession() = default;
void RenderSession::hello() {
  m_impl->event({{QStringLiteral("event"), QStringLiteral("hello")},
                 {QStringLiteral("protocol"), static_cast<int>(kProtocolVersion)},
                 {QStringLiteral("version"), QStringLiteral(MITCAD_VERSION)},
                 {QStringLiteral("transport"), QStringLiteral("iosurface")}});
}
bool RenderSession::command(const QJsonObject& command) {
  auto& state = *m_impl;
  const QString name = command.value(QStringLiteral("cmd")).toString();
  if (name == QLatin1String("stop")) { state.renderer.reset(); return false; }
  std::string error;
  if (!state.renderer) {
    if (name != QLatin1String("device")) { state.error(QStringLiteral("the first command must choose a device")); return false; }
    const auto devices = cyclesDevices(kernelFolder());
    const auto output = state.event;
    const auto fallback = [output](const DeviceInfo& device, const std::string& reason) {
      output({{QStringLiteral("event"), QStringLiteral("device")},
               {QStringLiteral("device"), deviceJson(device)},
               {QStringLiteral("message"), QString::fromStdString(reason)}});
    };
    state.renderer = makeRenderer(state.sink, true, command.value(QStringLiteral("id")).toString(QLatin1String(kDeviceAutomatic)),
                                  devices, fallback, error, &state.configuration);
    if (!state.renderer) { state.error(QString::fromStdString(error)); return false; }
    state.event({{QStringLiteral("event"), QStringLiteral("ready")},
                  {QStringLiteral("protocol"), static_cast<int>(kProtocolVersion)},
                  {QStringLiteral("renderer"), QString::fromStdString(state.renderer->description())},
                  {QStringLiteral("device"), deviceJson(state.renderer->device())},
                  {QStringLiteral("devices"), devicesJson(devices)}});
  } else if (name == QLatin1String("scene")) {
    const auto started = KeptFrame::Clock::now();
    SceneUpdate update;
    if (command.value(QStringLiteral("test")).toBool()) {
      update = testScene();
      update.ground = command.value(QStringLiteral("ground")).toBool(true);
    } else if (!sceneUpdateOf(command, update, error)) { state.error(QString::fromStdString(error)); return true; }
    const auto changes = state.renderer->updateScene(update);
    state.event(sceneEvent(command.value(QStringLiteral("sequence")).toInt(), update, changes,
                           millisecondsBetween(started, KeptFrame::Clock::now())));
    for (const auto& warning : changes.warnings) state.event({{QStringLiteral("event"), QStringLiteral("warning")},
                                                            {QStringLiteral("message"), QString::fromStdString(warning)}});
  } else if (name == QLatin1String("environment")) {
    const auto warning = state.renderer->setEnvironment(environmentOf(command));
    if (!warning.empty()) state.event({{QStringLiteral("event"), QStringLiteral("warning")},
                                       {QStringLiteral("message"), QString::fromStdString(warning)}});
  } else if (name == QLatin1String("pause")) applyPauseCommand(*state.renderer, command);
  else if (name == QLatin1String("view")) state.renderer->setView(viewOf(command));
  else if (name == QLatin1String("samples")) {
    std::vector<int> previews;
    for (const auto& at : command.value(QStringLiteral("previews")).toArray()) previews.push_back(at.toInt());
    state.renderer->setPreviews(previews);
    state.renderer->setSamples(command.value(QStringLiteral("count")).toInt());
  } else state.error(QStringLiteral("unknown command %1").arg(name));
  // Explicit regression instrumentation records applied mutation order on the
  // same service stream as scene acknowledgements. Production emits no trace.
  if (state.configuration.value(QStringLiteral("command_trace")).toBool())
    state.event({{QStringLiteral("event"), QStringLiteral("command_applied")},
                 {QStringLiteral("cmd"), name}, {QStringLiteral("sequence"), command.value(QStringLiteral("sequence"))}});
  return true;
}
} // namespace render
#endif

} // namespace mitcad

#ifdef MITCAD_RENDER_RUNTIME_ENTRY
extern "C" int mitcad_render_runtime_main(int argc, char* argv[]) {
#else
int main(int argc, char* argv[]) {
#endif
  using namespace mitcad;
  // Crash reports (mitcad#62): into the application's folder
  // (MITCAD_CRASH_DIR), which it offers then.
  crash::install("render-worker", MITCAD_VERSION);
  if (crash::testCrashRequested("render-worker")) {
    crash::crashNow();
  }
  QCoreApplication app(argc, argv);
  QCoreApplication::setApplicationName(QStringLiteral("mitcad-render"));
  QCoreApplication::setApplicationVersion(QStringLiteral(MITCAD_VERSION));
  QCommandLineParser parser;
  parser.setApplicationDescription(
      QStringLiteral("MitCad's render worker (protocol %1): MitCad starts it for View > Rendered.")
          .arg(render::kProtocolVersion));
  parser.addHelpOption();
  parser.addVersionOption();
  const QCommandLineOption channel(QStringLiteral("frame-channel"),
                                   QStringLiteral("The socket the frame memory comes through (POSIX)"),
                                   QStringLiteral("descriptor"));
  const QCommandLineOption bench(QStringLiteral("bench"),
                                 QStringLiteral("Render the test scene at <width>x<height> and print the time"),
                                 QStringLiteral("size"));
  const QCommandLineOption samples(QStringLiteral("samples"), QStringLiteral("Samples per pixel for --bench"),
                                   QStringLiteral("count"), QStringLiteral("64"));
  const QCommandLineOption output(QStringLiteral("output"), QStringLiteral("PNG file of --bench's image"),
                                  QStringLiteral("file"));
  const QCommandLineOption interactive(QStringLiteral("interactive"),
                                      QStringLiteral("--bench as the application renders, and after a camera change"));
  const QCommandLineOption orthographic(QStringLiteral("orthographic"), QStringLiteral("--bench with an orthographic camera"));
  const QCommandLineOption trace(QStringLiteral("trace"), QStringLiteral("--bench prints every frame"));
  const QCommandLineOption frameFiles(QStringLiteral("frames"),
                                  QStringLiteral("--bench prints every frame and saves it as PNG in this folder"),
                                  QStringLiteral("folder"));
  const QCommandLineOption previews(QStringLiteral("previews"),
                                    QStringLiteral("--bench --interactive: denoised previews at these sample counts"),
                                    QStringLiteral("n,m,..."));
  const QCommandLineOption updateTest(QStringLiteral("update-test"),
                                      QStringLiteral("Check that scene updates change only what changed"));
  const QCommandLineOption mapsTest(
      QStringLiteral("maps-test"),
      QStringLiteral("Check that a texture's maps render as they should, with test images in this folder"),
      QStringLiteral("folder"));
  const QCommandLineOption swapTest(
      QStringLiteral("swap-test"),
      QStringLiteral("Check that a body whose material changes while it renders, also to and from a texture, "
                     "renders in it, with test images in this folder"),
      QStringLiteral("folder"));
  const QCommandLineOption writeImages(
      QStringLiteral("write-test-images"),
      QStringLiteral("Write the UI tests' texture set (64 x 64 PNGs) into this folder and end"),
      QStringLiteral("folder"));
  const QCommandLineOption environment(
      QStringLiteral("environment"),
      QStringLiteral("--bench in this environment: the \"environment\" command's JSON (render settings)"),
      QStringLiteral("json"));
  const QCommandLineOption texture(
      QStringLiteral("texture"),
      QStringLiteral("--bench with this image on the block: <image>[,<size mm>[,box|planar]]"),
      QStringLiteral("image"));
  const QCommandLineOption batch(
      QStringLiteral("batch"),
      QStringLiteral("Render the job (JSON: scene, environment, view, output settings) to an image file"),
      QStringLiteral("job"));
  const QCommandLineOption control(QStringLiteral("control"),
                                   QStringLiteral("--batch reads stdin: \"stop\" or its end cancels the render"));
  const QCommandLineOption device(
      QStringLiteral("device"),
      QStringLiteral("--bench/--swap-test on this device: auto, cpu, a device's id or type"),
      QStringLiteral("device"), QLatin1String(render::kDeviceAutomatic));
  const QCommandLineOption compare(
      QStringLiteral("compare"),
      QStringLiteral("--bench on this device too, comparing the images and times (exit status 77 without "
                     "a device of --device)"),
      QStringLiteral("device"));
  const QCommandLineOption tolerance(
      QStringLiteral("tolerance"),
      QStringLiteral("--compare fails (exit status 4) when the images differ more: the mean and the 99th "
                     "percentile of the 8-bit channels' differences, as mean:p99"),
      QStringLiteral("limits"));
  const QCommandLineOption requireMetal(
      QStringLiteral("require-metal"),
      QStringLiteral("Require Metal and automatic MetalRT policy for --bench comparisons or --swap-test"));
  const QCommandLineOption listDevices(QStringLiteral("list-devices"),
                                       QStringLiteral("Print the render devices as JSON and end"));
  const QCommandLineOption probeDenoiser(
      QStringLiteral("probe-denoiser"),
      QStringLiteral("Print whether Open Image Denoise creates its device on each GPU, or its error, and end"));
  parser.addOptions({channel, bench, samples, output, interactive, orthographic, environment, texture, trace, frameFiles, previews,
                     updateTest, batch, control, device, compare, tolerance, requireMetal, listDevices, probeDenoiser, mapsTest,
                     swapTest, writeImages});
  parser.process(app);
  const QByteArray required = qgetenv("MITCAD_RENDER_REQUIRE_METAL");
  const bool requireMetalExecution = parser.isSet(requireMetal) || (!required.isEmpty() && required != "0");
  if (parser.isSet(requireMetal) && !parser.isSet(bench) && !parser.isSet(swapTest)) {
    std::fprintf(stderr, "render-failed --require-metal needs --bench or --swap-test\n");
    return 2;
  }
  if (parser.isSet(probeDenoiser)) {
    // Diagnostics of GPU denoising (mitcad#202, docs/rendering.md "Devices").
    for (const std::string& line : render::probeDenoisers(kernelFolder())) {
      std::printf("render-denoiser-probe %s\n", line.c_str());
    }
    return 0;
  }
  if (parser.isSet(listDevices)) {
    // For Preferences and mitcad-cli render --list-devices (mitcad#50).
    const QJsonObject list{{QStringLiteral("protocol"), static_cast<int>(render::kProtocolVersion)},
                           {QStringLiteral("devices"), devicesJson(render::cyclesDevices(kernelFolder()))}};
    const QByteArray line = QJsonDocument(list).toJson(QJsonDocument::Compact) + '\n';
    std::fwrite(line.constData(), 1, static_cast<std::size_t>(line.size()), stdout);
    return 0;
  }
  if (parser.isSet(writeImages)) {
    return writeTestImages(parser.value(writeImages));
  }
  // This windowless process must remain active while its renderer runs.
  // The guard precedes all rendering modes and is destroyed after their
  // sessions, also on a device failure, stdin ending or a cancelled job.
  mac::setThreadQuality(mac::ThreadQuality::UserInitiated);
  const mac::ProcessActivity activity("Rendering");
  if (parser.isSet(updateTest)) {
    return runUpdateTest();
  }
  if (parser.isSet(mapsTest)) {
    return runMapsTest(parser.value(mapsTest));
  }
  if (parser.isSet(swapTest)) {
    return runSwapTest(parser.value(swapTest), parser.isSet(device) ? parser.value(device) : QStringLiteral("cpu"),
                       requireMetalExecution);
  }
  if (parser.isSet(batch)) {
    return runBatch(parser.value(batch), parser.isSet(control));
  }
  if (parser.isSet(bench)) {
    Bench options;
    options.size = parser.value(bench);
    options.samples = parser.value(samples).toInt();
    for (const QString& at : parser.value(previews).split(QLatin1Char(','), Qt::SkipEmptyParts)) {
      options.previews.push_back(at.toInt());
    }
    options.interactive = parser.isSet(interactive);
    options.orthographic = parser.isSet(orthographic);
    options.trace = parser.isSet(trace);
    options.frames = parser.value(frameFiles);
    options.output = parser.value(output);
    options.environment = parser.value(environment);
    options.texture = parser.value(texture);
    options.device = parser.value(device);
    options.compare = parser.value(compare);
    options.tolerance = parser.value(tolerance);
    options.requireMetal = requireMetalExecution;
    return runBenchmark(options);
  }

  // Before anything else, so that an application of another version stops
  // here, even when the renderer cannot start.
  emitEvent({{QStringLiteral("event"), QStringLiteral("hello")},
             {QStringLiteral("protocol"), static_cast<int>(render::kProtocolVersion)},
             {QStringLiteral("version"), QStringLiteral(MITCAD_VERSION)}});
  SharedFrames frames;
#ifndef _WIN32
  bool channelOk = false;
  const int channelDescriptor = parser.value(channel).toInt(&channelOk);
  if (!channelOk || channelDescriptor < 0) {
    emitError(QStringLiteral("no --frame-channel: mitcad-render runs as MitCad's render worker"));
    return 2;
  }
  frames.setChannel(channelDescriptor);
#endif
  std::string error;
  // The device (mitcad#50): the first command chooses it ("device", as the
  // application sends it once it has read "hello"); without one it is
  // "auto". The renderer starts then, and "ready" names its device and the
  // others. When the device fails, the CPU takes over and "device" says
  // why.
  std::string line;
  bool pending = static_cast<bool>(std::getline(std::cin, line));
  QJsonObject first = QJsonDocument::fromJson(QByteArray::fromStdString(line)).object();
  QString choice = QLatin1String(render::kDeviceAutomatic);
  if (pending && first.value(QStringLiteral("cmd")).toString() == QLatin1String("device")) {
    choice = first.value(QStringLiteral("id")).toString(choice);
    pending = false;
  }
  if (!pending && first.isEmpty()) {
    return 0; // stdin ended: the application is gone
  }
  const std::vector<render::DeviceInfo> devices = render::cyclesDevices(kernelFolder());
  const auto fellBack = [](const render::DeviceInfo& device, const std::string& message) {
    emitEvent({{QStringLiteral("event"), QStringLiteral("device")},
               {QStringLiteral("device"), deviceJson(device)},
               {QStringLiteral("message"), QString::fromStdString(message)}});
  };
  const std::unique_ptr<render::Renderer> renderer = makeRenderer(frames, true, choice, devices, fellBack, error);
  if (!renderer) {
    emitError(QString::fromStdString(error));
    return 3;
  }
  emitEvent({{QStringLiteral("event"), QStringLiteral("ready")},
             {QStringLiteral("protocol"), static_cast<int>(render::kProtocolVersion)},
             {QStringLiteral("renderer"), QString::fromStdString(renderer->description())},
             {QStringLiteral("device"), deviceJson(renderer->device())},
             {QStringLiteral("devices"), devicesJson(devices)}});
  // Commands, a line each, until "stop" or the end of stdin (the
  // application is gone).
  while (pending || std::getline(std::cin, line)) {
    pending = false;
    const QJsonObject command = QJsonDocument::fromJson(QByteArray::fromStdString(line)).object();
    const QString name = command.value(QStringLiteral("cmd")).toString();
    if (name == QLatin1String("stop")) {
      break;
    }
    if (name == QLatin1String("device")) {
      emitError(QStringLiteral("the device is chosen by the first command only"));
      continue;
    }
    if (name == QLatin1String("memory")) {
      QString why;
      if (!frames.attach(command, why)) {
        emitError(QStringLiteral("cannot attach the frame memory: %1").arg(why));
      }
    } else if (name == QLatin1String("scene")) {
      const auto started = KeptFrame::Clock::now();
      render::SceneUpdate update;
      if (command.value(QStringLiteral("test")).toBool()) {
        update = render::testScene();
        update.ground = command.value(QStringLiteral("ground")).toBool(true);
      } else if (!render::sceneUpdateOf(command, update, error)) {
        emitError(QString::fromStdString(error));
        continue;
      }
      const render::SceneChanges changes = renderer->updateScene(update);
      emitSceneEvent(command.value(QStringLiteral("sequence")).toInt(), update, changes,
                     millisecondsBetween(started, KeptFrame::Clock::now()));
      // A texture image that cannot be read (mitcad#53).
      for (const std::string& warning : changes.warnings) {
        emitEvent({{QStringLiteral("event"), QStringLiteral("warning")},
                   {QStringLiteral("message"), QString::fromStdString(warning)}});
      }
    } else if (name == QLatin1String("environment")) {
      // The render settings' environment, background and ground (mitcad#47).
      const std::string warning = renderer->setEnvironment(environmentOf(command));
      if (!warning.empty()) {
        emitEvent({{QStringLiteral("event"), QStringLiteral("warning")},
                   {QStringLiteral("message"), QString::fromStdString(warning)}});
      }
      emitEvent({{QStringLiteral("event"), QStringLiteral("status")},
                 {QStringLiteral("text"),
                  QStringLiteral("environment %1, %2 lights")
                      .arg(command.value(QStringLiteral("environment"))
                               .toObject()
                               .value(QStringLiteral("preset"))
                               .toString())
                      .arg(command.value(QStringLiteral("lights")).toArray().size())}});
    } else if (name == QLatin1String("pause")) {
      applyPauseCommand(*renderer, command);
    } else if (name == QLatin1String("view")) {
      renderer->setView(viewOf(command));
    } else if (name == QLatin1String("samples")) {
      std::vector<int> sampleCounts;
      for (const QJsonValue& at : command.value(QStringLiteral("previews")).toArray()) {
        sampleCounts.push_back(at.toInt());
      }
      renderer->setPreviews(sampleCounts);
      renderer->setSamples(command.value(QStringLiteral("count")).toInt());
    } else if (!name.isEmpty()) {
      emitError(QStringLiteral("unknown command %1").arg(name));
    }
  }
  return 0;
}
