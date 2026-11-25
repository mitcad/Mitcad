// SPDX-License-Identifier: MIT
#pragma once

// A pane that moves between its places (mitcad#2367: the tutorial pane at
// the left, at the right or centred over the view): how long the move
// takes, whether the system asks for none, and the centred place.

#include <QRect>

namespace mitcad {

// How long a pane's move to another place takes, in milliseconds.
inline constexpr int kPaneMoveMs = 250;

// Whether the system's reduced-motion setting is on: Windows' Animation
// effects off (SPI_GETCLIENTAREAANIMATION), macOS's Reduce motion
// (com.apple.universalaccess reduceMotion), on Linux KDE's animation speed
// at its instant end (kdeglobals AnimationDurationFactor 0) or else GNOME's
// animations off (gsettings org.gnome.desktop.interface enable-animations).
// MITCAD_REDUCED_MOTION=1 or 0 decides instead (the UI tests). Read anew
// on each call: the user may change it while MitCad runs.
bool reducedMotion();

// kPaneMoveMs, or 0 (the pane is placed at once) with reduced motion.
int paneMoveMs();

// A centred pane in `area` (its introduction and review): most of it, four
// fifths of its width and nine tenths of its height, at least 480 x 320
// where the area has room for that, never larger than the area.
QRect centredPaneRect(const QRect& area);

} // namespace mitcad
