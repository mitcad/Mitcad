// SPDX-License-Identifier: MIT
// A loft's Perpendicular end condition (mitcad#2345): the loft leaves its
// section at right angles to a face the section lies on. Along the section
// the loft's surface meets the face at 90 degrees within 1e-6 rad (its
// normal at right angles to the face's normal), sampled at many points of
// every side face: a circle on a block's top (a plane; a chain of curves
// with the top picked, at both ends, the far one on its sketch's plane), a
// cylinder's top rim on the cylinder's wall (a face section, the wall
// picked; and an open half circle on it, a surface of curves), the top rim
// of a skewed barrel lofted through three circles (a freeform face whose
// normals are no image of the circle), and a block's top to a square above
// with four rails that leave and reach the sections upright. A free loft
// from the rim does not meet the wall so (the condition does something),
// and a section off its face is refused in words.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

constexpr double kRightAngle = 1.0e-6; // radians

g::Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

Region round(int curve, double cx, double cy, double radius) {
  g::Segment s;
  s.name = "c" + std::to_string(curve);
  s.kind = g::SegmentKind::Circle;
  s.center = gp_Pnt2d(cx, cy);
  s.radius = radius;
  Region region;
  region.loops.push_back(g::Loop{{s}});
  region.name = "r{" + s.name + "}";
  return region;
}

// A square of side `side` round the origin, lines c<first>..c<first + 3>
// counter-clockwise from its bottom side, keyed as the sketch keys them.
Region square(int first, double side) {
  const double h = side / 2;
  const auto key = [first](int i) {
    const int a = first + i;
    const int before = first + (i + 3) % 4;
    const int after = first + (i + 1) % 4;
    return "c" + std::to_string(a) + "[c" + std::to_string(before) + ",c" + std::to_string(after) + "]";
  };
  Region region;
  region.loops.emplace_back();
  auto& segments = region.loops.front().segments;
  segments.push_back(line(key(0), -h, -h, h, -h));
  segments.push_back(line(key(1), h, -h, h, h));
  segments.push_back(line(key(2), h, h, -h, h));
  segments.push_back(line(key(3), -h, h, -h, -h));
  region.name = "r{" + key(0) + "," + key(1) + "," + key(2) + "," + key(3) + "}";
  return region;
}

ShapePtr extruded(const std::string& feature, const Region& region, double start, double end) {
  g::ExtrudeSpec spec;
  spec.feature = feature;
  spec.regions = {region};
  spec.start = start;
  spec.end = end;
  return g::extrude(spec);
}

g::LoftSection region_at(double z, const Region& region) {
  g::LoftSection section;
  section.frame.origin = gp_Pnt(0, 0, z);
  section.region = region;
  return section;
}

g::LoftSection face_of(const ShapePtr& body, const std::string& face) {
  g::LoftSection section;
  section.kind = g::LoftSection::Kind::Face;
  section.body = body;
  section.face = face;
  return section;
}

// An arc (a circle when `closed`) about the Z axis through (cx, cy, z).
g::PathCurve arc(const std::string& name, double cx, double cy, double z, double radius, double from, double to,
                 bool closed) {
  g::PathCurve piece;
  piece.name = name;
  piece.curve.kind = g::ModelCurveKind::Conic;
  piece.curve.center = gp_Pnt(cx, cy, z);
  piece.curve.normal = gp_Dir(0, 0, 1);
  piece.curve.x_axis = gp_Dir(1, 0, 0);
  piece.curve.major = radius;
  piece.curve.minor = radius;
  piece.curve.first = from;
  piece.curve.last = to;
  piece.curve.closed = closed;
  return piece;
}

g::LoftSection chain(const g::Path& curves, bool planar) {
  g::LoftSection section;
  section.kind = g::LoftSection::Kind::Chain;
  section.chain = curves;
  if (planar) {
    section.normal = gp_Dir(0, 0, 1);
  }
  return section;
}

