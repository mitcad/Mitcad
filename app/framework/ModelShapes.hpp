// SPDX-License-Identifier: MIT
#pragma once

// Display shapes built from the model's JSON answers: datums and sketch
// geometry, which the model gives as numbers rather than shape handles.

#include <vector>

#include <QJsonArray>
#include <QJsonObject>

#include <TopoDS_Shape.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "mitcad/geometry/profile.hpp"

namespace mitcad {

gp_Pnt pointOf(const QJsonValue& xyz);
gp_Vec vectorOf(const QJsonValue& xyz);

// A datum of the `datums` query: a plane as a square of `size`, an axis as a
// line of `size` through its origin, a point as a vertex.
TopoDS_Shape datumShape(const QJsonObject& datum, double size);

// The frame of a `sketch` query's answer (x, y and normal in model space);
// false when the sketch did not evaluate.
bool sketchFrame(const QJsonObject& sketch, geometry::Frame& frame);
gp_Ax3 toAx3(const geometry::Frame& frame);

// The curves and points of a `sketch` query's answer in model space.
std::vector<SketchEntityDisplay> sketchEntities(const QJsonObject& sketch);

} // namespace mitcad
