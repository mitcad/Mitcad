// SPDX-License-Identifier: MIT
#pragma once

// Feature inputs drawn where they were (mitcad#957): the edges a Fillet or
// a Chamfer picked, as polylines the model keeps with its result, so that
// the application draws them when the feature is selected on the timeline
// without asking the kernel again.

#include <string>
#include <vector>

#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The edges the names in `edges` resolve to (every edge of a name without
// "#k") and the edges bounding the faces the names in `faces` resolve to,
// each edge once, in that order: a polyline of points along each edge's
// curve, its ends included, in the shape's coordinates (its location
// applied), no farther than `deflection` (mm) from the curve and turning by
// at most `angle` (radians) between neighbouring pieces. Degenerate edges
// are left out. A name that resolves to nothing is an error. The shape is
// left as it is.
std::vector<std::vector<gp_Pnt>> edge_polylines(const Shape& shape, const std::vector<std::string>& edges,
                                                const std::vector<std::string>& faces, double deflection,
                                                double angle);

} // namespace mitcad::geometry
