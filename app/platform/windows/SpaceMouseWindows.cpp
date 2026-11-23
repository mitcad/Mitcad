// SPDX-License-Identifier: MIT
// 3D mice on Windows (mitcad#951): Raw Input reads HID reports when
// 3DxWare is unavailable. A message-only window of its own registers for the
// multi-axis controllers (HID usage page 1, usage 8) with RIDEV_INPUTSINK,
// so that it gets them whichever of Mitcad's windows has the focus, and
// RIDEV_DEVNOTIFY for the devices that come and go; its window procedure
// reads them as Qt's event loop dispatches them. Input counts only while
// Mitcad is the foreground application (RIM_INPUT). The reports' parsing
// and their axes are SpaceMouseDevice.hpp's (SpaceMouseHidReader: where a
// device sends its rotation, mitcad#1034). Windows keeps one Raw Input
// target per device class and process, and RIDEV_REMOVE removes it for the
// whole process: the backend is the process's one (SpaceMouseConnection,
// mitcad#2297), never one per design.
//
// The debug log (mitcad#1034): at a device's first report, its name with
// its vendor and product ids, the size of its reports (dwSizeHid), its
// longest input report and where its report descriptor puts the rotation;
// the report ids it sends as each new one comes; while it moves, its raw
// reports, at most ten lines a second (SpaceMouseRawLog).
//
// The driver's emulated input (mitcad#1981): its profile can turn a cap's
// motion into wheel steps or keys, sent through its "KMJ Emulator" device.
// Only the mouse's own Raw Input would name that device for a wheel step
// (WM_MOUSEWHEEL names none): registering for every mouse's raw input
// would bring a message per mouse move and take over the process's mouse
// registration, so the view does not tell the emulator apart; it ignores
// the wheel during a motion instead (OcctViewer::wheelEvent).

#include "../SpaceMouseDevice.hpp"
#include "SpaceMouse3DxWare.hpp"

#include <cstddef>
#include <cstdint>
#include <map>
#include <memory>
#include <vector>

#include <QDebug>
#include <QElapsedTimer>
#include <QStringList>
#include <QTimer>

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <hidsdi.h>
#include <hidpi.h>
#include <tlhelp32.h>

