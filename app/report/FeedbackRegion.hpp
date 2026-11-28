// SPDX-License-Identifier: MIT
#pragma once

// Send Feedback on a Region (mitcad#2616): the region a drag picks and its
// picture from the screens. Qt Gui only (the helpers are in app.unit); the
// overlay that takes the drag is RegionPicker.hpp, the command
// ReportCenter.hpp.

#include <QImage>
#include <QList>
#include <QPoint>
#include <QRect>

namespace mitcad::report {

// The least width and height of a region, in logical pixels: a smaller drag
// is a click, which cancels.
inline constexpr int kRegionMinSide = 8;

// The region a drag from `from` to `to` picks (global logical coordinates,
// in either direction): the rectangle between them, `from` and `to` its
// opposite corners, clipped to `screens` (the bounding box of its parts on
// them). Null for a click (a side under kRegionMinSide before or after the
// clip) or a drag outside every screen.
QRect regionFromDrag(const QPoint& from, const QPoint& to, const QList<QRect>& screens);

// The region's parts on each of `screens`, in their order (an empty
// rectangle for a screen it does not touch).
QList<QRect> regionParts(const QRect& region, const QList<QRect>& screens);

// The picture of `region` (global logical coordinates) from the screens, as
// the user sees it, separate windows and the OpenGL view included: each
// screen's part from QScreen::grabWindow(0, …), in full device pixels (the
// highest device pixel ratio of the screens it touches; a screen with a
// lower one scaled up). Null when no screen gave its part (no screen, or a
// system that refuses it: a Wayland session, macOS without the Screen
// Recording permission).
QImage grabScreenRegion(const QRect& region);

} // namespace mitcad::report
