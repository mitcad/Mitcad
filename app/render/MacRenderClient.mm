// SPDX-License-Identifier: MIT
#include "render/MacRenderClient.hpp"
#include "render/MacRenderProtocol.hpp"
#include "render/MacCommandPayload.hpp"
#include "render/MacFrameControl.hpp"
#include "render/MacFrameSurface.hpp"
#include "render/MacXpcPeer.hpp"
#include "render/FrameMemory.hpp"
#include "render/RenderClient.hpp"
#include "framework/Diagnostics.hpp"

#include <Foundation/Foundation.h>
#include <IOSurface/IOSurfaceAPI.h>
#include <QJsonDocument>
#include <QCoreApplication>
#include <QEvent>
#include <QHash>
#include <QPointer>
#include <QTimer>
#include <atomic>
#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <new>
#include <mutex>
#include <utility>

namespace mitcad::render {
namespace {
// The file's texts in the context MacRenderClient, where lupdate records
// them too: MacRenderClient has no Q_OBJECT, so its own tr() is QObject's
// (mitcad#1474).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(MacRenderClient)
};

struct DispatchGate {
  std::mutex mutex;
  QObject* receiver = nullptr;
};

template<class Function>
void post(const std::shared_ptr<DispatchGate>& gate, Function function) {
  if (!gate) return;
  const std::lock_guard<std::mutex> guard(gate->mutex);
  if (gate->receiver) QMetaObject::invokeMethod(gate->receiver, std::move(function), Qt::QueuedConnection);
}

xpc_object_t message(const char* op) {
  auto result = xpc_dictionary_create_empty();
  xpc_dictionary_set_string(result, "op", op);
  xpc_dictionary_set_uint64(result, "version", kMacRenderProtocol);
  return result;
}
}
struct MacRenderClient::Impl {
  MacRenderClient* owner;
  xpc_connection_t connection = nullptr;
  Event event;
  Failure failure;
  QTimer heartbeat;
  QTimer startup;
  // Only the GUI client owns this QObject. Queued/XPC blocks retain a plain
  // mutex gate, never an owning QObject pointer or QObject deleter.
  QObject dispatcher;
  std::shared_ptr<DispatchGate> gate;
  std::shared_ptr<FrameMemory> control;
  std::shared_ptr<MacFrameSurfaces> surfaces;
  std::uint64_t generation = 0;
  std::uint64_t epoch = 0;
  std::size_t pendingBytes = 0;
  unsigned pendingRequests = 0;
  struct Request {
    xpc_object_t message = nullptr;
    std::shared_ptr<MacCommandPayload> payload;
    std::shared_ptr<std::atomic<bool>> answered = std::make_shared<std::atomic<bool>>(false);
    std::uint64_t id = 0;
    std::size_t bytes = 0;
    bool allocation = false;
    bool view = false;
    ~Request() { if (message) xpc_release(message); }
  };
  std::deque<std::shared_ptr<Request>> queued;
  std::uint64_t pendingBulkBytes = 0;
  bool bulkOutstanding = false;
  std::uint64_t nextRequest = 0;
  QHash<std::uint64_t, QTimer*> deadlines;
  bool allocationOutstanding = false;
  bool allocationSent = false;
  int desiredWidth = 0, desiredHeight = 0;
  bool greeted = false;
  bool waiting = false;
  QString lastServiceError;
  explicit Impl(MacRenderClient* object) : owner(object), dispatcher(object) {
    heartbeat.setInterval(5000);
    QObject::connect(&heartbeat, &QTimer::timeout, owner, [this] { submit(message("heartbeat"), 0); });
    startup.setSingleShot(true);
    startup.setInterval(120000); // A first Metal pipeline compilation may be slow, but remains bounded.
    QObject::connect(&startup, &QTimer::timeout, owner, [this] { fail(Text::tr("the render service did not become ready")); });
  }
  void fail(const QString& reason) {
    if (!connection) return;
    const auto report = failure;
    owner->stop();
    if (report) report(reason);
  }
  void removeDeadline(std::uint64_t id) {
    if (auto* timer = deadlines.take(id)) { timer->stop(); timer->deleteLater(); }
  }
  void completed(const std::shared_ptr<Request>& request) {
    request->answered->store(true); // Also disarm a coalesced queued view.
    removeDeadline(request->id);
    pendingBytes -= request->bytes;
    --pendingRequests;
    if (request->payload) pendingBulkBytes -= request->payload->size();
  }
  void flush() {
    // A bulk command is a FIFO barrier. Heartbeats and frame acknowledgements
    // bypass this queue below; they never mutate the scene or renderer setup.
    while (connection && !bulkOutstanding && !allocationSent && !queued.empty()) {
      auto request = std::move(queued.front());
      queued.pop_front();
      if (request->payload) bulkOutstanding = true;
      if (request->allocation) allocationSent = true;
      transmit(std::move(request));
    }
  }
  void transmit(std::shared_ptr<Request> request) {
    QPointer<MacRenderClient> guarded(owner);
    const auto sequence = epoch;
    const auto context = gate;
    xpc_connection_send_message_with_reply(connection, request->message, dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0),
        ^(xpc_object_t reply) {
          const bool accepted = xpc_get_type(reply) == XPC_TYPE_DICTIONARY && xpc_dictionary_get_bool(reply, "ok");
          request->answered->store(true);
          post(context, [guarded, sequence, request, accepted, context] {
            if (!guarded || guarded->m_impl->epoch != sequence || !guarded->m_impl->connection) return;
            auto& state = *guarded->m_impl;
            state.completed(request);
            if (request->payload) state.bulkOutstanding = false;
            if (!accepted) { state.fail(Text::tr("the render service rejected a command or disconnected")); return; }
            if (request->allocation) {
              state.allocationSent = false;
              state.allocationOutstanding = false;
              // Prepare the newest coalesced capacity before releasing queued
              // views. Allocations are transport barriers, not scene mutations.
              guarded->reserve(state.desiredWidth, state.desiredHeight);
            }
            state.flush();
          });
        });
  }
  void submit(xpc_object_t message, std::size_t bytes, bool allocation = false,
              std::shared_ptr<MacCommandPayload> payload = {}, bool view = false) {
    auto request = std::make_shared<Request>();
    request->message = message; // Consume even when the connection has stopped.
    request->payload = std::move(payload);
    request->bytes = bytes;
    request->allocation = allocation;
    request->view = view;
    if (!connection) return;
    // Only adjacent queued views are replaceable. Scene uploads introduce
    // incremental meshes and carry independent acknowledgements: never drop one.
    if (view && !queued.empty() && queued.back()->view) {
      completed(queued.back());
      queued.pop_back();
    }
    const auto bulkBytes = request->payload ? request->payload->size() : 0;
    if (bytes > kMacRenderMessageBytes || pendingBytes + bytes > kMacRenderPendingBytes || pendingRequests >= 1024 ||
        bulkBytes > kMacRenderPendingBulkBytes - pendingBulkBytes) {
      fail(Text::tr("the render command queue exceeded its bounded capacity")); return;
    }
    pendingBytes += bytes;
    pendingBulkBytes += bulkBytes;
    ++pendingRequests;
    request->id = ++nextRequest;
    QPointer<MacRenderClient> guarded(owner);
    const auto sequence = epoch;
    const auto answered = request->answered;
    auto* deadline = new QTimer(owner);
    deadline->setSingleShot(true);
    deadlines.insert(request->id, deadline);
    QObject::connect(deadline, &QTimer::timeout, owner, [guarded, sequence, answered] {
      if (guarded && guarded->m_impl->epoch == sequence && !answered->load())
        guarded->m_impl->fail(Text::tr("the render service command timed out"));
    });
    deadline->start(120000); // Includes bounded FIFO waiting, not just service time.
    const auto* op = xpc_dictionary_get_string(message, "op");
    if (op && (std::strcmp(op, "heartbeat") == 0 || std::strcmp(op, "consumed") == 0)) transmit(std::move(request));
    else {
      // A replacement allocation must precede views waiting for its capacity.
      // All scene/view mutations themselves retain their original FIFO order.
      if (allocation) queued.push_front(std::move(request));
      else queued.push_back(std::move(request));
      flush();
    }
  }
  void received(const QByteArray& json) {
    const auto document = QJsonDocument::fromJson(json);
    if (!document.isObject()) { fail(Text::tr("the render service sent an invalid event")); return; }
    const auto object = document.object();
    const auto name = object.value(QStringLiteral("event")).toString();
    if (name == QLatin1String("error")) lastServiceError = object.value(QStringLiteral("message")).toString();
    if (name == QLatin1String("hello")) {
      greeted = true;
      qInfo().noquote() << QStringLiteral("Render transport: IOSurface, private XPC session");
    }
    if (name == QLatin1String("ready")) startup.stop();
    if (name == QLatin1String("frame")) {
      if (object.value(QStringLiteral("memory")).toInteger() != static_cast<qint64>(generation) || !control) return;
      waiting = true;
    }
    if (event) event(object);
  }
};

