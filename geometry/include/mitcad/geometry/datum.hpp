// SPDX-License-Identifier: MIT
#pragma once

// Construction geometry: analytic descriptions of named faces, edges and
// vertices, points on paths and faces, and display shapes of datums
// (planes, axes, points). Mirrors core/model-base/src/datum.rs.

#include <optional>
#include <string>
#include <vector>

#include <Geom_Surface.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
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
// a split face that lie on one surface. Throws otherwise. A face stored as
// another surface within 1e-6 mm of a plane, cylinder, cone, sphere or
// torus is that one (planar splines: mitcad#1537; analytic_surface:
// mitcad#1855).
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

// A straight edge that is stored as a spline or a Bezier curve (the walls
// of sheet bodies, mitcad#1537; imported lines) is a line here, as a planar
// spline face is a plane to face_geometry; one within 1e-6 mm of a circle
// is that circle (is_circular, mitcad#1855).
CurveDescription edge_geometry(const Shape& shape, const std::string& edge);

// Whether an edge runs straight: a line, or a spline or Bezier curve whose
// poles lie within 1e-6 mm of the line through its ends (mitcad#1537). What
// the application's picks call a line, as edge_geometry does.
bool is_straight(const TopoDS_Edge& edge);

// The plane of a planar face, its normal outward (the face's orientation
// applied): a plane, or another surface within 1e-6 mm of one, as
// face_geometry reads it (mitcad#1537: the walls of sheet bodies are planar
// splines). Null for other faces and a mesh body's triangles.
std::optional<gp_Pln> face_plane(const TopoDS_Face& face);

// The plane, cylinder, cone, sphere or torus a face lies on, in the face's
// place (its location applied): its own surface, or the one its stored
// surface lies within 1e-6 mm of all over the face where it is stored as
// another type (mitcad#1855: an extrusion of a circle, a revolution, a
// B-spline such as the wall of a hole in a sheet body). Null for other
// faces and a mesh body's triangles. face_geometry reads faces so.
occ::handle<Geom_Surface> analytic_surface(const TopoDS_Face& face);

// What the application's picks call a face: "plane", "cylinder", "cone",
// "sphere" or "torus" for a face on one of them or within 1e-6 mm of one
// (planar splines as face_plane, the rest as analytic_surface reads them);
// null for other faces and a mesh body's triangles. The answers for faces
// stored as other surfaces are kept by the face's shape.
const char* analytic_kind(const TopoDS_Face& face);

// Whether an edge runs on a circle: a circle or an arc, or a curve stored
// as another type within 1e-6 mm of one (a spline, mitcad#1855). What the
// application's picks call a circle, as edge_geometry reads it.
bool is_circular(const TopoDS_Edge& edge);

// The surfaces of every face and the curves of every edge in one pass, in
// the shape's face and edge order (Shape::face_names, Shape::edge_name name
// them): the .f3d import looks for the faces and edges through a point of
// large bodies (mitcad#87), where finding each by its name takes as long
// as the whole list. A face whose surface is not a plane, cylinder, cone,
// sphere or torus has only its type (no planar spline test, no
// analytic_surface), and an edge that is not a line or circle only its
// type. A degenerate
// edge has the type "degenerate".
std::vector<SurfaceDescription> face_geometries(const Shape& shape);
std::vector<CurveDescription> edge_geometries(const Shape& shape);

// The surfaces of the faces with these indices (Shape::face) and the curves
// of the edges with these indices (Shape::edge), each read as face_geometry
// and edge_geometry read one face or edge: a face stored as a spline, a
// revolution or an extrusion within 1e-6 mm of a plane, cylinder, cone,
// sphere or torus is that one, an edge stored as a spline within 1e-6 mm of
// a line or circle that one; the rest have only their type, as in
// face_geometries and edge_geometries. The `faces` and `edges` queries
// read the faces and edges stored as other types so (mitcad#1863); a face
// found to be on none of them is known by its shape afterwards
// (analytic_kind), so asking again is quick. An index out of range throws.
std::vector<SurfaceDescription> recognised_face_geometries(const Shape& shape, const std::vector<int>& faces);
std::vector<CurveDescription> recognised_edge_geometries(const Shape& shape, const std::vector<int>& edges);

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

// A point on a face, inside its edges, with the outward normal there (the
// face's orientation and location applied): the middle of the face's
// parameter range where that lies on the face, else the point of a 16 x 16
// grid over the range nearest that middle that lies inside the face and has
// a normal (not a cone's tip). Where the drag handle of a curved face's
// Offset starts (Press Pull, Offset Face; mitcad#2478): the middle of a
// cone's or a loft's bounding box lies on its axis, off the face. Null for
// a mesh body's triangles and a face with no such point.
std::optional<SurfacePoint> face_inner_point(const TopoDS_Face& face);

// Display shapes: a square face of side `size` centred on the origin, an
// edge of length `size` centred on the origin, a vertex.
ShapePtr plane_shape(const gp_Pnt& origin, const gp_Dir& normal, const gp_Dir& x_axis,
                     double size);
ShapePtr axis_shape(const gp_Pnt& origin, const gp_Dir& direction, double size);
ShapePtr point_shape(const gp_Pnt& point);

} // namespace mitcad::geometry
