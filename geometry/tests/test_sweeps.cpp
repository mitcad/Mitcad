// SPDX-License-Identifier: MIT
// Sweeps, lofts, pipes, coils, ribs and webs (F3): the volumes of the T0
// models (sweep_*, loft_*, pipe_*, ui_coil, ui_rib) and analytic ones,
// names, and failures.

#include <algorithm>
#include <cmath>
#include <limits>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Line.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Ax3.hxx>

#include "../src/util.hpp"
#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The analysis library's integrals (query.hpp), exact enough for B-spline
// faces, also for faces that run many turns: OCCT's adaptive integration
// gave a nut threaded with such faces (a thread built as a B-rep,
// mitcad#235) 0.056 mm³ too much of its 269.28 mm³, and 1.40 too little
// with 18 turns.
double precise_volume(const Shape& shape) { return mitcad::geometry::volume(shape); }

ModelCurve line3(const gp_Pnt& a, const gp_Pnt& b) {
  ModelCurve curve;
  curve.kind = ModelCurveKind::Line;
  curve.start = a;
  curve.end = b;
  return curve;
}

ModelCurve arc3(const gp_Pnt& center, const gp_Dir& normal, const gp_Dir& x, double radius, double first,
                double last) {
  ModelCurve curve;
  curve.kind = ModelCurveKind::Conic;
  curve.center = center;
  curve.normal = normal;
  curve.x_axis = x;
  curve.major = curve.minor = radius;
  curve.first = first;
  curve.last = last;
  curve.closed = last - first >= 2 * kPi - 1e-12;
  return curve;
}

// The path of sweep_path and pipe_path: (0,0)-(40,0), a quarter of radius 20
// about (40, 20) to (60, 20), then up to (60, 60); 80 + 10 pi long.
Path bent_path() {
  return {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(40, 0, 0))},
          {"c2", arc3(gp_Pnt(40, 20, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 20, -kPi / 2, 0)},
          {"c3", line3(gp_Pnt(60, 20, 0), gp_Pnt(60, 60, 0))}};
}

Path line_path(double x0, double x1) { return {{"c1", line3(gp_Pnt(x0, 0, 0), gp_Pnt(x1, 0, 0))}}; }

// The YZ origin plane's sketch frame (x = -Z, y = +Y, normal +X).
Frame yz() {
  Frame frame;
  frame.x_axis = gp_Dir(0, 0, -1);
  frame.y_axis = gp_Dir(0, 1, 0);
  return frame;
}

SweepSpec circle_sweep(Path path, double radius) {
  SweepSpec spec;
  spec.feature = "F5";
  spec.frame = yz();
  spec.regions = {circle(1, 0, 0, radius)};
  spec.path = std::move(path);
  return spec;
}

double face_min_x(const Shape& shape, const std::string& name) {
  const std::vector<int> faces = shape.find_faces(name);
  return faces.size() == 1 ? bounding_box(Shape(shape.face(faces.front()))).min.X() : -1e9;
}

void test_sweeps() {
  // sweep_path: V = pi r^2 L (Pappus).
  const ShapePtr bent = sweep(circle_sweep(bent_path(), 5));
  CHECK_NEAR_TOL(precise_volume(*bent), kPi * 25 * (80 + 10 * kPi), 1e-6);
  CHECK(faces_named(*bent, "F5:start(r{c1})") == 1 && faces_named(*bent, "F5:end(r{c1})") == 1);
  CHECK(!bent->find_faces("F5:side(c1)").empty());
  CHECK(near(bounding_box(Shape(bent->face(bent->find_faces("F5:start(r{c1})").front()))).max.X(), 0, 1e-9));
  CHECK(edge_names_unique(*bent));

  // The profile inside the path: both ways; start ends side one.
  SweepSpec middle = circle_sweep(line_path(-20, 30), 5);
  const ShapePtr both = sweep(middle);
  CHECK_NEAR_TOL(precise_volume(*both), kPi * 25 * 50, 1e-6);
  CHECK(near(face_min_x(*both, "F5:start(r{c1})"), 30, 1e-9));
  CHECK(near(face_min_x(*both, "F5:end(r{c1})"), -20, 1e-9));
  // Halves of each side: from -10 to 15.
  middle.extent1 = 0.5;
  middle.extent2 = 0.5;
  const ShapePtr halves = sweep(middle);
  CHECK_NEAR_TOL(precise_volume(*halves), kPi * 25 * 25, 1e-6);
  CHECK(near(bounding_box(*halves).min.X(), -10, 1e-6) && near(bounding_box(*halves).max.X(), 15, 1e-6));
  // Half of the bent path, from its start.
  SweepSpec half = circle_sweep(bent_path(), 5);
  half.extent1 = 0.5;
  CHECK_NEAR_TOL(precise_volume(*sweep(half)), kPi * 25 * (80 + 10 * kPi) / 2, 1e-6);

  // sweep_twist: a 10 mm square turned 90 degrees over 50 mm keeps its area.
  SweepSpec twist;
  twist.feature = "F5";
  twist.frame = yz();
  twist.regions = {rectangle(1, -5, -5, 10, 10)};
  twist.path = line_path(0, 50);
  twist.twist = kPi / 2;
  const ShapePtr twisted = sweep(twist);
  CHECK_NEAR_TOL(precise_volume(*twisted), 10 * 10 * 50, 1e-5);
  CHECK(faces_named(*twisted, side("F5", 1, 0)) == 1 && faces_named(*twisted, end_cap("F5", 1)) == 1);
  CHECK(edge_names_unique(*twisted));

  // sweep_guide_rail: the rail from (0, 5) to (50, 10) scales the circle
  // into a frustum.
  SweepSpec rail = circle_sweep(line_path(0, 50), 5);
  rail.rail = {{"c2", line3(gp_Pnt(0, 5, 0), gp_Pnt(50, 10, 0))}};
  CHECK_NEAR_TOL(precise_volume(*sweep(rail)), kPi * 50 / 3 * (25 + 50 + 100), 1e-5);
  // Only towards the rail: an elliptic frustum, a = 5..10, b = 5.
  rail.scaling = ProfileScaling::Stretch;
  CHECK_NEAR_TOL(precise_volume(*sweep(rail)), kPi * 5 * 50 * (5 + 10) / 2, 1e-5);
  rail.scaling = ProfileScaling::None;
  CHECK_NEAR_TOL(precise_volume(*sweep(rail)), kPi * 25 * 50, 1e-5);

  // A taper: the circle grows by tan(taper) per unit of length, the same
  // frustum.
  SweepSpec taper = circle_sweep(line_path(0, 50), 5);
  taper.taper = std::atan(0.1);
  CHECK_NEAR_TOL(precise_volume(*sweep(taper)), kPi * 50 / 3 * (25 + 50 + 100), 1e-5);
  // Along the bend the area grows the same way: V = pi int (5 + s/10)^2 ds.
  SweepSpec bent_taper = circle_sweep(bent_path(), 5);
  bent_taper.taper = std::atan(0.1);
  const double length = 80 + 10 * kPi;
  const double r1 = 5 + length / 10;
  CHECK_NEAR_TOL(precise_volume(*sweep(bent_taper)), kPi * 10.0 / 3.0 * (r1 * r1 * r1 - 125), 1e-4);
  bent_taper.taper = -std::atan(0.1);
  CHECK(throws_with([&] { sweep(bent_taper); }, "taper closes the profile"));
  // Each side of a square leans out by the angle (the outline is offset, not
  // scaled): 5 degrees over 50 mm make the side 10 + 2 * 50 tan 5.
  SweepSpec lean = twist;
  lean.twist = 0;
  lean.taper = kPi / 36;
  const double end_side = 10 + 100 * std::tan(kPi / 36);
  CHECK_NEAR_TOL(precise_volume(*sweep(lean)), 50.0 / 3 * (100 + 10 * end_side + end_side * end_side), 1e-6);
  CHECK(faces_named(*sweep(lean), side("F5", 1, 0)) == 1);

  // Parallel along 60 degrees of a circle: the profile keeps facing +X, so
  // every slice across X is the circle: V = A * 50 sin 60.
  SweepSpec parallel = circle_sweep(
      {{"c1", arc3(gp_Pnt(0, 50, 0), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0), 50, 0, kPi / 3)}}, 2);
  CHECK_NEAR_TOL(precise_volume(*sweep(parallel)), kPi * 4 * 50 * kPi / 3, 1e-6);
  parallel.orientation = SweepOrientation::Parallel;
  CHECK_NEAR_TOL(precise_volume(*sweep(parallel)), kPi * 4 * 50 * std::sqrt(3.0) / 2, 1e-5);
  parallel.twist = 1.0;
  CHECK(throws_with([&] { sweep(parallel); }, "unsupported: twists"));

  // A ring profile keeps its hole; round a closed path there are no caps.
  SweepSpec ring = circle_sweep(
      {{"c4", arc3(gp_Pnt(0, 30, 0), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0), 30, 0, 2 * kPi)}}, 5);
  Region annulus = circle(1, 0, 0, 5);
  annulus.loops.push_back(circle(2, 0, 0, 3).loops.front());
  annulus.name = "r{c1}";
  ring.regions = {annulus};
  ring.extent2 = 0;
  const ShapePtr torus = sweep(ring);
  CHECK_NEAR_TOL(precise_volume(*torus), kPi * (25 - 9) * 2 * kPi * 30, 1e-6);
  CHECK(torus->find_faces("F5:start(r{c1})").empty());
  CHECK(!torus->find_faces("F5:side(c2)").empty());
  ring.extent1 = 0.25;
  CHECK_NEAR_TOL(precise_volume(*sweep(ring)), kPi * 16 * 2 * kPi * 30 / 4, 1e-6);

  // Paths that do not join.
  SweepSpec gap = circle_sweep(bent_path(), 5);
  gap.path[2].curve.start = gp_Pnt(70, 20, 0);
  CHECK(throws_with([&] { sweep(gap); }, "path curve c3 does not join"));
}

// A path whose first line meets its arc at a slight kink (0.7 degrees, as
// sketches drawn without a tangent constraint have): (0,0)-(10,0), a quarter
// of radius 5 less the kink, then 20 up along +Y; and its length.
Path kinked_path(double& length) {
  const double kink = 0.012;
  const gp_Pnt center(10 - 5 * std::sin(kink), 5 * std::cos(kink), 0);
  const gp_Pnt top(center.X() + 5, center.Y(), 0);
  length = 10 + 5 * (kPi / 2 - kink) + 20;
  return {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0))},
          {"c2", arc3(center, gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 5, kink - kPi / 2, 0)},
          {"c3", line3(top, gp_Pnt(top.X(), top.Y() + 20, 0))}};
}

// A sketch frame in the plane y = `y` (normal +Y) with its origin at x.
Frame xz_at(double x, double y) {
  Frame frame;
  frame.origin = gp_Pnt(x, y, 0);
  frame.x_axis = gp_Dir(1, 0, 0);
  frame.y_axis = gp_Dir(0, 0, -1);
  return frame;
}

bool valid(const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); }

// Sweeps with the profile at an end of the path, swept backwards (only the
// part before the profile), as cuts and joins.
void test_backward_sweeps() {
  // The bent path's end is (60, 60), where its last line runs along +Y.
  SweepSpec back = circle_sweep(bent_path(), 5);
  back.frame = xz_at(60, 60);
  back.regions = {circle(1, 0, 0, 5)};
  back.extent1 = 0;
  back.extent2 = 1;
  const ShapePtr bent = sweep(back);
  CHECK_NEAR_TOL(precise_volume(*bent), kPi * 25 * (80 + 10 * kPi), 1e-6);
  // The start cap is at the profile, the end cap at the path's start.
  const std::vector<int> start = bent->find_faces("F5:start(r{c1})");
  const std::vector<int> end = bent->find_faces("F5:end(r{c1})");
  CHECK(start.size() == 1 && end.size() == 1);
  if (start.size() == 1 && end.size() == 1) {
    CHECK(near(bounding_box(Shape(bent->face(start.front()))).min.Y(), 60, 1e-9));
    CHECK(near(bounding_box(Shape(bent->face(end.front()))).max.X(), 0, 1e-9));
  }
  // Half of the part before the profile: the last 40 + 5 pi.
  back.extent2 = 0.5;
  CHECK_NEAR_TOL(precise_volume(*sweep(back)), kPi * 25 * (80 + 10 * kPi) / 2, 1e-6);
  // Forwards from the end there is nothing to sweep, also with the profile
  // off the end by less than the modelling tolerance.
  back.extent1 = 1;
  back.extent2 = 0;
  CHECK(throws_with([&] { sweep(back); }, "covers none of the path"));
  SweepSpec short_path = circle_sweep(line_path(0, 10), 2);
  short_path.frame.origin = gp_Pnt(10 - 5e-8, 0, 0);
  short_path.extent2 = 0;
  CHECK(throws_with([&] { sweep(short_path); }, "covers none of the path"));
  short_path.extent1 = 0;
  short_path.extent2 = 1;
  CHECK_NEAR_TOL(precise_volume(*sweep(short_path)), kPi * 4 * 10, 1e-6);

  // A rounded rectangle at the end of a path with a slight kink: the mitre
  // at the kink is a sliver OCCT does not build, so the corner is rounded.
  double length = 0;
  SweepSpec kinked;
  kinked.feature = "F7";
  kinked.path = kinked_path(length);
  const gp_Pnt top = kinked.path.back().curve.end;
  kinked.frame = xz_at(top.X(), top.Y());
  Region rounded;
  rounded.name = "r1";
  Loop loop;
  const auto line = [](double x0, double y0, double x1, double y1, const char* name) {
    Segment segment;
    segment.name = name;
    segment.start = gp_Pnt2d(x0, y0);
    segment.end = gp_Pnt2d(x1, y1);
    return segment;
  };
  const auto corner = [](double cx, double cy, double from, const char* name) {
    Segment segment;
    segment.name = name;
    segment.kind = SegmentKind::Arc;
    segment.center = gp_Pnt2d(cx, cy);
    segment.radius = 1;
    segment.start_angle = from;
    segment.end_angle = from + kPi / 2;
    return segment;
  };
  loop.segments = {line(2, -2, -2, -2, "c1"), corner(-2, -1, kPi, "c2"), line(-3, -1, -3, 1, "c3"),
                   corner(-2, 1, kPi / 2, "c4"), line(-2, 2, 2, 2, "c5"), corner(2, 1, 0, "c6"),
                   line(3, 1, 3, -1, "c7"), corner(2, -1, -kPi / 2, "c8")};
  rounded.loops = {loop};
  kinked.regions = {rounded};
  kinked.extent1 = 0;
  kinked.extent2 = 1;
  const double area = 6 * 4 - (4 - kPi);
  const ShapePtr tool = sweep(kinked);
  CHECK_NEAR_TOL(precise_volume(*tool), area * length, 1e-3);
  CHECK(faces_named(*tool, "F7:start(r1)") == 1 && faces_named(*tool, "F7:end(r1)") == 1);
  CHECK(!tool->find_faces("F7:side(c3)").empty());
  // The same forwards from the path's start.
  SweepSpec forward = kinked;
  forward.frame.origin = gp_Pnt(0, 0, 0);
  forward.frame.x_axis = gp_Dir(0, 0, -1);
  forward.frame.y_axis = gp_Dir(0, 1, 0);
  forward.extent1 = 1;
  forward.extent2 = 0;
  CHECK_NEAR_TOL(precise_volume(*sweep(forward)), area * length, 1e-3);

  // A circle there: neither mitred nor rounded corners build, the section
  // turns through the kink.
  SweepSpec circular = kinked;
  circular.regions = {circle(9, 0, 0, 2)};
  const ShapePtr tube = sweep(circular);
  CHECK_NEAR_TOL(precise_volume(*tube), kPi * 4 * length, 1e-3);
  CHECK(faces_named(*tube, "F7:start(r{c9})") == 1 && !tube->find_faces("F7:side(c9)").empty());

  // As a cut and a join: a block round the last 15 mm of the path.
  const ShapePtr block = extrude("F1", {rectangle(1, top.X() - 10, 10, 20, 20)}, -10, 10);
  const double inside = area * 15;
  const BooleanResult cut = boolean(BooleanOp::Cut, {block.get()}, *tool);
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() == 1) {
    CHECK(valid(*cut.pieces[0].shape));
    CHECK_NEAR_TOL(precise_volume(*cut.pieces[0].shape), 20 * 20 * 20 - inside, 1e-5);
  }
  const BooleanResult join = boolean(BooleanOp::Join, {block.get()}, *tool);
  CHECK(join.pieces.size() == 1);
  if (join.pieces.size() == 1) {
    CHECK(valid(*join.pieces[0].shape));
    CHECK_NEAR_TOL(precise_volume(*join.pieces[0].shape), 20 * 20 * 20 + area * length - inside, 1e-4);
  }
}

