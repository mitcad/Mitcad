// SPDX-License-Identifier: MIT
// Port patch 1000: a chamfer whose chain runs across a slight concave
// crease of its other face, over a narrow face, and the faces beyond that
// narrow face grown past their B-spline surfaces' bounds. Three changes:
// 0980's crease turn is a rounding's only (a chamfer's contact passes the
// crease); 0671's ChFi3d_KPartCutChainToEnd looks only at faces of the
// spine's edge (with a narrow face taken unbounded, ChamferOverNarrowFace,
// it threw in BRepTools::OriEdgeInFace); faces grown past their surfaces'
// bounds get those surfaces continued (ChFi3d_ContinueGrownFaces).
//
// A synthetic fin 1 thick (x from -0.5 to 0.5, y from -4 to 4) on a plate
// whose top is a B-spline surface (so that the fin's root chamfer is a
// walked B-spline, degree 1 across). The fin's tip, extruded along x, is a
// circular arc (radius 10, from y = -4 to 0, 3.4 high at its top, 3.198 at
// its ends: a cylinder) and a cubic B-spline (from y = 0 to 4, dipping to
// 2.69 at y = 2.14, 3.3 high and flat at y = 4); at y = 0 the spline starts
// 2 degrees steeper upwards than the arc ends: a concave crease of the tip
// across the fin. The tip's edges with the sides, an arc and a spline each,
// are one chain on each side (the tip's faces within 0.1 rad of tangent
// across the crease).
//
// The root's two edges chamfered by 2.6, then both top chains by 0.263:
// the strip of each side between the root's chamfer and the tip is 0.6 high
// along the arc and narrower than 0.263 along the spline from y = 1.39 to
// 2.72 (0.09 at its narrowest). Without the patch the chamfer throws (the
// crease turn, then OriEdgeInFace). With it: built, valid, one solid, every
// B-spline face within its surface's parameter bounds (the root's chamfers
// grow up to the tip's chamfers where the strips are narrow).
//
// The boundary: the root chamfered by 2.0, every strip wider than 0.263
// (0.69 at its narrowest): only the crease's change applies, the chamfer
// walks across the crease as a plain chamfer: built, valid, every B-spline
// face within its bounds, the material taken that of the chamfer's
// triangles swept along the two chains (within 0.5 %: the arc's end plane
// is not square to it).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <ShapeCustom.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kArcRadius = 10.0;
constexpr double kArcTop    = 3.4;   // the arc's highest point, at y = -2
constexpr double kSplineEnd = 3.3;   // the spline's height at y = 4
constexpr double kTipChamfer = 0.263;

// The arc's height at its ends (y = -4 and 0).
double arc_end() { return kArcTop - kArcRadius + std::sqrt(kArcRadius * kArcRadius - 4.0); }

// The tip's arc in the plane x = x0, from y = -4 to 0.
occ::handle<Geom_TrimmedCurve> tip_arc(double x0) {
  return GC_MakeArcOfCircle(gp_Pnt(x0, -4, arc_end()), gp_Pnt(x0, -2, kArcTop), gp_Pnt(x0, 0, arc_end())).Value();
}

// The tip's spline in the plane x = x0, from y = 0 to 4: its start tangent
// the arc's end tangent turned 2 degrees upwards (a concave crease).
occ::handle<Geom_BSplineCurve> tip_spline(double x0) {
  const double arc_slope = std::atan(-2.0 / std::sqrt(kArcRadius * kArcRadius - 4.0));
  const double slope     = std::tan(arc_slope + 2.0 * test::kPi / 180.0);
  NCollection_Array1<gp_Pnt> poles(1, 6);
  poles(1) = gp_Pnt(x0, 0, arc_end());
  poles(2) = gp_Pnt(x0, 1, arc_end() + slope);
  poles(3) = gp_Pnt(x0, 1.8, 2.66);
  poles(4) = gp_Pnt(x0, 2.6, 2.66);
  poles(5) = gp_Pnt(x0, 3.3, kSplineEnd);
  poles(6) = gp_Pnt(x0, 4, kSplineEnd);
  NCollection_Array1<double> knots(1, 4);
  NCollection_Array1<int>    mults(1, 4);
  for (int i = 1; i <= 4; ++i) {
    knots(i) = i - 1;
    mults(i) = i == 1 || i == 4 ? 4 : 1;
  }
  return new Geom_BSplineCurve(poles, knots, mults, 3);
}

// The fin (its foot 1 inside the plate) on the plate, the plate's faces
// B-spline surfaces.
TopoDS_Shape fin_on_plate() {
  const double            x0 = -0.5;
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, -4, -1), gp_Pnt(x0, 4, -1)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, 4, -1), gp_Pnt(x0, 4, kSplineEnd)).Edge());
  wire.Add(TopoDS::Edge(BRepBuilderAPI_MakeEdge(tip_spline(x0)).Edge().Reversed()));
  wire.Add(TopoDS::Edge(BRepBuilderAPI_MakeEdge(tip_arc(x0)).Edge().Reversed()));
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, -4, arc_end()), gp_Pnt(x0, -4, -1)).Edge());
  const TopoDS_Shape fin =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(1, 0, 0)).Shape();
  const TopoDS_Shape plate = ShapeCustom::ConvertToBSpline(
    BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -5), gp_Pnt(10, 10, 0)).Shape(), true, true, true, true);
  return BRepAlgoAPI_Fuse(plate, fin).Shape();
}