MacRenderClient::MacRenderClient(QObject* parent) : QObject(parent), m_impl(std::make_unique<Impl>(this)) {}
MacRenderClient::~MacRenderClient() { stop(); }
bool MacRenderClient::start(Event event, Failure failure) {
  stop();
  auto& state = *m_impl;
  state.event = std::move(event);
  state.failure = std::move(failure);
  state.gate = std::make_shared<DispatchGate>();
  state.gate->receiver = &state.dispatcher;
  state.connection = xpc_connection_create(kMacRenderService, nullptr);
  if (!state.connection) return false;
  NSString* bundle = [NSBundle.mainBundle.bundlePath stringByAppendingPathComponent:@"Contents/XPCServices/org.mitcad.renderer.xpc"];
  std::string error;
  const bool valid = requirePeer(state.connection, bundle.fileSystemRepresentation, error);
  QPointer<MacRenderClient> guarded(this);
  const auto sequence = state.epoch;
  const auto context = state.gate;
  // Capture no owning Objective-C connection reference in its own handler.
  // XPC keeps the invoking connection alive for the duration of this callback.
  const auto peerAddress = reinterpret_cast<std::uintptr_t>(state.connection);
  xpc_connection_set_event_handler(state.connection, ^(xpc_object_t incoming) {
    const bool failed = xpc_get_type(incoming) == XPC_TYPE_ERROR;
    std::size_t bytes = 0;
    const void* data = xpc_get_type(incoming) == XPC_TYPE_DICTIONARY ? xpc_dictionary_get_data(incoming, "event", &bytes) : nullptr;
    const bool malformed = !failed && (!data || bytes > kMacRenderMessageBytes);
    QString reason;
    if (failed) {
      const char* description = xpc_dictionary_get_string(incoming, XPC_ERROR_KEY_DESCRIPTION);
      reason = QString::fromUtf8(description ? description : "unspecified XPC error");
      char* invalidation = xpc_connection_copy_invalidation_reason(reinterpret_cast<xpc_connection_t>(peerAddress));
      if (invalidation) { reason += QStringLiteral("; ") + QString::fromUtf8(invalidation); std::free(invalidation); }
    } else if (malformed) reason = QStringLiteral("unexpected XPC event object or payload size %1").arg(qulonglong(bytes));
    const QByteArray payload = data && !malformed ? QByteArray(static_cast<const char*>(data), static_cast<int>(bytes)) : QByteArray();
    post(context, [guarded, sequence, failed, malformed, payload, reason, context] {
      if (!guarded || guarded->m_impl->epoch != sequence || !guarded->m_impl->connection) return;
      auto& target = *guarded->m_impl;
      if (failed || malformed) {
        const auto detail = target.lastServiceError.isEmpty() ? reason : target.lastServiceError + QStringLiteral("; ") + reason;
        target.fail(Text::tr("the render service disconnected: %1").arg(detail));
      }
      else target.received(payload);
    });
  });
  xpc_connection_activate(state.connection);
  if (!valid) {
    const auto reason = QString::fromStdString(error);
    QMetaObject::invokeMethod(this, [this, sequence, reason] { if (m_impl->epoch == sequence) m_impl->fail(reason); }, Qt::QueuedConnection);
    return true; // Report consistently through the asynchronous failure callback.
  }
  state.heartbeat.start();
  state.startup.start();
  // A no-reply message owns the service's asynchronous rendering activity.
  // Ordinary acceptance replies end their boost before Cycles has finished.
  auto activity = message("activity");
  xpc_connection_send_message(state.connection, activity);
  xpc_release(activity);
  auto open = message("open");
  // XPC services do not inherit the GUI's environment. These existing testing
  // choices are session-local factory inputs, never service-wide setenv calls.
  const QJsonObject configuration{{QStringLiteral("denoiser"), qEnvironmentVariable("MITCAD_RENDER_TEST_DENOISER").left(1024)},
      {QStringLiteral("denoiser_failure"), qEnvironmentVariable("MITCAD_RENDER_TEST_DENOISER_FAILURE").left(1024)},
      {QStringLiteral("device_failure"), qEnvironmentVariable("MITCAD_RENDER_TEST_DEVICE_FAILURE").left(1024)},
      {QStringLiteral("native_pass_compare"), qEnvironmentVariableIntValue("MITCAD_RENDER_TEST_NATIVE_PASS_COMPARE") == 1},
      {QStringLiteral("command_trace"), qEnvironmentVariableIntValue("MITCAD_RENDER_TEST_COMMAND_TRACE") == 1}};
  const auto config = QJsonDocument(configuration).toJson(QJsonDocument::Compact);
  xpc_dictionary_set_data(open, "configuration", config.constData(), static_cast<std::size_t>(config.size()));
  state.submit(open, static_cast<std::size_t>(config.size()));
  return true;
}
void MacRenderClient::stop() {
  auto& state = *m_impl;
  ++state.epoch;
  const auto gate = std::exchange(state.gate, {});
  if (gate) {
    const std::lock_guard<std::mutex> guard(gate->mutex);
    gate->receiver = nullptr;
  }
  // No producer can enqueue after the gate closes. Removing old events here
  // also drops their plain captured state before a restart or GUI destruction.
  QCoreApplication::removePostedEvents(&state.dispatcher, QEvent::MetaCall);
  state.heartbeat.stop();
  state.startup.stop();
  if (state.connection) { xpc_connection_cancel(state.connection); xpc_release(state.connection); state.connection = nullptr; }
  // Displayed leases own their old mapping and surfaces independently.
  state.control.reset();
  state.surfaces.reset();
  state.greeted = state.waiting = false;
  state.pendingBytes = 0;
  state.pendingRequests = 0;
  state.pendingBulkBytes = 0;
  state.bulkOutstanding = false;
  state.queued.clear();
  for (auto* timer : std::as_const(state.deadlines)) { timer->stop(); timer->deleteLater(); }
  state.deadlines.clear();
  state.allocationOutstanding = false;
  state.allocationSent = false;
  state.desiredWidth = state.desiredHeight = 0;
  state.lastServiceError.clear();
}
bool MacRenderClient::isRunning() const { return m_impl->connection != nullptr; }
qint64 MacRenderClient::processId() const { return m_impl->connection ? xpc_connection_get_pid(m_impl->connection) : 0; }
void MacRenderClient::send(const QJsonObject& command) {
  auto& state = *m_impl;
  if (!state.connection || !state.greeted) return;
  const auto json = QJsonDocument(command).toJson(QJsonDocument::Compact);
  auto request = message("command");
  const bool view = command.value(QStringLiteral("cmd")).toString() == QLatin1String("view");
  if (static_cast<std::size_t>(json.size()) > kMacRenderMessageBytes) {
    // Check spool capacity before writing another potentially large file.
    if (static_cast<std::uint64_t>(json.size()) > kMacRenderPendingBulkBytes - state.pendingBulkBytes) {
      xpc_release(request); state.fail(Text::tr("the render command queue exceeded its bounded capacity")); return;
    }
    auto payload = std::make_shared<MacCommandPayload>();
    std::string error;
    if (!payload->create(json, error)) { xpc_release(request); state.fail(QString::fromStdString(error)); return; }
    xpc_dictionary_set_fd(request, "command_fd", payload->descriptor());
    xpc_dictionary_set_uint64(request, "command_bytes", payload->size());
    state.submit(request, 1024, false, std::move(payload), view);
  } else {
    xpc_dictionary_set_data(request, "command", json.constData(), static_cast<std::size_t>(json.size()));
    state.submit(request, static_cast<std::size_t>(json.size()), false, {}, view);
  }
}
void MacRenderClient::reserve(int width, int height) {
  auto& state = *m_impl;
  if (!state.connection || width <= 0 || height <= 0 || width > int(kMacFrameMaxDimension) || height > int(kMacFrameMaxDimension)) return;
  state.desiredWidth = width;
  state.desiredHeight = height;
  if (state.allocationOutstanding) return; // Coalesce resize wishes before allocating any more surface planes.
  if (state.control) {
    const auto* previous = static_cast<const MacFrameControl*>(state.control->data());
    if (std::uint32_t(width) <= previous->capacityWidth && std::uint32_t(height) <= previous->capacityHeight) return;
  }
  const auto capacityWidth = std::min(kMacFrameMaxDimension, (std::uint32_t(width) + std::uint32_t(width) / 4 + 7) & ~7u);
  const auto capacityHeight = std::min(kMacFrameMaxDimension, std::uint32_t(height) + std::uint32_t(height) / 4);
  if (!validMacFrameDimensions(capacityWidth, capacityHeight)) {
    state.fail(Text::tr("the rendered view exceeds the supported surface capacity")); return;
  }
  auto mapping = std::make_shared<FrameMemory>();
  auto surfaces = std::make_shared<MacFrameSurfaces>();
  std::string error;
  if (!mapping->create(sizeof(MacFrameControl), error) || !surfaces->create(capacityWidth, capacityHeight, error)) {
    state.fail(QString::fromStdString(error)); return;
  }
  auto* control = new (mapping->data()) MacFrameControl();
  control->capacityWidth = capacityWidth;
  control->capacityHeight = capacityHeight;
  control->generation = ++state.generation;
  auto request = message("allocate");
  xpc_dictionary_set_uint64(request, "generation", control->generation);
  xpc_dictionary_set_fd(request, "control", mapping->descriptor());
  auto array = xpc_array_create_empty();
  for (std::uint32_t slot = 0; slot < kFrameSlots; ++slot) for (std::uint32_t plane = 0; plane < kFramePlanes; ++plane) {
    auto object = IOSurfaceCreateXPCObject(surfaces->surface(slot, plane));
    xpc_array_append_value(array, object);
    xpc_release(object);
  }
  xpc_dictionary_set_value(request, "surfaces", array);
  xpc_release(array);
  state.control = std::move(mapping);
  state.surfaces = std::move(surfaces);
  state.waiting = false;
  state.allocationOutstanding = true;
  state.submit(request, sizeof(MacFrameControl), true);
}
bool MacRenderClient::takeFrame(Frame& frame) {
  auto& state = *m_impl;
  if (!state.waiting || !state.control || !state.surfaces) return false;
  state.waiting = false;
  ScopedTiming claim("render surface claim");
  frame.surface = claimMacFrame(state.surfaces, state.control, frame.info);
  if (!frame.surface) { state.fail(Text::tr("the render service published an invalid surface frame")); return false; }
  if (frame.surface && timingLogged()) claim.setDetail(QStringLiteral("view %1 samples %2 %3x%4 denoised %5")
      .arg(frame.info.view).arg(frame.info.samples).arg(frame.info.width).arg(frame.info.height).arg(frame.info.denoised));
  frame.pixels.clear();
  frame.catcher.clear();
  auto acknowledgement = message("consumed");
  xpc_dictionary_set_uint64(acknowledgement, "generation", state.generation);
  xpc_dictionary_set_uint64(acknowledgement, "publication", frame.surface->publication());
  state.submit(acknowledgement, 0);
  return frame.surface != nullptr;
}
} // namespace mitcad::render
