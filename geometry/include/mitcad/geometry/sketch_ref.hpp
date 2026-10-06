// SPDX-License-Identifier: MIT
#pragma once

// Model geometry that sketches project: the curves of named edges, faces
// and vertices. Lengths are millimetres, angles radians. (The plane of a
// face a sketch lies on is reference.hpp's face_plane.)

#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class ModelCurveKind { Point, Line, Conic, BSpline };

// A curve in model space. A conic is center + major cos(t) x_axis + minor
// sin(t) (normal × x_axis) for t from first to last (a circle when major ==
// minor); a B-spline has its full knot vector (poles + degree + 1 knots).
struct ModelCurve {
  ModelCurveKind kind = ModelCurveKind::Line;
  gp_Pnt start; // Point, Line
  gp_Pnt end;   // Line
  gp_Pnt center;
  gp_Dir normal;
  gp_Dir x_axis;
  double major = 0.0;
  double minor = 0.0;
  double first = 0.0;
  double last = 0.0;
  bool closed = false;
  int degree = 0;
  std::vector<gp_Pnt> poles;
  std::vector<double> weights; // empty: non-rational
  std::vector<double> knots;
};

// The curves of what a name resolves to: an edge name ("E{...}") its edges,
// a vertex name ("V{...}") its points, a face name the boundary edges of its
// faces, each edge once. Empty when nothing matches. Curves other than
// lines, circles, ellipses and B-splines become B-splines.
std::vector<ModelCurve> curves_of(const Shape& shape, const std::string& name);

// FreeCAD import: the face ('F'), edge ('E') or vertex ('V') `index`
// (0-based, TopExp order, which FreeCAD's Face<n>, Edge<n> and Vertex<n>
// count from 1) with its name (empty when its faces have none) and its
// curves as curves_of gives them; found is false when the shape has fewer.
struct IndexedElement {
  bool found = false;
  std::string name;
  std::vector<ModelCurve> curves;
};

IndexedElement indexed_element(const Shape& shape, char kind, int index);

} // namespace mitcad::geometry
