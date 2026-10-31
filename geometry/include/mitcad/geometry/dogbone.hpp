// SPDX-License-Identifier: MIT
#pragma once

// Dogbones (mitcad#1011): relief cylinders at a body's inside corners, so
// that a router bit of their diameter clears the sharp corner. Lengths are
// millimetres.

#include <array>
#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The edges taken: those `edges` names (each must be a straight inside
// corner, a concave edge between two faces, or it is an error that says
// so); of the faces `faces` names, their edges that are straight inside
// corners parallel to `axis`, and of such a face that is planar and across
// `axis` (a pocket's floor), every such edge of the body with an end on
// it; with neither, every such edge of the body. Picks that give no edge
// are an error.
//
// For each edge a cylinder of radius diameter / 2 runs along the whole
// edge, parallel to it. Diagonal: its centre on the corner's bisector,
// diameter / 2 - extra from the edge, so that its circle passes `extra`
// beyond the sharp corner. Along a side (a T-bone): its centre diameter / 2
// off one face of the corner, into the corner's opening, and `extra` beyond
// the corner along that face; the face is the picked face the edge came
// from, otherwise the longer of the corner's two faces (the one reaching
// farther from the edge).
//
// Faces: <feature>:dogbone(<edge>) along the cylinder, dogbone_start(<edge>)
// at the edge's first end and dogbone_end(<edge>) at its last, <edge> the
// edge's name in the body (its index among the body's edges when it has
// none). The cylinders are one compound; they may overlap. The body is not
// modified.
struct DogboneSpec {
  std::string feature;
  std::vector<std::string> edges;
  std::vector<std::string> faces;
  // The router bit's axis, a unit vector.
  std::array<double, 3> axis{0.0, 0.0, 1.0};
  double diameter = 0.0;
  double extra = 0.0;
  bool along_side = false;
};

ShapePtr dogbone_tool(const Shape& body, const DogboneSpec& spec);

} // namespace mitcad::geometry