// The parallel orientation along paths that turn through the profile's
// plane: a translated rectangle, 4 wide along the path's plane and 6 across
// it.
void test_parallel_turns() {
  SweepSpec spec;
  spec.feature = "F5";
  spec.frame = yz();
  spec.regions = {rectangle(1, -3, -2, 6, 4)};
  spec.orientation = SweepOrientation::Parallel;
  // 120 degrees of a circle of radius 50 from the origin, facing +X at the
  // start: the path turns back in X after 90 degrees. At x the section is
  // the rectangle at y = 50 -+ sqrt(50^2 - x^2) (the second beyond x =
  // 25 sqrt 3): V = 6 (4 * 50 + int min(4, 2 sqrt(50^2 - x^2)) dx), the
  // copies overlapping beyond x0 = 50 sqrt(1 - 0.04^2).
  spec.path = {{"c9", arc3(gp_Pnt(0, 50, 0), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0), 50, 0, 2 * kPi / 3)}};
  const double u0 = std::sqrt(1 - 0.04 * 0.04);
  const auto quarter = [](double u) { return (u * std::sqrt(1 - u * u) + std::asin(u)) / 2; };
  const double tail = 5000 * (quarter(1.0) - quarter(u0));
  const ShapePtr turned = sweep(spec);
  CHECK(valid(*turned));
  CHECK_NEAR_TOL(precise_volume(*turned), 6 * (200 + 4 * (50 * u0 - 25 * std::sqrt(3.0)) + tail), 1e-6);
  // Where it turns back the copy farthest along X is a face of its own.
  const std::string turn = face_name("F5", "turn", rectangle_region_name(1));
  CHECK(faces_named(*turned, turn) == 1);
  CHECK(faces_named(*turned, start_cap("F5", 1)) == 1 && faces_named(*turned, end_cap("F5", 1)) == 1);
  CHECK(turned->find_faces(side("F5", 1, 0)).size() == 2);
  CHECK(edge_names_unique(*turned));
  if (faces_named(*turned, turn) == 1) {
    const BoundingBox box = bounding_box(Shape(turned->face(turned->find_faces(turn).front())));
    CHECK(near(box.min.X(), 50, 1e-6) && near(box.max.X(), 50, 1e-6));
  }
  // The profile in the middle of a half circle, swept both ways: the same
  // as the half circle from its start.
  spec.path = {{"c9", arc3(gp_Pnt(0, 50, 0), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0), 50, -kPi / 3, 2 * kPi / 3)}};
  const double half = 6 * (4 * 50 * u0 + tail);
  CHECK_NEAR_TOL(precise_volume(*sweep(spec)), 6 * 200 + half, 1e-6);
  // A path that only touches the profile's plane (an S of two quarters
  // meeting along +Y) moves the rectangle 100 along X: no turn face.
  spec.path = {{"c9", arc3(gp_Pnt(0, 50, 0), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0), 50, 0, kPi / 2)},
               {"c10", arc3(gp_Pnt(100, 50, 0), gp_Dir(0, 0, -1), gp_Dir(-1, 0, 0), 50, 0, kPi / 2)}};
  const ShapePtr s_bend = sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*s_bend), 24 * 100, 1e-6);
  CHECK(faces_named(*s_bend, turn) == 0);
  // Once round a circle the rectangle turns back twice and has no caps.
  spec.path = {{"c9", arc3(gp_Pnt(0, 30, 0), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0), 30, 0, 2 * kPi)}};
  spec.extent2 = 0;
  const ShapePtr ring = sweep(spec);
  CHECK(valid(*ring));
  CHECK(ring->find_faces(start_cap("F5", 1)).empty() && ring->find_faces(turn).size() == 2);
  const double v0 = std::sqrt(1 - (2.0 / 30) * (2.0 / 30));
  const double ring_tail = 1800 * (quarter(1.0) - quarter(v0));
  CHECK_NEAR_TOL(precise_volume(*ring), 6 * (240 + 4 * 2 * 30 * v0 + 2 * ring_tail), 1e-6);
}

// Guide rails with several regions of a sketch: each region is turned and
// sized about the path, the one off the path as well.
void test_rail_regions() {
  // The rail runs from 8 to 12 beside the path along +Y: everything grows
  // by 1 + x/100 about the path, nothing turns. A circle of radius 3 round
  // the path and one of radius 2 10 above it (sketch u = -10 is z = +10).
  SweepSpec rail = circle_sweep(line_path(0, 50), 3);
  rail.regions.push_back(circle(2, -10, 0, 2));
  rail.rail = {{"c3", line3(gp_Pnt(0, 8, 0), gp_Pnt(50, 12, 0))}};
  const double grown = 100.0 / 3.0 * (1.5 * 1.5 * 1.5 - 1);
  const ShapePtr both = sweep(rail);
  CHECK_NEAR_TOL(precise_volume(*both), kPi * (9 + 4) * grown, 1e-5);
  CHECK(faces_named(*both, "F5:start(r{c2})") == 1 && faces_named(*both, "F5:end(r{c1})") == 1);
  // The one off the path alone; and backwards from the end of the path,
  // where it has its size at the rail's 12 and shrinks towards the start.
  rail.regions = {circle(2, -10, 0, 2)};
  CHECK_NEAR_TOL(precise_volume(*sweep(rail)), kPi * 4 * grown, 1e-5);
  rail.frame.origin = gp_Pnt(50, 0, 0);
  rail.extent1 = 0;
  CHECK_NEAR_TOL(precise_volume(*sweep(rail)), kPi * 4 * grown / (1.5 * 1.5), 1e-5);
}

// An end scale and a second guide rail (mitcad#464).
void test_two_rails_and_end_scale() {
  // A circle of radius 4 scaled to half over 50: a frustum of radii 4 and 2.
  SweepSpec cone = circle_sweep(line_path(0, 50), 4);
  cone.scale = 0.5;
  CHECK_NEAR_TOL(precise_volume(*sweep(cone)), kPi * 50 / 3 * (16 + 8 + 4), 1e-5);
  // The profile inside the path: the scale is reached at both ends, 30 and
  // 20 from it; a circle of 5 grows to 10 both ways.
  SweepSpec middle = circle_sweep(line_path(-20, 30), 5);
  middle.scale = 2;
  CHECK_NEAR_TOL(precise_volume(*sweep(middle)), kPi * 50 / 3 * (25 + 50 + 100), 1e-5);
  // With a twist: a 10 mm square turned a quarter turn and halved over 50,
  // each section 100 (1 - s / 100)^2.
  SweepSpec turned;
  turned.feature = "F5";
  turned.frame = yz();
  turned.regions = {rectangle(1, -5, -5, 10, 10)};
  turned.path = line_path(0, 50);
  turned.twist = kPi / 2;
  turned.scale = 0.5;
  CHECK_NEAR_TOL(precise_volume(*sweep(turned)), 100 * 100 / 3.0 * (1 - 0.125), 1e-5);

  // Two rails beside a 4 x 10 rectangle (sketch x = -Z, y = +Y): from
  // Y = -5 and 5 out to -7 and 7 over 50. The profile moves with the first
  // rail and grows with the rails' distance, k = 1 + 0.008 x: sections
  // 40 k^2 (scale), 40 k (stretch, only across) or 40 (none).
  SweepSpec rails;
  rails.feature = "F5";
  rails.frame = yz();
  rails.regions = {rectangle(1, -2, -5, 4, 10)};
  rails.path = line_path(0, 50);
  rails.rail = {{"c2", line3(gp_Pnt(0, -5, 0), gp_Pnt(50, -7, 0))}};
  rails.rail2 = {{"c3", line3(gp_Pnt(0, 5, 0), gp_Pnt(50, 7, 0))}};
  const ShapePtr scaled = sweep(rails);
  CHECK_NEAR_TOL(precise_volume(*scaled), 40 * (1.4 * 1.4 * 1.4 - 1) / (3 * 0.008), 1e-5);
  const BoundingBox wide = bounding_box(*scaled);
  CHECK(near(wide.min.Y(), -7, 1e-6) && near(wide.max.Y(), 7, 1e-6) && near(wide.max.Z(), 2.8, 1e-6));
  CHECK(faces_named(*scaled, start_cap("F5", 1)) == 1 && faces_named(*scaled, end_cap("F5", 1)) == 1);
  rails.scaling = ProfileScaling::Stretch;
  CHECK_NEAR_TOL(precise_volume(*sweep(rails)), 40 * 60.0, 1e-5);
  rails.scaling = ProfileScaling::None;
  CHECK_NEAR_TOL(precise_volume(*sweep(rails)), 40 * 50.0, 1e-5);
  // The first rail straight, the second opening from 10 to 20 away: the
  // profile grows about the first rail's point, k = 1 + x / 50, up to
  // Y = 15 and Z = 4 at the end.
  rails.scaling = ProfileScaling::Scale;
  rails.rail = {{"c2", line3(gp_Pnt(0, -5, 0), gp_Pnt(50, -5, 0))}};
  rails.rail2 = {{"c3", line3(gp_Pnt(0, 5, 0), gp_Pnt(50, 15, 0))}};
  const ShapePtr opened = sweep(rails);
  CHECK_NEAR_TOL(precise_volume(*opened), 40 * 50 / 3.0 * 7, 1e-5);
  const BoundingBox one_side = bounding_box(*opened);
  CHECK(near(one_side.min.Y(), -5, 1e-6) && near(one_side.max.Y(), 15, 1e-6) &&
        near(one_side.max.Z(), 4, 1e-6));

  // Refused: rails that meet, a second rail alone, an end scale with a rail
  // and one out of range.
  rails.rail2 = rails.rail;
  CHECK(throws_with([&] { sweep(rails); }, "the guide rails meet"));
  SweepSpec alone = circle_sweep(line_path(0, 50), 4);
  alone.rail2 = {{"c3", line3(gp_Pnt(0, 5, 0), gp_Pnt(50, 15, 0))}};
  CHECK(throws_with([&] { sweep(alone); }, "needs a first one"));
  alone.rail = alone.rail2;
  alone.rail2.clear();
  alone.scale = 2;
  CHECK(throws_with([&] { sweep(alone); }, "an end scale is not taken"));
  cone.scale = 0;
  CHECK(throws_with([&] { sweep(cone); }, "between 0.01 and 100"));
}

// The point of a guide rail where it crosses the plane square to the path
// (mitcad#469).
void test_rail_plane() {
  // A circle of radius 5 round the path along x from 0 to 50, guided by an
  // arc of radius 65 about (25, -55) that bulges from y = 5 at the ends to
  // 10 in the middle; the arc runs from x = 50 to 0, against the path. The
  // rail's speed along the path varies, so its points at the fractions of
  // its length are not where it crosses the planes square to the path.
  const double radius = 65.0;
  const auto rail_y = [&](double x) { return -55.0 + std::sqrt(radius * radius - (x - 25) * (x - 25)); };
  SweepSpec spec = circle_sweep(line_path(0, 50), 5);
  spec.rail = {{"c2", arc3(gp_Pnt(25, -55, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), radius, std::atan2(60.0, 25.0),
                           std::atan2(60.0, -25.0))}};
  const ShapePtr fraction = sweep(spec);
  spec.rail_point = RailPoint::Plane;
  const ShapePtr plane = sweep(spec);
  CHECK(valid(*plane));
  // The distance from a point to the sweep's faces.
  const auto off = [](const Shape& shape, const gp_Pnt& p) {
    double nearest = std::numeric_limits<double>::infinity();
    for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More(); f.Next()) {
      BRepExtrema_DistShapeShape measure(BRepBuilderAPI_MakeVertex(p).Vertex(), f.Current());
      if (measure.IsDone()) {
        nearest = std::min(nearest, measure.Value());
      }
    }
    return nearest;
  };
  // The sections at the stations (eight along a line guided by one curve)
  // pass through the rail's crossings; the fraction rule's do not.
  for (const double x : {0.0, 6.25, 12.5, 25.0, 37.5, 50.0}) {
    const gp_Pnt q(x, rail_y(x), 0);
    CHECK(off(*plane, q) < 1e-6);
    if (x > 0.0 && x < 50.0 && x != 25.0) {
      CHECK(off(*fraction, q) > 1e-2);
    }
  }
  // V = pi * integral of y^2: the section is the circle scaled to y(x).
  const double s = 25 * 60 + radius * radius * std::asin(5.0 / 13.0);
  const double exact = kPi * ((radius * radius + 55 * 55) * 50 - 2 * 25 * 25 * 25 / 3.0 - 110 * s);
  CHECK_NEAR_TOL(precise_volume(*plane), exact, 1e-4);
  const BoundingBox box = bounding_box(*plane);
  CHECK(near(box.max.Y(), 10, 1e-4) && near(box.min.X(), 0, 1e-6) && near(box.max.X(), 50, 1e-6));
  // Two rails: the second the first mirrored across the path; the profile
  // moves with the first rail's crossing and sizes with the line across.
  spec.rail2 = {{"c3", arc3(gp_Pnt(25, 55, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), radius, std::atan2(-60.0, -25.0),
                            std::atan2(-60.0, 25.0))}};
  const ShapePtr between = sweep(spec);
  for (const double x : {12.5, 25.0, 43.75}) {
    CHECK(off(*between, gp_Pnt(x, rail_y(x), 0)) < 1e-6);
    CHECK(off(*between, gp_Pnt(x, -rail_y(x), 0)) < 1e-6);
  }
  CHECK_NEAR_TOL(precise_volume(*between), exact, 1e-4);
  // A rail that stops short of the path's end.
  spec.rail2.clear();
  spec.rail = {{"c2", line3(gp_Pnt(0, 5, 0), gp_Pnt(40, 8, 0))}};
  CHECK(throws_with([&] { sweep(spec); }, "does not reach the plane square to the path"));
}

