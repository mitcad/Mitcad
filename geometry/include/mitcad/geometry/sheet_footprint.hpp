// SPDX-License-Identifier: MIT
#pragma once

// Sheet footprints (mitcad#1628): where an ordinary cut across a bend left
// the bend's cylindrical surfaces, in the bend chart's coordinates, so the
// model can remove the same material from the chart as Sheet Cut does.

#include <vector>

#include "mitcad/geometry/sheet.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The boundaries of the faces of `shape` on the cylinder of `radius` about
// the chart's axis (within `tolerance`), in the chart's coordinates: u the
// angle from radial_start towards angular_direction times neutral_radius,
// v along the axis from the centre. One list per face; each edge is the
// face's own parameter curve mapped affinely (never sampled), in no
// particular order or direction.
std::vector<std::vector<BoundedCurve2>> sheet_cylinder_footprint(const Shape& shape,
                                                                 const CylinderEmbedding& chart,
                                                                 double radius, double tolerance);

} // namespace mitcad::geometry
