// SPDX-License-Identifier: MIT
// Faces moved by a rigid motion, their neighbours following
// (move_faces.hpp, mitcad#343: the .f3d import's moved faces).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopAbs.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/move_faces.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/press_pull.hpp"
#include "mitcad/geometry/profile.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

double volume_of(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.Mass();
}

Bnd_Box box_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, false, false);
  return box;
}

gp_Trsf translation(double x, double y, double z) {
  gp_Trsf t;
  t.SetTranslation(gp_Vec(x, y, z));
  return t;
}

// A rod's side moved square to its axis: its ends, planes square to the
// axis, stay where they are and follow it, so the rod moves whole.
void test_rod_side_moved_sideways() {
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 9)}, -32.5, 32.5);
  const std::string sideName = face_name("F2", "side", "c1");
  const ShapePtr moved = move_faces_rigidly("F3", *rod, {sideName}, translation(-1.8559, 0.0, -0.3614));
  CHECK(BRepCheck_Analyzer(moved->occt()).IsValid());
  CHECK_NEAR(volume_of(*moved), volume_of(*rod));
  CHECK(moved->face_count() == rod->face_count());
  CHECK(faces_named(*moved, sideName) == 1);
  double x0, y0, z0, x1, y1, z1;
  box_of(moved->occt()).Get(x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(x0, -9.0 - 1.8559, 1e-7);
  CHECK_NEAR_TOL(x1, 9.0 - 1.8559, 1e-7);
  CHECK_NEAR_TOL(y0, -9.0, 1e-7);
  CHECK_NEAR_TOL(y1, 9.0, 1e-7);
  CHECK_NEAR_TOL(z0, -32.5, 1e-7);
  CHECK_NEAR_TOL(z1, 32.5, 1e-7);
}

// A block's end face pushed out along its normal, and its top tilted about
// the top's edge along x: the sides follow along their planes.
void test_block_faces_moved() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  // The side at x = 60.
  std::string endName;
  for (int i = 0; i < 4; ++i) {
    for (int f = 0; f < block->face_count(); ++f) {
      if (has_name(block->face_names(f), side("F2", 1, i))) {
        double x0, y0, z0, x1, y1, z1;
        box_of(block->face(f)).Get(x0, y0, z0, x1, y1, z1);
        if (std::abs(x0 - 60.0) < 1e-6 && std::abs(x1 - 60.0) < 1e-6) {
          endName = side("F2", 1, i);
        }
      }
    }
  }
  CHECK(!endName.empty());
  const ShapePtr longer = move_faces_rigidly("F3", *block, {endName}, translation(5.0, 0.0, 0.0));
  CHECK(BRepCheck_Analyzer(longer->occt()).IsValid());
  CHECK_NEAR(volume_of(*longer), 65.0 * 40.0 * 20.0);
  // (Moved along itself, a plane stays: still the same body.)
  CHECK(throws_with([&] { move_faces_rigidly("F3", *block, {endName}, gp_Trsf()); }, "moves nothing"));

  const double angle = 5.0 * kPi / 180.0;
  gp_Trsf tilt;
  tilt.SetRotation(gp_Ax1(gp_Pnt(0, 0, 20), gp_Dir(1, 0, 0)), angle);
  const ShapePtr tilted = move_faces_rigidly("F3", *block, {end_cap("F2", 1)}, tilt);
  CHECK(BRepCheck_Analyzer(tilted->occt()).IsValid());
  // The top rises by y tan(angle) towards y = 40.
  CHECK_NEAR(volume_of(*tilted), 60.0 * (20.0 * 40.0 + 0.5 * 40.0 * 40.0 * std::tan(angle)));
  CHECK(faces_named(*tilted, end_cap("F2", 1)) == 1);
}