g::LoftEnd perpendicular(const ShapePtr& body = nullptr, const std::string& face = std::string(),
                         double weight = 1.0) {
  g::LoftEnd end;
  end.kind = g::LoftEnd::Kind::Perpendicular;
  end.weight = weight;
  end.body = body;
  end.face = face;
  return end;
}

// A cubic Bezier rail from a to b, leaving a and reaching b along Z.
g::Path upright_rail(const gp_Pnt& a, const gp_Pnt& b) {
  g::PathCurve piece;
  piece.name = "c90";
  piece.curve.kind = g::ModelCurveKind::BSpline;
  piece.curve.degree = 3;
  const double rise = (b.Z() - a.Z()) / 3;
  piece.curve.poles = {a, gp_Pnt(a.X(), a.Y(), a.Z() + rise), gp_Pnt(b.X(), b.Y(), b.Z() - rise), b};
  piece.curve.knots = {0, 0, 0, 0, 1, 1, 1, 1};
  return {piece};
}

bool valid(const g::Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); }

bool named(const g::Shape& shape, int face, const std::string& prefix) {
  for (const std::string& name : shape.face_names(face)) {
    if (name.rfind(prefix, 0) == 0) {
      return true;
    }
  }
  return false;
}

std::vector<int> faces_starting(const g::Shape& shape, const std::string& prefix) {
  std::vector<int> faces;
  for (int i = 0; i < shape.face_count(); ++i) {
    if (named(shape, i, prefix)) {
      faces.push_back(i);
    }
  }
  return faces;
}

// The largest angle by which the side faces (named "<feature>:side(...")
// miss meeting the face at right angles along the section at their first
// (`at_start`) or last v: asin |m . n|, m the side's unit normal, n the
// face's; 64 points per side face. -1 when there is no side face.
template <class Normal>
double worst_miss(const g::Shape& loft, const std::string& feature, bool at_start, Normal normal) {
  double worst = -1.0;
  for (const int index : faces_starting(loft, feature + ":side(")) {
    const TopoDS_Face& face = loft.face(index);
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const double v = at_start ? v0 : v1;
    for (int k = 0; k <= 64; ++k) {
      const double u = u0 + (u1 - u0) * k / 64;
      gp_Pnt p;
      gp_Vec su;
      gp_Vec sv;
      surface->D1(u, v, p, su, sv);
      const gp_Vec m = su.Crossed(sv);
      if (m.Magnitude() <= 1.0e-12) {
        continue;
      }
      const gp_Vec n = normal(p);
      worst = std::max(worst, std::asin(std::min(1.0, std::abs(m.Normalized().Dot(n.Normalized())))));
    }
  }
  return worst;
}

gp_Vec up(const gp_Pnt&) { return gp_Vec(0, 0, 1); }

gp_Vec radial(const gp_Pnt& p) { return gp_Vec(p.X(), p.Y(), 0); }

void report(const char* what, double miss) {
  std::printf("%s: meets the face %.3g rad off a right angle\n", what, miss);
}

void test_circle_on_a_plane() {
  // A circle of radius 10 on the top of a 60 x 60 block (z = 0), the top
  // picked, to a circle of radius 5 at z = 30 on its sketch's plane, both
  // ends perpendicular: upright at both.
  const Region top_square = square(1, 60);
  const ShapePtr block = extruded("F2", top_square, -10, 0);
  const std::string top = g::face_name("F2", "end", top_square.name);
  g::LoftSpec spec;
  spec.feature = "F5";
  spec.solid = false;
  spec.sections = {chain({arc("c7", 0, 0, 0, 10, 0, 2 * kPi, true)}, true),
                   chain({arc("c8", 0, 0, 30, 5, 0, 2 * kPi, true)}, true)};
  spec.start = perpendicular(block, top);
  spec.end = perpendicular();
  const ShapePtr loft = g::loft(spec);
  CHECK(faces_starting(*loft, "F5:side(c7)").size() == 1);
  const double start = worst_miss(*loft, "F5", true, up);
  const double end = worst_miss(*loft, "F5", false, up);
  report("circle on a plane, start", start);
  report("circle on a plane, end", end);
  CHECK(start >= 0.0 && start <= kRightAngle);
  CHECK(end >= 0.0 && end <= kRightAngle);

  // The section off its face is refused in words.
  spec.sections.front() = chain({arc("c7", 0, 0, 1, 10, 0, 2 * kPi, true)}, true);
  CHECK(throws_with([&] { g::loft(spec); }, "off the face it is to leave at right angles"));
  // Curves in no plane without a face: no face to leave at right angles.
  spec.sections.front() = chain({arc("c7", 0, 0, 0, 10, 0, 2 * kPi, true)}, false);
  spec.start = perpendicular();
  CHECK(throws_with([&] { g::loft(spec); }, "leaving it at right angles needs the face it lies on"));
}

