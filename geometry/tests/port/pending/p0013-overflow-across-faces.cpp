// SPDX-License-Identifier: MIT
// Port patch 0013: 0011's edge overflow across several narrow faces and on
// both sides of the edge, and two roundings that overlap on a face between
// their edges made one. Each side's support moves on across the edges its
// contact passes, past faces the ball cannot touch (parallel to the other
// side's), until the ball touches a face beyond; the start must clear the
// faces it passed. Two stripes whose contacts on a face between their edges
// each lie beyond the other's are computed again across each other's edge
// and give one blend.
//
// A block 20 x 10 (40 long) whose top front corner is cut into two steps,
// each 1 high and 1 deep: the top edge rounded with 6 (the ball passes the
// first step's side and tread and the second step's onto the front), and
// the edge between the first tread and the second side (both contacts leave
// their faces) rounded with 6: built, valid, the four narrow faces
// consumed, the block's corner rounded with 6. The same edge with 0.5: the
// contacts stay on the steps (as stock). A block bevelled 1.5 at its top
// front edge, both bevel edges rounded with 6 (each rounding wider than
// the bevel) or 4 (each fits on it, they overlap): one rounding, the bevel
// gone; bevelled 3 and rounded with 4 the two do not overlap (as stock).

#include <cmath>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <gp_Pln.hxx>
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

// The planar faces of a shape that lie in the plane.
int faces_in_plane(const TopoDS_Shape& shape, const gp_Pln& plane) {
  int count = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(e.Current()));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln own = surface.Plane();
    if (own.Axis().IsParallel(plane.Axis(), 1e-9) && plane.Distance(own.Location()) < 1e-7) {
      ++count;
    }
  }
  return count;
}

// The block with two steps at its top front corner: the first side y = 2
// (z from 9 to 10), the first tread z = 9 (y from 1 to 2), the second side
// y = 1 (z from 8 to 9), the second tread z = 8 (y from 0 to 1), then the
// front y = 0 down to the bottom.
TopoDS_Shape two_steps() {
  return profile_prism({{0, 0}, {20, 0}, {20, 10}, {2, 10}, {2, 9}, {1, 9}, {1, 8}, {0, 8}});
}

// The block's corner rounded with 6 (centre at y = 6, z = 4): every corner
// of the steps lies more than 6 from that centre (sqrt 41 at the nearest),
// so the steps lie wholly in the corner it takes away.
void check_corner_rounded_with_six(const TopoDS_Shape& body, const TopoDS_Shape& rounded) {
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(rounded), 20.0 * 10.0 * 40.0 - test::fillet_loss(6.0) * 40.0, 1e-6);
  // The two sides and the two treads gone, one rounding in their place.
  CHECK(faces_in_plane(rounded, gp_Pln(gp_Pnt(0, 2, 0), gp_Dir(0, 1, 0))) == 0);
  CHECK(faces_in_plane(rounded, gp_Pln(gp_Pnt(0, 1, 0), gp_Dir(0, 1, 0))) == 0);
  CHECK(faces_in_plane(rounded, gp_Pln(gp_Pnt(0, 0, 9), gp_Dir(0, 0, 1))) == 0);
  CHECK(faces_in_plane(rounded, gp_Pln(gp_Pnt(0, 0, 8), gp_Dir(0, 0, 1))) == 0);
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) - 3);
}

// The top edge (top and first side) rounded with 6: the contact on the
// first side leaves it; past it the first tread (parallel to the top), the
// second side (its contact below its foot) and the second tread (parallel
// again); the ball touches the front. OCCT alone found no start; 0011
// alone rolls on the first side's foot, a ball that cuts into the second
// step.
void test_rounding_passes_two_steps_onto_the_front() {
  const TopoDS_Shape body = two_steps();
  check_corner_rounded_with_six(body, port::rounded(body, {nearest_edge(body, gp_Pnt(20, 2, 10))}, 6.0));
}