// A boss's side moved on the block it stands on: the boss's foot moves on
// the block's top, which keeps its plane.
void test_boss_side_moved_on_a_block() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 10);
  const ShapePtr boss = extrude("F4", {circle(5, 20, 20, 5)}, 10, 20);
  const BooleanResult joined = boolean(BooleanOp::Join, {block.get()}, *boss);
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() != 1) {
    return;
  }
  const ShapePtr body = joined.pieces.front().shape;
  const ShapePtr moved =
      move_faces_rigidly("F5", *body, {face_name("F4", "side", "c5")}, translation(4.0, -3.0, 0.0));
  CHECK(BRepCheck_Analyzer(moved->occt()).IsValid());
  CHECK_NEAR(volume_of(*moved), volume_of(*body));
  double x0, y0, z0, x1, y1, z1;
  box_of(moved->occt()).Get(x0, y0, z0, x1, y1, z1);
  CHECK_NEAR(z1, 20.0);
  // The boss's top moved with its side.
  for (int f = 0; f < moved->face_count(); ++f) {
    if (has_name(moved->face_names(f), face_name("F4", "end", "r{c5}"))) {
      box_of(moved->face(f)).Get(x0, y0, z0, x1, y1, z1);
      CHECK_NEAR_TOL(0.5 * (x0 + x1), 24.0, 1e-7);
      CHECK_NEAR_TOL(0.5 * (y0 + y1), 17.0, 1e-7);
    }
  }
}

// The boss's top rim in the bosses below.
enum class Rim { Sharp, Chamfered, Rounded };

// A plate 60 x 40 x 10 with a boss joined on its top: `outline` extruded
// 20 up from the plate's top with `taper` (radians; negative narrows it),
// its top rim sharp, chamfered by 1 or rounded with radius 1. Null when
// the join gives more than one body.
ShapePtr boss_on_plate(const Region& outline, double taper, Rim rim) {
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 10);
  ExtrudeFeatureSpec spec;
  spec.feature = "F4";
  spec.frame.origin = gp_Pnt(0, 0, 10);
  spec.regions = {outline};
  spec.side1.distance = 20;
  spec.side1.taper = taper;
  const ShapePtr boss = extrude_feature(spec);
  const BooleanResult joined = boolean(BooleanOp::Join, {plate.get()}, *boss);
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() != 1) {
    return nullptr;
  }
  const ShapePtr body = joined.pieces.front().shape;
  const std::string top = face_name("F4", "end", outline.name);
  if (rim == Rim::Chamfered) {
    ChamferSet set;
    set.faces = {top};
    set.spec.distance = 1.0;
    return chamfer("F6", *body, {set});
  }
  if (rim == Rim::Rounded) {
    FilletSet set;
    set.faces = {top};
    set.radius = 1.0;
    return fillet("F6", *body, {set});
  }
  return body;
}

// The names of the boss's faces: every face above the plate's top.
std::vector<std::string> boss_faces(const Shape& body) {
  std::vector<std::string> names;
  for (int f = 0; f < body.face_count(); ++f) {
    double x0, y0, z0, x1, y1, z1;
    box_of(body.face(f)).Get(x0, y0, z0, x1, y1, z1);
    CHECK(!body.face_names(f).empty());
    if (z1 > 10.5 && !body.face_names(f).empty()) {
      names.push_back(body.face_names(f).front());
    }
  }
  return names;
}

// The middle of the box of the face named `name`.
gp_Pnt middle_of(const Shape& body, const std::string& name) {
  for (int f = 0; f < body.face_count(); ++f) {
    if (has_name(body.face_names(f), name)) {
      double x0, y0, z0, x1, y1, z1;
      box_of(body.face(f)).Get(x0, y0, z0, x1, y1, z1);
      return gp_Pnt(0.5 * (x0 + x1), 0.5 * (y0 + y1), 0.5 * (z0 + z1));
    }
  }
  CHECK(false);
  return gp_Pnt();
}

// All faces of the boss moved by (dx, dy) along the plate: one valid solid
// of the same volume and faces, the plate where it was, the boss's faces
// still found by their names and its top moved by (dx, dy).
void check_boss_slides(const char* what, const ShapePtr& body, const Region& outline, double dx, double dy) {
  std::printf("move faces: %s by (%g, %g)\n", what, dx, dy);
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  const std::vector<std::string> names = boss_faces(*body);
  CHECK(names.size() >= 2);
  const ShapePtr moved = move_faces_rigidly("F7", *body, names, translation(dx, dy, 0.0));
  CHECK(BRepCheck_Analyzer(moved->occt()).IsValid());
  int solids = 0;
  for (TopExp_Explorer s(moved->occt(), TopAbs_SOLID); s.More(); s.Next()) {
    ++solids;
  }
  CHECK(solids == 1);
  CHECK_NEAR(volume_of(*moved), volume_of(*body));
  CHECK(moved->face_count() == body->face_count());
  for (const std::string& name : names) {
    CHECK(faces_named(*moved, name) == 1);
  }
  double x0, y0, z0, x1, y1, z1;
  box_of(moved->occt()).Get(x0, y0, z0, x1, y1, z1);
  CHECK_NEAR(x0, 0.0);
  CHECK_NEAR(x1, 60.0);
  CHECK_NEAR(y0, 0.0);
  CHECK_NEAR(y1, 40.0);
  CHECK_NEAR(z0, 0.0);
  CHECK_NEAR(z1, 30.0);
  const std::string top = face_name("F4", "end", outline.name);
  const gp_Pnt was = middle_of(*body, top);
  const gp_Pnt now = middle_of(*moved, top);
  CHECK_NEAR(now.X(), was.X() + dx);
  CHECK_NEAR(now.Y(), was.Y() + dy);
  CHECK_NEAR(now.Z(), was.Z());
}

