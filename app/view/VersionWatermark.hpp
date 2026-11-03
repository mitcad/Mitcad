// SPDX-License-Identifier: MIT
#pragma once

#include <QObject>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QWidget>

namespace mitcad {

class OcctViewer;

// MitCad's version over the 3D view's lower right (mitcad#1209, app/COMMANDS.md
// "Main window"): a small, low-contrast "MitCad 0.42.0" in the view's font,
// in the theme's secondary text colour at reduced opacity, just above the
// timeline (in the Floating layout above its card and the message band,
// in the Docked layout at the view's bottom). It shows in every layout, on
// every platform and in the sketch and drawing modes (there at the
// sheet's lower right, beside the drawing's panel).
//
// A child of the view, as small as its text and transparent for the mouse:
// clicks, drags and the wheel at its place reach the view. It keeps clear
// of what floats over the view (the cards and the navigation bar, the
// message band, the orientation cube; OcctViewer::coveredRects and
// overlayInsets): where one would overlap it, it moves up above it, or
// beside it to its left for a panel along the view's right edge; where
// there is no room it is hidden. Never under a card: the cards are above
// the view and its children. It is drawn on the window, not into the
// view's picture, so the view's images (Export Snapshot, the screenshots,
// thumbnails, renders) never show it.
//
// Preferences > General > Show the version in the view (on by default)
// turns it off in every window; the window title keeps the version.
//
// For UI tests it logs, when it changes and the window is visible,
// "Version watermark 'MitCad 0.42.0' at cx,cy: box x,y wxh, view x,y wxh"
// (its centre, its rectangle and the view's in the window's coordinates)
// or "Version watermark hidden: preference" ("hidden: no room").
class VersionWatermark : public QWidget {
  Q_OBJECT

public:
  explicit VersionWatermark(OcctViewer* viewer);

  // "MitCad <version>" (QCoreApplication::applicationVersion).
  static QString text();

  // A part of the view to keep to instead of the view's free area (a
  // drawing's sheet while it is open, beside its panel); null: the view's.
  // Its moves and size changes are followed.
  void setArea(QWidget* area);

  // Places it again once the event loop is back.
  void requestPlace();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void changeEvent(QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;

private:
  void place();
  void logLine(const QString& line);
  bool darkBackground() const;

  OcctViewer* m_viewer;
  QPointer<QWidget> m_area;
  QPointer<QWidget> m_areaHolder; // the view's child that holds m_area
  bool m_placeQueued = false;
  bool m_dark = false;
  QString m_logged;
};

// Preferences > General > Show the version in the view (mitcad#1209): on
// unless turned off.
bool versionWatermarkShown();
// Saves the choice and tells every window's watermark.
void setVersionWatermarkShown(bool shown);

// Tells the windows' watermarks that the preference changed.
class VersionWatermarkNotifier : public QObject {
  Q_OBJECT

public:
  static VersionWatermarkNotifier& instance();

signals:
  void shownChanged(bool shown);
};

} // namespace mitcad
