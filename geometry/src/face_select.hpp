// SPDX-License-Identifier: MIT
#pragma once

// Selections shared by the operations that work on faces and edges of a
// body (fillets, chamfers, shells, drafts, face operations, splits): name
// resolution, tangent chains and planes. Internal to the geometry library.

#include <optional>
#include <string>
#include <vector>

#include <Bnd_Box.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/tool.hpp"

namespace mitcad::geometry::detail {

// The faces the references resolve to (every piece of a split face), each
// once, in reference order. Throws for an empty list or an unknown name.
std::vector<int> resolve_faces(const Shape& body, const std::vector<std::string>& references,
                               const char* what = "faces");

// The edges the edge references resolve to, then every edge of the faces,
// each once. Throws for an empty selection or an unknown name.
std::vector<int> resolve_edges(const Shape& body, const std::vector<std::string>& edges,
                               const std::vector<std::string>& faces = {});

// The normal of the face pointing out of the material (the face's
// orientation applied) where the edge on it is at relative parameter t.
gp_Dir normal_on_edge(const TopoDS_Face& face, const TopoDS_Edge& edge, double t = 0.5);

// The angle between the outward normals of two faces along their common
// edge at relative parameter t.
double normal_angle(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b,
                    double t = 0.5);

// True when two different faces meet tangentially along the whole edge.
bool is_smooth(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b);

// The faces plus every face connected to them through smooth edges, as a
// tangent chain selection grows.
std::vector<int> tangent_closure(const Shape& body, std::vector<int> faces);

// The faces of the body at an edge (one for a seam or a free edge).
std::vector<int> faces_at_edge(const Shape& body, int edge);

// The plane of a planar face with its normal pointing out of the material;
// null for other surfaces.
std::optional<gp_Pln> face_plane(const TopoDS_Face& face);

// A rectangular face on the plane that covers the box several times over,
// so that it cuts through everything inside the box.
TopoDS_Face plane_face(const gp_Pln& plane, const Bnd_Box& box);

// The box of a shape, not enlarged by tolerances.
Bnd_Box box_of(const TopoDS_Shape& shape);

// The smallest name of a face, as edge names use it; empty when unnamed.
std::string primary_name(const Shape& body, int face);

// The plane of a Plane tool, or of a planar Face tool with its normal out of
// the face's material. Throws for other tools; `what` names the role.
gp_Pln tool_plane(const Tool& tool, const char* what);

// The faces a tool cuts with, around a box: a plane as a face covering the
// box, the faces of a Face tool (extended along their surfaces when asked),
// all faces of a Body tool, or the curves of a Curves tool swept both ways
// through the box. `keys` holds the role key of each face for the names of
// what it cuts (the tool face's name, a sketch segment, or empty).
struct Cutter {
  TopoDS_Shape shape;
  std::vector<TopoDS_Shape> faces;
  std::vector<std::string> keys;
  std::optional<gp_Dir> normal; // planar tools
};

Cutter make_cutter(const Tool& tool, const Bnd_Box& around, bool extend);

} // namespace mitcad::geometry::detail
