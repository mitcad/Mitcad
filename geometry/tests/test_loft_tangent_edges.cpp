// SPDX-License-Identifier: MIT
// Tangent Edges: Merge (mitcad#2162). A circle of radius 30 lofted to a
// slot 160 x 40 (arc centres at x = +-60, radius 20) 50 mm above it: with
// the slot's lines and arcs joined into one smooth edge the side is one
// face of a valid solid, tangent-continuous (G1) everywhere: across every
// knot of its surface (the slot's former line-arc joins among them) and
// across its seam the normals on either side agree within 1e-6 rad, at
// heights all the way between the sections. The same with three sections
// (the Hermite surfaces), with a tangent end condition at a cylinder's top
// face (the skinned surfaces), and for a surface loft of the sketch curves.
// Kept, the side is four faces as before; a rectangle keeps its corner
// edges under Merge. The joined slot lies within 1e-7 mm of the slot.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <set>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepTools.hxx>
#include <Geom_BSplineSurface.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/surface_sweeps.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

constexpr double kG1 = 1.0e-6; // radians

g::Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

g::Segment arc(const std::string& name, double cx, double cy, double radius, double from, double to) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Arc;
  s.center = gp_Pnt2d(cx, cy);
  s.radius = radius;
  s.start_angle = from;
  s.end_angle = to;
  return s;
}

// The slot of the ticket counter-clockwise from its bottom line: c1 the
// bottom line, c2 the right arc, c3 the top line, c4 the left arc, keyed as
// the sketch keys them.
Region slot() {
  Region region;
  region.loops.emplace_back();
  auto& segments = region.loops.front().segments;
  segments.push_back(line("c1[c4,c2]", -60, -20, 60, -20));
  segments.push_back(arc("c2[c1,c3]", 60, 0, 20, -kPi / 2, kPi / 2));
  segments.push_back(line("c3[c2,c4]", 60, 20, -60, 20));
  segments.push_back(arc("c4[c3,c1]", -60, 0, 20, kPi / 2, 3 * kPi / 2));
  region.name = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
  return region;
}

Region round(int curve, double radius) {
  g::Segment s;
  s.name = "c" + std::to_string(curve);
  s.kind = g::SegmentKind::Circle;
  s.center = gp_Pnt2d(0, 0);
  s.radius = radius;
  Region region;
  region.loops.push_back(g::Loop{{s}});
  region.name = "r{" + s.name + "}";
  return region;
}

// A square of side `side` round the origin, lines c11..c14.
Region square(double side) {
  const double h = side / 2;
  Region region;
  region.loops.emplace_back();
  auto& segments = region.loops.front().segments;
  segments.push_back(line("c11[c14,c12]", -h, -h, h, -h));
  segments.push_back(line("c12[c11,c13]", h, -h, h, h));
  segments.push_back(line("c13[c12,c14]", h, h, -h, h));
  segments.push_back(line("c14[c13,c11]", -h, h, -h, -h));
  region.name = "r{c11[c14,c12],c12[c11,c13],c13[c12,c14],c14[c13,c11]}";
  return region;
}

g::LoftSection region_at(double z, const Region& region) {
  g::LoftSection section;
  section.frame.origin = gp_Pnt(0, 0, z);
  section.region = region;
  return section;
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

// The faces with a name that starts with `prefix`.
std::vector<int> faces_starting(const g::Shape& shape, const std::string& prefix) {
  std::vector<int> faces;
  for (int i = 0; i < shape.face_count(); ++i) {
    if (named(shape, i, prefix)) {
      faces.push_back(i);
    }
  }
  return faces;
}

int faces_called(const g::Shape& shape, const std::string& name) {
  int count = 0;
  for (int i = 0; i < shape.face_count(); ++i) {
    const auto names = shape.face_names(i);
    count += std::find(names.begin(), names.end(), name) != names.end() ? 1 : 0;
  }
  return count;
}

std::set<std::string> all_names(const g::Shape& shape) {
  std::set<std::string> names;
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      names.insert(name);
    }
  }
  return names;
}

gp_Vec normal_at(const BRepAdaptor_Surface& surface, double u, double v) {
  gp_Pnt p;
  gp_Vec su;
  gp_Vec sv;
  surface.D1(u, v, p, su, sv);
  return su.Crossed(sv);
}

