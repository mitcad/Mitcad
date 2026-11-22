// SPDX-License-Identifier: MIT
#include "SpaceMouse3DxWare.hpp"

#include "../SpaceMouseDriverReports.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>

#include <QAbstractNativeEventFilter>
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QGuiApplication>
#include <QTimer>
#include <QWindow>

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <shlobj.h>

namespace mitcad {
namespace {

// Runtime interface of the installed 3DxWare SiApp device-event API.
// The driver writes the request and event storage. The event fields used
// below are the fixed-width fields shared by the 32- and 64-bit interfaces.
constexpr int kAnyDevice = -1;
constexpr int kReceiveMessages = 1;
constexpr int kEventAvailable = 5;
constexpr int kNotEvent = 7;
constexpr int kDriverStopped = 8;
constexpr int kDeviceClosed = 17;

// The test's stand-in for the runtime (MITCAD_TEST_3DXWARE=1, mitcad#2152):
// it loads as the installed runtime does and reports one device, "Test 3D
// mouse", so the backend asks for MitCad's window at the moment a real
// driver start does; it opens that device only for a window, and sends no
// events. The VM has no 3DxWare.
namespace standin {
HWND openedFor = nullptr;
int openedDevice = 0; // the open device's handle points here

int WINAPI initialize() { return 0; }
void WINAPI terminate() {}
void WINAPI initWindow(void*, HWND window) { openedFor = window; }
void* WINAPI open(char*, int, void*, int, void*) {
  qInfo().noquote() << (openedFor != nullptr ? QStringLiteral("3D mouse: test 3DxWare runtime opened its device")
                                             : QStringLiteral("3D mouse: test 3DxWare runtime got no window"));
  return openedFor != nullptr ? &openedDevice : nullptr;
}
int WINAPI close(void*) { return 0; }
void WINAPI initEvent(void*, UINT, WPARAM, LPARAM) {}
int WINAPI getEvent(void*, int, void*, void*) { return kNotEvent; }
int WINAPI deviceCount() { return 1; }
int WINAPI deviceName(void*, void* name) {
  // The device name record's first field: its text (at most 512 bytes).
  static constexpr char kName[] = "Test 3D mouse";
  std::memcpy(name, kName, sizeof(kName));
  return 0;
}
} // namespace standin

HMODULE loadDriver() {
  if (const HMODULE module = LoadLibraryExW(L"siappdll.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32)) {
    return module;
  }
  wchar_t directory[MAX_PATH]{};
  if (FAILED(SHGetFolderPathW(nullptr, CSIDL_PROGRAM_FILES, nullptr, SHGFP_TYPE_CURRENT, directory))) {
    return nullptr;
  }
  const QDir programs(QString::fromWCharArray(directory));
  for (const auto& subdirectory : {QStringLiteral("3DxWinCore64"), QStringLiteral("3DxWinCore")}) {
    const auto file = QDir::toNativeSeparators(
        programs.filePath(QStringLiteral("3Dconnexion/3DxWare/%1/siappdll.dll").arg(subdirectory))).toStdWString();
    if (const HMODULE module = LoadLibraryExW(
            file.c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32)) {
      return module;
    }
  }
  return nullptr;
}

class SiRuntime {
public:
  static std::shared_ptr<SiRuntime> acquire();

  ~SiRuntime() {
    if (m_initialized) {
      terminate();
    }
    if (m_module) {
      FreeLibrary(m_module);
    }
  }

  void (WINAPI* initWindow)(void*, HWND) = nullptr;
  void* (WINAPI* open)(char*, int, void*, int, void*) = nullptr;
  int (WINAPI* close)(void*) = nullptr;
  void (WINAPI* initEvent)(void*, UINT, WPARAM, LPARAM) = nullptr;
  int (WINAPI* getEvent)(void*, int, void*, void*) = nullptr;
  int (WINAPI* deviceCount)() = nullptr;
  int (WINAPI* deviceName)(void*, void*) = nullptr;

private:
  template <class Function> Function resolve(const char* name) const {
    const FARPROC address = GetProcAddress(m_module, name);
    Function function = nullptr;
    static_assert(sizeof(function) == sizeof(address));
    std::memcpy(&function, &address, sizeof(function));
    return function;
  }

  bool start();
  bool startStandIn();

  HMODULE m_module = nullptr;
  void (WINAPI* terminate)() = nullptr;
  bool m_initialized = false;
};

bool SiRuntime::startStandIn() {
  qInfo().noquote() << QStringLiteral("3D mouse: test 3DxWare runtime (MITCAD_TEST_3DXWARE) reports one device");
  terminate = &standin::terminate;
  initWindow = &standin::initWindow;
  open = &standin::open;
  close = &standin::close;
  initEvent = &standin::initEvent;
  getEvent = &standin::getEvent;
  deviceCount = &standin::deviceCount;
  deviceName = &standin::deviceName;
  m_initialized = standin::initialize() == 0;
  return m_initialized;
}

bool SiRuntime::start() {
  if (qEnvironmentVariableIntValue("MITCAD_TEST_3DXWARE") == 1) {
    return startStandIn();
  }
  m_module = loadDriver();
  if (!m_module) {
    return false;
  }
  const auto initialize = resolve<int (WINAPI*)()>("SiInitialize");
  terminate = resolve<decltype(terminate)>("SiTerminate");
  initWindow = resolve<decltype(initWindow)>("SiOpenWinInit");
  open = resolve<decltype(open)>("SiOpen");
  close = resolve<decltype(close)>("SiClose");
  initEvent = resolve<decltype(initEvent)>("SiGetEventWinInit");
  getEvent = resolve<decltype(getEvent)>("SiGetEvent");
  deviceCount = resolve<decltype(deviceCount)>("SiGetNumDevices");
  deviceName = resolve<decltype(deviceName)>("SiGetDeviceName");
  m_initialized = initialize && terminate && initWindow && open && close &&
                  initEvent && getEvent && deviceCount && initialize() == 0;
  return m_initialized;
}

std::shared_ptr<SiRuntime> SiRuntime::acquire() {
  static std::weak_ptr<SiRuntime> shared;
  if (auto runtime = shared.lock()) {
    return runtime;
  }
  auto runtime = std::make_shared<SiRuntime>();
  if (!runtime->start()) {
    return {};
  }
  shared = runtime;
  return runtime;
}

class WindowsDriverBackend final : public SpaceMouseBackend, private QAbstractNativeEventFilter {
public:
  ~WindowsDriverBackend() override {
    if (m_device) {
      if (auto* app = QCoreApplication::instance()) {
        app->removeNativeEventFilter(this);
      }
      m_runtime->close(m_device);
    }
  }

  QString name() const override { return QStringLiteral("3DxWare"); }

  bool start(QString& device) override {
    m_runtime = SiRuntime::acquire();
    if (!m_runtime || m_runtime->deviceCount() <= 0 || !nativeWindow) {
      return false;
    }
    // An existing window only (mitcad#2152): none before the design is
    // shown in its frame; the Windows backend tries again then.
    m_window = reinterpret_cast<HWND>(nativeWindow());
    if (!m_window) {
      qInfo().noquote() << QStringLiteral("3D mouse: 3DxWare has a device, MitCad's window is not shown yet");
      return false;
    }
    alignas(std::max_align_t) std::array<std::byte, 1024> request{};
    m_runtime->initWindow(request.data(), m_window);
    char appName[] = "MitCad";
    m_device = m_runtime->open(appName, kAnyDevice, nullptr, kReceiveMessages, request.data());
    if (!m_device) {
      return false;
    }

    std::array<char, 512> product{};
    if (m_runtime->deviceName && m_runtime->deviceName(m_device, product.data()) == 0) {
      product.back() = '\0';
      m_label = QString::fromLocal8Bit(product.data()).trimmed();
    }
    if (m_label.isEmpty()) {
      m_label = QCoreApplication::translate("SpaceMouseBackend", "3D mouse");
    }
    m_label = QCoreApplication::translate("SpaceMouseBackend", "%1 via 3DxWare").arg(m_label);
    device = m_label;
    QCoreApplication::instance()->installNativeEventFilter(this);
    QObject::connect(qGuiApp, &QGuiApplication::focusWindowChanged, &m_health, [this] { stopMotion(); });
    QObject::connect(qGuiApp, &QGuiApplication::applicationStateChanged, &m_health, [this] { stopMotion(); });
    QObject::connect(&m_health, &QTimer::timeout, &m_health, [this] { checkDevice(); });
    m_health.start(2500);
    return true;
  }

private:
  void stopMotion() {
    if (m_moving && motion) {
      motion({});
    }
    m_moving = false;
  }

  void checkDevice() {
    if (m_runtime->deviceCount() <= 0) {
      deviceLost();
    }
  }

  void deviceLost() {
    m_health.stop();
    m_connected = false;
    stopMotion();
    if (connection) {
      connection(false, m_label);
    }
  }

  void reportButtons(std::uint32_t current, bool active) {
    const auto previous = m_buttons;
    m_buttons = current;
    if (active && button) {
      spaceMouseButtonChanges(previous, current, button);
    }
  }

  bool nativeEventFilter(const QByteArray& eventType, void* message, qintptr* result) override {
    if (!m_connected || (eventType != "windows_generic_MSG" && eventType != "windows_dispatcher_MSG")) {
      return false;
    }
    const auto* event = static_cast<const MSG*>(message);
    if (event->hwnd != m_window) {
      return false;
    }
    alignas(std::max_align_t) std::array<std::byte, 64> request{};
    alignas(std::max_align_t) std::array<std::byte, 8192> data{};
    m_runtime->initEvent(request.data(), event->message, event->wParam, event->lParam);
    const int status = m_runtime->getEvent(m_device, 0, request.data(), data.data());
    if (status == kDriverStopped || status == kDeviceClosed) {
      deviceLost();
    }
    if (status != kEventAvailable) {
      return false;
    }

    const bool active = QGuiApplication::applicationState() == Qt::ApplicationActive &&
                        (!acceptsInput || acceptsInput());
    const auto decoded = parseSiAppDeviceEvent(data.data(), data.size());
    using Kind = SiAppDeviceEvent::Kind;
    switch (decoded.kind) {
    case Kind::Motion:
      if (active && motion) {
        const auto mapped = spaceMouseFromSpnav(decoded.axes);
        m_moving = mapped != SpaceMouseAxes{};
        motion(mapped);
      }
      break;
    case Kind::Rest:
      stopMotion();
      break;
    case Kind::Buttons:
      reportButtons(decoded.buttons, active);
      break;
    case Kind::ButtonDown:
    case Kind::ButtonUp: {
      const std::uint32_t bit = std::uint32_t{1} << decoded.button;
      reportButtons(decoded.kind == Kind::ButtonDown ? m_buttons | bit : m_buttons & ~bit, active);
      break;
    }
    case Kind::DeviceChange:
      checkDevice();
      break;
    case Kind::Other:
      return false;
    }
    if (result) {
      *result = 0;
    }
    return true;
  }

  std::shared_ptr<SiRuntime> m_runtime;
  HWND m_window = nullptr;
  void* m_device = nullptr;
  QTimer m_health;
  QString m_label;
  std::uint32_t m_buttons = 0;
  bool m_moving = false;
  bool m_connected = true;
};

} // namespace

std::unique_ptr<SpaceMouseBackend> createWindows3DxWareBackend() {
  return std::make_unique<WindowsDriverBackend>();
}

} // namespace mitcad
