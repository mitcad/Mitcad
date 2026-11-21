// SPDX-License-Identifier: MIT
// Port patch 1830: a fillet or chamfer ending at a vertex where it passes
// a face between two faces each tangent to it there (GitHub
// mitcad/Mitcad#449, mitcad#2039).
//
// A low block whose plan is a half disc of radius 5 (y <= 0, 5 high)
// stands against a wall 10 wide and 5 thick (0 <= y <= 5) whose top is a
// half cylinder of radius 5 about the y axis at z = 5: the wall's side
// planes (x = +-5, up to z = 5) run on tangentially into the block's
// curved side and into the wall's curved top. The concave edge where the
// block's top meets the wall's front (y = 0, z = 5, 10 long) is rounded
// with r, or chamfered with r. At each end of the edge five faces meet;
// the end of the blend crosses the block's curved side and the wall's
// curved top, both continued past the vertex, and passes the wall's side
// plane on the far side of the vertex.
//
// Before the patch, OCCT cut the whole end on the wall's top alone (a
// shortcut for faces on one surface): an invalid shape, the wall's top
// left without its curves on the face and with an edge 11 mm in tolerance,
// the body 142 mm3 smaller (the reported fillet's broken preview; the
// chamfer refused as invalid). With only the shortcut held to its premise,
// the general cut crossed the side plane too, its three curves crossing
// each other: invalid again.
//
// Now each end is cut on the block's side and on the wall's top, which
// meet along the intersection of their surfaces from the vertex: one valid
// solid whose added volume is that of the blend's section swept along the
// edge and cut by both cylinders continued, 2 times the integral over the
// section of sqrt(25 - max(s, h)^2) (s, h the section's point's distances
// from the wall's front and the block's top), computed here.
//
// The nearest case outside the patch: the wall's top rounded with 3 at its
// edges only, so that its side plane runs on above the block (one tangent
// edge at the vertex, the side plane crossed): the general cut as before,
// one valid solid of the same kind of volume (the block's side continued
// up to the plane).

#include <cmath>
#include <cstdio>
#include <functional>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>

#include "port_test.hpp"

namespace {

constexpr double kR     = 5.0; // the block's plan and the wall's top: radius
constexpr double kLow   = 5.0; // the block's height, where the wall's top begins
constexpr double kThick = 5.0; // the wall's thickness along y

// The block's half disc (y <= 0) raised kLow, fused with the wall: a
// rectangle up to kLow under a half disc of radius kR, or under a rectangle
// up to `wall_top` whose top edges are rounded with `top_rounding`
// (`top_rounding` 0: the half disc), swept along y over kThick.
TopoDS_Shape stepped_body(double wall_top, double top_rounding) {
  BRepBuilderAPI_MakeWire plan;
  plan.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(kR, 0, 0), gp_Pnt(-kR, 0, 0)).Edge());
  plan.Add(BRepBuilderAPI_MakeEdge(
             GC_MakeArcOfCircle(gp_Pnt(-kR, 0, 0), gp_Pnt(0, -kR, 0), gp_Pnt(kR, 0, 0)).Value())
             .Edge());
  const TopoDS_Shape block =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(plan.Wire(), true).Face(), gp_Vec(0, 0, kLow)).Shape();

  BRepBuilderAPI_MakeWire side;
  if (top_rounding <= 0.0) {
    side.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-kR, 0, 0), gp_Pnt(kR, 0, 0)).Edge());
    side.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(kR, 0, 0), gp_Pnt(kR, 0, kLow)).Edge());
    side.Add(BRepBuilderAPI_MakeEdge(
               GC_MakeArcOfCircle(gp_Pnt(kR, 0, kLow), gp_Pnt(0, 0, kLow + kR), gp_Pnt(-kR, 0, kLow)).Value())
               .Edge());
    side.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-kR, 0, kLow), gp_Pnt(-kR, 0, 0)).Edge());
  } else {
    const double rho = top_rounding;
    const double s   = std::sqrt(0.5) * rho; // the arcs' middles off their centres
    side.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-kR, 0, 0), gp_Pnt(kR, 0, 0)).Edge());
    side.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(kR, 0, 0), gp_Pnt(kR, 0, wall_top - rho)).Edge());
    side.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(gp_Pnt(kR, 0, wall_top - rho),
                                                        gp_Pnt(kR - rho + s, 0, wall_top - rho + s),
                                                        gp_Pnt(kR - rho, 0, wall_top))
                                       .Value())
               .Edge());
    side.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(kR - rho, 0, wall_top), gp_Pnt(-kR + rho, 0, wall_top)).Edge());
    side.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(gp_Pnt(-kR + rho, 0, wall_top),
                                                        gp_Pnt(-kR + rho - s, 0, wall_top - rho + s),
                                                        gp_Pnt(-kR, 0, wall_top - rho))
                                       .Value())
               .Edge());
    side.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-kR, 0, wall_top - rho), gp_Pnt(-kR, 0, 0)).Edge());
  }
  const TopoDS_Shape wall =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(side.Wire(), true).Face(), gp_Vec(0, kThick, 0)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(block, wall).Shape());
  unify.Build();
  return unify.Shape();
}

