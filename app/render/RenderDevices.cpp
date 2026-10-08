// SPDX-License-Identifier: MIT
#include "render/RenderDevices.hpp"

#include <algorithm>
#include <atomic>
#include <cctype>
#include <chrono>
#include <condition_variable>
#include <map>
#include <mutex>
#include <optional>
#include <thread>
#include <utility>

namespace mitcad::render {

namespace {

std::string lowercase(std::string text) {
  std::transform(text.begin(), text.end(), text.begin(),
                 [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
  return text;
}

DeviceInfo cpuOf(const std::vector<DeviceInfo>& devices) {
  for (const DeviceInfo& device : devices) {
    if (device.type == "CPU") {
      return device;
    }
  }
  DeviceInfo cpu;
  cpu.type = "CPU";
  cpu.name = "CPU";
  return cpu;
}

std::string nameOf(const DeviceInfo& device) {
  if (device.name.empty()) {
    return device.type;
  }
  return device.type == "CPU" ? device.name : device.name + " (" + device.type + ")";
}

// The renderer's sink as one renderer sees it: once that renderer is
// replaced, its frames no longer reach the sink.
class DeviceSink : public FrameSink {
public:
  DeviceSink(FrameSink& sink, std::function<void()> firstFrame) : m_sink(sink), m_firstFrame(std::move(firstFrame)) {}

  void retire() { m_active = false; }

  FrameBuffers beginFrame(int width, int height, int fullWidth, int fullHeight, bool withCatcher) override {
    m_forwarding = m_active.load();
    return m_forwarding ? m_sink.beginFrame(width, height, fullWidth, fullHeight, withCatcher) : FrameBuffers();
  }
  void endFrame(std::uint64_t view, int samples, bool denoised) override {
    if (m_forwarding) {
      m_sink.endFrame(view, samples, denoised);
    }
    if (m_firstFrame && !m_framed.exchange(true)) {
      m_firstFrame();
    }
  }
  void finished(std::uint64_t view, int samples, double seconds) override {
    if (m_active) {
      m_sink.finished(view, samples, seconds);
    }
  }

private:
  FrameSink& m_sink;
  std::function<void()> m_firstFrame;
  std::atomic<bool> m_active{true};
  std::atomic<bool> m_framed{false};
  bool m_forwarding = false; // the renderer's thread only
};

class FallbackRenderer : public Renderer {
public:
  FallbackRenderer(FrameSink& sink, RendererFactory factory, DeviceReport report, std::string testFailure)
      : m_sink(sink), m_factory(std::move(factory)), m_report(std::move(report)),
        m_testFailure(std::move(testFailure)) {}

  ~FallbackRenderer() override {
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_stop = true;
    }
    m_changed.notify_all();
    if (m_watcher.joinable()) {
      m_watcher.join();
    }
    m_inner.reset();
  }

  // Starts on the device, else on the CPU; false with `error` when neither
  // starts.
  bool start(const DeviceInfo& device, std::string& error) {
    std::string why;
    if (!startOn(device, why) && device.type != "CPU") {
      std::string cpuError;
      if (!startOn(DeviceInfo(), cpuError)) {
        error = why + "; the CPU: " + cpuError;
        return false;
      }
      m_report(m_inner->device(), fallbackMessage(device, why));
    } else if (!m_inner) {
      error = why;
      return false;
    }
    m_watcher = std::thread([this] { watch(); });
    return true;
  }

  std::string description() const override { return current()->description(); }

  // The meshes the renderer holds and the last bodies are kept, so that
  // the CPU can take over with the whole scene.
  SceneChanges updateScene(const SceneUpdate& update) override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    for (const auto& [key, mesh] : update.meshes) {
      m_meshes[key] = mesh;
    }
    for (const std::string& key : update.released) {
      m_meshes.erase(key);
    }
    m_bodies = update.bodies;
    m_ground = update.ground;
    m_hasScene = true;
    return m_inner->updateScene(update);
  }
  void setView(const ViewData& view) override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_view = view;
    m_inner->setView(view);
  }
  void setSamples(int samples) override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_samples = samples;
    m_inner->setSamples(samples);
  }
  void setPreviews(const std::vector<int>& samples) override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_previews = samples;
    m_inner->setPreviews(samples);
  }
  std::string setEnvironment(const EnvironmentData& environment) override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_environment = environment;
    return m_inner->setEnvironment(environment);
  }
  void setFinal(bool denoise, double timeLimit) override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_final = std::make_pair(denoise, timeLimit);
    m_inner->setFinal(denoise, timeLimit);
  }

  // Until the samples are done, also when the device failed on the way and
  // the CPU rendered them again.
  void wait() override {
    for (;;) {
      const std::shared_ptr<Renderer> renderer = current();
      renderer->wait();
      std::unique_lock<std::mutex> lock(m_mutex);
      m_changed.wait(lock, [&] { return m_stop || m_inner != renderer || !replaceable(*renderer); });
      if (m_stop || m_inner == renderer) {
        return;
      }
    }
  }

  int currentSample() const override { return current()->currentSample(); }
  DeviceInfo device() const override { return current()->device(); }
  std::string deviceError() const override { return current()->deviceError(); }

