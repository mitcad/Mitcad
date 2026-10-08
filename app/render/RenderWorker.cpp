// SPDX-License-Identifier: MIT
// mitcad-render, the render worker (docs/rendering.md): an executable of
// its own without windows that renders what the application asks on stdin
// into shared memory (RenderProtocol.hpp), so that a crash or an
// out-of-memory in the renderer ends only it, and so that only it loads
// the renderer's libraries. The application starts it for View > Rendered,
// and with --batch for a final render to an image file (File > Render
// Image, mitcad-cli render; mitcad#48).

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
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

#include "render/FrameMemory.hpp"
#include "render/RenderDevices.hpp"
#include "render/RenderImage.hpp"
#include "render/RenderOutput.hpp"
#include "render/RenderProtocol.hpp"
#include "render/RenderScene.hpp"
#include "render/Renderer.hpp"
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

// Where Cycles finds its GPU kernels (<folder>/lib/kernel_*.cubin.zst):
// lib/mitcad/cycles beside the executable's bin folder in a package, else
// cycles next to the executable (a build folder).
std::string kernelFolder() {
  const QDir executable(QCoreApplication::applicationDirPath());
  for (const QString& candidate : {QStringLiteral("../lib/mitcad/cycles"), QStringLiteral("cycles")}) {
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
          {QStringLiteral("denoiser"), device.denoisesOnDevice ? QStringLiteral("device") : QStringLiteral("cpu")}};
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
                                               const render::DeviceReport& fellBack, std::string& error) {
  std::string message;
  const render::DeviceInfo device = render::chooseDevice(choice.toStdString(), devices, message);
  if (!message.empty()) {
    fellBack(device, message);
  }
  const std::string folder = kernelFolder();
  const auto factory = [interactive, folder](render::FrameSink& target, const render::DeviceInfo& on,
                                             std::string& why) {
    render::RendererOptions options;
    options.interactive = interactive;
    options.device = on;
    options.kernelFolder = folder;
    return render::makeCyclesRenderer(target, options, why);
  };
  return render::makeDeviceRenderer(sink, device, factory, fellBack,
                                    qEnvironmentVariable("MITCAD_RENDER_TEST_DEVICE_FAILURE").toStdString(), error);
}