namespace mitcad {
namespace {

constexpr USHORT kGenericDesktop = 0x01;
constexpr USHORT kMultiAxisController = 0x08;
constexpr USAGE kUsageRx = 0x33;
constexpr wchar_t kWindowClass[] = L"MitcadSpaceMouse";

bool isSpaceMouse(HANDLE device) {
  RID_DEVICE_INFO info{};
  info.cbSize = sizeof(info);
  UINT size = sizeof(info);
  if (GetRawInputDeviceInfoW(device, RIDI_DEVICEINFO, &info, &size) == static_cast<UINT>(-1) ||
      info.dwType != RIM_TYPEHID) {
    return false;
  }
  return info.hid.usUsagePage == kGenericDesktop && info.hid.usUsage == kMultiAxisController;
}

// "SpaceMouse Compact (HID 256f:c635)": the product string when the device
// gives one, and its vendor and product ids.
QString deviceName(HANDLE device) {
  RID_DEVICE_INFO info{};
  info.cbSize = sizeof(info);
  UINT size = sizeof(info);
  QString ids;
  if (GetRawInputDeviceInfoW(device, RIDI_DEVICEINFO, &info, &size) != static_cast<UINT>(-1) &&
      info.dwType == RIM_TYPEHID) {
    ids = QStringLiteral("HID %1:%2")
              .arg(static_cast<uint>(info.hid.dwVendorId), 4, 16, QLatin1Char('0'))
              .arg(static_cast<uint>(info.hid.dwProductId), 4, 16, QLatin1Char('0'));
  }
  QString product;
  UINT chars = 0;
  if (GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, nullptr, &chars) == 0 && chars > 0) {
    std::vector<wchar_t> path(static_cast<std::size_t>(chars) + 1, L'\0');
    if (GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, path.data(), &chars) != static_cast<UINT>(-1)) {
      // No access asked: enough for the device's strings.
      const HANDLE file = CreateFileW(path.data(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0,
                                      nullptr);
      if (file != INVALID_HANDLE_VALUE) {
        wchar_t text[127] = {};
        if (HidD_GetProductString(file, text, static_cast<ULONG>(sizeof(text)))) {
          product = QString::fromWCharArray(text).trimmed();
        }
        CloseHandle(file);
      }
    }
  }
  if (product.isEmpty()) {
    return ids.isEmpty() ? QStringLiteral("HID multi-axis controller") : ids;
  }
  return ids.isEmpty() ? product : QStringLiteral("%1 (%2)").arg(product, ids);
}

// Where the device's report descriptor puts the rotation (the usage Rx of
// the generic desktop page): in report 1 with the translation or in a
// report of its own; `inputLength` the longest input report in bytes (the
// report id included), 0 when the descriptor cannot be read.
SpaceMouseRotationReport rotationReportOf(HANDLE device, USHORT& inputLength) {
  inputLength = 0;
  UINT size = 0;
  if (GetRawInputDeviceInfoW(device, RIDI_PREPARSEDDATA, nullptr, &size) != 0 || size == 0) {
    return SpaceMouseRotationReport::Unknown;
  }
  std::vector<std::uint64_t> preparsedData((static_cast<std::size_t>(size) + 7) / 8, 0);
  if (GetRawInputDeviceInfoW(device, RIDI_PREPARSEDDATA, preparsedData.data(), &size) == static_cast<UINT>(-1)) {
    return SpaceMouseRotationReport::Unknown;
  }
  const auto preparsed = reinterpret_cast<PHIDP_PREPARSED_DATA>(preparsedData.data());
  HIDP_CAPS caps{};
  if (HidP_GetCaps(preparsed, &caps) != HIDP_STATUS_SUCCESS) {
    return SpaceMouseRotationReport::Unknown;
  }
  inputLength = caps.InputReportByteLength;
  USHORT count = caps.NumberInputValueCaps;
  if (count == 0) {
    return SpaceMouseRotationReport::Unknown;
  }
  std::vector<HIDP_VALUE_CAPS> values(count);
  if (HidP_GetValueCaps(HidP_Input, values.data(), &count, preparsed) != HIDP_STATUS_SUCCESS) {
    return SpaceMouseRotationReport::Unknown;
  }
  for (USHORT index = 0; index < count && index < values.size(); ++index) {
    const HIDP_VALUE_CAPS& value = values[index];
    if (value.UsagePage != kGenericDesktop) {
      continue;
    }
    const USAGE first = value.IsRange ? value.Range.UsageMin : value.NotRange.Usage;
    const USAGE last = value.IsRange ? value.Range.UsageMax : value.NotRange.Usage;
    if (first <= kUsageRx && kUsageRx <= last) {
      return value.ReportID == 1 ? SpaceMouseRotationReport::WithTranslation : SpaceMouseRotationReport::OwnReport;
    }
  }
  return SpaceMouseRotationReport::Unknown;
}

class RawInputBackend final : public SpaceMouseBackend {
public:
  ~RawInputBackend() override {
    if (m_registered) {
      RAWINPUTDEVICE device{kGenericDesktop, kMultiAxisController, RIDEV_REMOVE, nullptr};
      RegisterRawInputDevices(&device, 1, static_cast<UINT>(sizeof(device)));
    }
    if (m_window != nullptr) {
      SetWindowLongPtrW(m_window, GWLP_USERDATA, 0);
      DestroyWindow(m_window);
    }
  }

  QString name() const override { return QStringLiteral("Raw Input"); }