private:
  std::shared_ptr<Renderer> current() const {
    const std::lock_guard<std::mutex> lock(m_mutex);
    return m_inner;
  }

  // A renderer on the device (an empty one: the CPU) with its own sink;
  // false when it cannot start or its device failed at once.
  bool startOn(const DeviceInfo& device, std::string& error) {
    const bool first = !m_inner;
    auto sink = std::make_shared<DeviceSink>(m_sink, first && !m_testFailure.empty()
                                                         ? std::function<void()>([this] { simulateFailure(); })
                                                         : std::function<void()>());
    std::unique_ptr<Renderer> renderer = m_factory(*sink, device, error);
    if (!renderer) {
      return false;
    }
    error = renderer->deviceError();
    if (!error.empty()) {
      return false;
    }
    // The renderer and its sink go together, the renderer first: its
    // session's thread may write frames until it ends, also when a wait()
    // still holds it after the CPU took over.
    auto pair = std::make_shared<std::pair<std::shared_ptr<DeviceSink>, std::unique_ptr<Renderer>>>(
        sink, std::move(renderer));
    m_inner = std::shared_ptr<Renderer>(pair, pair->second.get());
    m_innerSink = std::move(sink);
    m_first = m_inner.get();
    if (!first) {
      m_first = nullptr;
    }
    return true;
  }

  static std::string fallbackMessage(const DeviceInfo& device, const std::string& why) {
    return "Rendering on the CPU: " + nameOf(device) + " failed (" + why + ")";
  }

  // The device's error, or the test hook's for the first renderer.
  std::string failure(const Renderer& renderer) const {
    if (&renderer == m_first && m_simulated) {
      return m_testFailure;
    }
    return renderer.deviceError();
  }

  // The renderer's device failed and the CPU can take over (with the lock).
  bool replaceable(const Renderer& renderer) const {
    return !m_fallbackFailed && (renderer.device().type != "CPU" || &renderer == m_first) &&
           !failure(renderer).empty();
  }

  // From the renderer's thread, which may hold the renderer's own locks
  // while the main thread waits for them with m_mutex held: no lock here
  // (the watcher looks every 100 ms).
  void simulateFailure() {
    m_simulated = true;
    m_changed.notify_all();
  }

  // Watches the device: when it fails, the CPU takes over with what the
  // failed one had.
  void watch() {
    std::unique_lock<std::mutex> lock(m_mutex);
    while (!m_stop) {
      m_changed.wait_for(lock, std::chrono::milliseconds(100));
      if (m_stop || !replaceable(*m_inner)) {
        continue;
      }
      const DeviceInfo failed = m_inner->device();
      const std::string why = failure(*m_inner);
      std::shared_ptr<Renderer> old = m_inner;
      std::shared_ptr<DeviceSink> oldSink = m_innerSink;
      oldSink->retire();
      std::string error;
      if (!startOn(DeviceInfo(), error)) {
        m_fallbackFailed = true;
        lock.unlock();
        m_report(failed, "The render device failed (" + why + ") and the CPU could not take over (" + error + ")");
        m_changed.notify_all();
        lock.lock();
        continue;
      }
      // What the failed renderer had, in the order the worker sends it.
      if (m_samples) {
        m_inner->setSamples(*m_samples);
      }
      if (m_previews) {
        m_inner->setPreviews(*m_previews);
      }
      if (m_final) {
        m_inner->setFinal(m_final->first, m_final->second);
      }
      if (m_environment) {
        m_inner->setEnvironment(*m_environment);
      }
      if (m_hasScene) {
        m_inner->updateScene(wholeScene());
      }
      if (m_view) {
        m_inner->setView(*m_view);
      }
      const DeviceInfo now = m_inner->device();
      lock.unlock();
      m_report(now, fallbackMessage(failed, why));
      // The failed renderer ends here, not while the lock is held: its
      // session's thread may still deliver a frame (to its retired sink).
      old.reset();
      m_changed.notify_all();
      lock.lock();
    }
  }

  FrameSink& m_sink;
  RendererFactory m_factory;
  DeviceReport m_report;
  std::string m_testFailure;
  mutable std::mutex m_mutex;
  std::condition_variable m_changed;
  std::shared_ptr<Renderer> m_inner;
  std::shared_ptr<DeviceSink> m_innerSink; // the current renderer's
  const Renderer* m_first = nullptr;       // the first renderer, while it renders
  std::atomic<bool> m_simulated{false};
  bool m_fallbackFailed = false;
  bool m_stop = false;
  std::thread m_watcher;
  // The scene as the renderer holds it: all its meshes and the bodies.
  SceneUpdate wholeScene() const {
    SceneUpdate scene;
    scene.meshes.assign(m_meshes.begin(), m_meshes.end());
    scene.bodies = m_bodies;
    scene.ground = m_ground;
    return scene;
  }

  // What the renderer was given, for the CPU to take over.
  std::map<std::string, MeshData> m_meshes;
  std::vector<BodyData> m_bodies;
  bool m_ground = true;
  bool m_hasScene = false;
  std::optional<std::vector<int>> m_previews;
  std::optional<ViewData> m_view;
  std::optional<EnvironmentData> m_environment;
  std::optional<int> m_samples;
  std::optional<std::pair<bool, double>> m_final;
};

} // namespace