// Frames into the application's shared memory (RenderProtocol.hpp).
class SharedFrames : public render::FrameSink {
public:
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
void emitSceneEvent(int sequence, const render::SceneUpdate& update, const render::SceneChanges& changes,
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
  emitEvent({{QStringLiteral("event"), QStringLiteral("scene")},
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
             {QStringLiteral("milliseconds"), milliseconds}});
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
// settings as the model has them (core/model/src/api/commands.md, "Render
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
  std::printf("render-update %s\n", failures == 0 ? "ok" : "FAILED");
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
  std::string error;
  const auto fellBack = [](const render::DeviceInfo&, const std::string& message) {
    std::printf("render-fallback %s\n", message.c_str());
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
    const QStringList parts = bench.texture.split(QLatin1Char(','));
    render::MaterialData& material = scene.bodies.front().material;
    material.texture = QFileInfo(parts.value(0)).absoluteFilePath().toStdString();
    const float repeat = parts.size() > 1 ? parts.at(1).toFloat() : 20.0f;
    material.textureSize = {repeat, repeat};
    material.texturePlanar = parts.value(2) == QLatin1String("planar");
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
  std::printf("render-device %s %s, denoising on the %s\n", run.device.type.c_str(), run.device.name.c_str(),
              run.device.denoisesOnDevice ? "device" : "CPU");
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
  return run.pixels.empty() ? 3 : 0;
}

QImage benchImage(const BenchRun& run) {
  return render::displayImage(run.pixels.data(), run.width, run.height, QColor(247, 248, 250), QColor(223, 227, 234),
                              render::Look(), run.catcher.empty() ? nullptr : run.catcher.data());
}

// --bench WxH [--device D] [--compare E]: the test scene's time on a device;
// with --compare also on another device, and how far apart the images are
// (their 8-bit sRGB channels: the mean and the 99th percentile of the
// differences). Exit status 77 (a skipped test) when --compare is given and
// there is no device of the --device choice.
int runBenchmark(const Bench& bench) {
  const std::vector<render::DeviceInfo> devices = render::cyclesDevices(kernelFolder());
  if (!bench.compare.isEmpty()) {
    std::string missing;
    const render::DeviceInfo device = render::chooseDevice(bench.device.toStdString(), devices, missing);
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
  if (!bench.output.isEmpty() && !benchImage(run).save(bench.output)) {
    std::fprintf(stderr, "render-failed cannot write %s\n", qPrintable(bench.output));
    return 3;
  }
  if (bench.compare.isEmpty()) {
    return 0;
  }
  BenchRun other;
  if (const int status = benchOnce(bench, bench.compare, devices, other); status != 0) {
    return status;
  }
  const QImage a = benchImage(run).convertToFormat(QImage::Format_RGB32);
  const QImage b = benchImage(other).convertToFormat(QImage::Format_RGB32);
  if (a.size() != b.size()) {
    std::fprintf(stderr, "render-failed the images differ in size\n");
    return 3;
  }
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
  const double mean = sum / static_cast<double>(differences.size());
  std::printf("render-compare %s %.3f s, %s %.3f s (%.2f times), image difference mean %.2f, 99%% %d\n",
              run.device.type.c_str(), run.seconds, other.device.type.c_str(), other.seconds,
              other.seconds / std::max(run.seconds, 1e-6), mean, *percentile);
  if (!bench.tolerance.isEmpty()) {
    const QStringList limits = bench.tolerance.split(QLatin1Char(':'));
    if (mean > limits.value(0).toDouble() || *percentile > limits.value(1).toInt()) {
      std::fprintf(stderr, "render-failed the images differ more than %s\n", qPrintable(bench.tolerance));
      return 4;
    }
  }
  return 0;
}


// The final render's frames (--batch): the newest one, and when all
// samples are done.
class BatchFrames : public render::FrameSink {
public:
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

} // namespace mitcad

int main(int argc, char* argv[]) {
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
      QStringLiteral("Mitcad's render worker (protocol %1): Mitcad starts it for View > Rendered.")
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
      QStringLiteral("--bench on this device: auto (the best GPU, else the CPU), cpu, a device's id or type"),
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
  const QCommandLineOption listDevices(QStringLiteral("list-devices"),
                                       QStringLiteral("Print the render devices as JSON and end"));
  parser.addOptions({channel, bench, samples, output, interactive, orthographic, environment, texture, trace, frameFiles, previews,
                     updateTest, batch, control, device, compare, tolerance, listDevices});
  parser.process(app);
  if (parser.isSet(listDevices)) {
    // For Preferences and mitcad-cli render --list-devices (mitcad#50).
    const QJsonObject list{{QStringLiteral("protocol"), static_cast<int>(render::kProtocolVersion)},
                           {QStringLiteral("devices"), devicesJson(render::cyclesDevices(kernelFolder()))}};
    const QByteArray line = QJsonDocument(list).toJson(QJsonDocument::Compact) + '\n';
    std::fwrite(line.constData(), 1, static_cast<std::size_t>(line.size()), stdout);
    return 0;
  }
  if (parser.isSet(updateTest)) {
    return runUpdateTest();
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
    emitError(QStringLiteral("no --frame-channel: mitcad-render runs as Mitcad's render worker"));
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
        emitError(QString::fromStdString(warning));
      }
    } else if (name == QLatin1String("environment")) {
      // The render settings' environment, background and ground (mitcad#47).
      const std::string warning = renderer->setEnvironment(environmentOf(command));
      if (!warning.empty()) {
        emitError(QString::fromStdString(warning));
      }
      emitEvent({{QStringLiteral("event"), QStringLiteral("status")},
                 {QStringLiteral("text"),
                  QStringLiteral("environment %1, %2 lights")
                      .arg(command.value(QStringLiteral("environment"))
                               .toObject()
                               .value(QStringLiteral("preset"))
                               .toString())
                      .arg(command.value(QStringLiteral("lights")).toArray().size())}});
    } else if (name == QLatin1String("view")) {
      renderer->setView(viewOf(command));
    } else if (name == QLatin1String("samples")) {
      std::vector<int> previews;
      for (const QJsonValue& at : command.value(QStringLiteral("previews")).toArray()) {
        previews.push_back(at.toInt());
      }
      renderer->setPreviews(previews);
      renderer->setSamples(command.value(QStringLiteral("count")).toInt());
    } else if (!name.isEmpty()) {
      emitError(QStringLiteral("unknown command %1").arg(name));
    }
  }
  return 0;
}
