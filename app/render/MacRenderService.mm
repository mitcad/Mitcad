// SPDX-License-Identifier: MIT
#include "render/MacFrameControl.hpp"
#include "render/MacFrameSurface.hpp"
#include "render/MacFrameMetal.hpp"
#include "render/MacRenderProtocol.hpp"
#include "render/MacCommandPayload.hpp"
#include "render/MacXpcPeer.hpp"
#include "render/FrameMemory.hpp"
#include "render/RenderSession.hpp"
#include "platform/MacChrome.hpp"
#include "report/CrashHandler.hpp"

#include <Foundation/Foundation.h>
#include <IOSurface/IOSurfaceAPI.h>
#include <os/log.h>
#include <QCoreApplication>
#include <QJsonDocument>
#include <array>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <mutex>
#include <utility>
#include <sys/stat.h>
#include <unistd.h>

namespace {
using namespace mitcad::render;
std::atomic<unsigned> g_connections{0};
std::uint64_t nowSeconds() {
  return static_cast<std::uint64_t>(std::chrono::duration_cast<std::chrono::seconds>(
      std::chrono::steady_clock::now().time_since_epoch()).count());
}

class SurfaceFrames : public FrameSink {
public:
  RenderSession::Event event;
  std::function<void()> failed;
  bool metalDisplayBuffers() const override { return m_metal; }
  void displayInteropUsed() override {
    if (!m_reportedInterop.exchange(true)) event({{QStringLiteral("event"), QStringLiteral("transport")},
        {QStringLiteral("kind"), QStringLiteral("metal-buffer")}});
  }
  void auxiliaryNativeUsed(bool graphics, bool composite) override {
    const unsigned bit = 1u << ((graphics ? 1 : 0) + (composite ? 2 : 0));
    if ((m_reportedAuxiliary.fetch_or(bit) & bit) == 0)
      event({{QStringLiteral("event"), QStringLiteral("transport")},
             {QStringLiteral("kind"), QStringLiteral("native-auxiliary")},
             {QStringLiteral("graphics"), graphics}, {QStringLiteral("composite"), composite}});
  }
  void auxiliaryNativeCompared(unsigned maxHalfUlps, bool composite) override {
    event({{QStringLiteral("event"), QStringLiteral("transport")},
           {QStringLiteral("kind"), QStringLiteral("native-auxiliary-reference")},
           {QStringLiteral("maxHalfUlps"), int(maxHalfUlps)}, {QStringLiteral("composite"), composite}});
  }
  bool attach(xpc_object_t message) {
    std::lock_guard<std::mutex> guard(m_mutex);
    const auto generation = xpc_dictionary_get_uint64(message, "generation");
    if (generation == 0 || generation <= m_generation) return false;
    const int descriptor = xpc_dictionary_dup_fd(message, "control");
    struct stat status{};
    if (descriptor < 0) return false;
    const auto page = sysconf(_SC_PAGESIZE);
    if (fstat(descriptor, &status) != 0 || page <= 0 || status.st_size < 0 ||
        !validMacFrameStorage(static_cast<std::size_t>(status.st_size), static_cast<std::size_t>(page))) {
      close(descriptor); return false;
    }
    auto mapping = std::make_shared<FrameMemory>();
    std::string error;
    if (!mapping->attach(descriptor, error)) return false;
    auto* control = static_cast<MacFrameControl*>(mapping->data());
    if (!validMacFrameControl(*control, mapping->size()) || control->generation != generation ||
        control->published.load() != kNoSlot || control->readingMask.load() != 0 || control->publicationCounter != 0) return false;
    xpc_object_t array = xpc_dictionary_get_value(message, "surfaces");
    if (!array || xpc_get_type(array) != XPC_TYPE_ARRAY || xpc_array_get_count(array) != kFrameSlots * kFramePlanes) return false;
    std::array<IOSurfaceRef, kFrameSlots * kFramePlanes> imported{};
    for (std::size_t index = 0; index < imported.size(); ++index) imported[index] = IOSurfaceLookupFromXPCObject(xpc_array_get_value(array, index));
    auto surfaces = std::make_shared<MacFrameSurfaces>();
    if (!surfaces->adopt(imported, control->capacityWidth, control->capacityHeight, error)) return false;
    m_allocation = std::make_shared<Allocation>();
    m_allocation->mapping = std::move(mapping);
    m_allocation->surfaces = std::move(surfaces);
    m_allocation->generation = generation;
    m_allocation->width = control->capacityWidth;
    m_allocation->height = control->capacityHeight;
    m_generation = generation;
    m_notice = false;
    return true;
  }
  bool directDisplayBuffers() const override { return true; }
  std::shared_ptr<DirectFrame> beginDisplayFrame(int width, int height, int fullWidth, int fullHeight) override {
    std::lock_guard<std::mutex> guard(m_mutex);
    auto* control = header();
    if (!control || control->magic != kMacFrameMagic || control->version != kMacFrameVersion ||
        control->generation != m_generation || control->capacityWidth != m_allocation->width ||
        control->capacityHeight != m_allocation->height || width <= 0 || height <= 0 ||
        static_cast<std::uint32_t>(width) > m_allocation->width || static_cast<std::uint32_t>(height) > m_allocation->height ||
        (control->readingMask.load() & ~kMacFrameReaderMask) != 0) return {};
    auto slot = kNoSlot;
    const auto retained = control->readingMask.load() | m_allocation->writingMask;
    const auto published = control->published.load();
    for (std::uint32_t index = 0; index < kFrameSlots; ++index) {
      if (index != published && (retained & (1u << index)) == 0) { slot = index; break; }
    }
    std::string error;
    if (slot == kNoSlot || !m_allocation->surfaces->lock(slot, error)) return {};
    m_allocation->writingMask |= 1u << slot;
    auto& info = control->frames[slot];
    info = {};
    info.width = static_cast<std::uint32_t>(width);
    info.height = static_cast<std::uint32_t>(height);
    info.fullWidth = static_cast<std::uint32_t>(fullWidth);
    info.fullHeight = static_cast<std::uint32_t>(fullHeight);
    return std::make_shared<Staging>(*this, m_allocation, slot);
  }
  FrameBuffers beginFrame(int width, int height, int fullWidth, int fullHeight, bool catcher) override {
    m_fallback = beginDisplayFrame(width, height, fullWidth, fullHeight);
    m_fallbackCatcher = catcher;
    auto result = m_fallback ? m_fallback->buffers() : FrameBuffers{};
    if (!catcher) result.catcher = nullptr;
    return result;
  }
  void endFrame(std::uint64_t view, int samples, bool denoised) override {
    if (m_fallback) m_fallback->publish(view, samples, denoised, m_fallbackCatcher);
    m_fallback.reset();
  }
  void consumed(std::uint64_t generation, std::uint64_t publication) {
    std::lock_guard<std::mutex> guard(m_mutex);
    if (generation != m_generation || publication < m_noticedPublication) return;
    m_notice = false;
    auto* control = header();
    const auto slot = control ? control->published.load() : kNoSlot;
    if (slot < kFrameSlots && control->publications[slot] > publication) notify();
  }
  void finished(std::uint64_t view, int samples, double seconds) override {
    event({{QStringLiteral("event"), QStringLiteral("done")}, {QStringLiteral("view"), static_cast<qint64>(view)},
            {QStringLiteral("samples"), samples}, {QStringLiteral("seconds"), seconds}});
  }
  void denoisingDevice(const DeviceInfo& device) override {
    event({{QStringLiteral("event"), QStringLiteral("denoiser")},
            {QStringLiteral("device"), renderDeviceJson(device)}});
  }
private:
  struct Allocation {
    std::shared_ptr<FrameMemory> mapping;
    std::shared_ptr<MacFrameSurfaces> surfaces;
    std::uint64_t generation = 0;
    std::uint32_t width = 0, height = 0;
    std::uint32_t writingMask = 0;
    std::array<std::unique_ptr<MacFrameMetalBuffer>, kFrameSlots * kFramePlanes> metal;
    MacFrameControl* header() const { return static_cast<MacFrameControl*>(mapping->data()); }
  };
  struct Staging : DirectFrame {
    SurfaceFrames& owner;
    std::shared_ptr<Allocation> allocation;
    std::uint32_t slot;
    bool locked = true;
    Staging(SurfaceFrames& sink, std::shared_ptr<Allocation> source, std::uint32_t index)
        : owner(sink), allocation(std::move(source)), slot(index) {}
    ~Staging() override { discard(); }
    void discard() override {
      std::lock_guard<std::mutex> guard(owner.m_mutex);
      if (locked) {
        std::string error;
        allocation->surfaces->unlock(slot, error);
        allocation->writingMask &= ~(1u << slot);
        locked = false;
      }
    }
    FrameBuffers buffers() const override { return {allocation->surfaces->pixels(slot), allocation->surfaces->pixels(slot, 1)}; }
    void* metalBuffer(std::uint32_t plane) const {
      std::lock_guard<std::mutex> guard(owner.m_mutex);
      auto& cached = allocation->metal[slot * kFramePlanes + plane];
      if (!cached) {
        auto buffer = std::make_unique<MacFrameMetalBuffer>();
        std::string error;
        if (!buffer->wrap(allocation->surfaces->surface(slot, plane), error)) {
          owner.event({{QStringLiteral("event"), QStringLiteral("error")}, {QStringLiteral("message"), QString::fromStdString(error)}});
          if (owner.failed) owner.failed();
          return nullptr;
        }
        cached = std::move(buffer);
      }
      return cached->handle();
    }
    std::size_t metalBufferSize(std::uint32_t plane) const {
      std::lock_guard<std::mutex> guard(owner.m_mutex);
      const auto& cached = allocation->metal[slot * kFramePlanes + plane];
      return cached ? cached->size() : 0;
    }
    void* nativeMetalBuffer() const override { return metalBuffer(0); }
    std::size_t nativeMetalBufferSize() const override { return metalBufferSize(0); }
    void* nativeCatcherMetalBuffer() const override { return metalBuffer(1); }
    std::size_t nativeCatcherMetalBufferSize() const override { return metalBufferSize(1); }
    void publish(std::uint64_t view, int samples, bool denoised, bool catcher) override {
      std::lock_guard<std::mutex> guard(owner.m_mutex);
      if (!locked) return;
      auto* control = allocation->header();
      auto& info = control->frames[slot];
      info.view = view;
      info.samples = static_cast<std::uint32_t>(samples);
      info.denoised = denoised ? 1 : 0;
      info.catcher = catcher ? 1 : 0;
      std::string error;
      const bool unlocked = allocation->surfaces->unlock(slot, error);
      locked = false;
      allocation->writingMask &= ~(1u << slot);
      if (unlocked) {
        control->publications[slot] = ++control->publicationCounter;
        control->published.store(slot);
        if (owner.m_allocation == allocation) owner.notify();
      }
    }
  };
  MacFrameControl* header() { return m_allocation ? m_allocation->header() : nullptr; }
  void notify() {
    if (m_notice) return;
    auto* control = header();
    const auto slot = control->published.load();
    if (slot >= kFrameSlots) return;
    m_notice = true;
    m_noticed = control->frames[slot];
    m_noticedPublication = control->publications[slot];
    event({{QStringLiteral("event"), QStringLiteral("frame")}, {QStringLiteral("memory"), static_cast<qint64>(m_generation)},
            {QStringLiteral("view"), static_cast<qint64>(m_noticed.view)}, {QStringLiteral("samples"), static_cast<int>(m_noticed.samples)},
            {QStringLiteral("publication"), static_cast<qint64>(m_noticedPublication)},
            {QStringLiteral("denoised"), m_noticed.denoised != 0}});
  }
  std::mutex m_mutex;
  std::shared_ptr<Allocation> m_allocation;
  std::shared_ptr<DirectFrame> m_fallback;
  bool m_fallbackCatcher = false;
  std::uint64_t m_generation = 0;
  bool m_notice = false;
  FrameInfo m_noticed{};
  std::uint64_t m_noticedPublication = 0;
  const bool m_metal = MacFrameMetalBuffer::supported();
  std::atomic<bool> m_reportedInterop{false};
  std::atomic<unsigned> m_reportedAuxiliary{0};
};

struct Session : std::enable_shared_from_this<Session> {
  xpc_connection_t connection = nullptr; // XPC owns this through its context/finalizer.
  dispatch_queue_t commands = nullptr;
  dispatch_source_t deadline = nullptr;
  std::mutex output;
  bool alive = true;
  bool opened = false;
  std::atomic<std::size_t> pending{0};
  std::atomic<bool> bulkPending{false};
  std::atomic<unsigned> allocations{0};
  std::atomic<std::uint64_t> lastReceived{nowSeconds()};
  std::atomic<std::uint64_t> activeCommand{0};
  SurfaceFrames frames;
  std::unique_ptr<RenderSession> renderer;
  xpc_object_t activity = nullptr; // Under output; one no-reply work message.
  void endActivity() {
    xpc_object_t message = nullptr;
    { std::lock_guard<std::mutex> guard(output); message = std::exchange(activity, nullptr); }
    if (message) xpc_release(message);
  }
  ~Session() {
    renderer.reset();
    endActivity();
    if (deadline) { dispatch_source_cancel(deadline); dispatch_release(deadline); }
    if (commands) dispatch_release(commands);
    --g_connections;
  }
  void event(const QJsonObject& event) {
    const auto json = QJsonDocument(event).toJson(QJsonDocument::Compact);
    if (static_cast<std::size_t>(json.size()) > kMacRenderMessageBytes) return;
    std::lock_guard<std::mutex> guard(output);
    if (!alive) return;
    auto message = xpc_dictionary_create_empty();
    xpc_dictionary_set_data(message, "event", json.constData(), static_cast<std::size_t>(json.size()));
    xpc_connection_send_message(connection, message);
    xpc_release(message);
  }
  void close() {
    {
      std::lock_guard<std::mutex> guard(output);
      if (!alive) return;
      alive = false;
      xpc_connection_cancel(connection);
    }
    // Renderer destruction belongs to its serial command queue, never the
    // Cycles callback thread. Keep the watchdog alive through cancellation.
    auto self = shared_from_this();
    dispatch_async(commands, ^{
      self->activeCommand.store(nowSeconds());
      self->renderer.reset();
      self->endActivity();
      self->activeCommand.store(0);
    });
  }
  // Blocks preserve reference-variable types rather than copying their referent.
  // Take an owning value so the queued block never references its caller's stack.
  void received(xpc_object_t message, std::shared_ptr<Session> self) {
    if (xpc_get_type(message) == XPC_TYPE_ERROR) { close(); return; }
    if (xpc_get_type(message) != XPC_TYPE_DICTIONARY ||
        xpc_dictionary_get_uint64(message, "version") != kMacRenderProtocol) { close(); return; }
    lastReceived.store(nowSeconds());
    const char* op = xpc_dictionary_get_string(message, "op");
    if (!op) { close(); return; }
    os_log(OS_LOG_DEFAULT, "MitCad renderer peer=%d received %{public}s", xpc_connection_get_pid(connection), op);
    if (std::strcmp(op, "activity") == 0) {
      bool accepted = false;
      {
        std::lock_guard<std::mutex> guard(output);
        if (alive && !opened && !activity && xpc_dictionary_get_count(message) == 2) {
          // Cycles keeps working after command acceptance replies. Retaining
          // this no-reply message keeps its XPC transaction and the client's
          // priority boost until close has actually stopped that work.
          activity = xpc_retain(message);
          accepted = true;
        }
      }
      if (!accepted) close();
      return;
    }
    auto reply = xpc_dictionary_create_reply(message);
    if (!reply) { close(); return; }
    if (std::strcmp(op, "heartbeat") == 0 || std::strcmp(op, "consumed") == 0) {
      if (std::strcmp(op, "consumed") == 0) frames.consumed(xpc_dictionary_get_uint64(message, "generation"),
          xpc_dictionary_get_uint64(message, "publication"));
      xpc_dictionary_set_bool(reply, "ok", true);
      xpc_connection_send_message(connection, reply);
      xpc_release(reply);
      return;
    }
    std::size_t length = 0;
    const auto commandValue = xpc_dictionary_get_value(message, "command");
    const void* json = xpc_dictionary_get_data(message, "command", &length);
    const bool open = std::strcmp(op, "open") == 0;
    const bool allocate = std::strcmp(op, "allocate") == 0;
    auto bulkValue = xpc_dictionary_get_value(message, "command_fd");
    auto bulkSizeValue = xpc_dictionary_get_value(message, "command_bytes");
    const bool bulk = bulkValue != nullptr;
    const auto bulkSize = xpc_dictionary_get_uint64(message, "command_bytes");
    const bool validBulk = bulk && xpc_get_type(bulkValue) == XPC_TYPE_FD && bulkSizeValue &&
        xpc_get_type(bulkSizeValue) == XPC_TYPE_UINT64 && bulkSize > 0 && bulkSize <= kMacCommandPayloadMaxBytes && !commandValue;
    if ((open && opened) || (!open && !opened) || length > kMacRenderMessageBytes ||
        (!open && !allocate && (std::strcmp(op, "command") != 0 ||
            (bulk ? !validBulk : (!commandValue || xpc_get_type(commandValue) != XPC_TYPE_DATA || !json)))) ||
        ((!bulk && bulkSizeValue) || ((open || allocate) && (bulk || commandValue))) ||
        (bulk && bulkPending.exchange(true)) ||
        pending.fetch_add(length + 1024) + length + 1024 > kMacRenderPendingBytes) {
      xpc_release(reply); close(); return;
    }
    if (allocate && allocations.fetch_add(1) >= 2) {
      --allocations;
      xpc_release(reply); close(); return;
    }
    if (open) opened = true;
    xpc_retain(message);
    dispatch_async(commands, ^{
      @autoreleasepool {
        bool ok = false;
        { std::lock_guard<std::mutex> guard(self->output); ok = self->alive; }
        self->activeCommand.store(nowSeconds());
        if (!ok) {} // Drain bounded queued messages after cancellation without running their commands.
        else if (open) {
          std::size_t configBytes = 0;
          const auto* config = static_cast<const char*>(xpc_dictionary_get_data(message, "configuration", &configBytes));
          const auto document = config && configBytes <= 16384 ? QJsonDocument::fromJson(QByteArray(config, static_cast<int>(configBytes))) : QJsonDocument();
          ok = document.isObject();
          if (ok) {
            self->renderer = std::make_unique<RenderSession>(self->frames, self->frames.event, document.object());
            self->renderer->hello();
          }
        } else if (allocate) ok = self->frames.attach(message);
        else {
          QByteArray bytes;
          std::string error;
          const bool valid = !bulk || MacCommandPayload::readOwned(
              xpc_dictionary_dup_fd(message, "command_fd"), bulkSize, bytes, error);
          if (!bulk) bytes = QByteArray(static_cast<const char*>(json), static_cast<int>(length));
          const auto document = valid ? QJsonDocument::fromJson(bytes) : QJsonDocument();
          ok = document.isObject() && self->renderer && self->renderer->command(document.object());
          if (!valid) self->frames.event({{QStringLiteral("event"), QStringLiteral("error")},
              {QStringLiteral("message"), QString::fromStdString(error)}});
        }
        self->activeCommand.store(0);
        os_log(OS_LOG_DEFAULT, "MitCad renderer peer=%d completed %{public}s accepted=%d",
               xpc_connection_get_pid(self->connection), op, ok);
        self->pending.fetch_sub(length + 1024);
        if (bulk) self->bulkPending.store(false);
        if (allocate) --self->allocations;
        {
          std::lock_guard<std::mutex> guard(self->output);
          if (self->alive) {
            xpc_dictionary_set_bool(reply, "ok", ok);
            xpc_connection_send_message(self->connection, reply);
          }
        }
        xpc_release(reply);
        xpc_release(message);
        if (!ok) self->close();
      }
    });
  }
};

void accept(xpc_connection_t connection) {
  @autoreleasepool {
    if (xpc_connection_get_euid(connection) != geteuid()) { xpc_connection_cancel(connection); return; }
    if (g_connections.fetch_add(1) >= kMacRenderConnections) { --g_connections; xpc_connection_cancel(connection); return; }
    auto session = std::shared_ptr<Session>(new Session, [](Session* value) {
      if (value->commands) dispatch_async(value->commands, ^{ delete value; });
      else delete value;
    });
    session->connection = connection;
    NSString* app = NSBundle.mainBundle.bundlePath;
    for (int index = 0; index < 3; ++index) app = app.stringByDeletingLastPathComponent;
    std::string error;
    if (!requirePeer(connection, app.fileSystemRepresentation, error)) {
      std::fprintf(stderr, "render XPC: %s\n", error.c_str()); xpc_connection_cancel(connection); return;
    }
    auto queue = dispatch_queue_create("org.mitcad.renderer.connection", DISPATCH_QUEUE_SERIAL);
    auto commandAttributes = dispatch_queue_attr_make_with_qos_class(DISPATCH_QUEUE_SERIAL, QOS_CLASS_USER_INITIATED, 0);
    session->commands = dispatch_queue_create("org.mitcad.renderer.commands", commandAttributes);
    session->deadline = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, queue);
    std::weak_ptr<Session> weak = session;
    session->frames.event = [weak](const QJsonObject& event) { if (auto active = weak.lock()) active->event(event); };
    session->frames.failed = [weak] { if (auto active = weak.lock()) active->close(); };
    dispatch_source_set_event_handler(session->deadline, ^{
      if (auto active = weak.lock()) {
        const auto now = nowSeconds();
        const auto command = active->activeCommand.load();
        // A wedged driver/renderer must not retain sessions forever. Terminating
        // this renderer service reports failure to every affected document;
        // the GUI and its retained frame leases survive and may reconnect.
        if (command != 0 && now - command > 120) std::_Exit(70);
        if (now - active->lastReceived.load() > 30) active->close();
      }
    });
    dispatch_source_set_timer(session->deadline, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC), 5 * NSEC_PER_SEC, 0);
    dispatch_resume(session->deadline);
    xpc_connection_set_context(connection, new std::shared_ptr<Session>(session));
    xpc_connection_set_finalizer_f(connection, [](void* value) { delete static_cast<std::shared_ptr<Session>*>(value); });
    xpc_connection_set_target_queue(connection, queue);
    xpc_connection_set_event_handler(connection, ^(xpc_object_t message) {
      if (auto active = weak.lock()) active->received(message, active);
    });
    xpc_connection_activate(connection);
    dispatch_release(queue);
  }
}
} // namespace

int main(int argc, char** argv) {
  mitcad::crash::install("render-worker", MITCAD_VERSION);
  QCoreApplication application(argc, argv);
  QCoreApplication::setApplicationName(QStringLiteral("mitcad-render"));
  mitcad::mac::setThreadQuality(mitcad::mac::ThreadQuality::UserInitiated);
  const mitcad::mac::ProcessActivity activity("Rendering");
  xpc_main(accept);
}
