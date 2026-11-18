// SPDX-License-Identifier: MIT
// Port patch 0503: two analytic roundings meeting at a vertex of four sharp
// edges are joined along their intersection (PerformTwoCornerSameExt) where
// their ends are one point. Each end is computed on its own face's curve of
// the edge it lies on, so on an edge whose two curves differ within its
// tolerance the ends lie that far apart; the patch takes them for one point
// within twice the distance between the edge's curves on its two faces there
// (and Precision::Confusion() more), where stock OCCT took 1e-7 and left a
// plate of no width between them.
//
// The case: a slot through a round boss, its rim chamfered with 1 and the
// chamfer's lower edges rounded with 1. At each of the four corners the
// wall's rounding (a cylinder) meets the boss's (a torus), both ending on
// the crease between the chamfer's plane and cone. Inside: the crease's
// curve on the cone moved 2e-7 off its curve on the plane, within a
// tolerance of 1e-4: no plate, the edges' tolerances kept. Outside: the
// curves exact and the boss's rounding 1e-6 larger, its ends about 4e-7
// from the wall's (beyond the curves' spread, nought): the plate as before.

#include <algorithm>
#include <cmath>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec2d.hxx>

#include "port_test.hpp"

namespace {

constexpr double kHeight = 10.0;

// A slot 3 wide through a round boss (radius 6, 10 high), its top rim
// chamfered with 1. With `spread`, a copy whose plane-cone creases have
// their curve on the cone turned about the boss's axis so that it lies
// `spread` off the curve on the plane at the boss's radius 5.7 (where the
// roundings end), the edges' tolerance 1e-4.
TopoDS_Shape chamfered_slot_through_a_boss(double spread) {
  const TopoDS_Shape boss =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ(), gp::DX()), 6, kHeight).Shape();
  const TopoDS_Shape bar = BRepPrimAPI_MakeBox(gp_Pnt(-1.5, -15, 0), 3, 30, kHeight).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(boss, bar).Shape(), true, true, true);
  unify.Build();
  const TopoDS_Shape        body = unify.Shape();
  BRepFilletAPI_MakeChamfer chamfer(body);
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face&        face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface s(face);
    if (s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - kHeight) < 1e-9
        && s.Plane().Axis().Direction().IsParallel(gp::DZ(), 1e-12)) {
      for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
        chamfer.Add(1, 1, TopoDS::Edge(e.Current()), face);
      }
    }
  }
  const TopoDS_Shape chamfered = port::built(chamfer, "chamfer");
  CHECK(!chamfered.IsNull());
  if (chamfered.IsNull() || spread == 0.0) {
    return chamfered;
  }
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(chamfered).Shape();
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(copy, TopAbs_EDGE, TopAbs_FACE, faces);
  BRep_Builder builder;
  int          moved = 0;
  for (int i = 1; i <= faces.Extent(); ++i) {
    if (faces(i).Extent() != 2) {
      continue;
    }
    const TopoDS_Face&        a  = TopoDS::Face(faces(i).First());
    const TopoDS_Face&        b  = TopoDS::Face(faces(i).Last());
    const GeomAbs_SurfaceType ta = BRepAdaptor_Surface(a).GetType();
    const GeomAbs_SurfaceType tb = BRepAdaptor_Surface(b).GetType();
    if (!((ta == GeomAbs_Plane && tb == GeomAbs_Cone) || (ta == GeomAbs_Cone && tb == GeomAbs_Plane))) {
      continue;
    }
    const TopoDS_Edge&        crease = TopoDS::Edge(faces.FindKey(i));
    const TopoDS_Face&        cone   = ta == GeomAbs_Cone ? a : b;
    double                    first = 0.0, last = 0.0;
    occ::handle<Geom2d_Curve> curve =
      occ::down_cast<Geom2d_Curve>(BRep_Tool::CurveOnSurface(crease, cone, first, last)->Copy());
    // The cone's first parameter is the angle about the boss's axis.
    curve->Translate(gp_Vec2d(spread / 5.7, 0));
    builder.UpdateEdge(crease, curve, cone, 1e-4);
    ++moved;
  }
  CHECK(moved >= 4);
  CHECK(port::valid(copy));
  return copy;
}

