// SPDX-License-Identifier: MIT
// A rounding wider than the face beside its edge, whose far edge goes on
// tangentially into a round face (GitHub mitcad/Mitcad#560, mitcad#2208).
//
// A block 25 deep (z 0..25) on a section in (x, y): its bottom at y = 15
// from x = -19 to 0 (the face A), a riser 3 high down to y = 12 at x = 0
// (the narrow face N), a half disc of radius 15.5 about (15.5, 12) bulging
// down from there (the round face C, tangent to N at its far edge), a ledge
// at y = 12 out to x = 52, the sides up to y = 44. The concave edge between
// A and N is rounded with 11.9: the ball takes N away and rolls on over C,
// tangent to A and to C (its centre 11.9 under A, 27.4 from C's axis). The
// rounding adds the section F between A, N, C and the ball's arc along the
// whole depth. A corner of the block is cut off through its height, along
// the line through (p, 0) and (-19, 8.3) in (x, z): where F lies past that
// line (the cut face grown down to meet the rounding) nothing is added.
//
// Cases: no cut; the cut ending at p = -2.8, short of N (the reporter's
// design: the rounding ends partly on the start face, partly on the cut
// face); the cut moved past the origin to p = 2, cutting N and C too (the
// reporter's variant that built before); and the reporter's cut with the
// cut's vertical edge and the end face's left edge rounded with 9 first
// (the other order of the design's two fillets). Each: one valid closed
// solid without a note, holding the whole input, within the input's box,
// no face left on N's plane, the rounding's face named after the picked
// edge, and the volume added F's prism less what lies past the cut, within
// 1e-5 (1e-4 after the other rounding, which comes within 0.3 of the ball's
// contact on A at the far end). Before, the rounding was built on the body
// with N continued down past its far edge: the ball kept to N's plane and
// hung beside C as a sheet.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The section (x, y) and the depth.
constexpr double kLeft = -19.0;
constexpr double kA = 15.0;      // A's height
constexpr double kFar = 12.0;    // N's far edge
constexpr double kCentre = 15.5; // C's axis at (kCentre, kFar), its radius kCentre
constexpr double kRight = 52.0;
constexpr double kTop = 44.0;
constexpr double kDepth = 25.0;
constexpr double kRadius = 11.9;
// The cut's line in (x, z): through (p, 0) and (kLeft, kCutDepth).
constexpr double kCutDepth = 8.3;
// The other fillet's radius.
constexpr double kOther = 9.0;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return std::abs(props.Mass());
}

Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  Segment s;
  s.name = name;
  s.kind = SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

ShapePtr extruded(const std::string& feature, const Frame& frame, const gp_Dir& direction, Region region,
                  double end) {
  ExtrudeSpec spec;
  spec.feature = feature;
  spec.frame = frame;
  spec.direction = direction;
  spec.regions = {std::move(region)};
  spec.start = 0.0;
  spec.end = end;
  return extrude(spec);
}