// The edge between the first tread and the second side rounded with 6: the
// tread's contact leaves it across the first side onto the top, the side's
// across the second tread onto the front; the same rounding.
void test_rounding_between_two_narrow_faces_leaves_both() {
  const TopoDS_Shape body = two_steps();
  check_corner_rounded_with_six(body, port::rounded(body, {nearest_edge(body, gp_Pnt(20, 1, 9))}, 6.0));
}

// The boundary: that edge with 0.5, its contacts 0.5 along the tread and
// the side (each 1 wide): an analytic blend, as stock.
void test_rounding_within_the_narrow_faces_is_as_stock() {
  const TopoDS_Shape body    = two_steps();
  const TopoDS_Shape rounded = port::rounded(body, {nearest_edge(body, gp_Pnt(20, 1, 9))}, 0.5);
  CHECK(port::valid(rounded));
  if (port::valid(rounded)) {
    CHECK_NEAR_TOL(port::volume_of(rounded), port::volume_of(body) - test::fillet_loss(0.5) * 40.0, 1e-7);
    CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 1);
  }
}

// A block 20 x 10 (40 long) bevelled d at its top front edge.
TopoDS_Shape bevelled(double d) {
  return profile_prism({{0, 0}, {20, 0}, {20, 10}, {d, 10}, {0, 10 - d}});
}

// Both edges of the bevel 1.5 rounded with r: each rounding's contact on
// the bevel lies r tan(22.5 deg) from its edge, 2.49 with 6 (beyond the
// bevel, 2.12 wide) and 1.66 with 4 (on it, past the other's contact).
// Either way the two make one rounding tangent to the top and the front:
// the block's corner rounded with r (the bevel's corners lie outside the
// ball), and the bevel vanishes.
void test_overlapping_roundings_of_a_bevel_are_one() {
  for (const double r : {6.0, 4.0}) {
    const TopoDS_Shape body = bevelled(1.5);
    const TopoDS_Shape rounded =
        port::rounded(body, {nearest_edge(body, gp_Pnt(20, 1.5, 10)), nearest_edge(body, gp_Pnt(20, 0, 8.5))}, r);
    CHECK(port::valid(rounded));
    if (!port::valid(rounded)) {
      continue;
    }
    CHECK_NEAR_TOL(port::volume_of(rounded), 20.0 * 10.0 * 40.0 - test::fillet_loss(r) * 40.0, 1e-6);
    CHECK(port::count_of(rounded, TopAbs_FACE) == 7);
  }
}

// The boundary: the bevel 3 (4.24 wide) rounded with 4 on both edges: the
// contacts on the bevel lie 1.66 from each edge and do not meet, two
// analytic blends as stock; the bevel stays between them. Each takes away
// r^2 (tan(pi / 8) - pi / 8) of the section.
void test_roundings_apart_on_a_wide_bevel_are_as_stock() {
  const TopoDS_Shape body = bevelled(3.0);
  const TopoDS_Shape rounded =
      port::rounded(body, {nearest_edge(body, gp_Pnt(20, 3, 10)), nearest_edge(body, gp_Pnt(20, 0, 7))}, 4.0);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  const double each = 16.0 * (std::tan(test::kPi / 8.0) - test::kPi / 8.0);
  CHECK_NEAR_TOL(port::volume_of(rounded), (200.0 - 4.5 - 2.0 * each) * 40.0, 1e-7);
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 2);
  const gp_Dir bevel_normal(0, -1, 1);
  CHECK(faces_in_plane(rounded, gp_Pln(gp_Pnt(0, 0, 7), bevel_normal)) == 1);
}

} // namespace

MITCAD_PORT_TEST("0013", test_rounding_passes_two_steps_onto_the_front);
MITCAD_PORT_TEST("0013", test_rounding_between_two_narrow_faces_leaves_both);
MITCAD_PORT_TEST("0013", test_rounding_within_the_narrow_faces_is_as_stock);
MITCAD_PORT_TEST("0013", test_overlapping_roundings_of_a_bevel_are_one);
MITCAD_PORT_TEST("0013", test_roundings_apart_on_a_wide_bevel_are_as_stock);