// A round boss of radius 6.1 tapered by -1 degree with its top rim
// chamfered by 1, all its faces (wall, chamfer, top) moved 12.4 along x,
// farther than it is wide (GitHub #714, mitcad#2543): its top circles,
// closed edges that move with their faces, came out of no length
// ("Geom2d_TrimmedCurve::U1 == U2"). The same with no taper and with a
// sharp and a rounded rim, and moved the other way, where the foot's
// circle as it was lies on the far side of its new place.
void test_round_boss_slides_along_its_plate() {
  const Region disc = circle(5, 20, 20, 6.1);
  const double taper = -kPi / 180.0;
  check_boss_slides("a tapered round boss, chamfered", boss_on_plate(disc, taper, Rim::Chamfered), disc, 12.4, 0);
  check_boss_slides("a tapered round boss, sharp", boss_on_plate(disc, taper, Rim::Sharp), disc, 12.4, 0);
  check_boss_slides("a tapered round boss, rounded", boss_on_plate(disc, taper, Rim::Rounded), disc, 12.4, 0);
  check_boss_slides("a round boss, sharp", boss_on_plate(disc, 0.0, Rim::Sharp), disc, 12.4, 0);
  check_boss_slides("a round boss, rounded", boss_on_plate(disc, 0.0, Rim::Rounded), disc, 12.4, 0);
  check_boss_slides("a tapered round boss, chamfered", boss_on_plate(disc, taper, Rim::Chamfered), disc, -12.4, 0);
}

// An outline of eight arcs of radius 3 about (cx, cy), c11 to c18
// counter-clockwise from +x, each tangent to the next, as the reported
// boss's are: the four facing along x and y bulge out about centres 4 from
// (cx, cy), the four diagonal ones curve in about centres as far out as
// makes them touch their neighbours' circles (about 8.12). Its width is
// 14; the arc facing +y reaches 7 up.
Region eight_arcs(double cx, double cy) {
  const double radius = 3.0;
  const double inner = 4.0;
  const double c45 = std::cos(kPi / 4.0);
  // (A diagonal centre lies 2 radii from its neighbours' centres.)
  const double outer = inner * c45 + std::sqrt(4.0 * radius * radius - inner * inner * c45 * c45);
  // Arc k's centre: along x and y for even k, diagonal for odd ones.
  const auto centre = [&](int k) {
    const double angle = k * kPi / 4.0;
    const double distance = k % 2 == 0 ? inner : outer;
    return gp_Pnt2d(cx + distance * std::cos(angle), cy + distance * std::sin(angle));
  };
  // The direction from centre a to centre b.
  const auto towards = [](const gp_Pnt2d& a, const gp_Pnt2d& b) { return std::atan2(b.Y() - a.Y(), b.X() - a.X()); };
  const auto curve = [](int k) { return "c" + std::to_string(11 + (k + 8) % 8); };
  Region region;
  Loop loop;
  region.name = "r{";
  for (int k = 0; k < 8; ++k) {
    const gp_Pnt2d here = centre(k);
    const gp_Pnt2d before = centre(k + 7);
    const gp_Pnt2d after = centre(k + 1);
    Segment s;
    s.name = curve(k) + "[" + curve(k - 1) + "," + curve(k + 1) + "]";
    s.kind = SegmentKind::Arc;
    s.center = here;
    s.radius = radius;
    // An arc runs counter-clockwise about its centre: one that bulges out
    // from where it touches the arc before to the arc after, one that
    // curves in the other way round.
    if (k % 2 == 0) {
      s.start_angle = towards(here, before);
      s.end_angle = towards(here, after);
    } else {
      s.start_angle = towards(here, after);
      s.end_angle = towards(here, before);
    }
    loop.segments.push_back(s);
    region.name += (k == 0 ? "" : ",") + s.name;
  }
  region.name += "}";
  region.loops.push_back(loop);
  return region;
}

