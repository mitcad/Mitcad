// SPDX-License-Identifier: MIT
#pragma once

// Display shapes built from the model's JSON answers: datums and sketch
// geometry, which the model gives as numbers rather than shape handles.

#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include <TopoDS_Shape.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "mitcad/geometry/profile.hpp"

namespace mitcad {

gp_Pnt pointOf(const QJsonValue& xyz);
// The point of an array of at least three numbers; none for anything else.
std::optional<gp_Pnt> pointFrom(const QJsonValue& xyz);
gp_Vec vectorOf(const QJsonValue& xyz);
// An Intersection Curve's polyline (the `feature_curves` query, mitcad#1405;
// a preview's `curve`, mitcad#1496) as a wire; null for fewer than two
// points.
TopoDS_Shape polylineShape(const QJsonArray& points);
// A curve feature's curve as the `feature_curves` query or a preview's
// `curve` gives it: its `polylines` as a compound of wires when it has
// them (an Isoparametric Curve's curves apart, mitcad#2522), else its
// `points` (polylineShape); null without two points.
TopoDS_Shape featureCurveShape(const QJsonObject& curve);

// A placement of the model's queries: 3 or 4 rows of 4 numbers.
gp_Trsf trsfOf(const QJsonArray& rows);
bool isIdentity(const QJsonArray& rows);

// A datum of the `datums` query: a plane as a square of `size`, an axis as a
// line of `size` through its origin, a point as a vertex.
TopoDS_Shape datumShape(const QJsonObject& datum, double size);
// Where the view draws a construction plane or axis (mitcad#1827): the
// `drawn` of the `datums` and `datum` queries, fitted to what it is made
// from, at its resize handles' size, or the default size.
struct DrawnDatum {
  bool plane = true;
  gp_Pnt centre;               // the rectangle's centre, the line's middle
  gp_Vec xAxis{1.0, 0.0, 0.0}; // the rectangle's width, the line's way (unit)
  gp_Vec yAxis{0.0, 1.0, 0.0}; // the rectangle's height (unit)
  double width = 0.0;          // the rectangle's width, the line's length, mm
  double height = 0.0;         // the rectangle's height, mm
  QString source;              // display_size, fitted or default
};
// A datum's `drawn`; none when it has none (an origin datum, a point, a
// joint origin) or it is degenerate.
std::optional<DrawnDatum> drawnDatum(const QJsonObject& datum);
// A construction datum as the view draws it: its `drawn` rectangle or line,
// else as datumShape with `size`.
TopoDS_Shape drawnDatumShape(const QJsonObject& datum, double size);
// A coordinate system's triad (mitcad#1775): of a plane datum with its
// frame, the line of `length` from its origin along X (`axis` 0), Y (1) or
// Z (2, the normal); all three as one compound for -1. Null for a datum
// that is not a plane.
TopoDS_Shape frameTriadShape(const QJsonObject& datum, double length, int axis = -1);
// A fixed plane of a definition (`{"origin", "normal"[, "x_axis"]}`) as the
// `datum` query gives a plane (`type`, `origin`, `normal`, `x_axis`,
// `y_axis`), its frame as the model makes it (mitcad#1137): the origin as
// given, `x_axis` made square to the normal, without one the model's X
// projected (Y when the plane faces X). Empty for a degenerate one.
QJsonObject fixedPlaneDatum(const QJsonObject& plane);
// A datum of the model's queries (in its component's coordinates) moved by
// a placement: where an occurrence of its component shows it (mitcad#1029),
// its `drawn` extent with it (mitcad#1827).
QJsonObject placedDatum(const QJsonObject& datum, const gp_Trsf& placement);
// A datum's geometry for logs and UI tests, to 6 significant digits:
// "origin (x, y, z), normal (x, y, z)" of a plane, "origin (…), direction
// (…)" of an axis, "point (x, y, z)" of a point.
QString datumText(const QJsonObject& datum);

// The frame of a `sketch` query's answer (x, y and normal in model space);
// false when the sketch did not evaluate.
bool sketchFrame(const QJsonObject& sketch, geometry::Frame& frame);
gp_Ax3 toAx3(const geometry::Frame& frame);

// The curves and points of a `sketch` query's answer in model space. A
// sketch on a cylindrical or conical face (its `surface`, mitcad#1768) has
// them on the face: its curves from `onFace`, the `sketch_surface` query's
// answer with `curves` (each curve's polyline on the face), the rest laid
// onto the face by the same map.
std::vector<SketchEntityDisplay> sketchEntities(const QJsonObject& sketch, const QJsonObject& onFace = QJsonObject());

// The points and curves of a `sketch3d` query's answer (mitcad#1524) in
// its component's coordinates: points, lines, arcs, circles and splines
// where the last solve left them (a fit-point spline through its fit
// points, by its hidden control points), a converted sketch's plane-bound
// ellipses, and reference curves as found from their sources, drawn as
// reference geometry. Each entity's id is its point or curve id, as the
// sketch3d commands take it. Empty when the sketch has no frame (its
// support was not found).
std::vector<SketchEntityDisplay> spatialSketchEntities(const QJsonObject& sketch);

} // namespace mitcad
