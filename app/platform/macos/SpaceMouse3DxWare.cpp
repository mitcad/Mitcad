// SPDX-License-Identifier: MIT
#include "SpaceMouse3DxWare.hpp"

#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <limits>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDebug>
#include <QGuiApplication>
#include <QLibrary>
#include <QWindow>

namespace mitcad {
namespace {

// Device-event API documented by 3DxWareMac SDK 1.0.4, sections 5 and 7.
// Only the user's installed runtime is loaded. The byte offsets are for
// the documented two-byte-aligned device-state message.
constexpr std::uint32_t kDeviceStateMessage = 0x33645352; // '3dSR'
constexpr std::uint32_t kReadDeviceId = 0x33646964;       // '3did'
// The client takes the device over: the driver passes its buttons to
// MitCad and runs no assignment of its own for them, so a labeled key
// (a QuickView key, Rotation Toggle) has one owner and acts once
// (mitcad#2642).
constexpr std::uint16_t kTakeOverAssignments = 1;
constexpr std::uint32_t kAxesAndButtons = 0x3fff;
constexpr std::uint16_t kButtonEvent = 2;
constexpr std::uint16_t kAxisEvent = 3;
constexpr std::size_t kClientOffset = 2;
constexpr std::size_t kCommandOffset = 4;
constexpr std::size_t kAxesOffset = 30;
constexpr std::size_t kButtonsOffset = 44;

template <typename T> T readValue(const char* packet, std::size_t offset) {
  T value{};
  std::memcpy(&value, packet + offset, sizeof(value));
  return value;
}

class DriverBackend;

class DriverSession {
public:
  static DriverSession& instance() {
    static DriverSession session;
    return session;
  }

  bool subscribe(DriverBackend* backend);
  void unsubscribe(DriverBackend* backend);
  QString deviceName() const;

private:
  using MessageHandler = void (*)(std::uint32_t, std::uint32_t, void*);
  using DeviceHandler = void (*)(std::uint32_t);
  using SetHandlers = std::int16_t (*)(MessageHandler, DeviceHandler, DeviceHandler, bool);
  using RegisterClient = std::uint16_t (*)(std::uint32_t, std::uint8_t*, std::uint16_t, std::uint32_t);
  using UnregisterClient = void (*)(std::uint16_t);
  using CleanUpHandlers = void (*)();
  using SetButtonMask = void (*)(std::uint16_t, std::uint32_t);
  using ClientControl = std::int16_t (*)(std::uint16_t, std::uint32_t, std::int32_t, std::int32_t*);

  bool open();
  void dispatch(const char* packet);
  void deviceChanged();
  void resetInput();
  static void receive(std::uint32_t, std::uint32_t message, void* data);
  static void deviceEvent(std::uint32_t);

  QLibrary m_library{
      QStringLiteral("/Library/Frameworks/3DconnexionClient.framework/3DconnexionClient")};
  UnregisterClient m_unregister = nullptr;
  CleanUpHandlers m_cleanup = nullptr;
  ClientControl m_control = nullptr;
  std::uint16_t m_client = 0;
  std::array<std::uint8_t, 256> m_executableName{};
  std::vector<DriverBackend*> m_backends;
  QMetaObject::Connection m_appState;
  QMetaObject::Connection m_focusWindow;
};

class DriverBackend final : public SpaceMouse3DxWareBackend {
public:
  explicit DriverBackend(QString hidName) : m_hidName(std::move(hidName)) {}

  ~DriverBackend() override {
    if (m_subscribed) {
      DriverSession::instance().unsubscribe(this);
    }
  }

  QString name() const override { return QStringLiteral("3DxWare"); }
  bool isListening() const override { return m_subscribed; }

  bool start(QString& device) override {
    auto& session = DriverSession::instance();
    m_subscribed = session.subscribe(this);
    if (m_subscribed) {
      m_device = session.deviceName();
    }
    device = m_device;
    return !device.isEmpty();
  }

  bool canReceive() const {
    return QGuiApplication::applicationState() == Qt::ApplicationActive &&
           (!acceptsInput || acceptsInput());
  }

  void input(const SpaceMouseAxes& axes) {
    if (m_device.isEmpty() && axes != SpaceMouseAxes{}) {
      setDevice(m_hidName);
    }
    m_moving = axes != SpaceMouseAxes{};
    if (motion) {
      motion(axes);
    }
  }

  void buttons(std::uint32_t mask) {
    if (m_device.isEmpty() && mask != 0) {
      setDevice(m_hidName);
    }
    if (button) {
      spaceMouseButtonChanges(m_buttons, mask, button);
    }
    m_buttons = mask;
  }

  void reset() {
    if (m_moving) {
      input({});
    }
    m_buttons = 0;
  }

