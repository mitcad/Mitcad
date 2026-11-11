// SPDX-License-Identifier: MIT
#pragma once

// Model geometry that sketches project: the curves of named edges, faces
// and vertices. Lengths are millimetres, angles radians. (The plane of a
// face a sketch lies on is reference.hpp's face_plane.)

#include <string>
#include <vector>

#include <TopoDS_Edge.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class ModelCurveKind { Point, Line, Conic, BSpline, Helix };

// A curve in model space. A conic is center + major cos(t) x_axis + minor
// sin(t) (normal × x_axis) for t from first to last (a circle when major ==
// minor); a B-spline has its full knot vector (poles + degree + 1 knots); a
// helix (a path, mitcad#385) is center + major (cos(t) x_axis + sin(t)
// (normal × x_axis)) + minor t / 2π normal for t from first to last: its
// radius is `major`, its rise per turn `minor` (negative: left-handed).
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

// A planar face's non-degenerate boundary edges, with canonical edge names
// and curves in the body's model space (no occurrence transform). Includes
// all pieces of a split face, omitting their shared internal edges, sorted
// by name. Selecting just one piece retains its edge against another piece.
// Empty when the face does not resolve; throws for a non-planar face or a
// boundary edge without a topological name, as on an open surface body.
struct NamedBoundaryCurve {
  std::string name;
  ModelCurve curve;
};
std::vector<NamedBoundaryCurve> face_boundary(const Shape& shape, const std::string& face);

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

// Every named edge of a shape with its curve, by index (mitcad#1656): what
// curves_of gives for each edge's own name, without resolving the names
// back (a scan of every edge per name on a body of few faces). Edges
// without a name, and those whose curve cannot be read (curves_of's
// failure for that name), are left out.
std::vector<NamedBoundaryCurve> named_edge_curves(const Shape& shape);

// Section curves of sketches (mitcad#389): where `plane` cuts the faces a
// name resolves to (every face of the shape for an empty name), each piece
// as curves_of gives an edge's curve, with the names of the faces it lies
// on (the first name of each). Pieces of B-spline curves that meet end to
// end where no other piece meets are joined into one B-spline, exactly
// (continuous in position where they meet), as are the pieces of a closed
// loop of them; lines and conics stay pieces of their own.
struct SectionCurve {
  ModelCurve curve;
  std::vector<std::string> faces;
};
std::vector<SectionCurve> section_curves(const Shape& shape, const std::string& name, const gp_Pnt& origin,
                                         const gp_Dir& normal);

// Crossing points of a sketch's Intersect (mitcad#736): where `plane`
// crosses the edges an edge name resolves to (every edge of a name without
// #k), each point once: where an edge passes through the plane or touches
// it, and the ends of an edge (or of a piece of it) lying in the plane.
// Empty when the plane misses them.
std::vector<gp_Pnt> section_points(const Shape& shape, const std::string& name, const gp_Pnt& origin,
                                   const gp_Dir& normal);

// One edge's curve as curves_of gives it (the outlines of faces,
// face_outline.hpp, mitcad#1265).
ModelCurve edge_curve(const TopoDS_Edge& edge);

} // namespace mitcad::geometry
