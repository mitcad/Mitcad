// SPDX-License-Identifier: MIT
#pragma once

// Measurement between and of sub-shapes (vertices, edges, faces, bodies).

#include <optional>

#include <Message_ProgressRange.hxx>

#include "mitcad/analysis/common.hpp"

namespace mitcad::analysis {

// Minimum distance between two shapes and the closest points.
struct Distance {
  double value = 0.0;
  Vec3 on_a;
  Vec3 on_b;
  // One shape lies (partly) inside a solid of the other; the distance is 0.
  bool inside = false;
  // Found as the gap of two planar faces in parallel planes, from a point
  // of one over the other (mitcad#716), without the full extrema.
  bool parallel_faces = false;
};

// Works for any pair of vertices, edges, faces, shells and solids. Two
// planar faces in parallel planes are measured from a few points of each
// (a point inside it, its vertices) whose foot on the other's plane lies in
// the other face: the planes' gap, the same minimum the full extrema give,
// without pairing every edge and vertex of the two faces; when no such
// point is found, the full extrema as for any other pair. `range`'s user
// break stops the full extrema inside (a cancelled model job, mitcad#2577:
// one distance between detailed bodies in an assembly path's probe), and
// the measurement then throws Error.
Distance min_distance(const TopoDS_Shape& a, const TopoDS_Shape& b,
                      const Message_ProgressRange& range = Message_ProgressRange());

// Angle between two planar faces (between their outward normals, in
// [0, pi]), two linear edges (in [0, pi/2]; edges with a common vertex are
// measured from it, in [0, pi]) or a linear edge and a planar face (in
// [0, pi/2]). Throws Error for other shapes.
double angle(const TopoDS_Shape& a, const TopoDS_Shape& b);

// Total length of the edges of a shape (an edge, a wire, ...), each shared
// edge counted once.
double length(const TopoDS_Shape& shape);

// Total area of the faces of a shape, each shared face counted once.
double area(const TopoDS_Shape& shape);

// A circular edge or arc, or a cylindrical or spherical face.
struct Circle {
  Vec3 center; // circle centre; for a cylinder the axis point nearest the face's middle
  Vec3 axis;   // unit normal of the circle plane or cylinder axis; zero for a sphere
  double radius = 0.0;
  double sweep = 0.0; // arc angle, 2 pi for a full circle; 0 for faces
};

// The circle of an edge or a face, if it is circular.
std::optional<Circle> circle(const TopoDS_Shape& edge_or_face);

// Coordinates of a vertex.
Vec3 point(const TopoDS_Shape& vertex);

} // namespace mitcad::analysis
