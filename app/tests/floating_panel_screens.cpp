// SPDX-License-Identifier: MIT
// The Docked layout's floating panels at the next start (mitcad#2070,
// GitHub mitcad/Mitcad#451, framework/WindowPlace.hpp floatingFrameStays):
// a panel whose kept frame lies wholly within its connected screen's
// available area keeps its place; one on a screen no longer connected, half
// off a screen or across two screens goes back into its dock; one monitor
// gone out of three docks only the panels that were on it.
#include "UnitTests.hpp"
#include "framework/WindowPlace.hpp"

#include <cstdio>

#include <QList>
#include <QRect>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "floating_panel_screens.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::floatingFrameStays;
using mitcad::ScreenArea;

// Three monitors side by side: the primary with a task bar at the bottom,
// a second to its right with a menu bar at the top, a third right of that.
const ScreenArea kPrimary{QStringLiteral("DISPLAY1"), QRect(0, 0, 1920, 1040)};
const ScreenArea kSecond{QStringLiteral("DISPLAY2"), QRect(1920, 16, 1280, 1008)};
const ScreenArea kThird{QStringLiteral("DISPLAY3"), QRect(3200, 0, 1920, 1080)};

void keptTests() {
  const QList<ScreenArea> all{kPrimary, kSecond, kThird};
  // Wholly within its screen: kept, on any of them.
  CHECK(floatingFrameStays(QRect(100, 600, 400, 200), QStringLiteral("DISPLAY1"), all));
  CHECK(floatingFrameStays(QRect(2000, 100, 400, 300), QStringLiteral("DISPLAY2"), all));
  CHECK(floatingFrameStays(QRect(4000, 500, 600, 400), QStringLiteral("DISPLAY3"), all));
  // Exactly the available area: kept.
  CHECK(floatingFrameStays(kSecond.available, QStringLiteral("DISPLAY2"), all));
  // No screen name kept: wholly within any connected screen is enough.
  CHECK(floatingFrameStays(QRect(2000, 100, 400, 300), QString(), all));
}

void dockedTests() {
  const QList<ScreenArea> all{kPrimary, kSecond, kThird};
  const QList<ScreenArea> primaryOnly{kPrimary};
  // Its screen no longer connected: docked, also where the kept frame
  // would lie within another screen (it is never moved onto one).
  CHECK(!floatingFrameStays(QRect(2000, 100, 400, 300), QStringLiteral("DISPLAY2"), primaryOnly));
  CHECK(!floatingFrameStays(QRect(100, 100, 400, 300), QStringLiteral("gone-monitor"), all));
  // Half off its screen: past the left edge, below the task bar, above the
  // menu bar.
  CHECK(!floatingFrameStays(QRect(-200, 100, 400, 300), QStringLiteral("DISPLAY1"), all));
  CHECK(!floatingFrameStays(QRect(100, 900, 400, 300), QStringLiteral("DISPLAY1"), all));
  CHECK(!floatingFrameStays(QRect(2000, 0, 400, 300), QStringLiteral("DISPLAY2"), all));
  // Across two screens: docked, whichever of them it was kept on.
  CHECK(!floatingFrameStays(QRect(1800, 100, 400, 300), QStringLiteral("DISPLAY1"), all));
  CHECK(!floatingFrameStays(QRect(1800, 100, 400, 300), QStringLiteral("DISPLAY2"), all));
  CHECK(!floatingFrameStays(QRect(1800, 100, 400, 300), QString(), all));
  // Its screen there but the frame on another: docked.
  CHECK(!floatingFrameStays(QRect(100, 100, 400, 300), QStringLiteral("DISPLAY2"), all));
  // No screens, or no frame: docked.
  CHECK(!floatingFrameStays(QRect(100, 100, 400, 300), QStringLiteral("DISPLAY1"), {}));
  CHECK(!floatingFrameStays(QRect(), QStringLiteral("DISPLAY1"), all));
}

void oneOfThreeGoneTests() {
  // Three panels, one on each monitor; the second monitor is removed: only
  // the panel that was on it goes back into its dock.
  struct Panel {
    QRect frame;
    QString screen;
  };
  const Panel panels[] = {{QRect(100, 600, 400, 200), QStringLiteral("DISPLAY1")},
                          {QRect(2000, 100, 400, 300), QStringLiteral("DISPLAY2")},
                          {QRect(4000, 500, 600, 400), QStringLiteral("DISPLAY3")}};
  const QList<ScreenArea> remaining{kPrimary, kThird};
  CHECK(floatingFrameStays(panels[0].frame, panels[0].screen, remaining));
  CHECK(!floatingFrameStays(panels[1].frame, panels[1].screen, remaining));
  CHECK(floatingFrameStays(panels[2].frame, panels[2].screen, remaining));
  // The third monitor now at another place (the second gone, it moved
  // left): its panel's kept frame is off it, docked.
  const QList<ScreenArea> moved{kPrimary, {QStringLiteral("DISPLAY3"), QRect(1920, 0, 1920, 1080)}};
  CHECK(floatingFrameStays(panels[0].frame, panels[0].screen, moved));
  CHECK(!floatingFrameStays(panels[2].frame, panels[2].screen, moved));
}

} // namespace

int floatingPanelScreensTests() {
  keptTests();
  dockedTests();
  oneOfThreeGoneTests();
  return failures;
}
MITCAD_UNIT_TEST(floatingPanelScreensTests);
