// SPDX-License-Identifier: MIT
#pragma once

// The Browser's preferences (mitcad#1699).

#include <QObject>
#include <QString>

namespace mitcad {

// Preferences > General > "Show used sketches under their features"
// (settings "browser/consumeSketches"), off by default: while it is on,
// the Browser lists each component's features in a Features folder, with
// the sketches each uses under it (a sketch several features use under each
// of them), and keeps only the sketches no feature uses in the Sketches
// folder (BrowserPanel::setConsumeSketches). Off, the Browser is as before.
bool browserConsumeSketches();
// Saves the choice and tells every window's Browser, which rebuilds its
// rows at once.
void setBrowserConsumeSketches(bool on);

// Where a Browser row's Activate radio button or Show eye is (mitcad#2223,
// Preferences > Display > Browser Activate button and Browser Show eye):
// Classic, the default, keeps it where it always was (the radio button
// right after the component's name, the eye before the row's icon); Right
// puts it in one column at the right edge of the visible row, the same x on
// every row (BrowserPanel::setControlPlaces).
enum class BrowserControlPlace { Classic, Right };
// The place's settings value ("classic", "right") and back; an unknown
// value is Classic.
QString browserControlPlaceId(BrowserControlPlace place);
BrowserControlPlace browserControlPlaceFromId(const QString& id);
// The saved places (settings "browser/activateButtonPlace" and
// "browser/showEyePlace"), Classic without a saved choice.
BrowserControlPlace browserActivatePlace();
BrowserControlPlace browserEyePlace();
// Save a place and tell every window's Browser, which lays its rows out
// again at once; nothing when it is already the saved one. Restore Defaults
// saves Classic for both.
void setBrowserActivatePlace(BrowserControlPlace place);
void setBrowserEyePlace(BrowserControlPlace place);

// How the timeline is shown (mitcad#2427, GitHub mitcad/Mitcad#581,
// Preferences > Display > Timeline): Strip, the default, the bar of icons
// at the bottom; History list, a panel in the left column under the Browser
// with a row per timeline item, its name beside its icon, and the marker a
// bar between the rows (TimelineWidget::Presentation). Every window
// follows a change at once.
enum class TimelinePresentation { Strip, HistoryList };
// The choice's settings value ("strip", "list") and back; an unknown value
// is Strip.
QString timelinePresentationId(TimelinePresentation presentation);
TimelinePresentation timelinePresentationFromId(const QString& id);
// The saved choice (settings "timeline/presentation"), Strip without one.
TimelinePresentation timelinePresentation();
// Saves the choice and tells every window (timelinePresentationChanged)
// when it changed; Restore Defaults saves Strip.
void setTimelinePresentation(TimelinePresentation presentation);

// Tells the windows' Browsers that the preference changed.
class BrowserPreferencesNotifier : public QObject {
  Q_OBJECT

public:
  static BrowserPreferencesNotifier& instance();

signals:
  void consumeSketchesChanged(bool on);
  // The Activate radio button's or the Show eye's place changed
  // (mitcad#2223); both are read again.
  void controlPlacesChanged();
  // The timeline's presentation changed (mitcad#2427): Strip or History
  // list, read again with timelinePresentation().
  void timelinePresentationChanged();
};

} // namespace mitcad
