// SPDX-License-Identifier: MIT
// Port patch 0271: two roundings whose contact lines on the face between
// their edges cross each other twice, the face narrowing to less than the
// two roundings' width part of the way. OCCT failed ("fillets have too big
// radiuses"); with the patch the two blends cut each other along their
// intersection between the crossings, and the face is left in two pieces.
//
// test_fillet_results.cpp rounds a block whose two long sides are concave
// arcs (two tori) and a channel narrowing between bulging walls. Here a
// block 20 x 9 x 10 with one long side straight (y = 0) and the other a
// concave arc of radius 20, the top 8.48 wide at the ends and 5.8 in the
// middle; both top edges rounded with 4 (a cylinder and a torus, their
// contacts on the top crossing twice where it is 8 wide): built, valid,
// the top in two pieces, the same body as each edge rounded alone and the
// two in common. The boundary: rounded with 2.5, and with 2.8 (the
// contacts 0.2 apart in the middle): no crossing, the top one face, as
// stock.

#include <vector>

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

// The block x in [0, 20], y in [0, 9], z in [0, 10] whose side y = 9 is cut
// in by 3.2 in the middle by a vertical cylinder of radius 20 (its seam
// outside the block): the top 5.8 wide at x = 10, 8.48 at the ends.
TopoDS_Shape block_with_one_arc_side() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 9, 10)).Shape();
  const TopoDS_Shape arc =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, 25.8, -1), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), 20, 12).Shape();
  return BRepAlgoAPI_Cut(block, arc).Shape();
}

const gp_Pnt kStraight(10, 0, 10), kArc(10, 5.8, 10);

// Both top edges rounded with r at once, and each alone, the two in common
// (the two roundings cut by each other); null shapes where they fail.
struct Rounded {
  TopoDS_Shape both;
  TopoDS_Shape apart;
};

Rounded rounded_top(const TopoDS_Shape& body, double r) {
  Rounded result;
  result.both               = port::rounded(body, {nearest_edge(body, kStraight), nearest_edge(body, kArc)}, r);
  const TopoDS_Shape one    = port::rounded(body, {nearest_edge(body, kStraight)}, r);
  const TopoDS_Shape other  = port::rounded(body, {nearest_edge(body, kArc)}, r);
  CHECK(port::valid(one));
  CHECK(port::valid(other));
  if (port::valid(one) && port::valid(other)) {
    result.apart = BRepAlgoAPI_Common(one, other).Shape();
  }
  return result;
}

// Rounded with 4: the contacts cross twice; between the crossings the two
// blends cut each other and the top is left in two pieces.
void test_cylinder_and_torus_cut_by_each_other_where_the_top_narrows() {
  const TopoDS_Shape body = block_with_one_arc_side();
  CHECK(port::valid(body));
  const Rounded r = rounded_top(body, 4.0);
  CHECK(port::valid(r.both));
  if (!port::valid(r.both) || r.apart.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(r.both), port::volume_of(r.apart), 1e-8);
  CHECK(top_pieces(r.both) == 2);
  // The block's six faces, the top in two, two roundings.
  CHECK(port::count_of(r.both, TopAbs_FACE) == 9);
}

// The boundary: rounded with 2.5 and 2.8, the contacts apart all along;
// the top stays one face, as stock.
void test_roundings_whose_contacts_stay_apart_are_as_stock() {
  const TopoDS_Shape body = block_with_one_arc_side();
  for (const double radius : {2.5, 2.8}) {
    const Rounded r = rounded_top(body, radius);
    CHECK(port::valid(r.both));
    if (!port::valid(r.both) || r.apart.IsNull()) {
      continue;
    }
    CHECK_NEAR_TOL(port::volume_of(r.both), port::volume_of(r.apart), 1e-8);
    CHECK(top_pieces(r.both) == 1);
    CHECK(port::count_of(r.both, TopAbs_FACE) == 8);
  }
}

} // namespace

MITCAD_PORT_TEST("0271", test_cylinder_and_torus_cut_by_each_other_where_the_top_narrows);
MITCAD_PORT_TEST("0271", test_roundings_whose_contacts_stay_apart_are_as_stock);
