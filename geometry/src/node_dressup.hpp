// SPDX-License-Identifier: MIT
#pragma once

// Roundings of the valleys where tubes meet, run on through their nodes
// (mitcad#1205).
//
// Where tubes of one radius meet with their axes crossing, the edge between
// two of them is a valley (an arc of the ellipse halfway between their
// axes), and its rounding is the ball rolling between the two cylinders.
// Where three tubes meet at a node, a small facet (the cap of a rod along
// the node's axis, narrower than the rounding) holds the three valleys'
// ends apart: OCCT's corners at the facet's vertices find no edge between
// the faces round them and fail. Built here instead, as the files' bodies
// have it: each valley's rounding, run on past its end through the node,
// is the material between the ball and the two cylinders; the roundings
// join the body together, and where they overlap at the node each keeps
// what lies outside the others (they meet along their crossings, the
// facet buried). Each rounding is OCCT's own, of the valley edge of the
// two tubes alone.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// One edge's rounding: the body's edge and its radius.
struct NodeEdge {
  int edge = -1;
  double radius = 0.0;
};

// The body with the valleys' roundings joined to it, each named
// <feature>:fillet(<edge>) (a valley the booleans split into edges between
// the same two faces is one rounding, with all their names), the body's
// faces keeping their names. Null unless every edge is a concave edge
// between two convex cylinders of one radius whose axes cross, each end of
// its valley either on a plane (the end's third face) or at a vertex of a
// face of the body all of whose vertices end edges of the set, between
// convex cylinders (a node's facet, at least one of them), and the result
// is one valid solid with every rounding in it and nothing else of the
// roundings' pieces left.
ShapePtr node_fillet(const std::string& feature, const Shape& body, const std::vector<NodeEdge>& edges);

} // namespace mitcad::geometry::detail
