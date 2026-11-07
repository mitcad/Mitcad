// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › "Small circles" (mitcad#879): Adaptive (the
// default) meshes the view's small circles and arcs by their size, in at
// least 12 pieces a turn; Full detail meshes every circle with the view's
// angle, as before (view/ViewTessellation.hpp, SmallCircles). Kept in the
// application's settings ("view/smallCircles": "adaptive" or "full").
// Changing it re-meshes the open designs' shown bodies (each window on
// SmallCirclesNotifier::changed, MainWindow::remeshDisplay); a design's
// display meshes in its result store are keyed by it
// (displayMeshStoreOptions). The exports' meshes do not follow it.

#include <optional>

#include <QObject>
#include <QString>

#include "view/ViewTessellation.hpp"

namespace mitcad {

// The id kept in the settings and logged, and back; nullopt for another.
QString smallCirclesId(SmallCircles choice);
std::optional<SmallCircles> smallCirclesFromId(const QString& id);

// The application's choice (QSettings of the application); Adaptive when
// none or an unknown one is kept.
SmallCircles savedSmallCircles();

// At the start, before any body is meshed: the view's meshes follow the
// saved choice.
void applySavedSmallCircles();

// Saves the choice, makes the view's meshes from now on follow it and tells
// SmallCirclesNotifier; false when it was already so.
bool saveSmallCircles(SmallCircles choice);

// Tells the windows that the choice changed (saveSmallCircles), so that
// they mesh their shown bodies again.
class SmallCirclesNotifier : public QObject {
  Q_OBJECT

public:
  static SmallCirclesNotifier& instance();

signals:
  void changed();
};

} // namespace mitcad