void test_pipes() {
  // pipe_path: an 8 mm rod along the bent path.
  PipeSpec rod;
  rod.feature = "F2";
  rod.path = bent_path();
  rod.size = 8;
  const ShapePtr bar = pipe(rod);
  CHECK_NEAR_TOL(precise_volume(*bar), kPi * 16 * (80 + 10 * kPi), 1e-6);
  CHECK(faces_named(*bar, "F2:start") == 1 && faces_named(*bar, "F2:end") == 1);
  CHECK(!bar->find_faces("F2:side0").empty());
  CHECK(edge_names_unique(*bar));
  // pipe_hollow: 10 mm with a 1 mm wall along 50 mm.
  PipeSpec tube;
  tube.feature = "F2";
  tube.path = line_path(0, 50);
  tube.size = 10;
  tube.thickness = 1;
  const ShapePtr hollow = pipe(tube);
  CHECK_NEAR_TOL(precise_volume(*hollow), kPi * (25 - 16) * 50, 1e-6);
  CHECK(faces_named(*hollow, "F2:inner0") == 1);
  // A square of side 10 (mitcad#25: the size is the side); with the wall,
  // minus a square of side 8.
  tube.section = PipeSection::Square;
  CHECK_NEAR_TOL(precise_volume(*pipe(tube)), (100 - 64) * 50, 1e-6);
  CHECK(faces_named(*pipe(tube), "F2:side3") == 1 && faces_named(*pipe(tube), "F2:inner3") == 1);
  // An equilateral triangle of height 10: area h^2 / sqrt 3, a corner up
  // and the path at the middle of its height.
  tube.section = PipeSection::Triangular;
  tube.thickness = 0;
  const ShapePtr prism = pipe(tube);
  CHECK_NEAR_TOL(precise_volume(*prism), 100 / std::sqrt(3.0) * 50, 1e-6);
  CHECK(near(bounding_box(*prism).min.Z(), -5, 1e-6) && near(bounding_box(*prism).max.Z(), 5, 1e-6));
  tube.thickness = 3.5;
  CHECK(throws_with([&] { pipe(tube); }, "wall must be thinner"));
  // Round a circle: a torus without caps; three quarters of it, and a part
  // that runs back through the start.
  PipeSpec round;
  round.feature = "F2";
  round.path = {{"c1", arc3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 30, 0, 2 * kPi)}};
  round.size = 4;
  const ShapePtr ring = pipe(round);
  CHECK_NEAR_TOL(precise_volume(*ring), kPi * 4 * 2 * kPi * 30, 1e-6);
  CHECK(ring->find_faces("F2:start").empty());
  round.extent1 = 0.75;
  CHECK_NEAR_TOL(precise_volume(*pipe(round)), kPi * 4 * 2 * kPi * 30 * 0.75, 1e-6);
  round.extent1 = 0.25;
  round.extent2 = 0.25;
  const ShapePtr arc = pipe(round);
  CHECK_NEAR_TOL(precise_volume(*arc), kPi * 4 * 2 * kPi * 30 * 0.5, 1e-6);
  // From (0, -30) round through (30, 0) to (0, 30); the caps lie in x = 0.
  CHECK(near(bounding_box(*arc).min.X(), 0, 1e-6) && near(bounding_box(*arc).max.X(), 32, 1e-6));
}

void test_coils() {
  // Diameter 40, 3 turns of pitch 10, a 4 mm circle on the diameter: the
  // section lies in the plane through the axis, so V = A 2 pi R 3
  // (mitcad#25: the volumes of the variant models).
  CoilSpec spring;
  spring.feature = "F1";
  spring.diameter = 40;
  spring.revolutions = 3;
  spring.pitch = 10;
  spring.size = 4;
  const ShapePtr helix = coil(spring);
  CHECK_NEAR_TOL(precise_volume(*helix), kPi * 4 * 2 * kPi * 20 * 3, 1e-5);
  CHECK(faces_named(*helix, "F1:start") == 1 && faces_named(*helix, "F1:end") == 1);
  CHECK(!helix->find_faces("F1:side0").empty());
  const BoundingBox box = bounding_box(*helix);
  CHECK(near(box.max.X(), 22, 1e-3) && near(box.min.Z(), -2, 2e-2) && near(box.max.Z(), 32, 2e-2));
  // Clockwise: mirrored in Y.
  spring.clockwise = true;
  const ShapePtr left = coil(spring);
  CHECK_NEAR_TOL(precise_volume(*left), kPi * 4 * 2 * kPi * 20 * 3, 1e-5);
  CHECK(near(mass_properties(*left).center.Y(), -mass_properties(*helix).center.Y(), 1e-4));
  // A square of side 4 inside the diameter: its centre 20 - 2 from the axis.
  spring.clockwise = false;
  spring.section = CoilSection::Square;
  spring.position = CoilPosition::Inside;
  CHECK_NEAR_TOL(precise_volume(*coil(spring)), 16 * 2 * kPi * 18 * 3, 1e-5);
  CHECK(near(bounding_box(*coil(spring)).max.X(), 20, 1e-3));
  // Triangles of height 4 (a third of it from the base to the centre),
  // their radial extent centred on the diameter.
  const double triangle = 16 / std::sqrt(3.0);
  spring.section = CoilSection::TriangularExternal;
  spring.position = CoilPosition::OnCenter;
  CHECK_NEAR_TOL(precise_volume(*coil(spring)), triangle * 2 * kPi * (20 - 4.0 / 6) * 3, 1e-5);
  CHECK(near(bounding_box(*coil(spring)).max.X(), 22, 1e-3));
  spring.section = CoilSection::TriangularInternal;
  CHECK_NEAR_TOL(precise_volume(*coil(spring)), triangle * 2 * kPi * (20 + 4.0 / 6) * 3, 1e-5);
  // A cone widening by 5 degrees: the centre's radius 20 grows by
  // 30 tan 5 degrees on the way.
  spring.section = CoilSection::Circular;
  spring.angle = 5 * kPi / 180;
  CHECK_NEAR_TOL(precise_volume(*coil(spring)),
                 kPi * 4 * 2 * kPi * 3 * (20 + 30 * std::tan(spring.angle) / 2), 1e-4);
  spring.angle = 0;
  // A flat spiral: two turns, growing 6 per turn, a 4 mm circle.
  CoilSpec flat = spring;
  flat.position = CoilPosition::OnCenter;
  flat.spiral = true;
  flat.revolutions = 2;
  flat.pitch = 6;
  const ShapePtr spiral = coil(flat);
  CHECK_NEAR_TOL(precise_volume(*spiral), kPi * 4 * 2 * kPi * 2 * 26, 1e-4);
  CHECK(near(bounding_box(*spiral).max.Z(), 2, 1e-3));
  // Overlapping turns and a section across the axis fail.
  spring.section = CoilSection::Square;
  spring.pitch = 2;
  CHECK(throws_with([&] { coil(spring); }, "pitch is smaller than its section"));
  spring.pitch = 10;
  spring.diameter = 2;
  CHECK(throws_with([&] { coil(spring); }, "reaches its axis"));
  flat.pitch = 3;
  CHECK(throws_with([&] { coil(flat); }, "spiral's pitch"));
}

// The faces of a shape named <feature>:<role>(...).
int faces_with(const Shape& shape, const std::string& prefix) {
  int count = 0;
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      if (name.rfind(prefix, 0) == 0) {
        ++count;
        break;
      }
    }
  }
  return count;
}

// FreeCAD's helices (mitcad#4, #59): a circle of radius 1 eight from the
// axis in a plane through it, 4 turns of pitch 5. The profile stays in
// planes through the axis, so V = A * 2 pi * N * (the mean radius).
void test_helices() {
  HelixSweepSpec spec;
  spec.feature = "F1";
  spec.frame.x_axis = gp_Dir(1, 0, 0);
  spec.frame.y_axis = gp_Dir(0, 0, 1);
  spec.regions = {circle(1, 8, 0, 1)};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = 5;
  spec.revolutions = 4;
  const ShapePtr screw = helix_sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*screw), kPi * 2 * kPi * 4 * 8, 1e-6);
  CHECK(faces_with(*screw, "F1:start(") == 1 && faces_with(*screw, "F1:end(") == 1);
  CHECK(faces_with(*screw, "F1:side(") >= 1);
  // FreeCAD's construction (`freecad`), growing 1 a turn: the mean radius
  // is 10, the end 12 from the axis.
  spec.growth = 1;
  spec.freecad = true;
  const ShapePtr grown = helix_sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*grown), kPi * 2 * kPi * 4 * 10, 1e-5);
  CHECK(faces_with(*grown, "F1:start(") == 1 && faces_with(*grown, "F1:end(") == 1);
  CHECK(faces_with(*grown, "F1:side(") >= 1);
  CHECK(near(bounding_box(*grown).max.Z(), 21, 1e-3) && near(bounding_box(*grown).max.X(), 13, 1e-3));
  // Reversed and narrowing half a millimetre a turn, as FreeCAD 1.1 builds
  // it (volumes and heights measured there): from a profile above the
  // axis' base it goes down the axis; from one at its base FreeCAD's
  // construction takes it up the axis, widening.
  spec.growth = -0.5;
  spec.flip = true;
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, -1));
  spec.regions = {circle(1, 8, 80, 1)};
  const ShapePtr down = helix_sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*down), 558.9404195932443, 1e-6);
  CHECK(near(bounding_box(*down).min.Z(), 59, 0.1));
  spec.regions = {circle(1, 8, 0, 1)};
  const ShapePtr up = helix_sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*up), 710.6111727249141, 1e-6);
  CHECK(near(bounding_box(*up).max.Z(), 21, 0.1));
  spec.growth = std::numeric_limits<double>::infinity();
  CHECK(throws_with([&] { helix_sweep(spec); }, "growth"));
}

// The centre of the face of a shape that has the name, or nothing.
std::optional<gp_Pnt> face_center(const Shape& shape, const std::string& prefix) {
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      if (name.rfind(prefix, 0) == 0) {
        GProp_GProps props;
        BRepGProp::SurfaceProperties(shape.face(i), props);
        return props.CentreOfMass();
      }
    }
  }
  return std::nullopt;
}

// Mitcad's growing helices (mitcad#83): the profile, a circle of radius 1
// eight from the axis in a plane through it, moves out by the growth in
// proportion to the turn, whatever its height, the growth's sign and the
// direction, so V = A * 2 pi * N * (8 + N growth / 2) exactly, and after
// N = 4.25 turns the end cap's centre is a quarter turn round (right-handed
// about the direction of travel), 8 + N growth out and N pitch along.
void test_growing_helices() {
  HelixSweepSpec spec;
  spec.feature = "F1";
  spec.frame.x_axis = gp_Dir(1, 0, 0);
  spec.frame.y_axis = gp_Dir(0, 0, 1);
  spec.pitch = 5;
  spec.revolutions = 4.25;
  for (const double growth : {1.0, -0.5}) {
    for (const double height : {0.0, 80.0}) {
      for (const double way : {1.0, -1.0}) {
        for (const bool left : {false, true}) {
          spec.growth = growth;
          spec.regions = {circle(1, 8, height, 1)};
          spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, way));
          spec.flip = way < 0;
          spec.left_handed = left;
          const ShapePtr helix = helix_sweep(spec);
          const double end = 8 + 4.25 * growth;
          const double top = height + way * 4.25 * 5;
          CHECK_NEAR_TOL(precise_volume(*helix), kPi * 2 * kPi * 4.25 * (8 + end) / 2, 1e-6);
          CHECK(faces_with(*helix, "F1:start(") == 1 && faces_with(*helix, "F1:end(") == 1);
          CHECK(faces_with(*helix, "F1:side(") >= 1);
          const double turn = (left ? -1.0 : 1.0) * way;
          const std::optional<gp_Pnt> cap = face_center(*helix, "F1:end(");
          CHECK(cap && cap->Distance(gp_Pnt(0, turn * end, top)) < 1e-6);
          const BoundingBox box = bounding_box(*helix);
          CHECK(near(box.min.Z(), std::min(height, top) - 1, 1e-3));
          CHECK(near(box.max.Z(), std::max(height, top) + 1, 1e-3));
        }
      }
    }
  }
  // A ring (a circle of radius 2 with a hole of radius 1) and a square
  // beside it, two regions, 12 turns narrowing 0.25 a turn: the ring's mean
  // radius is 6.5, the square's (from 11 to 13) 10.5.
  spec.left_handed = false;
  spec.flip = false;
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  Region ring = circle(1, 8, 0, 2);
  ring.loops.push_back(circle(2, 8, 0, 1).loops.front());
  spec.regions = {ring, rectangle(3, 11, -1, 2, 2)};
  spec.revolutions = 12;
  spec.pitch = 6;
  spec.growth = -0.25;
  const ShapePtr coils = helix_sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*coils), 3 * kPi * 2 * kPi * 12 * 6.5 + 4 * 2 * kPi * 12 * 10.5, 1e-6);
  CHECK(faces_with(*coils, "F1:start(") == 2 && faces_with(*coils, "F1:end(") == 2);
  // A profile across the axis (on XY, the axis Z) moves within its plane
  // as it turns and moves out, so the volume is its area times the rise;
  // after two turns it is 2 out and 10 up.
  Frame flat;
  spec.frame = flat;
  spec.regions = {circle(1, 8, 0, 1)};
  spec.revolutions = 2;
  spec.pitch = 5;
  spec.growth = 1;
  const ShapePtr risen = helix_sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*risen), kPi * 10, 1e-6);
  const std::optional<gp_Pnt> top = face_center(*risen, "F1:end(");
  CHECK(top && top->Distance(gp_Pnt(10, 0, 10)) < 1e-6);
  spec.frame.x_axis = gp_Dir(1, 0, 0);
  spec.frame.y_axis = gp_Dir(0, 0, 1);
  // A profile centred on the axis moves out across its plane's normal.
  spec.left_handed = false;
  spec.flip = false;
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.regions = {circle(1, 0, 0, 1)};
  spec.growth = 3;
  spec.revolutions = 1;
  spec.pitch = 10;
  CHECK_NEAR_TOL(precise_volume(*helix_sweep(spec)), kPi * 2 * kPi * 1.5, 1e-6);
  // Narrowing into the axis fails.
  spec.regions = {circle(1, 8, 0, 1)};
  spec.growth = -2;
  spec.revolutions = 4;
  CHECK(throws_with([&] { helix_sweep(spec); }, "narrows into its axis"));
}

// Flat ends (mitcad#235). A 1 x h rectangle from 5 to 6 out (on XZ, the
// axis Z), pitch 4, 2 turns, then a transition of a quarter turn and a flat
// part of three quarters: 3 turns in all, and the rise is 8 + 4 / 2 pi *
// (pi / 2) / 2 = 8.5. Every section in a plane through the axis is the
// union of the profile's copies there, so the volume is 5.5 times the
// integral of their union's height over a turn.
void test_helices_with_flat_ends() {
  HelixSweepSpec spec;
  spec.feature = "F1";
  spec.frame.x_axis = gp_Dir(1, 0, 0);
  spec.frame.y_axis = gp_Dir(0, 0, 1);
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = 4;
  spec.revolutions = 2;
  spec.end_transition = kPi / 2;
  spec.end_flat = 1.5 * kPi;
  // Half a millimetre high: the copies do not meet.
  spec.regions = {rectangle(1, 5, 0, 1, 0.5)};
  const ShapePtr low = helix_sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*low), 0.5 * 5.5 * 6 * kPi, 1e-6);
  CHECK(near(bounding_box(*low).max.Z(), 9.0, 1e-6));
  const std::optional<gp_Pnt> cap = face_center(*low, "F1:end(");
  CHECK(cap && cap->Distance(gp_Pnt(5.5, 0, 8.75)) < 1e-6);
  // Two millimetres high: the flat part passes through the last turn
  // (pieces fused). Over the first quarter turn the three copies are apart
  // (6); then the flat copy at 8.5 meets the one a turn before it once that
  // reaches 8.5 - 2, at 1.25 pi: 2 + (10.5 - (4 + 2 theta / pi)).
  spec.regions = {rectangle(1, 5, 0, 1, 2)};
  const ShapePtr tall = helix_sweep(spec);
  CHECK(BRepCheck_Analyzer(tall->occt()).IsValid());
  CHECK_NEAR_TOL(precise_volume(*tall), 5.5 * (6 * 1.25 * kPi + 3.9375 * kPi), 1e-6);
  CHECK(near(bounding_box(*tall).max.Z(), 10.5, 1e-6));
  CHECK(faces_with(*tall, "F1:start(") == 1 && faces_with(*tall, "F1:end(") == 1);
  // A start of the same kind and a growth that follows the rise: 1 out per
  // pitch of rise, so the end cap is 8.5 / 4 further out, and the flat
  // parts stay where they are (the start cap's centre a full turn round
  // from where the rise begins).
  spec.regions = {rectangle(1, 5, 0, 1, 0.5)};
  spec.start_flat = 1.5 * kPi;
  spec.start_transition = kPi / 2;
  spec.end_transition = 0;
  spec.end_flat = 0;
  spec.growth = 1;
  const ShapePtr grown = helix_sweep(spec);
  const std::optional<gp_Pnt> end = face_center(*grown, "F1:end(");
  CHECK(end && end->Distance(gp_Pnt(5.5 + 8.5 / 4, 0, 8.75)) < 1e-6);
  // The volume: the profile's area times its centre's path round the
  // axis, 3 turns at 5.5 and then, from the transition on, moving out with
  // the rise.
  CHECK(precise_volume(*grown) > 0.5 * 5.5 * 6 * kPi);
  spec.growth = 0;
  spec.freecad = true;
  CHECK(throws_with([&] { helix_sweep(spec); }, "Mitcad's construction"));
  spec.freecad = false;
  spec.start_flat = -1;
  CHECK(throws_with([&] { helix_sweep(spec); }, "must not be negative"));
}

