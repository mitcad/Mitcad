// SPDX-License-Identifier: MIT
// Tapers whose walls meet or vanish within the extrusion (mitcad#1851):
// the section at the height h is the profile offset by h tan(angle) with
// mitred corners, its topology changing on the way. A slot or a gap that a
// widening taper closes fills from there on, a hole it shrinks closes, two
// islands merge; a narrow part a narrowing taper eats away ends there; a
// taper that closes the whole profile before the end fails in words. Each
// solid's volume is the integral of its section's area, worked out by hand
// for each profile (a quadratic in the offset between its events,
// integrated exactly with Simpson's rule between them). An Emboss wrapped
// on a cylinder and sweeps along curved paths (mitcad#1854) bend that
// taper: their volumes are those integrals with the bend's change of
// volume (a cubic between the events).

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Ax1.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/emboss.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// A closed polygon of lines c<first>.. in the order of its points,
// counter-clockwise for an outer loop; each line's key names the lines
// before and after it as the sketch does.
Loop polygon_loop(const std::vector<std::array<double, 2>>& points, int first, std::string& key) {
  const int count = static_cast<int>(points.size());
  const auto curve = [count, first](int k) { return "c" + std::to_string(first + (k + count) % count); };
  Loop loop;
  for (int i = 0; i < count; ++i) {
    Segment segment;
    segment.kind = SegmentKind::Line;
    const auto& [x0, y0] = points[static_cast<std::size_t>(i)];
    const auto& [x1, y1] = points[static_cast<std::size_t>((i + 1) % count)];
    segment.start = gp_Pnt2d(x0, y0);
    segment.end = gp_Pnt2d(x1, y1);
    segment.name = curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
    key += (key.size() > 2 ? "," : "") + segment.name;
    loop.segments.push_back(segment);
  }
  return loop;
}

Region polygon(const std::vector<std::array<double, 2>>& points) {
  Region region;
  std::string key = "r{";
  region.loops.push_back(polygon_loop(points, 1, key));
  region.name = key + "}";
  return region;
}

ExtrudeFeatureSpec spec_of(std::vector<Region> regions, double length, double degrees) {
  ExtrudeFeatureSpec spec;
  spec.feature = "F2";
  spec.regions = std::move(regions);
  spec.side1.distance = length;
  spec.side1.taper = degrees * kPi / 180.0;
  return spec;
}

// The integral of the section's area over 0..h: Simpson's rule between
// the events (exact for the quadratics between them).
template <class Area>
double integral(Area area, double h, std::vector<double> events) {
  events.push_back(0.0);
  events.push_back(h);
  std::sort(events.begin(), events.end());
  double sum = 0.0;
  for (std::size_t i = 0; i + 1 < events.size(); ++i) {
    const double a = std::clamp(events[i], 0.0, h);
    const double b = std::clamp(events[i + 1], 0.0, h);
    if (b > a) {
      sum += (b - a) / 6.0 * (area(a) + 4.0 * area(0.5 * (a + b)) + area(b));
    }
  }
  return sum;
}

int solids_of(const Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  return solids;
}

// A valid solid of the expected volume reaching the far end.
void check_solid(const char* what, const ShapePtr& shape, double expected, double height) {
  CHECK(shape != nullptr);
  if (!shape) {
    return;
  }
  const bool valid = BRepCheck_Analyzer(shape->occt()).IsValid();
  const double got = volume(*shape);
  std::printf("%s: volume %.6f (expected %.6f), %d solid(s), %d faces%s\n", what, got, expected, solids_of(*shape),
              shape->face_count(), valid ? "" : ", INVALID");
  CHECK(valid);
  CHECK_NEAR_TOL(got, expected, 1e-6);
  CHECK(near(bounding_box(*shape).max.Z(), height, 1e-7) && near(bounding_box(*shape).min.Z(), 0.0));
}