// The largest angle between the face's normals on either side of each
// interior knot of its surface along the sections (u) and across its seam
// (u at its ends, the side being closed round), at heights from 2 % to 98 %
// of the way between its ends. `knots` gets the interior knots' count.
double worst_kink(const TopoDS_Face& face, int* knots = nullptr) {
  const BRepAdaptor_Surface surface(face);
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  std::vector<double> at;
  if (surface.GetType() == GeomAbs_BSplineSurface) {
    const occ::handle<Geom_BSplineSurface> spline = surface.BSpline();
    for (int i = 1; i <= spline->NbUKnots(); ++i) {
      const double u = spline->UKnot(i);
      if (u > u0 + 1.0e-9 * (u1 - u0) && u < u1 - 1.0e-9 * (u1 - u0)) {
        at.push_back(u);
      }
    }
  }
  if (knots != nullptr) {
    *knots = static_cast<int>(at.size());
  }
  const double step = 1.0e-9 * (u1 - u0);
  double worst = 0.0;
  for (int k = 0; k <= 12; ++k) {
    const double v = v0 + (v1 - v0) * (0.02 + 0.96 * k / 12);
    for (const double u : at) {
      worst = std::max(worst, normal_at(surface, u - step, v).Angle(normal_at(surface, u + step, v)));
    }
    worst = std::max(worst, normal_at(surface, u0 + step, v).Angle(normal_at(surface, u1 - step, v)));
  }
  return worst;
}

// The side of a merged loft: its one face named with `prefix`.
TopoDS_Face side_face(const g::Shape& shape, const std::string& prefix) {
  const std::vector<int> faces = faces_starting(shape, prefix);
  if (faces.size() != 1) {
    std::fprintf(stderr, "%zu faces named %s...\n", faces.size(), prefix.c_str());
    return {};
  }
  return shape.face(faces.front());
}

// The farthest of points along the slot at height z from the edges of the
// face named `cap`.
double off_slot(const g::Shape& shape, const std::string& cap, double z) {
  const std::vector<int> faces = shape.find_faces(cap);
  if (faces.size() != 1) {
    return 1.0e9;
  }
  const TopoDS_Wire boundary = BRepTools::OuterWire(shape.face(faces.front()));
  std::vector<gp_Pnt> points;
  for (int i = 0; i <= 20; ++i) {
    const double x = -60 + 120.0 * i / 20;
    points.emplace_back(x, -20, z);
    points.emplace_back(x, 20, z);
    const double a = -kPi / 2 + kPi * i / 20;
    points.emplace_back(60 + 20 * std::cos(a), 20 * std::sin(a), z);
    points.emplace_back(-60 - 20 * std::cos(a), 20 * std::sin(a), z);
  }
  double worst = 0.0;
  for (const gp_Pnt& p : points) {
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), boundary, Extrema_ExtFlag_MIN);
    if (!distance.IsDone()) {
      return 1.0e9;
    }
    worst = std::max(worst, distance.Value());
  }
  return worst;
}

void check_smooth_side(const g::Shape& body, const std::string& prefix, const char* what) {
  const TopoDS_Face side = side_face(body, prefix);
  CHECK(!side.IsNull());
  if (side.IsNull()) {
    return;
  }
  int knots = 0;
  const double kink = worst_kink(side, &knots);
  std::printf("%s: %d interior knots, normals within %.3g rad\n", what, knots, kink);
  CHECK(knots >= 4);
  CHECK(kink <= kG1);
}

g::LoftSpec circle_to_slot(bool merge) {
  g::LoftSpec spec;
  spec.feature = "F4";
  spec.sections = {region_at(0, round(5, 30)), region_at(50, slot())};
  spec.merge_tangent = merge;
  return spec;
}

