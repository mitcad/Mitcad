// SPDX-License-Identifier: MIT
#pragma once

// Snapping of the cursor in sketch mode: to points (line
// ends, centres, the origin), midpoints, quadrants, intersections, onto
// curves and optionally to a grid. Tolerances are in screen pixels, so
// snapping feels the same at any zoom.

#include <functional>

#include <QPointF>
#include <QSet>
#include <QString>

#include "SketchModel.hpp"

namespace mitcad::sketch {

struct Snap {
  enum class Kind { None, Grid, Point, Center, Origin, Midpoint, Quadrant, Intersection, OnCurve };

  Kind kind = Kind::None;
  V2 at;          // the snapped position
  QString point;  // Point, Center: the point (Origin: the origin's point, if any)
  QString curve;  // Center: its circle; Midpoint, Quadrant, OnCurve, Intersection: the curve
  QString curve2; // Intersection: the other curve
  QPointF screen; // where the cursor was

  // On existing geometry (a new point there gets a constraint).
  bool onGeometry() const { return kind != Kind::None && kind != Kind::Grid; }
  // At an existing point, which a new curve can share.
  bool atPoint() const { return kind == Kind::Point || kind == Kind::Center; }
  QString describe() const;
};

struct SnapSettings {
  const SketchModel* model = nullptr;
  std::function<QPointF(const V2&)> toScreen;
  bool grid = true;
  double gridStep = 1.0; // mm
  QSet<QString> skipPoints;
  QSet<QString> skipCurves;
};

// The snapped position for the cursor at `screen`, where `raw` is the
// cursor's point on the sketch plane.
Snap snap(const SnapSettings& settings, const V2& raw, const QPointF& screen);

} // namespace mitcad::sketch
