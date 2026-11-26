// SPDX-License-Identifier: MIT
// A rounding wider than the face beside its edge, whose far edge goes on
// tangentially into a round wall bending away from the material (GitHub
// mitcad/Mitcad#667, mitcad#2419: a handle's hole whose straight side meets
// its round end through a short straight stretch, the 3 mm inside offset of
// the handle's outline leaving a stretch 0.104 wide before the corner).
//
// A block 10 deep (z 0..10) on a section in (x, y) from (-25, -25) to
// (15, 20), with a hole through it: its right side on x = 0 (the face A)
// from y = -15 up to y = R = 9, a straight stretch w wide along y = R to
// x = -w (the narrow face N), the hole's round wall of radius R about
// (-w, 0) from there round to (-w - R, 0) (the face C, tangent to N at N's
// far edge and bending away from the material), its left side down to
// y = -15 and its bottom. The concave edge between A and N (along z at
// (0, R)) is rounded with r = 3: the ball takes N away and rolls on over C,
// tangent to A and to C from inside (its centre r from A, R - r from C's
// axis). Before, the rounding failed with "the fillet is wider than the
// face beside its edge and would take that face away, but cannot meet the
// face beyond it": the path that lets the ball roll over the face beyond
// (mitcad#2208) only took material away under faces beyond that bend into
// the material, and C continued past N's far edge runs out of it.
//
// Cases: w = 0.104 (the report's proportions; C continued lies 0.6
// micrometres under N at most), 0.5 and 1.5. Each: one valid closed solid
// without a note, holding the whole input, no face left on N's plane, the
// rounding's face named after the picked edge, and the volume added the
// section F between A, N, C and the ball's arc along the whole depth,
// within 1e-5.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kWall = 9.0;   // C's radius R; N lies on y = R
constexpr double kRadius = 3.0; // the rounding
constexpr double kDepth = 10.0;
constexpr double kBottom = -15.0; // the hole's bottom

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

// The block with the hole, N `w` wide.
ShapePtr block(double w) {
  Region section;
  section.name = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
  Loop outer;
  outer.segments.push_back(line("c1[c4,c2]", -25.0, -25.0, 15.0, -25.0));
  outer.segments.push_back(line("c2[c1,c3]", 15.0, -25.0, 15.0, 20.0));
  outer.segments.push_back(line("c3[c2,c4]", 15.0, 20.0, -25.0, 20.0));
  outer.segments.push_back(line("c4[c3,c1]", -25.0, 20.0, -25.0, -25.0));
  section.loops.push_back(outer);
  Loop hole;
  hole.segments.push_back(line("c5[c10,c6]", 0.0, kBottom, 0.0, kWall));
  hole.segments.push_back(line("c6[c5,c7]", 0.0, kWall, -w, kWall));
  Segment arc;
  arc.name = "c7[c6,c8]";
  arc.kind = SegmentKind::Arc;
  arc.center = gp_Pnt2d(-w, 0.0);
  arc.radius = kWall;
  arc.start_angle = 0.5 * kPi;
  arc.end_angle = kPi;
  arc.start = gp_Pnt2d(-w, kWall);
  arc.end = gp_Pnt2d(-w - kWall, 0.0);
  hole.segments.push_back(arc);
  hole.segments.push_back(line("c8[c7,c10]", -w - kWall, 0.0, -w - kWall, kBottom));
  hole.segments.push_back(line("c10[c8,c5]", -w - kWall, kBottom, 0.0, kBottom));
  section.loops.push_back(hole);
  ExtrudeSpec spec;
  spec.feature = "F2";
  spec.frame = Frame();
  spec.direction = gp_Dir(0, 0, 1);
  spec.regions = {section};
  spec.start = 0.0;
  spec.end = kDepth;
  return extrude(spec);
}