void test_circle_to_slot() {
  const ShapePtr merged = g::loft(circle_to_slot(true));
  CHECK(valid(*merged));
  // The side and the two caps.
  CHECK(merged->face_count() == 3);
  CHECK(faces_called(*merged, "F4:side(c5)") == 1);
  CHECK(faces_called(*merged, "F4:start(r{c5})") == 1);
  CHECK(faces_called(*merged, g::face_name("F4", "end", slot().name)) == 1);
  check_smooth_side(*merged, "F4:side(", "circle to slot, merged");
  // The joined slot is the slot within 1e-7 mm.
  const double off = off_slot(*merged, g::face_name("F4", "end", slot().name), 50);
  std::printf("the joined slot lies within %.3g mm of the slot\n", off);
  CHECK(off <= 1.0e-7);

  // Kept: a side face for each of the slot's edges (and the right arc in
  // two where the circle's own start falls), as before.
  const ShapePtr kept = g::loft(circle_to_slot(false));
  CHECK(valid(*kept));
  const std::size_t kept_sides = faces_starting(*kept, "F4:side(").size();
  std::printf("circle to slot, kept: %zu side faces\n", kept_sides);
  CHECK(kept_sides >= 4 && kept->face_count() == static_cast<int>(kept_sides) + 2);
  // Between the slot's and the circle's prisms' volumes.
  const double v = g::volume(*merged);
  CHECK(v > kPi * 900 * 50 && v < (120 * 40 + kPi * 400) * 50);
}

void test_slot_first_names_its_first_segment() {
  // The slot first: smooth all round, its side is named after its segment
  // whose key sorts first, the bottom line, as that line's face is named
  // when the edges are kept; also when its loop starts elsewhere.
  g::LoftSpec spec;
  spec.feature = "F4";
  spec.sections = {region_at(0, slot()), region_at(50, round(5, 30))};
  spec.merge_tangent = true;
  const ShapePtr merged = g::loft(spec);
  CHECK(valid(*merged) && merged->face_count() == 3);
  CHECK(faces_called(*merged, "F4:side(c1[c4,c2])") == 1);
  check_smooth_side(*merged, "F4:side(", "slot to circle, merged");
  Region turned = slot();
  auto& segments = turned.loops.front().segments;
  std::rotate(segments.begin(), segments.begin() + 2, segments.end());
  spec.sections.front() = region_at(0, turned);
  const ShapePtr started_elsewhere = g::loft(spec);
  CHECK(faces_called(*started_elsewhere, "F4:side(c1[c4,c2])") == 1);
  spec.merge_tangent = false;
  const ShapePtr kept = g::loft(spec);
  CHECK(faces_called(*kept, "F4:side(c1[c4,c2])") == 1);
}

void test_three_sections() {
  // Circle, slot, circle: the Hermite surfaces through three sections.
  g::LoftSpec spec;
  spec.feature = "F6";
  spec.sections = {region_at(0, round(5, 30)), region_at(50, slot()), region_at(100, round(7, 30))};
  spec.merge_tangent = true;
  const ShapePtr merged = g::loft(spec);
  CHECK(valid(*merged));
  CHECK(merged->face_count() == 3);
  CHECK(faces_called(*merged, "F6:side(c5)") == 1);
  check_smooth_side(*merged, "F6:side(", "circle, slot, circle, merged");
  spec.merge_tangent = false;
  const ShapePtr kept = g::loft(spec);
  CHECK(valid(*kept) && faces_starting(*kept, "F6:side(").size() >= 4);
}

void test_tangent_end() {
  // From the top of a cylinder of radius 30 (20 high, up to z = 0),
  // tangent to its wall, to the slot 50 mm above: the skinned surfaces.
  g::ExtrudeSpec post;
  post.feature = "F2";
  post.regions = {round(1, 30)};
  post.start = -20;
  post.end = 0;
  const ShapePtr cylinder = g::extrude(post);
  g::LoftSpec spec;
  spec.feature = "F7";
  g::LoftSection top;
  top.kind = g::LoftSection::Kind::Face;
  top.body = cylinder;
  top.face = "F2:end(r{c1})";
  spec.sections = {top, region_at(50, slot())};
  spec.start.kind = g::LoftEnd::Kind::Tangent;
  spec.merge_tangent = true;
  const ShapePtr merged = g::loft(spec);
  CHECK(valid(*merged));
  CHECK(merged->face_count() == 3);
  CHECK(faces_starting(*merged, "F7:side(").size() == 1);
  check_smooth_side(*merged, "F7:side(", "tangent end to slot, merged");
  // It leaves the cylinder's wall tangentially: upright along the start.
  const TopoDS_Face side = side_face(*merged, "F7:side(");
  if (!side.IsNull()) {
    const BRepAdaptor_Surface surface(side);
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(side, u0, u1, v0, v1);
    double tilt = 0.0;
    for (int k = 0; k < 16; ++k) {
      const gp_Vec n = normal_at(surface, u0 + (u1 - u0) * (k + 0.5) / 16, v0).Normalized();
      tilt = std::max(tilt, std::abs(n.Z()));
    }
    std::printf("tangent end: leaves the wall tilted by %.3g\n", tilt);
    CHECK(tilt <= 1.0e-5);
  }
  spec.merge_tangent = false;
  const ShapePtr kept = g::loft(spec);
  CHECK(valid(*kept) && faces_starting(*kept, "F7:side(").size() >= 4);
}

