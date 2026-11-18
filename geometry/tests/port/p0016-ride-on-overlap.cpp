// SPDX-License-Identifier: MIT
// Port patch 0016: two roundings of a face narrower than both whose balls
// cannot make one rounding over it (no ball touches the faces beyond on
// both sides, they are parallel). 0013's computation of the two over that
// face fails, and so did the fillet. With the patch the earlier rounding is
// computed on its own faces and the later one rolls on the earlier one's
// contact line on that face (RideOnOverlap); the two meet on that line and
// the face vanishes. Both convex or both concave.
//
// A plate 1 thick standing 6 high (40 long), both top rims rounded with 0.8
// (convex: the contacts on the top lie 0.8 from each rim, one ball over the
// top would have radius 0.5): built, valid, the top consumed, the volume
// exact. The same with the rims of a slot's floor 1 wide (concave). A plate
// 2 thick: 0.8 fits, two roundings as stock.
//
// The section: the earlier rounding takes (or fills) the quarter corner
// r^2 (1 - pi / 4); the later ball, tangent to its own side (centre r from
// it, d = 2 r - t from the earlier contact line across the face of width
// t), passes through that line h = sqrt(r^2 - d^2) from the face. Between
// its side, the face and its arc it takes r h - h d / 2 - r^2 asin(h / r) / 2.
// The plate and the slot are symmetric, so the order of the two does not
// change the volume.

#include <cmath>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeBox.hxx>

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

// The section the two roundings of radius r on the rims of a face t wide
// take away (or fill), the later riding on the earlier's contact line.
double ridden_section(double r, double t) {
  const double d = 2.0 * r - t;
  const double h = std::sqrt(r * r - d * d);
  return test::fillet_loss(r) + r * h - 0.5 * h * d - 0.5 * r * r * std::asin(h / r);
}

// The two edges of a shape nearest to the points, rounded with r.
TopoDS_Shape both_rounded(const TopoDS_Shape& body, const gp_Pnt& first, const gp_Pnt& second, double r) {
  return port::rounded(body, {nearest_edge(body, first), nearest_edge(body, second)}, r);
}

// The plate 1 thick (y from 0 to 1), 6 high: both top rims with 0.8. The
// top gone, the two roundings in its place.
void test_rims_of_a_thin_plate_ride_on_each_other() {
  const TopoDS_Shape plate   = BRepPrimAPI_MakeBox(40, 1, 6).Shape();
  const TopoDS_Shape rounded = both_rounded(plate, gp_Pnt(20, 0, 6), gp_Pnt(20, 1, 6), 0.8);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(rounded), 240.0 - ridden_section(0.8, 1.0) * 40.0, 1e-6);
  CHECK(port::count_of(rounded, TopAbs_FACE) == 7);
}

// A block 11 x 6 (40 long) with a slot 1 wide (y from 0 to 1) and 3 deep
// along it: the floor's rims (concave) with 0.8. The floor gone, filled up
// to the two roundings.
void test_rims_of_a_narrow_slots_floor_ride_on_each_other() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, -5, 0), gp_Pnt(40, 6, 6)).Shape();
  const TopoDS_Shape slot  = BRepPrimAPI_MakeBox(gp_Pnt(-1, 0, 3), gp_Pnt(41, 1, 7)).Shape();
  const TopoDS_Shape body  = BRepAlgoAPI_Cut(block, slot).Shape();
  CHECK(port::valid(body));
  const TopoDS_Shape filled = both_rounded(body, gp_Pnt(20, 0, 3), gp_Pnt(20, 1, 3), 0.8);
  CHECK(port::valid(filled));
  if (!port::valid(filled)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(filled), port::volume_of(body) + ridden_section(0.8, 1.0) * 40.0, 1e-6);
  CHECK(port::count_of(filled, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 1);
}

// The boundary: a plate 2 thick, both top rims with 0.8. The contacts on
// the top lie 0.4 apart: two analytic blends as stock, the top stays.
void test_rims_of_a_plate_wide_enough_are_as_stock() {
  const TopoDS_Shape plate   = BRepPrimAPI_MakeBox(40, 2, 6).Shape();
  const TopoDS_Shape rounded = both_rounded(plate, gp_Pnt(20, 0, 6), gp_Pnt(20, 2, 6), 0.8);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(rounded), 480.0 - 2.0 * test::fillet_loss(0.8) * 40.0, 1e-7);
  CHECK(port::count_of(rounded, TopAbs_FACE) == 8);
}

} // namespace

MITCAD_PORT_TEST("0016", test_rims_of_a_thin_plate_ride_on_each_other);
MITCAD_PORT_TEST("0016", test_rims_of_a_narrow_slots_floor_ride_on_each_other);
MITCAD_PORT_TEST("0016", test_rims_of_a_plate_wide_enough_are_as_stock);