// The block, its corner cut off through its height along the line through
// (p, 0) and (kLeft, kCutDepth) in (x, z) where `cut`.
ShapePtr block(bool cut, double p) {
  Region section;
  section.name = "r{c1[c18,c7],c7[c1,c9],c9[c7,c11],c11[c9,c14],c14[c11,c16],c16[c14,c18],c18[c16,c1]}";
  Loop loop;
  loop.segments.push_back(line("c1[c18,c7]", kLeft, kA, 0.0, kA));
  loop.segments.push_back(line("c7[c1,c9]", 0.0, kA, 0.0, kFar));
  Segment arc;
  arc.name = "c9[c7,c11]";
  arc.kind = SegmentKind::Arc;
  arc.center = gp_Pnt2d(kCentre, kFar);
  arc.radius = kCentre;
  arc.start_angle = kPi;
  arc.end_angle = 2.0 * kPi;
  arc.start = gp_Pnt2d(0.0, kFar);
  arc.end = gp_Pnt2d(2.0 * kCentre, kFar);
  loop.segments.push_back(arc);
  loop.segments.push_back(line("c11[c9,c14]", 2.0 * kCentre, kFar, kRight, kFar));
  loop.segments.push_back(line("c14[c11,c16]", kRight, kFar, kRight, kTop));
  loop.segments.push_back(line("c16[c14,c18]", kRight, kTop, kLeft, kTop));
  loop.segments.push_back(line("c18[c16,c1]", kLeft, kTop, kLeft, kA));
  section.loops.push_back(loop);
  const ShapePtr body = extruded("F2", Frame(), gp_Dir(0, 0, 1), section, kDepth);
  if (!cut) {
    return body;
  }
  // The triangle on the corner's side of the line, past the body: the
  // line's ends half its length further on, and a point beyond the corner.
  const double ax = p + 0.5 * (p - kLeft);
  const double az = -0.5 * kCutDepth;
  const double bx = kLeft - 0.5 * (p - kLeft);
  const double bz = kCutDepth + 0.5 * kCutDepth;
  const double cx = kLeft - 11.0;
  const double cz = -10.0;
  Region triangle;
  triangle.name = "r{k1[k3,k2],k2[k1,k3],k3[k2,k1]}";
  Loop corner;
  corner.segments.push_back(line("k1[k3,k2]", cx, cz, ax, az));
  corner.segments.push_back(line("k2[k1,k3]", ax, az, bx, bz));
  corner.segments.push_back(line("k3[k2,k1]", bx, bz, cx, cz));
  triangle.loops.push_back(corner);
  // Sketched on the plane y = 50 in (x, z), cut down through the block.
  Frame frame;
  frame.origin = gp_Pnt(0, 50, 0);
  frame.x_axis = gp_Dir(1, 0, 0);
  frame.y_axis = gp_Dir(0, 0, 1);
  const ShapePtr tool = extruded("F4", frame, gp_Dir(0, -1, 0), triangle, 70.0);
  return boolean(BooleanOp::Cut, {body.get()}, *tool).pieces.at(0).shape;
}

// The straight edge of the shape along `axis` (0 x, 1 y, 2 z) whose other
// two coordinates are (u, v) (in the order x, y, z without the axis'), its
// name; empty where there is none.
std::string edge_along(const Shape& body, int axis, double u, double v) {
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    if (curve.GetType() != GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    const int i = axis == 0 ? 2 : 1;
    const int j = axis == 2 ? 2 : 3;
    if (std::abs(a.Coord(i) - u) < 1e-6 && std::abs(b.Coord(i) - u) < 1e-6 && std::abs(a.Coord(j) - v) < 1e-6 &&
        std::abs(b.Coord(j) - v) < 1e-6 && std::abs(a.Coord(axis + 1) - b.Coord(axis + 1)) > 1e-3) {
      return body.edge_name(e);
    }
  }
  return {};
}

// The section F the rounding adds and how much of its prism lies past the
// cut (none without one): the ball's centre (xc, yc), its contact on C at
// x = xt.
struct Added {
  double radius = kRadius;
  double area = 0.0;
  double xc = 0.0;
  double yc = 0.0;
  double xt = 0.0;
};

// Half the integral of x dy - y dx along a circle's arc about (cx, cy) of
// radius r from angle t0 to t1.
double arc_term(double cx, double cy, double r, double t0, double t1) {
  return 0.5 * (r * r * (t1 - t0) + cx * r * (std::sin(t1) - std::sin(t0)) - cy * r * (std::cos(t1) - std::cos(t0)));
}

double line_term(double x0, double y0, double x1, double y1) { return 0.5 * (x0 * y1 - x1 * y0); }

Added section_added(double radius = kRadius) {
  Added f;
  f.radius = radius;
  f.yc = kA - radius;
  const double dy = kFar - f.yc;
  f.xc = kCentre - std::sqrt((kCentre + radius) * (kCentre + radius) - dy * dy);
  // The contact on C, by its angle about C's axis (between pi and 3 pi / 2).
  double t = std::atan2(f.yc - kFar, f.xc - kCentre);
  if (t < 0.0) {
    t += 2.0 * kPi;
  }
  const double tx = kCentre + kCentre * std::cos(t);
  const double ty = kFar + kCentre * std::sin(t);
  f.xt = tx;
  // F's boundary: the contact on A, along A to N, down N, along C to its
  // contact, back along the ball's arc (clockwise: the sum is negative).
  double sum = line_term(f.xc, kA, 0.0, kA) + line_term(0.0, kA, 0.0, kFar) + arc_term(kCentre, kFar, kCentre, kPi, t);
  sum += arc_term(f.xc, f.yc, radius, std::atan2(ty - f.yc, tx - f.xc), 0.5 * kPi);
  f.area = std::abs(sum);
  return f;
}

