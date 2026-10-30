// SPDX-License-Identifier: MIT
// 3D mice on Windows (mitcad#951): the device's HID reports through Raw
// Input, no vendor SDK. A message-only window of its own registers for the
// multi-axis controllers (HID usage page 1, usage 8) with RIDEV_INPUTSINK,
// so that it gets them whichever of Mitcad's windows has the focus, and
// RIDEV_DEVNOTIFY for the devices that come and go; its window procedure
// reads them as Qt's event loop dispatches them. Input counts only while
// Mitcad is the foreground application (RIM_INPUT). The reports' parsing
// and their axes are SpaceMouseDevice.hpp's.

#include "../SpaceMouseDevice.hpp"

#include <cstddef>
#include <cstdint>
#include <map>
#include <memory>
#include <vector>

#include <QDebug>
#include <QStringList>

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <hidsdi.h>

namespace mitcad {
namespace {

constexpr USHORT kGenericDesktop = 0x01;
constexpr USHORT kMultiAxisController = 0x08;
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
    std::array<int, 3> t{};
    std::array<int, 3> r{};
    std::uint32_t pressed = 0;
    bool moving = false;
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
    State& state = m_states[raw->header.hDevice];
    bool moved = false;
    for (std::size_t index = 0; index < reportCount; ++index) {
      SpaceMouseHidReport report;
      if (!parseSpaceMouseHidReport(reports + index * reportSize, reportSize, report)) {
        continue;
      }
      if (report.translation) {
        state.t = report.t;
        moved = true;
      }
      if (report.rotation) {
        state.r = report.r;
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
    if (!moved || !motion) {
      return;
    }
    if (foreground) {
      const SpaceMouseAxes axes = spaceMouseFromHid(state.t, state.r);
      state.moving = axes != SpaceMouseAxes{};
      motion(axes);
    } else if (state.moving) {
      // Another application has the device now.
      state.moving = false;
      motion(SpaceMouseAxes{});
    }
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
};

} // namespace

std::unique_ptr<SpaceMouseBackend> createSpaceMouseBackend() {
  return std::make_unique<RawInputBackend>();
}

} // namespace mitcad