// A polygon c1..cn through the points (sketch coordinates).
Region polygon(const std::vector<gp_Pnt2d>& points) {
  Region region;
  region.name = "r{polygon}";
  Loop loop;
  for (std::size_t i = 0; i < points.size(); ++i) {
    Segment segment;
    segment.name = "c" + std::to_string(i + 1);
    segment.start = points[i];
    segment.end = points[(i + 1) % points.size()];
    loop.segments.push_back(segment);
  }
  region.loops.push_back(loop);
  return region;
}

// An M3 thread's groove (pitch 0.5005) with rounded roots and crests, on
// the XZ plane as x the radius and y the height: its base on radius r1 =
// 1.2294 from z = zm - 0.2368 to zm + 0.2368, its crest on r2 (1.5).
Region m3_groove(double zm, double r2 = 1.5) {
  const double r1 = 1.2294, p1 = 0.0313, p2 = 0.0625, zb = zm + 0.2368;
  const double c30 = std::sqrt(3.0) / 2.0;
  const gp_Pnt2d a(r1 + 0.5 * p1, zb - c30 * p1);
  const double t = (r2 - 0.5 * p2 - r1 - 0.5 * p1) / c30;
  const gp_Pnt2d b(a.X() + c30 * t, a.Y() - 0.5 * t);
  const double zc = b.Y() - c30 * p2;
  const auto mirror = [&](const gp_Pnt2d& p) { return gp_Pnt2d(p.X(), 2 * zm - p.Y()); };
  const auto line = [](const char* name, const gp_Pnt2d& s, const gp_Pnt2d& e) {
    Segment segment;
    segment.name = name;
    segment.start = s;
    segment.end = e;
    return segment;
  };
  const auto arc = [](const char* name, const gp_Pnt2d& c, double radius, double from, double to) {
    Segment segment;
    segment.name = name;
    segment.kind = SegmentKind::Arc;
    segment.center = c;
    segment.radius = radius;
    segment.start_angle = from * kPi / 180;
    segment.end_angle = to * kPi / 180;
    return segment;
  };
  Region region;
  region.name = "r{m3}";
  Loop loop;
  loop.segments = {line("c1", gp_Pnt2d(r1, 2 * zm - zb), gp_Pnt2d(r1, zb)),
                   arc("c2", gp_Pnt2d(r1 + p1, zb), p1, 180, 240),
                   line("c3", a, b),
                   arc("c4", gp_Pnt2d(r2 - p2, zc), p2, 0, 60),
                   line("c5", gp_Pnt2d(r2, zc), gp_Pnt2d(r2, 2 * zm - zc)),
                   arc("c6", gp_Pnt2d(r2 - p2, 2 * zm - zc), p2, -60, 0),
                   line("c7", mirror(b), mirror(a)),
                   arc("c8", gp_Pnt2d(r1 + p1, 2 * zm - zb), p1, 120, 180)};
  region.loops.push_back(loop);
  return region;
}

// A ring about Z from radius `inner` to `outer`, from z0 to z1 (a rod
// without `inner`).
ShapePtr ring_body(double inner, double outer, double z0, double z1) {
  Region section = circle(1, 0, 0, outer);
  if (inner > 0) {
    section.loops.push_back(circle(2, 0, 0, inner).loops.front());
  }
  return extrude("F1", {section}, z0, z1);
}

// Helices whose profile has a side on a cylinder of the body they are cut
// from or joined to (mitcad#60), about Z: the result is one valid solid
// of the body's volume less or plus the helix's (the helix swept without
// the body) within `tolerance`, or, without one (0), less than the body.
void test_helices_on_cylinders() {
  struct Case {
    const char* what;
    BooleanOp op;
    ShapePtr body;
    Frame frame;
    Region profile;
    double pitch;
    double revolutions;
    double tolerance;
  };
  Frame xz; // the sketch's x the radius, its y the height
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  Frame across; // the sketch's x the height, its y towards -Y
  across.x_axis = gp_Dir(0, 0, 1);
  across.y_axis = gp_Dir(0, 1, 0);
  // A triangle with its base on radius 5 from z = 4, `base` long, its apex
  // `apex` out from it.
  const auto triangle = [](double base, double apex) {
    return polygon({{5, 4}, {5 + apex, 4 + base / 2}, {5, 4 + base}});
  };
  const ShapePtr nut = ring_body(5, 10, 0, 20);
  const ShapePtr rod = ring_body(0, 5, 0, 20);
  const Case cases[] = {
      // An internal thread that starts and ends inside the hole's length.
      {"internal thread", BooleanOp::Cut, nut, xz, triangle(1, 0.6), 1.5, 6, 1e-6},
      // Turns that touch: the base as long as the pitch.
      {"touching turns", BooleanOp::Cut, nut, xz, triangle(1.5, 0.6), 1.5, 6, 1e-5},
      // External threads cut into a rod and joined to it.
      {"external thread", BooleanOp::Cut, rod, xz, triangle(1, -0.6), 1.5, 6, 1e-6},
      {"joined thread", BooleanOp::Join, rod, xz, triangle(1, 0.6), 1.5, 6, 1e-6},
      {"joined touching turns", BooleanOp::Join, rod, xz, triangle(1.5, 0.6), 1.5, 6, 1e-5},
      // Touching turns from the end of a short rod with a hole to beyond
      // its other end, sketched across the axis: the sliver joined beyond
      // the rod's end adds to the volume.
      {"joined from the end", BooleanOp::Join, ring_body(2.5, 6.25, 0, 5.5), across,
       polygon({{-2, -6.25}, {-1, -7.65}, {0, -6.25}}), 2, 5, 1e-4},
      // A round groove touching the hole.
      {"round groove", BooleanOp::Cut, nut, xz, circle(1, 5.5, 6, 0.5), 1.5, 6, 1e-6},
      // An M3 internal thread inside a hole, 6 and 18 turns.
      {"M3 thread", BooleanOp::Cut, ring_body(1.2294, 4, 0, 6), xz, m3_groove(0.75), 0.5005, 6, 1e-6},
      {"long M3 thread", BooleanOp::Cut, ring_body(1.2294, 4, 0, 12), xz, m3_groove(0.75), 0.5005, 18, 1e-6},
      // The groove through a ring as thick as it, from one end of the
      // ring past its other end.
      {"M3 ring", BooleanOp::Cut, ring_body(1.2294, 1.5, 0, 2), xz, m3_groove(-0.25), 0.5005, 3 / 0.5005, 0},
      // An external M3 thread whose crest is drawn 0.17 um outside the rod
      // it is cut into, from beyond one end of the rod past the other.
      {"M3 crest off the rod", BooleanOp::Cut, ring_body(0, 1.5, 0, 3.35), xz, m3_groove(-0.25, 1.50017), 0.5005,
       4.35 / 0.5005, 0},
  };
  for (const Case& c : cases) {
    HelixSweepSpec spec;
    spec.feature = "F2";
    spec.frame = c.frame;
    spec.regions = {c.profile};
    spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
    spec.pitch = c.pitch;
    spec.revolutions = c.revolutions;
    const double body = precise_volume(*c.body);
    const double helix = precise_volume(*helix_sweep(spec));
    const double expected = body + (c.op == BooleanOp::Cut ? -helix : helix);
    spec.bodies = {c.body};
    try {
      const BooleanResult result = boolean(c.op, {c.body.get()}, *helix_sweep(spec));
      const bool one = result.pieces.size() == 1;
      // Valid in each of several checks: the checker's threads shared an
      // edge's curve between its two faces and called the round groove's
      // faces unorientable in some runs (mitcad#203, OCCT port patch 0033).
      bool valid = one;
      for (int check = 0; valid && check < 4; ++check) {
        valid = detail::is_valid(result.pieces.front().shape->occt());
      }
      const double volume = one ? precise_volume(*result.pieces.front().shape) : 0.0;
      const bool right = c.tolerance > 0 ? near(volume, expected, c.tolerance) : volume > 0 && volume < body;
      if (!one || !valid || !right) {
        std::fprintf(stderr, "helix on a cylinder, %s: %zu pieces, %s, volume %.9g, expected %.9g\n", c.what,
                     result.pieces.size(), valid ? "valid" : "invalid", volume, expected);
        ++failures;
      }
    } catch (const std::exception& error) {
      std::fprintf(stderr, "helix on a cylinder, %s: %s\n", c.what, error.what());
      ++failures;
    }
  }
  // Turns that overlap: a triangle whose base on the cylinder is longer
  // than the pitch, joined to a nut (an internal thread) and cut from a
  // rod. The tool is the union of its turns: at radius r they cover N w(r)
  // along the axis where the triangle's width w(r) is at most the pitch,
  // else (N - 1) pitch + w(r), N the (whole) turns.
  struct Overlap {
    const char* what;
    BooleanOp op;
    ShapePtr body;
    double base_radius;
    double apex_radius;
    double base;
    double pitch;
    double turns;
  };
  const Overlap overlaps[] = {
      {"overlapping turns joined", BooleanOp::Join, ring_body(17, 23, 0, 15), 17, 15.1, 2.194, 2, 8},
      {"overlapping turns cut", BooleanOp::Cut, rod, 5, 4.4, 1.65, 1.5, 6},
  };
  for (const Overlap& o : overlaps) {
    HelixSweepSpec spec;
    spec.feature = "F2";
    spec.frame = xz;
    spec.regions = {polygon({{o.base_radius, 4}, {o.apex_radius, 4 + o.base / 2}, {o.base_radius, 4 + o.base}})};
    spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
    spec.pitch = o.pitch;
    spec.revolutions = o.turns;
    spec.bodies = {o.body};
    double tool = 0;
    constexpr int kSteps = 100000;
    const double depth = o.base_radius - o.apex_radius;
    for (int i = 0; i < kSteps; ++i) {
      const double t = (i + 0.5) / kSteps;
      const double r = o.apex_radius + depth * t;
      const double w = o.base * t;
      const double covered = w <= o.pitch ? o.turns * w : (o.turns - 1) * o.pitch + w;
      tool += 2 * kPi * r * covered * std::abs(depth) / kSteps;
    }
    const double body = precise_volume(*o.body);
    const double expected = o.op == BooleanOp::Cut ? body - tool : body + tool;
    try {
      const BooleanResult result = boolean(o.op, {o.body.get()}, *helix_sweep(spec));
      const bool one = result.pieces.size() == 1;
      const bool valid = one && detail::is_valid(result.pieces.front().shape->occt());
      const double volume = one ? precise_volume(*result.pieces.front().shape) : 0.0;
      if (!valid || !near(volume, expected, 1e-5)) {
        std::fprintf(stderr, "helix on a cylinder, %s: %zu pieces, %s, volume %.9g, expected %.9g\n", o.what,
                     result.pieces.size(), valid ? "valid" : "invalid", volume, expected);
        ++failures;
      }
    } catch (const std::exception& error) {
      std::fprintf(stderr, "helix on a cylinder, %s: %s\n", o.what, error.what());
      ++failures;
    }
  }
}

Frame xy_at(double z) {
  Frame frame;
  frame.origin = gp_Pnt(0, 0, z);
  return frame;
}

void test_lofts() {
  // loft_two_squares: a 40 mm square at z = 0 to a 20 mm one at z = 30.
  LoftSpec frustum;
  frustum.feature = "F5";
  LoftSection bottom;
  bottom.frame = xy_at(0);
  bottom.region = rectangle(1, -20, -20, 40, 40);
  LoftSection top;
  top.frame = xy_at(30);
  top.region = rectangle(5, -10, -10, 20, 20);
  frustum.sections = {bottom, top};
  const ShapePtr pyramid = loft(frustum);
  CHECK_NEAR_TOL(precise_volume(*pyramid), 30.0 / 3 * (1600 + 400 + 800), 1e-6);
  CHECK(faces_named(*pyramid, start_cap("F5", 1)) == 1 && faces_named(*pyramid, end_cap("F5", 5)) == 1);
  CHECK(faces_named(*pyramid, side("F5", 1, 0)) == 1);
  CHECK(edge_names_unique(*pyramid));

  // loft_three_circles: a smooth radius through 20, 10, 20 is convex, so
  // inside the two cones (ruled) and outside the waist's cylinder; the
  // parabola through them gives pi 7466.7.
  LoftSpec waist;
  waist.feature = "F5";
  for (const auto& [z, r] : {std::pair{0.0, 20.0}, {20.0, 10.0}, {40.0, 20.0}}) {
    LoftSection section;
    section.frame = xy_at(z);
    section.region = circle(1, 0, 0, r);
    waist.sections.push_back(section);
  }
  const double cones = kPi * 20 / 3 * (400 + 200 + 100) * 2;
  CHECK_NEAR_TOL(precise_volume(*loft(waist)), kPi * 22400 / 3, 1e-6);
  waist.ruled = true;
  CHECK_NEAR_TOL(precise_volume(*loft(waist)), cones, 1e-6);

  // loft_centerline: two 20 mm circles along a straight centre line.
  LoftSpec cylinder;
  cylinder.feature = "F5";
  LoftSection low;
  low.frame = xy_at(0);
  low.region = circle(1, 0, 0, 10);
  LoftSection high = low;
  high.frame = xy_at(40);
  cylinder.sections = {low, high};
  cylinder.centerline = {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(0, 0, 40))}};
  const ShapePtr along = loft(cylinder);
  CHECK_NEAR_TOL(precise_volume(*along), kPi * 100 * 40, 1e-6);
  CHECK(faces_named(*along, "F5:start(r{c1})") == 1);

  // A point at the end: a cone.
  LoftSpec cone;
  cone.feature = "F5";
  LoftSection tip;
  tip.kind = LoftSection::Kind::Point;
  tip.point = gp_Pnt(0, 0, 30);
  cone.sections = {low, tip};
  const ShapePtr pointed = loft(cone);
  CHECK_NEAR_TOL(precise_volume(*pointed), kPi * 100 * 30 / 3, 1e-6);
  CHECK(pointed->find_faces("F5:end(r{c1})").empty());

  // From the top face of a block to a sketch circle above it.
  const ShapePtr block = extrude("F2", {rectangle(1, -10, -10, 20, 20)}, 0, 10);
  LoftSpec onto;
  onto.feature = "F7";
  LoftSection face;
  face.kind = LoftSection::Kind::Face;
  face.body = block;
  face.face = end_cap("F2", 1);
  LoftSection square = top;
  square.frame = xy_at(40);
  onto.sections = {face, square};
  const ShapePtr from_face = loft(onto);
  CHECK_NEAR_TOL(precise_volume(*from_face), 30 * 400, 1e-6);
  CHECK(faces_named(*from_face, "F7:start(" + end_cap("F2", 1) + ")") == 1);
  CHECK(faces_named(*from_face, "F7:side(" + edge(side("F2", 1, 0), end_cap("F2", 1)) + ")") == 1);

  // Closed and ruled: four squares round the Z axis make a square ring.
  LoftSpec ring;
  ring.feature = "F5";
  ring.closed = true;
  ring.ruled = true;
  for (int i = 0; i < 4; ++i) {
    const double a = kPi / 2 * i;
    LoftSection section;
    section.frame.origin = gp_Pnt(30 * std::cos(a), 30 * std::sin(a), 0);
    section.frame.x_axis = gp_Dir(std::cos(a), std::sin(a), 0);
    section.frame.y_axis = gp_Dir(0, 0, 1);
    section.region = rectangle(1, -5, -5, 10, 10);
    ring.sections.push_back(section);
  }
  const ShapePtr closed = loft(ring);
  CHECK(closed->find_faces(start_cap("F5", 1)).empty());
  // Four ruled pieces, each 30 * 10 * 10 (the Jacobian of the linear blend
  // between squares a quarter turn apart is the distance from the axis).
  CHECK_NEAR_TOL(precise_volume(*closed), 12000, 1e-6);
  CHECK(closed->face_count() == 16 && edge_names_unique(*closed));
  ring.ruled = false;
  const ShapePtr round_ring = loft(ring);
  // Periodic piecewise cubic through four squares a quarter turn apart (the
  // corners follow Catmull-Rom curves, the sections matched corner for
  // corner): 16400 against 19200 for a C2 interpolation and 12000 ruled.
  CHECK_NEAR_TOL(precise_volume(*round_ring), 16400, 1e-6);
  {
    // Smooth where it closes: the surface's v derivatives at its two ends.
    const BRepAdaptor_Surface surface(round_ring->face(0));
    gp_Pnt p0, p1;
    gp_Vec du0, dv0, du1, dv1;
    const double u = (surface.FirstUParameter() + surface.LastUParameter()) / 2;
    surface.D1(u, surface.FirstVParameter(), p0, du0, dv0);
    surface.D1(u, surface.LastVParameter(), p1, du1, dv1);
    CHECK(p0.Distance(p1) < 1e-9 && dv0.Angle(dv1) < 1e-9);
  }
  CHECK(round_ring->face_count() == 4 && faces_named(*round_ring, side("F5", 1, 2)) == 1);
  CHECK(edge_names_unique(*round_ring));
  // Four circles round a ring: the stored closed loft of four circles of 10 mm
  // 30 mm from the axis has the volume 12880.53 (the sections' points follow
  // the periodic Catmull-Rom curves; the torus through them is 14804.41).
  LoftSpec tube;
  tube.feature = "F5";
  tube.closed = true;
  for (int i = 0; i < 4; ++i) {
    LoftSection section = ring.sections[static_cast<std::size_t>(i)];
    section.region = circle(1, 0, 0, 5);
    tube.sections.push_back(section);
  }
  const ShapePtr torus_like = loft(tube);
  CHECK_NEAR_TOL(precise_volume(*torus_like), 12880.53, 1e-6);
  CHECK(torus_like->face_count() == 1 && faces_named(*torus_like, "F5:side(c1)") == 1);
  // The same circles, open: three quarters of the ring, 9503.3178 (the first
  // and last pieces are the parabolas that arrive with the inner tangents).
  LoftSpec arc = tube;
  arc.closed = false;
  const ShapePtr open_ring = loft(arc);
  CHECK_NEAR_TOL(precise_volume(*open_ring), 9503.3178, 1e-6);
  CHECK(faces_named(*open_ring, "F5:side(c1)") == 1 && faces_named(*open_ring, "F5:start(r{c1})") == 1 && faces_named(*open_ring, "F5:end(r{c1})") == 1);

  // A surface loft: the side faces of a cylinder between two circles, a
  // shell without caps.
  LoftSpec sheet = cylinder;
  sheet.centerline.clear();
  sheet.solid = false;
  const ShapePtr surface = loft(sheet);
  GProp_GProps skin;
  BRepGProp::SurfaceProperties(surface->occt(), skin);
  CHECK_NEAR_TOL(skin.Mass(), 2 * kPi * 10 * 40, 1e-6);
  CHECK(surface->face_count() == 1 && faces_named(*surface, "F5:side(c1)") == 1);
  CHECK(surface->find_faces("F5:start(r{c1})").empty() && !TopExp_Explorer(surface->occt(), TopAbs_SOLID).More());

  // A section with holes is not lofted.
  LoftSpec holed = frustum;
  holed.sections[0].region.loops.push_back(circle(9, 0, 0, 5).loops.front());
  CHECK(throws_with([&] { loft(holed); }, "unsupported: a loft section 1 with holes"));
}

