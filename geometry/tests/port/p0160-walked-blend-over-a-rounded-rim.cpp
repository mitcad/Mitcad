// SPDX-License-Identifier: MIT
// Port patch 0160: a blend whose contact on its face reaches the rounded
// rim of a hole in that face, rounded more sharply than the ball, does not
// go onto the rim (the ball cannot touch it); the blend is walked on over
// the gap on the face's own surface and the rim cuts it. Before, the walk's
// first step left the rim at once and the fillet failed.
//
// A block 40 x 20 x 10, its top front edge rounded with 3 (the contact on
// the top 3 behind the front). A hole down through the top whose rim is
// rounded more sharply than 3, its mouth reaching in front of the contact
// and its bore behind it: radius 1.5 at 4.6 behind the front, the rim
// rounded with 0.5 (mouth from 2.6, bore from 3.1), and radius 2 at 5.5
// behind, the rim rounded with 1 (mouth from 2.5, bore from 3.5); the
// rim's seam to the side, away from the cut. Exact check: the rounding
// takes from the body what the ball's corner region holds (the region the
// hole leaves in the body). The boundary: the hole of radius 1.5 at 5.5
// behind the front, its mouth (from 3.5) behind the contact, as stock.
// (geometry/tests/test_fillet_overflow.cpp checks the first hole with the
// rim's seam in front and behind.)

#include <cmath>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 3.0;

// The block with a hole of radius `bore` down through it at (20, y), its
// rim on the top rounded with `rim`; a null shape when that fails.
TopoDS_Shape block_with_rounded_hole(double y, double bore, double rim) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(40, 20, 10)).Shape();
  // (The seam of the hole along +x, away from the front.)
  const TopoDS_Shape hole =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(20, y, 11), gp_Dir(0, 0, -1), gp_Dir(1, 0, 0)), bore, 12).Shape();
  const TopoDS_Shape holed = BRepAlgoAPI_Cut(block, hole).Shape();
  const std::vector<TopoDS_Edge> mouth = port::edges_where(holed, [y, bore](const gp_Pnt& p) {
    return std::abs(p.Z() - 10) < 1e-6 && std::abs(std::hypot(p.X() - 20, p.Y() - y) - bore) < 1e-6;
  });
  CHECK(!mouth.empty());
  const TopoDS_Shape body = port::rounded(holed, mouth, rim);
  CHECK(port::valid(body));
  return body;
}

// The top front edge rounded with kRadius; null when it fails.
TopoDS_Shape front_rounded(const TopoDS_Shape& body) {
  const std::vector<TopoDS_Edge> front = port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Y()) < 1e-6 && std::abs(p.Z() - 10) < 1e-6;
  });
  CHECK(front.size() == 1u);
  return port::rounded(body, front, kRadius);
}

// The body less the ball's corner region along the front top edge.
double expected_volume(const TopoDS_Shape& body) {
  const TopoDS_Shape corner = BRepPrimAPI_MakeBox(gp_Pnt(-1, 0, 10 - kRadius), gp_Pnt(41, kRadius, 11)).Shape();
  const TopoDS_Shape ball =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-2, kRadius, 10 - kRadius), gp_Dir(1, 0, 0)), kRadius, 44).Shape();
  const TopoDS_Shape region = BRepAlgoAPI_Cut(corner, ball).Shape();
  return port::volume_of(BRepAlgoAPI_Cut(body, region).Shape());
}

void check_rounding_over_the_hole(double y, double bore, double rim) {
  const TopoDS_Shape body = block_with_rounded_hole(y, bore, rim);
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape result = front_rounded(body);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    std::fprintf(stderr, "hole at %g, bore %g, rim %g\n", y, bore, rim);
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(result), expected_volume(body), 1e-6);
}

// The rim rounded with 0.5: the contact crosses the mouth, the blend is
// walked over it and cut by the rim.
void test_blend_over_a_hole_whose_rim_is_rounded_with_half() {
  check_rounding_over_the_hole(4.6, 1.5, 0.5);
}

// A wider hole, its rim rounded with 1 (still sharper than the ball).
void test_blend_over_a_hole_whose_rim_is_rounded_with_one() {
  check_rounding_over_the_hole(5.5, 2.0, 1.0);
}

// The boundary: the mouth behind the contact; the blend never reaches the
// rim, one analytic blend as stock.
void test_blend_short_of_a_rounded_rim_is_as_stock() {
  const TopoDS_Shape body = block_with_rounded_hole(5.5, 1.5, 0.5);
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape result = front_rounded(body);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(result), port::volume_of(body) - test::fillet_loss(kRadius) * 40.0, 1e-7);
  CHECK(port::count_of(result, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 1);
}

} // namespace

MITCAD_PORT_TEST("0160", test_blend_over_a_hole_whose_rim_is_rounded_with_half);
MITCAD_PORT_TEST("0160", test_blend_over_a_hole_whose_rim_is_rounded_with_one);
MITCAD_PORT_TEST("0160", test_blend_short_of_a_rounded_rim_is_as_stock);
