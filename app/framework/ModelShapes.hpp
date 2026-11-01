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

// A placement of the model's queries: 3 or 4 rows of 4 numbers.
gp_Trsf trsfOf(const QJsonArray& rows);
bool isIdentity(const QJsonArray& rows);

// A datum of the `datums` query: a plane as a square of `size`, an axis as a
// line of `size` through its origin, a point as a vertex.
TopoDS_Shape datumShape(const QJsonObject& datum, double size);
// A datum of the model's queries (in its component's coordinates) moved by
// a placement: where an occurrence of its component shows it (mitcad#1029).
QJsonObject placedDatum(const QJsonObject& datum, const gp_Trsf& placement);
// A datum's geometry for logs and UI tests, to 6 significant digits:
// "origin (x, y, z), normal (x, y, z)" of a plane, "origin (…), direction
// (…)" of an axis, "point (x, y, z)" of a point.
QString datumText(const QJsonObject& datum);

// The frame of a `sketch` query's answer (x, y and normal in model space);
// false when the sketch did not evaluate.
bool sketchFrame(const QJsonObject& sketch, geometry::Frame& frame);
gp_Ax3 toAx3(const geometry::Frame& frame);

// The curves and points of a `sketch` query's answer in model space.
std::vector<SketchEntityDisplay> sketchEntities(const QJsonObject& sketch);

} // namespace mitcad