ShapePtr built(const char* what, const ExtrudeFeatureSpec& spec) {
  try {
    return extrude_feature(spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: %s\n", what, error.what());
    return nullptr;
  }
}

// A 30 x 20 block with a slot 4 wide and 8 deep from the middle of its top
// side (lines c1..c8 from the origin).
Region slotted_block() {
  return polygon({{0, 0}, {30, 0}, {30, 20}, {17, 20}, {17, 12}, {13, 12}, {13, 20}, {0, 20}});
}

// Its section's area offset out by d: the block grown, less the slot's
// width shrunk by 2d over its depth (its floor and its mouth move up
// alike), until the slot closes at d = 2.
double slotted_area(double d) { return (30 + 2 * d) * (20 + 2 * d) - std::max(0.0, 4 - 2 * d) * 8; }

void test_slot_closed_by_a_widening_taper() {
  // 20 degrees over 10 mm: the walls move 3.64 mm out; the slot's walls
  // meet 2 / tan 20 = 5.49 mm up, and the slot is filled from there on.
  const double t = std::tan(20.0 * kPi / 180.0);
  const Region region = slotted_block();
  const ShapePtr closed = built("slot", spec_of({region}, 10, 20));
  check_solid("slot", closed,
              integral([t](double h) { return slotted_area(h * t); }, 10, {2 / t}), 10);
  if (!closed) {
    return;
  }
  CHECK(solids_of(*closed) == 1);
  // The slot's walls lean as their faces, the caps keep their names.
  CHECK(!closed->find_faces("F2:side(c5[c4,c6])").empty());
  CHECK(!closed->find_faces("F2:side(c4[c3,c5])").empty() && !closed->find_faces("F2:side(c6[c5,c7])").empty());
  CHECK(faces_named(*closed, "F2:start(" + region.name + ")") == 1);
  CHECK(faces_named(*closed, "F2:end(" + region.name + ")") == 1);
  CHECK(edge_names_unique(*closed));

  // The same, the event exactly at the end, a little before it (no event:
  // the draft, as before) and a little after it.
  for (const double scale : {1.0, 1.0 - 1e-3, 1.0 + 1e-3}) {
    const double h = 2 / t * scale;
    char what[64];
    std::snprintf(what, sizeof what, "slot closing at %g of the end", 1.0 / scale);
    check_solid(what, built(what, spec_of({region}, h, 20)),
                integral([t](double z) { return slotted_area(z * t); }, h, {2 / t}), h);
  }
}

void test_t_slot_closed_by_a_widening_taper() {
  // A 40 x 30 block with a T-slot from its top: a neck 4 wide and 6 deep,
  // under it a head 16 wide and 6 high. Leaning out by 15 degrees over 20
  // mm, the neck closes at d = 2, leaving the head as a hole that shrinks
  // (its shoulders move down, its floor up) and closes at d = 3.
  const Region region = polygon({{0, 0},
                                 {40, 0},
                                 {40, 30},
                                 {22, 30},
                                 {22, 24},
                                 {28, 24},
                                 {28, 18},
                                 {12, 18},
                                 {12, 24},
                                 {18, 24},
                                 {18, 30},
                                 {0, 30}});
  const double t = std::tan(15.0 * kPi / 180.0);
  const auto area = [t](double h) {
    const double d = h * t;
    return (40 + 2 * d) * (30 + 2 * d) -
           (std::max(0.0, 4 - 2 * d) * (6 + 2 * d) + std::max(0.0, 16 - 2 * d) * std::max(0.0, 6 - 2 * d));
  };
  const ShapePtr closed = built("T-slot", spec_of({region}, 20, 15));
  check_solid("T-slot", closed, integral(area, 20, {2 / t, 3 / t}), 20);
  if (closed) {
    CHECK(solids_of(*closed) == 1 && edge_names_unique(*closed));
  }
}

void test_islands_merging() {
  // Two 10 x 10 squares 4 apart, leaning out by 30 degrees over 10 mm:
  // they meet 2 / tan 30 mm up and are one from there on.
  const double t = std::tan(30.0 * kPi / 180.0);
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 10, 10), rectangle(5, 14, 0, 10, 10)}, 10, 30);
  const auto area = [t](double h) {
    const double d = h * t;
    return 2 * (10 + 2 * d) * (10 + 2 * d) - std::max(0.0, 2 * d - 4) * (10 + 2 * d);
  };
  const ShapePtr merged = built("islands", spec);
  check_solid("islands", merged, integral(area, 10, {2 / t}), 10);
  if (merged) {
    CHECK(solids_of(*merged) == 1);
  }
}

