// SPDX-License-Identifier: MIT
#pragma once

// The pointer of the sketch tools (mitcad#3): a slim precision cross that
// Mitcad draws itself, so that it looks the same on every platform and
// cursor theme (the system's crosshair can be large enough to hide what is
// drawn).

#include <QCursor>
#include <QPixmap>
#include <QPoint>
#include <QRect>
#include <QString>

namespace mitcad {

// Where the parts of a precision cursor are, in device pixels of its
// pixmap.
struct PrecisionCursorLayout {
  QRect gap;    // empty around the hot spot (the pick point stays visible)
  QRect arms;   // the cross with its halo
  QRect badge;  // the tool's icon
  QPoint hotSpot; // in logical pixels
  int size = 0; // the pixmap's width and height
};

// The layout at device pixel ratio `dpr`, `scale` times the standard size
// (the system's pointer size; see pointerScale): a cross of four 1 px
// arms, 19 x 19 logical pixels with a 5 px gap in the middle, its hot spot
// in the middle; a 12 px badge 10 px right of and below the hot spot.
// 32 x 32 logical pixels at the standard size.
PrecisionCursorLayout precisionCursorLayout(qreal dpr, qreal scale = 1.0);

// The cursor's picture: dark arms (#1b1e23) with a 1 px light halo
// (white at 85 %), the tool's icon `toolIcon` (an icon of app/icons, drawn
// small on a light plate; none for an empty or unknown name), and, while
// the pick snaps to a point (`snapping`), a hollow 5 px square in the gap.
// Lines are whole device pixels, so they stay sharp at 100, 150 and 200 %.
QPixmap precisionCursorPixmap(const QString& toolIcon, qreal dpr, bool snapping, qreal scale = 1.0);

// The precision cursor of a sketch tool for a screen of ratio `dpr`, at
// the system's pointer size; cached by tool, ratio and snapping.
QCursor precisionCursor(const QString& toolIcon, qreal dpr, bool snapping = false);

// The system's pointer size against the standard one, where it can be
// read: XCURSOR_SIZE on Linux (24 standard), the pointer size of Windows'
// accessibility settings (32 standard). Between 1 and 2.
qreal pointerScale();

} // namespace mitcad
