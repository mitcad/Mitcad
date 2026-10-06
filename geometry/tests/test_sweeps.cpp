// SPDX-License-Identifier: MIT
// Sweeps, lofts, pipes, coils, ribs and webs (F3): the volumes of the T0
// models (sweep_*, loft_*, pipe_*, ui_coil, ui_rib) and analytic ones,
// names, and failures.

#include <BRepAdaptor_Surface.hxx>
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
  guarded("pipes", test_pipes);
  guarded("coils", test_coils);
  guarded("lofts", test_lofts);
  guarded("ribs", test_ribs);
}

} // namespace test
