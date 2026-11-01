// SPDX-License-Identifier: MIT
#pragma once

// Roundings that take a face away, round a loop of edges, mitred where
// they meet (mitcad#1118).
//
// Along a straight edge between two planes, a rounding wider than one of
// the faces (a wall's top bevelled all across, then its outer edge rounded)
// takes that face away: its ball rolls on the face's far edge, and the
// rounding is a cylinder from its contact on the other face to that edge,
// where it meets the face beyond. Where such roundings run round a loop of
// edges (the outer top edges of a tray whose walls are bevelled all across),
// OCCT's corner, two roundings meeting where two more sharp edges meet,
// fails. Built here instead: each rounding's section (the material between
// the ball and the two faces, the face it takes away whole), swept along its
// edge and cut at each end by the plane halfway between it and the rounding
// it meets there, is cut from the body. The two roundings at a corner are
// mirror images of each other in that plane, so their cylinders meet on it:
// a mitre, as two equal roundings meet where the corner between them is
// sharp.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// One edge's rounding: the body's edge and its radius.
struct MitredEdge {
  int edge = -1;
  double radius = 0.0;
};

// The body with the roundings of the edges cut away, each named
// <feature>:fillet(<edge>), the body's faces keeping their names. Null
// unless every edge is a straight convex edge between two planes whose
// rounding takes one of them away, its ball rolling on that face's far
// edge (a line along the edge, between it and a plane), every end of every
// edge is the end of exactly one other edge whose rounding is its mirror
// image in the plane halfway between them, and the result is one valid
// solid.
ShapePtr mitred_fillet(const std::string& feature, const Shape& body, const std::vector<MitredEdge>& edges);

} // namespace mitcad::geometry::detail
