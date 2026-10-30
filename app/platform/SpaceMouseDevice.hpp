// SPDX-License-Identifier: MIT
#pragma once

// 3D mice (6-DOF devices, mitcad#951): the small interface the view's
// SpaceMouseController reads them through, one backend per platform, and
// the pure parsing of the devices' reports, here so that the unit tests
// check it on every platform. No vendor SDK: Windows reads the device's
// HID reports through Raw Input (platform/windows/SpaceMouseWindows.cpp),
// Linux and the BSDs the spacenavd daemon's socket
// (platform/unix/SpaceMouseSpnav.cpp), macOS the HID values through
// IOKit's HID Manager (platform/macos/SpaceMouseMac.cpp).
//
// The axes come in the view's frame, normalized to about ±1 at full
// deflection: x to the right, y up and z toward the user (out of the
// screen), right-handed; the translations first (tx, ty, tz), then the
// rotations (rx, ry, rz) about the same axes, positive counter-clockwise
// seen from the axis' tip. Each backend maps its device's frame into this
// one; Preferences can reverse any axis.

#include <array>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>

#include <QString>

namespace mitcad {

using SpaceMouseAxes = std::array<double, 6>;

enum class SpaceMouseAxis : int { Tx, Ty, Tz, Rx, Ry, Rz };

// The raw count of a full deflection: about 350 for the common devices
// (their reports reach a little beyond; the view's mapping clamps).
constexpr double kSpaceMouseFullScale = 350.0;

// One report of a 3D mouse's HID interface (usage page 1, usage 8, the
// multi-axis controller), the report id first: id 1 the translation (three
// 16-bit little-endian integers), id 2 the rotation, id 1 with 12 bytes
// both (newer devices), id 3 the buttons as a bit mask (bit 0 the first
// button, at most 32).
struct SpaceMouseHidReport {
  bool translation = false;
  bool rotation = false;
  bool buttons = false;
  std::array<int, 3> t{};
  std::array<int, 3> r{};
  std::uint32_t pressed = 0;
};

inline bool parseSpaceMouseHidReport(const std::uint8_t* data, std::size_t size, SpaceMouseHidReport& report) {
  report = SpaceMouseHidReport();
  if (data == nullptr || size < 2) {
    return false;
  }
  const auto int16At = [data](std::size_t offset) {
    const auto bits = static_cast<std::uint16_t>(data[offset] | (data[offset + 1] << 8));
    return static_cast<int>(static_cast<std::int16_t>(bits));
  };
  switch (data[0]) {
  case 1:
    if (size >= 7) {
      report.translation = true;
      report.t = {int16At(1), int16At(3), int16At(5)};
    }
    if (size >= 13) {
      report.rotation = true;
      report.r = {int16At(7), int16At(9), int16At(11)};
    }
    return report.translation;
  case 2:
    if (size >= 7) {
      report.rotation = true;
      report.r = {int16At(1), int16At(3), int16At(5)};
    }
    return report.rotation;
  case 3:
    report.buttons = true;
    for (std::size_t byte = 1; byte < size && byte <= 4; ++byte) {
      report.pressed |= static_cast<std::uint32_t>(data[byte]) << (8 * (byte - 1));
    }
    return true;
  default:
    return false;
  }
}

// HID's axes (X to the right, Y toward the user, Z down; Rx, Ry, Rz about
// them, right-handed) in the view's frame. IOKit's HID values on macOS
// have the same axes.
inline SpaceMouseAxes spaceMouseFromHid(const std::array<int, 3>& t, const std::array<int, 3>& r) {
  const double scale = 1.0 / kSpaceMouseFullScale;
  return {t[0] * scale, -t[2] * scale, t[1] * scale, r[0] * scale, -r[2] * scale, r[1] * scale};
}

// One event of spacenavd's socket (its original protocol, which the daemon
// speaks to every client that asks for no other): eight 32-bit integers in
// the machine's order. data[0] is the kind (0 a motion, 1 a button press,
// 2 a release); a motion's axes follow (x, y, z, rx, ry, rz) and then its
// period in ms, a button event's number follows.
struct SpnavEvent {
  enum class Kind { Motion, Press, Release, Other };
  Kind kind = Kind::Other;
  std::array<int, 6> motion{};
  int button = -1;
};

inline SpnavEvent parseSpnavEvent(const std::array<std::int32_t, 8>& data) {
  SpnavEvent event;
  switch (data[0]) {
  case 0:
    event.kind = SpnavEvent::Kind::Motion;
    for (std::size_t axis = 0; axis < 6; ++axis) {
      event.motion[axis] = data[axis + 1];
    }
    break;
  case 1:
  case 2:
    event.kind = data[0] == 1 ? SpnavEvent::Kind::Press : SpnavEvent::Kind::Release;
    event.button = data[1];
    break;
  default:
    break;
  }
  return event;
}

// spacenavd's frame (left-handed: x to the right, y up, z into the
// screen, as libspnav documents it) in the view's.
inline SpaceMouseAxes spaceMouseFromSpnav(const std::array<int, 6>& motion) {
  const double scale = 1.0 / kSpaceMouseFullScale;
  return {motion[0] * scale, motion[1] * scale, -motion[2] * scale,
          -motion[3] * scale, -motion[4] * scale, motion[5] * scale};
}

// Calls `changed(button, pressed)` for each bit that differs between two
// button masks, the lowest first.
template <typename Changed>
void spaceMouseButtonChanges(std::uint32_t before, std::uint32_t after, Changed&& changed) {
  for (int bit = 0; bit < 32; ++bit) {
    const std::uint32_t mask = std::uint32_t(1) << bit;
    if (((before ^ after) & mask) != 0) {
      changed(bit, (after & mask) != 0);
    }
  }
}

// A platform's way to a 3D mouse. It runs on the GUI thread and reports
// through the callbacks; a backend that found nothing at start() keeps
// listening for a device (or the daemon) that comes later.
class SpaceMouseBackend {
public:
  virtual ~SpaceMouseBackend() = default;
  // Its name in the log: "Raw Input", "spacenavd", "IOKit HID".
  virtual QString name() const = 0;
  // Starts listening. True when a device (or spacenavd) is there now, with
  // its name in `device`.
  virtual bool start(QString& device) = 0;

  // The axes moved (the view's frame, see above).
  std::function<void(const SpaceMouseAxes&)> motion;
  // A button (0 the first) went down or up.
  std::function<void(int button, bool pressed)> button;
  // A device came or went after start().
  std::function<void(bool connected, const QString& device)> connection;
};

// This platform's backend (never null; it may find nothing).
std::unique_ptr<SpaceMouseBackend> createSpaceMouseBackend();

} // namespace mitcad
