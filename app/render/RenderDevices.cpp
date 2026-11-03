// SPDX-License-Identifier: MIT
#include "render/RenderDevices.hpp"

#include <algorithm>
#include <atomic>
#include <cctype>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
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

// It denoises on a GPU (its own or another), not on the CPU.
bool gpuDenoising(const DeviceInfo& device) {
  return !device.denoiser.type.empty() && device.denoiser.type != "CPU";
}

// The GPU its denoiser runs on, for a message.
std::string denoiserNameOf(const DeviceInfo& device) {
  if (device.denoiser.name.empty() || device.denoiser.id == device.id) {
    return nameOf(device);
  }
  return device.denoiser.name + " (" + device.denoiser.type + ")";
}

// The renderer's sink as one renderer sees it: once that renderer is
// replaced, its frames no longer reach the sink.
class DeviceSink : public FrameSink, public std::enable_shared_from_this<DeviceSink> {
  class ForwardedFrame final : public DirectFrame {
  public:
    ForwardedFrame(std::shared_ptr<DeviceSink> owner, std::shared_ptr<DirectFrame> frame)
        : m_owner(std::move(owner)), m_frame(std::move(frame)) {}
    ~ForwardedFrame() override { discard(); }
    FrameBuffers buffers() const override { return m_frame->buffers(); }
    void* nativeMetalBuffer() const override { return m_frame->nativeMetalBuffer(); }
    std::size_t nativeMetalBufferSize() const override { return m_frame->nativeMetalBufferSize(); }
    void* nativeCatcherMetalBuffer() const override { return m_frame->nativeCatcherMetalBuffer(); }
    std::size_t nativeCatcherMetalBufferSize() const override { return m_frame->nativeCatcherMetalBufferSize(); }
    void publish(std::uint64_t view, int samples, bool denoised, bool catcher) override {
      const std::lock_guard<std::mutex> lock(m_owner->m_publicationMutex);
      if (m_complete) return;
      m_complete = true;
      if (m_owner->m_active.load()) {
        m_frame->publish(view, samples, denoised, catcher);
        m_owner->firstFrame();
      } else m_frame->discard();
    }
    void discard() override {
      const std::lock_guard<std::mutex> lock(m_owner->m_publicationMutex);
      if (m_complete) return;
      m_complete = true;
      m_frame->discard();
    }
  private:
    std::shared_ptr<DeviceSink> m_owner;
    std::shared_ptr<DirectFrame> m_frame;
    bool m_complete = false;
  };
public:
  DeviceSink(FrameSink& sink, std::function<void()> firstFrame) : m_sink(sink), m_firstFrame(std::move(firstFrame)) {}

  void retire() {
    const std::lock_guard<std::mutex> lock(m_publicationMutex);
    m_active = false;
  }

  // Capabilities stay stable through retirement. An inactive renderer gets
  // no new direct frame rather than switching a cached GPU interop path to
  // an incompatible host buffer while it finishes cancellation.
  bool directDisplayBuffers() const override { return m_sink.directDisplayBuffers(); }
  bool metalDisplayBuffers() const override { return m_sink.metalDisplayBuffers(); }
  void displayInteropUsed() override { if (m_active.load()) m_sink.displayInteropUsed(); }
  void auxiliaryNativeUsed(bool graphics, bool composite) override {
    if (m_active.load()) m_sink.auxiliaryNativeUsed(graphics, composite);
  }
  void auxiliaryNativeCompared(unsigned maxHalfUlps, bool composite) override {
    if (m_active.load()) m_sink.auxiliaryNativeCompared(maxHalfUlps, composite);
  }
  std::shared_ptr<DirectFrame> beginDisplayFrame(int width, int height, int fullWidth, int fullHeight) override {
    if (!m_active.load()) return {};
    auto frame = m_sink.beginDisplayFrame(width, height, fullWidth, fullHeight);
    if (!frame) return {};
    if (!m_active.load()) { frame->discard(); return {}; }
    return std::make_shared<ForwardedFrame>(shared_from_this(), std::move(frame));
  }

