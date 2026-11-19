// SPDX-License-Identifier: MIT
#pragma once

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>

namespace mitcad {

struct SiAppDeviceEvent {
  enum class Kind { Other, Motion, Rest, Buttons, ButtonDown, ButtonUp, DeviceChange };
  Kind kind = Kind::Other;
  std::array<int, 6> axes{};
  std::uint32_t buttons = 0;
  int button = -1;
};

// The SiApp message begins with a 32-bit type. In the motion payload, the
// four button words precede the six 32-bit axes. Hardware button events
// put the one-based button number at the beginning of their payload.
inline SiAppDeviceEvent parseSiAppDeviceEvent(const std::byte* data, std::size_t length) {
  if (!data || length < 4) {
    return {};
  }
  const auto word = [data](std::size_t offset) {
    std::uint32_t value{};
    std::memcpy(&value, data + offset, sizeof(value));
    return value;
  };
  SiAppDeviceEvent result;
  switch (word(0)) {
  case 1:
    if (length >= 12) {
      result.kind = SiAppDeviceEvent::Kind::Buttons;
      result.buttons = word(8);
    }
    break;
  case 2:
    if (length >= 44) {
      result.kind = SiAppDeviceEvent::Kind::Motion;
      for (std::size_t axis = 0; axis < result.axes.size(); ++axis) {
        std::int32_t value{};
        std::memcpy(&value, data + 20 + axis * sizeof(value), sizeof(value));
        result.axes[axis] = std::clamp(value, std::int32_t{-32768}, std::int32_t{32767});
      }
    }
    break;
  case 4:
    result.kind = SiAppDeviceEvent::Kind::Rest;
    break;
  case 12:
  case 13:
    if (length >= 8) {
      const auto button = word(4);
      if (button >= 1 && button <= 32) {
        result.kind = word(0) == 12 ? SiAppDeviceEvent::Kind::ButtonDown : SiAppDeviceEvent::Kind::ButtonUp;
        result.button = static_cast<int>(button) - 1;
      }
    }
    break;
  case 14:
    result.kind = SiAppDeviceEvent::Kind::DeviceChange;
    break;
  default:
    break;
  }
  return result;
}

// In X11's 16-bit ClientMessage, six signed axes start at word two.
inline std::array<int, 6> magellanMotion(const std::array<std::uint16_t, 10>& words) {
  std::array<int, 6> axes{};
  for (std::size_t axis = 0; axis < axes.size(); ++axis) {
    std::int16_t value{};
    std::memcpy(&value, &words[axis + 2], sizeof(value));
    axes[axis] = value;
  }
  return axes;
}

} // namespace mitcad