// F's height at x: from the ball's arc up to A (x <= 0) or to C.
double height_of(const Added& f, double x) {
  const double ball = f.yc + std::sqrt(std::max(0.0, f.radius * f.radius - (x - f.xc) * (x - f.xc)));
  const double top =
      x <= 0.0 ? kA : kFar - std::sqrt(std::max(0.0, kCentre * kCentre - (x - kCentre) * (x - kCentre)));
  return std::max(0.0, top - ball);
}

// The volume the rounding adds: F's prism less its part past the cut,
// where the cut's line runs through (p, 0) (Simpson's rule across F).
double added_volume(bool cut, double p, double radius = kRadius) {
  const Added f = section_added(radius);
  double past = 0.0;
  if (cut) {
    const int n = 40000;
    const double h = (f.xt - f.xc) / n;
    for (int i = 0; i <= n; ++i) {
      const double x = f.xc + i * h;
      const double depth = std::max(0.0, kCutDepth * (p - x) / (p - kLeft));
      const double w = (i == 0 || i == n) ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0);
      past += w * height_of(f, x) * depth;
    }
    past *= h / 3.0;
  }
  return kDepth * f.area - past;
}

// What an earlier rounding of the end face's left edge with `other` takes
// from the added prism (mitcad#2330): F's part, at the end face, past that
// rounding's arc about (kLeft + other, kDepth - other) in (x, z), towards
// the corner, where the rounding reaches past the ball's contact on A
// (Simpson's rule across F up to the arc's centre).
double past_corner_rounding(double other, double radius) {
  const Added f = section_added(radius);
  const double cx = kLeft + other;
  const double cz = kDepth - other;
  const double x1 = std::min(cx, f.xt);
  if (!(x1 > f.xc)) {
    return 0.0;
  }
  const int n = 40000;
  const double h = (x1 - f.xc) / n;
  double sum = 0.0;
  for (int i = 0; i <= n; ++i) {
    const double x = f.xc + i * h;
    const double dx = x - cx;
    const double arc = cz + std::sqrt(std::max(0.0, other * other - dx * dx));
    const double w = (i == 0 || i == n) ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0);
    sum += w * height_of(f, x) * std::max(0.0, kDepth - arc);
  }
  return sum * h / 3.0;
}

Bnd_Box box_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, false, false);
  return box;
}

bool closed(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1;
}

// The faces of the shape on the plane x = 0.
int faces_on_n(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()), false);
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    if (std::abs(std::abs(plane.Axis().Direction().X()) - 1.0) < 1e-9 && std::abs(plane.Location().X()) < 1e-6) {
      ++n;
    }
  }
  return n;
}

// The planar faces of the shape on A's plane (y = kA) facing up, out of
// material below them: the top of a lip the rounding left standing out
// past an earlier rounding of a corner of A (mitcad#2330); A itself faces
// down.
int faces_up_on_a(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    const BRepAdaptor_Surface surface(face, false);
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    double y = plane.Axis().Direction().Y();
    if (!plane.Direct()) {
      y = -y;
    }
    if (face.Orientation() == TopAbs_REVERSED) {
      y = -y;
    }
    if (std::abs(std::abs(y) - 1.0) < 1e-9 && std::abs(plane.Location().Y() - kA) < 1e-6 && y > 0.0) {
      ++n;
    }
  }
  return n;
}

// Whether a face's name is `name` or a piece of it (`name`#k).
bool holds_face(const Shape& shape, const std::string& name) {
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& held : shape.face_names(f)) {
      if (held == name || held.compare(0, name.size() + 1, name + "#") == 0) {
        return true;
      }
    }
  }
  return false;
}

