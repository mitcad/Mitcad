// SPDX-License-Identifier: MIT
// Port patch 0055: an analytic blend whose contact leaves its face N across
// a hole's rim and comes back runs on over the hole, and the hole's wall
// cuts it: a piece whose side on N is the blend's intersection with the
// wall fills the gap between the pieces split by the faces' domains. OCCT
// failed there or rolled the blend on the hole's rim.
//
// A block 40 x 20 x 10, its top front edge rounded with 3 (the contacts lie
// 3 down the front and 3 back on the top). A hole of radius 2 through the
// front (its top 1.5 under the edge, its centre 0.5 below the contact, so
// that the contact crosses it and the blend over it is cut by the wall
// between the two crossings only), and the same through the top (its
// centre 0.5 behind the contact): built, valid, the volume of the block
// less the rounding's corner and the hole, the part they share counted
// once. A hole whose top lies 4 under the edge: the contact passes above
// it, as stock. The holes' seams face away from the rounding (a seam that
// ends under the blend keeps OCCT's handling).

#include <cmath>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

// The edge of a shape whose middle lies nearest to p.
TopoDS_Edge nearest_edge(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double      nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&            candidate = TopoDS::Edge(e.Current());
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(candidate, first, last);
    if (curve.IsNull()) {
      continue;
    }
    const double d = curve->Value(0.5 * (first + last)).Distance(p);
    if (d < nearest) {
      nearest = d;
      best    = candidate;
    }
  }
  return best;
}

// The block with a hole of radius 2 along `axis` (the axis's x direction
// the hole's seam).
TopoDS_Shape holed_block(const gp_Ax2& axis, double length) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(40, 20, 10).Shape();
  return BRepAlgoAPI_Cut(block, BRepPrimAPI_MakeCylinder(axis, 2.0, length).Shape()).Shape();
}

// The volume the rounding's corner (behind the front y = 0 and under the
// top z = 10, outside the quarter circle about y = 3, z = 7) shares with a
// hole of radius 2 whose centre lies 0.5 beyond the contact: w from the
// contact into the corner, the hole's width 2 sqrt(4 - (w + 0.5)^2), the
// corner's depth 3 - sqrt(9 - w^2), up to the hole's edge at w = 1.5.
double corner_in_hole() {
  double    shared = 0.0;
  const int n      = 200000;
  for (int i = 0; i < n; ++i) {
    const double w = 1.5 * (i + 0.5) / n;
    shared += 2.0 * std::sqrt(4.0 - (w + 0.5) * (w + 0.5)) * (3.0 - std::sqrt(9.0 - w * w)) * (1.5 / n);
  }
  return shared;
}

const double kBlock  = 40.0 * 20.0 * 10.0;
const double kCorner = 40.0 * 9.0 * (1.0 - test::kPi / 4.0);

// The block's top front edge rounded with 3.
TopoDS_Shape top_front_rounded(const TopoDS_Shape& body) {
  return port::rounded(body, {nearest_edge(body, gp_Pnt(20, 0, 10))}, 3.0);
}

// The hole through the front (along y, 20 long in the block), its centre
// 3.5 under the edge: the contact on the front (3 under the edge) leaves
// the front across the hole's rim and comes back.
void test_rounding_over_a_hole_in_the_front_is_cut_by_its_wall() {
  const TopoDS_Shape body = holed_block(gp_Ax2(gp_Pnt(20, -1, 6.5), gp_Dir(0, 1, 0), gp_Dir(0, 0, -1)), 22.0);
  const TopoDS_Shape rounded = top_front_rounded(body);
  CHECK(port::valid(rounded));
  if (port::valid(rounded)) {
    CHECK_NEAR_TOL(port::volume_of(rounded), kBlock - 80.0 * test::kPi - kCorner + corner_in_hole(), 1e-7);
  }
}

// The hole through the top (along z, 10 long in the block), its centre 3.5
// behind the front: the contact on the top crosses it.
void test_rounding_over_a_hole_in_the_top_is_cut_by_its_wall() {
  const TopoDS_Shape body = holed_block(gp_Ax2(gp_Pnt(20, 3.5, -1), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), 12.0);
  const TopoDS_Shape rounded = top_front_rounded(body);
  CHECK(port::valid(rounded));
  if (port::valid(rounded)) {
    CHECK_NEAR_TOL(port::volume_of(rounded), kBlock - 40.0 * test::kPi - kCorner + corner_in_hole(), 1e-7);
  }
}

// The boundary: the hole through the front 6 under the edge (its top 4
// under it), below the contact: no gap, an analytic blend as stock.
void test_rounding_above_a_hole_is_as_stock() {
  const TopoDS_Shape body    = holed_block(gp_Ax2(gp_Pnt(20, -1, 4), gp_Dir(0, 1, 0), gp_Dir(0, 0, -1)), 22.0);
  const TopoDS_Shape rounded = top_front_rounded(body);
  CHECK(port::valid(rounded));
  if (port::valid(rounded)) {
    CHECK_NEAR_TOL(port::volume_of(rounded), kBlock - 80.0 * test::kPi - kCorner, 1e-7);
    CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 1);
  }
}

} // namespace

MITCAD_PORT_TEST("0055", test_rounding_over_a_hole_in_the_front_is_cut_by_its_wall);
MITCAD_PORT_TEST("0055", test_rounding_over_a_hole_in_the_top_is_cut_by_its_wall);
MITCAD_PORT_TEST("0055", test_rounding_above_a_hole_is_as_stock);
