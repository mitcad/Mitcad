// SPDX-License-Identifier: MIT
// Port patch 0013: 0011's edge overflow across several narrow faces and on
// both sides of the edge, and two roundings that overlap on a face between
// their edges made one. Each side's support moves on across the edges its
// contact passes, past faces the ball cannot touch (a sliver), until the
// ball touches a face beyond; the start must clear the faces it passed.
// Two stripes whose contacts on a face between their edges each lie beyond
// the other's (OverlappingStripes) are computed again across each other's
// edge and give one blend.
//
// A block 20 x 10 (40 long in x) whose top front corner is cut off by two
// strips narrower than the rounding, in the section (y, z): from (1, 10) to
// (0.3, 9.6) and on to the front at (0, 9), or from (1, 10) to a sliver
// (0.001, 9.2) and on to (0, 9.2). The top edge of the first strip rounded
// with 5: the ball runs over both strips onto the front; the body is the
// block's corner rounded with 5 (the strips lie wholly in what it takes
// away), one face less. The edge between two strips (1, 10), (0.4, 9.85),
// (0, 9) rounded with 5: both contacts leave their strips, the same
// rounding. A block 40 x 20 x 10 bevelled 1 or 2 along its top front edge,
// both bevel edges rounded with 5: with 1 each rounding runs over the bevel,
// with 2 each fits on it (2.07 from its edge) but they overlap: one
// rounding tangent to the top and the front, the bevel gone (7 faces). The
// boundary: the bevel 4 wide, both edges rounded with 1: the contacts on
// the bevel stay apart, two blends and the bevel between them.

#include <cmath>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

// A prism 40 long in x of a closed polygon in the (y, z) plane.
TopoDS_Shape profile_prism(const std::vector<std::pair<double, double>>& yz) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& corner : yz) {
    polygon.Add(gp_Pnt(0, corner.first, corner.second));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(40, 0, 0)).Shape();
}

// The edge of a shape whose middle lies nearest to p.
TopoDS_Edge nearest_edge(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double      nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& candidate = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(candidate)) {
      continue;
    }
    const BRepAdaptor_Curve curve(candidate);
    const double d = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p);
    if (d < nearest) {
      nearest = d;
      best    = candidate;
    }
  }
  return best;
}

// The block's corner rounded with 5, one face less than the body (its
// strips gone, the rounding in their place).
void check_corner_rounded_with_five(const TopoDS_Shape& body, const TopoDS_Shape& rounded) {
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(rounded), 20.0 * 10.0 * 40.0 - test::fillet_loss(5.0) * 40.0, 1e-7);
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) - 1);
}

// The first strip's top edge rounded with 5: the ball runs over both strips
// (the second 0.75 wide, or a sliver of 0.001 on which no ball touches)
// onto the front.
void test_rounding_runs_over_two_strips_onto_the_front() {
  for (const double z : {9.0, 9.2}) {
    const std::pair<double, double> second = z == 9.0 ? std::pair<double, double>{0.3, 9.6}
                                                      : std::pair<double, double>{0.001, 9.2};
    const TopoDS_Shape body = profile_prism({{0, 0}, {20, 0}, {20, 10}, {1, 10}, second, {0, z}});
    check_corner_rounded_with_five(body, port::rounded(body, {nearest_edge(body, gp_Pnt(20, 1, 10))}, 5.0));
  }
}

// The edge between the two strips rounded with 5: the ball leaves both,
// touching the top and the front.
void test_rounding_between_two_strips_leaves_both() {
  const TopoDS_Shape body = profile_prism({{0, 0}, {20, 0}, {20, 10}, {1, 10}, {0.4, 9.85}, {0, 9}});
  check_corner_rounded_with_five(body, port::rounded(body, {nearest_edge(body, gp_Pnt(20, 0.4, 9.85))}, 5.0));
}

// A block 40 x 20 x 10 bevelled d along its top front edge.
TopoDS_Shape bevelled(double d) {
  return profile_prism({{0, 0}, {20, 0}, {20, 10}, {d, 10}, {0, 10 - d}});
}

// Both edges of a bevel 1 or 2 wide rounded with 5: one rounding, the
// bevel gone.
void test_overlapping_roundings_of_a_bevel_are_one() {
  for (const double d : {1.0, 2.0}) {
    const TopoDS_Shape body    = bevelled(d);
    const TopoDS_Shape rounded = port::rounded(
      body, {nearest_edge(body, gp_Pnt(20, d, 10)), nearest_edge(body, gp_Pnt(20, 0, 10 - d))}, 5.0);
    CHECK(port::valid(rounded));
    if (!port::valid(rounded)) {
      std::fprintf(stderr, "bevel %g: no valid rounding\n", d);
      continue;
    }
    CHECK_NEAR_TOL(port::volume_of(rounded), 20.0 * 10.0 * 40.0 - test::fillet_loss(5.0) * 40.0, 1e-7);
    CHECK(port::count_of(rounded, TopAbs_FACE) == 7);
  }
}

// The boundary: the bevel 4 wide (5.66), both edges rounded with 1: each
// contact on the bevel lies tan(22.5 deg) = 0.41 from its edge, far apart:
// two blends as stock, the bevel between them. Each takes away
// r^2 (tan(pi / 8) - pi / 8) of the section.
void test_roundings_apart_on_a_wide_bevel_are_as_stock() {
  const TopoDS_Shape body = bevelled(4.0);
  const TopoDS_Shape rounded =
    port::rounded(body, {nearest_edge(body, gp_Pnt(20, 4, 10)), nearest_edge(body, gp_Pnt(20, 0, 6))}, 1.0);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  const double each = std::tan(test::kPi / 8.0) - test::kPi / 8.0;
  CHECK_NEAR_TOL(port::volume_of(rounded), (200.0 - 8.0 - 2.0 * each) * 40.0, 1e-7);
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 2);
}

} // namespace

MITCAD_PORT_TEST("0013", test_rounding_runs_over_two_strips_onto_the_front);
MITCAD_PORT_TEST("0013", test_rounding_between_two_strips_leaves_both);
MITCAD_PORT_TEST("0013", test_overlapping_roundings_of_a_bevel_are_one);
MITCAD_PORT_TEST("0013", test_roundings_apart_on_a_wide_bevel_are_as_stock);