void test_hole_closing() {
  // A 30 x 30 square with a 10 x 10 hole in its middle, leaning out by 30
  // degrees over 12 mm: the hole shrinks by 2d and closes in a point
  // 5 / tan 30 = 8.66 mm up.
  const double t = std::tan(30.0 * kPi / 180.0);
  Region holed = rectangle(1, 0, 0, 30, 30);
  holed.loops.push_back(rectangle(5, 10, 10, 10, 10).loops[0]);
  const auto area = [t](double h) {
    const double d = h * t;
    return (30 + 2 * d) * (30 + 2 * d) - std::pow(std::max(0.0, 10 - 2 * d), 2);
  };
  const ShapePtr closed = built("hole", spec_of({holed}, 12, 30));
  check_solid("hole", closed, integral(area, 12, {5 / t}), 12);
  if (closed) {
    CHECK(solids_of(*closed) == 1 && !closed->find_faces("F2:side(c5[c8,c6])").empty());
  }
}

void test_neck_vanishing() {
  // Two 20 x 20 squares joined by a neck 4 wide and 10 long, leaning in by
  // 30 degrees over 6 mm: the neck's walls meet 2 / tan 30 = 3.46 mm up and
  // the neck ends there; above it the two squares go on apart (the solid
  // stays one, joined below), each shrinking by 2d.
  const Region region = polygon({{0, 0},
                                 {20, 0},
                                 {20, 8},
                                 {30, 8},
                                 {30, 0},
                                 {50, 0},
                                 {50, 20},
                                 {30, 20},
                                 {30, 12},
                                 {20, 12},
                                 {20, 20},
                                 {0, 20}});
  const double t = std::tan(30.0 * kPi / 180.0);
  const auto area = [t](double h) {
    const double d = h * t;
    return 2 * (20 - 2 * d) * (20 - 2 * d) + std::max(0.0, 4 - 2 * d) * (10 + 2 * d);
  };
  const ShapePtr split = built("neck", spec_of({region}, 6, -30));
  check_solid("neck", split, integral(area, 6, {2 / t}), 6);
  if (split) {
    CHECK(solids_of(*split) == 1);
    // The far end in two pieces, one on each square.
    CHECK(split->find_faces("F2:end(" + region.name + ")").size() == 2);
    CHECK(edge_names_unique(*split));
  }
}

void test_rounded_slot_closing() {
  // A 30 x 24 block with a slot 6 wide from its top, ending in a half
  // circle of radius 3 about (15, 14). Leaning out by 20 degrees over 12
  // mm, its walls move in by d and the half circle's radius shrinks to
  // 3 - d: the slot closes 3 / tan 20 = 8.24 mm up.
  Region region;
  region.name = "r{c1,c2,c3,c4,c5,c6,c7,c8}";
  Loop loop;
  const auto line = [&loop](const char* name, gp_Pnt2d a, gp_Pnt2d b) {
    Segment segment;
    segment.name = name;
    segment.kind = SegmentKind::Line;
    segment.start = a;
    segment.end = b;
    loop.segments.push_back(segment);
  };
  line("c1", gp_Pnt2d(0, 0), gp_Pnt2d(30, 0));
  line("c2", gp_Pnt2d(30, 0), gp_Pnt2d(30, 24));
  line("c3", gp_Pnt2d(30, 24), gp_Pnt2d(18, 24));
  line("c4", gp_Pnt2d(18, 24), gp_Pnt2d(18, 14));
  Segment arc;
  arc.name = "c5";
  arc.kind = SegmentKind::Arc;
  arc.center = gp_Pnt2d(15, 14);
  arc.radius = 3;
  arc.start_angle = kPi;
  arc.end_angle = 2 * kPi;
  loop.segments.push_back(arc);
  line("c6", gp_Pnt2d(12, 14), gp_Pnt2d(12, 24));
  line("c7", gp_Pnt2d(12, 24), gp_Pnt2d(0, 24));
  line("c8", gp_Pnt2d(0, 24), gp_Pnt2d(0, 0));
  region.loops.push_back(loop);
  const double t = std::tan(20.0 * kPi / 180.0);
  const auto area = [t](double h) {
    const double d = h * t;
    const double r = std::max(0.0, 3 - d);
    return (30 + 2 * d) * (24 + 2 * d) - (2 * r * (10 + d) + kPi * r * r / 2);
  };
  const ShapePtr closed = built("rounded slot", spec_of({region}, 12, 20));
  check_solid("rounded slot", closed, integral(area, 12, {3 / t}), 12);
  if (closed) {
    CHECK(solids_of(*closed) == 1 && !closed->find_faces("F2:side(c5)").empty());
  }
}