// The bracket of ui_rib: a 60 x 40 x 5 base and a 5 x 40 x 40 plate on it.
ShapePtr bracket() {
  const ShapePtr base = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 5);
  const ShapePtr plate = extrude("F3", {rectangle(5, 0, 0, 5, 40)}, 5, 45);
  const BooleanResult joined = boolean(BooleanOp::Join, {base.get()}, *plate);
  return joined.pieces.at(0).shape;
}

// The midplane y = 20 of the bracket, sketch x along X and y along Z.
Frame midplane() {
  Frame frame;
  frame.origin = gp_Pnt(0, 20, 0);
  frame.x_axis = gp_Dir(1, 0, 0);
  frame.y_axis = gp_Dir(0, 0, 1);
  return frame;
}

void test_ribs() {
  const ShapePtr body = bracket();
  CHECK_NEAR(volume(*body), 20000);
  // ui_rib: the line (5, 35) - (35, 5) on the midplane, 4 thick, to the
  // corner: a triangle 30 x 30 / 2 * 4.
  RibSpec spec;
  spec.feature = "F6";
  spec.frame = midplane();
  spec.chains = {{{"c1", line3(gp_Pnt(5, 20, 35), gp_Pnt(35, 20, 5))}}};
  spec.thickness = 4;
  spec.bodies = {body};
  spec.flip = true;
  const ShapePtr web = rib(spec);
  CHECK_NEAR(volume(*web), 30.0 * 30.0 / 2 * 4);
  CHECK(faces_named(*web, "F6:side(c1)") == 1 && faces_named(*web, "F6:wall1") == 1);
  const BooleanResult joined = boolean(BooleanOp::Join, {body.get()}, *web);
  CHECK_NEAR(volume(*joined.pieces.at(0).shape), 20000 + 30.0 * 30.0 / 2 * 4);
  // Away from the corner nothing closes it off.
  spec.flip = false;
  CHECK(throws_with([&] { rib(spec); }, "does not end at faces of the bodies"));
  // A depth instead: a parallelogram 10 deep along the line.
  spec.depth = 10.0;
  spec.location = ThicknessLocation::Side1;
  const ShapePtr deep = rib(spec);
  CHECK_NEAR(volume(*deep), std::hypot(30.0, 30.0) * 10 * 4);
  // Side 1 is along the sketch normal, -Y.
  CHECK(near(bounding_box(*deep).min.Y(), 16, 1e-9) && near(bounding_box(*deep).max.Y(), 20, 1e-9));

  // Extend Ends (mitcad#1746): the line (10, 30) - (30, 10) stops 5 short
  // of the plate and the base along itself. As drawn, up to the next faces,
  // its tips at right angles to it cut two corners off the triangle: 400 *
  // 4; continued along it, the ends fill up to the slanted faces: the whole
  // triangle.
  RibSpec shorter;
  shorter.feature = "F8";
  shorter.frame = midplane();
  shorter.chains = {{{"c1", line3(gp_Pnt(10, 20, 30), gp_Pnt(30, 20, 10))}}};
  shorter.thickness = 4;
  shorter.bodies = {body};
  shorter.flip = true;
  CHECK_NEAR(volume(*rib(shorter)), 400.0 * 4);
  shorter.extend_ends = true;
  const ShapePtr filled = rib(shorter);
  CHECK_NEAR(volume(*filled), 30.0 * 30.0 / 2 * 4);
  CHECK(filled->notes().empty());
  const BooleanResult whole = boolean(BooleanOp::Join, {body.get()}, *filled);
  CHECK(whole.pieces.size() == 1);
  CHECK_NEAR(volume(*whole.pieces.at(0).shape), 20000 + 30.0 * 30.0 / 2 * 4);
  // The input bodies are left as they were.
  CHECK_NEAR(volume(*body), 20000);
  // End Offset -1: the faces each end stops at move 1 back along the line,
  // so the two corner triangles' legs (5 sqrt(2), along the line and the
  // tip) are 1 shorter: 400 + 2 * (5 sqrt(2) - 1)^2 / 2.
  shorter.end_offset = -1.0;
  CHECK_NEAR(volume(*rib(shorter)), (451.0 - 10 * std::sqrt(2.0)) * 4);
  shorter.end_offset = 0.0;
  // With a depth of 3 on the base alone, the start (up towards where the
  // plate is not) meets nothing and stays, with a note; the end fills down
  // to the base's top: 4 * (20 sqrt(2) * 3 + the strip 3 wide from the tip
  // to z = 5, 15 sqrt(2) - 4.5).
  const ShapePtr base_only = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 5);
  shorter.bodies = {base_only};
  shorter.depth = 3.0;
  const ShapePtr half = rib(shorter);
  CHECK_NEAR(volume(*half), 4 * (75 * std::sqrt(2.0) - 4.5));
  CHECK(half->notes().size() == 1);
  CHECK(!half->notes().empty() && half->notes().front().find("start") != std::string::npos &&
        half->notes().front().find("meets no face of the bodies") != std::string::npos);
  // Direction (mitcad#1746): a 100 x 40 x 10 base with two cylinders of
  // radius 10 on it, at x = 20 (to z = 60) and x = 80 (to z = 30); on the
  // plane y = 20 a slanted line from the first's outline at z = 50 to the
  // second's top edge, (30, 50) - (70, 30), grows straight down (-Z) up to
  // the next faces: the trapezoid under it down to the base, 40 * 30 * 4.
  // With Extend Ends the ends fill the slivers between the tips (x = 30 and
  // 70) and the cylinders' curved faces, across the 4 mm thickness: in all
  // 60 * the integral of 10 - sqrt(100 - t^2) for t from -2 to 2.
  const ShapePtr floor = extrude("F2", {rectangle(1, 0, 0, 100, 40)}, 0, 10);
  const ShapePtr tall = extrude("F3", {circle(5, 20, 20, 10)}, 10, 60);
  const ShapePtr low_boss = extrude("F4", {circle(6, 80, 20, 10)}, 10, 30);
  const ShapePtr one = boolean(BooleanOp::Join, {floor.get()}, *tall).pieces.at(0).shape;
  const ShapePtr bosses = boolean(BooleanOp::Join, {one.get()}, *low_boss).pieces.at(0).shape;
  CHECK_NEAR(volume(*bosses), 40000 + 7000 * kPi);
  RibSpec down;
  down.feature = "F9";
  down.frame = midplane();
  down.chains = {{{"c1", line3(gp_Pnt(30, 20, 50), gp_Pnt(70, 20, 30))}}};
  down.thickness = 4;
  down.bodies = {bosses};
  down.direction = gp_Dir(0, 0, -1);
  CHECK_NEAR(volume(*rib(down)), 40.0 * 30 * 4);
  down.extend_ends = true;
  const ShapePtr between = rib(down);
  const double slivers = 60 * (40 - 2 * std::sqrt(96.0) - 100 * std::asin(0.2));
  CHECK_NEAR(volume(*between), 40.0 * 30 * 4 + slivers);
  CHECK(between->notes().empty());
  const BooleanResult braced = boolean(BooleanOp::Join, {bosses.get()}, *between);
  CHECK(braced.pieces.size() == 1);
  CHECK_NEAR(volume(*braced.pieces.at(0).shape), 40000 + 7000 * kPi + 40.0 * 30 * 4 + slivers);
  // Flipped it grows up, where nothing closes it off.
  down.flip = true;
  CHECK(throws_with([&] { rib(down); }, "does not end at faces of the bodies"));
  down.flip = false;
  // A direction along the line's chord gives the rib no width.
  down.direction = gp_Dir(gp_Vec(gp_Pnt(30, 20, 50), gp_Pnt(70, 20, 30)));
  CHECK(throws_with([&] { rib(down); }, "the rib's direction runs along its curves' chord"));

  // A web's curves are not continued.
  RibSpec web_ends;
  web_ends.feature = "F9";
  web_ends.web = true;
  web_ends.thickness = 2;
  web_ends.depth = 10.0;
  web_ends.extend_ends = true;
  web_ends.chains = {{{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(40, 0, 0))}}};
  CHECK(throws_with([&] { rib(web_ends); }, "a web's curves end where they are drawn"));

  // Webs: two crossing lines 2 thick, 10 deep from the XY plane.
  RibSpec webs;
  webs.feature = "F7";
  webs.web = true;
  webs.thickness = 2;
  webs.depth = 10.0;
  webs.chains = {{{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(40, 0, 0))}},
                 {{"c2", line3(gp_Pnt(20, -20, 0), gp_Pnt(20, 20, 0))}}};
  const ShapePtr cross = rib(webs);
  CHECK_NEAR(volume(*cross), 800 + 800 - 2 * 2 * 10);
  CHECK(faces_named(*cross, "F7:start(c1)") >= 1 && !cross->find_faces("F7:wall1(c2)").empty());
  // An arc web, side 1 (inside it).
  webs.chains = {{{"c3", arc3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 20, 0, kPi / 2)}}};
  webs.location = ThicknessLocation::Side1;
  CHECK_NEAR(volume(*rib(webs)), kPi / 4 * (400 - 324) * 10);
  // Up to the next face: from the base's top (z = 5) a web under a roof at
  // z = 25..30 reaches it.
  const ShapePtr roof = extrude("F4", {rectangle(9, 0, 0, 60, 40)}, 25, 30);
  webs.chains = {{{"c1", line3(gp_Pnt(10, 20, 5), gp_Pnt(50, 20, 5))}}};
  webs.frame = xy_at(5);
  webs.depth.reset();
  webs.location = ThicknessLocation::Symmetric;
  webs.bodies = {body, roof};
  CHECK_NEAR(volume(*rib(webs)), 40 * 2 * 20);
  webs.flip = true;
  CHECK(throws_with([&] { rib(webs); }, "lies inside the bodies"));
}

// A B-spline as a path curve.
ModelCurve bspline3(const occ::handle<Geom_BSplineCurve>& curve) {
  ModelCurve model;
  model.kind = ModelCurveKind::BSpline;
  model.degree = curve->Degree();
  for (int i = 1; i <= curve->NbPoles(); ++i) {
    model.poles.push_back(curve->Pole(i));
    if (curve->IsRational()) {
      model.weights.push_back(curve->Weight(i));
    }
  }
  for (const double k : curve->KnotSequence()) {
    model.knots.push_back(k);
  }
  return model;
}

// A helix on the cylinder of `radius` about +Z from (radius, 0, 0),
// counter-clockwise, `turns` turns rising `pitch` each, as a B-spline.
ModelCurve helix3(double radius, double pitch, double turns) {
  const occ::handle<Geom_Surface> surface =
      new Geom_CylindricalSurface(gp_Ax3(gp::Origin(), gp::DZ(), gp::DX()), radius);
  const gp_Vec2d direction(2 * kPi, pitch);
  const occ::handle<Geom2d_Line> line = new Geom2d_Line(gp_Pnt2d(0, 0), gp_Dir2d(direction));
  TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(line, surface, 0.0, turns * direction.Magnitude()).Edge();
  BRepLib::BuildCurves3d(edge, 1.0e-8, GeomAbs_C2, 14, 400);
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  return bspline3(GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last)));
}

// A face through the Z axis from z = 0 to `length`, made of the lines
// through it of half-length `half` at z, turned by `twist` z / length
// from the X axis: its normal along the axis turns with z.
ShapePtr twisted_face(double length, double half, double twist) {
  BRepOffsetAPI_ThruSections loft(false, false, 1.0e-7);
  constexpr int kSections = 17;
  for (int i = 0; i < kSections; ++i) {
    const double z = length * i / (kSections - 1);
    const double a = twist * z / length;
    const gp_Vec d(half * std::cos(a), half * std::sin(a), 0);
    const gp_Pnt c(0, 0, z);
    loft.AddWire(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(c.Translated(-d), c.Translated(d)).Edge()).Wire());
  }
  loft.Build();
  TopoDS_Face face;
  for (TopExp_Explorer f(loft.Shape(), TopAbs_FACE); f.More(); f.Next()) {
    face = TopoDS::Face(f.Current());
  }
  return std::make_shared<Shape>(face, std::vector<Shape::NamedFace>{{face, {"F3:twisted"}}});
}

