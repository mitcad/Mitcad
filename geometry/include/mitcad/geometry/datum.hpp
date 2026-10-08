// SPDX-License-Identifier: MIT
#pragma once

// Construction geometry: analytic descriptions of named faces, edges and
// vertices, points on paths and faces, and display shapes of datums
// (planes, axes, points). Mirrors core/model/src/datum.rs.

#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The surface of a face. Normals are outward (the face orientation
// applied).
struct SurfaceDescription {
  // plane, cylinder, cone, sphere, torus, or another surface type as
  // face_infos names it (bspline, revolution, ...).
  std::string type;
  // Plane: the point nearest to the model origin. Cylinder and cone: the
  // axis point nearest to the face's centre. Sphere and torus: the centre.
  gp_Pnt origin;
  // Plane: the normal. Cylinder, cone and torus: the axis, pointing along
  // its largest component, whatever the kernel's surface says (the model's
  // canonical_axis). A plane's frame is the model's (DatumPlane::on_plane).
  gp_Dir axis;
  // Cylinder, sphere; cone: at the origin, growing along the axis by
  // tan(half_angle); torus: the major radius.
  double radius = 0.0;
  double minor_radius = 0.0; // torus
  double half_angle = 0.0;   // cone
};

// The surface of the faces a name resolves to: one face, or the pieces of
// a split face that lie on one surface. Throws otherwise.
SurfaceDescription face_geometry(const Shape& shape, const std::string& face);

// The curve of one edge, in the direction of its parameter.
struct CurveDescription {
  std::string type; // line, circle, or another curve type
  gp_Pnt start;
  gp_Pnt end;
  // Circles and arcs, counter-clockwise about the normal.
  gp_Pnt center;
  gp_Dir normal;
  double radius = 0.0;
};

CurveDescription edge_geometry(const Shape& shape, const std::string& edge);

// The surfaces of every face and the curves of every edge in one pass, in
// the shape's face and edge order (Shape::face_names, Shape::edge_name name
// them): the .f3d import looks for the faces and edges through a point of
// large bodies (mitcad#87), where finding each by its name takes as long
// as the whole list. A face whose surface is not a plane, cylinder, cone,
// sphere or torus has only its type (no planar spline test). A degenerate
// edge has the type "degenerate".
std::vector<SurfaceDescription> face_geometries(const Shape& shape);
std::vector<CurveDescription> edge_geometries(const Shape& shape);

gp_Pnt vertex_point(const Shape& shape, const std::string& vertex);

enum class PathAt {
  Fraction, // of the length, 0..1
  Length,   // mm from the start
  Near,     // the path point nearest to a point
};

struct PathPoint {
  gp_Pnt point;
  gp_Dir tangent;
};

// A point of the chain of edges, joined in the order given and running
// from the free end of the first edge. Fractions and lengths beyond the
// ends continue straight along the end tangents.
PathPoint path_point(const Shape& shape, const std::vector<std::string>& edges, PathAt at,
                     double value, const gp_Pnt& near);

struct SurfacePoint {
  gp_Pnt point;
  gp_Dir normal;
};

// The point of the faces a name resolves to nearest to `near`, with the
// outward normal there.
SurfacePoint face_point_normal(const Shape& shape, const std::string& face, const gp_Pnt& near);

// Display shapes: a square face of side `size` centred on the origin, an
// edge of length `size` centred on the origin, a vertex.
ShapePtr plane_shape(const gp_Pnt& origin, const gp_Dir& normal, const gp_Dir& x_axis,
                     double size);
ShapePtr axis_shape(const gp_Pnt& origin, const gp_Dir& direction, double size);
ShapePtr point_shape(const gp_Pnt& point);

} // namespace mitcad::geometry
