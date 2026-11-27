// SPDX-License-Identifier: MIT
// A fillet on a knife edge with OCCT's fillet alone (mitcad#1984, GitHub
// mitcad/Mitcad#415, port patch 1770): two faces meeting at the edge
// tangentially, at 0 degrees, the material between them a wedge that thins
// to nothing. A ball of radius r touches both faces only where the wedge is
// 2 r thick, away from the edge; the rounding takes the tip beyond the two
// contact lines away and closes it with the ball's swept surface (for a
// straight edge a cylinder of radius r).
//
// The bodies are prisms 20 high along z of a profile with a cusp:
// - the reported sketch: a rectangle 130 by 210 and a circle of radius 65
//   centred on its bottom edge's midpoint, the rectangle's region outside
//   the circle; the side wall x = 65 and the circle's cylinder meet at the
//   cusp (65, 0);
// - two cylinders: the disc of radius 50 at the origin less the disc of
//   radius 30 centred at (20, 0), above y = 0; they meet at the cusp (50, 0);
// - both knife edges of the reported body with the two square corners of
//   its wall in one fillet.
//
// Each result is one valid closed solid whose volume is the exact shape's
// within 1e-6 relative: the prism's less the tip's section times the
// height, the section bounded by the two faces from the cusp to their
// contacts and by the ball's circle between the contacts (Green's theorem
// over lines and arcs). Before 1770 the two cylinders failed with no start
// (the plane and the cylinder OCCT computes analytically). A radius too
// large for the wedge (the ball would lie past the faces' ends) fails.

#include <cmath>
#include <cstdio>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Curve.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kHeight = 20.0;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// Twice the signed area Green's theorem gives a part of a boundary
// traversed with the region on its left: x dy - y dx.
double twice_area_of_line(double x0, double y0, double x1, double y1) {
  return x0 * y1 - y0 * x1;
}

// The same for the arc of the circle (cx, cy, radius) from angle a to b
// (b < a: clockwise).
double twice_area_of_arc(double cx, double cy, double radius, double a, double b) {
  return radius * radius * (b - a) + radius * cx * (std::sin(b) - std::sin(a)) -
         radius * cy * (std::cos(b) - std::cos(a));
}

TopoDS_Edge line(const gp_Pnt& a, const gp_Pnt& b) { return BRepBuilderAPI_MakeEdge(a, b); }

TopoDS_Edge arc(const gp_Pnt& a, const gp_Pnt& through, const gp_Pnt& b) {
  const occ::handle<Geom_Curve> curve(GC_MakeArcOfCircle(a, through, b).Value());
  return BRepBuilderAPI_MakeEdge(curve);
}

TopoDS_Shape prism(const std::vector<TopoDS_Edge>& edges, double height) {
  BRepBuilderAPI_MakeWire wire;
  for (const TopoDS_Edge& e : edges) {
    wire.Add(e);
  }
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true);
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, height)).Shape();
}

// The reported sketch's region: the rectangle 130 by 210 outside the circle
// of radius 65 centred on its bottom edge's midpoint (the origin).
TopoDS_Shape reported_body() {
  // (The circle's parameter 0 at the cusp, as a sketch's circle has it.)
  const gp_Circ circle(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ(), gp::DX()), 65.0);
  const TopoDS_Edge bite = TopoDS::Edge(BRepBuilderAPI_MakeEdge(circle, 0.0, kPi).Edge().Reversed());
  return prism({line(gp_Pnt(65, 0, 0), gp_Pnt(65, 210, 0)), line(gp_Pnt(65, 210, 0), gp_Pnt(-65, 210, 0)),
                line(gp_Pnt(-65, 210, 0), gp_Pnt(-65, 0, 0)), bite},
               kHeight);
}

