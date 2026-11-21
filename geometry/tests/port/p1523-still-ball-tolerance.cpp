// SPDX-License-Identifier: MIT
// Port patch 1523: a ball that stays put between tubes of its radius within
// a tolerance is walked as one between exact tubes. 1520 takes a ball for
// staying put where its supports are tubes of its radius (the centre seen
// from the contact moving at less than a thousandth of the contact's
// speed), and the walk's tolerance (1e-4) holds the centres seen from the
// two contacts together. A stored rounding approximated by a B-spline is a
// tube only within a tolerance, its sections' curvature rippling: there the
// rim's rounding through the mitre failed.
//
// Half a lid: a cylinder of radius 23 from z = 28 to the top z = 30, cut by
// the plane y = 0, the edges of its flat side (the two vertical ones and
// the one along the top) rounded with 1. The rounding along the top (a
// cylinder about y = 1, z = 29) is then replaced by a B-spline tube fitted
// through its points with the section's radius rippling by
// amp sin(k t) (t the angle round the section; nought at both contacts).
// The top rim rounded with 1 runs through the mitres of the vertical
// roundings and that tube, where its ball stays put on both roundings'
// axes. With the ripple 6e-6 (k = 4) and 1e-5 (k = 2 and 6): built, valid,
// the rounding taking off what it takes off on the unrippled B-spline tube
// (within 1e-5). The boundary: the tube fitted without a ripple, built as
// before.

#include <algorithm>
#include <cmath>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools_ReShape.hxx>
#include <GeomAPI_PointsToBSplineSurface.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array2.hxx>
#include <ShapeFix_Shape.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 1.0;  // the roundings' radius
constexpr double kRim    = 23.0; // the lid's radius
constexpr double kTop    = 30.0; // its top
constexpr double kLow    = 28.0; // its bottom

// The half lid with its flat side's edges rounded (but the lowest one).
TopoDS_Shape half_lid() {
  TopoDS_Shape body =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kLow), gp_Dir(0, 0, 1)), kRim, kTop - kLow).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, -10), gp_Pnt(30, 0, 40)).Shape()).Shape();
  BRepFilletAPI_MakeFillet flat(body);
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt            a = c.Value(c.FirstParameter());
    const gp_Pnt            b = c.Value(c.LastParameter());
    const gp_Pnt            m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(a.Y()) < 1e-7 && std::abs(b.Y()) < 1e-7 && std::abs(m.Y()) < 1e-7 &&
        !(std::abs(a.Z() - kLow) < 1e-7 && std::abs(b.Z() - kLow) < 1e-7) && flat.Contour(TopoDS::Edge(ex.Current())) == 0) {
      flat.Add(kRadius, TopoDS::Edge(ex.Current()));
    }
  }
  return port::built(flat, "flat side");
}

// The body with the rounding along the top (the cylinder along x) replaced
// by a fitted B-spline tube whose section's radius ripples by amp sin(k t).
TopoDS_Shape with_rippled_tube(const TopoDS_Shape& body, double amp, int k) {
  TopoDS_Face target;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(f.Current()));
    if (s.GetType() == GeomAbs_Cylinder && s.Cylinder().Axis().Direction().IsParallel(gp_Dir(1, 0, 0), 1e-9)) {
      target = TopoDS::Face(f.Current());
    }
  }
  CHECK(!target.IsNull());
  if (target.IsNull()) {
    return TopoDS_Shape();
  }
  // The tube a tenth of the quarter past both contacts, the full length.
  const int                  nx = 9, nt = 61;
  NCollection_Array2<gp_Pnt> points(1, nx, 1, nt);
  for (int i = 1; i <= nx; ++i) {
    const double x = -24.0 + 48.0 * (i - 1) / (nx - 1);
    for (int j = 1; j <= nt; ++j) {
      const double t = 0.5 * test::kPi * (1.2 * (j - 1) / (nt - 1) - 0.1);
      const double r = kRadius + amp * std::sin(k * t);
      points.SetValue(i, j, gp_Pnt(x, kRadius - r * std::cos(t), kTop - kRadius + r * std::sin(t)));
    }
  }
  const occ::handle<Geom_BSplineSurface> tube =
    GeomAPI_PointsToBSplineSurface(points, 3, 8, GeomAbs_C2, 1e-6).Surface();
  TopoDS_Face face =
    BRepBuilderAPI_MakeFace(tube, TopoDS::Wire(TopExp_Explorer(target, TopAbs_WIRE).Current()), true).Face();
  face.Orientation(target.Orientation());
  const occ::handle<BRepTools_ReShape> reshape = new BRepTools_ReShape();
  reshape->Replace(target, face);
  ShapeFix_Shape fix(reshape->Apply(body));
  fix.Perform();
  return fix.Shape();
}

// The top rim rounded with 1: the volume taken off, -1 where the rounding
// fails or is invalid.
double rim_removes(const TopoDS_Shape& body) {
  CHECK(port::valid(body));
  std::vector<TopoDS_Edge> rim;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge       edge = TopoDS::Edge(ex.Current());
    const BRepAdaptor_Curve c(edge);
    if (c.GetType() == GeomAbs_Circle && std::abs(c.Circle().Radius() - kRim) < 1e-6 &&
        std::abs(c.Circle().Location().Z() - kTop) < 1e-6 &&
        std::none_of(rim.begin(), rim.end(), [&edge](const TopoDS_Edge& e) { return e.IsSame(edge); })) {
      rim.push_back(edge);
    }
  }
  CHECK(rim.size() == 1);
  if (rim.size() != 1) {
    return -1.0;
  }
  const TopoDS_Shape result = port::rounded(body, rim, kRadius);
  if (!port::valid(result)) {
    return -1.0;
  }
  return port::volume_of(body) - port::volume_of(result);
}

// The rim through the mitres with the rippled tube.
void test_rim_through_a_mitre_with_a_rippled_tube() {
  const TopoDS_Shape lid = half_lid();
  CHECK(port::valid(lid));
  const double plain = rim_removes(with_rippled_tube(lid, 0.0, 1));
  CHECK(plain > 0.0);
  const struct {
    double amp;
    int    k;
  } ripples[] = {{6e-6, 4}, {1e-5, 2}, {1e-5, 6}};
  for (const auto& ripple : ripples) {
    const double removed = rim_removes(with_rippled_tube(lid, ripple.amp, ripple.k));
    if (removed < 0.0) {
      std::fprintf(stderr, "ripple %g (k = %d): no valid rounding\n", ripple.amp, ripple.k);
    }
    CHECK(removed > 0.0);
    if (removed > 0.0 && plain > 0.0) {
      CHECK_NEAR_TOL(removed, plain, 1e-5);
    }
  }
}

// The boundary: the fitted tube without a ripple, built as before.
void test_rim_through_a_mitre_with_a_fitted_tube_is_as_before() {
  CHECK(rim_removes(with_rippled_tube(half_lid(), 0.0, 1)) > 0.0);
}

} // namespace

MITCAD_PORT_TEST("1523", test_rim_through_a_mitre_with_a_rippled_tube);
MITCAD_PORT_TEST("1523", test_rim_through_a_mitre_with_a_fitted_tube_is_as_before);