// The straight edge along z at (x, y), its name; empty where there is none.
std::string edge_along_z(const Shape& body, double x, double y) {
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    if (curve.GetType() != GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    if (std::abs(a.X() - x) < 1e-6 && std::abs(b.X() - x) < 1e-6 && std::abs(a.Y() - y) < 1e-6 &&
        std::abs(b.Y() - y) < 1e-6 && std::abs(a.Z() - b.Z()) > 1e-3) {
      return body.edge_name(e);
    }
  }
  return {};
}

// Half the integral of x dy - y dx along a circle's arc about (cx, cy) of
// radius r from angle t0 to t1.
double arc_term(double cx, double cy, double r, double t0, double t1) {
  return 0.5 * (r * r * (t1 - t0) + cx * r * (std::sin(t1) - std::sin(t0)) - cy * r * (std::cos(t1) - std::cos(t0)));
}

double line_term(double x0, double y0, double x1, double y1) { return 0.5 * (x0 * y1 - x1 * y0); }

// The area of F: the ball's centre (-r, yc), R - r from C's axis (-w, 0);
// its contact on C at the angle t about that axis (the same angle about
// the ball's centre). F's boundary counter-clockwise: up A from the
// ball's contact (0, yc) to (0, R), along N to (-w, R), along C to the
// contact, back along the ball's arc to (0, yc).
double area_added(double w) {
  const double dx = w - kRadius;
  const double yc = std::sqrt((kWall - kRadius) * (kWall - kRadius) - dx * dx);
  const double t = std::atan2(yc, -kRadius + w);
  const double sum = line_term(0.0, yc, 0.0, kWall) + line_term(0.0, kWall, -w, kWall) +
                     arc_term(-w, 0.0, kWall, 0.5 * kPi, t) + arc_term(-kRadius, yc, kRadius, t, 0.0);
  return std::abs(sum);
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

// The faces of the shape on N's plane, y = R.
int faces_on_n(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()), false);
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    if (std::abs(std::abs(plane.Axis().Direction().Y()) - 1.0) < 1e-9 &&
        std::abs(plane.Location().Y() - kWall) < 1e-6) {
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

// The edge between A and N rounded with 3 as F7, checked as the header says.
void round_and_check(const char* label, double w) {
  const ShapePtr body = block(w);
  const std::string edge = edge_along_z(*body, 0.0, kWall);
  CHECK(!edge.empty());
  if (edge.empty()) {
    return;
  }
  FilletSet set;
  set.edges = {edge};
  set.radius = kRadius;
  ShapePtr result;
  try {
    result = fillet("F7", *body, {set});
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
  const double before = volume_of(body->occt());
  const double after = volume_of(shape);
  const double added = kDepth * area_added(w);
  std::printf("%s: volume %.6f -> %.6f, added %.6f, expected %.6f\n", label, before, after, after - before, added);
  CHECK_NEAR_TOL(after - before, added, 1e-5);
  // (The input wholly within the result: the rounding only adds.)
  BRepAlgoAPI_Cut left(body->occt(), shape);
  CHECK(left.IsDone());
  if (left.IsDone()) {
    CHECK(volume_of(left.Shape()) < 1e-6 * before);
  }
  // N gone; the rounding named after the picked edge.
  CHECK(faces_on_n(shape) == 0);
  CHECK(holds_face(*result, "F7:fillet(" + edge + ")"));
}

void test_report_proportions() { round_and_check("strip 0.104", 0.104); }

void test_half_wide_strip() { round_and_check("strip 0.5", 0.5); }

void test_wider_strip() { round_and_check("strip 1.5", 1.5); }

} // namespace
} // namespace test

int main() {
  test::guarded("test_report_proportions", test::test_report_proportions);
  test::guarded("test_half_wide_strip", test::test_half_wide_strip);
  test::guarded("test_wider_strip", test::test_wider_strip);
  if (test::failures == 0) {
    std::puts("test_fillet_narrow_face_hole_wall: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