void test_two_sides() {
  // The slotted block: side one 10 mm leaning out by 20 degrees (the slot
  // closes), side two 6 mm against the normal leaning in by 10 degrees
  // (no event: the arms are 13 wide), its section 568 - 116 d + 4 d^2.
  const double t1 = std::tan(20.0 * kPi / 180.0);
  const double t2 = std::tan(10.0 * kPi / 180.0);
  ExtrudeFeatureSpec spec = spec_of({slotted_block()}, 10, 20);
  ExtrudeSide two;
  two.distance = 6;
  two.taper = -10.0 * kPi / 180.0;
  spec.side2 = two;
  const ShapePtr both = built("two sides", spec);
  const double expected = integral([t1](double h) { return slotted_area(h * t1); }, 10, {2 / t1}) +
                          integral([t2](double h) {
                            const double d = h * t2;
                            return 568 - 116 * d + 4 * d * d;
                          }, 6, {});
  CHECK(both != nullptr);
  if (both) {
    CHECK(BRepCheck_Analyzer(both->occt()).IsValid());
    CHECK_NEAR_TOL(volume(*both), expected, 1e-6);
    CHECK(near(bounding_box(*both).max.Z(), 10, 1e-7) && near(bounding_box(*both).min.Z(), -6, 1e-7));
  }
}

void test_thin() {
  // A thin wall 1 inside a 10 x 10 square: a ring whose 8 x 8 hole a
  // widening taper of 30 degrees over 10 mm closes 4 / tan 30 = 6.93 mm up.
  const double t = std::tan(30.0 * kPi / 180.0);
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 10, 10)}, 10, 30);
  spec.side1.thin = ThinWall{WallLocation::Side2, 1, 0};
  const auto area = [t](double h) {
    const double d = h * t;
    return (10 + 2 * d) * (10 + 2 * d) - std::pow(std::max(0.0, 8 - 2 * d), 2);
  };
  check_solid("thin", built("thin", spec), integral(area, 10, {4 / t}), 10);
}

// The C of lines of mitcad#1186 (test_extrude_feature.cpp): a 23 x 44 block
// less a 20 x 40 bite from its left side, strips 2 wide at the bottom and
// the top and 3 wide at the right. Leaning in by 45 degrees, the strips 2
// wide vanish 1 mm up and the one 3 wide 1.5 mm up.
Region c_profile() {
  return polygon({{0, 0}, {23, 0}, {23, 44}, {0, 44}, {0, 42}, {20, 42}, {20, 2}, {0, 2}});
}

void test_narrow_parts_ending() {
  // Over 1.2 mm the bottom and top strips end 1 mm up; the right one goes
  // on, 3 - 2d wide and 44 - 2d long.
  const auto area = [](double d) { return d < 1 ? 212 - 174 * d + 4 * d * d : (3 - 2 * d) * (44 - 2 * d); };
  const ShapePtr ending = built("C over 1.2 mm", spec_of({c_profile()}, 1.2, -45));
  check_solid("C over 1.2 mm", ending, integral(area, 1.2, {1.0}), 1.2);
}

void test_whole_profile_closing() {
  // Over 3 mm the whole C closes 1.5 mm up: the failure says so in words,
  // no names, ids or checker statuses in what a person reads.
  std::string text;
  try {
    extrude_feature(spec_of({c_profile()}, 3, -45));
  } catch (const std::exception& error) {
    text = error.what();
  }
  const std::string message = error_message(text);
  std::printf("test_whole_profile_closing: %s\n", message.c_str());
  CHECK(message == "the taper closes the whole profile before the end of the extrusion; "
                   "make the angle or the distance smaller");
  CHECK(message.find("F2:") == std::string::npos && message.find("BRepCheck") == std::string::npos);
  // The detail says where, for agents.
  CHECK(text.find("closes 1.5 mm from the sketch plane, before the extrusion's end 3 mm from it") !=
        std::string::npos);
}

