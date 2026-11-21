// SPDX-License-Identifier: MIT
// Port patch 1000: a chamfer whose chain runs across a slight concave
// crease of its other face. 0980's crease turn (StartSol: a face taken for
// tangent that meets the face before at a concave crease of more than the
// angular tolerance is no continuation; the ball turns about the crease's
// edge, rolling on it) took the chamfer's piece after the crease for a
// blend rolling on the crease's edge, which chamfers do not have
// ("PerformSurf Not Implemented"): the chamfer failed. A chamfer's contact,
// at its distance along the face, passes such a crease onto the next face:
// the crease turn is a rounding's only.
//
// A prism 10 long in x whose section in the (y, z) plane has a top of two
// lines meeting at V = (0, 10): z = 10 for y from -10 to 0, then rising at
// `degrees` for 10; its ends square to the top's lines. The top is two
// faces meeting at a concave crease along x through V; the edges between
// the top and the front x = 0 are one chain (the faces within a tenth of a
// radian of tangent). The chain chamfered by 1: at 2 and 4 degrees (past
// the angular tolerance) built, valid, one solid; the chamfer takes off its
// triangle of 1 / 2 along the chain, its centroid's path 2 / 3 tan(a / 2)
// longer than the chain over the crease (within 1e-3: the chamfer's walk
// across the crease). The boundary: the top flat (no crease), as stock:
// one plane, exactly 1 / 2 times 20.

#include <cmath>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

// The prism whose top rises at `degrees` past V; the rising line a
// B-spline (straight, of degree 2), so that its face is an extrusion of a
// B-spline and the chamfer is walked there.
TopoDS_Shape creased_prism(double degrees) {
  const double a = degrees * test::kPi / 180.0;
  // The far end of the rising line, and the end face down from it square
  // to that line to the bottom z = 0.
  const gp_Pnt v(0, 0, 10), d(0, 10.0 * std::cos(a), 10.0 + 10.0 * std::sin(a));
  const gp_Pnt e(0, d.Y() + d.Z() * std::tan(a), 0), b(0, -10, 0), t(0, -10, 10);
  NCollection_Array1<gp_Pnt> poles(1, 3);
  poles.SetValue(1, v);
  poles.SetValue(2, gp_Pnt(0.5 * (v.XYZ() + d.XYZ())));
  poles.SetValue(3, d);
  NCollection_Array1<double> knots(1, 2);
  knots.SetValue(1, 0.0);
  knots.SetValue(2, 1.0);
  NCollection_Array1<int> mults(1, 2);
  mults.SetValue(1, 3);
  mults.SetValue(2, 3);
  const occ::handle<Geom_BSplineCurve> rising = new Geom_BSplineCurve(poles, knots, mults, 2);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(b, e).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(e, d).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(rising).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(v, t).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(t, b).Edge());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(10, 0, 0)).Shape();
}

// The chain of the top's edges with the front x = 0, chamfered by 1; a
// null shape where it fails.
TopoDS_Shape front_top_chamfered(const TopoDS_Shape& body) {
  const std::vector<TopoDS_Edge> chain = port::edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.X()) < 1e-9 && m.Z() > 10.0 - 1e-9;
  });
  CHECK(!chain.empty());
  BRepFilletAPI_MakeChamfer chamfer(body);
  chamfer.Add(1.0, chain.front());
  return port::built(chamfer, "chamfer");
}

void check_chamfered_across_the_crease(double degrees) {
  const TopoDS_Shape body = creased_prism(degrees);
  CHECK(port::valid(body));
  const TopoDS_Shape result = front_top_chamfered(body);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    std::fprintf(stderr, "crease of %g degrees: no valid chamfer\n", degrees);
    return;
  }
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  const double half    = 0.5 * degrees * test::kPi / 180.0;
  const double removed = port::volume_of(body) - port::volume_of(result);
  CHECK_NEAR_TOL(removed, 0.5 * (20.0 + 2.0 / 3.0 * std::tan(half)), 1e-3);
}

// A crease of 2 degrees: the chamfer runs across it.
void test_chamfer_across_a_crease_of_two_degrees() {
  check_chamfered_across_the_crease(2.0);
}

// A crease of 4 degrees.
void test_chamfer_across_a_crease_of_four_degrees() {
  check_chamfered_across_the_crease(4.0);
}

// The boundary: no crease (the B-spline continuing the top's line): the
// chamfer as stock.
void test_chamfer_along_a_flat_top_is_as_stock() {
  const TopoDS_Shape body   = creased_prism(0.0);
  const TopoDS_Shape result = front_top_chamfered(body);
  CHECK(port::valid(result));
  if (port::valid(result)) {
    CHECK_NEAR_TOL(port::volume_of(body) - port::volume_of(result), 10.0, 1e-3);
  }
}

} // namespace

MITCAD_PORT_TEST("1000", test_chamfer_across_a_crease_of_two_degrees);
MITCAD_PORT_TEST("1000", test_chamfer_across_a_crease_of_four_degrees);
MITCAD_PORT_TEST("1000", test_chamfer_along_a_flat_top_is_as_stock);