// Guide surfaces (mitcad#157): the profile keeps its place in the frame
// of the path's tangent and the surface's normal.
void test_guide_surfaces() {
  // A thread-like rectangle in the XZ plane, beside a cylinder of radius
  // 10, swept along a helix on it guided by the cylinder: a screw motion,
  // so V = A * 2 pi * N * (the mean radius) = 4 * 2 pi * 2 * 11, the
  // helix sweep's solid.
  const ShapePtr cylinder = extrude("F2", {circle(1, 0, 0, 10)}, -1, 20);
  SweepSpec thread;
  thread.feature = "F5";
  thread.frame.x_axis = gp_Dir(1, 0, 0);
  thread.frame.y_axis = gp_Dir(0, 0, 1);
  thread.regions = {rectangle(1, 10, -1, 2, 2)};
  thread.path = {{"E{F2:side(c1)}", helix3(10, 5, 2)}};
  thread.guide = {{cylinder, "F2:side(c1)"}};
  const ShapePtr guided = sweep(thread);
  CHECK(BRepCheck_Analyzer(guided->occt()).IsValid());
  CHECK_NEAR_TOL(precise_volume(*guided), 4 * 2 * kPi * 2 * 11, 1e-6);
  HelixSweepSpec screw;
  screw.feature = "F6";
  screw.frame = thread.frame;
  screw.regions = thread.regions;
  screw.axis = gp_Ax1(gp::Origin(), gp::DZ());
  screw.pitch = 5;
  screw.revolutions = 2;
  CHECK_NEAR_TOL(precise_volume(*guided), precise_volume(*helix_sweep(screw)), 1e-6);
  CHECK(faces_with(*guided, "F5:start(") == 1 && faces_with(*guided, "F5:end(") == 1);
  CHECK(faces_named(*guided, side("F5", 1, 0)) >= 1 && edge_names_unique(*guided));
  CHECK(near(bounding_box(*guided).max.Z(), 11, 1e-4));

  // A rectangle swept along the Z axis on a face that twists about it by
  // a quarter turn: a twisted prism, V = A L, its end turned a quarter:
  // the long sides along Y. Without the guide surface it does not turn.
  const double length = 40;
  const ShapePtr twisted = twisted_face(length, 6, kPi / 2);
  SweepSpec prism;
  prism.feature = "F5";
  prism.regions = {rectangle(1, -4, -1, 8, 2)};
  prism.path = {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(0, 0, length))}};
  prism.guide = {{twisted, "F3:twisted"}};
  const ShapePtr turned = sweep(prism);
  CHECK(BRepCheck_Analyzer(turned->occt()).IsValid());
  CHECK_NEAR_TOL(precise_volume(*turned), 16 * length, 1e-5);
  const auto end_box = [](const Shape& shape) {
    return bounding_box(Shape(shape.face(shape.find_faces(end_cap("F5", 1)).front())));
  };
  CHECK(near(end_box(*turned).max.X(), 1, 1e-3) && near(end_box(*turned).max.Y(), 4, 1e-3));
  prism.guide.clear();
  CHECK(near(end_box(*sweep(prism)).max.X(), 4, 1e-6));

  // A path beside the guide surface (mitcad#161: a thread's crest guided
  // by its flank) takes the surface's normal at its nearest points: the
  // helix of radius 11 through the rectangle's middle beside the cylinder
  // makes the same screw motion.
  thread.path = {{"c1", helix3(11, 5, 2)}};
  const ShapePtr beside = sweep(thread);
  CHECK(BRepCheck_Analyzer(beside->occt()).IsValid());
  CHECK_NEAR_TOL(precise_volume(*beside), 4 * 2 * kPi * 2 * 11, 1e-5);
  CHECK(near(bounding_box(*beside).max.Z(), 11, 1e-3));

  // A profile taller than the pitch (6 against 5): the turns overlap, and
  // the sweep is their union, a ring of radii 10 and 12 over 2 * 5 + 6 - 5
  // of height at each angle: V = pi (12^2 - 10^2) 11 (not twice the
  // screw solid of one turn).
  SweepSpec tall = thread;
  tall.regions = {rectangle(1, 10, -3, 2, 6)};
  tall.path = {{"E{F2:side(c1)}", helix3(10, 5, 2)}};
  const ShapePtr overlapping = sweep(tall);
  CHECK(BRepCheck_Analyzer(overlapping->occt()).IsValid());
  CHECK_NEAR_TOL(precise_volume(*overlapping), kPi * (144 - 100) * 11, 1e-4);
  CHECK(faces_with(*overlapping, "F5:start(") == 1 && faces_with(*overlapping, "F5:end(") == 1);

  // A rail or the parallel orientation do not go with guide surfaces.
  prism.guide = {{twisted, "F3:twisted"}};
  prism.path = {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(0, 0, length))}};
  prism.orientation = SweepOrientation::Parallel;
  CHECK(throws_with([&] { sweep(prism); }, "parallel orientation"));
  prism.orientation = SweepOrientation::Perpendicular;
  prism.rail = {{"c2", line3(gp_Pnt(0, 5, 0), gp_Pnt(0, 5, length))}};
  CHECK(throws_with([&] { sweep(prism); }, "not both"));
  prism.rail.clear();
  prism.guide = {{twisted, "F3:nothing"}};
  CHECK(throws_with([&] { sweep(prism); }, "does not exist"));
}

} // namespace

// The distinct names of a shape's faces, without the numbers of their
// occurrences (a face per piece of a helix: #0, #1, ...).
std::set<std::string> face_name_set(const Shape& shape) {
  std::set<std::string> names;
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      names.insert(name.substr(0, name.find('#')));
    }
  }
  return names;
}

// Helices with their pieces' seams shifted (HelixSweepSpec::piece_shift,
// mitcad#235, the retry of a failed boolean): the same shape and face names
// as without, whether the helix is otherwise one piece, pieces on a
// cylinder of a body or pieces of a helix with flat ends; booleans with
// them give the same result. (The cut the shift is for, a groove whose
// unshifted cut left a part's body uncut, was not reproduced on simple
// bodies: a rounded groove down a nut's hole, its base as long as the pitch
// on the hole, cut alike with the nut turned every 10 degrees. The retry
// itself is tested in the model with a failure put in.)
void test_shifted_helix_pieces() {
  Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  const auto same = [](HelixSweepSpec spec, const char* what) -> ShapePtr {
    ShapePtr plain;
    ShapePtr shifted;
    try {
      plain = helix_sweep(spec);
      spec.piece_shift = 1.0 / 6.0;
      shifted = helix_sweep(spec);
    } catch (const std::exception& error) {
      std::fprintf(stderr, "shifted helix pieces, %s: %s\n", what, error.what());
      ++failures;
      return nullptr;
    }
    const bool valid = BRepCheck_Analyzer(shifted->occt()).IsValid();
    const bool volume = near(precise_volume(*shifted), precise_volume(*plain), 1e-6);
    const bool names = face_name_set(*shifted) == face_name_set(*plain);
    if (!valid || !volume || !names) {
      std::fprintf(stderr, "shifted helix pieces, %s: %s, volume %.9g (%.9g), names %s\n", what,
                   valid ? "valid" : "invalid", precise_volume(*shifted), precise_volume(*plain),
                   names ? "the same" : "differ");
      for (const std::string& name : face_name_set(*plain)) {
        std::fprintf(stderr, "  without: %s\n", name.c_str());
      }
      for (const std::string& name : face_name_set(*shifted)) {
        std::fprintf(stderr, "  shifted: %s\n", name.c_str());
      }
      ++failures;
    }
    return shifted;
  };
  // One piece without a shift: a circle eight from the axis, 4 turns.
  HelixSweepSpec spec;
  spec.feature = "F1";
  spec.frame = xz;
  spec.regions = {circle(1, 8, 0, 1)};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = 5;
  spec.revolutions = 4;
  const ShapePtr one = same(spec, "one piece");
  CHECK(one && faces_with(*one, "F1:start(") == 1 && faces_with(*one, "F1:end(") == 1);
  // Overlapping turns (a triangle whose base on a rod is longer than the
  // pitch), a fraction of a turn at the end.
  spec.regions = {polygon({{5, 4}, {4.4, 4.825}, {5, 5.65}})};
  spec.pitch = 1.5;
  spec.revolutions = 5.9;
  spec.bodies = {ring_body(0, 5, 0, 20)};
  same(spec, "overlapping turns");
  spec.bodies.clear();
  // A thread on a nut's hole (pieces of a third of a turn), cut from it.
  const ShapePtr nut = ring_body(5, 10, 0, 20);
  spec.regions = {polygon({{5, 4}, {5.6, 4.5}, {5, 5}})};
  spec.pitch = 1.5;
  spec.revolutions = 6;
  spec.bodies = {nut};
  same(spec, "thread");
  const BooleanResult plain = boolean(BooleanOp::Cut, {nut.get()}, *helix_sweep(spec));
  spec.piece_shift = 1.0 / 6.0;
  const BooleanResult shifted = boolean(BooleanOp::Cut, {nut.get()}, *helix_sweep(spec));
  CHECK(plain.pieces.size() == 1 && shifted.pieces.size() == 1);
  if (plain.pieces.size() == 1 && shifted.pieces.size() == 1) {
    CHECK(detail::is_valid(shifted.pieces.front().shape->occt()));
    CHECK_NEAR_TOL(precise_volume(*shifted.pieces.front().shape), precise_volume(*plain.pieces.front().shape),
                   1e-6);
  }
  // Flat ends (pieces of the growing construction), turns passing through
  // each other.
  HelixSweepSpec ends;
  ends.feature = "F1";
  ends.frame = xz;
  ends.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  ends.pitch = 4;
  ends.revolutions = 2;
  ends.end_transition = kPi / 2;
  ends.end_flat = 1.5 * kPi;
  ends.regions = {rectangle(1, 5, 0, 1, 2)};
  same(ends, "flat ends");
  // A shift is less than a third of a turn.
  spec.piece_shift = 1.0 / 3.0;
  CHECK(throws_with([&] { helix_sweep(spec); }, "piece shift"));
  spec.piece_shift = -0.1;
  CHECK(throws_with([&] { helix_sweep(spec); }, "piece shift"));
}

// A helix swept in pieces on a body's cylinder has the sides its straight
// sides along the axis sweep as cylinders (the sweep makes them B-splines
// within its tolerance; mitcad#235: a cut across a thread beside its
// root's cylinder removed almost nothing): the same solid, valid, cut from
// the body as before.
void test_helix_sides_on_cylinders_exact() {
  Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  const auto cylinders = [](const Shape& shape) {
    int count = 0;
    for (TopExp_Explorer it(shape.occt(), TopAbs_FACE); it.More(); it.Next()) {
      count += BRepAdaptor_Surface(TopoDS::Face(it.Current())).GetType() == GeomAbs_Cylinder ? 1 : 0;
    }
    return count;
  };
  // A thread on a nut's hole, its base (on the hole and outside it, 1 mm
  // from the axis) swept in pieces of a third of a turn.
  const ShapePtr nut = ring_body(5, 10, 0, 20);
  HelixSweepSpec spec;
  spec.feature = "F1";
  spec.frame = xz;
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = 1.5;
  spec.revolutions = 6;
  spec.regions = {polygon({{5, 4}, {5.6, 4.5}, {5, 5}})};
  const ShapePtr one = helix_sweep(spec);
  spec.bodies = {nut};
  const ShapePtr pieces = helix_sweep(spec);
  CHECK(cylinders(*one) == 0);
  CHECK_NEAR_TOL(cylinders(*pieces), 18, 0.0);
  CHECK(BRepCheck_Analyzer(pieces->occt()).IsValid());
  CHECK_NEAR_TOL(precise_volume(*pieces), precise_volume(*one), 1e-6);
  const BooleanResult cut = boolean(BooleanOp::Cut, {nut.get()}, *pieces);
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() == 1) {
    CHECK(detail::is_valid(cut.pieces.front().shape->occt()));
    CHECK_NEAR_TOL(precise_volume(*cut.pieces.front().shape), precise_volume(*nut) - precise_volume(*one), 1e-6);
  }
}

// A thread with touching turns cut into a blind bore and running out past
// its bottom into the material there, and a ring joined over its end
// (mitcad#235): the material inside the bore's cylinder beyond the bore
// stays whole, so that the ring fills the thread's end without a slot. The
// turns meet in one edge on that cylinder past the bottom too; the face
// merge after the intersection with the cylinder inside the bore went back
// and forth over the same seams there without end (OCCT port patch 0590).
void test_helix_slivers_within_the_bodies_faces() {
  struct Case {
    const char* what;
    double bore;     // the bore's radius
    double outer;    // the body's
    double bottom;   // the bore from z = 0 to its bottom, the body to twice that
    double start;    // the thread's base from z = start
    double pitch;    // as long as the base
    double depth;    // the thread's
    double turns;
    double ring_from; // the ring over the thread's end from z = ring_from to bottom + 5
    bool yz;         // the profile in the YZ plane, else XZ
    bool left;       // left-handed
  };
  const Case cases[] = {
      {"a nut", 5, 10, 10, 4, 1.5, 0.6, 6, 9, false, false},
      // The thread starting below the body and running out 2.6 mm past the
      // bore's bottom, the ring from the bottom up.
      {"a pipe's socket", 19.125, 23, 10, -2.1, 2.1, 2.1, 6, 10, false, false},
      {"a pipe's socket sketched on YZ", 19.125, 23, 10, -2.1, 2.1, 2.1, 6, 10, true, false},
      {"a left-handed thread in the socket", 19.125, 23, 10, -2.1, 2.1, 2.1, 6, 10, true, true},
      // The thread ending 0.3 mm past the bottom, the ring from below it.
      {"a short run-out", 8, 12, 10, 0.3, 1.25, 0.8, 7.6, 9.5, false, false},
  };
  for (const Case& c : cases) {
    Frame xz;
    xz.x_axis = c.yz ? gp_Dir(0, 1, 0) : gp_Dir(1, 0, 0);
    xz.y_axis = gp_Dir(0, 0, 1);
    const ShapePtr block = extrude("F1", {circle(1, 0, 0, c.outer)}, 0, 2 * c.bottom);
    const ShapePtr bore = extrude("F2", {circle(2, 0, 0, c.bore)}, 0, c.bottom);
    const BooleanResult blind = boolean(BooleanOp::Cut, {block.get()}, *bore);
    CHECK(blind.pieces.size() == 1);
    if (blind.pieces.size() != 1) {
      continue;
    }
    const ShapePtr body = blind.pieces.front().shape;
    HelixSweepSpec spec;
    spec.feature = "F3";
    spec.frame = xz;
    spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
    spec.pitch = c.pitch;
    spec.revolutions = c.turns;
    spec.left_handed = c.left;
    spec.regions = {polygon(
        {{c.bore, c.start}, {c.bore + c.depth, c.start + c.pitch / 2}, {c.bore, c.start + c.pitch}})};
    spec.bodies = {body};
    const ShapePtr helix = helix_sweep(spec);
    CHECK(BRepCheck_Analyzer(helix->occt()).IsValid());
    const BooleanResult cut = boolean(BooleanOp::Cut, {body.get()}, *helix);
    CHECK(cut.pieces.size() == 1);
    if (cut.pieces.size() != 1) {
      continue;
    }
    const ShapePtr threaded = cut.pieces.front().shape;
    CHECK(detail::is_valid(threaded->occt()));
    // The material inside the bore's cylinder beyond its bottom.
    const ShapePtr core = extrude("F4", {circle(3, 0, 0, c.bore)}, c.bottom, 2 * c.bottom);
    const BooleanResult inside = boolean(BooleanOp::Intersect, {threaded.get()}, *core);
    CHECK(inside.pieces.size() == 1);
    if (inside.pieces.size() == 1) {
      const double volume = precise_volume(*inside.pieces.front().shape);
      if (!near(volume, kPi * c.bore * c.bore * c.bottom, 1e-8)) {
        std::fprintf(stderr, "helix slivers, %s: inside the bore's cylinder %.9g, expected %.9g\n", c.what,
                     volume, kPi * c.bore * c.bore * c.bottom);
        ++failures;
      }
    }
    // The ring over the thread's end fills it: the part from the bottom up
    // is whole.
    const double ring_to = c.bottom + 5;
    const ShapePtr ring = ring_body(c.bore, c.outer, c.ring_from, ring_to);
    const BooleanResult joined = boolean(BooleanOp::Join, {threaded.get()}, *ring);
    CHECK(joined.pieces.size() == 1);
    if (joined.pieces.size() != 1) {
      continue;
    }
    CHECK(detail::is_valid(joined.pieces.front().shape->occt()));
    const ShapePtr top = extrude("F5", {circle(4, 0, 0, c.outer)}, c.bottom, ring_to);
    const BooleanResult filled = boolean(BooleanOp::Intersect, {joined.pieces.front().shape.get()}, *top);
    CHECK(filled.pieces.size() == 1);
    if (filled.pieces.size() == 1) {
      const double volume = precise_volume(*filled.pieces.front().shape);
      GProp_GProps area;
      BRepGProp::SurfaceProperties(filled.pieces.front().shape->occt(), area, 1.0e-8);
      const double r2 = c.outer * c.outer;
      if (!near(volume, kPi * r2 * 5, 1e-8) || !near(area.Mass(), 2 * kPi * r2 + 2 * kPi * c.outer * 5, 1e-8)) {
        std::fprintf(stderr, "helix slivers, %s: the ring's part %.9g mm3, %.9g mm2, expected %.9g, %.9g\n", c.what,
                     volume, area.Mass(), kPi * r2 * 5, 2 * kPi * r2 + 2 * kPi * c.outer * 5);
        ++failures;
      }
    }
  }
}

