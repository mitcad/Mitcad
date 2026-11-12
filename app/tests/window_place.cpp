// SPDX-License-Identifier: MIT
// The design window's kept size and place (mitcad#1710,
// framework/WindowPlace.hpp): a kept frame fitted to the connected screens
// (a removed monitor, half off-screen, larger than the screen, the title
// bar above the top), a new window cascaded from another, and the place
// kept in and read from the settings.
#include "framework/WindowPlace.hpp"

#include <cstdio>

#include <QFile>
#include <QList>
#include <QMargins>
#include <QRect>
#include <QSettings>
#include <QSize>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "window_place.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// A frame wholly within one of `areas`.
bool within(const QRect& frame, const QList<QRect>& areas) {
  for (const QRect& area : areas) {
    if (area.contains(frame)) {
      return true;
    }
  }
  return false;
}

void fitTests() {
  using mitcad::fitFrameToScreens;
  // The primary screen with a task bar at the bottom, and a second one to
  // its right with a menu bar at the top.
  const QRect primary(0, 0, 1920, 1040);
  const QRect second(1920, 16, 1280, 1008);
  const QList<QRect> one{primary};
  const QList<QRect> both{primary, second};

  // Within a screen: kept as it is, on either screen.
  CHECK(fitFrameToScreens(QRect(100, 80, 1000, 700), one, 0) == QRect(100, 80, 1000, 700));
  CHECK(fitFrameToScreens(QRect(2000, 100, 1000, 700), both, 0) == QRect(2000, 100, 1000, 700));
  // Exactly the available area: kept.
  CHECK(fitFrameToScreens(primary, one, 0) == primary);

  // A removed monitor: the place was on the second screen, which is gone;
  // moved into the primary one whole, its size kept.
  const QRect removed = fitFrameToScreens(QRect(2200, 100, 1000, 700), one, 0);
  CHECK(removed.size() == QSize(1000, 700));
  CHECK(primary.contains(removed));
  CHECK(removed == QRect(920, 100, 1000, 700));
  // Far away (a screen arrangement that changed): also into the primary.
  CHECK(primary.contains(fitFrameToScreens(QRect(-5000, 4000, 900, 600), one, 0)));
  // A preferred screen out of range: the first one.
  CHECK(primary.contains(fitFrameToScreens(QRect(2200, 100, 1000, 700), one, 3)));

  // Half off-screen to the right and below: moved in, size kept.
  const QRect half = fitFrameToScreens(QRect(1500, 600, 1000, 700), one, 0);
  CHECK(half == QRect(920, 340, 1000, 700));
  CHECK(primary.contains(half));
  // Half off to the left: at the left edge.
  CHECK(fitFrameToScreens(QRect(-500, 100, 1000, 700), one, 0) == QRect(0, 100, 1000, 700));

  // Larger than the screen: shrunk to its available area.
  CHECK(fitFrameToScreens(QRect(-50, -20, 2500, 1300), one, 0) == primary);
  CHECK(fitFrameToScreens(QRect(300, 200, 1000, 1300), one, 0) == QRect(300, 0, 1000, 1040));

  // The title bar above the top of the screen: moved down to it.
  const QRect above = fitFrameToScreens(QRect(200, -30, 800, 600), one, 0);
  CHECK(above == QRect(200, 0, 800, 600));
  CHECK(above.top() >= primary.top());
  // Below the second screen's menu bar, on the screen it was on.
  const QRect underBar = fitFrameToScreens(QRect(2100, 0, 800, 600), both, 1);
  CHECK(underBar == QRect(2100, 16, 800, 600));
  CHECK(second.contains(underBar));

  // Partly off the screen it was on: into that screen, not the primary.
  const QRect onSecond = fitFrameToScreens(QRect(2800, 100, 800, 600), both, 1);
  CHECK(onSecond == QRect(2400, 100, 800, 600));
  // Across two screens: wholly into the preferred one.
  const QRect across = fitFrameToScreens(QRect(1600, 100, 800, 600), both, 0);
  CHECK(within(across, {primary}));
  CHECK(across == QRect(1120, 100, 800, 600));

  // Without screens nothing is known: unchanged.
  CHECK(fitFrameToScreens(QRect(5000, 5000, 800, 600), {}, 0) == QRect(5000, 5000, 800, 600));
}

void cascadeTests() {
  using mitcad::cascadedFrame;
  const QRect available(0, 0, 1920, 1040);
  // Right of and below the window it opens from by one step.
  CHECK(cascadedFrame(QRect(100, 100, 800, 600), QSize(800, 600), 30, available) == QRect(130, 130, 800, 600));
  // Another size: the last one, at the cascaded place.
  CHECK(cascadedFrame(QRect(100, 100, 800, 600), QSize(1000, 700), 30, available) == QRect(130, 130, 1000, 700));
  // Past the right edge: back at the left one.
  CHECK(cascadedFrame(QRect(1100, 100, 800, 600), QSize(800, 600), 30, available) == QRect(0, 130, 800, 600));
  // Past the bottom: back at the top.
  CHECK(cascadedFrame(QRect(100, 420, 800, 600), QSize(800, 600), 30, available) == QRect(130, 0, 800, 600));
  // On a screen whose area does not start at 0, 0.
  const QRect second(1920, 16, 1280, 1008);
  CHECK(cascadedFrame(QRect(2300, 400, 800, 600), QSize(800, 600), 30, second) == QRect(2330, 16, 800, 600));
}