  void setDevice(const QString& device) {
    if (m_device == device) {
      return;
    }
    const QString previous = m_device;
    m_device = device;
    reset();
    if (connection) {
      connection(!device.isEmpty(), device.isEmpty() ? previous : device);
    }
  }

private:
  QString m_hidName;
  QString m_device;
  std::uint32_t m_buttons = 0;
  bool m_moving = false;
  bool m_subscribed = false;
};

bool DriverSession::open() {
  m_library.setLoadHints(QLibrary::ResolveAllSymbolsHint | QLibrary::PreventUnloadHint);
  if (!m_library.isLoaded() && !m_library.load()) {
    qDebug().noquote() << "3D mouse: 3DxWare framework unavailable:" << m_library.errorString();
    return false;
  }
  const auto setHandlers = reinterpret_cast<SetHandlers>(m_library.resolve("SetConnexionHandlers"));
  const auto registerClient = reinterpret_cast<RegisterClient>(m_library.resolve("RegisterConnexionClient"));
  const auto setButtonMask = reinterpret_cast<SetButtonMask>(m_library.resolve("SetConnexionClientButtonMask"));
  m_unregister = reinterpret_cast<UnregisterClient>(m_library.resolve("UnregisterConnexionClient"));
  m_cleanup = reinterpret_cast<CleanUpHandlers>(m_library.resolve("CleanupConnexionHandlers"));
  m_control = reinterpret_cast<ClientControl>(m_library.resolve("ConnexionClientControl"));
  if (!setHandlers || !registerClient || !setButtonMask || !m_unregister || !m_cleanup || !m_control) {
    qWarning("3D mouse: the installed 3DxWare framework has an incomplete device-event API");
    return false;
  }

  // Request main-thread callbacks. The driver looks up the foreground app
  // using the executable name when no creator signature is supplied.
  const auto status = setHandlers(&DriverSession::receive, &DriverSession::deviceEvent,
                                  &DriverSession::deviceEvent, false);
  if (status != 0) {
    qWarning("3D mouse: 3DxWare could not install its event handlers (%d)", static_cast<int>(status));
    return false;
  }
  const QByteArray executable = QCoreApplication::applicationFilePath().section(QLatin1Char('/'), -1).toUtf8().left(255);
  m_executableName[0] = static_cast<std::uint8_t>(executable.size());
  std::memcpy(m_executableName.data() + 1, executable.constData(), static_cast<std::size_t>(executable.size()));
  m_client = registerClient(0, m_executableName.data(), kTakeOverAssignments, kAxesAndButtons);
  if (m_client == 0) {
    m_cleanup();
    qWarning("3D mouse: 3DxWare did not register the application");
    return false;
  }
  setButtonMask(m_client, std::numeric_limits<std::uint32_t>::max());
  qDebug("3D mouse: registered a 3DxWare client");
  return true;
}

bool DriverSession::subscribe(DriverBackend* backend) {
  if (m_backends.empty()) {
    if (!open()) {
      return false;
    }
    m_appState = QObject::connect(qGuiApp, &QGuiApplication::applicationStateChanged, qGuiApp,
                                  [this] { resetInput(); });
    m_focusWindow = QObject::connect(qGuiApp, &QGuiApplication::focusWindowChanged, qGuiApp,
                                     [this] { resetInput(); });
  }
  m_backends.push_back(backend);
  return true;
}

void DriverSession::unsubscribe(DriverBackend* backend) {
  m_backends.erase(std::remove(m_backends.begin(), m_backends.end(), backend), m_backends.end());
  if (m_backends.empty() && m_client != 0) {
    QObject::disconnect(m_focusWindow);
    QObject::disconnect(m_appState);
    const auto oldClient = m_client;
    m_client = 0;
    m_unregister(oldClient);
    m_cleanup();
  }
}

QString DriverSession::deviceName() const {
  std::int32_t id = 0;
  if (m_client == 0 || m_control(m_client, kReadDeviceId, 0, &id) != 0 || id == 0) {
    return {};
  }
  const auto device = static_cast<std::uint32_t>(id);
  return QStringLiteral("HID %1:%2").arg(device >> 16, 4, 16, QLatin1Char('0'))
                                      .arg(device & 0xffff, 4, 16, QLatin1Char('0'));
}

void DriverSession::receive(std::uint32_t, std::uint32_t message, void* data) {
  if (message == kDeviceStateMessage && data) {
    instance().dispatch(static_cast<const char*>(data));
  }
}

void DriverSession::deviceEvent(std::uint32_t) {
  // Give the driver time to finish updating its connected-device state.
  QMetaObject::invokeMethod(qGuiApp, [] { instance().deviceChanged(); }, Qt::QueuedConnection);
}

void DriverSession::deviceChanged() {
  const QString device = deviceName();
  for (auto* backend : m_backends) {
    backend->setDevice(device);
  }
}

void DriverSession::resetInput() {
  for (auto* backend : m_backends) {
    backend->reset();
  }
}

void DriverSession::dispatch(const char* packet) {
  if (m_client == 0 || readValue<std::uint16_t>(packet, kClientOffset) != m_client) {
    return;
  }
  const auto command = readValue<std::uint16_t>(packet, kCommandOffset);
  if (command == kAxisEvent) {
    std::array<std::int16_t, 6> raw{};
    std::memcpy(raw.data(), packet + kAxesOffset, sizeof(raw));
    const auto axes = spaceMouseFromHid({raw[0], raw[1], raw[2]}, {raw[3], raw[4], raw[5]});
    for (auto* backend : m_backends) {
      if (backend->canReceive()) {
        backend->input(axes);
      }
    }
  } else if (command == kButtonEvent) {
    const auto mask = readValue<std::uint32_t>(packet, kButtonsOffset);
    for (auto* backend : m_backends) {
      if (backend->canReceive()) {
        backend->buttons(mask);
      }
    }
  }
}

} // namespace

std::unique_ptr<SpaceMouse3DxWareBackend> create3DxWareSpaceMouseBackend(QString hidName) {
  return std::make_unique<DriverBackend>(std::move(hidName));
}

} // namespace mitcad
