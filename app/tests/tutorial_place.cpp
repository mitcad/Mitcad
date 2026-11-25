// SPDX-License-Identifier: MIT
// The tutorial pane's places (mitcad#2367, tutorials/TutorialPlace.hpp,
// framework/PaneMotion): the names a page's window.mitcad.place takes and
// the ones refused, the centred pane's rectangle in areas large and small,
// and the move's length with and without reduced motion
// (MITCAD_REDUCED_MOTION deciding).
#include "UnitTests.hpp"
#include "framework/PaneMotion.hpp"
#include "tutorials/TutorialPlace.hpp"

#include <cstdio>
#include <initializer_list>

#include <QByteArray>
#include <QRect>
#include <QString>
#include <QtGlobal>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "tutorial_place.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

bool parses(const char* name, TutorialPlace expected) {
  TutorialPlace place = expected == TutorialPlace::Left ? TutorialPlace::Right : TutorialPlace::Left;
  return parseTutorialPlace(QString::fromLatin1(name), &place) && place == expected;
}

bool refused(const char* name) {
  TutorialPlace place = TutorialPlace::Right;
  return !parseTutorialPlace(QString::fromLatin1(name), &place) && place == TutorialPlace::Right;
}

void names() {
  CHECK(parses("left", TutorialPlace::Left));
  CHECK(parses("right", TutorialPlace::Right));
  CHECK(parses("centre", TutorialPlace::Centre));
  CHECK(parses("center", TutorialPlace::Centre));
  // Only these, as written: the place is left as it was otherwise.
  CHECK(refused(""));
  CHECK(refused("Left"));
  CHECK(refused("middle"));
  CHECK(refused("top"));
  CHECK(refused(" right"));
  // The answers' and the log's names read back as the same places.
  for (TutorialPlace place : {TutorialPlace::Left, TutorialPlace::Right, TutorialPlace::Centre}) {
    TutorialPlace read = place == TutorialPlace::Left ? TutorialPlace::Right : TutorialPlace::Left;
    CHECK(parseTutorialPlace(tutorialPlaceName(place), &read) && read == place);
  }
  CHECK(tutorialPlaceName(TutorialPlace::Centre) == QLatin1String("centre"));
}

void centred() {
  // Most of a large area: four fifths of its width, nine tenths of its
  // height, in its middle.
  const QRect large(12, 100, 1000, 600);
  const QRect pane = centredPaneRect(large);
  CHECK(pane.width() == 800);
  CHECK(pane.height() == 540);
  CHECK(pane.center().x() == large.center().x() || pane.center().x() + 1 == large.center().x());
  CHECK(pane.center().y() == large.center().y() || pane.center().y() + 1 == large.center().y());
  CHECK(large.contains(pane));
  // A smaller area: at least 480 x 320, where it has room for that.
  const QRect small(0, 0, 520, 340);
  const QRect smallPane = centredPaneRect(small);
  CHECK(smallPane.width() == 480);
  CHECK(smallPane.height() == 320);
  CHECK(small.contains(smallPane));
  // Smaller still: the whole area, never more.
  const QRect tiny(5, 7, 300, 200);
  CHECK(centredPaneRect(tiny) == tiny);
  // No area: an empty rectangle at its corner.
  CHECK(centredPaneRect(QRect(3, 4, 0, 10)).isEmpty());
}

void motion() {
  const QByteArray before = qgetenv("MITCAD_REDUCED_MOTION");
  const bool had = qEnvironmentVariableIsSet("MITCAD_REDUCED_MOTION");
  qputenv("MITCAD_REDUCED_MOTION", "1");
  CHECK(reducedMotion());
  CHECK(paneMoveMs() == 0);
  qputenv("MITCAD_REDUCED_MOTION", "0");
  CHECK(!reducedMotion());
  CHECK(paneMoveMs() == kPaneMoveMs);
  CHECK(kPaneMoveMs == 250);
  if (had) {
    qputenv("MITCAD_REDUCED_MOTION", before);
  } else {
    qunsetenv("MITCAD_REDUCED_MOTION");
  }
}

} // namespace

int tutorialPlaceTests() {
  failures = 0;
  names();
  centred();
  motion();
  return failures;
}
MITCAD_UNIT_TEST(tutorialPlaceTests);