  bool start(QString& device) override {
    m_clock.start();
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    WNDCLASSEXW windowClass{};
    windowClass.cbSize = sizeof(windowClass);
    windowClass.lpfnWndProc = &RawInputBackend::windowProcedure;
    windowClass.hInstance = instance;
    windowClass.lpszClassName = kWindowClass;
    // A second backend finds the class registered already.
    if (RegisterClassExW(&windowClass) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS) {
      qWarning().noquote() << QStringLiteral("3D mouse: Raw Input's window class failed (%1)").arg(GetLastError());
      return false;
    }
    m_window = CreateWindowExW(0, kWindowClass, L"", 0, 0, 0, 0, 0, HWND_MESSAGE, nullptr, instance, nullptr);
    if (m_window == nullptr) {
      qWarning().noquote() << QStringLiteral("3D mouse: Raw Input's window failed (%1)").arg(GetLastError());
      return false;
    }
    SetWindowLongPtrW(m_window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(this));
    RAWINPUTDEVICE wanted{kGenericDesktop, kMultiAxisController, RIDEV_INPUTSINK | RIDEV_DEVNOTIFY, m_window};
    if (!RegisterRawInputDevices(&wanted, 1, static_cast<UINT>(sizeof(wanted)))) {
      qWarning().noquote() << QStringLiteral("3D mouse: Raw Input's registration failed (%1)").arg(GetLastError());
      return false;
    }
    m_registered = true;
    // The devices there now; the arrivals that registering reports for
    // them too are known then.
    UINT count = 0;
    if (GetRawInputDeviceList(nullptr, &count, static_cast<UINT>(sizeof(RAWINPUTDEVICELIST))) == 0 && count > 0) {
      std::vector<RAWINPUTDEVICELIST> list(count);
      const UINT listed = GetRawInputDeviceList(list.data(), &count, static_cast<UINT>(sizeof(RAWINPUTDEVICELIST)));
      if (listed != static_cast<UINT>(-1)) {
        for (UINT index = 0; index < listed; ++index) {
          if (list[index].dwType == RIM_TYPEHID && isSpaceMouse(list[index].hDevice)) {
            m_devices[list[index].hDevice] = deviceName(list[index].hDevice);
          }
        }
      }
    }
    if (m_devices.empty()) {
      return false;
    }
    QStringList names;
    for (const auto& entry : m_devices) {
      names.append(entry.second);
    }
    device = names.join(QStringLiteral(", "));
    return true;
  }

private:
  struct State {
    SpaceMouseHidReader reader;
    SpaceMouseRawLog rawLog;
    std::uint32_t pressed = 0;
    bool moving = false;
    bool described = false;
  };

  static LRESULT CALLBACK windowProcedure(HWND window, UINT message, WPARAM wParam, LPARAM lParam) {
    auto* backend = reinterpret_cast<RawInputBackend*>(GetWindowLongPtrW(window, GWLP_USERDATA));
    if (backend != nullptr) {
      if (message == WM_INPUT) {
        backend->input(wParam, reinterpret_cast<HRAWINPUT>(lParam));
      } else if (message == WM_INPUT_DEVICE_CHANGE) {
        backend->deviceChanged(wParam, reinterpret_cast<HANDLE>(lParam));
      }
    }
    // WM_INPUT's clean-up is DefWindowProc's.
    return DefWindowProcW(window, message, wParam, lParam);
  }

  void input(WPARAM code, HRAWINPUT handle) {
    UINT size = 0;
    if (GetRawInputData(handle, RID_INPUT, nullptr, &size, static_cast<UINT>(sizeof(RAWINPUTHEADER))) != 0 ||
        size < sizeof(RAWINPUTHEADER)) {
      return;
    }
    // RAWINPUT wants 8-byte alignment on 64-bit Windows.
    m_buffer.assign((static_cast<std::size_t>(size) + 7) / 8, 0);
    if (GetRawInputData(handle, RID_INPUT, m_buffer.data(), &size, static_cast<UINT>(sizeof(RAWINPUTHEADER))) ==
        static_cast<UINT>(-1)) {
      return;
    }
    const auto* raw = reinterpret_cast<const RAWINPUT*>(m_buffer.data());
    if (raw->header.dwType != RIM_TYPEHID) {
      return;
    }
    const auto* base = reinterpret_cast<const std::uint8_t*>(m_buffer.data());
    const auto* reports = reinterpret_cast<const std::uint8_t*>(raw->data.hid.bRawData);
    const std::size_t reportSize = raw->data.hid.dwSizeHid;
    const std::size_t reportCount = raw->data.hid.dwCount;
    if (reportSize == 0 || static_cast<std::size_t>(reports - base) + reportSize * reportCount > size) {
      return;
    }
    const bool foreground = GET_RAWINPUT_CODE_WPARAM(code) == RIM_INPUT;
    const HANDLE source = raw->header.hDevice;
    State& state = m_states[source];
    if (!state.described) {
      describe(source, state, reportSize);
    }
    const std::size_t idsBefore = state.reader.reportIds().size();
    bool moved = false;
    for (std::size_t index = 0; index < reportCount; ++index) {
      const std::uint8_t* data = reports + index * reportSize;
      state.rawLog.add(data, reportSize);
      SpaceMouseHidReport report;
      if (!state.reader.read(data, reportSize, report)) {
        continue;
      }
      if (report.translation || report.rotation) {
        moved = true;
      }
      if (report.buttons) {
        const std::uint32_t before = state.pressed;
        state.pressed = report.pressed;
        if (foreground && button) {
          spaceMouseButtonChanges(before, report.pressed, [this](int which, bool down) { button(which, down); });
        }
      }
    }
    if (state.reader.reportIds().size() != idsBefore) {
      qDebug().noquote() << QStringLiteral("3D mouse: %1 sends report ids %2, its rotation %3")
                                .arg(label(source), state.reader.reportIdsText(),
                                     QString::fromLatin1(spaceMouseRotationReportText(state.reader.rotationReport())));
    }
    if (!moved) {
      return;
    }
    const SpaceMouseAxes axes = state.reader.axes();
    // The raw reports while the cap moves, thinned, and the last ones when
    // it comes to rest.
    const QString line = state.rawLog.take(m_clock.elapsed(), state.reader.translation(), state.reader.rotation(),
                                           state.moving && axes == SpaceMouseAxes{});
    if (!line.isEmpty()) {
      qDebug().noquote() << line;
    }
    if (!motion) {
      return;
    }
    if (foreground) {
      state.moving = axes != SpaceMouseAxes{};
      motion(axes);
    } else if (state.moving) {
      // Another application has the device now.
      state.moving = false;
      motion(SpaceMouseAxes{});
    }
  }

