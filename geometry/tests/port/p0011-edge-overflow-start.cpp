// SPDX-License-Identifier: MIT
// Port patch 0011: the edge overflow of the rolling ball fillet. Where the
// ball tangent to the two faces of an edge cannot touch one of them within
// that face along the whole edge (a face narrower than the rounding), OCCT
// found no start (StartsolFailure) and the fillet failed. With the patch
// the start lies on the face beyond the narrow face's far edge or, where
// the ball cannot touch that face, rolls on the far edge, and the narrow
// face vanishes.
//
// A step 2 high along a block (40 long): its top edge (convex) and its foot
// (concave) rounded with 3. The face beyond the side's far edge (the ledge
// below, the top above) is parallel to the edge's other face, so the ball
// rolls on that edge: built, valid, the side consumed, the volume exact.
// With 1.9 the contact stays on the side (an analytic blend, as stock). A
// bar 2 x 2 whose edge is rounded with 5, wider than both its faces: no ball
// touches a face, the fillet fails as stock.

#include <cmath>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
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

// The section a rounding of radius r takes away (convex) or fills
// (concave) where it rolls on the far edge of a side h high (h < r): the
// ball's centre lies r from the other face and r from that edge, c along
// that face from the edge; the region between the face, the side and the
// arc: the integral of r - sqrt(r^2 - u^2) for u in [0, c].
double rolled_section(double r, double h) {
  const double c = std::sqrt(2.0 * r * h - h * h);
  return r * c - 0.5 * c * (r - h) - 0.5 * r * r * std::asin(c / r);
}

// The step: a block 10 deep, 12 high behind y = 6, a ledge 10 high in
// front of it; the step's side (y = 6) is 2 high, the top (z = 12) 4 deep,
// the ledge (z = 10) 6 deep.
TopoDS_Shape step() {
  return profile_prism({{0, 0}, {10, 0}, {10, 12}, {6, 12}, {6, 10}, {0, 10}});
}

const gp_Pln kSide(gp_Pnt(0, 6, 0), gp_Dir(0, 1, 0));

// The step's top edge rounded with 3: the contact on the side (3 below the
// top) lies below the side's foot all along. The ball cannot touch the
// ledge (parallel to the top), so it rolls on the foot: it touches the top
// sqrt 8 behind the side and runs down to the ledge, the side vanishes.
// OCCT alone found no start.
void test_top_edge_of_a_low_step_rolls_on_its_foot() {
  const TopoDS_Shape body    = step();
  const TopoDS_Shape rounded = port::rounded(body, {nearest_edge(body, gp_Pnt(20, 6, 12))}, 3.0);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(rounded), port::volume_of(body) - rolled_section(3.0, 2.0) * 40.0, 1e-6);
  // The side replaced by the rounding.
  CHECK(faces_in_plane(rounded, kSide) == 0);
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE));
}

// The step's foot (concave) rounded with 3: the contact on the side lies
// above its top edge; the ball cannot touch the top (parallel to the
// ledge), so it rolls on that edge and fills the step up to it.
void test_foot_of_a_low_step_rolls_on_its_top_edge() {
  const TopoDS_Shape body   = step();
  const TopoDS_Shape filled = port::rounded(body, {nearest_edge(body, gp_Pnt(20, 6, 10))}, 3.0);
  CHECK(port::valid(filled));
  if (!port::valid(filled)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(filled), port::volume_of(body) + rolled_section(3.0, 2.0) * 40.0, 1e-6);
  CHECK(faces_in_plane(filled, kSide) == 0);
  CHECK(port::count_of(filled, TopAbs_FACE) == port::count_of(body, TopAbs_FACE));
}

// The boundary: with 1.9 both contacts lie on their faces (the side's 0.1
// from its far edge), an analytic blend as stock; the side stays.
void test_rounding_within_the_side_is_as_stock() {
  const TopoDS_Shape body    = step();
  const TopoDS_Shape rounded = port::rounded(body, {nearest_edge(body, gp_Pnt(20, 6, 12))}, 1.9);
  CHECK(port::valid(rounded));
  if (port::valid(rounded)) {
    CHECK_NEAR_TOL(port::volume_of(rounded), port::volume_of(body) - test::fillet_loss(1.9) * 40.0, 1e-7);
    CHECK(faces_in_plane(rounded, kSide) == 1);
    CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 1);
  }
  const TopoDS_Shape filled = port::rounded(body, {nearest_edge(body, gp_Pnt(20, 6, 10))}, 1.9);
  CHECK(port::valid(filled));
  if (port::valid(filled)) {
    CHECK_NEAR_TOL(port::volume_of(filled), port::volume_of(body) + test::fillet_loss(1.9) * 40.0, 1e-7);
    CHECK(faces_in_plane(filled, kSide) == 1);
  }
}

// The other boundary: an edge of a bar 2 x 2 rounded with 5. Both contacts
// leave their faces, and past each far edge lies a face parallel to the
// other side's: the ball could only roll on both far edges, touching no
// face. Not made, as stock.
void test_rounding_wider_than_both_faces_fails_as_stock() {
  const TopoDS_Shape bar     = BRepPrimAPI_MakeBox(40, 2, 2).Shape();
  const TopoDS_Shape rounded = port::rounded(bar, {nearest_edge(bar, gp_Pnt(20, 0, 2))}, 5.0);
  CHECK(!port::valid(rounded));
}

} // namespace

MITCAD_PORT_TEST("0011", test_top_edge_of_a_low_step_rolls_on_its_foot);
MITCAD_PORT_TEST("0011", test_foot_of_a_low_step_rolls_on_its_top_edge);
MITCAD_PORT_TEST("0011", test_rounding_within_the_side_is_as_stock);
MITCAD_PORT_TEST("0011", test_rounding_wider_than_both_faces_fails_as_stock);