// The slotted block's volume tapered by 20 degrees over 10 mm.
double slotted_volume() {
  const double t = std::tan(20.0 * kPi / 180.0);
  return integral([t](double h) { return slotted_area(h * t); }, 10, {2 / t});
}

void test_emboss_closing_a_slot() {
  // Emboss's Taper Angle (mitcad#1547) on a plane: the slotted block,
  // moved 10 in x and y, projected along +Z onto the top of a 60 x 40 x 5
  // box and raised 10 with a taper of 20 degrees: its slot closes 5.49 mm
  // up as an extrusion's does (the walls lean as the region moved by
  // 10 tan 20 at the top).
  ExtrudeFeatureSpec box = spec_of({rectangle(1, 0, 0, 60, 40)}, 5, 0);
  box.feature = "F1";
  const ShapePtr body = extrude_feature(box);
  std::vector<std::array<double, 2>> points{{0, 0}, {30, 0}, {30, 20}, {17, 20}, {17, 12}, {13, 12}, {13, 20}, {0, 20}};
  for (auto& [x, y] : points) {
    x += 10;
    y += 10;
  }
  EmbossProjectSpec spec;
  spec.feature = "F3";
  spec.regions = {polygon(points)};
  spec.face = end_cap("F1", 1);
  spec.direction = gp_Vec(0, 0, 1);
  spec.depth = 10;
  spec.taper = 20.0 * kPi / 180.0;
  ShapePtr raised;
  try {
    raised = projected_emboss(*body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "emboss: %s\n", error.what());
  }
  CHECK(raised != nullptr);
  if (raised) {
    CHECK(BRepCheck_Analyzer(raised->occt()).IsValid());
    CHECK_NEAR_TOL(volume(*raised), slotted_volume(), 1e-6);
    CHECK(near(bounding_box(*raised).min.Z(), 5.0) && near(bounding_box(*raised).max.Z(), 15.0, 1e-7));
    CHECK(!raised->find_faces("F3:side(c5[c4,c6])").empty());
  }
}

void test_path_sweep_closing_a_slot() {
  // A sweep with a taper along a straight path: the slotted block on the
  // YZ plane swept 10 along +X with a taper of 20 degrees, its walls
  // leaning out by the angle per unit of the path: the slot closes 5.49
  // mm along, as an extrusion's does.
  SweepSpec spec;
  spec.feature = "F5";
  spec.frame.x_axis = gp_Dir(0, 0, -1);
  spec.frame.y_axis = gp_Dir(0, 1, 0);
  spec.regions = {slotted_block()};
  ModelCurve line;
  line.kind = ModelCurveKind::Line;
  line.start = gp_Pnt(0, 0, 0);
  line.end = gp_Pnt(10, 0, 0);
  spec.path = {{"c1", line}};
  spec.taper = 20.0 * kPi / 180.0;
  ShapePtr swept;
  try {
    swept = sweep(spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "sweep: %s\n", error.what());
  }
  CHECK(swept != nullptr);
  if (swept) {
    CHECK(BRepCheck_Analyzer(swept->occt()).IsValid());
    CHECK_NEAR_TOL(volume(*swept), slotted_volume(), 1e-6);
    CHECK(near(bounding_box(*swept).min.X(), 0.0) && near(bounding_box(*swept).max.X(), 10.0, 1e-7));
  }
}

// A shape's volume, integrated finely (its faces bent B-splines).
double fine_volume(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props, 1e-9);
  return props.Mass();
}

// A valid solid of the expected volume (within 1e-5 of it: the bent faces
// lie within 1e-7 of their size of the bend's surfaces, mitcad#1854).
void check_bent(const char* what, const ShapePtr& shape, double expected) {
  CHECK(shape != nullptr);
  if (!shape) {
    return;
  }
  const bool valid = BRepCheck_Analyzer(shape->occt()).IsValid();
  const double got = fine_volume(*shape);
  std::printf("%s: volume %.6f (expected %.6f), %d solid(s), %d faces%s\n", what, got, expected, solids_of(*shape),
              shape->face_count(), valid ? "" : ", INVALID");
  CHECK(valid);
  CHECK(solids_of(*shape) == 1);
  CHECK_NEAR_TOL(got, expected, 1e-5);
}

