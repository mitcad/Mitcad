// SPDX-License-Identifier: MIT
// Port patch 0271: two roundings whose contact lines on the face between
// their edges cross each other twice, the face narrowing to less than the
// two roundings' width part of the way. OCCT failed ("fillets have too big
// radiuses"); with the patch the two blends cut each other along their
// intersection between the crossings, and the face is left in two pieces.
//
// A block 20 x 12 x 10 whose two long sides are concave arcs of radius 20
// (vertical cylinders), cut in by `cut` at the middle: the top 12 - 2 cut
// wide there and wider towards the ends. Both top edges rounded with r1 and
// r2, the contacts on the top r1 and r2 from their edges:
// - cut 3.1 (the top 5.8 wide in the middle), both rounded with 4: the
//   contacts cross twice where the top is 8 wide;
// - cut 3.5 (5 wide), rounded with 4 and 3: they cross where it is 7 wide;
// built, valid, the top in two pieces, the same body as each edge rounded
// alone and the two in common. The boundary: cut 3.1 rounded with 2.8 (the
// contacts 0.2 apart in the middle): no crossing, the top one face, as
// stock.

#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

#include "port_test.hpp"

namespace {

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

// The planar faces of a shape in the plane z = 10 (the top's pieces).
int top_pieces(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    if (plane.Axis().Direction().IsParallel(gp_Dir(0, 0, 1), 1e-9) && plane.Distance(gp_Pnt(0, 0, 10)) < 1e-7) {
      ++n;
    }
  }
  return n;
}

// The block x in [0, 20], y in [0, 12], z in [0, 10] whose long sides are
// cut in by `cut` in the middle by vertical cylinders of radius 20.
TopoDS_Shape waisted_block(double cut) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 12, 10)).Shape();
  const TopoDS_Shape c1    = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, cut - 20, -1), gp_Dir(0, 0, 1)), 20, 12).Shape();
  const TopoDS_Shape c2 =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, 32 - cut, -1), gp_Dir(0, 0, 1)), 20, 12).Shape();
  return BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(block, c1).Shape(), c2).Shape();
}

// Both top edges rounded (r1, r2) at once, and each alone, the two in
// common (the two roundings cut by each other); null shapes where they
// fail.
struct Rounded {
  TopoDS_Shape both;
  TopoDS_Shape apart;
};

Rounded rounded_top(const TopoDS_Shape& body, double cut, double r1, double r2) {
  const TopoDS_Edge e1 = nearest_edge(body, gp_Pnt(10, cut, 10));
  const TopoDS_Edge e2 = nearest_edge(body, gp_Pnt(10, 12 - cut, 10));
  Rounded           result;
  BRepFilletAPI_MakeFillet maker(body);
  maker.Add(r1, e1);
  maker.Add(r2, e2);
  result.both              = port::built(maker, "both top edges");
  const TopoDS_Shape one   = port::rounded(body, {e1}, r1);
  const TopoDS_Shape other = port::rounded(body, {e2}, r2);
  CHECK(port::valid(one));
  CHECK(port::valid(other));
  if (port::valid(one) && port::valid(other)) {
    result.apart = BRepAlgoAPI_Common(one, other).Shape();
  }
  return result;
}

void check_cut_by_each_other(double cut, double r1, double r2) {
  const TopoDS_Shape body = waisted_block(cut);
  CHECK(port::valid(body));
  const Rounded r = rounded_top(body, cut, r1, r2);
  CHECK(port::valid(r.both));
  if (!port::valid(r.both) || r.apart.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(r.both), port::volume_of(r.apart), 1e-8);
  // The block's six faces, the top in two, two roundings.
  CHECK(top_pieces(r.both) == 2);
  CHECK(port::count_of(r.both, TopAbs_FACE) == 9);
}

// The top 5.8 wide in the middle, both edges rounded with 4.
void test_equal_roundings_cut_by_each_other_where_the_top_narrows() {
  check_cut_by_each_other(3.1, 4.0, 4.0);
}

// The top 5 wide in the middle, the edges rounded with 4 and 3.
void test_unequal_roundings_cut_by_each_other_where_the_top_narrows() {
  check_cut_by_each_other(3.5, 4.0, 3.0);
}

// The boundary: the top 5.8 wide in the middle, rounded with 2.8: the
// contacts stay 0.2 apart; the top stays one face, as stock.
void test_roundings_whose_contacts_stay_apart_are_as_stock() {
  const TopoDS_Shape body = waisted_block(3.1);
  const Rounded      r    = rounded_top(body, 3.1, 2.8, 2.8);
  CHECK(port::valid(r.both));
  if (!port::valid(r.both) || r.apart.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(r.both), port::volume_of(r.apart), 1e-8);
  CHECK(top_pieces(r.both) == 1);
  CHECK(port::count_of(r.both, TopAbs_FACE) == 8);
}

} // namespace

MITCAD_PORT_TEST("0271", test_equal_roundings_cut_by_each_other_where_the_top_narrows);
MITCAD_PORT_TEST("0271", test_unequal_roundings_cut_by_each_other_where_the_top_narrows);
MITCAD_PORT_TEST("0271", test_roundings_whose_contacts_stay_apart_are_as_stock);