// Rounds the chamfer's lower edges (at height 9): those along the slot's
// walls with `wall`, those round the boss with `boss`.
TopoDS_Shape round_chamfers_lower_edges(const TopoDS_Shape& body, double wall, double boss) {
  BRepFilletAPI_MakeFillet rounding(body);
  int                      edges = 0;
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&      edge = TopoDS::Edge(e.Current());
    const BRepAdaptor_Curve c(edge);
    const double            t0 = c.FirstParameter(), t1 = c.LastParameter();
    if (std::abs(c.Value(t0).Z() - 9) < 1e-9 && std::abs(c.Value(t1).Z() - 9) < 1e-9
        && std::abs(c.Value(0.5 * (t0 + t1)).Z() - 9) < 1e-9 && rounding.Contour(edge) == 0) {
      rounding.Add(c.GetType() == GeomAbs_Line ? wall : boss, edge);
      ++edges;
    }
  }
  CHECK(edges == 8);
  return port::built(rounding, "rounding");
}

// The faces of a corner's plate (B-spline or Bezier surfaces): none where
// the roundings are joined along their intersection.
int plates(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const GeomAbs_SurfaceType t = BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType();
    n += t == GeomAbs_BSplineSurface || t == GeomAbs_BezierSurface ? 1 : 0;
  }
  return n;
}

double max_edge_tolerance(const TopoDS_Shape& shape) {
  double tolerance = 0.0;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
  }
  return tolerance;
}

// Inside: the curves 2e-7 apart, the ends about as far (stock's limit
// 1e-7), within twice that: joined along the intersection, no plate, the
// edges within the crease's tolerance, the same material as the exact body.
void test_corner_ends_within_twice_the_curves_spread_are_joined() {
  const TopoDS_Shape exact_body = chamfered_slot_through_a_boss(0.0);
  const TopoDS_Shape exact      = round_chamfers_lower_edges(exact_body, 1.0, 1.0);
  const TopoDS_Shape body       = chamfered_slot_through_a_boss(2e-7);
  const TopoDS_Shape rounded    = round_chamfers_lower_edges(body, 1.0, 1.0);
  CHECK(!exact.IsNull());
  CHECK(!rounded.IsNull());
  if (exact.IsNull() || rounded.IsNull()) {
    return;
  }
  const int    n         = plates(rounded);
  const double tolerance = max_edge_tolerance(rounded);
  if (n != 0 || tolerance > 1e-4) {
    std::fprintf(stderr, "corner ends 2e-7 apart: %d plates, edge tolerance up to %g\n", n, tolerance);
  }
  CHECK(n == 0);
  CHECK(tolerance <= 1e-4);
  CHECK(port::valid(rounded));
  const double change = port::volume_of(rounded) - port::volume_of(body);
  const double want   = port::volume_of(exact) - port::volume_of(exact_body);
  CHECK(std::abs(change - want) < 1e-6);
}

// Outside: the curves exact (their spread nought, the limit 1e-7) and the
// boss's rounding 1e-6 larger than the wall's, so that its ends lie about
// 4e-7 from the wall's on both creases: not one point, the corner the plate
// it was before the patch (or no result), never the joined intersection.
void test_corner_ends_beyond_the_curves_spread_are_not_joined() {
  const TopoDS_Shape body    = chamfered_slot_through_a_boss(0.0);
  const TopoDS_Shape rounded = round_chamfers_lower_edges(body, 1.0, 1.0 + 1e-6);
  const int          n       = rounded.IsNull() ? -1 : plates(rounded);
  if (n == 0) {
    std::fprintf(stderr, "corner ends 4e-7 apart on exact curves: joined, no plate\n");
  }
  CHECK(rounded.IsNull() || n > 0);
}

} // namespace

MITCAD_PORT_TEST("0503", test_corner_ends_within_twice_the_curves_spread_are_joined);
MITCAD_PORT_TEST("0503", test_corner_ends_beyond_the_curves_spread_are_not_joined);
