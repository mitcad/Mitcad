// SPDX-License-Identifier: MIT
#pragma once

// Selections shared by the operations that work on faces and edges of a
// body (fillets, chamfers, shells, drafts, face operations, splits): name
// resolution, tangent chains and planes. Internal to the geometry library.

#include <optional>
#include <string>
#include <vector>

#include <Bnd_Box.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
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

// The faces of every edge of the body, found once: for operations that ask
// for the faces of many edges (faces_at_edge maps the whole body each time,
// seconds for a thousand edges of a body of thousands of faces).
class EdgeFaces {
public:
  explicit EdgeFaces(const Shape& body);
  // As faces_at_edge(body, edge).
  std::vector<int> at(int edge) const;

private:
  const Shape& m_body;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      m_ancestors;
};

// Mitcad's own tangent chains of edge selections (mitcad#224). OCCT's
// fillets and chamfers continue a selected edge into the next only where
// every other edge at the vertex between them lies between tangent faces
// (ChFi3d_Builder::FaceTangency): at a vertex where a third sharp edge
// meets (a ring that two chamfers split into two arcs) they stop, although
// the next edge continues the first smoothly. This adds to `owner` (the
// selection of each edge of the body: its set, -1 for none) every edge
// that no set selects and that meets an edge of a set that `follows` (by
// set) end to end with the same direction (within
// kTangentChainAngle) and lies between two faces at an angle (not a smooth
// edge, which no dressup rounds) that is convex where the edge's is convex
// and concave where it is concave, then the same from those edges. Returns
// how many it added.
//
// The convexity: a rounding's ball lies inside the material's angle at a
// convex edge and outside it at a concave one, so a rounding cannot run on
// from one into the other; where such edges meet end to end, the rounding
// vanishes (a post's top circle under a block as wide as the post: the
// circle touches the block's long bottom edges where the underside narrows
// to nothing, the convex long edges running on from the concave circle's
// arcs there; the file's tangent chain from one arc takes the other arcs
// only, mitcad#439). OCCT's contour has one type of concavity too
// (ChFiDS_Spine::SetTypeOfConcavity).
//
// The angle: in the reference data of imported designs, the edges a
// file's tangent chain added met within 5.3e-4 rad; the next larger
// angles between edges of one selection (0.012 rad and up) were between
// edges the file had selected itself. 1e-2 rad lies between, and is also
// OCCT's own angular tolerance of its fillets.
//
// Through a turn (mitcad#1435): an edge also continues into a sharp edge
// of its convexity that meets it at an angle on a face they share, where
// their other faces meet at that vertex along a crease that fades out:
// the two faces are tangent at the crease's other end. The faces beside
// the shared one are then one smooth surface but for that crease, and the
// edges along them run on into each other. An opening's wall meets the
// top of a plate, a face X tangent to the top and the torus of a rim's
// rounding, also tangent to the top; X and the torus meet along a crease
// from the top (tangent there) down to the wall, so the wall's edges with
// X and with the torus meet there at 0.95 rad. The file's tangent chain
// from the wall's edges with the top and X took the torus's edges on,
// round the whole wall. Where the other faces meet along a crease that
// stays one along its length (0.73 to 0.81 rad in another design), the
// chain stops there as before.
constexpr double kTangentChainAngle = 1.0e-2;
int follow_tangent_chains(const Shape& body, std::vector<int>& owner, const std::vector<char>& follows);

// The rule of follow_tangent_chains, with the body's vertices and edge faces
// mapped once: for chains asked edge by edge (the tangent chain query,
// tangent_chain.hpp, mitcad#290) as for the dressups' selections.
class EdgeContinuations {
public:
  explicit EdgeContinuations(const Shape& body);
  // Whether the edge lies between two faces at an angle (an edge a dressup
  // rounds); worked out once per edge.
  bool sharp(int edge) const;
  // The sharp edges that meet the edge end to end in the same direction
  // (within kTangentChainAngle), the edge itself left out, except those
  // convex where the edge is concave or concave where it is convex; and
  // those of the edge's convexity that meet it at an angle on a face they
  // share across a crease that fades out (follow_tangent_chains).
  std::vector<int> next(int edge) const;

private:
  // Whether the edges `e` and `o`, at the vertex, lie on one face and their
  // other faces meet there along an edge whose faces are tangent at its
  // other end.
  bool across_fading_crease(int e, int o, const TopoDS_Vertex& vertex) const;

  // 1 for a convex edge, 0 for a concave one, -2 for neither (a seam, a
  // free edge, one of mixed or unknown convexity); worked out once per
  // edge.
  int convexity(int edge) const;

  const Shape& m_body;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      m_edgesOf;
  EdgeFaces m_edgeFaces;
  mutable std::vector<signed char> m_sharp;     // -1: not worked out yet
  mutable std::vector<signed char> m_convexity; // -1: not worked out yet
};

// The faces plus every face connected to them through smooth edges
// (tangent_closure), with the body's edge faces mapped once by the caller:
// `ancestors` maps each edge of the body to its faces.
std::vector<int> tangent_closure(
    const Shape& body, std::vector<int> faces,
    const NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>&
        ancestors);

// The plane's normal out of the face's material: the plane's handedness
// and the face's orientation both apply.
gp_Dir planar_normal(const TopoDS_Face& face, const gp_Pln& plane);

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