// The disc of radius 50 less the disc of radius 30 centred at (20, 0), above
// y = 0: both circles' parameter 0 at the cusp, as a sketch's circles have
// it (their cylinders' seams on the knife edge), or arcs through three
// points (their parameters from elsewhere).
TopoDS_Shape two_cylinders_body(bool seams_at_cusp) {
  if (!seams_at_cusp) {
    return prism({arc(gp_Pnt(50, 0, 0), gp_Pnt(0, 50, 0), gp_Pnt(-50, 0, 0)), line(gp_Pnt(-50, 0, 0), gp_Pnt(-10, 0, 0)),
                  arc(gp_Pnt(-10, 0, 0), gp_Pnt(20, 30, 0), gp_Pnt(50, 0, 0))},
                 kHeight);
  }
  const gp_Circ outer(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ(), gp::DX()), 50.0);
  const gp_Circ inner(gp_Ax2(gp_Pnt(20, 0, 0), gp::DZ(), gp::DX()), 30.0);
  const TopoDS_Edge rim = BRepBuilderAPI_MakeEdge(outer, 0.0, kPi);
  const TopoDS_Edge bite = TopoDS::Edge(BRepBuilderAPI_MakeEdge(inner, 0.0, kPi).Edge().Reversed());
  return prism({rim, line(gp_Pnt(-50, 0, 0), gp_Pnt(-10, 0, 0)), bite}, kHeight);
}

// The edges on the vertical line through (x, y).
std::vector<TopoDS_Edge> edges_at(const TopoDS_Shape& body, double x, double y = 0.0) {
  std::vector<TopoDS_Edge> found;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(edges(i));
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    if (std::abs(a.X() - x) < 1e-7 && std::abs(b.X() - x) < 1e-7 && std::abs(a.Y() - y) < 1e-7 &&
        std::abs(b.Y() - y) < 1e-7) {
      found.push_back(e);
    }
  }
  return found;
}

// The tip's section the rounding takes from the reported body: the wall
// x = 65 from the cusp up to the ball's contact, the ball's circle round the
// tip's side to its contact with the circle of 65, that circle back to the
// cusp.
double reported_tip_area(double r) {
  const double cx = 65.0 - r;
  const double cy = std::sqrt((65.0 + r) * (65.0 + r) - cx * cx);
  double twice = twice_area_of_line(65.0, 0.0, 65.0, cy);
  twice += twice_area_of_arc(cx, cy, r, 0.0, std::atan2(-cy, -cx));
  twice += twice_area_of_arc(0.0, 0.0, 65.0, std::atan2(cy, cx), 0.0);
  return twice / 2.0;
}

// The tip's section of the two cylinders: the circle of 50 from the cusp to
// the ball's contact, the ball's circle round the tip's side to its contact
// with the circle of 30, that circle back to the cusp.
double two_cylinders_tip_area(double r) {
  const double a = 50.0 - r; // the ball's centre from the origin
  const double b = 30.0 + r; // and from (20, 0)
  const double cx = (a * a - b * b + 400.0) / 40.0;
  const double cy = std::sqrt(a * a - cx * cx);
  double twice = twice_area_of_arc(0.0, 0.0, 50.0, 0.0, std::atan2(cy, cx));
  twice += twice_area_of_arc(cx, cy, r, std::atan2(cy, cx), std::atan2(-cy, 20.0 - cx));
  twice += twice_area_of_arc(20.0, 0.0, 30.0, std::atan2(cy, cx - 20.0), 0.0);
  return twice / 2.0;
}

// Rounds the edges with r; the result, or a null shape where it fails.
TopoDS_Shape round(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r) {
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    for (const TopoDS_Edge& e : edges) {
      fillet.Add(r, e);
    }
    fillet.Build();
    if (fillet.IsDone()) {
      return fillet.Shape();
    }
    std::fprintf(stderr, "  not done (failure kind %d)\n", static_cast<int>(fillet.Failure()));
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "  threw: %s\n", failure.what());
  }
  return TopoDS_Shape();
}