// A boss of eight arcs tapered by -1 degree, its eight sides and top moved
// 8 along y (GitHub #714, mitcad#2543): the foot's arc facing +y (radius
// 3) as it was lies beyond its new circle's centre, and its new edge
// came out running the other way round the circle, the long way between
// its vertices ("move faces:" without a reason). Sharp, chamfered by 1 and
// rounded with radius 1 (the top's faces moved too), by (0, 8) and by
// (-3, 2).
void test_eight_arc_boss_slides_along_its_plate() {
  const Region outline = eight_arcs(20, 20);
  const double taper = -kPi / 180.0;
  for (const auto& [what, rim] : {std::pair<const char*, Rim>{"an eight-arc boss, sharp", Rim::Sharp},
                                  std::pair<const char*, Rim>{"an eight-arc boss, chamfered", Rim::Chamfered},
                                  std::pair<const char*, Rim>{"an eight-arc boss, rounded", Rim::Rounded}}) {
    const ShapePtr body = boss_on_plate(outline, taper, rim);
    check_boss_slides(what, body, outline, 0, 8);
    check_boss_slides(what, body, outline, -3, 2);
  }
}

// A boss slid over the edge of its plate is refused with the reason: its
// foot would run past the plate's top's boundary.
void test_boss_slid_off_its_plate() {
  const Region disc = circle(5, 20, 20, 6.1);
  const ShapePtr body = boss_on_plate(disc, -kPi / 180.0, Rim::Chamfered);
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  CHECK(throws_with([&] { move_faces_rigidly("F7", *body, boss_faces(*body), translation(35, 0, 0)); },
                    "neighbouring face"));
}

// A motion that would change the body's faces is refused.
void test_moves_that_change_the_faces() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  // (The top pushed down past the bottom.)
  CHECK(throws_with([&] { move_faces_rigidly("F3", *block, {end_cap("F2", 1)}, translation(0, 0, -30)); }, ""));
  gp_Trsf scale;
  scale.SetScale(gp_Pnt(0, 0, 0), 2.0);
  CHECK(throws_with([&] { move_faces_rigidly("F3", *block, {end_cap("F2", 1)}, scale); }, "rigid"));
}

// mitcad#2547: a filleted boss moved whole on the block it stands on.

// Segment i (0..7) of a rounded rectangle, c1..c8 in a ring.
std::string rounded_segment(int i) {
  const auto curve = [](int k) { return "c" + std::to_string((k + 8) % 8 + 1); };
  return curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
}

// A rectangle 2a by 2b about (cx, cy) with corners of radius r,
// counter-clockwise: the bottom line c1, its corner arc c2, the right line
// c3 and so on.
Region rounded_rectangle(double cx, double cy, double a, double b, double r) {
  const auto line = [](int i, double x0, double y0, double x1, double y1) {
    Segment s;
    s.name = rounded_segment(i);
    s.kind = SegmentKind::Line;
    s.start = gp_Pnt2d(x0, y0);
    s.end = gp_Pnt2d(x1, y1);
    return s;
  };
  const auto arc = [r](int i, double x, double y, double from) {
    Segment s;
    s.name = rounded_segment(i);
    s.kind = SegmentKind::Arc;
    s.center = gp_Pnt2d(x, y);
    s.radius = r;
    s.start_angle = from;
    s.end_angle = from + 0.5 * kPi;
    s.start = gp_Pnt2d(x + r * std::cos(from), y + r * std::sin(from));
    s.end = gp_Pnt2d(x + r * std::cos(from + 0.5 * kPi), y + r * std::sin(from + 0.5 * kPi));
    return s;
  };
  const double x0 = cx - a, x1 = cx + a, y0 = cy - b, y1 = cy + b;
  Loop loop;
  loop.segments = {line(0, x0 + r, y0, x1 - r, y0), arc(1, x1 - r, y0 + r, 1.5 * kPi),
                   line(2, x1, y0 + r, x1, y1 - r), arc(3, x1 - r, y1 - r, 0.0),
                   line(4, x1 - r, y1, x0 + r, y1), arc(5, x0 + r, y1 - r, 0.5 * kPi),
                   line(6, x0, y1 - r, x0, y0 + r), arc(7, x0 + r, y0 + r, kPi)};
  Region region;
  region.name = "r{";
  for (int i = 0; i < 8; ++i) {
    region.name += (i == 0 ? "" : ",") + rounded_segment(i);
  }
  region.name += "}";
  region.loops.push_back(loop);
  return region;
}