void test_wrapped_emboss_closing_a_slot() {
  // Emboss wrapped on a cylinder with a Taper Angle (mitcad#1854): the
  // slotted block with its middle at the sketch's origin, on the plane
  // tangent to a cylinder of radius 25 about Z at +Y (the sketch's x around
  // the axis, y along it), raised 5 to the radius 30 with a taper of 30
  // degrees. On the top's development the region is 30 / 25 = 1.2 times as
  // wide around the axis, its slot 4.8 wide; the top's loops moved by
  // 5 tan 30 = 2.89 cross there (the taper used to fail). Built flat, the
  // section h up is that region offset by d = h tan 30 with mitred
  // corners, of the area (36 + 2d)(20 + 2d) - max(0, 4.8 - 2d) 8 (the slot
  // closing 2.4 / tan 30 = 4.16 up), and the bend takes it to the radius
  // 25 + h, around the axis by (25 + h) / 30 of the development's lengths
  // (those of the radius 30), along it as they are: the volume is the
  // integral over h of the area times (25 + h) / 30 (a cubic between the
  // events, so Simpson's rule is exact).
  std::vector<std::array<double, 2>> points{{0, 0}, {30, 0}, {30, 20}, {17, 20}, {17, 12}, {13, 12}, {13, 20}, {0, 20}};
  for (auto& [x, y] : points) {
    x -= 15;
    y -= 10;
  }
  EmbossWrapSpec spec;
  spec.feature = "F7";
  spec.frame.origin = gp_Pnt(0, 25, 0);
  spec.frame.x_axis = gp_Dir(-1, 0, 0);
  spec.frame.y_axis = gp_Dir(0, 0, 1);
  spec.regions = {polygon(points)};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  spec.radius = 25;
  spec.to_radius = 30;
  spec.taper = 30.0 * kPi / 180.0;
  ShapePtr raised;
  try {
    raised = wrapped_emboss(spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "wrapped emboss: %s\n", error.what());
  }
  const double t = std::tan(spec.taper);
  const auto area = [t](double h) {
    const double d = h * t;
    return ((36 + 2 * d) * (20 + 2 * d) - std::max(0.0, 4.8 - 2 * d) * 8) * (25 + h) / 30;
  };
  check_bent("wrapped emboss", raised, integral(area, 5, {2.4 / t}));
  if (raised) {
    // Its foot and top lie on the cylinders, its walls named as without
    // the taper.
    const std::array<std::pair<std::string, double>, 2> caps{{{"start", 25.0}, {"end", 30.0}}};
    for (const auto& [role, radius] : caps) {
      const std::vector<int> faces = raised->find_faces("F7:" + role + "(" + spec.regions[0].name + ")");
      CHECK(faces.size() == 1);
      for (const int face : faces) {
        const BRepAdaptor_Surface surface(raised->face(face));
        CHECK(surface.GetType() == GeomAbs_Cylinder && near(surface.Cylinder().Radius(), radius));
      }
    }
    CHECK(!raised->find_faces("F7:side(c1[c8,c2])").empty());
  }
}

// The slotted block's section offset by d, and its first moment about the
// line y = 0 (the slot's mouth at y = 20 + d, its floor at 12 + d).
double slotted_moment(double d) {
  return 10 * (30 + 2 * d) * (20 + 2 * d) - (16 + d) * 8 * std::max(0.0, 4 - 2 * d);
}