void check_result(const TopoDS_Shape& result, double expected) {
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(count(result, TopAbs_SOLID) == 1);
  CHECK(count(result, TopAbs_SHELL) == 1);
  for (TopExp_Explorer shell(result, TopAbs_SHELL); shell.More(); shell.Next()) {
    CHECK(BRep_Tool::IsClosed(shell.Current()));
  }
  const double volume = volume_of(result);
  std::printf("  volume %.9f, expected %.9f\n", volume, expected);
  CHECK_NEAR(volume, expected);
}

void test_reported_knife_edge() {
  const TopoDS_Shape body = reported_body();
  const double prism_volume = volume_of(body);
  CHECK_NEAR(prism_volume, kHeight * (130.0 * 210.0 - kPi * 65.0 * 65.0 / 2.0));
  const std::vector<TopoDS_Edge> knife = edges_at(body, 65.0);
  CHECK(knife.size() == 1);
  for (const double r : {1.0, 5.0}) {
    std::printf(" the reported knife edge, radius %.1f\n", r);
    check_result(round(body, knife, r), prism_volume - kHeight * reported_tip_area(r));
  }
}

void test_two_cylinders() {
  // Each body, and its mirror image across the plane x = 0 (the faces'
  // orientations the other way round).
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp_Pnt(0, 0, 0), gp::DX()));
  for (const bool seams_at_cusp : {true, false}) {
    for (const bool mirrored : {false, true}) {
      const TopoDS_Shape original = two_cylinders_body(seams_at_cusp);
      const TopoDS_Shape body = mirrored ? BRepBuilderAPI_Transform(original, mirror, true).Shape() : original;
      const double prism_volume = volume_of(body);
      CHECK_NEAR(prism_volume, kHeight * kPi * (50.0 * 50.0 - 30.0 * 30.0) / 2.0);
      const std::vector<TopoDS_Edge> knife = edges_at(body, mirrored ? -50.0 : 50.0);
      CHECK(knife.size() == 1);
      for (const double r : {1.0, 5.0}) {
        std::printf(" two cylinders (%s%s), radius %.1f\n", seams_at_cusp ? "seams at the cusp" : "arcs",
                    mirrored ? ", mirrored" : "", r);
        check_result(round(body, knife, r), prism_volume - kHeight * two_cylinders_tip_area(r));
      }
    }
  }
}

// Both knife edges and the two square corners of the reported body in one
// fillet.
void test_with_ordinary_edges() {
  const TopoDS_Shape body = reported_body();
  const double prism_volume = volume_of(body);
  std::vector<TopoDS_Edge> edges;
  for (const auto& [x, y] : {std::pair{65.0, 0.0}, std::pair{-65.0, 0.0}, std::pair{65.0, 210.0}, std::pair{-65.0, 210.0}}) {
    const std::vector<TopoDS_Edge> at = edges_at(body, x, y);
    CHECK(at.size() == 1);
    edges.insert(edges.end(), at.begin(), at.end());
  }
  const double r = 2.0;
  std::printf(" both knife edges and two square corners, radius %.1f\n", r);
  check_result(round(body, edges, r),
               prism_volume - kHeight * (2.0 * reported_tip_area(r) + 2.0 * r * r * (1.0 - kPi / 4.0)));
}

// A radius whose ball lies past the faces' ends: no result, a failure.
void test_radius_too_large() {
  const TopoDS_Shape body = two_cylinders_body(true);
  std::printf(" two cylinders, radius 25 (larger than the wedge allows)\n");
  CHECK(round(body, edges_at(body, 50.0), 25.0).IsNull());
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_knife_edge) {
  test::guarded("reported_knife_edge", test::test_reported_knife_edge);
  test::guarded("two_cylinders", test::test_two_cylinders);
  test::guarded("with_ordinary_edges", test::test_with_ordinary_edges);
  test::guarded("radius_too_large", test::test_radius_too_large);
  return test::failures == 0 ? 0 : 1;
}
