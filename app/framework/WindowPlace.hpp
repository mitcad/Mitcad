// SPDX-License-Identifier: MIT
#pragma once

// The design window's size, place and state kept between starts (mitcad#1710,
// GitHub mitcad/Mitcad#283, app/COMMANDS.md "The window's size and place"):
// saved when a design window closes and at quit, restored for the first
// window at the next start, and the last size for a new window, cascaded
// from the window it opens from. A kept place that is no longer on the
// connected screens (a removed monitor, another resolution, a window half
// off-screen) is moved, and shrunk if needed, into one screen's available
// area, so that the window and its title bar are always reachable.
//
// Kept in the application's settings, group "mainWindow":
//   geometry    the window's own area (without the title bar and borders)
//               in its normal state, global logical pixels (QRect)
//   frame       the title bar's and borders' sizes "left,top,right,bottom",
//               measured in the normal state (absent: not known yet)
//   screen      the name of the screen the window was on
//   maximized   true when it was maximized (macOS: zoomed)
//   fullScreen  true when it was full screen

#include <QList>
#include <QMargins>
#include <QRect>
#include <QSize>
#include <QString>

class QSettings;
class QWidget;

namespace mitcad {

struct WindowPlace {
  // The window's own area in its normal state; invalid when none is kept.
  QRect geometry;
  // The title bar and borders around it, when known.
  QMargins frame;
  bool frameKnown = false;
  QString screen;
  bool maximized = false;
  bool fullScreen = false;
  bool valid() const { return geometry.isValid(); }
};

// A window's frame (title bar and borders included) on the screens whose
// available areas (without the task bar, the Dock, the menu bar) are
// `available`: unchanged when it lies wholly within one of them; otherwise
// moved, and made smaller where it is larger, into the area `preferred`
// (an index into `available`; out of range: the first), so that all of it,
// its title bar first, is on that screen. Without screens, unchanged.
QRect fitFrameToScreens(const QRect& frame, const QList<QRect>& available, int preferred);

// The frame of a new window of `frameSize` cascaded from the frame `from`
// of the window it opens from: `step` pixels right of and below it, so that
// it does not cover that window exactly; back at the available area's left
// or top edge where it would reach past its right or bottom edge.
QRect cascadedFrame(const QRect& from, const QSize& frameSize, int step, const QRect& available);

// The title bar and borders as kept, "8,31,8,8"; false for text that is not
// four numbers of 0 to 500.
bool frameMarginsFromText(const QString& text, QMargins& margins);
QString frameMarginsText(const QMargins& margins);

// The kept place in `settings` (invalid geometry when none is kept), and
// `place` kept there. The frame is kept only when known; otherwise the one
// kept before stays.
WindowPlace savedWindowPlace(const QSettings& settings);
void saveWindowPlace(QSettings& settings, const WindowPlace& place);

// The place of a shown top-level design window: its normal geometry (also
// while maximized or full screen), its frame when it is in its normal
// state, its screen and state. Invalid for a minimized window, whose
// normal geometry the platform may not report.
WindowPlace windowPlaceOf(const QWidget* window);

// Saves the place of a top-level design window (QSettings) unless it is
// minimized or was never shown; logs "Window place saved: ...".
void saveWindowPlace(const QWidget* window);

// Before the first design window shows: its kept place, fitted to the
// connected screens, with its maximized or full-screen state; without a
// kept place startWindowSize(). Logs "Window place restored: ..." or
// "Window place: none saved, ...".
void restoreWindowPlace(QWidget* window);

// Before a new design window shows beside `from` (the window it opens
// from, shown): `from`'s place saved as the last one, and the new window at
// its size, cascaded from it and fitted to the screens, in the normal
// state. Logs "Window place cascaded: ...".
void cascadeWindowPlace(QWidget* window, const QWidget* from);

// "100,80 1000x700 (frame 8,31,8,8) on <screen>, maximized" for the logs.
QString windowPlaceDescription(const WindowPlace& place);

} // namespace mitcad
