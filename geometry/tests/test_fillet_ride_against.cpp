// SPDX-License-Identifier: MIT
// Both rims of a disc thinner than the two roundings together, built with
// OCCT's fillet alone, where the rims' circles turn opposite ways round the
// disc's axis. No ball touches both flat faces, so the rounding computed
// second rolls on the first one's contact line on the band (port patch
// 0016) and the two blends meet on that line. That line ran the second
// blend's way only where both circles turn the same way; the other way the
// second blend's face came out reversed with a wire not closed. A ring of a
// stored design whose two rims start at one point and run opposite ways is
// the case behind the patch 0930 of the OCCT port (third_party/vcpkg-ports,
// mitcad#343).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

constexpr double kRadius = 20.0; // the disc's radius
constexpr double kHeight = 2.5;  // its thickness, from -kHeight / 2 to kHeight / 2
constexpr double kRound = 1.5;   // the rims' rounding, more than half the thickness

// The disc as two halves, each a cylinder standing on the plane z = 0, the
// lower one upside down, joined: each rim's circle turns about its own
// cylinder's axis, the lower one's -z, and both start at (kRadius, 0).
TopoDS_Shape disc_with_rims_turning_opposite_ways() {
  const TopoDS_Shape upper =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), kRadius, 0.5 * kHeight)
      .Shape();
  const TopoDS_Shape lower =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, -1), gp_Dir(1, 0, 0)), kRadius, 0.5 * kHeight)
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(upper, lower).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The rim circle at height z.
TopoDS_Edge rim_at(const TopoDS_Shape& body, double z) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - kRadius) < 1e-9 &&
        std::abs(curve.Circle().Location().Z() - z) < 1e-9) {
      return TopoDS::Edge(edges(i));
    }
  }
  return TopoDS_Edge();
}

// Which way the edge's curve turns about +z (its circle's axis; a rounding
// runs along the edge's curve).
double turn_of(const TopoDS_Edge& edge) { return BRepAdaptor_Curve(edge).Circle().Axis().Direction().Z(); }

// The disc rounded on both rims, the rim listed first whole: its section
// from the axis, the first rim's quarter circle up to the band at kRound
// from that rim, then the arc of the ball touching the other flat face that
// passes through that point.
double rounded_disc_volume(bool lower_first) {
  const double h = 0.5 * kHeight, r = kRound;
  // The ball of the riding rounding: kRound from the flat face, its contact
  // with the line on the band at distance kRound.
  const double d = kHeight - r;                      // the line's distance from the other face
  const double c = std::sqrt(r * r - (r - d) * (r - d)); // the ball's centre from the band
  const double s = lower_first ? 1.0 : -1.0;         // the first rim's side, -z for the lower one
  const double z0 = -s * h;                          // the first rim's face
  const double z1 = s * h;                           // the other face
  const gp_Pnt a(0, 0, z0), b(kRadius - r, 0, z0), line(kRadius, 0, z0 + s * r), e(kRadius - c, 0, z1),
    f(0, 0, z1);
  const gp_Pnt mid1(kRadius - r + r * std::sin(kPi / 4), 0, z0 + s * (r - r * std::cos(kPi / 4)));
  // The riding ball's centre and a point between the line and e on its arc.
  const gp_Pnt centre(kRadius - c, 0, z1 - s * r);
  const gp_Vec v1(centre, line), v2(centre, e);
  gp_Vec vm = v1.Normalized() + v2.Normalized();
  vm.Normalize();
  const gp_Pnt mid2 = centre.Translated(vm * r);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(a, b).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(b, mid1, line).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(line, mid2, e).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(e, f).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(f, a).Edge());
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return std::abs(volume_of(BRepPrimAPI_MakeRevol(section, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape()));
}

void round_both_rims(bool lower_first) {
  const TopoDS_Shape body = disc_with_rims_turning_opposite_ways();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const TopoDS_Edge lower = rim_at(body, -0.5 * kHeight), upper = rim_at(body, 0.5 * kHeight);
  CHECK(!lower.IsNull() && !upper.IsNull());
  if (lower.IsNull() || upper.IsNull()) {
    return;
  }
  // (The case: the two circles turn opposite ways.)
  CHECK(turn_of(lower) * turn_of(upper) < 0.0);
  TopoDS_Shape rounded;
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(kRound, lower_first ? lower : upper);
    fillet.Add(kRound, lower_first ? upper : lower);
    fillet.Build();
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      return;
    }
    rounded = fillet.Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    CHECK(false);
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  CHECK_NEAR_TOL(volume_of(rounded), rounded_disc_volume(lower_first), 1e-6);
}

void test_rims_turning_opposite_ways_lower_first() { round_both_rims(true); }

void test_rims_turning_opposite_ways_upper_first() { round_both_rims(false); }

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_ride_against) {
  test::guarded("test_rims_turning_opposite_ways_lower_first", test::test_rims_turning_opposite_ways_lower_first);
  test::guarded("test_rims_turning_opposite_ways_upper_first", test::test_rims_turning_opposite_ways_upper_first);
  if (test::failures == 0) {
    std::puts("test_fillet_ride_against: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