// The sweep of slotted_block along `path`, tapered by 20 degrees.
ShapePtr swept_slot(const char* what, Path path) {
  SweepSpec spec;
  spec.feature = "F8";
  spec.frame.x_axis = gp_Dir(0, 0, -1);
  spec.frame.y_axis = gp_Dir(0, 1, 0);
  spec.regions = {slotted_block()};
  spec.path = std::move(path);
  spec.taper = 20.0 * kPi / 180.0;
  try {
    return sweep(spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: %s\n", what, error.what());
    return nullptr;
  }
}

// An arc of radius 100 about (x, 100, 0) in the XY plane, from (x, 0, 0)
// on along +X, `length` long.
ModelCurve arc_from(double x, double length) {
  ModelCurve arc;
  arc.kind = ModelCurveKind::Conic;
  arc.center = gp_Pnt(x, 100, 0);
  arc.normal = gp_Dir(0, 0, 1);
  arc.x_axis = gp_Dir(0, -1, 0);
  arc.major = arc.minor = 100;
  arc.first = 0;
  arc.last = length / 100;
  return arc;
}

void test_path_sweep_closing_a_slot_along_an_arc() {
  // A sweep with a taper along a curved path (mitcad#1854): the slotted
  // block on the YZ plane (its y along +Y) swept 10 along an arc of radius
  // 100 curving towards +Y, its walls leaning out by 20 degrees per unit of
  // the path: the slot closes 2 / tan 20 = 5.49 along, as along a line.
  // The section s along is the block offset by d = s tan 20 in the plane
  // square to the path, turned about the arc's axis; its points y from the
  // path run (100 - y) / 100 as far: the volume is the integral over s of
  // the area less its moment about y = 0 over 100 (Pappus; a cubic between
  // the events).
  const double t = std::tan(20.0 * kPi / 180.0);
  const auto section = [t](double s) { return slotted_area(s * t) - slotted_moment(s * t) / 100; };
  check_bent("sweep along an arc", swept_slot("sweep along an arc", {{"c1", arc_from(0, 10)}}),
             integral(section, 10, {2 / t}));
}

void test_path_sweep_closing_a_slot_along_a_line_and_an_arc() {
  // The same along 4 of a line and 6 of an arc on from it: the line's part
  // as an extrusion's, the arc's as above (the slot closing on the arc).
  ModelCurve line;
  line.kind = ModelCurveKind::Line;
  line.start = gp_Pnt(0, 0, 0);
  line.end = gp_Pnt(4, 0, 0);
  const double t = std::tan(20.0 * kPi / 180.0);
  const auto straight = [t](double s) { return slotted_area(s * t); };
  // (u along the arc, from 4 along the path.)
  const auto bent = [t](double u) { return slotted_area((4 + u) * t) - slotted_moment((4 + u) * t) / 100; };
  check_bent("sweep along a line and an arc",
             swept_slot("sweep along a line and an arc", {{"c1", line}, {"c2", arc_from(4, 6)}}),
             integral(straight, 4, {}) + integral(bent, 6, {2 / t - 4}));
}

} // namespace

// Each test named on stdout before it runs, flushed, so that a crash
// shows where it was (the suite crashed with no output in a scheduled run,
// mitcad#1851).
static void traced(const char* name, void (*test)()) {
  std::printf("%s\n", name);
  std::fflush(stdout);
  guarded(name, test);
}

void taper_event_tests() {
  traced("test_slot_closed_by_a_widening_taper", test_slot_closed_by_a_widening_taper);
  traced("test_t_slot_closed_by_a_widening_taper", test_t_slot_closed_by_a_widening_taper);
  traced("test_islands_merging", test_islands_merging);
  traced("test_hole_closing", test_hole_closing);
  traced("test_neck_vanishing", test_neck_vanishing);
  traced("test_rounded_slot_closing", test_rounded_slot_closing);
  traced("test_two_sides", test_two_sides);
  traced("test_thin", test_thin);
  traced("test_narrow_parts_ending", test_narrow_parts_ending);
  traced("test_whole_profile_closing", test_whole_profile_closing);
  traced("test_emboss_closing_a_slot", test_emboss_closing_a_slot);
  traced("test_path_sweep_closing_a_slot", test_path_sweep_closing_a_slot);
  traced("test_wrapped_emboss_closing_a_slot", test_wrapped_emboss_closing_a_slot);
  traced("test_path_sweep_closing_a_slot_along_an_arc", test_path_sweep_closing_a_slot_along_an_arc);
  traced("test_path_sweep_closing_a_slot_along_a_line_and_an_arc",
         test_path_sweep_closing_a_slot_along_a_line_and_an_arc);
}

} // namespace test