// A thread whose profile's side on the bore's cylinder is as long as the
// pitch has turns that meet in one edge there, as the bodies stored in files
// keep them (mitcad#235): no strip of the cylinder is left between the
// turns, and a rolling ball rounds that crest. The nut is cut through by a
// thread from below it to above it; the rounding of the crests within the
// nut removes the ball's analytic volume (the section of the material the
// ball cuts off at the crest, turned about the axis over the nut's height).
void test_helix_turns_meet_in_one_edge() {
  const double bore = 5;
  const double pitch = 1.5;
  const double depth = 0.6;
  const double height = 10;
  const double ball = 0.1;
  const ShapePtr nut = ring_body(bore, 10, 0, height);
  Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  HelixSweepSpec spec;
  spec.feature = "F2";
  spec.frame = xz;
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = pitch;
  spec.revolutions = 9;
  spec.regions = {polygon({{bore, -2}, {bore + depth, -2 + pitch / 2}, {bore, -2 + pitch}})};
  spec.bodies = {nut};
  const BooleanResult cut = boolean(BooleanOp::Cut, {nut.get()}, *helix_sweep(spec));
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() != 1) {
    return;
  }
  const ShapePtr threaded = cut.pieces.front().shape;
  CHECK(detail::is_valid(threaded->occt()));
  // The thread takes a triangle of the section's material every pitch.
  const double removed = 0.5 * pitch * depth * 2 * kPi * (bore + depth / 3) * height / pitch;
  CHECK_NEAR_TOL(precise_volume(*threaded), precise_volume(*nut) - removed, 1e-6);
  // No face is left on the bore's cylinder, and the crests are edges
  // between the flanks (side(c1) below a turn, side(c2) above it).
  std::vector<std::string> crests;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(threaded->occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  for (TopExp_Explorer f(threaded->occt(), TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    CHECK(surface.GetType() != GeomAbs_Cylinder || std::abs(surface.Cylinder().Radius() - bore) > 1e-3);
  }
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    bool lower = false;
    bool upper = false;
    for (const TopoDS_Shape& face : faces_of(i)) {
      for (const std::string& name : threaded->names_of_face(face)) {
        lower = lower || name.rfind("F2:side(c1", 0) == 0;
        upper = upper || name.rfind("F2:side(c2", 0) == 0;
      }
    }
    BRepAdaptor_Curve curve(TopoDS::Edge(faces_of.FindKey(i)));
    const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2);
    if (lower && upper && std::abs(std::hypot(middle.X(), middle.Y()) - bore) < 1e-5) {
      if (const auto name = threaded->name_of_edge(faces_of.FindKey(i))) {
        crests.push_back(*name);
      }
    }
  }
  CHECK(!crests.empty());
  // The section the ball cuts off at the crest, x from the crest outwards
  // (the material between the flanks 2 x tan(half) wide, `half` the half
  // angle of its wedge from the radial direction): its moment about the axis.
  const double half = std::atan2(pitch / 2, depth);
  const double centre = ball / std::sin(half);
  const double tangent = ball / std::tan(half) * std::cos(half);
  double moment = 0;
  constexpr int kSteps = 200000;
  for (int k = 0; k < kSteps; ++k) {
    const double x = tangent * (k + 0.5) / kSteps;
    double width = 2 * x * std::tan(half);
    if (std::abs(centre - x) < ball) {
      width -= 2 * std::sqrt(ball * ball - (centre - x) * (centre - x));
    }
    moment += (bore + x) * width * tangent / kSteps;
  }
  try {
    const ShapePtr rounded = fillet("F3", *threaded, crests, ball);
    CHECK(detail::is_valid(rounded->occt()));
    // The ball rolls across the crest's helix, which leans 2.7 degrees from
    // the axial section: the axial section's rounding turned about the axis
    // is within a percent of it (0.4 %).
    const double expected = 2 * kPi * moment * height / pitch;
    const double change = precise_volume(*threaded) - precise_volume(*rounded);
    if (std::abs(change - expected) > 1e-2 * expected) {
      std::fprintf(stderr, "turns that meet: the crests' rounding removed %.9g, expected %.9g (%zu edges)\n", change,
                   expected, crests.size());
      ++failures;
    }
  } catch (const std::exception& error) {
    std::fprintf(stderr, "turns that meet: the crests' rounding failed: %s\n", error.what());
    ++failures;
  }
}

// The roots of a nut's thread rounded up to the nut's faces (mitcad#415):
// the thread runs from below the nut to above it, so each root's rounding
// ends where the nut's bottom and top cut it, and the two contacts of the
// rounding, on the flanks either side of the root, cross those planes far
// apart along the root (the thread's lead is 0.47 degrees). The walk past
// the root's end follows the flank's iso-line the root runs on (port patch
// 0742): before, it followed the guide's smooth extension, which leaves the
// root's helix within millimetres, the contacts swerved, and the result was
// invalid, its ends joined by vertices with tolerances of tens of
// millimetres (as the `.ipt` import's rounding of a thread's root running
// out past a bore's bottom left them). The rounding adds the section
// between the flanks and the ball turned about the axis over the nut's
// height, and each plane gains
// that section turned through the angle the root rises across it.
void test_thread_roots_rounded_up_to_the_nuts_faces() {
  const double bore = 19;
  const double pitch = 1;
  const double depth = 0.5;
  const double height = 10;
  const double ball = 0.3;
  const ShapePtr nut = ring_body(bore, bore + 10, 0, height);
  Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  HelixSweepSpec spec;
  spec.feature = "F2";
  spec.frame = xz;
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = pitch;
  spec.revolutions = (height + 4) / pitch;
  spec.regions = {polygon({{bore, -2}, {bore + depth, -2 + pitch / 2}, {bore, -2 + pitch}})};
  spec.bodies = {nut};
  const BooleanResult cut = boolean(BooleanOp::Cut, {nut.get()}, *helix_sweep(spec));
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() != 1) {
    return;
  }
  const ShapePtr threaded = cut.pieces.front().shape;
  // The root: the edge between the flanks on the thread's outer radius.
  std::vector<std::string> roots;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(threaded->occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    BRepAdaptor_Curve curve(TopoDS::Edge(faces_of.FindKey(i)));
    const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2);
    if (faces_of(i).Extent() == 2 && std::abs(std::hypot(middle.X(), middle.Y()) - bore - depth) < 1e-5) {
      if (const auto name = threaded->name_of_edge(faces_of.FindKey(i))) {
        roots.push_back(*name);
      }
    }
  }
  CHECK(!roots.empty());
  // The section the ball fills at the root: between the flanks, whose
  // wedge has the half angle `half` from the radial direction, and the
  // ball; its moment about the axis (x inwards from the root).
  const double half = std::atan2(pitch / 2, depth);
  const double centre = ball / std::sin(half);
  const double tangent = ball / std::tan(half) * std::cos(half);
  double moment = 0;
  constexpr int kSteps = 200000;
  for (int k = 0; k < kSteps; ++k) {
    const double x = tangent * (k + 0.5) / kSteps;
    double width = 2 * x * std::tan(half);
    if (std::abs(centre - x) < ball) {
      width -= 2 * std::sqrt(ball * ball - (centre - x) * (centre - x));
    }
    moment += (bore + depth - x) * width * tangent / kSteps;
  }
  const auto plane_area = [](const Shape& shape, double z) {
    double sum = 0;
    for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More(); f.Next()) {
      const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
      if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Location().Z() - z) < 1e-9) {
        GProp_GProps props;
        BRepGProp::SurfaceProperties(f.Current(), props);
        sum += props.Mass();
      }
    }
    return sum;
  };
  try {
    const ShapePtr rounded = fillet("F3", *threaded, roots, ball);
    CHECK(detail::is_valid(rounded->occt()));
    double largest = 0;
    for (TopExp_Explorer v(rounded->occt(), TopAbs_VERTEX); v.More(); v.Next()) {
      largest = std::max(largest, BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())));
    }
    CHECK(largest < 1e-3);
    const double expected = 2 * kPi * moment * height / pitch;
    const double change = precise_volume(*rounded) - precise_volume(*threaded);
    if (std::abs(change - expected) > 1e-2 * expected) {
      std::fprintf(stderr, "thread roots: the rounding added %.9g, expected %.9g (%zu edges)\n", change, expected,
                   roots.size());
      ++failures;
    }
    // Each plane gains what the ball fills of the root's groove there: the
    // section turned through the angle the root rises across it, its area
    // 2 pi moment / pitch.
    for (const double z : {0.0, height}) {
      const double gain = plane_area(*rounded, z) - plane_area(*threaded, z);
      const double section = 2 * kPi * moment / pitch;
      if (std::abs(gain - section) > 1e-2 * section) {
        std::fprintf(stderr, "thread roots: the plane at z = %g gained %.6g, expected %.6g\n", z, gain, section);
        ++failures;
      }
    }
  } catch (const std::exception& error) {
    std::fprintf(stderr, "thread roots: the rounding failed: %s\n", error.what());
    ++failures;
  }
}

// A thread's crest rounded down to its run-out (mitcad#422): a post of
// radius 22 has a groove of pitch 1.5 cut up through its top (2.5 turns
// from 1.5 radians round, so that the cut below lies away from the
// cylinder's seam), its flanks sloping 1 in 2, and the groove runs out at
// its start along a straight cut, its profile carried on level along the
// tangent backwards, so that the run-out's lower flank is the plane
// tangent to the groove's lower flank along the start profile. The lower
// crest (the edge between the post's cylinder and the lower flank) is
// rounded with 0.15 from the top down to the run-out, where its edge runs
// on tangentially into the run-out's crest, which is not rounded. Near the
// start the plane only touches the rounding; a turn back, where the crest
// rises out of it again (331 degrees up), it cuts the rounding across. The
// rounding ends at that cut, as the `.ipt` files' history stores such a
// rounding (port patch 0790): a small face on the plane closes it there,
// and the crest stays sharp from there down to the run-out. Before, the
// corner at the run-out failed (the plane does not cut the rounding there).
void test_thread_crest_rounded_down_to_a_run_out() {
  const double radius = 22;
  const double pitch = 1.5;
  const double ball = 0.15;
  const double start = 1.5; // the groove's start angle
  const ShapePtr post = ring_body(0, radius, -3, 3.6);
  Frame xz;
  xz.x_axis = gp_Dir(std::cos(start), std::sin(start), 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  const Region groove = polygon({{radius + 0.5, 0}, {radius - 0.9, 0.7}, {radius + 0.5, 1.4}});
  HelixSweepSpec spec;
  spec.feature = "F2";
  spec.frame = xz;
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.pitch = pitch;
  spec.revolutions = 2.5;
  spec.regions = {groove};
  spec.bodies = {post};
  const BooleanResult threaded = boolean(BooleanOp::Cut, {post.get()}, *helix_sweep(spec));
  CHECK(threaded.pieces.size() == 1);
  if (threaded.pieces.size() != 1) {
    return;
  }
  ExtrudeSpec run_out;
  run_out.feature = "F3";
  run_out.frame = xz;
  run_out.regions = {groove};
  run_out.direction = gp_Dir(std::sin(start), -std::cos(start), 0);
  run_out.start = 0;
  run_out.end = 8;
  const BooleanResult cut = boolean(BooleanOp::Cut, {threaded.pieces.front().shape.get()}, *extrude(run_out));
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() != 1) {
    return;
  }
  const ShapePtr body = cut.pieces.front().shape;
  // The lower crest: on the cylinder, at the height the lower flank (from
  // z = 0 at radius 22.5 up by 1 in 2) meets it, risen by the pitch a turn.
  const double crest0 = 0.25;
  const auto on_crest = [&](const gp_Pnt& p) {
    if (std::abs(std::hypot(p.X(), p.Y()) - radius) > 1e-4) {
      return false;
    }
    const double turns = (std::atan2(p.Y(), p.X()) - start) / (2 * kPi);
    const double k = std::round((p.Z() - crest0) / pitch - turns);
    return std::abs(p.Z() - crest0 - pitch * (turns + k)) < 1e-4;
  };
  const auto along_crest = [&](const TopoDS_Edge& edge) {
    const BRepAdaptor_Curve curve(edge);
    for (int k = 0; k <= 4; ++k) {
      if (!on_crest(curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 4))) {
        return false;
      }
    }
    return true;
  };
  std::vector<std::string> crest;
  for (int i = 0; i < body->edge_count(); ++i) {
    if (along_crest(body->edge(i))) {
      if (const auto name = body->name_of_edge(body->edge(i))) {
        crest.push_back(*name);
      }
    }
  }
  CHECK(!crest.empty());
  // Where the run-out's lower flank, the plane through the start's crest
  // point along the tangent there rising 1 in 2 inwards, cuts the crest a
  // turn back: at the angle phi before the start where the crest, 2 pi -
  // phi up the helix, comes back to the plane's height on the cylinder.
  double phi = 0.5;
  for (int it = 0; it < 100; ++it) {
    const double f = pitch * (2 * kPi - phi) / (2 * kPi) - 0.5 * radius * (1 - std::cos(phi));
    const double df = -pitch / (2 * kPi) - 0.5 * radius * std::sin(phi);
    phi -= f / df;
  }
  const double sharp = radius * (2 * kPi - phi); // the crest left sharp, nearly
  try {
    const ShapePtr rounded = fillet("F4", *body, crest, ball);
    CHECK(detail::is_valid(rounded->occt()));
    // A small face on the run-out's plane where the cut is.
    const gp_Pnt q(radius * std::cos(start - phi), radius * std::sin(start - phi),
                   crest0 + pitch * (2 * kPi - phi) / (2 * kPi));
    // (Across the flank line at the start, rising 1 in 2 inwards.)
    const gp_Dir plane_normal(0.5 * std::cos(start), 0.5 * std::sin(start), 1);
    int caps = 0;
    for (TopExp_Explorer f(rounded->occt(), TopAbs_FACE); f.More(); f.Next()) {
      const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
      if (surface.GetType() != GeomAbs_Plane || !surface.Plane().Axis().Direction().IsParallel(plane_normal, 1e-6)) {
        continue;
      }
      GProp_GProps props;
      BRepGProp::SurfaceProperties(f.Current(), props);
      if (props.Mass() < 0.05 && props.CentreOfMass().Distance(q) < 0.3) {
        ++caps;
      }
    }
    if (caps != 1) {
      std::fprintf(stderr, "crest to a run-out: %d small faces on the run-out's plane at the cut, expected 1\n", caps);
      ++failures;
    }
    // The crest left sharp from the run-out up to the cut.
    double left = 0;
    for (int i = 0; i < rounded->edge_count(); ++i) {
      const TopoDS_Edge& edge = rounded->edge(i);
      if (along_crest(edge)) {
        GProp_GProps props;
        BRepGProp::LinearProperties(edge, props);
        left += props.Mass();
      }
    }
    if (std::abs(left - sharp) > 0.01 * sharp) {
      std::fprintf(stderr, "crest to a run-out: %.6g of the crest left sharp, expected %.6g\n", left, sharp);
      ++failures;
    }
  } catch (const std::exception& error) {
    std::fprintf(stderr, "crest to a run-out: the rounding failed: %s\n", error.what());
    ++failures;
  }
}