void marginTests() {
  QMargins margins;
  CHECK(mitcad::frameMarginsFromText(QStringLiteral("8,31,8,8"), margins));
  CHECK(margins == QMargins(8, 31, 8, 8));
  CHECK(mitcad::frameMarginsText(margins) == QStringLiteral("8,31,8,8"));
  CHECK(mitcad::frameMarginsFromText(QStringLiteral("0,0,0,0"), margins));
  CHECK(margins.isNull());
  CHECK(!mitcad::frameMarginsFromText(QString(), margins));
  CHECK(!mitcad::frameMarginsFromText(QStringLiteral("8,31,8"), margins));
  CHECK(!mitcad::frameMarginsFromText(QStringLiteral("8,-1,8,8"), margins));
  CHECK(!mitcad::frameMarginsFromText(QStringLiteral("8,a,8,8"), margins));
  CHECK(!mitcad::frameMarginsFromText(QStringLiteral("8,9999,8,8"), margins));
}

void settingTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString path = directory.filePath(QStringLiteral("settings.ini"));
  {
    QSettings settings(path, QSettings::IniFormat);
    // None kept.
    CHECK(!mitcad::savedWindowPlace(settings).valid());
    // Kept and read back.
    mitcad::WindowPlace place;
    place.geometry = QRect(120, 90, 1000, 700);
    place.frame = QMargins(8, 31, 8, 8);
    place.frameKnown = true;
    place.screen = QStringLiteral("Screen 2");
    place.maximized = true;
    mitcad::saveWindowPlace(settings, place);
    const mitcad::WindowPlace read = mitcad::savedWindowPlace(settings);
    CHECK(read.valid());
    CHECK(read.geometry == QRect(120, 90, 1000, 700));
    CHECK(read.frameKnown);
    CHECK(read.frame == QMargins(8, 31, 8, 8));
    CHECK(read.screen == QStringLiteral("Screen 2"));
    CHECK(read.maximized);
    CHECK(!read.fullScreen);
    CHECK(mitcad::windowPlaceDescription(read) ==
          QStringLiteral("120,90 1000x700 (frame 8,31,8,8) on Screen 2, maximized"));
    // A place whose frame is not known keeps the frame kept before.
    mitcad::WindowPlace later;
    later.geometry = QRect(10, 20, 900, 600);
    later.fullScreen = true;
    mitcad::saveWindowPlace(settings, later);
    const mitcad::WindowPlace again = mitcad::savedWindowPlace(settings);
    CHECK(again.geometry == QRect(10, 20, 900, 600));
    CHECK(again.frameKnown && again.frame == QMargins(8, 31, 8, 8));
    CHECK(again.fullScreen && !again.maximized);
    CHECK(again.screen.isEmpty());
    // No place: nothing changes.
    mitcad::saveWindowPlace(settings, mitcad::WindowPlace());
    CHECK(mitcad::savedWindowPlace(settings).geometry == QRect(10, 20, 900, 600));
  }

  // As a UI test seeds it, written into the file (each in a file of its
  // own: QSettings caches a file it has read).
  const auto seeded = [&directory](const char* name, const char* text) {
    const QString file = directory.filePath(QString::fromLatin1(name));
    QFile out(file);
    if (out.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
      out.write(text);
      out.close();
    }
    return mitcad::savedWindowPlace(QSettings(file, QSettings::IniFormat));
  };
  const mitcad::WindowPlace distant =
      seeded("far.ini", "[mainWindow]\ngeometry=@Rect(5000 4000 900 600)\nmaximized=false\n");
  CHECK(distant.valid());
  CHECK(distant.geometry == QRect(5000, 4000, 900, 600));
  CHECK(!distant.frameKnown);
  CHECK(!distant.maximized);
  const mitcad::WindowPlace zoomed =
      seeded("zoomed.ini", "[mainWindow]\ngeometry=@Rect(40 30 900 600)\nframe=\"0,0,0,0\"\nmaximized=true\n");
  CHECK(zoomed.valid() && zoomed.maximized && zoomed.frameKnown && zoomed.frame.isNull());
  // The frame without quotes (an INI list) reads the same.
  const mitcad::WindowPlace listed =
      seeded("listed.ini", "[mainWindow]\ngeometry=@Rect(40 30 900 600)\nframe=8,31,8,8\n");
  CHECK(listed.frameKnown && listed.frame == QMargins(8, 31, 8, 8));
  // An empty or absurd geometry is no place.
  CHECK(!seeded("empty.ini", "[mainWindow]\ngeometry=@Rect(0 0 0 0)\n").valid());
  CHECK(!seeded("absurd.ini", "[mainWindow]\ngeometry=@Rect(0 0 900000 600)\n").valid());
  CHECK(!seeded("text.ini", "[mainWindow]\ngeometry=large\n").valid());
}

} // namespace

int windowPlaceTests() {
  fitTests();
  cascadeTests();
  marginTests();
  settingTests();
  return failures;
}
