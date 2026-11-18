// SPDX-License-Identifier: MIT
// Port patch 0074: a walked blend whose contact on face N leaves N across
// an edge into the face B at the spine's end vertex V, before the spine
// reaches V, is walked on with N's surface unbounded until its other
// contact (on Fo) reaches the edge between Fo and B at V, and B cuts that
// piece: the stripe ends in a point there, with no corner at V. OCCT's
// corner at V, where Fo is tangent to B, gave an invalid result or failed.
//
// An inside corner between a wall N (the side y = 0 of a plate a thick
// under the top z = 0, the plate on y < 0) and a wall W (the side x = 0 of a
// block on x < 0) whose top edge is rounded with a; the corner's edge, the
// arc where N meets W's rounding, rounded with r. W's rounding made a
// surface of revolution, so that the blend is walked. Near the top the
// contact on N leaves N across its top edge before the arc ends: built,
// valid, the blend ending in a point on the edge between W's rounding and
// the top, r from N, the fill exact (a = 3, r = 2 and a = 2, r = 1.5;
// geometry.fillet_overflow has a = 2, r = 3). With the plate rising 5 above
// W's top the contact stays on N, the corner's edge runs on along W's top
// to the block's end: no cut, as stock.
//
// The blend is the torus about the axis of W's rounding (y through x = -a,
// z = -a), its centre circle of radius R = a + r lying r from N. The fill,
// between N, W's rounding and the torus, revolved about that axis from the
// plate's bottom (the end section of the arc) up to the top: at distance
// rho from the axis the section's height is y(rho) and the angle up to the
// top asin(a / rho); with rho = R - r cos(psi), y = r - r sin(psi) the
// integrand is smooth.

#include <cmath>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <ShapeCustom.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>

#include "port_test.hpp"

namespace {

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

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, true);
  unify.Build();
  return unify.Shape();
}

// The block (x < 0, z from -10 to 0) fused with the plate (y < 0, from
// z = -a up to `top`, x from `from` to 20), W's top edge rounded with a and
// made a surface of revolution; a null shape when that rounding fails.
TopoDS_Shape corner_body(double a, double top, double from) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-20, -5, -10), gp_Pnt(0, 20, 0)).Shape();
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(from, -20, -a), gp_Pnt(20, 0, top)).Shape();
  const TopoDS_Shape body  = unified(BRepAlgoAPI_Fuse(block, plate).Shape());
  const TopoDS_Shape rounded = port::rounded(body, {nearest_edge(body, gp_Pnt(0, 10, 0))}, a);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return TopoDS_Shape();
  }
  return ShapeCustom::ConvertToRevolution(rounded);
}

// The torus's fill up to the top (`whole`: the full quarter turn).
double torus_fill(double a, double r, bool whole) {
  double    fill = 0.0;
  const int n    = 20000;
  for (int i = 0; i < n; ++i) {
    const double psi = 0.5 * test::kPi * (i + 0.5) / n;
    const double rho = a + r - r * std::cos(psi);
    const double turn = whole ? 0.5 * test::kPi : std::asin(a / rho);
    fill += rho * turn * (r - r * std::sin(psi)) * r * std::sin(psi) * (0.5 * test::kPi / n);
  }
  return fill;
}

// The arc where N meets W's rounding, rounded with r.
TopoDS_Shape corner_rounded(const TopoDS_Shape& body, double a, double r) {
  const double mid = -a + a / std::sqrt(2.0);
  return port::rounded(body, {nearest_edge(body, gp_Pnt(mid, 0, mid))}, r);
}

bool has_vertex_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p) < 1e-6) {
      return true;
    }
  }
  return false;
}

void check_cut_by_the_top(double a, double r) {
  const TopoDS_Shape body = corner_body(a, 0.0, -5.0);
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape rounded = corner_rounded(body, a, r);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(rounded), port::volume_of(body) + torus_fill(a, r, false), 1e-7);
  // The blend ends in a point on the edge between W's rounding and the
  // top, r from N.
  CHECK(has_vertex_at(rounded, gp_Pnt(-a, r, 0)));
}

// W rounded with 3, the plate 3 thick, the corner with 2: the contact on N
// (5 from the axis) reaches the top 37 degrees up the arc.
void test_walked_corner_rounding_is_cut_by_the_top() {
  check_cut_by_the_top(3.0, 2.0);
}

// W rounded with 2, the plate 2 thick, the corner with 1.5: the contact on
// N (3.5 from the axis) reaches the top 35 degrees up the arc.
void test_smaller_walked_corner_rounding_is_cut_by_the_top() {
  check_cut_by_the_top(2.0, 1.5);
}

// The boundary: the plate rises 5 above W's top and runs along the whole
// block (x from -20). N's contact stays on N; past the arc the corner's
// edge runs on tangentially along W's top (N and the top, a quarter
// cylinder of r) to the block's end x = -20. No cut, as stock: the whole
// torus quarter and the cylinder along the 18 of the straight edge.
void test_walked_corner_rounding_below_a_higher_wall_is_as_stock() {
  const double       a = 2.0, r = 1.5;
  const TopoDS_Shape body = corner_body(a, 5.0, -20.0);
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape rounded = corner_rounded(body, a, r);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(rounded),
                 port::volume_of(body) + torus_fill(a, r, true) + test::fillet_loss(r) * (20.0 - a), 1e-7);
}

} // namespace

MITCAD_PORT_TEST("0074", test_walked_corner_rounding_is_cut_by_the_top);
MITCAD_PORT_TEST("0074", test_smaller_walked_corner_rounding_is_cut_by_the_top);
MITCAD_PORT_TEST("0074", test_walked_corner_rounding_below_a_higher_wall_is_as_stock);