// The body and the faces of its boss (each by a name of its own).
struct Boss {
  ShapePtr body;
  std::vector<std::string> faces;
};

// A block 60 x 40 x 10; on its top a rectangle 16 x 10 about (30, 20) with
// corners of radius 2, extruded 10 with a taper of -1 degree; the boss's
// top edges filleted with radius 1. The boss: 8 sides, its top and 8
// fillets, the 17 faces a whole boss's move selects.
Boss filleted_boss() {
  Boss boss;
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 10);
  ExtrudeFeatureSpec spec;
  spec.feature = "F4";
  spec.frame.origin = gp_Pnt(0, 0, 10);
  const Region outline = rounded_rectangle(30, 20, 8, 5, 2);
  spec.regions = {outline};
  spec.side1.distance = 10.0;
  spec.side1.taper = -kPi / 180.0;
  const ShapePtr tapered = extrude_feature(spec);
  const BooleanResult joined = boolean(BooleanOp::Join, {block.get()}, *tapered);
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() != 1) {
    return boss;
  }
  const std::string top = face_name("F4", "end", outline.name);
  std::vector<std::string> rims;
  for (int i = 0; i < 8; ++i) {
    rims.push_back(edge(top, face_name("F4", "side", rounded_segment(i))));
  }
  boss.body = fillet("F5", *joined.pieces.front().shape, rims, 1.0);
  for (int f = 0; f < boss.body->face_count(); ++f) {
    double x0, y0, z0, x1, y1, z1;
    box_of(boss.body->face(f)).Get(x0, y0, z0, x1, y1, z1);
    if (z1 > 10.5 && !boss.body->face_names(f).empty()) {
      boss.faces.push_back(boss.body->face_names(f).front());
    }
  }
  return boss;
}

int solids_in(const Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

// The box about the faces carrying any of `names`.
Bnd_Box box_of_faces(const Shape& shape, const std::vector<std::string>& names) {
  Bnd_Box box;
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& name : names) {
      if (has_name(shape.face_names(f), name)) {
        BRepBndLib::AddOptimal(shape.face(f), box, false, false);
        break;
      }
    }
  }
  return box;
}

void check_same_box(const Bnd_Box& a, const Bnd_Box& b) {
  double a0, a1, a2, a3, a4, a5, b0, b1, b2, b3, b4, b5;
  a.Get(a0, a1, a2, a3, a4, a5);
  b.Get(b0, b1, b2, b3, b4, b5);
  CHECK_NEAR_TOL(a0, b0, 1e-6);
  CHECK_NEAR_TOL(a1, b1, 1e-6);
  CHECK_NEAR_TOL(a2, b2, 1e-6);
  CHECK_NEAR_TOL(a3, b3, 1e-6);
  CHECK_NEAR_TOL(a4, b4, 1e-6);
  CHECK_NEAR_TOL(a5, b5, 1e-6);
}

// One valid solid with every face of the boss there once by its name.
void check_boss_moved(const Boss& boss, const Shape& moved) {
  CHECK(BRepCheck_Analyzer(moved.occt()).IsValid());
  CHECK(solids_in(moved) == 1);
  CHECK(moved.face_count() == boss.body->face_count());
  for (const std::string& name : boss.faces) {
    CHECK(faces_named(moved, name) == 1);
  }
  CHECK(faces_named(moved, end_cap("F2", 1)) == 1);
}

// The motion applied once, and in `steps` equal steps one after the
// other, each on the result of the step before.
ShapePtr moved_in_steps(const Boss& boss, const gp_Trsf& step, int steps) {
  ShapePtr shape = boss.body;
  for (int i = 0; i < steps; ++i) {
    shape = move_faces_rigidly("F6", *shape, boss.faces, step);
  }
  return shape;
}

