// SPDX-License-Identifier: MIT
#pragma once

// Where the tutorial pane is (mitcad#2367, docs/tutorials.md "Placement"):
// the page's window.mitcad.place asks for a side; plain Qt Core.

#include <QString>

namespace mitcad {

enum class TutorialPlace {
  Left,   // the left column (the pane's place as a tutorial opens)
  Right,  // the right column: the Browser at the left stays free
  Centre, // centred over the view, most of the window
};

// The place a page names: "left", "right", "centre" (or "center"); false
// for anything else.
inline bool parseTutorialPlace(const QString& name, TutorialPlace* place) {
  if (name == QLatin1String("left")) {
    *place = TutorialPlace::Left;
  } else if (name == QLatin1String("right")) {
    *place = TutorialPlace::Right;
  } else if (name == QLatin1String("centre") || name == QLatin1String("center")) {
    *place = TutorialPlace::Centre;
  } else {
    return false;
  }
  return true;
}

// Its name in the bridge's answers and the log.
inline QString tutorialPlaceName(TutorialPlace place) {
  switch (place) {
  case TutorialPlace::Left: return QStringLiteral("left");
  case TutorialPlace::Right: return QStringLiteral("right");
  case TutorialPlace::Centre: return QStringLiteral("centre");
  }
  return QStringLiteral("left");
}

} // namespace mitcad
