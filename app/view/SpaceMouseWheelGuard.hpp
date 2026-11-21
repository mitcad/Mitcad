// SPDX-License-Identifier: MIT
#pragma once

// The wheel while a 3D mouse moves the view (mitcad#1981, GitHub
// mitcad/Mitcad#412 and #297): the vendor driver's profile can send
// emulated wheel input for a cap's motion (its Tilt axis by default), and
// each such wheel step zoomed the 3D view, about the cursor, against the
// motion: a zoom in jumped back smaller. The view ignores the wheel while a
// motion runs and for a short time after it; a wheel without a recent
// motion zooms as before. Plain C++, so that the unit test drives it with
// times of its own (tests/space_mouse_wheel.cpp); OcctViewer feeds it a
// monotonic clock's milliseconds.

#include <cstdint>

namespace mitcad {

class SpaceMouseWheelGuard {
public:
  // How long after a motion's last input the wheel stays ignored: the
  // driver's emulated steps can trail the cap's return to rest.
  static constexpr std::int64_t kQuietMilliseconds = 300;
  // At most one ignored wheel's log line in this time (ten a second).
  static constexpr std::int64_t kLogMilliseconds = 100;

  // The device's input at `now`: `moving` while the cap is deflected,
  // false at rest (the quiet time counts from a motion's end; rest without
  // a motion before it changes nothing).
  void motion(std::int64_t now, bool moving) {
    if (!moving && !m_moving) {
      return;
    }
    m_moving = moving;
    m_last = now;
    m_seen = true;
  }

  // Whether a wheel event at `now` may zoom: not during a motion, nor in
  // the quiet time after it.
  bool allowWheel(std::int64_t now) const {
    if (m_moving) {
      return false;
    }
    return !m_seen || now - m_last >= kQuietMilliseconds;
  }

  // Input that arrives at `now` came during a motion (or its quiet time):
  // the debug log names it, as possibly the driver's.
  bool duringMotion(std::int64_t now) const { return !allowWheel(now); }

  // An ignored wheel at `now`: whether its line is logged now, and in
  // `skipped` how many ignored wheels were left out of the log since the
  // last line.
  bool logIgnored(std::int64_t now, int& skipped) {
    if (m_logged && now - m_lastLog < kLogMilliseconds) {
      ++m_skipped;
      skipped = 0;
      return false;
    }
    skipped = m_skipped;
    m_skipped = 0;
    m_lastLog = now;
    m_logged = true;
    return true;
  }

private:
  std::int64_t m_last = 0;
  std::int64_t m_lastLog = 0;
  int m_skipped = 0;
  bool m_moving = false;
  bool m_seen = false;
  bool m_logged = false;
};

} // namespace mitcad
