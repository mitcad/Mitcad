// SPDX-License-Identifier: MIT
// The wheel during a 3D mouse motion (mitcad#1981, view/SpaceMouseWheelGuard.hpp):
// a wheel during a motion and in the quiet time after it is ignored (the
// vendor driver's emulated wheel steps zoomed the view against the
// motion), one without a motion and one after the quiet time zooms; the
// ignored wheels' log lines are thinned; the cap counts as moving when any
// axis is past the dead zone. And the driver's processes known by name for
// Preferences' hint (platform/SpaceMouseDevice.hpp).
#include "UnitTests.hpp"

#include <cstdint>
#include <cstdio>

#include <QString>

#include "platform/SpaceMouseDevice.hpp"
#include "view/SpaceMouseNavigation.hpp"
#include "view/SpaceMouseWheelGuard.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "space_mouse_wheel.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

using mitcad::SpaceMouseWheelGuard;
constexpr std::int64_t kQuiet = SpaceMouseWheelGuard::kQuietMilliseconds;

void quietTimeTests() {
  // No motion yet: the wheel zooms, at any time.
  SpaceMouseWheelGuard guard;
  CHECK(guard.allowWheel(0));
  CHECK(guard.allowWheel(5000));
  CHECK(!guard.duringMotion(5000));
  // The cap at rest without a motion before it starts no quiet time.
  guard.motion(5000, false);
  CHECK(guard.allowWheel(5000));
  CHECK(guard.allowWheel(5001));

  // During a motion: ignored, however long since its last input.
  guard.motion(1000, true);
  CHECK(!guard.allowWheel(1000));
  CHECK(!guard.allowWheel(1010));
  CHECK(!guard.allowWheel(1000 + 10 * kQuiet));
  CHECK(guard.duringMotion(1010));
  guard.motion(1500, true);
  CHECK(!guard.allowWheel(1600));

  // Just after it: ignored for the quiet time from the motion's end.
  guard.motion(2000, false);
  CHECK(!guard.allowWheel(2000));
  CHECK(!guard.allowWheel(2001));
  CHECK(!guard.allowWheel(2000 + kQuiet - 1));
  CHECK(guard.duringMotion(2000 + kQuiet - 1));

  // After the quiet time: the wheel zooms again.
  CHECK(guard.allowWheel(2000 + kQuiet));
  CHECK(guard.allowWheel(2000 + 10 * kQuiet));
  CHECK(!guard.duringMotion(2000 + kQuiet));

  // A new motion ignores it again.
  guard.motion(9000, true);
  CHECK(!guard.allowWheel(9000));
  guard.motion(9200, false);
  CHECK(!guard.allowWheel(9200 + kQuiet / 2));
  CHECK(guard.allowWheel(9200 + kQuiet));
}

void logTests() {
  // The first ignored wheel is logged; the next ones within the interval
  // are counted and named by the next line.
  SpaceMouseWheelGuard guard;
  const std::int64_t interval = SpaceMouseWheelGuard::kLogMilliseconds;
  int skipped = -1;
  CHECK(guard.logIgnored(100, skipped));
  CHECK(skipped == 0);
  CHECK(!guard.logIgnored(110, skipped));
  CHECK(!guard.logIgnored(100 + interval - 1, skipped));
  CHECK(guard.logIgnored(100 + interval, skipped));
  CHECK(skipped == 2);
  CHECK(guard.logIgnored(100 + 3 * interval, skipped));
  CHECK(skipped == 0);
}

void deflectionTests() {
  using mitcad::SpaceMouseAxes;
  using mitcad::kSpaceMouseDeadzone;
  CHECK(!mitcad::spaceMouseDeflected(SpaceMouseAxes{}));
  CHECK(!mitcad::spaceMouseDeflected(SpaceMouseAxes{kSpaceMouseDeadzone, 0.0, 0.0, 0.0, 0.0, 0.0}));
  CHECK(!mitcad::spaceMouseDeflected(SpaceMouseAxes{0.0, -0.01, 0.0, 0.0, 0.0, 0.02}));
  // A tilt alone (rx), as the driver's profile turns into wheel steps.
  CHECK(mitcad::spaceMouseDeflected(SpaceMouseAxes{0.0, 0.0, 0.0, -0.3, 0.0, 0.0}));
  CHECK(mitcad::spaceMouseDeflected(SpaceMouseAxes{0.0, 0.0, 0.5, 0.0, 0.0, 0.0}));
}

void driverProcessTests() {
  CHECK(mitcad::isSpaceMouseDriverProcess(QStringLiteral("3DxService.exe")));
  CHECK(mitcad::isSpaceMouseDriverProcess(QStringLiteral("3dxservice.EXE")));
  CHECK(mitcad::isSpaceMouseDriverProcess(QStringLiteral("3dxsrv.exe")));
  CHECK(!mitcad::isSpaceMouseDriverProcess(QStringLiteral("mitcad.exe")));
  CHECK(!mitcad::isSpaceMouseDriverProcess(QStringLiteral("3DxService")));
  CHECK(!mitcad::isSpaceMouseDriverProcess(QString()));
}

} // namespace

int spaceMouseWheelTests() {
  quietTimeTests();
  logTests();
  deflectionTests();
  driverProcessTests();
  return failures;
}
MITCAD_UNIT_TEST(spaceMouseWheelTests);
