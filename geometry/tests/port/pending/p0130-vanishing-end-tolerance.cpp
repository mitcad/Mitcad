// SPDX-License-Identifier: MIT
// Port patch 0130: a rounding whose two faces come tangent at the vertex
// where it ends vanishes there. Where the walk stops short of the vertex
// (its sections singular) with both contacts within a hundredth of the
// radius of it, both faces holding it and their normals there less than
// 0.002 rad from parallel, ChFi3d_ShrinkVanishingEnd moves the walked
// contact curves' ends to the vertex and raises their tolerance by the
// distance their end moved (mitcad#1986: a B-spline whose end pole moves by
// d moves by at most d; it was 1.01 times the larger end's distance), and
// the rounding ends in a point there. Without the patch the corner at the
// vertex failed or built a cap of micrometres whose wire intersects itself.
//
// The body, synthetic: a top of two faces meeting along a straight edge
// (y = 0, z = 0) from x = 0 to the vertex at x = x1, the plane z = 0 for
// y < 0 and for y > 0 the surface z = -k (x0 - x) y up to x0 = 1 (an
// angle of atan(k (x0 - x)) with the plane, going linearly to nought) and
// z = 0 beyond: from x0 to the vertex the faces are tangent, so the walk of
// the edge's rounding stops at x0 (a few micrometres before it, where its
// section of width R k (x0 - x) gets singular), x1 - x0 from the vertex.
// The edge rounded with R = 1:
// - inside, x1 - x0 = 0.99 % of R (the contacts' ends within the
//   hundredth): valid, the spine's edge gone, the rounding ending in a
//   point at the vertex, and every tolerance at least the distance the
//   ends moved (x1 - x0) and at most that plus the walk's own;
// - outside, x1 - x0 = 1.01 % of R: the end is left as the walk made it
//   (no tolerance raised by the hundredth; the corner fails or is built
//   otherwise).

#include <cmath>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Precision.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 1.0; // the rounding's radius
constexpr double kTaper  = 0.5; // k: the faces' angle falls by atan(k) per unit along the edge
constexpr double kFlat   = 1.0; // x0: where the faces come tangent
constexpr double kWidth  = 2.0; // each face's width beside the edge
constexpr double kDepth  = 3.0; // the body's depth below the edge

// The body whose top edge's faces come tangent at x0 and stay tangent up to
// the edge's end vertex at x0 + tangent (see above).
TopoDS_Shape tangent_ended_top(double tangent) {
  const double x1 = kFlat + tangent;
  // z = -k (x0 - x) y, bilinear up to x0 and flat beyond (degree 1 in x and
  // y, the parameters the coordinates).
  NCollection_Array2<gp_Pnt> poles(1, 3, 1, 2);
  poles.SetValue(1, 1, gp_Pnt(0, 0, 0));
  poles.SetValue(1, 2, gp_Pnt(0, kWidth, -kTaper * kFlat * kWidth));
  poles.SetValue(2, 1, gp_Pnt(kFlat, 0, 0));
  poles.SetValue(2, 2, gp_Pnt(kFlat, kWidth, 0));
  poles.SetValue(3, 1, gp_Pnt(x1, 0, 0));
  poles.SetValue(3, 2, gp_Pnt(x1, kWidth, 0));
  NCollection_Array1<double> uknots(1, 3);
  uknots.SetValue(1, 0.0);
  uknots.SetValue(2, kFlat);
  uknots.SetValue(3, x1);
  NCollection_Array1<int> umults(1, 3);
  umults.SetValue(1, 2);
  umults.SetValue(2, 1);
  umults.SetValue(3, 2);
  NCollection_Array1<double> vknots(1, 2);
  vknots.SetValue(1, 0.0);
  vknots.SetValue(2, kWidth);
  NCollection_Array1<int> vmults(1, 2);
  vmults.SetValue(1, 2);
  vmults.SetValue(2, 2);
  const occ::handle<Geom_BSplineSurface> sloped =
    new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, 1, 1);
  const occ::handle<Geom_Surface> surface(sloped);
  const TopoDS_Face   face  = BRepBuilderAPI_MakeFace(surface, Precision::Confusion()).Face();
  const TopoDS_Shape  right = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, -kDepth)).Shape();
  const TopoDS_Shape  left  = BRepPrimAPI_MakeBox(gp_Pnt(0, -kWidth, -kDepth), gp_Pnt(x1, 0, 0)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(left, right).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

std::vector<TopoDS_Edge> on_the_spine(const TopoDS_Shape& shape) {
  return port::edges_where(shape, [](const gp_Pnt& m) { return std::abs(m.Y()) < 1e-9 && std::abs(m.Z()) < 1e-9; });
}

// Inside: the contacts' ends 0.99 % of the radius from the vertex.
void test_rounding_vanishing_within_a_hundredth_of_the_radius() {
  const double       tangent = 0.0099 * kRadius;
  const TopoDS_Shape body    = tangent_ended_top(tangent);
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> spine = on_the_spine(body);
  CHECK(spine.size() == 1);
  if (spine.size() != 1) {
    return;
  }
  const TopoDS_Shape result = port::rounded(body, spine, kRadius);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  // The rounding reaches the vertex: nothing of the spine's edge is left.
  CHECK(on_the_spine(result).empty());
  // The contact curves' ends moved by x1 - x0 and a few micrometres (the
  // walk stops a little before x0, its contacts as far from the edge); the
  // tolerances are raised by that, plus the walk's own (1e-4 at most).
  const double tolerance = port::max_tolerance(result);
  if (tolerance < 0.99 * tangent || tolerance > tangent + 2e-4) {
    std::fprintf(stderr, "vanishing end 0.99 %%: largest tolerance %.7g, the ends moved about %.7g\n", tolerance,
                 tangent);
  }
  CHECK(tolerance >= 0.99 * tangent);
  CHECK(tolerance <= tangent + 2e-4);
}

// Outside: the contacts' ends 1.01 % of the radius from the vertex, past
// the hundredth: not moved, so no tolerance raised to a hundredth of the
// radius (the corner fails, or is built as without the patch).
void test_rounding_end_past_a_hundredth_of_the_radius_is_left() {
  const TopoDS_Shape body = tangent_ended_top(0.0101 * kRadius);
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> spine = on_the_spine(body);
  CHECK(spine.size() == 1);
  if (spine.size() != 1) {
    return;
  }
  const TopoDS_Shape result = port::rounded(body, spine, kRadius);
  if (!result.IsNull()) {
    std::fprintf(stderr, "vanishing end 1.01 %%: built (valid %d), largest tolerance %.7g\n",
                 port::valid(result) ? 1 : 0, port::max_tolerance(result));
  }
  CHECK(result.IsNull() || port::max_tolerance(result) < 0.01 * kRadius);
}

} // namespace

MITCAD_PORT_TEST("0130", test_rounding_vanishing_within_a_hundredth_of_the_radius);
MITCAD_PORT_TEST("0130", test_rounding_end_past_a_hundredth_of_the_radius_is_left);
