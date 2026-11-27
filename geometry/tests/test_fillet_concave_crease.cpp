// SPDX-License-Identifier: MIT
// A rounding whose contact on its side crosses a concave crease of a few
// degrees, built with OCCT's fillet alone: the side is two faces meeting
// along a slanted line from the rounded edge, the second turned outward
// about that line, so that the faces' normals lie within the tenth of a
// radian OCCT's fillet takes for tangent. The ball cannot pass its contact
// from one face onto the other there: it turns about the crease until it
// touches the second face. OCCT walked on as over a tangent edge, its walk
// on the second face starting a little further on than the one on the
// first face ended, and the gap left the shape invalid or the fillet
// undone. The ball now rolls on the crease's edge between the two. A fin
// of a stored design, its chamfered top joined to the straight part by
// slivers meeting their neighbours at creases of a few degrees, is the case
// behind the patch 0980 of the OCCT port (third_party/vcpkg-ports,
// mitcad#343).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <ShapeCustom.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The material a rounding of radius r takes off an edge of dihedral angle
// theta, per unit of its length.
double section_loss(double theta, double r) {
  return r * r * (1.0 / std::tan(theta / 2) - (kPi - theta) / 2);
}

TopoDS_Face polygon(std::initializer_list<gp_Pnt> points) {
  BRepBuilderAPI_MakePolygon poly;
  for (const gp_Pnt& p : points) {
    poly.Add(p);
  }
  poly.Close();
  return BRepBuilderAPI_MakeFace(poly.Wire(), true).Face();
}

struct Case {
  double tilt;   // degrees the front beyond the crease turns outward
  double radius; // the rounding's
  bool reversed; // the edges given the other way round
};

// The box [0, 20] x [0, 10] x [0, 10], its front y = 0 split by the crease
// from (8, 0, 10) on the top edge to (12, 0, 0), the front beyond it turned
// outward about the crease; its faces as B-splines (the walk, not the
// analytic roundings of planes, takes them). The top's two front edges
// rounded.
void check_case(const Case& c) {
  const double t = std::tan(c.tilt * kPi / 180.0);
  const gp_Pnt P0(8, 0, 10), P1(12, 0, 0), Q0(20, 0, 10), Q1(20, 0, 0);
  // A point's distance in the front plane from the crease's line.
  const auto off = [&](double x, double z) {
    const double dx = P1.X() - P0.X(), dz = P1.Z() - P0.Z();
    return std::abs((x - P0.X()) * dz - (z - P0.Z()) * dx) / std::sqrt(dx * dx + dz * dz);
  };
  const gp_Pnt R0(20, -off(20, 10) * t, 10), R1(20, -off(20, 0) * t, 0);
  BRepBuilderAPI_Sewing sewing;
  sewing.Add(polygon({P0, Q0, Q1, P1}));
  sewing.Add(polygon({P0, P1, R1, R0}));
  sewing.Add(polygon({P0, R0, Q0}));
  sewing.Add(polygon({P1, Q1, R1}));
  sewing.Add(polygon({Q0, R0, R1, Q1}));
  sewing.Perform();
  TopoDS_Solid wedge = BRepBuilderAPI_MakeSolid(TopoDS::Shell(sewing.SewedShape())).Solid();
  BRepLib::OrientClosedSolid(wedge);
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 10, 10)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(box, wedge).Shape(), true, true, false);
  unify.Build();
  const TopoDS_Shape body = ShapeCustom::ConvertToBSpline(unify.Shape(), true, true, true, true);
  CHECK(BRepCheck_Analyzer(body).IsValid());

  std::vector<TopoDS_Edge> front;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    BRepAdaptor_Curve e(TopoDS::Edge(ex.Current()));
    const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
    if (std::abs(a.Z() - 10) < 1e-9 && std::abs(b.Z() - 10) < 1e-9 && a.Y() < 1e-9 && b.Y() < 1e-9
        && std::max(a.X(), b.X()) > 1e-9 && std::min(a.X(), b.X()) < 20 - 1e-9) {
      front.push_back(TopoDS::Edge(ex.Current()));
    }
  }
  if (c.reversed) {
    std::reverse(front.begin(), front.end());
  }
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& e : front) {
    if (fillet.Contour(e) == 0) {
      fillet.Add(c.radius, e);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
  }
  std::printf("crease of %g degrees, radius %g, edges %s:", c.tilt, c.radius, c.reversed ? "reversed" : "as found");
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    std::printf(" not done\n");
    return;
  }
  const TopoDS_Shape rounded = fillet.Shape();
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  // No gap between the pieces closed by an edge's tolerance.
  double tolerance = 0.0;
  for (TopExp_Explorer ex(rounded, TopAbs_EDGE); ex.More(); ex.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(ex.Current())));
  }
  CHECK(tolerance < 1e-4);
  // The material taken: the two edges' sections along them (the top square
  // to the front before the crease, at the angle of the turned front
  // beyond it); the turn about the crease and the edges' kink at the
  // crease's top add a few thousandths.
  const gp_Vec nTop(0, 0, 1);
  const gp_Vec nFront = gp_Vec(P0, P1).Crossed(gp_Vec(P0, R0)).Normalized();
  const double theta = kPi - nTop.Angle(nFront);
  const double want = section_loss(kPi / 2, c.radius) * P0.Distance(gp_Pnt(0, 0, 10))
                      + section_loss(theta, c.radius) * P0.Distance(R0);
  const double loss = volume_of(body) - volume_of(rounded);
  std::printf(" valid, largest tolerance %.2g, material taken %.6f, %.6f wanted\n", tolerance, loss, want);
  CHECK_NEAR_TOL(loss, want, 5e-3);
}

void test_rounding_over_a_concave_crease_of_its_side() {
  for (const double tilt : {3.0, 5.0}) {
    for (const double radius : {1.0, 0.5}) {
      for (const bool reversed : {false, true}) {
        check_case({tilt, radius, reversed});
      }
    }
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_concave_crease) {
  test::guarded("test_rounding_over_a_concave_crease_of_its_side",
                test::test_rounding_over_a_concave_crease_of_its_side);
  if (test::failures == 0) {
    std::puts("test_fillet_concave_crease: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
