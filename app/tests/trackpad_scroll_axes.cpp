// SPDX-License-Identifier: MIT
// Alt + a two-finger trackpad scroll orbits the way the fingers move
// (GitHub mitcad/Mitcad#381, #662; view/Navigation.hpp): Qt's xcb and
// Wayland plugins swap a scroll's axes while Alt is held, and the view swaps
// a trackpad's back. On xcb a touchpad is known from its device, on Wayland
// from its pixel deltas (Qt gives none for a wheel). A wheel, a scroll
// without Alt and other platforms keep Qt's axes.
#include "UnitTests.hpp"
#include "view/Navigation.hpp"

#include <cstdio>

#include <QString>
#include <Qt>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "trackpad_scroll_axes.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::trackpadScrollAxesSwapped;

const QString kXcb = QStringLiteral("xcb");
const QString kWayland = QStringLiteral("wayland");
const QString kWaylandEgl = QStringLiteral("wayland-egl");

void xcbTests() {
  // A touchpad's wheel steps under Alt: swapped back, as before.
  CHECK(trackpadScrollAxesSwapped(kXcb, Qt::AltModifier, true, false));
  // Pixel deltas do not decide on xcb: only the touchpad device does.
  CHECK(!trackpadScrollAxesSwapped(kXcb, Qt::AltModifier, false, true));
  CHECK(!trackpadScrollAxesSwapped(kXcb, Qt::AltModifier, false, false)); // a wheel
  CHECK(!trackpadScrollAxesSwapped(kXcb, Qt::NoModifier, true, false));
  CHECK(!trackpadScrollAxesSwapped(kXcb, Qt::ShiftModifier, true, false));
  CHECK(!trackpadScrollAxesSwapped(kXcb, Qt::ControlModifier, true, false));
}

void waylandTests() {
  // A finger scroll (pixel deltas) under Alt: swapped back, whatever the
  // device says (GitHub mitcad/Mitcad#662).
  for (const QString& platform : {kWayland, kWaylandEgl}) {
    CHECK(trackpadScrollAxesSwapped(platform, Qt::AltModifier, false, true));
    CHECK(trackpadScrollAxesSwapped(platform, Qt::AltModifier, true, true));
    CHECK(trackpadScrollAxesSwapped(platform, Qt::AltModifier | Qt::ShiftModifier, false, true));
    // A wheel (no pixel deltas) keeps Qt's swap, also on a touchpad device.
    CHECK(!trackpadScrollAxesSwapped(platform, Qt::AltModifier, false, false));
    CHECK(!trackpadScrollAxesSwapped(platform, Qt::AltModifier, true, false));
    // Without Alt nothing was swapped: pan, Shift orbit and Ctrl zoom.
    CHECK(!trackpadScrollAxesSwapped(platform, Qt::NoModifier, false, true));
    CHECK(!trackpadScrollAxesSwapped(platform, Qt::ShiftModifier, false, true));
    CHECK(!trackpadScrollAxesSwapped(platform, Qt::ControlModifier, false, true));
  }
}

void otherPlatformTests() {
  for (const QString& platform : {QStringLiteral("windows"), QStringLiteral("cocoa"), QStringLiteral("offscreen")}) {
    CHECK(!trackpadScrollAxesSwapped(platform, Qt::AltModifier, true, true));
    CHECK(!trackpadScrollAxesSwapped(platform, Qt::AltModifier, false, true));
  }
}

} // namespace

int trackpadScrollAxesTests() {
  xcbTests();
  waylandTests();
  otherPlatformTests();
  return failures;
}
MITCAD_UNIT_TEST(trackpadScrollAxesTests);