void test_corners_stay() {
  // A square to a circle: the square's corners are sharp, so Merge keeps
  // its four edges, a face each. (The circle, one smooth edge, starts
  // where the ray towards the square's first corner meets it, so the
  // matching does not cut a side of the square where the circle's own
  // start falls, as it does when the edges are kept.)
  g::LoftSpec spec;
  spec.feature = "F8";
  spec.sections = {region_at(0, square(60)), region_at(40, round(5, 20))};
  spec.merge_tangent = true;
  const ShapePtr merged = g::loft(spec);
  spec.merge_tangent = false;
  const ShapePtr kept = g::loft(spec);
  CHECK(valid(*merged) && valid(*kept));
  CHECK(merged->face_count() == 6);
  for (const char* segment : {"c11[c14,c12]", "c12[c11,c13]", "c13[c12,c14]", "c14[c13,c11]"}) {
    CHECK(faces_called(*merged, g::face_name("F8", "side", segment)) == 1);
  }
  const std::set<std::string> names = all_names(*merged);
  CHECK(names.count("F8:start(" + square(60).name + ")") == 1 && names.count("F8:end(r{c5})") == 1);
  // (The circle runs by arc length when merged, so its points pair with
  // the square's a little otherwise.)
  CHECK_NEAR_TOL(g::volume(*merged), g::volume(*kept), 1e-2);
}

g::PathCurve path_line(const std::string& name, double x0, double y0, double x1, double y1, double z) {
  g::PathCurve piece;
  piece.name = name;
  piece.curve.kind = g::ModelCurveKind::Line;
  piece.curve.start = gp_Pnt(x0, y0, z);
  piece.curve.end = gp_Pnt(x1, y1, z);
  return piece;
}

g::PathCurve path_arc(const std::string& name, double cx, double radius, double from, double to, double z,
                      bool closed = false) {
  g::PathCurve piece;
  piece.name = name;
  piece.curve.kind = g::ModelCurveKind::Conic;
  piece.curve.center = gp_Pnt(cx, 0, z);
  piece.curve.normal = gp_Dir(0, 0, 1);
  piece.curve.x_axis = gp_Dir(1, 0, 0);
  piece.curve.major = radius;
  piece.curve.minor = radius;
  piece.curve.first = from;
  piece.curve.last = to;
  piece.curve.closed = closed;
  return piece;
}

void test_surface_loft_of_curves() {
  // The surface loft of sketch curves: the circle's chain to the slot's.
  const g::Path circle = {path_arc("c5", 0, 30, 0, 2 * kPi, 0, true)};
  const g::Path slot_curves = {path_line("c1", -60, -20, 60, -20, 50), path_arc("c2", 60, 20, -kPi / 2, kPi / 2, 50),
                               path_line("c3", 60, 20, -60, 20, 50),
                               path_arc("c4", -60, 20, kPi / 2, 3 * kPi / 2, 50)};
  const ShapePtr merged = g::loft_sheet("F9", {circle, slot_curves}, false, false, true);
  CHECK(merged->face_count() == 1);
  CHECK(faces_called(*merged, "F9:side(c5)") == 1);
  check_smooth_side(*merged, "F9:side(", "surface loft of curves, merged");
  const ShapePtr kept = g::loft_sheet("F9", {circle, slot_curves}, false, false);
  CHECK(kept->face_count() >= 4);
}

} // namespace
} // namespace test

int main() {
  test::guarded("circle to slot", test::test_circle_to_slot);
  test::guarded("slot first names its first segment", test::test_slot_first_names_its_first_segment);
  test::guarded("three sections", test::test_three_sections);
  test::guarded("tangent end", test::test_tangent_end);
  test::guarded("corners stay", test::test_corners_stay);
  test::guarded("surface loft of curves", test::test_surface_loft_of_curves);
  if (test::failures == 0) {
    std::puts("test_loft_tangent_edges: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
