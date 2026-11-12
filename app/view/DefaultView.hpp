// SPDX-License-Identifier: MIT
// The 3D view's default direction (mitcad#1698): Home without a design's own
// home view, a design read into a view that still looked at a new design's
// Top, and a new design's first sketch turn to it. It looks from the front
// right and above as the isometric view does, but a few degrees off the
// cube's diagonal: seen exactly along the diagonal, the origin planes'
// edges lie on top of each other pairwise (the XY plane's edge at x = +s
// and the YZ plane's at z = -s on one line, and so on), so the three planes
// are hard to tell apart. View › Isometric and the orientation cube's
// corners stay on the exact diagonal (isometricViewDirection).
//
// Off the diagonal, edges along the same axis in two planes are apart on
// the screen by s·||a|-|b||/sqrt(a²+b²), where a and b are the other two
// components of the view's direction: so the components' sizes are kept
// apart pairwise. Azimuth 41° and elevation 30° give (0.568, 0.654, 0.5):
// the planes' parallel edges at least 9 % of the half side apart, every
// plane seen at least half open (|n·d| ≥ 0.5), the axes 53° or more apart
// on the screen. tests/default_view.cpp checks it.
#pragma once

#include <cmath>

#include <gp_Dir.hxx>

namespace mitcad {

// Degrees from the front (-Y) towards the right (+X), and above the XY
// plane. The isometric view's are 45° and 35.26°.
constexpr double kDefaultViewAzimuth = 41.0;
constexpr double kDefaultViewElevation = 30.0;

// From the eye to the model, as the standard views give their direction.
inline gp_Dir defaultViewDirection() {
  constexpr double kDegree = 3.14159265358979323846 / 180.0;
  const double azimuth = kDefaultViewAzimuth * kDegree;
  const double elevation = kDefaultViewElevation * kDegree;
  return gp_Dir(-std::cos(elevation) * std::sin(azimuth), std::cos(elevation) * std::cos(azimuth),
                -std::sin(elevation));
}

// Z up, as the isometric view.
inline gp_Dir defaultViewUp() { return gp_Dir(0.0, 0.0, 1.0); }

// View › Isometric (the standard view `iso`) and the orientation cube's
// corner from the front right and above: the exact cube diagonal.
inline gp_Dir isometricViewDirection() { return gp_Dir(-1.0, 1.0, -1.0); }

} // namespace mitcad
