// SPDX-License-Identifier: MIT
#pragma once

// A 3D sketch's Fillet without the view (mitcad#1548): which points of the
// sketch are corners it rounds (where exactly two ordinary lines end, not
// in line), which two lines make a corner, the tangent arc of a radius at
// a corner and why it does not fit, the radius the pointer's ray gives
// along the corner's bisector (a dragged radius), and the
// `sketch3d.fillet` command a pick sends. Local coordinates (mm), the
// geometry the model makes (core/sketch/src/spatial_edit.rs). Plain Qt
// Core, so the unit tests need no view.

#include <optional>
#include <vector>

#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

#include "Sketch3dDraw.hpp"
#include "Sketch3dModel.hpp"

namespace mitcad::sketch {

// The corner two lines of the sketch make, as the model rounds it.
struct FilletCorner3 {
  QString point;  // the shared end
  QString first;  // the lines: a corner point's in the sketch's order
  QString second;
  bool byLines = false; // picked as two lines (the command names them)
  Xyz at{};             // the corner
  Xyz firstEnd{};       // each line's other end
  Xyz secondEnd{};
};

// The corner at `point`: the two ordinary (not construction) lines that end
// there, when they are exactly two and not in line; none otherwise.
std::optional<FilletCorner3> filletCornerAt(const Sketch3dModel& model, const QString& point);
// The corner of the lines `a` and `b`: their shared end; none when they
// are not two lines of the sketch, share no end or are in line there.
std::optional<FilletCorner3> filletCornerOfLines(const Sketch3dModel& model, const QString& a, const QString& b);
// Every point of the sketch that is a corner, in the sketch's order.
QStringList filletCornerPoints(const Sketch3dModel& model);
// Why `a` and `b` make no corner (translated, in words: "Line 1 and Line 3
// do not share an end", mitcad#1653); empty when they do.
QString filletLinesProblem(const Sketch3dModel& model, const QString& a, const QString& b);

// The tangent arc at a corner: counter-clockwise about `normal` from the
// tangent point on the first line (`start`) to the one on the second.
struct FilletArc {
  Xyz center{};
  Xyz normal{0.0, 0.0, 1.0};
  Xyz start{};
  Xyz end{};
  double radius = 0.0;
  double setback = 0.0; // from the corner to each tangent point
  double sweep = 0.0;   // radians
};

// The arc of `radius` at `corner`; none, with the reason in `problem`
// (translated, the lines named in words as the palette names them: "does
// not fit Line 2", mitcad#1653), when the radius is not above 1e-6 mm or
// its tangent point would not lie inside either line.
std::optional<FilletArc> filletArc(const Sketch3dModel& model, const FilletCorner3& corner, double radius,
                                   QString* problem);
// The largest radius whose tangent points still lie inside both lines.
double filletMaxRadius(const FilletCorner3& corner);
// The radius whose arc's middle is where the pointer's ray passes nearest
// to the corner's bisector (inside the corner), kept below the largest
// that fits; none when the ray gives no place there (the bisector seen
// end-on and the corner's plane edge-on, or the place outside the corner).
std::optional<double> filletRadiusOnRay(const FilletCorner3& corner, const Ray3& ray);
// `segments` + 1 points along the arc.
std::vector<Xyz> filletArcPoints(const FilletArc& arc, int segments);

// sketch3d.fillet of the corner: by its point, or by its two lines when it
// was picked so; `radius` a number (mm) or the typed expression.
QJsonObject filletCommandFor(const QString& sketch, const FilletCorner3& corner, const QJsonValue& radius);

} // namespace mitcad::sketch