// The edges chamfered with an equal distance d, each chain once.
TopoDS_Shape chamfered_chains(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double d) {
  BRepFilletAPI_MakeChamfer chamfer(body);
  for (const TopoDS_Edge& edge : edges) {
    if (chamfer.Contour(edge) == 0) {
      chamfer.Add(d, edge);
    }
  }
  return port::built(chamfer, "chamfer");
}

// Whether every face on a B-spline surface lies within that surface's
// parameter bounds (to 1e-4 of their ranges).
bool faces_within_their_surfaces(const TopoDS_Shape& shape) {
  bool within = true;
  for (TopExp_Explorer fx(shape, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face&                     face = TopoDS::Face(fx.Current());
    TopLoc_Location                        loc;
    const occ::handle<Geom_BSplineSurface> surface =
      occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(face, loc));
    if (surface.IsNull()) {
      continue;
    }
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0, su0 = 0, su1 = 0, sv0 = 0, sv1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    surface->Bounds(su0, su1, sv0, sv1);
    const double mu = 1e-4 * (su1 - su0), mv = 1e-4 * (sv1 - sv0);
    if (u0 < su0 - mu || u1 > su1 + mu || v0 < sv0 - mv || v1 > sv1 + mv) {
      std::fprintf(stderr, "p1000: a face over [%g, %g] x [%g, %g] on a surface over [%g, %g] x [%g, %g]\n", u0, u1,
                   v0, v1, su0, su1, sv0, sv1);
      within = false;
    }
  }
  return within;
}

// The fin's root chamfered by `root`, then both top chains by 0.263; a null
// shape where a step fails. `rooted` gets the body after the root's chamfer.
TopoDS_Shape fin_chamfered(double root, TopoDS_Shape& rooted) {
  const TopoDS_Shape body = fin_on_plate();
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> foot = port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(std::abs(p.X()) - 0.5) < 1e-5 && std::abs(p.Z()) < 1e-5 && std::abs(p.Y()) < 3.9;
  });
  CHECK(foot.size() == 2);
  if (!port::valid(body) || foot.size() != 2) {
    return TopoDS_Shape();
  }
  rooted = chamfered_chains(body, foot, root);
  CHECK(port::valid(rooted));
  if (!port::valid(rooted)) {
    return TopoDS_Shape();
  }
  // The tip's edges with the sides: an arc and a spline on each side.
  const std::vector<TopoDS_Edge> tip = port::edges_where(rooted, [root](const gp_Pnt& p) {
    return std::abs(std::abs(p.X()) - 0.5) < 1e-6 && p.Z() > root + 0.05 && std::abs(p.Y()) < 3.99;
  });
  CHECK(tip.size() == 4);
  if (tip.size() != 4) {
    return TopoDS_Shape();
  }
  return chamfered_chains(rooted, tip, kTipChamfer);
}

// The material the tip's chamfer of distance d takes along a curve of the
// tip's edge at x = 0.5: its sections square to the edge, the triangle of
// legs d along -x (on the tip) and d in the side, swept along the edge with
// its curvature (Pappus).
double swept_triangles(const occ::handle<Geom_Curve>& curve, double d) {
  const double a0 = curve->FirstParameter(), a1 = curve->LastParameter();
  const int    n  = 4000;
  double       sum = 0.0;
  for (int i = 0; i <= n; ++i) {
    gp_Pnt e;
    gp_Vec d1, d2;
    curve->D2(a0 + (a1 - a0) * i / n, e, d1, d2);
    const gp_Vec t = d1.Normalized();
    gp_Vec       m = gp_Vec(1, 0, 0).Crossed(t);
    if (m.Z() > 0) {
      m.Reverse();
    }
    const gp_Vec curvature = (d2 - t * d2.Dot(t)) / d1.SquareMagnitude();
    const double factor    = 1.0 - curvature.Dot(m) * d / 3.0;
    const double w         = (i == 0 || i == n) ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0);
    sum += w * 0.5 * d * d * factor * d1.Magnitude();
  }
  return sum * (a1 - a0) / n / 3.0;
}

// The root chamfered by 2.6: the strips narrower than the tip's chamfer
// along the spline; both top chains chamfered across the crease.
void test_chamfer_across_a_concave_crease_over_narrow_strips() {
  TopoDS_Shape       rooted;
  const TopoDS_Shape result = fin_chamfered(2.6, rooted);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  CHECK(faces_within_their_surfaces(result));
}

// The boundary: the root chamfered by 2.0, every strip wider than the
// tip's chamfer: the chamfer walks across the crease as usual.
void test_chamfer_across_a_concave_crease_over_wide_strips() {
  TopoDS_Shape       rooted;
  const TopoDS_Shape result = fin_chamfered(2.0, rooted);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  CHECK(faces_within_their_surfaces(result));
  const occ::handle<Geom_Curve> arc(tip_arc(0.5));
  const occ::handle<Geom_Curve> spline(tip_spline(0.5));
  const double                  taken  = 2.0 * (swept_triangles(arc, kTipChamfer) + swept_triangles(spline, kTipChamfer));
  CHECK_NEAR_TOL(port::volume_of(rooted) - port::volume_of(result), taken, 5e-3);
}

} // namespace

MITCAD_PORT_TEST("1000", test_chamfer_across_a_concave_crease_over_narrow_strips);
MITCAD_PORT_TEST("1000", test_chamfer_across_a_concave_crease_over_wide_strips);
