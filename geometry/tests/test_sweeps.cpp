// SPDX-License-Identifier: MIT
// Sweeps, lofts, pipes, coils, ribs and webs (F3): the volumes of the T0
// models (sweep_*, loft_*, pipe_*, ui_coil, ui_rib) and analytic ones,
// names, and failures.

#include <limits>
#include <optional>

#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// Adaptive integration, exact enough for B-spline faces.
double precise_volume(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props, 1.0e-8);
  return props.Mass();
}

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
  // A square in a 10 mm circle: side 10 / sqrt 2; with the wall, minus a
  // square of side 10 / sqrt 2 - 2.
  tube.section = PipeSection::Square;
  const double side_length = 10 / std::sqrt(2.0);
  CHECK_NEAR_TOL(precise_volume(*pipe(tube)),
                 (side_length * side_length - (side_length - 2) * (side_length - 2)) * 50, 1e-6);
  CHECK(faces_named(*pipe(tube), "F2:side3") == 1 && faces_named(*pipe(tube), "F2:inner3") == 1);
  // An equilateral triangle in it: area 3 sqrt(3) / 4 r^2.
  tube.section = PipeSection::Triangular;
  tube.thickness = 0;
  CHECK_NEAR_TOL(precise_volume(*pipe(tube)), 3 * std::sqrt(3.0) / 4 * 25 * 50, 1e-6);
  tube.thickness = 2.5;
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
  // ui_coil: diameter 40, 3 turns of pitch 10, a 4 mm circle on the
  // diameter: V = pi 2^2 * 3 sqrt((40 pi)^2 + 10^2).
  CoilSpec spring;
  spring.feature = "F1";
  spring.diameter = 40;
  spring.revolutions = 3;
  spring.pitch = 10;
  spring.size = 4;
  const ShapePtr helix = coil(spring);
  const double turn = std::hypot(40 * kPi, 10.0);
  CHECK_NEAR_TOL(precise_volume(*helix), kPi * 4 * 3 * turn, 1e-5);
  CHECK(faces_named(*helix, "F1:start") == 1 && faces_named(*helix, "F1:end") == 1);
  CHECK(!helix->find_faces("F1:side0").empty());
  const BoundingBox box = bounding_box(*helix);
  CHECK(near(box.max.X(), 22, 1e-3) && near(box.min.Z(), -2, 2e-2) && near(box.max.Z(), 32, 2e-2));
  // Clockwise: mirrored in Y.
  spring.clockwise = true;
  const ShapePtr left = coil(spring);
  CHECK_NEAR_TOL(precise_volume(*left), kPi * 4 * 3 * turn, 1e-5);
  CHECK(near(mass_properties(*left).center.Y(), -mass_properties(*helix).center.Y(), 1e-4));
  // A square inside the diameter: side 4 / sqrt 2, its centre 20 - side / 2
  // from the axis.
  spring.clockwise = false;
  spring.section = CoilSection::Square;
  spring.position = CoilPosition::Inside;
  const double side_length = 4 / std::sqrt(2.0);
  const double inner = 20 - side_length / 2;
  CHECK_NEAR_TOL(precise_volume(*coil(spring)),
                 side_length * side_length * 3 * std::hypot(2 * kPi * inner, 10.0), 1e-5);
  CHECK(near(bounding_box(*coil(spring)).max.X(), 20, 1e-3));
  // A flat spiral: two turns, growing 6 per turn, a 4 mm circle.
  CoilSpec flat = spring;
  flat.section = CoilSection::Circular;
  flat.position = CoilPosition::OnCenter;
  flat.spiral = true;
  flat.revolutions = 2;
  flat.pitch = 6;
  const ShapePtr spiral = coil(flat);
  CHECK(precise_volume(*spiral) > 0 && near(bounding_box(*spiral).max.Z(), 2, 1e-3));
  // Overlapping turns and a section across the axis fail.
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
  CHECK_NEAR_TOL(precise_volume(*round_ring), 19200, 1e-6);
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
  // Four circles round a ring: a tube a little fuller than the torus (the
  // periodic interpolation of points a quarter turn apart bulges out by
  // about 2 %, as for the squares: 19200 against 100 * 2 pi 30).
  LoftSpec tube;
  tube.feature = "F5";
  tube.closed = true;
  for (int i = 0; i < 4; ++i) {
    LoftSection section = ring.sections[static_cast<std::size_t>(i)];
    section.region = circle(1, 0, 0, 5);
    tube.sections.push_back(section);
  }
  const ShapePtr torus_like = loft(tube);
  const double torus = 25 * kPi * 2 * kPi * 30;
  CHECK(precise_volume(*torus_like) > torus && precise_volume(*torus_like) < 1.03 * torus);
  CHECK(torus_like->face_count() == 1 && faces_named(*torus_like, "F5:side(c1)") == 1);

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

} // namespace

void sweeps_tests() {
  guarded("sweeps", test_sweeps);
  guarded("backward sweeps", test_backward_sweeps);
  guarded("parallel turns", test_parallel_turns);
  guarded("rail regions", test_rail_regions);
  guarded("pipes", test_pipes);
  guarded("coils", test_coils);
  guarded("helices", test_helices);
  guarded("growing helices", test_growing_helices);
  guarded("lofts", test_lofts);
  guarded("ribs", test_ribs);
}

} // namespace test