// Moved on the block further than its corners' radius: the foot of a
// corner, a circle on the block's top, now has its centre past where the
// corner was. (A boss moved more than about its corners' radius failed.)
void test_filleted_boss_moved_past_its_corners() {
  const Boss boss = filleted_boss();
  CHECK(boss.faces.size() == 17);
  if (boss.faces.size() != 17) {
    return;
  }
  const double before = volume_of(*boss.body);
  for (const gp_Vec& shift :
       {gp_Vec(0, 4.5, 0), gp_Vec(0, -4.5, 0), gp_Vec(5, 0, 0), gp_Vec(-5, 0, 0), gp_Vec(6, -5, 0)}) {
    gp_Trsf motion;
    motion.SetTranslation(shift);
    const ShapePtr moved = move_faces_rigidly("F6", *boss.body, boss.faces, motion);
    check_boss_moved(boss, *moved);
    CHECK_NEAR(volume_of(*moved), before);
    Bnd_Box expected = box_of_faces(*boss.body, boss.faces);
    expected = expected.Transformed(motion);
    check_same_box(box_of_faces(*moved, boss.faces), expected);
  }
  // In one step as in three each under the corners' radius.
  gp_Trsf third;
  third.SetTranslation(gp_Vec(0, 1.5, 0));
  gp_Trsf whole;
  whole.SetTranslation(gp_Vec(0, 4.5, 0));
  const ShapePtr staged = moved_in_steps(boss, third, 3);
  const ShapePtr once = moved_in_steps(boss, whole, 1);
  check_boss_moved(boss, *staged);
  check_boss_moved(boss, *once);
  CHECK_NEAR(volume_of(*once), volume_of(*staged));
  check_same_box(box_of_faces(*once, boss.faces), box_of_faces(*staged, boss.faces));
}

// Turned on the block about an upright axis beside it, and tilted about a
// level axis through its middle: in one step as in small ones.
void test_filleted_boss_turned() {
  const Boss boss = filleted_boss();
  CHECK(boss.faces.size() == 17);
  if (boss.faces.size() != 17) {
    return;
  }
  const double before = volume_of(*boss.body);
  const gp_Ax1 upright(gp_Pnt(30, 5, 10), gp_Dir(0, 0, 1));
  gp_Trsf step;
  step.SetRotation(upright, 6.0 * kPi / 180.0);
  gp_Trsf whole;
  whole.SetRotation(upright, 30.0 * kPi / 180.0);
  const ShapePtr staged = moved_in_steps(boss, step, 5);
  const ShapePtr once = moved_in_steps(boss, whole, 1);
  check_boss_moved(boss, *staged);
  check_boss_moved(boss, *once);
  CHECK_NEAR(volume_of(*once), before);
  CHECK_NEAR(volume_of(*staged), before);
  check_same_box(box_of_faces(*once, boss.faces), box_of_faces(*staged, boss.faces));
  // The boss's top, about (30, 20), turned 30 degrees about (30, 5).
  const std::string top = face_name("F4", "end", rounded_rectangle(30, 20, 8, 5, 2).name);
  for (int f = 0; f < once->face_count(); ++f) {
    if (has_name(once->face_names(f), top)) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(once->face(f), props);
      CHECK_NEAR_TOL(props.CentreOfMass().X(), 30.0 - 15.0 * std::sin(kPi / 6.0), 1e-6);
      CHECK_NEAR_TOL(props.CentreOfMass().Y(), 5.0 + 15.0 * std::cos(kPi / 6.0), 1e-6);
      CHECK_NEAR_TOL(props.CentreOfMass().Z(), 20.0, 1e-6);
    }
  }

  const gp_Ax1 level(gp_Pnt(30, 20, 15), gp_Dir(1, 0, 0));
  gp_Trsf tiltStep;
  tiltStep.SetRotation(level, 7.0 * kPi / 180.0);
  gp_Trsf tilt;
  tilt.SetRotation(level, 35.0 * kPi / 180.0);
  const ShapePtr tiltedStaged = moved_in_steps(boss, tiltStep, 5);
  const ShapePtr tilted = moved_in_steps(boss, tilt, 1);
  check_boss_moved(boss, *tiltedStaged);
  check_boss_moved(boss, *tilted);
  CHECK_NEAR(volume_of(*tilted), volume_of(*tiltedStaged));
  check_same_box(box_of_faces(*tilted, boss.faces), box_of_faces(*tiltedStaged, boss.faces));
}