  FrameBuffers beginFrame(int width, int height, int fullWidth, int fullHeight, bool withCatcher) override {
    m_forwarding = m_active.load();
    return m_forwarding ? m_sink.beginFrame(width, height, fullWidth, fullHeight, withCatcher) : FrameBuffers();
  }
  void endFrame(std::uint64_t view, int samples, bool denoised) override {
    if (m_forwarding) {
      m_sink.endFrame(view, samples, denoised);
    }
    firstFrame();
  }
  void finished(std::uint64_t view, int samples, double seconds) override {
    if (m_active) {
      m_sink.finished(view, samples, seconds);
    }
  }
  void denoisingDevice(const DeviceInfo& device) override {
    if (m_active) {
      m_sink.denoisingDevice(device);
    }
  }

private:
  void firstFrame() {
    // This callback only sets the fallback's atomic failure marker and wakes
    // its watcher; it never acquires the fallback mutex. Direct publication
    // keeps it inside retire()'s lifetime barrier, avoiding a lock inversion.
    if (m_firstFrame && !m_framed.exchange(true)) m_firstFrame();
  }
  FrameSink& m_sink;
  std::function<void()> m_firstFrame;
  std::atomic<bool> m_active{true};
  std::atomic<bool> m_framed{false};
  std::mutex m_publicationMutex;
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
      if (m_innerSink) m_innerSink->retire();
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
    if (!startOn(device, false, why)) {
      std::string cpuError;
      if (!startOn(DeviceInfo(), true, cpuError)) {
        error = why + "; the CPU: " + cpuError;
        return false;
      }
      m_report(m_inner->device(), fallbackMessage(device, why));
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
  void setPaused(bool paused) override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_paused = paused;
    m_inner->setPaused(paused);
  }
  bool isPaused() const override { return current()->isPaused(); }
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
  bool startOn(const DeviceInfo& device, bool cpuDenoising, std::string& error) {
    const bool first = !m_inner;
    auto sink = std::make_shared<DeviceSink>(m_sink, first && !cpuDenoising && !m_testFailure.empty()
                                                         ? std::function<void()>([this] { simulateFailure(); })
                                                         : std::function<void()>());
    std::unique_ptr<Renderer> renderer = m_factory(*sink, device, cpuDenoising, error);
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
    m_first = first && !cpuDenoising ? m_inner.get() : nullptr;
    return true;
  }

  static std::string fallbackMessage(const DeviceInfo& device, const std::string& why) {
    return "Rendering on the CPU: " + nameOf(device) + " failed (" + why + ")";
  }
  static std::string denoisingMessage(const DeviceInfo& device, const std::string& why) {
    return "Denoising on the CPU: Open Image Denoise on " + denoiserNameOf(device) + " failed (" + why + ")";
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
    const DeviceInfo device = renderer.device();
    return !m_fallbackFailed && (device.type != "CPU" || gpuDenoising(device) || &renderer == m_first) &&
           !failure(renderer).empty();
  }

  // From the renderer's thread, which may hold the renderer's own locks
  // while the main thread waits for them with m_mutex held: no lock here
  // (the watcher looks every 100 ms).
  void simulateFailure() {
    m_simulated = true;
    m_changed.notify_all();
  }

  // Watches the device: when it fails, another renderer takes over with
  // what the failed one had. A GPU that also denoised on a GPU first gets
  // the same GPU with CPU denoising (Cycles' error does not say whether the
  // path tracer or the denoiser failed: both share the device), once; else,
  // or when that fails too, the CPU. The test hook's failure is the path
  // tracer's.
  void watch() {
    std::unique_lock<std::mutex> lock(m_mutex);
    while (!m_stop) {
      m_changed.wait_for(lock, std::chrono::milliseconds(100));
      if (m_stop || !replaceable(*m_inner)) {
        continue;
      }
      const DeviceInfo failed = m_inner->device();
      const std::string why = failure(*m_inner);
      const bool simulated = m_inner.get() == m_first && m_simulated;
      std::shared_ptr<Renderer> old = m_inner;
      std::shared_ptr<DeviceSink> oldSink = m_innerSink;
      oldSink->retire();
      std::string error;
      std::string message = fallbackMessage(failed, why);
      if (!m_denoisingRetried && failed.type != "CPU" && gpuDenoising(failed) && !simulated) {
        m_denoisingRetried = true;
        std::string retryError;
        if (startOn(failed, true, retryError)) {
          message = denoisingMessage(failed, why);
        }
      }
      if (m_inner == old && !startOn(DeviceInfo(), true, error)) {
        m_fallbackFailed = true;
        lock.unlock();
        m_report(failed, "The render device failed (" + why + ") and the CPU could not take over (" + error + ")");
        m_changed.notify_all();
        lock.lock();
        continue;
      }
      m_inner->setPaused(m_paused);
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
      m_report(now, message);
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
  bool m_denoisingRetried = false; // the same GPU with CPU denoising took over once
  bool m_stop = false;
  bool m_paused = false;
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

bool metalAsyncPipelineCreation(const std::vector<DeviceInfo>& devices, const char* overrideValue) {
  if (overrideValue != nullptr) {
    return std::atoi(overrideValue) != 0;
  }
  return std::none_of(devices.begin(), devices.end(), [](const DeviceInfo& device) {
    return device.type == "METAL" && device.name.find("Apple Paravirtual device") != std::string::npos;
  });
}

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
