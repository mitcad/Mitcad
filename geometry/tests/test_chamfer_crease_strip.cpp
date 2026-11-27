// SPDX-License-Identifier: MIT
// A chamfer along a fin's tip edges where the tip's faces meet at a slight
// concave crease, over a side strip narrower than the chamfer, built with
// OCCT's chamfer alone. The fin is 1 thick (x in [-0.5, 0.5]) on a plate;
// its tip is a line over y in [-4, 0] and then a cubic (a profile's
// spline) whose start turns up by a few degrees from the line, so that the
// tip's two faces meet in a valley that the fillet takes for tangent. The
// tip's edges on the side x = 0.5 are chamfered 0.4 at 45 degrees. OCCT
// took the chamfer's piece after the line's analytic one for a blend
// rolling on the crease's edge, which chamfers do not have, and failed.
// With the foot of that side chamfered so high that the side strip is
// narrower than the chamfer along the cubic, bent down (patch 0531's case),
// the chamfer computed again over the strip's surface made an invalid
// shape at the line's analytic piece, and with the foot's face a B-spline
// that face, grown up to the chamfer, ran past its surface's bounds. A
// fin's top edges chamfered over slight creases of its top in a stored
// design are the case behind the patch 1000 of the OCCT port
// (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>
#include <tuple>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Map.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kChamfer = 0.4; // the tip's chamfer, both legs (45 degrees)
constexpr double kFootOut = 2.0; // the foot's chamfer along the plate

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The fin's tip: a line from y = -4 (z = z0) to y = 0 (z = z1), then a
// cubic over y in [0, 4] whose start turns up by `kink` degrees from the
// line, straight, or bent down by `bend` (its last two poles lowered by
// 0.15 and 0.4 times that).
struct Tip {
  double z0 = 3.0;
  double z1 = 2.6;
  double kink = 2.0;
  double bend = 0.0;
};

gp_Pnt tip_start(const Tip& tip, double x) { return gp_Pnt(x, -4.0, tip.z0); }
gp_Pnt tip_kink(const Tip& tip, double x) { return gp_Pnt(x, 0.0, tip.z1); }

occ::handle<Geom_BSplineCurve> tip_cubic(const Tip& tip, double x) {
  const double slope = std::atan((tip.z1 - tip.z0) / 4.0) + tip.kink * kPi / 180.0;
  const gp_Pnt a = tip_kink(tip, x);
  const gp_Vec t(0.0, 1.0, std::tan(slope));
  NCollection_Array1<gp_Pnt> poles(1, 4);
  poles(1) = a;
  poles(2) = a.Translated(t * (4.0 / 3.0));
  poles(3) = a.Translated(t * (8.0 / 3.0) + gp_Vec(0.0, 0.0, -0.15 * tip.bend));
  poles(4) = a.Translated(t * 4.0 + gp_Vec(0.0, 0.0, -0.4 * tip.bend));
  NCollection_Array1<double> knots(1, 2);
  NCollection_Array1<int>    mults(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  mults(1) = mults(2) = 4;
  return new Geom_BSplineCurve(poles, knots, mults, 3);
}

// The fin on the plate z <= 0: its profile in x = -0.5 extruded by 1.
TopoDS_Shape fin_on_plate(const Tip& tip) {
  const double x = -0.5;
  const occ::handle<Geom_BSplineCurve> cubic = tip_cubic(tip, x);
  const gp_Pnt P0 = tip_start(tip, x), P1 = tip_kink(tip, x), P2 = cubic->EndPoint();
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(P0, P1).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(cubic).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(P2, gp_Pnt(x, P2.Y(), -1.0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x, P2.Y(), -1.0), gp_Pnt(x, P0.Y(), -1.0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x, P0.Y(), -1.0), P0).Edge());
  const TopoDS_Shape fin =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(1, 0, 0)).Shape();
  return BRepAlgoAPI_Fuse(BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -5), gp_Pnt(10, 10, 0)).Shape(), fin).Shape();
}