// Moved over the side of the block: the block's top would have to hold
// the boss's foot past its own edge.
void test_filleted_boss_moved_off_its_block() {
  const Boss boss = filleted_boss();
  CHECK(boss.faces.size() == 17);
  if (boss.faces.size() != 17) {
    return;
  }
  gp_Trsf motion;
  motion.SetTranslation(gp_Vec(0, 20, 0));
  CHECK(throws_with([&] { move_faces_rigidly("F6", *boss.body, boss.faces, motion); },
                    "reach past the edge of a face around them"));
}

// A raised block on a base (mitcad#2548, GitHub mitcad/Mitcad#731): the
// base x 0..60, y 0..40, z 0..10; the block x 10..30, y 15..25, z 10..25
// joined on its top. Its end at x = 10 faces the base's side at x = 0.
struct Step {
  ShapePtr body;
  std::string end;  // the block's end at x = 10
  std::string side; // the base's side at x = 0
  std::string far;  // the base's side at x = 60
};

// The name of the face of `shape` that lies in the plane x = `x` and
// reaches z = `z`.
std::string face_at_x(const Shape& shape, double x, double z) {
  for (int f = 0; f < shape.face_count(); ++f) {
    double x0, y0, z0, x1, y1, z1;
    box_of(shape.face(f)).Get(x0, y0, z0, x1, y1, z1);
    if (std::abs(x0 - x) < 1e-6 && std::abs(x1 - x) < 1e-6 && z0 <= z + 1e-6 && z1 >= z - 1e-6 &&
        !shape.face_names(f).empty()) {
      return shape.face_names(f).front();
    }
  }
  return {};
}

std::optional<Step> step_body() {
  const ShapePtr base = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 10);
  const ShapePtr block = extrude("F4", {rectangle(5, 10, 15, 20, 10)}, 10, 25);
  const BooleanResult joined = boolean(BooleanOp::Join, {base.get()}, *block);
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() != 1) {
    return std::nullopt;
  }
  Step step;
  step.body = joined.pieces.front().shape;
  step.end = face_at_x(*step.body, 10.0, 20.0);
  step.side = face_at_x(*step.body, 0.0, 5.0);
  step.far = face_at_x(*step.body, 60.0, 5.0);
  CHECK(!step.end.empty() && !step.side.empty() && !step.far.empty());
  if (step.end.empty() || step.side.empty() || step.far.empty()) {
    return std::nullopt;
  }
  return step;
}

