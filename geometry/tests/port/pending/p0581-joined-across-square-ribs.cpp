// SPDX-License-Identifier: MIT
// Port patch 0581: roundings joined across a rib or a notch whose sides are
// square to the spine (0241's ChFi3d_JoinAcrossNotches). The sides cut the
// blend along sections and give no piece, so the piece after the gap starts
// where the blend crosses the far edge of the rib's end (or the notch's
// back); the patch splits that piece where the other stripe ends, and where
// a stripe joined before is joined again (two ribs, three stripes) every
// surface data on the old surface takes the new one. Without it the rib's
// side lost its edge with the blend and the shell was open, and with two
// ribs the stripe joined first stayed a face of its own.
//
// Checked: one blend face over the whole wall, the shell closed, the body
// valid and the material that of the rounding's quarter round over the
// wall, cut by the ribs (or the notch).

#include <cmath>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Dir.hxx>

#include "port_test.hpp"

namespace {

constexpr double kLength = 40.0; // along the spine (x)
constexpr double kRadius = 8.0;  // the rounding's radius
constexpr double kWall   = 5.0;  // the pocket's wall at y = kWall
constexpr double kRoof   = 15.0; // the pocket's ceiling at z = kRoof
constexpr double kStand  = 6.0;  // how far a rib stands out of the wall
constexpr double kThick  = 1.0;  // a rib's (or the notch's) width along the spine

TopoDS_Shape box(double x0, double y0, double z0, double x1, double y1, double z1) {
  return BRepPrimAPI_MakeBox(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1)).Shape();
}

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, true);
  unify.Build();
  return unify.Shape();
}

// A block 40 x 20 x 20 with a pocket under a ceiling at z = 15 in front of
// a wall at y = 5 (open below, in front and at both ends), ribs 1 thick
// standing out 6 from the wall at the given x, from the floor up to the
// ceiling. The wall's edge with the ceiling is broken at each rib.
TopoDS_Shape pocket_with_ribs(const std::vector<double>& ribs) {
  TopoDS_Shape pocket = box(-1, kWall, -1, kLength + 1, 21, kRoof);
  for (const double x : ribs) {
    pocket = BRepAlgoAPI_Cut(pocket, box(x - 0.5 * kThick, kWall - 1, -2, x + 0.5 * kThick, kWall + kStand, kRoof + 1))
               .Shape();
  }
  return unified(BRepAlgoAPI_Cut(box(0, 0, 0, kLength, 20, 20), pocket).Shape());
}

// What the rounding of the wall's edge with the ceiling adds: the corner
// y in [5, 13], z in [7, 15] outside the ball's circle, over the whole
// length (where a rib stands, the body has it already).
TopoDS_Shape pocket_rounding_fill() {
  const TopoDS_Shape corner = box(0, kWall, kRoof - kRadius, kLength, kWall + kRadius, kRoof);
  const TopoDS_Shape ball =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1, kWall + kRadius, kRoof - kRadius), gp::DX()), kRadius, kLength + 2)
      .Shape();
  return BRepAlgoAPI_Cut(corner, ball).Shape();
}

// A block 40 x 20 x 20 with a notch 1 wide and 6 deep cut into its front
// (y = 0) at x = 20, through its whole height: the front's edge with the top
// is broken at the notch. (A notch in the pocket's wall would not reach the
// pocket's rounding, which lies in front of the wall; the convex edge is
// the notch's counterpart of the pocket's rib.)
TopoDS_Shape notched_block() {
  const double x = 0.5 * kLength;
  return unified(
    BRepAlgoAPI_Cut(box(0, 0, 0, kLength, 20, 20), box(x - 0.5 * kThick, -1, -1, x + 0.5 * kThick, kStand, 21))
      .Shape());
}

// What the rounding of the front's edge with the top takes: the corner
// y in [0, 8], z in [12, 20] outside the ball's circle.
TopoDS_Shape notch_rounding_cut() {
  const TopoDS_Shape corner = box(-1, -1, 20 - kRadius, kLength + 1, kRadius, 21);
  const TopoDS_Shape ball =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-2, kRadius, 20 - kRadius), gp::DX()), kRadius, kLength + 4).Shape();
  return BRepAlgoAPI_Cut(corner, ball).Shape();
}

// The faces of the shape on a cylinder of the rounding's radius.
int rounding_faces(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - kRadius) < 1e-6) {
      ++n;
    }
  }
  return n;
}

// The edges of the shape that bound one face only (not seams, not
// degenerated): none in a closed shell.
int free_edges(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  int n = 0;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> distinct;
    bool                                                          seam = false;
    for (NCollection_List<TopoDS_Shape>::Iterator it(faces(i)); it.More(); it.Next()) {
      distinct.Add(it.Value());
      seam = seam || BRep_Tool::IsClosed(edge, TopoDS::Face(it.Value()));
    }
    if (distinct.Extent() == 1 && !seam) {
      ++n;
    }
  }
  return n;
}

// The edges of `body` on the line y = y0, z = z0 rounded with the radius:
// one blend face over them all, closed, valid, with the expected volume.
void check_joined(const TopoDS_Shape& body, double y0, double z0, int edges, double expected_volume) {
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> chain = port::edges_where(
    body, [y0, z0](const gp_Pnt& m) { return std::abs(m.Y() - y0) < 1e-7 && std::abs(m.Z() - z0) < 1e-7; });
  CHECK(static_cast<int>(chain.size()) == edges);
  const TopoDS_Shape result = port::rounded(body, chain, kRadius);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  const int faces = rounding_faces(result);
  const int open  = free_edges(result);
  if (faces != 1 || open != 0) {
    std::fprintf(stderr, "joined across: %d rounding faces, %d free edges\n", faces, open);
  }
  CHECK(faces == 1);
  CHECK(open == 0);
  CHECK(port::valid(result));
  CHECK_NEAR_TOL(port::volume_of(result), expected_volume, 1e-6);
}

// The case's shape: one rib, the ceiling's edge with the wall two edges.
void test_pocket_rounding_joined_across_a_square_rib() {
  const TopoDS_Shape body = pocket_with_ribs({0.5 * kLength});
  check_joined(body, kWall, kRoof, 2,
               port::volume_of(BRepAlgoAPI_Fuse(body, pocket_rounding_fill()).Shape()));
}

// Two ribs, three edges: the join chained over three stripes (the stripe
// joined first takes the surface of the second join too).
void test_pocket_rounding_joined_across_two_square_ribs() {
  const TopoDS_Shape body = pocket_with_ribs({13.5, 26.5});
  check_joined(body, kWall, kRoof, 3,
               port::volume_of(BRepAlgoAPI_Fuse(body, pocket_rounding_fill()).Shape()));
}

// A second geometry: a convex edge broken by a notch whose sides are square
// to the spine, its back 6 deep (the blend's contact on the top, 8 in, stays
// on the top over the notch).
void test_rounding_joined_across_a_square_notch() {
  const TopoDS_Shape body = notched_block();
  check_joined(body, 0.0, 20.0, 2, port::volume_of(BRepAlgoAPI_Cut(body, notch_rounding_cut()).Shape()));
}

} // namespace

MITCAD_PORT_TEST("0581", test_pocket_rounding_joined_across_a_square_rib);
MITCAD_PORT_TEST("0581", test_pocket_rounding_joined_across_two_square_ribs);
MITCAD_PORT_TEST("0581", test_rounding_joined_across_a_square_notch);
