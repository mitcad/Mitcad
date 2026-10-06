// SPDX-License-Identifier: MIT
#pragma once

// Setback corners of fillets and blend corners of chamfers (P6): the corner
// face OCCT makes where three or more dressed edges meet is replaced, with
// the parts of the roundings or bevels near the vertex, by one patch
// tangent to what is left of them and to the faces. Internal to the
// geometry library.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// A corner to set back: the vertex (its name in the body) and how far from
// it the roundings or bevels stop.
struct Setback {
  std::string vertex;
  double distance = 0.0;
};

// The corner faces of `dressed` (a fillet or chamfer of `body`, its faces
// named <feature>:corner(<vertex>) and <feature>:<role>(<edge>)) at the
// corners, and the parts of the roundings or bevels within a corner's
// distance of its vertex, replaced by an N-sided patch (OCCT's plate
// surface, BRepOffsetAPI_MakeFilling) tangent to the faces around the hole.
// The patch is named <feature>:corner(<vertex>); the rest keep their names.
ShapePtr setback_corners(const std::string& feature, const char* role, const Shape& body, const Shape& dressed,
                         const std::vector<Setback>& corners);

} // namespace mitcad::geometry::detail