DeviceInfo chooseDevice(const std::string& choice, const std::vector<DeviceInfo>& devices, std::string& message) {
  message.clear();
  const std::string wanted = lowercase(choice);
  if (wanted.empty() || wanted == kDeviceAutomatic) {
    for (const char* type : {"OPTIX", "CUDA", "HIP", "METAL", "ONEAPI"}) {
      for (const DeviceInfo& device : devices) {
        if (device.type == type) {
          return device;
        }
      }
    }
    return cpuOf(devices);
  }
  if (wanted == kDeviceCpu) {
    return cpuOf(devices);
  }
  for (const DeviceInfo& device : devices) {
    if (device.id == choice) {
      return device;
    }
  }
  for (const DeviceInfo& device : devices) {
    if (lowercase(device.type) == wanted) {
      return device;
    }
  }
  message = "There is no render device \"" + choice + "\"; rendering on the CPU";
  return cpuOf(devices);
}

std::unique_ptr<Renderer> makeDeviceRenderer(FrameSink& sink, const DeviceInfo& device, RendererFactory factory,
                                             DeviceReport report, std::string testFailure, std::string& error) {
  auto renderer =
      std::make_unique<FallbackRenderer>(sink, std::move(factory), std::move(report), std::move(testFailure));
  if (!renderer->start(device, error)) {
    return nullptr;
  }
  return renderer;
}

} // namespace mitcad::render
