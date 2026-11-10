// SPDX-License-Identifier: MIT
#pragma once

// The arcs the sketch Line tool draws by a drag from the end of a curve
// (mitcad#1393): tangent to the curve when the drag leaves along it, normal
// (square) to it when the drag leaves across it, placed by the cursor and
// by a typed radius and sweep. Plain plane geometry, unit tested in
// app/tests/line_arcs.cpp; the model makes the same arcs (`sketch.arc`
// with `tangent` or `normal`).

#include <optional>

#include "SketchGeometry.hpp"

namespace mitcad::sketch {

// The arc from `start` leaving along `direction` to `end`; none when it
// would be straight.
std::optional<Curve2> tangentArc(const V2& start, const V2& direction, const V2& end);

enum class LineArcKind { Tangent, Normal };

// The widest angle between a drag and the curve's leaving direction that
// still draws a tangent arc: 45 degrees.
constexpr double kLineArcTangentCone = kPi / 4.0;

// The kind of arc a drag `drag` (from the curve's end) draws from a curve
// that leaves its end along `leaving`: tangent within kLineArcTangentCone
// of `leaving`, else normal (when the curve takes one: `normalAllowed`).
LineArcKind lineArcKind(const V2& leaving, const V2& drag, bool normalAllowed);

// The direction an arc of `kind` leaves `start` with toward `toward`: the
// curve's own for a tangent arc, square to it on `toward`'s side for a
// normal one (as the model takes the side from the arc's end).
V2 lineArcDirection(LineArcKind kind, const V2& leaving, const V2& start, const V2& toward);

// The arc of `kind` from `start` (the curve leaving along `leaving`) to
// `end`; none when it would be straight. A normal arc leaves toward the
// side `end` is on.
std::optional<Curve2> lineArc(LineArcKind kind, const V2& start, const V2& leaving, const V2& end);

// The same, a normal arc leaving toward the side `toward` (the cursor) is
// on (mitcad#1608): one that sweeps 180 degrees or more ends behind the
// curve, on the other side, and keeps the side it is drawn to (the model's
// `sketch.arc` with `normal` and `toward`).
std::optional<Curve2> lineArc(LineArcKind kind, const V2& start, const V2& leaving, const V2& end,
                              const V2& toward);

// Where an arc leaving `start` along the unit direction `direction` ends:
// at `cursor`; with a typed `radius`, where the circle of that radius on
// the cursor's side meets the line to the cursor from its centre; with a
// typed `sweepAngle` (radians), that far round (the radius from the
// cursor: the chord's end nearest the cursor), or with both, there
// exactly.
V2 lineArcEnd(const V2& start, const V2& direction, const V2& cursor, std::optional<double> radius,
              std::optional<double> sweepAngle);

} // namespace mitcad::sketch
