// SPDX-License-Identifier: MIT
#pragma once

// The Center of Mass marker (mitcad#1094, app/COMMANDS.md "Center of
// Mass"): what the browser's row and the 3D view show of the
// `center_of_mass` query's answer (DocumentSnapshot::centerOfMass), its
// tooltip, and the marker's picture: a crossed circle of constant size on
// screen, two opposite quadrants filled in what an inspection points at
// (ColorRole::PreviewTarget) on a disc in the background's tone
// (ColorRole::SketchHalo), so that it reads over a body on a light and a
// dark background alike. Plain Qt Gui: the app's unit tests take this file
// alone.

#include <array>
#include <optional>

#include <QColor>
#include <QImage>
#include <QJsonObject>
#include <QString>

namespace mitcad {

struct CenterOfMassShown {
  // The browser row's light bulb.
  bool visible = true;
  // In the design's coordinates (mm); none without bodies or when the
  // center could not be computed (error).
  std::optional<std::array<double, 3>> point;
  double mass = 0.0;    // kg
  int bodies = 0;       // counted, one per placement
  int defaultMaterial = 0; // of them without a material, counted as steel
  QString error;
};

// The marker from the `center_of_mass` query's answer, with `marker`
// {"visible"} and, when the query failed, `error`; none when the design
// has no marker (an empty object or a null `marker`).
std::optional<CenterOfMassShown> centerOfMassOf(const QJsonObject& answer);

// The browser row's and the marker's tooltip: the coordinates (mm), the
// total mass and how many bodies count as steel for lack of a material;
// or why there is no center.
QString centerOfMassTip(const CenterOfMassShown& shown);

// For the UI tests: "Center of Mass: at x, y, z mm, mass m kg, n bodies,
// k without a material (shown|hidden)", "Center of Mass: no bodies
// (shown)", "Center of Mass: failed: <error>" or "Center of Mass: none".
// The coordinates with nine decimals, the mass with twelve.
QString centerOfMassLog(const std::optional<CenterOfMassShown>& shown);

// The marker's circle in view pixels (device independent): its radius, its
// ring's width and the disc in the background's tone around it.
constexpr double kCenterOfMassRadius = 7.0;
constexpr double kCenterOfMassRing = 1.6;
constexpr double kCenterOfMassHalo = 1.5;

// The marker's colors on a light or a dark background: the quadrants and
// the ring, and the disc under them.
QColor centerOfMassInk();
QColor centerOfMassDisc(bool darkBackground);

// The marker as an image for OCCT's marker (drawn pixel for pixel),
// `ratio` device pixels per view pixel: odd sized, centered on the middle
// pixel, transparent around; straight alpha (RGBA8888).
QImage centerOfMassImage(bool darkBackground, double ratio);

} // namespace mitcad
