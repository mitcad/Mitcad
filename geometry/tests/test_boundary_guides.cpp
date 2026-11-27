// SPDX-License-Identifier: MIT
// Boundary Surface's Fill a Loop through guide curves inside the loop
// (mitcad#2526, GitHub mitcad/Mitcad#711): a bowl's rim of seven arcs and
// two arcs across it that set its depth. The face is one, valid, bounded
// by the rim alone (seven edges: the guides neither bound nor split it)
// and runs through the rim and the guides within kGuideTolerance; the
// same loop without guides stays the flat disc it was; a guide outside the
// loop and a guide that is not curves are refused. Every operation leaves
// its inputs as they are (the input check is on).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/surface_fills.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

// The rim: the circle of radius 20 about the origin in the xy plane.
constexpr double kRim = 20.0;
// The guides: arcs of radius 29 about (0, 0, 21) through the rim's points
// on the axes, 8 deep at the middle (the bowl a spherical cap).
constexpr double kGuideRadius = 29.0;
constexpr double kGuideCenter = 21.0;
constexpr double kDepth = 8.0;

// An arc of a circle about `center` in the plane of `normal`, from
// `first` to `last` (radians from `x_axis`, toward normal x x_axis).
g::PathCurve arc(const std::string& name, gp_Pnt center, gp_Dir normal, gp_Dir x_axis, double radius,
                 double first, double last) {
  g::PathCurve piece;
  piece.name = name;
  piece.curve.kind = g::ModelCurveKind::Conic;
  piece.curve.center = center;
  piece.curve.normal = normal;
  piece.curve.x_axis = x_axis;
  piece.curve.major = radius;
  piece.curve.minor = radius;
  piece.curve.first = first;
  piece.curve.last = last;
  return piece;
}

// The rim as one sketch's chain: seven arcs c1 to c7 end to end.
g::BoundaryCurve rim() {
  g::BoundaryCurve curve;
  curve.kind = g::BoundaryCurve::Kind::Curves;
  curve.plane = gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  for (int i = 0; i < 7; ++i) {
    curve.curves.push_back(arc("c" + std::to_string(i + 1), gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), kRim,
                               2.0 * kPi * i / 7.0, 2.0 * kPi * (i + 1) / 7.0));
  }
  return curve;
}

// The angle on a guide arc where it meets the rim (its ends are this and
// pi minus this; its bottom is at pi / 2).
double guide_end() { return std::atan2(kGuideCenter, kRim); }

// A guide arc in the vertical plane through the z axis along `along`
// (x or y), from the rim to the rim through (0, 0, -8).
g::PathCurve guide_arc(const std::string& name, gp_Dir along) {
  // normal x along points down, so the arc dips below the rim's plane.
  const gp_Dir normal = gp_Dir(0, 0, 1).Crossed(along);
  return arc(name, gp_Pnt(0, 0, kGuideCenter), normal, along, kGuideRadius, guide_end(), kPi - guide_end());
}

// The two guides in one entry (curves that do not join: each a guide of
// its own), on the xz plane.
g::BoundaryCurve guides() {
  g::BoundaryCurve curve;
  curve.kind = g::BoundaryCurve::Kind::Curves;
  curve.plane = gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0));
  curve.curves = {guide_arc("c8", gp_Dir(1, 0, 0)), guide_arc("c9", gp_Dir(0, 1, 0))};
  return curve;
}

double distance_to(const gp_Pnt& p, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), shape);
  return distance.IsDone() ? distance.Value() : 1.0e9;
}

int edges_of(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(it.Current()))) {
      ++count;
    }
  }
  return count;
}

int faces_of(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

double lowest(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, false, false);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  return z0;
}

void test_a_rim_is_filled_through_its_guides_as_one_face() {
  const g::ShapePtr bowl = g::boundary_fill("F9", {rim()}, {guides()});
  const TopoDS_Shape& face = bowl->occt();
  CHECK(bowl->find_faces("F9:boundary").size() == 1);
  CHECK(faces_of(face) == 1);
  CHECK(BRepCheck_Analyzer(face).IsValid());
  // The rim's seven arcs are the face's edges; the guides add none.
  CHECK(edges_of(face) == 7);
  // The face runs through the rim and both guides.
  double rim_gap = 0.0;
  double guide_gap = 0.0;
  for (int i = 0; i <= 48; ++i) {
    const double a = 2.0 * kPi * i / 48.0;
    rim_gap = std::max(rim_gap, distance_to(gp_Pnt(kRim * std::cos(a), kRim * std::sin(a), 0.0), face));
    const double t = guide_end() + (kPi - 2.0 * guide_end()) * i / 48.0;
    const double across = kGuideRadius * std::cos(t);
    const double z = kGuideCenter - kGuideRadius * std::sin(t);
    guide_gap = std::max(guide_gap, distance_to(gp_Pnt(across, 0.0, z), face));
    guide_gap = std::max(guide_gap, distance_to(gp_Pnt(0.0, across, z), face));
  }
  CHECK(rim_gap <= g::kGuideTolerance);
  CHECK(guide_gap <= g::kGuideTolerance);
  if (rim_gap > g::kGuideTolerance || guide_gap > g::kGuideTolerance) {
    std::fprintf(stderr, "the bowl lies %g mm off its rim and %g mm off its guides\n", rim_gap, guide_gap);
  }
  // A bowl 8 deep, not the flat disc.
  CHECK(lowest(face) < -kDepth + 0.1);
}

void test_the_same_rim_without_guides_is_the_flat_disc() {
  const g::ShapePtr disc = g::boundary_fill("F9", {rim()});
  const TopoDS_Shape& face = disc->occt();
  CHECK(faces_of(face) == 1);
  CHECK(edges_of(face) == 7);
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  CHECK_NEAR_TOL(props.Mass(), kPi * kRim * kRim, 1e-3);
  CHECK(std::abs(lowest(face)) < 1e-3);
}

void test_a_guide_outside_the_loop_or_not_curves_is_refused() {
  // A line beside the rim: no face inside the rim runs through it.
  g::BoundaryCurve outside;
  outside.kind = g::BoundaryCurve::Kind::Curves;
  outside.plane = gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0));
  g::PathCurve line;
  line.name = "c10";
  line.curve.kind = g::ModelCurveKind::Line;
  line.curve.start = gp_Pnt(30, 0, 0);
  line.curve.end = gp_Pnt(40, 0, -5);
  outside.curves = {line};
  CHECK(throws_with([&] { g::boundary_fill("F9", {rim()}, {outside}); }, "guide curve"));
  // A point is no guide.
  g::BoundaryCurve point;
  point.kind = g::BoundaryCurve::Kind::Point;
  point.point = gp_Pnt(0, 0, -8);
  CHECK(throws_with([&] { g::boundary_fill("F9", {rim()}, {point}); }, "is sketch curves"));
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(boundary_guides) {
  mitcad::geometry::enable_input_checks(true);
  test::guarded("test_a_rim_is_filled_through_its_guides_as_one_face",
                test::test_a_rim_is_filled_through_its_guides_as_one_face);
  test::guarded("test_the_same_rim_without_guides_is_the_flat_disc",
                test::test_the_same_rim_without_guides_is_the_flat_disc);
  test::guarded("test_a_guide_outside_the_loop_or_not_curves_is_refused",
                test::test_a_guide_outside_the_loop_or_not_curves_is_refused);
  for (const std::string& change : mitcad::geometry::input_changes()) {
    std::fprintf(stderr, "an input changed: %s\n", change.c_str());
    ++test::failures;
  }
  if (test::failures == 0) {
    std::puts("test_boundary_guides: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