  // The device's name in the log: its product and ids.
  QString label(HANDLE device) const {
    const auto found = m_devices.find(device);
    return found != m_devices.end() ? found->second : deviceName(device);
  }

  // A device's first report: how it reports, once in the debug log, and
  // where its descriptor puts the rotation for its reader.
  void describe(HANDLE device, State& state, std::size_t reportSize) {
    state.described = true;
    USHORT inputLength = 0;
    const SpaceMouseRotationReport where = rotationReportOf(device, inputLength);
    state.reader.setRotationReport(where);
    qDebug().noquote() << QStringLiteral("3D mouse: %1: reports of %2 bytes (dwSizeHid), the longest input report "
                                         "%3 bytes, the rotation %4 by its report descriptor")
                              .arg(label(device))
                              .arg(static_cast<qulonglong>(reportSize))
                              .arg(static_cast<uint>(inputLength))
                              .arg(QString::fromLatin1(spaceMouseRotationReportText(where)));
  }

  void deviceChanged(WPARAM change, HANDLE device) {
    if (change == GIDC_ARRIVAL) {
      if (m_devices.count(device) != 0 || !isSpaceMouse(device)) {
        return;
      }
      const QString label = deviceName(device);
      m_devices[device] = label;
      if (connection) {
        connection(true, label);
      }
    } else if (change == GIDC_REMOVAL) {
      const auto found = m_devices.find(device);
      if (found == m_devices.end()) {
        return;
      }
      const QString label = found->second;
      m_devices.erase(found);
      m_states.erase(device);
      if (connection) {
        connection(false, label);
      }
    }
  }

  HWND m_window = nullptr;
  bool m_registered = false;
  std::map<HANDLE, QString> m_devices;
  std::map<HANDLE, State> m_states;
  std::vector<std::uint64_t> m_buffer;
  QElapsedTimer m_clock; // the raw reports' log interval
};

// Start one route at a time. Keep Raw Input usable when the system DLL or
// a driver device is unavailable, and check again as the driver comes up.
class WindowsBackend final : public SpaceMouseBackend {
public:
  WindowsBackend() {
    m_retry.setInterval(5000);
    QObject::connect(&m_retry, &QTimer::timeout, &m_retry, [this] {
      QString device;
      if ((!acceptsInput || acceptsInput()) && startDriver(device) && connection) {
        connection(true, device);
      }
    });
  }

  QString name() const override {
    if (m_backend) {
      return m_backend->name();
    }
    return m_driverOpen ? QStringLiteral("3DxWare") : QStringLiteral("Raw Input");
  }

  bool start(QString& device) override {
    if (startDriver(device)) {
      return true;
    }
    return startRaw(device);
  }

