// SPDX-License-Identifier: MIT
// Port patch 0581: roundings joined across a rib whose sides are square to
// the spine (0241's ChFi3d_JoinAcrossNotches). The rib's sides cut the
// blend along sections and give no piece, so the piece after the gap starts
// where the blend crosses the far edge of the rib's end; the patch splits
// that piece where the other stripe ends, and where a stripe joined before
// is joined again (two ribs, three stripes) every surface data on the old
// surface takes the new one. Without it the rib's side lost its edge with
// the blend and the shell was open, and with two ribs the stripe joined
// first stayed a face of its own.
//
// A pocket under a ceiling (the plate z from 0 to 10) between two walls,
// x < 0 and x > 40, from y = -15 to 15, 18 deep; at both ends of each wall
// a block 2 long whose side lies at 8 from its wall, where a rounding of 8
// touches the ceiling. Ribs 1 thick stand out 6 from both walls at y from 9
// to 10 (ribs & 1) and from -10 to -9 (ribs & 2). The walls' edges with the
// ceiling rounded with 8: built, valid, one cylinder along each wall (cut
// by the ribs), the material added the rounding's section along the walls
// and over each rib only its part beyond the rib's end. One rib on each
// wall, the other one, and both; the boundary: no ribs, one edge along
// each wall, as before.

#include <cmath>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>

#include "port_test.hpp"

namespace {

// The pocket with the ribs (bits of `ribs`).
TopoDS_Shape pocket_with_ribs(int ribs) {
  TopoDS_Shape s   = BRepPrimAPI_MakeBox(gp_Pnt(-20, -25, 0), gp_Pnt(60, 25, 10)).Shape();
  const auto   add = [&s](const gp_Pnt& a, const gp_Pnt& b) {
    s = BRepAlgoAPI_Fuse(s, BRepPrimAPI_MakeBox(a, b).Shape()).Shape();
  };
  add(gp_Pnt(-20, -15, -18), gp_Pnt(0, 15, 0));
  add(gp_Pnt(40, -15, -18), gp_Pnt(60, 15, 0));
  for (const double y : {15.0, -17.0}) {
    add(gp_Pnt(-20, y, -18), gp_Pnt(8, y + 2, 0));
    add(gp_Pnt(32, y, -18), gp_Pnt(60, y + 2, 0));
  }
  for (const double y : {9.0, -10.0}) {
    if ((ribs & (y > 0 ? 1 : 2)) != 0) {
      add(gp_Pnt(0, y, -18), gp_Pnt(6, y + 1, 0));
      add(gp_Pnt(34, y, -18), gp_Pnt(40, y + 1, 0));
    }
  }
  ShapeUpgrade_UnifySameDomain unify(s, true, true, true);
  unify.Build();
  return unify.Shape();
}

// The walls' edges with the ceiling rounded with 8: valid, the material
// added, one cylinder along each wall.
void check_pocket_rounded(int ribs) {
  const TopoDS_Shape body = pocket_with_ribs(ribs);
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> walls = port::edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.Z()) < 1e-7 && (std::abs(m.X()) < 1e-7 || std::abs(m.X() - 40) < 1e-7) &&
           std::abs(m.Y()) < 15;
  });
  const TopoDS_Shape result = port::rounded(body, walls, 8.0);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    std::fprintf(stderr, "pocket with ribs %d: %s\n", ribs, result.IsNull() ? "not built" : "invalid");
    return;
  }
  // The rounding's section along the walls, 30 long less a rib's 1; over
  // a rib only its part beyond the rib's end (x > 6).
  const int    nr      = ((ribs & 1) != 0 ? 1 : 0) + ((ribs & 2) != 0 ? 1 : 0);
  const double section = 64 - 16 * test::kPi;
  const double beyond  = 16 - (std::sqrt(60.0) + 32 * std::asin(0.25));
  CHECK_NEAR_TOL(port::volume_of(result) - port::volume_of(body), 2 * (section * (30 - nr) + beyond * nr), 1e-6);
  // One cylinder along each wall.
  int cylinders = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    cylinders += BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cylinder ? 1 : 0;
  }
  CHECK(cylinders == 2);
}

// A rib on each wall at y from 9 to 10: two stripes along each wall joined
// across it.
void test_roundings_joined_across_one_rib() {
  check_pocket_rounded(1);
}

// The rib at y from -10 to -9: the stripes the other way round.
void test_roundings_joined_across_the_other_rib() {
  check_pocket_rounded(2);
}

// Both ribs: three stripes along each wall, the middle one joined twice.
void test_roundings_joined_across_two_ribs() {
  check_pocket_rounded(3);
}

// The boundary: no ribs, one stripe along each wall; nothing to join.
void test_roundings_along_whole_walls_are_as_before() {
  check_pocket_rounded(0);
}

} // namespace

MITCAD_PORT_TEST("0581", test_roundings_joined_across_one_rib);
MITCAD_PORT_TEST("0581", test_roundings_joined_across_the_other_rib);
MITCAD_PORT_TEST("0581", test_roundings_joined_across_two_ribs);
MITCAD_PORT_TEST("0581", test_roundings_along_whole_walls_are_as_before);
