// SPDX-License-Identifier: MIT
#pragma once

// Fillets and chamfers of whole circular edges built as rings.
//
// OCCT's fillets and chamfers stop where a rounding or bevel would run
// over the edge of a neighbouring face (a chamfer of a hole wider than the
// wall beside it, a rounding of a rim that reaches a pocket in its face).
// Around a circle between two faces of revolution about its axis whose
// sections are straight lines (planes square to the axis, cylinders and
// cones on it), the dressup is the same section all the way round: the
// section, revolved about the axis, is cut from the body at a convex edge
// and joined to it at a concave one, running over whatever it meets.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The dressup of one edge of the body: a rounding of `radius`, or a bevel
// `distance` along `face` (the edge's first face when negative) and
// `distance2` along the other face, or at `angle` from `face` (radians;
// zero for two distances).
struct RingEdge {
  int edge = -1;
  bool fillet = true;
  double radius = 0.0;
  int face = -1;
  double distance = 0.0;
  double distance2 = 0.0;
  double angle = 0.0;
};

// The body with every edge's ring cut away or joined; the rounding or bevel
// of each is named <feature>:<role>(<edge>), the body's faces keep their
// names. Null when an edge is not such a circle or the result is not one
// valid solid.
ShapePtr ring_dressup(const std::string& feature, const char* role, const Shape& body,
                      const std::vector<RingEdge>& edges);

} // namespace mitcad::geometry::detail