// The picked edge of `body` rounded with `radius` (11.9) as F7, checked as
// the header says against `added` (relative `within`).
void round_and_check(const char* label, const Shape& body, double added, double within, double radius = kRadius) {
  const std::string edge = edge_along(body, 2, 0.0, kA);
  CHECK(!edge.empty());
  if (edge.empty()) {
    return;
  }
  FilletSet set;
  set.edges = {edge};
  set.radius = radius;
  ShapePtr result;
  try {
    result = fillet("F7", body, {set});
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: the fillet failed: %s\n", label, error.what());
    CHECK(false);
    return;
  }
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  const TopoDS_Shape& shape = result->occt();
  CHECK(closed(shape));
  CHECK(BRepCheck_Analyzer(shape).IsValid());
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "%s: note: %s\n", label, note.c_str());
  }
  CHECK(result->notes().empty());
  const double before = volume_of(body.occt());
  const double after = volume_of(shape);
  std::printf("%s: volume %.6f -> %.6f, added %.6f, expected %.6f\n", label, before, after, after - before, added);
  CHECK_NEAR_TOL(after - before, added, within);
  // (The input wholly within the result: the rounding only adds.)
  BRepAlgoAPI_Cut left(body.occt(), shape);
  CHECK(left.IsDone());
  if (left.IsDone()) {
    CHECK(volume_of(left.Shape()) < 1e-6 * before);
  }
  const Bnd_Box in = box_of(body.occt());
  const Bnd_Box out = box_of(shape);
  double ix0 = 0, iy0 = 0, iz0 = 0, ix1 = 0, iy1 = 0, iz1 = 0;
  double ox0 = 0, oy0 = 0, oz0 = 0, ox1 = 0, oy1 = 0, oz1 = 0;
  in.Get(ix0, iy0, iz0, ix1, iy1, iz1);
  out.Get(ox0, oy0, oz0, ox1, oy1, oz1);
  const double slack = 1e-6;
  CHECK(ox0 >= ix0 - slack && oy0 >= iy0 - slack && oz0 >= iz0 - slack);
  CHECK(ox1 <= ix1 + slack && oy1 <= iy1 + slack && oz1 <= iz1 + slack);
  // N gone; the rounding named after the picked edge.
  CHECK(faces_on_n(shape) == 0);
  CHECK(holds_face(*result, "F7:fillet(" + edge + ")"));
  // No lip standing out past an earlier rounding (mitcad#2330).
  const int up = faces_up_on_a(shape);
  if (up != 0) {
    std::fprintf(stderr, "%s: %d face(s) on A's plane facing up: a step\n", label, up);
  }
  CHECK(up == 0);
}

// The cut block with the cut's vertical edge with the left face and the
// end face's left edge rounded with `other` as F6; null where that fails.
ShapePtr other_rounded(double other) {
  const ShapePtr cut = block(true, -2.8);
  const std::string vertical = edge_along(*cut, 1, kLeft, kCutDepth);
  const std::string end = edge_along(*cut, 1, kLeft, kDepth);
  CHECK(!vertical.empty());
  CHECK(!end.empty());
  if (vertical.empty() || end.empty()) {
    return nullptr;
  }
  FilletSet set;
  set.edges = {vertical, end};
  set.radius = other;
  ShapePtr body;
  try {
    body = fillet("F6", *cut, {set});
  } catch (const std::exception& error) {
    std::fprintf(stderr, "the other fillet failed: %s\n", error.what());
    CHECK(false);
    return nullptr;
  }
  CHECK(body != nullptr);
  return body;
}

void test_rounding_over_the_round_face() {
  const ShapePtr body = block(false, 0.0);
  round_and_check("no cut", *body, added_volume(false, 0.0), 1e-5);
}

void test_rounding_ending_on_the_start_and_the_cut_face() {
  const ShapePtr body = block(true, -2.8);
  round_and_check("cut short of the narrow face", *body, added_volume(true, -2.8), 1e-5);
}

void test_rounding_ending_on_the_cut_face_past_the_origin() {
  const ShapePtr body = block(true, 2.0);
  round_and_check("cut past the origin", *body, added_volume(true, 2.0), 1e-5);
}

void test_rounding_after_the_other_fillet() {
  // The cut's vertical edge with the left face and the end face's left
  // edge, rounded with 9 as F6.
  if (const ShapePtr body = other_rounded(kOther)) {
    round_and_check("after the other fillet", *body, added_volume(true, -2.8), 1e-4);
  }
}