  // The window of the active design (nativeWindow) was placed or is
  // another one now: another design activated, or the active one closed
  // or put in another frame (mitcad#2297). A driver start that found no
  // window (the design not yet in its shown frame, mitcad#2152) is tried
  // again once there is one, whether the window is active or not, so that
  // a 3DxWare user gets the driver and not only Raw Input; a window it was
  // already tried with is not tried again (a design's tab shown again, its
  // frame restored). An open driver session follows a new top-level
  // window: the driver sends its events to the window it was opened for.
  void windowPlaced() override {
    if (!m_backend || !nativeWindow) {
      return;
    }
    const std::uintptr_t window = nativeWindow();
    if (window == 0 || window == m_triedWindow) {
      return;
    }
    if (m_driverOpen) {
      // Later, from the event loop: closing the session can destroy the
      // native filter whose input led here.
      QTimer::singleShot(0, &m_retry, [this] { rehome(); });
      return;
    }
    QString device;
    if (startDriver(device) && connection) {
      connection(true, device);
    }
  }

private:
  void wire(SpaceMouseBackend& backend) {
    backend.acceptsInput = acceptsInput;
    backend.nativeWindow = nativeWindow;
    backend.motion = motion;
    backend.button = button;
  }

  bool startDriver(QString& device) {
    m_triedWindow = nativeWindow ? nativeWindow() : 0;
    auto driver = createWindows3DxWareBackend();
    wire(*driver);
    if (!driver->start(device)) {
      return false;
    }
    driver->connection = [this](bool present, const QString& deviceName) {
      if (present) {
        return;
      }
      // Closing the driver can destroy the native filter that called us.
      QTimer::singleShot(0, &m_retry, [this, deviceName] {
        if (connection) {
          connection(false, deviceName);
        }
        QString rawDevice;
        if (startRaw(rawDevice) && connection) {
          connection(true, rawDevice);
        }
      });
    };
    if (motion) {
      motion({});
    }
    m_backend = std::move(driver);
    m_driverOpen = true;
    m_driverDevice = device;
    m_retry.stop();
    return true;
  }

  // The driver's session closed for the window it was opened for and
  // opened again for the active design's (mitcad#2297), the same device:
  // no new "connected" line. A driver that does not open for it leaves
  // Raw Input, the driver tried again every 5 s as at a start.
  void rehome() {
    const std::uintptr_t window = nativeWindow ? nativeWindow() : 0;
    if (!m_driverOpen || !m_backend || window == 0 || window == m_triedWindow) {
      return;
    }
    if (motion) {
      motion({});
    }
    m_backend.reset(); // closes the session (name() still says 3DxWare)
    QString device;
    if (startDriver(device)) {
      qInfo().noquote() << QStringLiteral("3D mouse: 3DxWare follows the active design's window");
      return;
    }
    if (connection) {
      connection(false, m_driverDevice);
    }
    QString rawDevice;
    if (startRaw(rawDevice) && connection) {
      connection(true, rawDevice);
    }
  }

  bool startRaw(QString& device) {
    m_driverOpen = false;
    auto raw = std::make_unique<RawInputBackend>();
    wire(*raw);
    raw->connection = connection;
    m_backend = std::move(raw);
    m_retry.start();
    return m_backend->start(device);
  }

  QTimer m_retry;
  std::unique_ptr<SpaceMouseBackend> m_backend;
  bool m_driverOpen = false;
  // The device the open driver session reported.
  QString m_driverDevice;
  // The window the last driver start was given (0: none yet).
  std::uintptr_t m_triedWindow = 0;
};

} // namespace

std::unique_ptr<SpaceMouseBackend> createSpaceMouseBackend() {
  return std::make_unique<WindowsBackend>();
}

// The driver's processes by name (mitcad#1981): a snapshot of the running
// processes, read once each time Preferences opens.
bool spaceMouseVendorDriverRunning() {
  const HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) {
    return false;
  }
  bool found = false;
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  for (BOOL more = Process32FirstW(snapshot, &entry); more && !found; more = Process32NextW(snapshot, &entry)) {
    found = isSpaceMouseDriverProcess(QString::fromWCharArray(entry.szExeFile));
  }
  CloseHandle(snapshot);
  return found;
}

} // namespace mitcad
