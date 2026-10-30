// SPDX-License-Identifier: MIT
// 3D mice on macOS (mitcad#951): the device's HID values through IOKit's
// HID Manager, matched as a multi-axis controller (HID usage page 1, usage
// 8), on the main run loop that Qt runs. No vendor framework: its client
// framework and headers are under a proprietary licence
// (docs/development.md#licence-policy). The vendor's driver, when it is
// installed and running, may hold the device for itself; then no values
// reach Mitcad (docs/user-guide.md says so). The axes are HID's, as on
// Windows (SpaceMouseDevice.hpp maps them); input counts only while Mitcad
// is the active application.

#include "../SpaceMouseDevice.hpp"

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <map>
#include <memory>
#include <vector>

#include <QByteArray>
#include <QDebug>
#include <QGuiApplication>
#include <QStringList>

#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/hid/IOHIDKeys.h>
#include <IOKit/hid/IOHIDManager.h>

namespace mitcad {
namespace {

constexpr int kGenericDesktop = 0x01;
constexpr int kMultiAxisController = 0x08;
constexpr std::uint32_t kButtonPage = 0x09;
constexpr std::uint32_t kUsageX = 0x30;  // X, Y, Z, Rx, Ry, Rz follow
constexpr std::uint32_t kUsageRz = 0x35;

QString stringProperty(IOHIDDeviceRef device, CFStringRef key) {
  const CFTypeRef value = IOHIDDeviceGetProperty(device, key);
  if (value == nullptr || CFGetTypeID(value) != CFStringGetTypeID()) {
    return QString();
  }
  const auto text = static_cast<CFStringRef>(value);
  const CFIndex size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(text), kCFStringEncodingUTF8) + 1;
  QByteArray bytes(static_cast<int>(size), '\0');
  if (!CFStringGetCString(text, bytes.data(), size, kCFStringEncodingUTF8)) {
    return QString();
  }
  return QString::fromUtf8(bytes.constData());
}

int intProperty(IOHIDDeviceRef device, CFStringRef key) {
  const CFTypeRef value = IOHIDDeviceGetProperty(device, key);
  int number = 0;
  if (value == nullptr || CFGetTypeID(value) != CFNumberGetTypeID() ||
      !CFNumberGetValue(static_cast<CFNumberRef>(value), kCFNumberIntType, &number)) {
    return 0;
  }
  return number;
}

// "SpaceMouse Compact (HID 256f:c635)".
QString deviceName(IOHIDDeviceRef device) {
  const QString ids = QStringLiteral("HID %1:%2")
                          .arg(intProperty(device, CFSTR(kIOHIDVendorIDKey)), 4, 16, QLatin1Char('0'))
                          .arg(intProperty(device, CFSTR(kIOHIDProductIDKey)), 4, 16, QLatin1Char('0'));
  const QString product = stringProperty(device, CFSTR(kIOHIDProductKey)).trimmed();
  return product.isEmpty() ? ids : QStringLiteral("%1 (%2)").arg(product, ids);
}

class HidBackend final : public SpaceMouseBackend {
public:
  ~HidBackend() override {
    if (m_manager != nullptr) {
      IOHIDManagerUnscheduleFromRunLoop(m_manager, CFRunLoopGetMain(), kCFRunLoopDefaultMode);
      IOHIDManagerClose(m_manager, kIOHIDOptionsTypeNone);
      CFRelease(m_manager);
    }
  }

  QString name() const override { return QStringLiteral("IOKit HID"); }