// The block `length` long along x from its far end at x = 30: one valid
// solid of the base's and the block's volumes, the block's end named and
// at x = 30 - length, the base's far side as it was. Past the base's side
// (length > 30), the overhang's underside is a step face of its own.
void check_step(const Step& step, const Shape& moved, double length, const std::string& feature) {
  CHECK(BRepCheck_Analyzer(moved.occt()).IsValid());
  int solids = 0;
  for (TopExp_Explorer it(moved.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  CHECK(solids == 1);
  CHECK_NEAR(volume_of(moved), 60.0 * 40.0 * 10.0 + length * 10.0 * 15.0);
  CHECK(faces_named(moved, step.end) == 1);
  CHECK(faces_named(moved, step.side) == 1);
  CHECK(faces_named(moved, step.far) == 1);
  for (int f = 0; f < moved.face_count(); ++f) {
    double x0, y0, z0, x1, y1, z1;
    box_of(moved.face(f)).Get(x0, y0, z0, x1, y1, z1);
    if (has_name(moved.face_names(f), step.end)) {
      CHECK_NEAR_TOL(x0, 30.0 - length, 1e-9);
      CHECK_NEAR_TOL(x1, 30.0 - length, 1e-9);
      CHECK_NEAR_TOL(z1, 25.0, 1e-9);
    }
    if (has_name(moved.face_names(f), step.far)) {
      CHECK_NEAR_TOL(x0, 60.0, 1e-9);
      CHECK_NEAR_TOL(x1, 60.0, 1e-9);
      CHECK_NEAR_TOL(z0, 0.0, 1e-9);
      CHECK_NEAR_TOL(z1, 10.0, 1e-9);
      CHECK_NEAR_TOL(y1 - y0, 40.0, 1e-9);
    }
  }
  const std::string underside = face_name(feature, "step", step.end);
  CHECK(faces_named(moved, underside) == (length > 30.0 + 1e-6 ? 1 : 0));
}

// The block's end moved along -x up to the base's side and past it
// (mitcad#2548): short of the side the faces' rebuild keeps the body's
// faces; flush with it and past it the base's top loses its boundary there,
// and the region the end sweeps is added. A distance within 1e-6 mm of
// flush, either way, ends flush (the design's own coordinates rounded in
// their last digits).
void test_end_moved_across_a_step() {
  const std::optional<Step> step = step_body();
  if (!step) {
    return;
  }
  const auto moved = [&](double dx) { return move_faces_rigidly("F6", *step->body, {step->end}, translation(dx, 0, 0)); };
  check_step(*step, *moved(-9.999), 29.999, "F6");
  check_step(*step, *moved(-10.0), 30.0, "F6");
  check_step(*step, *moved(-10.0000004), 30.0, "F6");
  check_step(*step, *moved(-9.9999996), 30.0, "F6");
  check_step(*step, *moved(-10.001), 30.001, "F6");
  check_step(*step, *moved(-14.0), 34.0, "F6");
  // Flush, the end and the base's side are one face holding both names.
  const ShapePtr flush = moved(-10.0);
  for (int f = 0; f < flush->face_count(); ++f) {
    if (has_name(flush->face_names(f), step->end)) {
      CHECK(has_name(flush->face_names(f), step->side));
    }
  }
  // Back from the overhang to where it was: the original volume.
  const ShapePtr out = moved(-14.0);
  const ShapePtr back = move_faces_rigidly("F7", *out, {step->end}, translation(14.0, 0, 0));
  check_step(*step, *back, 20.0, "F7");
}

// Offset Face and Press Pull (Straight, Along the walls) of the same end by
// the same distances along its outward normal (mitcad#2548).
void test_end_offset_and_pulled_across_a_step() {
  const std::optional<Step> step = step_body();
  if (!step) {
    return;
  }
  for (const double distance : {9.999, 10.0, 10.0000004, 10.001, 14.0}) {
    const double length = std::abs(distance - 10.0) < 1e-6 ? 30.0 : 20.0 + distance;
    check_step(*step, *offset_faces("F6", *step->body, {step->end}, distance), length, "F6");
    for (const PressPullRun run : {PressPullRun::Straight, PressPullRun::Along}) {
      PressPullSpec spec;
      spec.feature = "F6";
      spec.faces = {step->end};
      spec.run = run;
      spec.distance = distance;
      const ShapePtr pulled = press_pull(*step->body, spec);
      CHECK(BRepCheck_Analyzer(pulled->occt()).IsValid());
      CHECK_NEAR(volume_of(*pulled), 60.0 * 40.0 * 10.0 + length * 10.0 * 15.0);
      double x0, y0, z0, x1, y1, z1;
      box_of(pulled->occt()).Get(x0, y0, z0, x1, y1, z1);
      CHECK_NEAR_TOL(x0, std::min(0.0, 30.0 - length), 1e-9);
      CHECK_NEAR_TOL(x1, 60.0, 1e-9);
    }
  }
}

} // namespace

void move_faces_tests() {
  guarded("test_rod_side_moved_sideways", test_rod_side_moved_sideways);
  guarded("test_block_faces_moved", test_block_faces_moved);
  guarded("test_boss_side_moved_on_a_block", test_boss_side_moved_on_a_block);
  guarded("test_moves_that_change_the_faces", test_moves_that_change_the_faces);
  guarded("test_filleted_boss_moved_past_its_corners", test_filleted_boss_moved_past_its_corners);
  guarded("test_filleted_boss_turned", test_filleted_boss_turned);
  guarded("test_filleted_boss_moved_off_its_block", test_filleted_boss_moved_off_its_block);
  guarded("test_end_moved_across_a_step", test_end_moved_across_a_step);
  guarded("test_end_offset_and_pulled_across_a_step", test_end_offset_and_pulled_across_a_step);
  guarded("test_round_boss_slides_along_its_plate", test_round_boss_slides_along_its_plate);
  guarded("test_eight_arc_boss_slides_along_its_plate", test_eight_arc_boss_slides_along_its_plate);
  guarded("test_boss_slid_off_its_plate", test_boss_slid_off_its_plate);
}

} // namespace test