void test_rim_on_a_cylinder() {
  // The top of a post of radius 10 (z = 0 to 20) to a circle of radius 20
  // at z = 30, leaving the rim at right angles to the post's wall: outward,
  // level with the top.
  const ShapePtr post = extruded("F2", round(1, 0, 0, 10), 0, 20);
  g::LoftSpec spec;
  spec.feature = "F6";
  spec.sections = {face_of(post, "F2:end(r{c1})"), region_at(30, round(3, 0, 0, 20))};
  spec.start = perpendicular(post, "F2:side(c1)");
  const ShapePtr flared = g::loft(spec);
  CHECK(valid(*flared));
  const double miss = worst_miss(*flared, "F6", true, radial);
  report("rim on a cylinder", miss);
  CHECK(miss >= 0.0 && miss <= kRightAngle);
  // Free, the loft does not meet the wall at right angles.
  spec.start = g::LoftEnd();
  const ShapePtr free_loft = g::loft(spec);
  CHECK(worst_miss(*free_loft, "F6", true, radial) > 1.0e-2);
  // The section's own face (the top, a plane) without a face picked: it
  // leaves upright.
  spec.start = perpendicular(nullptr, std::string(), 2.0);
  const ShapePtr upright = g::loft(spec);
  const double own = worst_miss(*upright, "F6", true, up);
  report("rim on its own top face", own);
  CHECK(own >= 0.0 && own <= kRightAngle);

  // An open half circle on the wall at z = 20 to a half circle of radius
  // 20 at z = 30: a surface of curves, no corners at its ends.
  g::LoftSpec half;
  half.feature = "F7";
  half.solid = false;
  half.sections = {chain({arc("c4", 0, 0, 20, 10, 0, kPi, false)}, true),
                   chain({arc("c5", 0, 0, 30, 20, 0, kPi, false)}, true)};
  half.start = perpendicular(post, "F2:side(c1)");
  const ShapePtr sheet = g::loft(half);
  CHECK(faces_starting(*sheet, "F7:side(c4)").size() == 1);
  const double open = worst_miss(*sheet, "F7", true, radial);
  report("open half circle on a cylinder", open);
  CHECK(open >= 0.0 && open <= kRightAngle);
}