  bool start(QString& device) override {
    m_manager = IOHIDManagerCreate(kCFAllocatorDefault, kIOHIDOptionsTypeNone);
    if (m_manager == nullptr) {
      return false;
    }
    CFMutableDictionaryRef matching = CFDictionaryCreateMutable(kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks,
                                                                &kCFTypeDictionaryValueCallBacks);
    const int page = kGenericDesktop;
    const int usage = kMultiAxisController;
    CFNumberRef pageNumber = CFNumberCreate(kCFAllocatorDefault, kCFNumberIntType, &page);
    CFNumberRef usageNumber = CFNumberCreate(kCFAllocatorDefault, kCFNumberIntType, &usage);
    CFDictionarySetValue(matching, CFSTR(kIOHIDDeviceUsagePageKey), pageNumber);
    CFDictionarySetValue(matching, CFSTR(kIOHIDDeviceUsageKey), usageNumber);
    IOHIDManagerSetDeviceMatching(m_manager, matching);
    CFRelease(usageNumber);
    CFRelease(pageNumber);
    CFRelease(matching);
    IOHIDManagerRegisterDeviceMatchingCallback(m_manager, &HidBackend::matched, this);
    IOHIDManagerRegisterDeviceRemovalCallback(m_manager, &HidBackend::removed, this);
    IOHIDManagerRegisterInputValueCallback(m_manager, &HidBackend::changed, this);
    IOHIDManagerScheduleWithRunLoop(m_manager, CFRunLoopGetMain(), kCFRunLoopDefaultMode);
    const IOReturn opened = IOHIDManagerOpen(m_manager, kIOHIDOptionsTypeNone);
    if (opened != kIOReturnSuccess) {
      qWarning().noquote() << QStringLiteral("3D mouse: IOKit's HID Manager did not open (0x%1)")
                                  .arg(static_cast<quint32>(opened), 8, 16, QLatin1Char('0'));
    }
    // The devices there now; their matching callbacks come later from the
    // run loop and find them known.
    if (CFSetRef devices = IOHIDManagerCopyDevices(m_manager)) {
      const CFIndex count = CFSetGetCount(devices);
      std::vector<const void*> values(static_cast<std::size_t>(std::max<CFIndex>(count, 0)));
      CFSetGetValues(devices, values.data());
      for (const void* value : values) {
        auto* found = static_cast<IOHIDDeviceRef>(const_cast<void*>(value));
        m_devices[found] = deviceName(found);
      }
      CFRelease(devices);
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
  static void matched(void* context, IOReturn, void*, IOHIDDeviceRef device) {
    auto* self = static_cast<HidBackend*>(context);
    if (self->m_devices.count(device) != 0) {
      return;
    }
    const QString label = deviceName(device);
    self->m_devices[device] = label;
    if (self->connection) {
      self->connection(true, label);
    }
  }

  static void removed(void* context, IOReturn, void*, IOHIDDeviceRef device) {
    auto* self = static_cast<HidBackend*>(context);
    const auto found = self->m_devices.find(device);
    if (found == self->m_devices.end()) {
      return;
    }
    const QString label = found->second;
    self->m_devices.erase(found);
    self->m_t = {};
    self->m_r = {};
    if (self->m_moving && self->motion) {
      self->m_moving = false;
      self->motion(SpaceMouseAxes{});
    }
    if (self->connection) {
      self->connection(false, label);
    }
  }

  static void changed(void* context, IOReturn, void*, IOHIDValueRef value) {
    static_cast<HidBackend*>(context)->input(value);
  }

  void input(IOHIDValueRef value) {
    const IOHIDElementRef element = IOHIDValueGetElement(value);
    const std::uint32_t page = IOHIDElementGetUsagePage(element);
    const std::uint32_t usage = IOHIDElementGetUsage(element);
    const CFIndex number = IOHIDValueGetIntegerValue(value);
    const bool active = QGuiApplication::applicationState() == Qt::ApplicationActive;
    if (page == static_cast<std::uint32_t>(kGenericDesktop) && usage >= kUsageX && usage <= kUsageRz) {
      const std::size_t axis = usage - kUsageX;
      const int clamped = static_cast<int>(std::clamp<CFIndex>(number, -32768, 32767));
      if (axis < 3) {
        m_t[axis] = clamped;
      } else {
        m_r[axis - 3] = clamped;
      }
      if (!motion) {
        return;
      }
      if (active) {
        const SpaceMouseAxes axes = spaceMouseFromHid(m_t, m_r);
        m_moving = axes != SpaceMouseAxes{};
        motion(axes);
      } else if (m_moving) {
        m_moving = false;
        motion(SpaceMouseAxes{});
      }
    } else if (page == kButtonPage && usage >= 1 && usage <= 32) {
      if (button && (active || number == 0)) {
        button(static_cast<int>(usage) - 1, number != 0);
      }
    }
  }

  IOHIDManagerRef m_manager = nullptr;
  std::map<IOHIDDeviceRef, QString> m_devices;
  std::array<int, 3> m_t{};
  std::array<int, 3> m_r{};
  bool m_moving = false;
};

} // namespace

std::unique_ptr<SpaceMouseBackend> createSpaceMouseBackend() {
  return std::make_unique<HidBackend>();
}

} // namespace mitcad