// A fixed binormal (mitcad#397): a 20 x 2 profile, its 20 level, swept
// along a quarter of a circle of radius 50 in a plane tilted 20 degrees
// about X (a sloped arc, rising from (50, 0, 0) to its top, where it runs
// along -X). With the binormal up the profile stays level, so the end cap
// stands 2 high; keeping its angle to the arc's plane it is tilted there
// by 20 degrees. Either way the volume is the area times the length.
void test_fixed_binormal() {
  const double tilt = kPi / 9;
  const gp_Dir normal(0, -std::sin(tilt), std::cos(tilt));
  const gp_Dir ahead(0, std::cos(tilt), std::sin(tilt));
  SweepSpec spec;
  spec.feature = "F5";
  spec.frame.origin = gp_Pnt(50, 0, 0);
  spec.frame.x_axis = gp_Dir(1, 0, 0);
  spec.frame.y_axis = ahead.Crossed(gp_Dir(1, 0, 0));
  spec.regions = {rectangle(1, -10, -1, 20, 2)};
  spec.path = {{"c1", arc3(gp_Pnt(0, 0, 0), normal, gp_Dir(1, 0, 0), 50, 0, kPi / 2)}};
  const auto cap_height = [](const Shape& shape) {
    const std::vector<int> caps = shape.find_faces(end_cap("F5", 1));
    if (caps.size() != 1) {
      return -1.0;
    }
    const BoundingBox box = bounding_box(Shape(shape.face(caps.front())));
    return box.max.Z() - box.min.Z();
  };
  const ShapePtr tilted = sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*tilted), 40 * 50 * kPi / 2, 1e-6);
  CHECK(near(cap_height(*tilted), 20 * std::sin(tilt) + 2 * std::cos(tilt), 1e-6));
  spec.binormal = gp_Dir(0, 0, 1);
  const ShapePtr level = sweep(spec);
  CHECK_NEAR_TOL(precise_volume(*level), 40 * 50 * kPi / 2, 1e-6);
  CHECK_NEAR_TOL(cap_height(*level), 2, 1e-6);
  CHECK(faces_named(*level, start_cap("F5", 1)) == 1 && faces_named(*level, side("F5", 1, 0)) == 1);
  // A sloped line before the arc, on into it.
  SweepSpec longer = spec;
  const gp_Pnt before = gp_Pnt(50, 0, 0).Translated(gp_Vec(ahead) * -30);
  longer.frame.origin = before;
  longer.path.insert(longer.path.begin(), {"c2", line3(before, gp_Pnt(50, 0, 0))});
  const ShapePtr on = sweep(longer);
  CHECK_NEAR_TOL(precise_volume(*on), 40 * (30 + 50 * kPi / 2), 1e-6);
  CHECK_NEAR_TOL(cap_height(*on), 2, 1e-6);
  // Not round a corner (the frames on either side do not meet in a mitre).
  SweepSpec corner = longer;
  corner.path.back() = {"c1", line3(gp_Pnt(50, 0, 0), gp_Pnt(20, 0, 0))};
  CHECK(throws_with([&] { sweep(corner); }, "unsupported: a fixed binormal along a path with corners"));
  // Not with a twist, nor along a path that runs along the binormal.
  spec.twist = 0.5;
  CHECK(throws_with([&] { sweep(spec); }, "unsupported: a fixed binormal"));
  spec.twist = 0;
  spec.binormal = ahead;
  spec.path = {{"c1", line3(gp_Pnt(50, 0, 0), gp_Pnt(50, 0, 0).Translated(gp_Vec(ahead) * 30))}};
  CHECK(throws_with([&] { sweep(spec); }, "runs along the sweep's binormal"));
}

// Frenet, auxiliary and constant binormal frames (mitcad#539). Along the sloped arc of
// test_fixed_binormal the Frenet frame turns the profile with the arc, as
// the default frame does there: the end cap stands tilted by the arc's 20
// degrees. Along a straight path of 50 a curve 10 above it crossing from
// y = 10 to y = -10 turns a profile 8 high and 2 wide a quarter turn about
// the path (the direction from the path to the curve turns from 45 degrees
// on one side to 45 on the other), by the plane rule and by the fraction
// rule alike: the end cap is 8 wide and 2 high. Every section is the
// profile, so the volume is its area times the length.
void test_sweep_frames() {
  const double tilt = kPi / 9;
  const gp_Dir normal(0, -std::sin(tilt), std::cos(tilt));
  const gp_Dir ahead(0, std::cos(tilt), std::sin(tilt));
  const auto cap_box = [](const Shape& shape) {
    const std::vector<int> caps = shape.find_faces(end_cap("F5", 1));
    CHECK(caps.size() == 1);
    return caps.size() == 1 ? bounding_box(Shape(shape.face(caps.front()))) : bounding_box(shape);
  };
  SweepSpec arc;
  arc.feature = "F5";
  arc.frame.origin = gp_Pnt(50, 0, 0);
  arc.frame.x_axis = gp_Dir(1, 0, 0);
  arc.frame.y_axis = ahead.Crossed(gp_Dir(1, 0, 0));
  arc.regions = {rectangle(1, -10, -1, 20, 2)};
  arc.path = {{"c1", arc3(gp_Pnt(0, 0, 0), normal, gp_Dir(1, 0, 0), 50, 0, kPi / 2)}};
  arc.frenet = true;
  const ShapePtr frenet = sweep(arc);
  CHECK_NEAR_TOL(precise_volume(*frenet), 40 * 50 * kPi / 2, 1e-5);
  const BoundingBox tilted = cap_box(*frenet);
  CHECK_NEAR_TOL(tilted.max.Z() - tilted.min.Z(), 20 * std::sin(tilt) + 2 * std::cos(tilt), 1e-5);
  CHECK(faces_named(*frenet, start_cap("F5", 1)) == 1 && faces_named(*frenet, side("F5", 1, 0)) == 1);

  SweepSpec turned = circle_sweep(line_path(0, 50), 1);
  turned.regions = {rectangle(1, -4, -1, 8, 2)};
  turned.auxiliary = {{"c9", line3(gp_Pnt(0, 10, 10), gp_Pnt(50, -10, 10))}};
  for (const bool curvilinear : {false, true}) {
    turned.curvilinear = curvilinear;
    const ShapePtr shape = sweep(turned);
    // (The curvilinear rule's bar held 784.9 before the OCCT port's patch
    // 1030: its surfaces bent between the sections.)
    CHECK_NEAR_TOL(precise_volume(*shape), 16 * 50, 1e-7);
    const BoundingBox box = cap_box(*shape);
    CHECK_NEAR_TOL(box.max.Y() - box.min.Y(), 8, 1e-6);
    CHECK_NEAR_TOL(box.max.Z() - box.min.Z(), 2, 1e-6);
    CHECK(faces_named(*shape, start_cap("F5", 1)) == 1 && faces_named(*shape, side("F5", 1, 0)) == 1);
  }
  // A constant binormal: along the arc's axis the frame is the arc's own,
  // as Frenet's; leaning 30 degrees from it towards the arc's start, (0,
  // sin 50, cos 50), the band turns only about it. That band is the one the
  // reference model pd_pipe_modes sweeps (its Binormal in the pipe's own
  // placement): FreeCAD's stored pipe holds 2531.05987 mm³.
  arc.frenet = false;
  arc.constant_binormal = normal;
  CHECK_NEAR_TOL(precise_volume(*sweep(arc)), 40 * 50 * kPi / 2, 1e-6);
  arc.constant_binormal = gp_Dir(0, std::sin(5 * kPi / 18), std::cos(5 * kPi / 18));
  const ShapePtr leaning = sweep(arc);
  CHECK_NEAR_TOL(precise_volume(*leaning), 2531.059870978739, 1e-6);
  CHECK(faces_named(*leaning, end_cap("F5", 1)) == 1 && edge_names_unique(*leaning));
  arc.frenet = true;
  CHECK(throws_with([&] { sweep(arc); }, "not both"));
  // One frame at a time, and not with a twist or the parallel orientation.
  turned.frenet = true;
  CHECK(throws_with([&] { sweep(turned); }, "not both"));
  turned.frenet = false;
  turned.twist = 0.5;
  CHECK(throws_with([&] { sweep(turned); }, "unsupported: a Frenet or auxiliary frame"));
  turned.twist = 0;
  turned.orientation = SweepOrientation::Parallel;
  CHECK(throws_with([&] { sweep(turned); }, "unsupported: a Frenet or auxiliary frame"));
}

// A planar body face as a sweep's profile (mitcad#543): the 10 x 10 top of
// a 10 x 10 x 5 block swept 20 up from its centre, and a quarter of a
// circle of radius 30 round (Pappus: the face's area times the length its
// centre runs); the block stays as it is, its edges name the sides.
void test_face_sweep() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 10, 10)}, 0, 5);
  SweepSpec spec;
  spec.feature = "F5";
  spec.profile_body = block;
  spec.profile_face = end_cap("F2", 1);
  spec.profile_key = end_cap("F2", 1);
  spec.path = {{"c1", line3(gp_Pnt(5, 5, 5), gp_Pnt(5, 5, 25))}};
  const ShapePtr up = sweep(spec);
  CHECK_NEAR(precise_volume(*up), 100 * 20);
  CHECK_NEAR(precise_volume(*block), 500);
  const std::string start = face_name("F5", "start", end_cap("F2", 1));
  const std::string end = face_name("F5", "end", end_cap("F2", 1));
  CHECK(faces_named(*up, start) == 1 && faces_named(*up, end) == 1);
  CHECK(up->face_count() == 6 && edge_names_unique(*up));
  const std::vector<int> top = up->find_faces(end);
  CHECK(top.size() == 1 && near(bounding_box(Shape(up->face(top.front()))).min.Z(), 25, 1e-9));
  // Round a quarter of a circle about the line x = 35, y = 5 in z (the
  // face's centre 30 from it), in the XZ plane through it.
  spec.path = {{"c1", arc3(gp_Pnt(35, 5, 5), gp_Dir(0, -1, 0), gp_Dir(-1, 0, 0), 30, 0, kPi / 2)}};
  CHECK_NEAR_TOL(precise_volume(*sweep(spec)), 100 * 30 * kPi / 2, 1e-6);
  // Not with a twist, nor of a face the body does not have.
  spec.twist = 0.5;
  CHECK(throws_with([&] { sweep(spec); }, "unsupported: a sweep of a body face"));
  spec.twist = 0;
  spec.profile_face = face_name("F2", "end", "r{c9}");
  CHECK(throws_with([&] { sweep(spec); }, "has no face"));
}

// A parallel loft along a centre line (mitcad#547): two level circles of
// radius 5, at the start and the end of an eighth of a circle of radius 30
// rising from the origin in the XZ plane, only move along it: every level
// slice is the circle, so the volume is its area times the rise, and the end
// cap stays level (at right angles the arc would tilt it by 45 degrees).
void test_parallel_loft() {
  const double rise = 30 * std::sin(kPi / 4);
  LoftSpec spec;
  spec.feature = "F5";
  LoftSection low;
  low.frame = xy_at(0);
  low.region = circle(1, 0, 0, 5);
  LoftSection high = low;
  high.frame.origin = gp_Pnt(30 - 30 * std::cos(kPi / 4), 0, rise);
  spec.sections = {low, high};
  spec.centerline = {{"c1", arc3(gp_Pnt(30, 0, 0), gp_Dir(0, 1, 0), gp_Dir(-1, 0, 0), 30, 0, kPi / 4)}};
  spec.parallel = true;
  const ShapePtr level = loft(spec);
  CHECK_NEAR_TOL(precise_volume(*level), kPi * 25 * rise, 1e-5);
  const std::vector<int> caps = level->find_faces("F5:end(r{c1})");
  CHECK(caps.size() == 1);
  if (caps.size() == 1) {
    const BoundingBox box = bounding_box(Shape(level->face(caps.front())));
    CHECK(box.max.Z() - box.min.Z() < 1e-6);
  }
  CHECK(faces_named(*level, "F5:start(r{c1})") == 1 && faces_named(*level, "F5:side(c1)") == 1);
  // Not with a frame, nor without a centre line.
  spec.frenet = true;
  CHECK(throws_with([&] { loft(spec); }, "takes no frame"));
  spec.frenet = false;
  spec.centerline.clear();
  CHECK(throws_with([&] { loft(spec); }, "it needs one"));
}

// The suite's parts sweeps.1-sweeps.5 (test_main.cpp), each a ctest of its
// own so that they run beside each other (mitcad#870). Every test is in
// exactly one part. The Windows round of 28.10.2026 put nearly all the time
// in the helical booleans (helices on cylinders and the helix slivers,
// together 60 s or so, the helix pieces and turns about 45 s, the growing
// helices and thread roots about 20 s) and almost none in the plain sweeps,
// lofts and rails: each part has one or two of the slow tests and some of
// the quick ones. guarded() prints the time of each test of a second or
// more, for moving a line when the parts drift apart.
void sweeps_tests_1() {
  // A parallel loft along a centre line (mitcad#547).
  guarded("parallel loft", test_parallel_loft);
  guarded("sweeps", test_sweeps);
  // A body face as the profile (mitcad#543).
  guarded("face sweep", test_face_sweep);
  guarded("rail regions", test_rail_regions);
  guarded("helices", test_helices);
  guarded("helices on cylinders", test_helices_on_cylinders);
}

void sweeps_tests_2() {
  guarded("backward sweeps", test_backward_sweeps);
  guarded("parallel turns", test_parallel_turns);
  guarded("helix slivers within the bodies' faces", test_helix_slivers_within_the_bodies_faces);
  guarded("ribs", test_ribs);
}

void sweeps_tests_3() {
  // Frenet and auxiliary frames (mitcad#539).
  guarded("sweep frames", test_sweep_frames);
  guarded("fixed binormal", test_fixed_binormal);
  guarded("rail plane", test_rail_plane);
  guarded("coils", test_coils);
  guarded("growing helices", test_growing_helices);
  guarded("lofts", test_lofts);
  guarded("thread roots rounded up to the nut's faces", test_thread_roots_rounded_up_to_the_nuts_faces);
  guarded("thread crest rounded down to a run-out", test_thread_crest_rounded_down_to_a_run_out);
}

void sweeps_tests_4() {
  guarded("two rails and end scale", test_two_rails_and_end_scale);
  guarded("pipes", test_pipes);
  guarded("shifted helix pieces", test_shifted_helix_pieces);
  guarded("helix sides on cylinders exact", test_helix_sides_on_cylinders_exact);
}

void sweeps_tests_5() {
  guarded("helices with flat ends", test_helices_with_flat_ends);
  guarded("helix turns that meet in one edge", test_helix_turns_meet_in_one_edge);
  guarded("guide surfaces", test_guide_surfaces);
}

} // namespace test