// The concave edge along y = 0, z = kLow.
std::vector<TopoDS_Edge> concave_edge(const TopoDS_Shape& body) {
  return port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Y()) < 1e-9 && std::abs(p.Z() - kLow) < 1e-9 && std::abs(p.X()) < 1e-9;
  });
}

// The integral over [a, b] of 2 sqrt(kR^2 - t^2) dt.
double strip(double a, double b) {
  const auto F = [](double t) { return t * std::sqrt(kR * kR - t * t) + kR * kR * std::asin(t / kR); };
  return F(b) - F(a);
}

// The volume a blend of the edge adds, its section {(s, h): 0 <= h <=
// top(s), 0 <= s <= r} cut at each end by the two cylinders continued:
// over the section, the length 2 sqrt(kR^2 - max(s, h)^2).
double added_volume(double r, const std::function<double(double)>& top) {
  const int n    = 20000;
  double    sum  = 0.0;
  const double ds = r / n;
  for (int i = 0; i < n; ++i) {
    const double s = (i + 0.5) * ds;
    const double H = top(s);
    const double a = std::min(s, H);
    double       f = 2.0 * std::sqrt(kR * kR - s * s) * a;
    if (H > a) {
      f += strip(a, H);
    }
    sum += f * ds;
  }
  return sum;
}

double fillet_volume(double r) {
  return added_volume(r, [r](double s) { return r - std::sqrt(r * r - (r - s) * (r - s)); });
}

double chamfer_volume(double r) {
  return added_volume(r, [r](double s) { return r - s; });
}

void check_blend(const TopoDS_Shape& body, const TopoDS_Shape& result, double expected, const char* what) {
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  const double added = port::volume_of(result) - port::volume_of(body);
  std::printf("%s: valid %d, solids %d, largest tolerance %.3g, added %.6f (expected %.6f)\n", what,
              port::valid(result) ? 1 : 0, port::count_of(result, TopAbs_SOLID), port::max_tolerance(result),
              added, expected);
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  CHECK(port::max_tolerance(result) <= 1e-4);
  CHECK(std::abs(added - expected) <= 1e-5 * expected);
}

// The reported case at three sizes, the fillet and the chamfer.
void test_end_cut_across_a_tangent_face() {
  const TopoDS_Shape body = stepped_body(kLow + kR, 0.0);
  const std::vector<TopoDS_Edge> edge = concave_edge(body);
  CHECK(edge.size() == 1);
  if (edge.size() != 1) {
    return;
  }
  for (const double r : {0.5, 1.0, 3.0}) {
    std::printf("r = %g\n", r);
    check_blend(body, port::rounded(body, edge, r), fillet_volume(r), "fillet");
    check_blend(body, port::chamfered(body, edge, r), chamfer_volume(r), "chamfer");
  }
}

// The nearest case outside it: the wall's side plane runs on above the
// block (its top rounded with 3 at 10): the end crosses the block's side
// and the side plane, as before.
void test_end_cut_across_a_side_plane() {
  const TopoDS_Shape body = stepped_body(10.0, 3.0);
  const std::vector<TopoDS_Edge> edge = concave_edge(body);
  CHECK(edge.size() == 1);
  if (edge.size() != 1) {
    return;
  }
  const TopoDS_Shape result = port::rounded(body, edge, 1.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  std::printf("side plane: valid %d, added %.6f\n", port::valid(result) ? 1 : 0,
              port::volume_of(result) - port::volume_of(body));
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
}

} // namespace

MITCAD_PORT_TEST("1830", test_end_cut_across_a_tangent_face);
MITCAD_PORT_TEST("1830", test_end_cut_across_a_side_plane);