// GitHub #608 (mitcad#2330): the rounding with 14.2 (its contact on A at
// x = -12.0) after the other fillet with 9.5, whose rounding of the end
// face's left edge reaches A as far as x = -9.5, past that contact (its
// rounding of the cut's edge stays short of it, to x = -13.8). The
// rounding's contact runs off A onto the earlier rounding near the end
// face; the material it adds ends at that rounding continued down past A
// (as where the fillets come in the other order), no lip standing out past
// it on the end face's plane, no face on A's plane facing up. The volume
// added: F's prism less its part past the cut and its part past the
// earlier rounding.
void test_rounding_after_the_other_fillet_reaching_past_its_contact() {
  constexpr double other = 9.5;
  constexpr double radius = 14.2;
  CHECK(kLeft + other > section_added(radius).xc);
  if (const ShapePtr body = other_rounded(other)) {
    round_and_check("after the other fillet past the contact", *body,
                    added_volume(true, -2.8, radius) - past_corner_rounding(other, radius), 1e-5, radius);
  }
}

// The other order of the case above (mitcad#2330): the rounding with 14.2
// first, then the other fillet with 9.5. Its rounding of the end face's
// left edge ends on A's plane, which does not reach across it there (the
// rounding with 14.2 runs into A tangentially short of the corner); it runs
// on down through that rounding's material, so that no lip of it stands
// out past the other rounding (the bundle check of 23.11 found three faces
// facing up on the reported design in this order). One valid closed solid
// without a note, no face on A's plane facing up, the volume that of the
// first order within 1e-6.
void test_other_fillet_after_the_rounding_reaching_past_its_contact() {
  constexpr double other = 9.5;
  constexpr double radius = 14.2;
  FilletSet set;
  set.radius = radius;
  ShapePtr first_order;
  if (const ShapePtr body = other_rounded(other)) {
    set.edges = {edge_along(*body, 2, 0.0, kA)};
    try {
      first_order = fillet("F7", *body, {set});
    } catch (const std::exception& error) {
      std::fprintf(stderr, "the rounding after the other fillet failed: %s\n", error.what());
    }
  }
  const ShapePtr cut = block(true, -2.8);
  set.edges = {edge_along(*cut, 2, 0.0, kA)};
  ShapePtr rounded;
  ShapePtr second_order;
  try {
    rounded = fillet("F7", *cut, {set});
    FilletSet then;
    then.edges = {edge_along(*rounded, 1, kLeft, kCutDepth), edge_along(*rounded, 1, kLeft, kDepth)};
    CHECK(!then.edges[0].empty() && !then.edges[1].empty());
    then.radius = other;
    second_order = fillet("F6", *rounded, {then});
  } catch (const std::exception& error) {
    std::fprintf(stderr, "the other order failed: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK(second_order != nullptr);
  if (!second_order) {
    return;
  }
  const TopoDS_Shape& shape = second_order->occt();
  CHECK(closed(shape));
  CHECK(BRepCheck_Analyzer(shape).IsValid());
  for (const std::string& note : second_order->notes()) {
    std::fprintf(stderr, "other order: note: %s\n", note.c_str());
  }
  CHECK(second_order->notes().empty());
  const int up = faces_up_on_a(shape);
  if (up != 0) {
    std::fprintf(stderr, "other order: %d face(s) on A's plane facing up: a step\n", up);
  }
  CHECK(up == 0);
  CHECK(first_order != nullptr);
  if (first_order) {
    const double one = volume_of(first_order->occt());
    const double two = volume_of(shape);
    std::printf("both orders: %.6f and %.6f\n", one, two);
    CHECK_NEAR_TOL(two, one, 1e-6);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_rounding_over_the_round_face", test::test_rounding_over_the_round_face);
  test::guarded("test_rounding_ending_on_the_start_and_the_cut_face",
                test::test_rounding_ending_on_the_start_and_the_cut_face);
  test::guarded("test_rounding_ending_on_the_cut_face_past_the_origin",
                test::test_rounding_ending_on_the_cut_face_past_the_origin);
  test::guarded("test_rounding_after_the_other_fillet", test::test_rounding_after_the_other_fillet);
  test::guarded("test_rounding_after_the_other_fillet_reaching_past_its_contact",
                test::test_rounding_after_the_other_fillet_reaching_past_its_contact);
  test::guarded("test_other_fillet_after_the_rounding_reaching_past_its_contact",
                test::test_other_fillet_after_the_rounding_reaching_past_its_contact);
  if (test::failures == 0) {
    std::puts("test_fillet_narrow_face_tangent_beyond: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