TopoDS_Face side_of(const TopoDS_Shape& body) {
  TopoDS_Face side;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
    if (s.GetType() == GeomAbs_Plane && std::abs(std::abs(s.Plane().Axis().Direction().X()) - 1) < 1e-9 &&
        std::abs(s.Plane().Location().X() - 0.5) < 1e-9) {
      side = TopoDS::Face(ex.Current());
    }
  }
  return side;
}

// The foot of the side x = 0.5 chamfered `height` up the side and kFootOut
// along the plate (adding material); with `spline`, the foot's face then
// made a B-spline of degree 1 on its own bounds, its parameters and its
// edges' curves on it kept.
TopoDS_Shape with_foot(const TopoDS_Shape& body, double height, bool spline) {
  const TopoDS_Face side = side_of(body);
  BRepFilletAPI_MakeChamfer foot(body);
  for (TopExp_Explorer ex(side, TopAbs_EDGE); ex.More(); ex.Next()) {
    BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
    if (std::abs(a.Z()) < 1e-9 && std::abs(b.Z()) < 1e-9) {
      foot.Add(height, kFootOut, TopoDS::Edge(ex.Current()), side);
    }
  }
  foot.Build();
  CHECK(foot.IsDone());
  if (!foot.IsDone()) {
    return TopoDS_Shape();
  }
  const TopoDS_Shape out = foot.Shape();
  if (!spline) {
    return out;
  }
  BRep_Builder builder;
  int made = 0;
  for (TopExp_Explorer ex(out, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face f = TopoDS::Face(ex.Current());
    BRepAdaptor_Surface s(f);
    if (s.GetType() != GeomAbs_Plane || std::abs(s.Plane().Axis().Direction().X()) < 0.1 ||
        std::abs(s.Plane().Axis().Direction().Z()) < 0.1) {
      continue;
    }
    double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(f, u0, u1, v0, v1);
    TopLoc_Location location;
    const occ::handle<Geom_Surface> plane = BRep_Tool::Surface(f, location);
    const occ::handle<Geom_BSplineSurface> bspline =
        GeomConvert::SurfaceToBSplineSurface(new Geom_RectangularTrimmedSurface(plane, u0, u1, v0, v1));
    // (An edge's curve on a face is stored with the face's surface: each is
    // stored again with the B-spline.)
    std::vector<std::tuple<TopoDS_Edge, occ::handle<Geom2d_Curve>, double, double>> curves;
    for (TopExp_Explorer exE(f, TopAbs_EDGE); exE.More(); exE.Next()) {
      TopoDS_Edge e = TopoDS::Edge(exE.Current());
      double first = 0.0, last = 0.0;
      const occ::handle<Geom2d_Curve> c = BRep_Tool::CurveOnSurface(e, f, first, last);
      e.Orientation(TopAbs_FORWARD);
      curves.emplace_back(e, c, first, last);
    }
    builder.UpdateFace(f, bspline, location, BRep_Tool::Tolerance(f));
    for (const auto& [e, c, first, last] : curves) {
      builder.UpdateEdge(e, c, f, BRep_Tool::Tolerance(e));
      builder.Range(e, f, first, last);
    }
    ++made;
  }
  CHECK(made == 1);
  CHECK(BRepCheck_Analyzer(out).IsValid());
  return out;
}

// The tip's edges on the side x = 0.5 chamfered by kChamfer at 45 degrees
// (measured on the side), the contour running on across their vertex.
TopoDS_Shape chamfered_tip(const TopoDS_Shape& body) {
  const TopoDS_Face side = side_of(body);
  std::vector<TopoDS_Edge> tip;
  for (TopExp_Explorer ex(side, TopAbs_EDGE); ex.More(); ex.Next()) {
    BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
    if (a.Z() > 2.31 && b.Z() > 2.31) { // (the foot's chamfer reaches 2.3)
      tip.push_back(TopoDS::Edge(ex.Current()));
    }
  }
  CHECK(tip.size() == 2u);
  BRepFilletAPI_MakeChamfer maker(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> through;
  for (const TopoDS_Edge& e : tip) {
    through.Add(e);
  }
  maker.StopChainsAtTurns(through);
  for (const TopoDS_Edge& e : tip) {
    maker.AddDA(kChamfer, kPi / 4.0, e, side);
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "chamfer crease test: %s\n", failure.what());
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// Whether every face's parameters lie inside its surface's bounds.
bool faces_inside_their_surfaces(const TopoDS_Shape& shape) {
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& f = TopoDS::Face(ex.Current());
    double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0, s0 = 0.0, s1 = 0.0, t0 = 0.0, t1 = 0.0;
    BRepTools::UVBounds(f, u0, u1, v0, v1);
    BRep_Tool::Surface(f)->Bounds(s0, s1, t0, t1);
    const double eps = 1e-9;
    if (u0 < s0 - eps || u1 > s1 + eps || v0 < t0 - eps || v1 > t1 + eps) {
      std::fprintf(stderr, "a face runs past its surface's bounds: [%g, %g] x [%g, %g] in [%g, %g] x [%g, %g]\n",
                   u0, u1, v0, v1, s0, s1, t0, t1);
      return false;
    }
  }
  return true;
}


// The tip's edges chamfered across the valley, the side whole (no foot
// chamfer): the line's analytic piece, then the straight cubic's walked one.
// The material taken away is the section's triangle along both.
void test_chamfer_across_a_slight_valley_of_the_tip(double kink) {
  Tip tip;
  tip.kink = kink;
  const TopoDS_Shape body = fin_on_plate(tip);
  const TopoDS_Shape result = chamfered_tip(body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const double length =
      tip_start(tip, 0.5).Distance(tip_kink(tip, 0.5)) + tip_kink(tip, 0.5).Distance(tip_cubic(tip, 0.5)->EndPoint());
  // (Where the pieces meet, their sections differ by the kink: the two
  // wedges' overlap and gap there are kink * kChamfer^3 / 6 each, 4e-4 mm3
  // at two degrees; the pieces meet half way.)
  CHECK_NEAR_TOL(volume_of(body) - volume_of(result), 0.5 * kChamfer * kChamfer * length, 1e-4);
}

// The same chain (the line level, the cubic bent down) over a side strip
// narrower than the chamfer along the cubic (the foot chamfered 2.3 up the
// side): the foot's face grows up to the chamfer and, a B-spline, keeps
// inside its surface's bounds; the result is the planar foot's.
void test_chamfer_across_a_valley_over_a_narrow_strip(double bend) {
  Tip tip;
  tip.z1   = tip.z0;
  tip.bend = bend;
  double removed[2] = {0.0, 0.0};
  for (const bool spline : {false, true}) {
    const TopoDS_Shape body = with_foot(fin_on_plate(tip), 2.3, spline);
    if (body.IsNull()) {
      return;
    }
    const TopoDS_Shape result = chamfered_tip(body);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    CHECK(faces_inside_their_surfaces(result));
    removed[spline ? 1 : 0] = volume_of(body) - volume_of(result);
  }
  CHECK_NEAR_TOL(removed[1], removed[0], 1e-7);
  // (Less than the triangle along both, the foot's face taking its part.)
  CHECK(removed[0] > 0.0 && removed[0] < 0.5 * kChamfer * kChamfer * 8.1);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(chamfer_crease_strip) {
  for (const double kink : {2.0, 4.0}) {
    test::guarded("test_chamfer_across_a_slight_valley_of_the_tip",
                  [&] { test::test_chamfer_across_a_slight_valley_of_the_tip(kink); });
  }
  for (const double bend : {1.5, 1.7}) {
    test::guarded("test_chamfer_across_a_valley_over_a_narrow_strip",
                  [&] { test::test_chamfer_across_a_valley_over_a_narrow_strip(bend); });
  }
  if (test::failures == 0) {
    std::puts("test_chamfer_crease_strip: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