void test_rim_on_a_freeform_face() {
  // A barrel through circles of radius 20 at z = 0 and 40 and one of
  // radius 30 at z = 20 off the axis: a freeform side whose normals along
  // its top rim are no image of the rim. The rim, a circle of radius 20 at
  // z = 40, to a circle of radius 10 at z = 70, leaving at right angles to
  // the barrel's side.
  g::LoftSpec barrel_spec;
  barrel_spec.feature = "F3";
  g::LoftSection middle = region_at(20, round(2, 8, 3, 30));
  barrel_spec.sections = {region_at(0, round(1, 0, 0, 20)), middle, region_at(40, round(3, 0, 0, 20))};
  const ShapePtr barrel = g::loft(barrel_spec);
  const std::vector<int> sides = faces_starting(*barrel, "F3:side(");
  CHECK(sides.size() == 1);
  if (sides.size() != 1) {
    return;
  }
  const TopoDS_Face& side = barrel->face(sides.front());
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(side);
  double bounds[4] = {0, 0, 0, 0};
  BRepTools::UVBounds(side, bounds[0], bounds[1], bounds[2], bounds[3]);
  const std::string side_name = barrel->face_names(sides.front()).front();
  // The barrel's normal where a point of the rim projects onto its side.
  const auto barrel_normal = [&](const gp_Pnt& p) {
    GeomAPI_ProjectPointOnSurf projection(p, surface, bounds[0], bounds[1], bounds[2], bounds[3]);
    if (projection.NbPoints() == 0) {
      throw std::runtime_error("a point of the rim is not on the barrel");
    }
    double u = 0.0;
    double v = 0.0;
    projection.LowerDistanceParameters(u, v);
    gp_Pnt q;
    gp_Vec su;
    gp_Vec sv;
    surface->D1(u, v, q, su, sv);
    return su.Crossed(sv);
  };

  g::LoftSpec spec;
  spec.feature = "F8";
  spec.solid = false;
  spec.sections = {chain({arc("c6", 0, 0, 40, 20, 0, 2 * kPi, true)}, true),
                   chain({arc("c9", 0, 0, 70, 10, 0, 2 * kPi, true)}, true)};
  spec.start = perpendicular(barrel, side_name);
  const ShapePtr flared = g::loft(spec);
  const double miss = worst_miss(*flared, "F8", true, barrel_normal);
  report("rim on a freeform face", miss);
  CHECK(miss >= 0.0 && miss <= kRightAngle);
  // On its sketch's plane instead (no face): upright, so not at right
  // angles to the barrel, which slopes there.
  spec.start = perpendicular();
  const ShapePtr upright = g::loft(spec);
  CHECK(worst_miss(*upright, "F8", true, barrel_normal) > 1.0e-2);
}

void test_rails() {
  // The top of a 40 x 40 block (z = 0) to a 20 x 20 square at z = 30, both
  // ends perpendicular (the top its own face, the square its plane), with
  // rails at the corners that leave the top and reach the square upright:
  // the faces meet both planes at right angles all along.
  const Region top_square = square(1, 40);
  const ShapePtr block = extruded("F2", top_square, -10, 0);
  g::LoftSpec spec;
  spec.feature = "F9";
  spec.sections = {face_of(block, g::face_name("F2", "end", top_square.name)), region_at(30, square(5, 20))};
  spec.start = perpendicular();
  spec.end = perpendicular();
  for (const auto& [x, y] : {std::pair{-1.0, -1.0}, {1.0, -1.0}, {1.0, 1.0}, {-1.0, 1.0}}) {
    spec.rails.push_back(upright_rail(gp_Pnt(20 * x, 20 * y, 0), gp_Pnt(10 * x, 10 * y, 30)));
  }
  const ShapePtr railed = g::loft(spec);
  CHECK(valid(*railed));
  CHECK(faces_starting(*railed, "F9:side(").size() == 4);
  const double start = worst_miss(*railed, "F9", true, up);
  const double end = worst_miss(*railed, "F9", false, up);
  report("rails, start", start);
  report("rails, end", end);
  CHECK(start >= 0.0 && start <= kRightAngle);
  CHECK(end >= 0.0 && end <= kRightAngle);
  // Open sections take no rails.
  g::LoftSpec open;
  open.feature = "F10";
  open.solid = false;
  open.sections = {chain({arc("c4", 0, 0, 0, 10, 0, kPi, false)}, true),
                   chain({arc("c5", 0, 0, 30, 10, 0, kPi, false)}, true)};
  open.start = perpendicular();
  open.rails = {upright_rail(gp_Pnt(10, 0, 0), gp_Pnt(10, 0, 30))};
  CHECK(throws_with([&] { g::loft(open); }, "unsupported: loft rails with open sections"));
}

} // namespace
} // namespace test

int main() {
  test::guarded("circle on a plane", test::test_circle_on_a_plane);
  test::guarded("rim on a cylinder", test::test_rim_on_a_cylinder);
  test::guarded("rim on a freeform face", test::test_rim_on_a_freeform_face);
  test::guarded("rails", test::test_rails);
  if (test::failures == 0) {
    std::puts("test_loft_perpendicular: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
