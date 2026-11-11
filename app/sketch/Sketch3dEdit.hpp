// SPDX-License-Identifier: MIT
#pragma once

// Editing a 3D sketch without the view (mitcad#1526): the relations and
// dimensions the 3D Sketch Palette and the SKETCH tab's constraints add
// from the selection (Coincident, Point on Curve, Midpoint, Tangent,
// Equal, Fix and Unfix, X, Y and Z alignment of lines and arc normals, On
// Sketch Plane, Release from Plane, Release Plane, Detach; Parallel,
// Perpendicular, Angle, Symmetric, Concentric, Same Center, Curvature and
// On Plane, mitcad#1661; Length, Delta X, Y and Z, Radius, Diameter, Arc
// Length, Spline Length, Angle and Tangent Magnitude) as
// `sketch3d.*` commands, or why the selection takes none; what deleting
// points takes with it; the drag goals of a point, a curve and a spline's
// tangent handle; the geometry a drag's preview gives, sampled for the
// overlay; and the words of the relations, dimensions and references the
// palette lists. Everything is in the sketch's local coordinates (mm).
// Plain Qt Core, so the unit tests need no view.

#include <cmath>
#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

#include "Sketch3dDraw.hpp"
#include "Sketch3dModel.hpp"

namespace mitcad::sketch {

// The relations an edit adds or takes away from the selection.
enum class Relation3 {
  Coincident,       // two points; a point and a curve: Point on Curve
  PointOnCurve,     // a point and a curve
  Midpoint,         // a point and a line, an arc or a reference curve
  Tangent,          // two curves at a shared end (or a selected point)
  Equal,            // two lines' lengths, two arcs' or circles' radii
  Fix,              // points (and the points of curves) where they are
  Unfix,            // their Fix only
  AlignX,           // lines along X, arcs' and circles' normals along X
  AlignY,
  AlignZ,
  OnSketchPlane,    // points kept in the sketch plane
  ReleaseFromPlane, // points out of On Sketch Plane
  ReleasePlane,     // plane-bound arcs' and circles' normals freed
  Detach,           // points' attachments and relations with reference curves
  // mitcad#1661: directions are lines' and arcs' and circles' normals.
  Parallel,      // two directions parallel, no intersection implied
  Perpendicular, // two directions at right angles
  Angle,         // two directions held at their angle: a driving angle
  Symmetric,     // two points mirrored across a picked plane
  Concentric,    // two arcs or circles: one centre, parallel planes
  SameCenter,    // two arcs or circles: one centre only
  Curvature,     // G2 where a spline ends and a line, arc or spline joins
  OnPlane,       // points on a picked plane
};

// The dimensions an edit adds from the selection (Angle and Tangent
// Magnitude: mitcad#1661; a spline's length drives since then too).
enum class Dimension3 {
  Length,
  DeltaX,
  DeltaY,
  DeltaZ,
  Radius,
  Diameter,
  ArcLength,
  SplineLength,
  Angle,
  TangentMagnitude
};

// Every relation and dimension, in the palette's order.
std::vector<Relation3> allRelations();
std::vector<Dimension3> allDimensions();
// "Coincident", "Align X", "Delta Z" (translated); "coincident",
// "align_x", "delta_z" for the logs and the palette's control names.
QString relationName(Relation3 relation);
QString relationKey(Relation3 relation);
QString dimensionName(Dimension3 dimension);
QString dimensionKey(Dimension3 dimension);

// The sketch's points and curves that are selected; `plane` the plane a
// Symmetric or On Plane relation takes, picked after its button
// (mitcad#1661: `{"frame": "z"}`, `{"face": {...}}`, `{"construction":
// "F3"}`, `{"sketch": "F4"}`), none before.
struct EditSelection {
  QStringList points;
  QStringList curves;
  QJsonValue plane;
  bool empty() const { return points.isEmpty() && curves.isEmpty(); }
};

// Whether a relation takes a plane picked after its button (Symmetric, On
// Plane).
bool takesPlane(Relation3 relation);

// The commands of an edit (one undo step), or why the selection takes
// none (`problem`, translated).
struct EditCommand {
  QJsonArray commands;
  QString problem;
  bool valid() const { return problem.isEmpty() && !commands.isEmpty(); }
};

EditCommand relationCommand(const Sketch3dModel& model, const QString& sketch, Relation3 relation,
                            const EditSelection& selection);
EditCommand dimensionCommand(const Sketch3dModel& model, const QString& sketch, Dimension3 dimension,
                             const EditSelection& selection);
// The SKETCH tab's Sketch Dimension (D) in a 3D sketch: a line's Length,
// an arc's Radius, a circle's Diameter, a spline's driven Spline Length;
// two points take Delta X, Y or Z from the palette.
EditCommand autoDimensionCommand(const Sketch3dModel& model, const QString& sketch, const EditSelection& selection);
// Fix (the SKETCH tab's) toggles: Unfix when every selected point is
// fixed already.
Relation3 fixToggle(const Sketch3dModel& model, const EditSelection& selection);
// Whether a point has a Fix.
bool isFixed(const Sketch3dModel& model, const QString& point);

// What deleting `items` (points, curves, relations, dimensions) takes with
// it: the curves, relations and dimensions not listed that use a listed
// point (sketch3d.remove's `dependents`), in the sketch's order.
QStringList dependentsOf(const Sketch3dModel& model, const QStringList& items);

// The points a curve is drawn by (a line's ends, an arc's centre and ends,
// a spline's points), which a drag of the curve pulls; none for a
// reference curve.
QStringList dragPoints(const Sketch3dModel& model, const QString& curve);
// sketch3d.drag of points to targets (preview: nothing changes).
QJsonObject dragCommand(const QString& sketch, const QStringList& points, const std::vector<Xyz>& targets,
                        bool preview);
// sketch3d.drag of a fit-point spline's end tangent handle: `end` 0 its
// start, 1 its end; `tip` where the handle's tip is pulled to.
QJsonObject handleDragCommand(const QString& sketch, const QString& curve, int end, const Xyz& endPoint,
                              const Xyz& tip, bool preview);

// A tangent handle: its spline, its end (0, 1), the end point and the tip
// (the end plus a third of the tangent C'(t), the parameter running from 0
// to 1).
struct TangentHandle {
  QString curve;
  QString point;
  int end = 0;
  Xyz at{};
  Xyz tip{};
};
// The handles of the open fit-point splines at their free or tangent ends
// (the query's `end_tangents`).
std::vector<TangentHandle> tangentHandles(const Sketch3dModel& model);
// The tangent a handle's tip stands for, and back.
Xyz handleTip(const Xyz& at, const Xyz& tangent);
Xyz tangentOfTip(const Xyz& at, const Xyz& tip);

// Sample points of a curve from its points and geometry (the query's, or a
// drag preview's): a line, an arc, a circle, a spline by its control net;
// empty for a curve it cannot draw. `points` gives each point id's place.
template <typename PointAt>
std::vector<Xyz> curveSamples(const Sketch3dCurve& curve, const QJsonObject& geometry, PointAt pointAt);
// A clamped B-spline evaluated at `count` + 1 parameters over its range.
std::vector<Xyz> bsplinePoints(int degree, const std::vector<double>& knots, const std::vector<double>& weights,
                               const std::vector<Xyz>& control, int count);

// A relation's or dimension's line for agents and the log (English, with
// the ids): "k3 Length c3: 40.00 mm", "k5 Delta Z p2 to p4: 25.00 mm;
// driven", "k1 Coincident p2, p5", with In plane and redundant.
QString itemText(const Sketch3dModel& model, const Sketch3dItem& item, const QString& unit, double millimetres);
// The same as the palette's list shows it, in words (mitcad#1652,
// translated): "Length 1 on Line 1: 40.00 mm", "Offset 1, Delta Z from
// Point 2 to Point 4: 25.00 mm; driven", "Coincident 1 on Point 2, Point
// 5; redundant", "Attachment 1 on Point 1; linked to the origin". No ids.
QString itemWords(const Sketch3dModel& model, const Sketch3dItem& item, const QString& unit, double millimetres,
                  const SourceNames& names = {});
// A dimension's field text: its expression, or its measurement when
// driven; and its parameter's name.
QString itemValueText(const Sketch3dItem& item, double millimetres);
QString itemParameter(const Sketch3dItem& item);
// A linked reference's line for agents and the log (English, with the ids):
// id and source, body, occurrence, mode, status, users.
QString referenceText(const Sketch3dReference& reference);
// The same as the palette's list shows it, in words (mitcad#1652,
// translated): "Reference curve 1 (an edge of Extrude2); included in 3D;
// used by Point on Curve 1". No ids, topological names or coordinates.
QString referenceWords(const Sketch3dModel& model, const Sketch3dReference& reference, const SourceNames& names = {});
// The points a Detach of a record takes off it: an attachment's point, a
// reference curve's points on it.
QStringList detachPoints(const Sketch3dModel& model, const QString& record);

// ---------------------------------------------------------------------------

template <typename PointAt>
std::vector<Xyz> curveSamples(const Sketch3dCurve& curve, const QJsonObject& geometry, PointAt pointAt) {
  const QJsonObject& json = curve.json;
  const auto xyz = [](const QJsonValue& value) -> std::optional<Xyz> {
    const QJsonArray a = value.toArray();
    if (a.size() < 3) {
      return std::nullopt;
    }
    return Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()};
  };
  const auto at = [&pointAt, &json](const char* key) -> std::optional<Xyz> {
    return pointAt(json.value(QString::fromLatin1(key)).toString());
  };
  if (curve.type == QStringLiteral("line")) {
    const auto a = at("start");
    const auto b = at("end");
    if (a && b) {
      return {*a, *b};
    }
    return {};
  }
  if (curve.type == QStringLiteral("arc") || curve.type == QStringLiteral("circle")) {
    const auto c = at("center");
    const auto n = xyz(geometry.value(QStringLiteral("normal")));
    const auto normal = n ? unit(*n) : std::nullopt;
    if (!c || !normal) {
      return {};
    }
    ArcShape shape;
    shape.center = *c;
    shape.normal = *normal;
    Xyz start{};
    if (curve.type == QStringLiteral("arc")) {
      const auto s = at("start");
      if (!s) {
        return {};
      }
      start = *s;
      shape.sweep = geometry.value(QStringLiteral("sweep")).toDouble();
    } else {
      const double radius = geometry.value(QStringLiteral("radius")).toDouble();
      // A direction square to the normal.
      const Xyz other = std::abs((*normal)[0]) < 0.9 ? Xyz{1.0, 0.0, 0.0} : Xyz{0.0, 1.0, 0.0};
      const auto across = unit(cross(*normal, other));
      if (!across || radius <= 0.0) {
        return {};
      }
      start = add(*c, scaled(*across, radius));
      shape.sweep = 2.0 * 3.14159265358979323846;
    }
    shape.radius = distance(start, *c);
    return arcPoints(shape, start, 48);
  }
  if (curve.type == QStringLiteral("spline") || curve.type == QStringLiteral("fitted_spline")) {
    std::vector<double> knots;
    std::vector<double> weights;
    std::vector<Xyz> control;
    for (const QJsonValue& k : geometry.value(QStringLiteral("knots")).toArray()) {
      knots.push_back(k.toDouble());
    }
    for (const QJsonValue& w : geometry.value(QStringLiteral("weights")).toArray()) {
      weights.push_back(w.toDouble());
    }
    for (const QJsonValue& p : geometry.value(QStringLiteral("control_points")).toArray()) {
      if (const auto c = xyz(p)) {
        control.push_back(*c);
      }
    }
    return bsplinePoints(geometry.value(QStringLiteral("degree")).toInt(), knots, weights, control, 64);
  }
  return {};
}

} // namespace mitcad::sketch
