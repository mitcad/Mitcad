// SPDX-License-Identifier: MIT
// Drawings (mitcad#471): views of a block with a counterbored through hole
// with hidden lines removed, its section, and the round holes a drawing
// tabulates. The block is 40 x 30 x 20 from the origin; the hole of
// diameter 6.6 at (20, 15) runs along Z, counterbored 11 wide and 6.8 deep
// from the top.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepLib.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ElSLib.hxx>
#include <Geom2d_Curve.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Poly_Triangulation.hxx>
#include <ShapeUpgrade_ShapeDivideClosed.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Torus.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../src/drawing_outline.hpp"
#include "check.hpp"
#include "mitcad/geometry/drawing.hpp"
#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/sketch_ref.hpp"
#include "bundled_test.hpp"

namespace {

using namespace mitcad::geometry;

TopoDS_Shape cylinder(double x, double y, double z0, double z1, double radius) {
  return BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x, y, z0), gp_Dir(0.0, 0.0, 1.0)), radius, z1 - z0).Shape();
}

ShapePtr counterbored_block() {
  TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), 40.0, 30.0, 20.0).Shape();
  block = BRepAlgoAPI_Cut(block, cylinder(20.0, 15.0, -1.0, 21.0, 3.3)).Shape();
  block = BRepAlgoAPI_Cut(block, cylinder(20.0, 15.0, 13.2, 21.0, 5.5)).Shape();
  return std::make_shared<Shape>(block);
}

ShapePtr blind_hole_block() {
  TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), 40.0, 30.0, 20.0).Shape();
  block = BRepAlgoAPI_Cut(block, cylinder(10.0, 10.0, 12.0, 21.0, 2.5)).Shape();
  return std::make_shared<Shape>(block);
}

DrawingViewSpec view(const gp_Dir& toward_eye, const gp_Dir& x_axis, bool hidden) {
  DrawingViewSpec spec;
  spec.toward_eye = toward_eye;
  spec.x_axis = x_axis;
  spec.hidden = hidden;
  return spec;
}

bool near_value(double a, double b) { return std::abs(a - b) <= 1e-6; }

// Every visible line lies on the sides of the rectangle [0, w] x [0, h],
// and together they cover each side.
void check_outline(const DrawnView& drawn, double w, double h, const char* name) {
  double covered[4] = {0.0, 0.0, 0.0, 0.0}; // bottom, right, top, left
  int lines = 0;
  for (const DrawnCurve& c : drawn.curves) {
    if (!c.visible || c.type != DrawnCurve::Type::Line) {
      continue;
    }
    ++lines;
    const Point2 a = c.points[0];
    const Point2 b = c.points[1];
    const double length = std::hypot(b[0] - a[0], b[1] - a[1]);
    if (near_value(a[1], 0.0) && near_value(b[1], 0.0)) {
      covered[0] += length;
    } else if (near_value(a[0], w) && near_value(b[0], w)) {
      covered[1] += length;
    } else if (near_value(a[1], h) && near_value(b[1], h)) {
      covered[2] += length;
    } else if (near_value(a[0], 0.0) && near_value(b[0], 0.0)) {
      covered[3] += length;
    } else {
      std::fprintf(stderr, "%s: a visible line off the outline: (%.9g, %.9g) - (%.9g, %.9g)\n", name, a[0], a[1],
                   b[0], b[1]);
      ++test::failures;
    }
  }
  CHECK(lines >= 4);
  // Overlapping pieces (the counterbore's rim on the top edge) may add.
  CHECK(covered[0] >= w - 1e-6);
  CHECK(covered[1] >= h - 1e-6);
  CHECK(covered[2] >= w - 1e-6);
  CHECK(covered[3] >= h - 1e-6);
}

bool has_circle(const DrawnView& drawn, double cx, double cy, double radius, bool visible) {
  for (const DrawnCurve& c : drawn.curves) {
    if (c.type == DrawnCurve::Type::Arc && c.visible == visible && near_value(c.center[0], cx) &&
        near_value(c.center[1], cy) && near_value(c.radius, radius) && c.end - c.start > 2.0 * test::kPi - 1e-9) {
      return true;
    }
  }
  return false;
}

// A line along x = `x` from y0 to y1, hidden or seen.
bool has_line(const DrawnView& drawn, double x, double y0, double y1, bool visible) {
  for (const DrawnCurve& c : drawn.curves) {
    if (c.visible != visible || c.type != DrawnCurve::Type::Line) {
      continue;
    }
    const Point2 a = c.points[0];
    const Point2 b = c.points[1];
    if (near_value(a[0], x) && near_value(b[0], x) && near_value(std::min(a[1], b[1]), y0) &&
        near_value(std::max(a[1], b[1]), y1)) {
      return true;
    }
  }
  return false;
}

int hidden_count(const DrawnView& drawn) {
  int n = 0;
  for (const DrawnCurve& c : drawn.curves) {
    n += c.visible ? 0 : 1;
  }
  return n;
}

void test_three_views() {
  const ShapePtr block = counterbored_block();
  // From above: the outline and both circles of the hole, seen.
  const DrawnView top = project_view({block}, view(gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), false));
  CHECK(!top.polygonal);
  check_outline(top, 40.0, 30.0, "top");
  CHECK(has_circle(top, 20.0, 15.0, 5.5, true));
  CHECK(has_circle(top, 20.0, 15.0, 3.3, true));
  CHECK(hidden_count(top) == 0);

  // From the front (-Y, x along X, up along Z): the outline; the hole's
  // walls hidden, drawn only when asked.
  const DrawnView front = project_view({block}, view(gp_Dir(0, -1, 0), gp_Dir(1, 0, 0), false));
  check_outline(front, 40.0, 20.0, "front");
  CHECK(hidden_count(front) == 0);
  const DrawnView dashed = project_view({block}, view(gp_Dir(0, -1, 0), gp_Dir(1, 0, 0), true));
  check_outline(dashed, 40.0, 20.0, "front with hidden lines");
  CHECK(has_line(dashed, 16.7, 0.0, 13.2, false));
  CHECK(has_line(dashed, 23.3, 0.0, 13.2, false));
  CHECK(has_line(dashed, 14.5, 13.2, 20.0, false));
  CHECK(has_line(dashed, 25.5, 13.2, 20.0, false));

  // From the right (+X, x along Y, up along Z).
  const DrawnView right = project_view({block}, view(gp_Dir(1, 0, 0), gp_Dir(0, 1, 0), false));
  check_outline(right, 30.0, 20.0, "right");
  CHECK(hidden_count(right) == 0);

  // The polygonal algorithm: the same outline within its deflection.
  DrawingViewSpec polygonal = view(gp_Dir(0, -1, 0), gp_Dir(1, 0, 0), false);
  polygonal.exact = false;
  const DrawnView poly = project_view({block}, polygonal);
  CHECK(poly.polygonal);
  double xmin = 1e9, xmax = -1e9, ymin = 1e9, ymax = -1e9;
  for (const DrawnCurve& c : poly.curves) {
    for (const Point2& p : c.points) {
      xmin = std::min(xmin, p[0]);
      xmax = std::max(xmax, p[0]);
      ymin = std::min(ymin, p[1]);
      ymax = std::max(ymax, p[1]);
    }
  }
  CHECK(std::abs(xmin) < 1e-3 && std::abs(xmax - 40.0) < 1e-3);
  CHECK(std::abs(ymin) < 1e-3 && std::abs(ymax - 20.0) < 1e-3);
}

double area(const std::vector<Point2>& loop) {
  double a = 0.0;
  for (std::size_t i = 0; i < loop.size(); ++i) {
    const Point2& p = loop[i];
    const Point2& q = loop[(i + 1) % loop.size()];
    a += p[0] * q[1] - q[0] * p[1];
  }
  return a / 2.0;
}

void test_section_view() {
  // Through the hole's axis, looking from the front at what is behind.
  DrawingViewSpec spec = view(gp_Dir(0, -1, 0), gp_Dir(1, 0, 0), false);
  spec.section = DrawingSection{gp_Pnt(20.0, 15.0, 0.0), gp_Dir(0, -1, 0)};
  const DrawnView cut = project_view({counterbored_block()}, spec);
  CHECK(cut.cuts.size() == 2);
  double total = 0.0;
  for (const CutFace& face : cut.cuts) {
    double face_area = 0.0;
    for (const std::vector<Point2>& loop : face) {
      face_area += area(loop);
    }
    total += std::abs(face_area);
  }
  CHECK_NEAR(total, 40.0 * 20.0 - 6.6 * 13.2 - 11.0 * 6.8);
  // The hole's walls in the cut, seen.
  CHECK(has_line(cut, 16.7, 0.0, 13.2, true));
  CHECK(has_line(cut, 25.5, 13.2, 20.0, true));
}

// A stepped annular flange about Z (mitcad#2479): a disc of radius 150 from
// z 0 to 20, a hub of radius 100 on it up to z 50, a bore of radius 80
// through both, and eight bolt holes of radius 10 on a circle of radius
// 130, one of them at (0, 130) and one at (0, -130).
ShapePtr stepped_flange() {
  TopoDS_Shape flange = BRepAlgoAPI_Fuse(cylinder(0.0, 0.0, 0.0, 20.0, 150.0), cylinder(0.0, 0.0, 20.0, 50.0, 100.0)).Shape();
  flange = BRepAlgoAPI_Cut(flange, cylinder(0.0, 0.0, -1.0, 51.0, 80.0)).Shape();
  for (int i = 0; i < 8; ++i) {
    const double a = test::kPi / 4.0 * i;
    flange = BRepAlgoAPI_Cut(flange, cylinder(130.0 * std::cos(a), 130.0 * std::sin(a), -1.0, 21.0, 10.0)).Shape();
  }
  return std::make_shared<Shape>(flange);
}

// The cut faces' total area and their lowest x.
std::pair<double, double> cut_extent(const DrawnView& drawn) {
  double total = 0.0;
  double lowest = 1e9;
  for (const CutFace& face : drawn.cuts) {
    double face_area = 0.0;
    for (const std::vector<Point2>& loop : face) {
      face_area += area(loop);
      for (const Point2& p : loop) {
        lowest = std::min(lowest, p[0]);
      }
    }
    total += std::abs(face_area);
  }
  return {total, lowest};
}

void test_bounded_section_view() {
  // The line drawn on a view looking down the flange's axis from just
  // outside its upper rim (0, 160) to near its middle (0, 5): the plane
  // x = 0 seen from +x, the view's x the model's y, its y the model's z.
  const ShapePtr flange = stepped_flange();
  DrawingViewSpec spec = view(gp_Dir(1, 0, 0), gp_Dir(0, 1, 0), false);
  spec.section = DrawingSection{gp_Pnt(0.0, 160.0, 0.0), gp_Dir(1, 0, 0)};
  const DrawnView full = project_view({flange}, spec);
  DrawingSectionBound bound;
  bound.along = gp_Dir(0, -1, 0);
  bound.from = 0.0;
  bound.to = 155.0;
  spec.section_bound = bound;
  const DrawnView bounded = project_view({flange}, spec);

  // The whole plane: the upper and the lower profile, the bolt holes on
  // both sides cut open.
  const auto [full_area, full_lowest] = cut_extent(full);
  CHECK(full.cuts.size() == 4);
  CHECK_NEAR_TOL(full_area, 2.0 * (70.0 * 20.0 - 20.0 * 20.0 + 20.0 * 30.0), 1e-6);
  CHECK(full_lowest < -149.0);
  CHECK(has_line(full, 120.0, 0.0, 20.0, true) && has_line(full, 140.0, 0.0, 20.0, true));
  CHECK(has_line(full, -120.0, 0.0, 20.0, true) && has_line(full, -140.0, 0.0, 20.0, true));

  // Limited to the line: the upper profile alone is cut and hatched, the
  // lower half stays whole, seen from outside (its bolt hole hidden).
  const auto [bounded_area, bounded_lowest] = cut_extent(bounded);
  CHECK(bounded.cuts.size() == 2);
  CHECK_NEAR_TOL(bounded_area, 70.0 * 20.0 - 20.0 * 20.0 + 20.0 * 30.0, 1e-6);
  CHECK(bounded_lowest > 80.0 - 1e-6);
  CHECK(has_line(bounded, 120.0, 0.0, 20.0, true) && has_line(bounded, 140.0, 0.0, 20.0, true));
  CHECK(!has_line(bounded, -120.0, 0.0, 20.0, true) && !has_line(bounded, -140.0, 0.0, 20.0, true));
  // The lower half's outline is seen: its rim from z 0 to 20.
  CHECK(has_line(bounded, -150.0, 0.0, 20.0, true));

  // A line beyond the rim cuts nothing: the flange seen whole from +x.
  bound.from = -100.0;
  bound.to = -50.0;
  spec.section_bound = bound;
  const DrawnView beyond = project_view({flange}, spec);
  CHECK(beyond.cuts.empty());
  CHECK(!has_line(beyond, 120.0, 0.0, 20.0, true) && !has_line(beyond, -120.0, 0.0, 20.0, true));
  CHECK(has_line(beyond, 150.0, 0.0, 20.0, true) && has_line(beyond, -150.0, 0.0, 20.0, true));
}

void test_round_holes() {
  const std::vector<RoundHole> holes = round_holes(*counterbored_block());
  CHECK(holes.size() == 1);
  if (holes.size() == 1) {
    const RoundHole& h = holes.front();
    CHECK_NEAR(h.diameter, 6.6);
    CHECK(h.through);
    CHECK_NEAR(h.cbore_diameter, 11.0);
    CHECK_NEAR(h.cbore_depth, 6.8);
    CHECK(h.csink_diameter == 0.0);
    CHECK(h.opening.Distance(gp_Pnt(20.0, 15.0, 20.0)) < 1e-6);
    CHECK(h.direction.IsEqual(gp_Dir(0, 0, -1), 1e-9));
    CHECK_NEAR(h.depth, 20.0);
  }
  const std::vector<RoundHole> blind = round_holes(*blind_hole_block());
  CHECK(blind.size() == 1);
  if (blind.size() == 1) {
    const RoundHole& h = blind.front();
    CHECK_NEAR(h.diameter, 5.0);
    CHECK(!h.through);
    CHECK_NEAR(h.depth, 8.0);
    CHECK(h.opening.Distance(gp_Pnt(10.0, 10.0, 20.0)) < 1e-6);
    CHECK(h.direction.IsEqual(gp_Dir(0, 0, -1), 1e-9));
    CHECK(h.cbore_diameter == 0.0);
  }
  // A boss is no hole.
  TopoDS_Shape boss = BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), 10.0, 10.0, 10.0).Shape();
  CHECK(round_holes(Shape(cylinder(0.0, 0.0, 0.0, 10.0, 3.0))).empty());
  CHECK(round_holes(Shape(boss)).empty());
}

// --- The quick path (mitcad#962) ------------------------------------------------

using Segments = std::vector<std::array<Point2, 2>>;

void add_chords(Segments& segments, const std::vector<Point2>& points, bool closed) {
  const std::size_t n = points.size();
  for (std::size_t k = 0; k + 1 < n; ++k) {
    segments.push_back({points[k], points[k + 1]});
  }
  if (closed) {
    segments.push_back({points[n - 1], points[0]});
  }
}

std::vector<Point2> on_circle(double cx, double cy, double radius, double from, double to, int chords,
                              bool closed) {
  std::vector<Point2> points;
  const int n = closed ? chords : chords + 1;
  for (int k = 0; k < n; ++k) {
    const double a = from + (to - from) * k / chords;
    points.push_back({cx + radius * std::cos(a), cy + radius * std::sin(a)});
  }
  return points;
}

int count_of(const std::vector<DrawnCurve>& curves, DrawnCurve::Type type) {
  return static_cast<int>(
      std::count_if(curves.begin(), curves.end(), [&](const DrawnCurve& c) { return c.type == type; }));
}

void test_fit_segments() {
  // A rectangle, each side in three pieces: four lines.
  Segments rectangle;
  const Point2 corners[] = {{0.0, 0.0}, {30.0, 0.0}, {30.0, 20.0}, {0.0, 20.0}};
  for (int k = 0; k < 4; ++k) {
    const Point2 a = corners[k];
    const Point2 b = corners[(k + 1) % 4];
    const auto at = [&](double t) { return Point2{a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t}; };
    for (int s = 0; s < 3; ++s) {
      rectangle.push_back({at(s / 3.0), at((s + 1) / 3.0)});
    }
  }
  const std::vector<DrawnCurve> sides = fit_segments(rectangle, 0.01);
  CHECK(sides.size() == 4);
  CHECK(count_of(sides, DrawnCurve::Type::Line) == 4);
  double length = 0.0;
  for (const DrawnCurve& c : sides) {
    if (c.type == DrawnCurve::Type::Line) {
      length += std::hypot(c.points[1][0] - c.points[0][0], c.points[1][1] - c.points[0][1]);
    }
  }
  CHECK_NEAR(length, 100.0);

  // A circle in 32 chords (their sagitta 0.048): one whole arc.
  Segments circle;
  add_chords(circle, on_circle(5.0, -3.0, 10.0, 0.0, 2.0 * test::kPi, 32, true), true);
  const std::vector<DrawnCurve> whole = fit_segments(circle, 0.06);
  CHECK(whole.size() == 1);
  if (whole.size() == 1) {
    const DrawnCurve& c = whole.front();
    CHECK(c.type == DrawnCurve::Type::Arc);
    CHECK_NEAR(c.center[0], 5.0);
    CHECK_NEAR(c.center[1], -3.0);
    CHECK_NEAR(c.radius, 10.0);
    CHECK_NEAR(c.end - c.start, 2.0 * test::kPi);
  }

  // A hexagon's corners lie on a circle, its sides far from it: six lines.
  Segments hexagon;
  add_chords(hexagon, on_circle(0.0, 0.0, 3.0, 0.0, 2.0 * test::kPi, 6, true), true);
  const std::vector<DrawnCurve> hex = fit_segments(hexagon, 0.06);
  CHECK(hex.size() == 6);
  CHECK(count_of(hex, DrawnCurve::Type::Line) == 6);

  // A quarter of a circle in eight chords, then a line on from it along its
  // tangent: an arc and a line.
  std::vector<Point2> path = on_circle(0.0, 0.0, 10.0, -test::kPi / 2.0, 0.0, 8, false);
  path.push_back({10.0, 15.0});
  Segments bend;
  add_chords(bend, path, false);
  const std::vector<DrawnCurve> pieces = fit_segments(bend, 0.06);
  CHECK(pieces.size() == 2);
  CHECK(count_of(pieces, DrawnCurve::Type::Arc) == 1);
  CHECK(count_of(pieces, DrawnCurve::Type::Line) == 1);
  for (const DrawnCurve& c : pieces) {
    if (c.type == DrawnCurve::Type::Arc) {
      CHECK_NEAR(c.radius, 10.0);
      CHECK_NEAR(c.end - c.start, test::kPi / 2.0);
      CHECK_NEAR(std::cos(c.start), 0.0);
      CHECK_NEAR(std::sin(c.start), -1.0);
    }
  }
}

double segment_distance(const Point2& p, const Point2& a, const Point2& b) {
  const double dx = b[0] - a[0];
  const double dy = b[1] - a[1];
  const double length2 = dx * dx + dy * dy;
  double t = length2 > 0.0 ? ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length2 : 0.0;
  t = std::max(0.0, std::min(1.0, t));
  return std::hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

Point2 arc_point(const DrawnCurve& c, double angle) {
  return {c.center[0] + c.radius * std::cos(angle), c.center[1] + c.radius * std::sin(angle)};
}

double distance_to(const DrawnCurve& c, const Point2& p) {
  if (c.type == DrawnCurve::Type::Line) {
    return segment_distance(p, c.points[0], c.points[1]);
  }
  if (c.type == DrawnCurve::Type::Arc) {
    const double dx = p[0] - c.center[0];
    const double dy = p[1] - c.center[1];
    double along = std::fmod(std::atan2(dy, dx) - c.start, 2.0 * test::kPi);
    if (along < 0.0) {
      along += 2.0 * test::kPi;
    }
    if (along <= c.end - c.start + 1e-9) {
      return std::abs(std::hypot(dx, dy) - c.radius);
    }
    const Point2 a = arc_point(c, c.start);
    const Point2 b = arc_point(c, c.end);
    return std::min(std::hypot(p[0] - a[0], p[1] - a[1]), std::hypot(p[0] - b[0], p[1] - b[1]));
  }
  double nearest = 1e300;
  for (std::size_t k = 0; k + 1 < c.points.size(); ++k) {
    nearest = std::min(nearest, segment_distance(p, c.points[k], c.points[k + 1]));
  }
  return nearest;
}

// Points along a drawn curve.
std::vector<Point2> samples(const DrawnCurve& c) {
  std::vector<Point2> out;
  if (c.type == DrawnCurve::Type::Line) {
    for (int k = 0; k <= 16; ++k) {
      const double t = k / 16.0;
      out.push_back({c.points[0][0] + (c.points[1][0] - c.points[0][0]) * t,
                     c.points[0][1] + (c.points[1][1] - c.points[0][1]) * t});
    }
  } else if (c.type == DrawnCurve::Type::Arc) {
    for (int k = 0; k <= 64; ++k) {
      out.push_back(arc_point(c, c.start + (c.end - c.start) * k / 64.0));
    }
  } else {
    for (std::size_t k = 0; k < c.points.size(); ++k) {
      out.push_back(c.points[k]);
      if (k + 1 < c.points.size()) {
        out.push_back({(c.points[k][0] + c.points[k + 1][0]) / 2.0, (c.points[k][1] + c.points[k + 1][1]) / 2.0});
      }
    }
  }
  return out;
}

// The farthest a point of the curves of `from` seen (or hidden) lies from
// the other view's: a seen one from its seen curves, a hidden one from
// any (a hidden line under a seen one does not show).
double deviation(const DrawnView& from, const DrawnView& to, bool visible) {
  double worst = 0.0;
  for (const DrawnCurve& c : from.curves) {
    if (c.visible != visible) {
      continue;
    }
    for (const Point2& p : samples(c)) {
      double nearest = 1e300;
      for (const DrawnCurve& other : to.curves) {
        if (other.visible || !visible) {
          nearest = std::min(nearest, distance_to(other, p));
        }
      }
      worst = std::max(worst, nearest);
    }
  }
  return worst;
}

// A short line, then a quarter of a small circle on from it (mitcad#1671:
// where a hidden line was cut off at a tube's inside corner): drawn within
// the tolerance of them, not as an arc twice that from the line.
void test_fit_a_line_into_an_arc() {
  std::vector<Point2> hook{{-2.0, 4.0}};
  for (const Point2& p : on_circle(0.0, 0.0, 4.0, test::kPi / 2.0, 0.0, 8, false)) {
    hook.push_back(p);
  }
  Segments chords;
  add_chords(chords, hook, false);
  constexpr double kTolerance = 0.1;
  const std::vector<DrawnCurve> drawn = fit_segments(chords, kTolerance);
  CHECK(count_of(drawn, DrawnCurve::Type::Polyline) == 0);
  // The line and the arc themselves.
  DrawnCurve line;
  line.type = DrawnCurve::Type::Line;
  line.points = {{-2.0, 4.0}, {0.0, 4.0}};
  DrawnCurve arc;
  arc.type = DrawnCurve::Type::Arc;
  arc.radius = 4.0;
  arc.start = 0.0;
  arc.end = test::kPi / 2.0;
  double worst = 0.0;
  for (const DrawnCurve& c : drawn) {
    for (const Point2& p : samples(c)) {
      worst = std::max(worst, std::min(distance_to(line, p), distance_to(arc, p)));
    }
  }
  for (const DrawnCurve& truth : {line, arc}) {
    for (const Point2& p : samples(truth)) {
      double nearest = 1e300;
      for (const DrawnCurve& c : drawn) {
        nearest = std::min(nearest, distance_to(c, p));
      }
      worst = std::max(worst, nearest);
    }
  }
  if (!(worst <= kTolerance)) {
    std::fprintf(stderr, "a line into an arc: drawn %.4f mm from them\n", worst);
    ++test::failures;
  }
}

bool has_arc_near(const DrawnView& drawn, double cx, double cy, double radius) {
  for (const DrawnCurve& c : drawn.curves) {
    if (c.type == DrawnCurve::Type::Arc && c.visible && std::abs(c.center[0] - cx) < 1e-4 &&
        std::abs(c.center[1] - cy) < 1e-4 && std::abs(c.radius - radius) < 1e-4) {
      return true;
    }
  }
  return false;
}

// A closed outline has no preferred first chord. Its rounded corners must
// stay whole even when their chords turn more than their tangent joins do.
void test_fit_a_closed_rounded_outline() {
  std::vector<Point2> points;
  const Point2 centers[] = {{4.0, 4.0}, {1996.0, 4.0}, {1996.0, 56.0}, {4.0, 56.0}};
  for (int k = 0; k < 4; ++k) {
    const double start = test::kPi + k * test::kPi / 2.0;
    const auto arc = on_circle(centers[k][0], centers[k][1], 4.0, start, start + test::kPi / 2.0, 3, false);
    points.insert(points.end(), arc.begin(), arc.end());
  }
  for (const bool reverse : {false, true}) {
    if (reverse) {
      std::reverse(points.begin(), points.end());
    }
    for (std::size_t first = 0; first < points.size(); ++first) {
      std::vector<Point2> rotated(points.begin() + static_cast<std::ptrdiff_t>(first), points.end());
      rotated.insert(rotated.end(), points.begin(), points.begin() + static_cast<std::ptrdiff_t>(first));
      Segments chords;
      add_chords(chords, rotated, true);
      DrawnView drawn;
      drawn.curves = fit_segments(chords, 0.4);
      CHECK(count_of(drawn.curves, DrawnCurve::Type::Line) == 4);
      CHECK(count_of(drawn.curves, DrawnCurve::Type::Arc) == 4);
      for (const Point2& center : centers) {
        CHECK(has_arc_near(drawn, center[0], center[1], 4.0));
      }
    }
  }
}

// The quick path: the same shapes with their faces as B-splines (free-form)
// projected from a triangulation, its lines fitted, match the exact view of
// the shapes within the drawing's tolerance; their circles are arcs.
void test_quick_path() {
  const ShapePtr block = counterbored_block();
  const TopoDS_Shape post = cylinder(60.0, 15.0, 0.0, 30.0, 10.0);
  const ShapePtr exact_post = std::make_shared<Shape>(post);
  const ShapePtr free_block = std::make_shared<Shape>(BRepBuilderAPI_NurbsConvert(block->occt(), true).Shape());
  const ShapePtr free_post = std::make_shared<Shape>(BRepBuilderAPI_NurbsConvert(post, true).Shape());
  // Under half a thin line (0.25 mm) at 1:1.
  constexpr double kTolerance = 0.1;
  struct Case {
    gp_Dir toward_eye;
    gp_Dir x_axis;
    bool hidden;
    const char* name;
  };
  const Case cases[] = {
      {gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), false, "top"},
      {gp_Dir(0, -1, 0), gp_Dir(1, 0, 0), true, "front with hidden lines"},
      {gp_Dir(1, -1, 1), gp_Dir(1, 1, 0), false, "isometric"},
  };
  for (const Case& c : cases) {
    const DrawnView exact = project_view({block, exact_post}, view(c.toward_eye, c.x_axis, c.hidden));
    DrawingViewSpec spec = view(c.toward_eye, c.x_axis, c.hidden);
    spec.quick = true;
    const DrawnView quick = project_view({free_block, free_post}, spec);
    CHECK(!exact.polygonal);
    CHECK(quick.polygonal);
    for (const bool visible : {true, false}) {
      if (!visible && !c.hidden) {
        continue;
      }
      const double there = deviation(quick, exact, visible);
      const double back = deviation(exact, quick, visible);
      if (!(there <= kTolerance && back <= kTolerance)) {
        std::fprintf(stderr, "%s, %s lines: the quick view %.4f mm from the exact one, the exact %.4f mm from it\n",
                     c.name, visible ? "seen" : "hidden", there, back);
        ++test::failures;
      }
    }
    if (c.toward_eye.Z() > 0.99) {
      // From above: the hole's circles and the post's as arcs.
      CHECK(has_arc_near(quick, 20.0, 15.0, 5.5));
      CHECK(has_arc_near(quick, 20.0, 15.0, 3.3));
      CHECK(has_arc_near(quick, 60.0, 15.0, 10.0));
    }
  }
  // Without free-form faces the quick path is the exact one.
  DrawingViewSpec spec = view(gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), false);
  spec.quick = true;
  CHECK(!project_view({block, exact_post}, spec).polygonal);
}

// A view's lines all seen, to compare them whether seen or hidden.
DrawnView all_seen(DrawnView drawn) {
  for (DrawnCurve& c : drawn.curves) {
    c.visible = true;
  }
  return drawn;
}

// A drawn curve moved by dx along the view's x.
void shift(DrawnCurve& c, double dx) {
  for (Point2& p : c.points) {
    p[0] += dx;
  }
  for (Point2& p : c.poles) {
    p[0] += dx;
  }
  c.center[0] += dx;
}

// A post of radius 5 standing on (x, y) from z 0 to 30 with a cross hole
// of radius 2 along X at z 15: analytic faces only, the hole's two edges
// on the post B-splines.
TopoDS_Shape cross_drilled_post(double x, double y) {
  const TopoDS_Shape drill =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x - 10.0, y, 15.0), gp_Dir(1.0, 0.0, 0.0)), 2.0, 20.0).Shape();
  return BRepAlgoAPI_Cut(cylinder(x, y, 0.0, 30.0, 5.0), drill).Shape();
}

// The quick path for many edges on B-splines (mitcad#1033): one body of
// analytic faces whose edges where the holes meet the posts are B-splines,
// many of them against many faces, is projected on the quick path, within
// its tolerance of the exact lines; a few such edges keep the exact path.
void test_quick_path_for_curved_edges() {
  BRep_Builder builder;
  TopoDS_Compound posts;
  builder.MakeCompound(posts);
  // 100 posts 20 mm apart (one post moved: the same faces at each place).
  const TopoDS_Shape post = cross_drilled_post(0.0, 0.0);
  constexpr int kSide = 10;
  for (int i = 0; i < kSide; ++i) {
    for (int j = 0; j < kSide; ++j) {
      gp_Trsf move;
      move.SetTranslation(gp_Vec(20.0 * i, 20.0 * j, 0.0));
      builder.Add(posts, post.Moved(TopLoc_Location(move)));
    }
  }
  const ShapePtr many = std::make_shared<Shape>(posts);
  const ShapePtr few = std::make_shared<Shape>(post);
  DrawingViewSpec top = view(gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), false);
  top.quick = true;
  CHECK(!project_view({few}, top).polygonal);
  const DrawnView quick_top = project_view({many}, top);
  CHECK(quick_top.polygonal);
  CHECK(has_arc_near(quick_top, 0.0, 0.0, 5.0));
  CHECK(has_arc_near(quick_top, 180.0, 180.0, 5.0));
  // From the front with hidden lines, where the exact algorithm takes
  // minutes on them (each column's posts behind each other: 297 s in a
  // debug build, the quick path 0.2 s): the quick view's lines, seen and
  // hidden, within the quick path's tolerance of one post's exact lines
  // repeated along the row, both ways. That tolerance: 1.5 times a 2000th
  // of the 270 mm the posts span, with a margin; a twentieth of a
  // millimetre at the 1:5 such a view has on a sheet.
  DrawingViewSpec front = view(gp_Dir(0, -1, 0), gp_Dir(1, 0, 0), true);
  front.quick = true;
  const DrawnView quick = all_seen(project_view({many}, front));
  CHECK(quick.curves.size() > 0);
  const DrawnView one = project_view({few}, front);
  CHECK(!one.polygonal);
  DrawnView row;
  for (int i = 0; i < kSide; ++i) {
    for (DrawnCurve c : one.curves) {
      shift(c, 20.0 * i);
      c.visible = true;
      row.curves.push_back(std::move(c));
    }
  }
  constexpr double kTolerance = 0.25;
  const double there = deviation(quick, row, true);
  const double back = deviation(row, quick, true);
  if (!(there <= kTolerance && back <= kTolerance)) {
    std::fprintf(stderr, "posts: the quick view %.4f mm from the exact lines, they %.4f mm from it\n", there, back);
    ++test::failures;
  }
}

// --- The quick path at the view's scale, body by body (mitcad#1671) -----------

// A bar of the box from `corner` with `size`, its four edges along `axis`
// (0 X, 1 Y, 2 Z) rounded to `radius`.
TopoDS_Shape rounded_bar(const gp_Pnt& corner, const gp_Vec& size, int axis, double radius) {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(corner, size.X(), size.Y(), size.Z()).Shape();
  BRepFilletAPI_MakeFillet fillet(box);
  const gp_Dir along(axis == 0 ? 1.0 : 0.0, axis == 1 ? 1.0 : 0.0, axis == 2 ? 1.0 : 0.0);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(box, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    if (curve.GetType() == GeomAbs_Line && curve.Line().Direction().IsParallel(along, 1e-9)) {
      fillet.Add(radius, TopoDS::Edge(edges(i)));
    }
  }
  return fillet.Shape();
}

// A rounded rectangular tube along `axis` filling the box from `corner`
// with `size`: walls 4 mm thick, outside corners of radius 8, inside 4.
ShapePtr rounded_tube(const gp_Pnt& corner, const gp_Vec& size, int axis) {
  constexpr double kWall = 4.0;
  gp_Vec inset(kWall, kWall, kWall);
  gp_Vec inner = size - 2.0 * inset;
  inset.SetCoord(axis + 1, -1.0);
  inner.SetCoord(axis + 1, size.Coord(axis + 1) + 2.0);
  const TopoDS_Shape outside = rounded_bar(corner, size, axis, 8.0);
  const TopoDS_Shape inside = rounded_bar(corner.Translated(inset), inner, axis, 4.0);
  return std::make_shared<Shape>(BRepAlgoAPI_Cut(outside, inside).Shape());
}

// A 6 m frame of 80 x 80 tubes, 802 wide and 500 high: four tubes along X
// at its corners and at each end two posts and two cross tubes between
// them.
std::vector<ShapePtr> tube_frame() {
  std::vector<ShapePtr> tubes;
  for (const double y : {0.0, 722.0}) {
    for (const double z : {0.0, 420.0}) {
      tubes.push_back(rounded_tube(gp_Pnt(0.0, y, z), gp_Vec(6000.0, 80.0, 80.0), 0));
    }
  }
  for (const double x : {0.0, 5920.0}) {
    for (const double y : {0.0, 722.0}) {
      tubes.push_back(rounded_tube(gp_Pnt(x, y, 80.0), gp_Vec(80.0, 80.0, 340.0), 2));
    }
    for (const double z : {0.0, 420.0}) {
      tubes.push_back(rounded_tube(gp_Pnt(x, 80.0, z), gp_Vec(80.0, 642.0, 80.0), 1));
    }
  }
  return tubes;
}

// A wheel along Y with its faces as B-splines (free-form).
ShapePtr bspline_wheel(const gp_Pnt& center, double radius, double width) {
  const TopoDS_Shape wheel =
      BRepPrimAPI_MakeCylinder(gp_Ax2(center.Translated(gp_Vec(0.0, -width / 2.0, 0.0)), gp_Dir(0.0, 1.0, 0.0)),
                               radius, width)
          .Shape();
  return std::make_shared<Shape>(BRepBuilderAPI_NurbsConvert(wheel, true).Shape());
}

// The curves of the view whose source is from `first` to below `end`.
DrawnView sources_in(const DrawnView& drawn, int first, int end) {
  DrawnView out;
  for (const DrawnCurve& c : drawn.curves) {
    if (c.source >= first && c.source < end) {
      out.curves.push_back(c);
    }
  }
  return out;
}

// The end view of a 6 m frame of rounded tubes with one small free-form
// body, at 1:5 (mitcad#1671): its quick path deviates within 0.02 mm on
// the sheet, 0.1 mm in the model, not a 2000th of the frame's 6 m (3 mm);
// the tubes' corners come out as arcs and their sides as lines, hidden
// lines as the exact algorithm draws them. A wheel apart from the tubes in
// the view leaves them to the exact algorithm; one in front of a corner
// takes the tubes it overlaps to the quick path with it. Without the wheel
// the view is the exact one, quick path or not.
void test_quick_path_at_the_view_scale() {
  const std::vector<ShapePtr> frame = tube_frame();
  const int tubes = static_cast<int>(frame.size());
  // Along the frame's length from its end at x 6000: x along Y, up along Z.
  DrawingViewSpec spec = view(gp_Dir(1, 0, 0), gp_Dir(0, 1, 0), true);
  spec.quick = true;
  spec.quick_deviation = 0.1; // 0.02 mm on the sheet at 1:5
  DrawingViewSpec exact_spec = spec;
  exact_spec.quick = false;

  const DrawnView alone = project_view(frame, spec);
  const DrawnView alone_exact = project_view(frame, exact_spec);
  CHECK(!alone.polygonal);
  CHECK(alone.curves.size() == alone_exact.curves.size());
  CHECK(deviation(alone, alone_exact, true) < 1e-9 && deviation(alone_exact, alone, true) < 1e-9);
  CHECK(deviation(alone, alone_exact, false) < 1e-9 && deviation(alone_exact, alone, false) < 1e-9);

  struct Case {
    ShapePtr wheel;
    const char* name;
    // The tube in front of whose corner it is: then on the quick path.
    bool in_front;
  };
  const Case cases[] = {
      {bspline_wheel(gp_Pnt(5950.0, 760.0, -120.0), 50.0, 40.0), "a wheel apart", false},
      {bspline_wheel(gp_Pnt(6100.0, 50.0, 60.0), 30.0, 40.0), "a wheel in front of a corner", true},
  };
  // The tube at the corner (722, 420), not overlapped by either wheel.
  constexpr int kFarTube = 3;
  constexpr double kWanted = 0.1;
  for (const Case& c : cases) {
    std::vector<ShapePtr> shapes = frame;
    shapes.push_back(c.wheel);
    const DrawnView quick = project_view(shapes, spec);
    const DrawnView exact = project_view(shapes, exact_spec);
    CHECK(quick.polygonal);
    CHECK(!exact.polygonal);
    for (const bool visible : {true, false}) {
      // All lines, the wheel's too, within the wanted deviation of the
      // exact ones, both ways.
      const double there = deviation(quick, exact, visible);
      const double back = deviation(exact, quick, visible);
      std::fprintf(stderr, "%s, %s lines: the quick view %.4f mm from the exact one, the exact %.4f mm from it\n",
                   c.name, visible ? "seen" : "hidden", there, back);
      CHECK(there <= kWanted && back <= kWanted);
      // The far tube's exact lines in either case.
      const DrawnView quick_far = sources_in(quick, kFarTube, kFarTube + 1);
      const DrawnView exact_far = sources_in(exact, kFarTube, kFarTube + 1);
      CHECK(quick_far.curves.size() == exact_far.curves.size());
      CHECK(deviation(quick_far, exact_far, visible) < 1e-9 && deviation(exact_far, quick_far, visible) < 1e-9);
    }
    // The tubes' ends: their corners as arcs, outside and inside (one
    // centre); the corner tube's in front of which the wheel is too.
    CHECK(has_arc_near(quick, 730.0, 428.0, 8.0));
    CHECK(has_arc_near(quick, 730.0, 428.0, 4.0));
    CHECK(has_arc_near(quick, 794.0, 492.0, 8.0));
    CHECK(has_arc_near(quick, 8.0, 8.0, 8.0));
    CHECK(has_arc_near(quick, 8.0, 8.0, 4.0));
    // Apart, all tubes keep the exact lines.
    if (!c.in_front) {
      CHECK(sources_in(quick, 0, tubes).curves.size() == alone_exact.curves.size());
    }
  }
}

// --- Tangent edges (mitcad#1438) ----------------------------------------------

// The largest angle between the faces at an edge at which they meet
// tangentially, as the drawings take it (radians).
constexpr double kTangent = 1e-3;

// The two faces' outward normals at parameter t of the edge between them.
bool normals_at(const TopoDS_Edge& edge, const TopoDS_Face& face1, const TopoDS_Face& face2, double t,
                gp_Vec& n1, gp_Vec& n2) {
  for (int k = 0; k < 2; ++k) {
    const TopoDS_Face& face = k == 0 ? face1 : face2;
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull()) {
      return false;
    }
    const gp_Pnt2d uv = pcurve->Value(t);
    const BRepAdaptor_Surface surface(face);
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(uv.X(), uv.Y(), p, du, dv);
    gp_Vec n = du.Crossed(dv);
    if (n.Magnitude() < 1e-12) {
      return false;
    }
    n.Normalize();
    if (face.Orientation() == TopAbs_REVERSED) {
      n.Reverse();
    }
    (k == 0 ? n1 : n2) = n;
  }
  return true;
}

// An edge of a shape for the checks: points along it, and whether it is
// no edge a drawing shows as sharp: a seam (a face's edge with itself), or
// an edge whose two faces meet tangentially all along it (their normals
// within kTangent of each other at points along it).
const char* continuity_name(GeomAbs_Shape c) {
  switch (c) {
  case GeomAbs_C0:
    return "C0";
  case GeomAbs_G1:
    return "G1";
  case GeomAbs_C1:
    return "C1";
  case GeomAbs_G2:
    return "G2";
  case GeomAbs_C2:
    return "C2";
  case GeomAbs_C3:
    return "C3";
  default:
    return "CN";
  }
}

const char* surface_name(const TopoDS_Face& face) {
  switch (BRepAdaptor_Surface(face, false).GetType()) {
  case GeomAbs_Plane:
    return "plane";
  case GeomAbs_Cylinder:
    return "cylinder";
  case GeomAbs_Cone:
    return "cone";
  case GeomAbs_Sphere:
    return "sphere";
  case GeomAbs_Torus:
    return "torus";
  case GeomAbs_BSplineSurface:
    return "bspline";
  default:
    return "other";
  }
}

struct EdgeLine {
  std::vector<gp_Pnt> points;
  bool smooth = false;
  bool seam = false;
  // Its index among the shape's edges (test_drawing --edges) and its faces'
  // surfaces with the continuity stored on it, for the checks' messages.
  int index = 0;
  std::string what;
  // Between two faces of one surface, or where the curvature runs on from
  // one face to the other (G2 and up, as OCCT finds it): drawn never.
  bool within = false;
  // At the smooth edge's points, the face's normal (at right angles to a
  // view all along, the edge lies on that view's outline).
  std::vector<gp_Vec> normals;
  // A smooth edge's face normal at each of its points (zero where the face
  // has none).
  std::vector<gp_Vec> point_normals;
};

// How far apart two faces' normals at a point of the edge between them may
// be and still be the one normal they share there (as the drawings take it).
constexpr double kNormalsAgree = 0.05;

// The normal two faces share at each point of the edge between them, from
// each face's (zero where it has none): where they agree, or one face's
// where the other has none; where they disagree (next to the point where a
// fillet runs out to a point, whose normal there may point anywhere), the
// one nearer the normal they share at the nearest point toward the edge's
// middle where they agree; zero where none is found (mitcad#2510).
std::vector<gp_Vec> shared_normals(const std::vector<gp_Vec>& a, const std::vector<gp_Vec>& b) {
  const auto has = [](const gp_Vec& n) { return n.Magnitude() > 0.5; };
  const auto agree = [&](std::size_t k) { return has(a[k]) && has(b[k]) && a[k].Angle(b[k]) <= kNormalsAgree; };
  std::vector<gp_Vec> out(a.size(), gp_Vec(0.0, 0.0, 0.0));
  const std::size_t middle = a.size() / 2;
  for (std::size_t k = 0; k < a.size(); ++k) {
    if (agree(k) || !has(b[k])) {
      out[k] = a[k];
      continue;
    }
    if (!has(a[k])) {
      out[k] = b[k];
      continue;
    }
    for (std::size_t j = k; j != middle;) {
      j = j < middle ? j + 1 : j - 1;
      if (agree(j)) {
        out[k] = a[k].Angle(a[j]) <= b[k].Angle(a[j]) ? a[k] : b[k];
        break;
      }
    }
  }
  return out;
}

std::vector<EdgeLine> edge_lines(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  std::vector<EdgeLine> out;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    TopoDS_Face face1;
    TopoDS_Face face2;
    for (NCollection_List<TopoDS_Shape>::Iterator it(faces.FindFromIndex(i)); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Value());
      if (face1.IsNull()) {
        face1 = face;
      } else if (!face.IsSame(face1) && face2.IsNull()) {
        face2 = face;
      }
    }
    const bool seam = !face1.IsNull() && face2.IsNull() && faces.FindFromIndex(i).Extent() > 1;
    EdgeLine line;
    line.seam = seam;
    line.index = i;
    if (!face1.IsNull()) {
      const TopoDS_Face& other = face2.IsNull() ? face1 : face2;
      line.what = std::string(surface_name(face1)) + "/" + surface_name(other) + " stored " +
                  continuity_name(BRep_Tool::Continuity(edge, face1, other));
    }
    const BRepAdaptor_Curve curve(edge);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    constexpr int kPoints = 256;
    for (int k = 0; k <= kPoints; ++k) {
      line.points.push_back(curve.Value(first + (last - first) * k / kPoints));
    }
    if (seam || !face2.IsNull()) {
      line.smooth = true;
      for (int k = 1; k < 16 && line.smooth; ++k) {
        gp_Vec n1;
        gp_Vec n2;
        line.smooth = normals_at(edge, face1, seam ? face1 : face2, first + (last - first) * k / 16.0, n1, n2) &&
                      (seam || (n1.Dot(n2) > 0.0 && n1.Crossed(n2).Magnitude() <= kTangent));
        line.normals.push_back(n1);
      }
    }
    if (!line.smooth) {
      line.normals.clear();
    }
    if (line.smooth) {
      std::vector<gp_Vec> of_first;
      std::vector<gp_Vec> of_second;
      for (int k = 0; k <= kPoints; ++k) {
        const double t = first + (last - first) * k / kPoints;
        gp_Vec n1;
        gp_Vec n2;
        const bool found = normals_at(edge, face1, face1, t, n1, n2);
        of_first.push_back(found ? n1 : gp_Vec(0.0, 0.0, 0.0));
        const bool found2 = !face2.IsNull() && normals_at(edge, face2, face2, t, n1, n2);
        of_second.push_back(found2 ? n1 : gp_Vec(0.0, 0.0, 0.0));
      }
      line.point_normals = shared_normals(of_first, of_second);
    }
    if (line.smooth && !seam) {
      try {
        line.within = BRepLib::ContinuityOfFaces(edge, face1, face2, kTangent) >= GeomAbs_G2;
      } catch (const Standard_Failure&) {
        line.within = false;
      }
    }
    out.push_back(std::move(line));
  }
  return out;
}

Point2 projected(const gp_Pnt& p, const DrawingViewSpec& spec) {
  const gp_XYZ x = spec.x_axis.XYZ();
  const gp_XYZ y = spec.toward_eye.XYZ().Crossed(x);
  return {p.XYZ().Dot(x), p.XYZ().Dot(y)};
}

// Whether the points all lie within `tolerance` of the edge's projection.
bool along(const std::vector<Point2>& points, const EdgeLine& edge, const DrawingViewSpec& spec,
           double tolerance) {
  std::vector<Point2> line;
  for (const gp_Pnt& p : edge.points) {
    line.push_back(projected(p, spec));
  }
  for (const Point2& p : points) {
    double nearest = 1e300;
    for (std::size_t k = 0; k + 1 < line.size(); ++k) {
      nearest = std::min(nearest, segment_distance(p, line[k], line[k + 1]));
    }
    if (nearest > tolerance) {
      return false;
    }
  }
  return true;
}

// Whether the smooth edge lies on the view's outline: its faces' normal
// all along it at right angles to the view.
bool on_outline(const EdgeLine& edge, const DrawingViewSpec& spec) {
  for (const gp_Vec& n : edge.normals) {
    if (std::abs(n.Dot(gp_Vec(spec.toward_eye))) > 0.02) {
      return false;
    }
  }
  return true;
}

double extent_of(const std::vector<Point2>& points) {
  double xmin = 1e300, xmax = -1e300, ymin = 1e300, ymax = -1e300;
  for (const Point2& p : points) {
    xmin = std::min(xmin, p[0]);
    xmax = std::max(xmax, p[0]);
    ymin = std::min(ymin, p[1]);
    ymax = std::max(ymax, p[1]);
  }
  return std::max(xmax - xmin, ymax - ymin);
}

// The lines among `curves` that are smooth edges: longer than a tenth of a
// millimetre, along a smooth edge off the view's outline and along no
// other edge (a sharp edge, or one on the outline, which the view draws
// anyway). `seams`: those along seams only.
struct SmoothLines {
  int count = 0;
  int seams = 0;
  std::vector<std::vector<Point2>> lines;
};

SmoothLines smooth_lines(const std::vector<std::vector<Point2>>& curves, const std::vector<EdgeLine>& edges,
                         const DrawingViewSpec& spec, double tolerance) {
  SmoothLines out;
  for (const std::vector<Point2>& points : curves) {
    if (extent_of(points) < 0.1) {
      continue;
    }
    bool tangent = false;
    bool seam = false;
    bool other = false;
    for (const EdgeLine& edge : edges) {
      if (other) {
        break;
      }
      if (!along(points, edge, spec, tolerance)) {
        continue;
      }
      if (!edge.smooth || on_outline(edge, spec)) {
        other = true;
      } else {
        (edge.seam ? seam : tangent) = true;
      }
    }
    if ((tangent || seam) && !other) {
      ++out.count;
      out.seams += tangent ? 0 : 1;
      out.lines.push_back(points);
    }
  }
  return out;
}

std::vector<std::vector<Point2>> seen_lines(const DrawnView& drawn, DrawnKind kind) {
  std::vector<std::vector<Point2>> out;
  for (const DrawnCurve& c : drawn.curves) {
    if (c.visible && c.kind == kind) {
      out.push_back(samples(c));
    }
  }
  return out;
}

// The sharp edges OCCT's hidden-line removal draws seen, of the shape as it
// is: the edges' continuity as their operations left it.
std::vector<std::vector<Point2>> raw_sharp_lines(const TopoDS_Shape& shape, const DrawingViewSpec& spec) {
  occ::handle<HLRBRep_Algo> algo = new HLRBRep_Algo();
  algo->Add(shape);
  algo->Projector(HLRAlgo_Projector(gp_Ax2(gp::Origin(), spec.toward_eye, spec.x_axis)));
  algo->Update();
  algo->Hide();
  HLRBRep_HLRToShape out(algo);
  std::vector<std::vector<Point2>> lines;
  const TopoDS_Shape sharp = out.VCompound();
  if (sharp.IsNull()) {
    return lines;
  }
  for (TopExp_Explorer it(sharp, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    std::vector<Point2> points;
    for (int k = 0; k <= 32; ++k) {
      const gp_Pnt p = curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 32);
      points.push_back({p.X(), p.Y()});
    }
    lines.push_back(std::move(points));
  }
  return lines;
}

// The continuity stored on each edge between its two faces.
std::vector<GeomAbs_Shape> continuities(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  std::vector<GeomAbs_Shape> out;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& around = faces.FindFromIndex(i);
    if (around.Extent() == 2) {
      out.push_back(BRep_Tool::Continuity(TopoDS::Edge(faces.FindKey(i)), TopoDS::Face(around.First()),
                                          TopoDS::Face(around.Last())));
    }
  }
  return out;
}

// The continuity on every edge between two faces set to C0, as booleans
// and imports leave it (OCCT's fillet records it on the edges it makes; a
// later operation's edges have none).
void clear_continuities(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  BRep_Builder builder;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& around = faces.FindFromIndex(i);
    if (around.Extent() == 2) {
      builder.Continuity(TopoDS::Edge(faces.FindKey(i)), TopoDS::Face(around.First()), TopoDS::Face(around.Last()),
                         GeomAbs_C0);
    }
  }
}

// Seen lines along smooth edges (tangent edges and seams) in the views
// check_tangent_edges draws.
struct TangentViewCounts {
  int raw = 0;           // sharp lines of the shape as it is (OCCT's hidden-line removal alone)
  int raw_seams = 0;     // of which along seams
  int off = 0;           // with Tangent Edges off, exact
  int off_polygonal = 0; // and polygonal
  int on_sharp = 0;      // sharp lines with Tangent Edges on
  int on = 0;            // thin lines with Tangent Edges on
  int sharp_lost = 0;    // other sharp lines of the shape as it is no longer drawn
};

// Whether a line of a polygonal projection along a smooth edge is that
// algorithm's outline, not the edge drawn as a sharp one (mitcad#2447). The
// polygonal outline runs along the triangles' edges, so it may run along a
// chord or two of a tangent edge where the view's outline crosses the edge:
// at every point of the line, the faces' normal along the edge it lies along
// is within the triangulation's angle of square to the view
// (spec.quick_chord_angle). Nowhere else: next to the point where a fillet
// runs out to a point, the chords that algorithm flagged as outline (its
// sliver triangles there facing whichever way their other corners make
// them) are no longer drawn (mitcad#2510).
bool on_polygonal_outline(const std::vector<Point2>& line, const std::vector<EdgeLine>& edges,
                          const DrawingViewSpec& spec, double tolerance) {
  const double band = std::sin(spec.quick_chord_angle);
  const gp_Vec eye(spec.toward_eye);
  for (const Point2& p : line) {
    bool in_band = false;
    for (const EdgeLine& edge : edges) {
      if (!edge.smooth || edge.point_normals.size() != edge.points.size()) {
        continue;
      }
      for (std::size_t k = 0; k + 1 < edge.points.size() && !in_band; ++k) {
        if (segment_distance(p, projected(edge.points[k], spec), projected(edge.points[k + 1], spec)) > tolerance) {
          continue;
        }
        const gp_Vec& n = edge.point_normals[k];
        const gp_Vec& m = edge.point_normals[k + 1];
        in_band = (n.Magnitude() > 0.5 && std::abs(n.Dot(eye)) <= band) ||
                  (m.Magnitude() > 0.5 && std::abs(m.Dot(eye)) <= band);
      }
      if (in_band) {
        break;
      }
    }
    if (!in_band) {
      return false;
    }
  }
  return true;
}

// The six orthographic views and the isometric one (toward the eye, the x
// axis) the tangent edge checks project.
std::vector<std::pair<gp_Dir, gp_Dir>> view_directions() {
  const double r2 = 1.0 / std::sqrt(2.0);
  const double r3 = 1.0 / std::sqrt(3.0);
  return {{gp_Dir(0, -1, 0), gp_Dir(1, 0, 0)},  {gp_Dir(0, 1, 0), gp_Dir(-1, 0, 0)},
          {gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)},   {gp_Dir(0, 0, -1), gp_Dir(1, 0, 0)},
          {gp_Dir(1, 0, 0), gp_Dir(0, 1, 0)},   {gp_Dir(-1, 0, 0), gp_Dir(0, -1, 0)},
          {gp_Dir(r3, -r3, r3), gp_Dir(r2, r2, 0)}};
}

// Every orthographic view and an isometric one of the shape, checked for
// smooth edges drawn: with Tangent Edges off none (exact and polygonal),
// with it on thin ones and no sharp ones, and every other sharp line of the
// shape as it is still drawn; the shape is not changed.
TangentViewCounts check_tangent_edges(const ShapePtr& shape, const std::string& name) {
  const std::vector<GeomAbs_Shape> before = continuities(shape->occt());
  const std::vector<EdgeLine> edges = edge_lines(shape->occt());
  int tangent = 0;
  int seams = 0;
  for (const EdgeLine& e : edges) {
    tangent += e.smooth && !e.seam ? 1 : 0;
    seams += e.seam ? 1 : 0;
  }
  TangentViewCounts counts;
  for (const auto& [eye, x] : view_directions()) {
    const DrawingViewSpec spec = view(eye, x, false);
    char from[64];
    std::snprintf(from, sizeof from, "from (%.3f, %.3f, %.3f)", eye.X(), eye.Y(), eye.Z());
    const std::vector<std::vector<Point2>> raw_lines = raw_sharp_lines(shape->occt(), spec);
    const SmoothLines raw = smooth_lines(raw_lines, edges, spec, 0.01);
    counts.raw += raw.count;
    counts.raw_seams += raw.seams;

    const DrawnView off = project_view({shape}, spec);
    const SmoothLines off_lines = smooth_lines(seen_lines(off, DrawnKind::Sharp), edges, spec, 0.01);
    counts.off += off_lines.count;
    for (const std::vector<Point2>& line : off_lines.lines) {
      std::fprintf(stderr, "%s: a tangent edge drawn %s: (%.3f, %.3f) to (%.3f, %.3f)\n", name.c_str(), from,
                   line.front()[0], line.front()[1], line.back()[0], line.back()[1]);
    }
    // Every other sharp line of the shape as it is still drawn.
    for (const std::vector<Point2>& line : raw_lines) {
      if (extent_of(line) < 0.1 || smooth_lines({line}, edges, spec, 0.01).count > 0) {
        continue;
      }
      for (const Point2& p : line) {
        double nearest = 1e300;
        for (const DrawnCurve& c : off.curves) {
          if (c.visible) {
            nearest = std::min(nearest, distance_to(c, p));
          }
        }
        if (nearest > 0.01) {
          ++counts.sharp_lost;
          std::fprintf(stderr, "%s: a sharp line %s not drawn at (%.3f, %.3f)\n", name.c_str(), from, p[0], p[1]);
          break;
        }
      }
    }

    DrawingViewSpec polygonal = spec;
    polygonal.exact = false;
    const DrawnView quick = project_view({shape}, polygonal);
    const SmoothLines quick_lines = smooth_lines(seen_lines(quick, DrawnKind::Sharp), edges, spec, 0.1);
    for (const std::vector<Point2>& line : quick_lines.lines) {
      // That algorithm's outline along a chord of the edge (mitcad#2447).
      if (on_polygonal_outline(line, edges, polygonal, 0.1)) {
        continue;
      }
      ++counts.off_polygonal;
      std::fprintf(stderr, "%s: a tangent edge drawn polygonal %s: (%.3f, %.3f) to (%.3f, %.3f)\n", name.c_str(), from,
                   line.front()[0], line.front()[1], line.back()[0], line.back()[1]);
    }

    DrawingViewSpec with_tangents = spec;
    with_tangents.smooth = true;
    const DrawnView on = project_view({shape}, with_tangents);
    counts.on_sharp += smooth_lines(seen_lines(on, DrawnKind::Sharp), edges, spec, 0.01).count;
    counts.on += smooth_lines(seen_lines(on, DrawnKind::Smooth), edges, spec, 0.01).count;
  }
  std::printf("%s: %d tangent edges and %d seams of %zu edges; in 7 views, lines along them drawn as sharp from the "
              "shape as it is %d (%d along seams), with Tangent Edges off %d (polygonal %d), on %d thin and %d "
              "sharp; other sharp lines lost %d\n",
              name.c_str(), tangent, seams, edges.size(), counts.raw, counts.raw_seams, counts.off,
              counts.off_polygonal, counts.on, counts.on_sharp, counts.sharp_lost);
  CHECK(counts.off == 0);
  CHECK(counts.off_polygonal == 0);
  CHECK(counts.on_sharp == 0);
  CHECK(counts.on > 0);
  CHECK(counts.sharp_lost == 0);
  // The shape is as it was: the continuity on its edges unchanged.
  CHECK(continuities(shape->occt()) == before);
  return counts;
}

// How far a polygonal view's line may lie from a point of the exact view's
// and still be taken for it in check_polygonal_outline: a quarter of a
// millimetre, the triangulation's deviation and the fitting with room to
// spare, under half the chord of a round edge beside a crossing of the
// outline that a cleared outline flag would leave out.
constexpr double kOutlineGap = 0.25;

// The distance from p to the nearest of the view's lines.
double nearest_line(const DrawnView& drawn, const Point2& p) {
  double nearest = 1e300;
  for (const DrawnCurve& c : drawn.curves) {
    nearest = std::min(nearest, distance_to(c, p));
  }
  return nearest;
}

// Nothing of the outline left out by the polygonal view's outline rule
// (mitcad#2510), in every orthographic view and an isometric one, Tangent
// Edges off: the polygonal view keeps its algorithm's outline flags on
// chords of a smooth edge only where the faces come near square to the view.
// Where that algorithm by itself (the rule turned off) draws a line within
// kOutlineGap of a point of a line the exact algorithm draws seen, the view
// draws one too (seen or hidden in both, so that the two algorithms' hiding
// near the outline does not count). Points of the exact lines that the
// polygonal algorithm misses by itself (its triangulated outline off the
// exact one) are counted and printed, not checked.
void check_polygonal_outline(const ShapePtr& shape, const std::string& name) {
  int dropped = 0;
  int missed = 0;
  for (const auto& [eye, x] : view_directions()) {
    const DrawnView exact = project_view({shape}, view(eye, x, false));
    DrawingViewSpec spec = view(eye, x, true);
    spec.exact = false;
    const DrawnView polygonal = project_view({shape}, spec);
    mitcad::geometry::detail::set_polygonal_true_outlines(false);
    const DrawnView by_itself = project_view({shape}, spec);
    mitcad::geometry::detail::set_polygonal_true_outlines(true);
    CHECK(!exact.polygonal);
    CHECK(polygonal.polygonal);
    CHECK(by_itself.polygonal);
    for (const DrawnCurve& c : exact.curves) {
      if (!c.visible) {
        continue;
      }
      bool line_missed = false;
      for (const Point2& p : samples(c)) {
        const double off = nearest_line(polygonal, p);
        if (off <= kOutlineGap) {
          continue;
        }
        if (nearest_line(by_itself, p) > kOutlineGap) {
          line_missed = true;
          continue;
        }
        ++dropped;
        std::fprintf(stderr, "%s: from (%.3f, %.3f, %.3f) the polygonal view leaves out the line near (%.3f, %.3f) "
                             "of %s its algorithm draws, %.3f off\n",
                     name.c_str(), eye.X(), eye.Y(), eye.Z(), p[0], p[1],
                     c.kind == DrawnKind::Outline ? "the outline" : "a sharp edge", off);
        break;
      }
      missed += line_missed ? 1 : 0;
    }
  }
  std::printf("%s: in 7 views, %d exact lines with a point left out by the polygonal outline rule, %d with a point "
              "the polygonal algorithm misses by itself\n",
              name.c_str(), dropped, missed);
  CHECK(dropped == 0);
}

// A block 40 x 30 x 20 with its upright edges rounded 5 and its top edges
// 2, the edges' continuity left out as booleans and imports leave it
// (OCCT's fillet records it on the edges it makes; a later operation's
// edges have none): drawn without tangent edges, they were drawn as sharp
// edges.
void test_tangent_edges_of_a_filleted_block() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), 40.0, 30.0, 20.0).Shape();
  BRepFilletAPI_MakeFillet upright(box);
  for (TopExp_Explorer it(box, TopAbs_EDGE); it.More(); it.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(it.Current()));
    if (curve.GetType() == GeomAbs_Line && std::abs(curve.Line().Direction().Z()) > 0.5) {
      upright.Add(5.0, TopoDS::Edge(it.Current()));
    }
  }
  const TopoDS_Shape rounded = upright.Shape();
  BRepFilletAPI_MakeFillet top(rounded);
  for (TopExp_Explorer it(rounded, TopAbs_FACE); it.More(); it.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()));
    if (surface.GetType() == GeomAbs_Plane && surface.Plane().Location().Z() > 19.9 &&
        std::abs(surface.Plane().Axis().Direction().Z()) > 0.5) {
      for (TopExp_Explorer e(it.Current(), TopAbs_EDGE); e.More(); e.Next()) {
        top.Add(2.0, TopoDS::Edge(e.Current()));
      }
    }
  }
  TopoDS_Shape block = top.Shape();
  // The continuity as a later operation leaves it: none.
  clear_continuities(block);
  const TangentViewCounts counts = check_tangent_edges(std::make_shared<Shape>(block), "filleted block");
  // The outline crosses the tangent edges at the corners' blends.
  check_polygonal_outline(std::make_shared<Shape>(block), "filleted block");
  // The test's premise: as it is, the shape's tangent edges are drawn as
  // sharp ones.
  CHECK(counts.raw > counts.raw_seams);
}

// --- Hidden edges and seams of smooth shapes (mitcad#2447) ---------------------

// Every line of the view along edges, seen and hidden: sharp and smooth,
// not the faces' outlines, which are no edges.
std::vector<std::vector<Point2>> edge_lines_of(const DrawnView& drawn) {
  std::vector<std::vector<Point2>> out;
  for (const DrawnCurve& c : drawn.curves) {
    if (c.kind != DrawnKind::Outline) {
      out.push_back(samples(c));
    }
  }
  return out;
}

// The seams whose stored continuity makes them smooth edges (G1 or C1),
// as a sweep leaves the seams of a closed section's faces.
int seams_stored_smooth(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  int count = 0;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& around = faces.FindFromIndex(i);
    if (around.Extent() != 1) {
      continue;
    }
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(i));
    const TopoDS_Face& face = TopoDS::Face(around.First());
    bool forward = false;
    bool reversed = false;
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      if (it.Current().IsSame(edge)) {
        forward = forward || it.Current().Orientation() == TopAbs_FORWARD;
        reversed = reversed || it.Current().Orientation() == TopAbs_REVERSED;
      }
    }
    const GeomAbs_Shape stored = BRep_Tool::Continuity(edge, face, face);
    count += forward && reversed && stored >= GeomAbs_G1 && stored < GeomAbs_G2 ? 1 : 0;
  }
  return count;
}

struct HiddenViewCounts {
  int tangent_off = 0; // lines along tangent edges or seams with Tangent Edges off, seen or hidden
  int seams = 0;       // lines along seams with Tangent Edges on, seen or hidden
  int partial = 0;     // edges not drawn over their whole length with both on
  int checked = 0;     // edges checked for it in all views
  int outline_chords = 0; // polygonal outline lines along a tangent edge where it crosses the outline
};

// The shape's edges with Hidden Edges on in every orthographic view and an
// isometric one, exact, on the quick path as a view at 1:1 takes it, and
// polygonal at a reduced view's coarsest triangulation (mitcad#2014): with
// Tangent Edges off no line along a tangent edge or a seam, seen or
// hidden; with it on no line
// along a seam, and every other edge drawn over its whole length, seen or
// hidden (tangent edges and their hidden parts too), but those within one
// surface, which are never drawn. The faces' outlines are no edges and
// are not counted along one: a cylinder's outline may run where a tangent
// edge seen edge on lies.
HiddenViewCounts check_hidden_edges(const ShapePtr& shape, const std::string& name) {
  const std::vector<EdgeLine> edges = edge_lines(shape->occt());
  // The polygonal lines lie within one and a half times the
  // triangulation's deviation, a 2000th of the shape's size.
  const BoundingBox box = bounding_box(*shape);
  const double polygonal_tolerance = 0.01 + 2.0 * 5e-4 * box.min.Distance(box.max);
  HiddenViewCounts counts;
  // The exact algorithm; the quick path as a view at 1:1 on its sheet
  // takes it (ViewSpec::quick_deviation and quick_chord_angle: the
  // polygonal algorithm on shapes with free-form faces, as the reported
  // views of mitcad#2447 were drawn); the polygonal algorithm at a reduced
  // view's coarsest triangulation (mitcad#2014).
  enum class Mode { Exact, Quick, Polygonal };
  for (const auto& [eye, x] : view_directions()) {
    for (const Mode mode : {Mode::Exact, Mode::Quick, Mode::Polygonal}) {
      DrawingViewSpec spec = view(eye, x, true);
      double tolerance = 0.01;
      const char* how = "";
      if (mode == Mode::Quick) {
        spec.quick = true;
        spec.quick_deviation = 0.02;
        spec.quick_chord_angle = 0.2;
        tolerance = 0.05;
        how = " quick";
      } else if (mode == Mode::Polygonal) {
        spec.exact = false;
        spec.quick_chord_angle = 3.14159265358979 / 6.0;
        tolerance = polygonal_tolerance;
        how = " polygonal";
      }
      char from[96];
      std::snprintf(from, sizeof from, "from (%.3f, %.3f, %.3f)%s", eye.X(), eye.Y(), eye.Z(), how);

      const DrawnView off = project_view({shape}, spec);
      for (const DrawnCurve& c : off.curves) {
        // A face's outline is no edge: one may run where a tangent edge
        // seen edge on lies (the generator of a cylinder behind a fillet's
        // end, in the reported design of mitcad#1438), and it is drawn.
        if (c.kind == DrawnKind::Outline) {
          continue;
        }
        const std::vector<Point2> line = samples(c);
        if (smooth_lines({line}, edges, spec, tolerance).count == 0) {
          continue;
        }
        if (mode != Mode::Exact && on_polygonal_outline(line, edges, spec, tolerance)) {
          ++counts.outline_chords;
          continue;
        }
        ++counts.tangent_off;
        std::fprintf(stderr, "%s: a tangent edge drawn %s %s as %s: (%.3f, %.3f) to (%.3f, %.3f)\n", name.c_str(),
                     c.visible ? "seen" : "hidden", from, c.kind == DrawnKind::Sharp ? "sharp" : "smooth",
                     line.front()[0], line.front()[1], line.back()[0], line.back()[1]);
        // The smooth edges it lies along: where they run in the model,
        // their faces and the angle between the view and the faces.
        for (const EdgeLine& edge : edges) {
          if (!edge.smooth || !along(line, edge, spec, tolerance)) {
            continue;
          }
          const gp_Pnt& a = edge.points.front();
          const gp_Pnt& m = edge.points[edge.points.size() / 2];
          const gp_Pnt& b = edge.points.back();
          double facing = 0.0;
          for (const gp_Vec& n : edge.normals) {
            facing = std::max(facing, std::abs(n.Dot(gp_Vec(eye))));
          }
          std::fprintf(stderr,
                       "  along edge %d (%s%s): (%.3f, %.3f, %.3f) (%.3f, %.3f, %.3f) (%.3f, %.3f, %.3f), "
                       "normals at most %.4f toward the eye\n",
                       edge.index, edge.what.c_str(), edge.seam ? ", seam" : "", a.X(), a.Y(), a.Z(), m.X(), m.Y(),
                       m.Z(), b.X(), b.Y(), b.Z(), facing);
        }
      }

      DrawingViewSpec with_tangents = spec;
      with_tangents.smooth = true;
      const DrawnView on = project_view({shape}, with_tangents);
      const SmoothLines seams = smooth_lines(edge_lines_of(on), edges, spec, tolerance);
      counts.seams += seams.seams;
      for (const EdgeLine& edge : edges) {
        if (edge.seam || edge.within) {
          continue;
        }
        ++counts.checked;
        // Away from its ends, where another edge's line meets it.
        constexpr std::size_t kMargin = 8;
        for (std::size_t k = kMargin; k + kMargin < edge.points.size(); k += 4) {
          const Point2 p = projected(edge.points[k], spec);
          double nearest = 1e300;
          for (const DrawnCurve& c : on.curves) {
            nearest = std::min(nearest, distance_to(c, p));
          }
          if (nearest > tolerance) {
            ++counts.partial;
            std::fprintf(stderr, "%s: a%s edge not drawn %s at (%.3f, %.3f), %.3f off\n", name.c_str(),
                         edge.smooth ? " tangent" : "n", from, p[0], p[1], nearest);
            break;
          }
        }
      }
    }
  }
  std::printf("%s: with Hidden Edges, %d lines along tangent edges or seams with Tangent Edges off (and %d "
              "polygonal outline chords along them), %d lines along seams with it on, %d of %d edges not drawn "
              "whole\n",
              name.c_str(), counts.tangent_off, counts.outline_chords, counts.seams, counts.partial, counts.checked);
  CHECK(counts.tangent_off == 0);
  CHECK(counts.seams == 0);
  CHECK(counts.partial == 0);
  CHECK(counts.checked > 0);
  return counts;
}

// A tube of 20 mm with a 2 mm wall bent by a quarter circle of radius 30
// between two straight lengths of 50 mm (mitcad#2447, GitHub #686: such a
// tube's drawing): its faces cylinders and tori, meeting tangentially at
// the bend's ends.
ShapePtr bent_tube() {
  ModelCurve in;
  in.kind = ModelCurveKind::Line;
  in.start = gp_Pnt(0.0, 0.0, 0.0);
  in.end = gp_Pnt(50.0, 0.0, 0.0);
  ModelCurve bend;
  bend.kind = ModelCurveKind::Conic;
  bend.center = gp_Pnt(50.0, 30.0, 0.0);
  bend.normal = gp_Dir(0.0, 0.0, 1.0);
  bend.x_axis = gp_Dir(0.0, -1.0, 0.0);
  bend.major = 30.0;
  bend.minor = 30.0;
  bend.first = 0.0;
  bend.last = 3.14159265358979323846 / 2.0;
  ModelCurve out;
  out.kind = ModelCurveKind::Line;
  out.start = gp_Pnt(80.0, 30.0, 0.0);
  out.end = gp_Pnt(80.0, 80.0, 0.0);
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = {{"c1", in}, {"c2", bend}, {"c3", out}};
  spec.size = 20.0;
  spec.thickness = 2.0;
  return pipe(spec);
}

// The bent tube's views (mitcad#2447): with Tangent Edges off no line
// along the bend's ends, seen or hidden; with it on those drawn whole and
// thin, and the seams of the cylinders and the tori never, which the
// sweep had stored as smooth edges (G1) so that they were drawn as tangent
// edges; the same with the continuity left out, as a later operation or
// an import leaves it.
void test_tangent_edges_of_a_bent_tube() {
  const ShapePtr tube = bent_tube();
  // The premise: the sweep stores G1 on the seams it makes.
  const int smooth_seams = seams_stored_smooth(tube->occt());
  std::printf("bent tube: %d seams stored as smooth edges\n", smooth_seams);
  CHECK(smooth_seams > 0);
  check_tangent_edges(tube, "bent tube");
  check_hidden_edges(tube, "bent tube");
  // The outline crosses the circles where the bend meets the straight
  // lengths.
  check_polygonal_outline(tube, "bent tube");
  const ShapePtr bare = bent_tube();
  clear_continuities(bare->occt());
  check_tangent_edges(bare, "bent tube without continuity");
  check_hidden_edges(bare, "bent tube without continuity");
}

// GitHub #686's shape without its shell (mitcad#2447): a circle of 20 mm
// swept up 25 mm from the origin and on round a quarter circle of radius
// 30 towards -X, joined with its mirror image across the YZ plane (two
// bent tubes on a common stem, a Y), the edges where the two tori meet at
// the crotch rounded 10 mm. The fillet is free-form and its boundaries run
// slantwise through its own and the tori's parameters. Null when a step
// fails.
ShapePtr crotch_filleted_y() {
  ModelCurve stem;
  stem.kind = ModelCurveKind::Line;
  stem.start = gp_Pnt(0.0, 0.0, 0.0);
  stem.end = gp_Pnt(0.0, 0.0, 25.0);
  ModelCurve bend;
  bend.kind = ModelCurveKind::Conic;
  bend.center = gp_Pnt(-30.0, 0.0, 25.0);
  bend.normal = gp_Dir(0.0, -1.0, 0.0);
  bend.x_axis = gp_Dir(1.0, 0.0, 0.0);
  bend.major = 30.0;
  bend.minor = 30.0;
  bend.first = 0.0;
  bend.last = 3.14159265358979323846 / 2.0;
  PipeSpec spec;
  spec.feature = "F3";
  spec.path = {{"c1", stem}, {"c4", bend}};
  spec.size = 20.0;
  const TopoDS_Shape left = pipe(spec)->occt();
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp::Origin(), gp_Dir(1.0, 0.0, 0.0)));
  const TopoDS_Shape right = BRepBuilderAPI_Transform(left, mirror, true).Shape();
  BRepAlgoAPI_Fuse fuse(left, right);
  if (!fuse.IsDone()) {
    std::fprintf(stderr, "crotch: the fuse failed\n");
    return nullptr;
  }
  ShapeUpgrade_UnifySameDomain unify(fuse.Shape(), true, true, false);
  unify.Build();
  const TopoDS_Shape joined = unify.Shape();
  // The crotch: the edges between faces of the two tori.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndUniqueAncestors(joined, TopAbs_EDGE, TopAbs_FACE, faces);
  BRepFilletAPI_MakeFillet fillet(joined);
  int crotch = 0;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& around = faces.FindFromIndex(i);
    if (around.Extent() != 2) {
      continue;
    }
    const BRepAdaptor_Surface a(TopoDS::Face(around.First()), false);
    const BRepAdaptor_Surface b(TopoDS::Face(around.Last()), false);
    if (a.GetType() == GeomAbs_Torus && b.GetType() == GeomAbs_Torus &&
        a.Torus().Location().Distance(b.Torus().Location()) > 1.0) {
      fillet.Add(10.0, TopoDS::Edge(faces.FindKey(i)));
      ++crotch;
    }
  }
  if (crotch == 0) {
    std::fprintf(stderr, "crotch: no edge between the two tori\n");
    return nullptr;
  }
  try {
    fillet.Build();
  } catch (const Standard_Failure&) {
    std::fprintf(stderr, "crotch: the fillet failed\n");
    return nullptr;
  }
  if (!fillet.IsDone()) {
    std::fprintf(stderr, "crotch: the fillet failed\n");
    return nullptr;
  }
  return std::make_shared<Shape>(fillet.Shape());
}

// The edges between two faces that meet tangentially all along them (their
// normals within kTangent at points along it) that OCCT's own test
// (BRepLib::ContinuityOfFaces at that angle) takes as meeting at an angle.
int tangent_edges_occt_misses(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  int count = 0;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& around = faces.FindFromIndex(i);
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(i));
    if (around.Extent() != 2 || BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const TopoDS_Face& face1 = TopoDS::Face(around.First());
    const TopoDS_Face& face2 = TopoDS::Face(around.Last());
    const BRepAdaptor_Curve curve(edge);
    bool tangent = true;
    for (int k = 1; k < 16 && tangent; ++k) {
      gp_Vec n1;
      gp_Vec n2;
      const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 16.0;
      tangent = normals_at(edge, face1, face2, t, n1, n2) && n1.Dot(n2) > 0.0 && n1.Crossed(n2).Magnitude() <= kTangent;
    }
    if (!tangent) {
      continue;
    }
    try {
      count += BRepLib::ContinuityOfFaces(edge, face1, face2, kTangent) < GeomAbs_G1 ? 1 : 0;
    } catch (const Standard_Failure&) {
      ++count;
    }
  }
  return count;
}

// The filleted crotch's views (mitcad#2447): its fillet's boundaries are
// tangent edges all along, none drawn with Tangent Edges off, seen or
// hidden, and drawn whole and thin with it on, exact, on the quick path
// and polygonal; also with the continuity left out, as a later operation
// (the reported design's Shell) leaves it, where OCCT's own test takes some
// of them for edges at an angle (half of the fillet's boundary drawn as a
// sharp edge in the reported drawing).
void test_tangent_edges_of_a_filleted_crotch() {
  const ShapePtr y = crotch_filleted_y();
  CHECK(y != nullptr);
  if (y == nullptr) {
    return;
  }
  check_tangent_edges(y, "filleted crotch");
  check_hidden_edges(y, "filleted crotch");
  // Nothing of the outline left out next to the fillet's tips.
  check_polygonal_outline(y, "filleted crotch");
  const ShapePtr bare = crotch_filleted_y();
  if (bare == nullptr) {
    return;
  }
  clear_continuities(bare->occt());
  // The premise: OCCT's own test misses tangent edges of the fillet.
  const int missed = tangent_edges_occt_misses(bare->occt());
  std::printf("filleted crotch: %d tangent edges OCCT's own test takes for edges at an angle\n", missed);
  CHECK(missed > 0);
  check_tangent_edges(bare, "filleted crotch without continuity");
  check_hidden_edges(bare, "filleted crotch without continuity");
}

// The reported design of mitcad#1438 (a bearing block with fillets), its
// body written by `mitcad-cli export` as a .brep file
// (cli.drawing_tangent_edges_reported): no tangent edges drawn with
// Tangent Edges off: the body as the model makes it, and the same body with
// its edges' continuity left out as a later operation or an import leaves
// it.
int check_brep_file(const char* path) {
  TopoDS_Shape shape;
  TopoDS_Shape bare;
  BRep_Builder builder;
  if (!BRepTools::Read(shape, path, builder) || shape.IsNull() || !BRepTools::Read(bare, path, builder) ||
      bare.IsNull()) {
    std::fprintf(stderr, "test_drawing: cannot read %s\n", path);
    return 1;
  }
  test::guarded("the reported design's tangent edges", [&] {
    // OCCT's fillet records G1 on the tangent edges it makes (port patch
    // 0350 takes their tangency whichever way the faces face, mitcad#1880),
    // so OCCT's hidden-line removal alone draws none of them as sharp; the
    // check holds the drawing to that.
    check_tangent_edges(std::make_shared<Shape>(shape), path);
  });
  test::guarded("the reported design's tangent edges without their continuity", [&] {
    clear_continuities(bare);
    const TangentViewCounts counts =
        check_tangent_edges(std::make_shared<Shape>(bare), std::string(path) + " without continuity");
    // The premise: without the continuity, the body's tangent edges are
    // drawn as sharp ones by OCCT's hidden-line removal alone.
    CHECK(counts.raw > counts.raw_seams);
  });
  test::guarded("the reported design's hidden edges and seams", [&] {
    // mitcad#2447: hidden tangent edges left out with Tangent Edges off,
    // seams never drawn, every other edge drawn whole with both on.
    TopoDS_Shape again;
    if (BRepTools::Read(again, path, builder) && !again.IsNull()) {
      check_hidden_edges(std::make_shared<Shape>(again), path);
    }
  });
  if (test::failures == 0) {
    std::puts("test_drawing: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}

// `test_drawing --edges <file.brep>`, a look at a body's edges for a
// drawing that draws them wrongly (mitcad#2447):
// each edge between two faces (or a seam) with its faces' surfaces, the
// continuity stored on it, OCCT's continuity of its faces at the drawings'
// angle, the largest angle between their normals along it (degrees) and
// its length.
// `test_drawing --project <file.brep> <x> <y> <z>`, a probe (mitcad#2447):
// the body seen toward the eye (x, y, z) with the exact algorithm, hidden
// edges and without tangent edges, each drawn line with its kind, whether
// it is seen and its ends; with MITCAD_DRAWING_TRACE=edges the algorithm's
// flags on each face's edges too.
int print_view(const char* path, const gp_Dir& eye) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  if (!BRepTools::Read(shape, path, builder) || shape.IsNull()) {
    std::fprintf(stderr, "test_drawing: cannot read %s\n", path);
    return 1;
  }
  const gp_Ax2 axes(gp::Origin(), eye);
  const DrawnView drawn = project_view({std::make_shared<Shape>(shape)}, view(eye, axes.XDirection(), true));
  for (const DrawnCurve& c : drawn.curves) {
    const std::vector<Point2> points = samples(c);
    std::printf("line %s %s: (%.3f, %.3f) to (%.3f, %.3f)\n", c.visible ? "seen" : "hidden",
                c.kind == DrawnKind::Sharp ? "sharp" : (c.kind == DrawnKind::Outline ? "outline" : "smooth"),
                points.front()[0], points.front()[1], points.back()[0], points.back()[1]);
  }
  return 0;
}

int print_edges(const char* path) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  if (!BRepTools::Read(shape, path, builder) || shape.IsNull()) {
    std::fprintf(stderr, "test_drawing: cannot read %s\n", path);
    return 1;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(i));
    const NCollection_List<TopoDS_Shape>& around = faces.FindFromIndex(i);
    if (BRep_Tool::Degenerated(edge) || around.IsEmpty()) {
      continue;
    }
    const TopoDS_Face& face1 = TopoDS::Face(around.First());
    const TopoDS_Face& face2 = TopoDS::Face(around.Last());
    const BRepAdaptor_Curve curve(edge);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    double angle = 0.0;
    for (int k = 1; k < 32; ++k) {
      gp_Vec n1;
      gp_Vec n2;
      if (normals_at(edge, face1, face2, first + (last - first) * k / 32.0, n1, n2)) {
        angle = std::max(angle, n1.Angle(n2));
      }
    }
    const char* found = "-";
    try {
      found = continuity_name(BRepLib::ContinuityOfFaces(edge, face1, face2, kTangent));
    } catch (const Standard_Failure&) {
      found = "failed";
    }
    const gp_Pnt a = curve.Value(first);
    const gp_Pnt b = curve.Value(last);
    std::printf("edge %d: %s faces %d (%s, %s) stored %s found %s normals %.4f deg ends (%.3f, %.3f, %.3f) "
                "(%.3f, %.3f, %.3f)\n",
                i, curve.GetType() == GeomAbs_Line ? "line" : (curve.GetType() == GeomAbs_Circle ? "circle" : "curve"),
                around.Extent(), surface_name(face1), surface_name(face2),
                continuity_name(BRep_Tool::Continuity(edge, face1, face2)), found, angle * 180.0 / 3.14159265358979,
                a.X(), a.Y(), a.Z(), b.X(), b.Y(), b.Z());
  }
  return 0;
}

// A 30 x 20 x 10 box from model_render's isometric camera, as its line art
// projects it (mitcad#1736): looking along (-1, 1, -1) with Z up, the x axis
// the direction times up. The nine edges at the three faces seen are drawn
// seen and whole; the three at the far corner only with hidden lines.
void test_box_from_the_iso_view() {
  const ShapePtr box = std::make_shared<Shape>(BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), 30.0, 20.0, 10.0).Shape());
  const gp_Vec direction(-1.0, 1.0, -1.0);
  const gp_Dir toward_eye(-direction);
  const gp_Dir x_axis(direction.Crossed(gp_Vec(0.0, 0.0, 1.0)));
  for (const bool hidden : {false, true}) {
    const DrawnView drawn = project_view({box}, view(toward_eye, x_axis, hidden));
    CHECK(!drawn.polygonal);
    int seen = 0;
    for (const DrawnCurve& c : drawn.curves) {
      seen += c.visible ? 1 : 0;
      CHECK(c.type == DrawnCurve::Type::Line);
    }
    CHECK(seen == 9);
    CHECK(hidden_count(drawn) == (hidden ? 3 : 0));
  }
}

bool same_curves(const DrawnView& a, const DrawnView& b) {
  if (a.curves.size() != b.curves.size() || a.polygonal != b.polygonal) {
    return false;
  }
  for (std::size_t i = 0; i < a.curves.size(); ++i) {
    const DrawnCurve& x = a.curves[i];
    const DrawnCurve& y = b.curves[i];
    if (x.type != y.type || x.visible != y.visible || x.kind != y.kind || x.source != y.source ||
        x.points != y.points || x.center != y.center || x.radius != y.radius || x.start != y.start ||
        x.end != y.end) {
      return false;
    }
  }
  return true;
}

// A view of many parts on the quick path (mitcad#1997): its parts are
// boxed and meshed beside each other on OCCT's threads, each as it would
// be alone, so the view is the same every time and each part's lines are
// its own; and the bodies' boxes measured beside each other are those
// measured one by one, a shape given twice measured once.
void test_many_parts_on_the_quick_path() {
  std::vector<ShapePtr> parts;
  for (int k = 0; k < 24; ++k) {
    const double x = 30.0 * (k % 8);
    const double y = 30.0 * (k / 8);
    const TopoDS_Shape post = cylinder(x, y, 0.0, 10.0 + k, 6.0 + 0.25 * k);
    parts.push_back(std::make_shared<Shape>(BRepBuilderAPI_NurbsConvert(post, true).Shape()));
  }
  // An exact block across the first row, behind its posts from the front.
  parts.push_back(std::make_shared<Shape>(BRepPrimAPI_MakeBox(gp_Pnt(-10.0, 8.0, 0.0), 240.0, 5.0, 8.0).Shape()));
  DrawingViewSpec spec = view(gp_Dir(0, -1, 0), gp_Dir(1, 0, 0), true);
  spec.quick = true;
  spec.quick_deviation = 0.05;
  const DrawnView first = project_view(parts, spec);
  const DrawnView again = project_view(parts, spec);
  CHECK(first.polygonal);
  CHECK(same_curves(first, again));
  std::vector<int> lines(parts.size(), 0);
  for (const DrawnCurve& c : first.curves) {
    CHECK(c.source >= 0 && c.source < static_cast<int>(parts.size()));
    if (c.source >= 0 && c.source < static_cast<int>(parts.size())) {
      ++lines[static_cast<std::size_t>(c.source)];
    }
  }
  for (std::size_t k = 0; k < parts.size(); ++k) {
    if (lines[k] == 0) {
      std::fprintf(stderr, "part %zu has no lines of its own\n", k);
      ++test::failures;
    }
  }
  std::vector<ShapePtr> boxed = parts;
  boxed.push_back(parts[3]);
  // Fresh shapes: a box is kept on its shape once measured.
  std::vector<ShapePtr> alone;
  for (const ShapePtr& part : boxed) {
    alone.push_back(std::make_shared<Shape>(part->occt()));
  }
  const std::vector<BoundingBox> together = bounding_boxes(boxed);
  CHECK(together.size() == boxed.size());
  for (std::size_t k = 0; k < together.size() && k < alone.size(); ++k) {
    const BoundingBox one = bounding_box(*alone[k]);
    CHECK(!together[k].empty && !one.empty);
    CHECK(together[k].min.Distance(one.min) == 0.0 && together[k].max.Distance(one.max) == 0.0);
  }
}

// Small round edges on the quick path at a small scale (mitcad#2014): a
// 2 m free-form plate at 1:20, where the triangulation may deviate by
// 0.27 mm, so that its holes of 2 to 5 mm and its rounded corners of 4 mm
// get the fewest chords the triangulation's angle allows. Their circles
// and quarter circles are still drawn as arcs, and every line lies within
// the view's deviation of the exact view's.
void test_small_round_edges_at_a_small_scale() {
  TopoDS_Shape plate = rounded_bar(gp_Pnt(0.0, 0.0, 0.0), gp_Vec(2000.0, 60.0, 10.0), 2, 4.0);
  plate = BRepAlgoAPI_Cut(plate, cylinder(100.0, 30.0, -1.0, 11.0, 2.0)).Shape();
  plate = BRepAlgoAPI_Cut(plate, cylinder(200.0, 30.0, -1.0, 11.0, 3.0)).Shape();
  plate = BRepAlgoAPI_Cut(plate, cylinder(300.0, 30.0, -1.0, 11.0, 5.0)).Shape();
  const ShapePtr exact_plate = std::make_shared<Shape>(plate);
  const ShapePtr free_plate = std::make_shared<Shape>(BRepBuilderAPI_NurbsConvert(plate, true).Shape());
  DrawingViewSpec spec = view(gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), false);
  spec.quick = true;
  spec.quick_deviation = 0.4;               // 0.02 mm on the sheet at 1:20
  spec.quick_chord_angle = 3.14159265358979 / 6.0; // ViewSpec::quick_chord_angle at 1:20
  const DrawnView quick = project_view({free_plate}, spec);
  DrawingViewSpec exact_spec = spec;
  exact_spec.quick = false;
  const DrawnView exact = project_view({exact_plate}, exact_spec);
  CHECK(quick.polygonal);
  CHECK(!exact.polygonal);
  CHECK(has_arc_near(quick, 100.0, 30.0, 2.0));
  CHECK(has_arc_near(quick, 200.0, 30.0, 3.0));
  CHECK(has_arc_near(quick, 300.0, 30.0, 5.0));
  CHECK(has_arc_near(quick, 4.0, 4.0, 4.0));
  CHECK(has_arc_near(quick, 1996.0, 4.0, 4.0));
  CHECK(has_arc_near(quick, 4.0, 56.0, 4.0));
  CHECK(has_arc_near(quick, 1996.0, 56.0, 4.0));
  const double there = deviation(quick, exact, true);
  const double back = deviation(exact, quick, true);
  std::fprintf(stderr, "small round edges at 1:20: the quick view %.4f mm from the exact one, the exact %.4f mm from it\n",
               there, back);
  CHECK(there <= spec.quick_deviation && back <= spec.quick_deviation);
}

// --- Outlines along the edges of split round faces (mitcad#2539) ---------------

constexpr double kShaftRadius = 6.35;

// A shaft of radius kShaftRadius along z from -60 to 260 whose round face
// is split in two halves along x = +-kShaftRadius (`split`, as many
// imported bodies are), or whole with its seam at x = kShaftRadius, turned
// by `turn` radians about its axis (an occurrence's placement within
// rounding). Seen from the front or the back, those edges lie on its
// outline.
TopoDS_Shape shaft_along_z(bool split, double turn) {
  TopoDS_Shape shaft =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0.0, 0.0, -60.0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), kShaftRadius, 320.0)
          .Shape();
  if (split) {
    ShapeUpgrade_ShapeDivideClosed divide(shaft);
    divide.SetNbSplitPoints(1);
    divide.Perform();
    shaft = divide.Result();
  }
  if (turn != 0.0) {
    gp_Trsf rotation;
    rotation.SetRotation(gp_Ax1(gp::Origin(), gp_Dir(0, 0, 1)), turn);
    shaft = BRepBuilderAPI_Transform(shaft, rotation, true).Shape();
  }
  return shaft;
}

int round_faces(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    n += BRepAdaptor_Surface(TopoDS::Face(it.Current())).GetType() == GeomAbs_Cylinder ? 1 : 0;
  }
  return n;
}

// The points every half millimetre along x = `x` from y0 to y1 that no
// curve of the view (seen or hidden, `visible`) passes within 0.01 of.
int gaps_along(const DrawnView& drawn, double x, double y0, double y1, bool visible) {
  int gaps = 0;
  for (double y = y0 + 0.25; y < y1; y += 0.5) {
    double nearest = 1e300;
    for (const DrawnCurve& c : drawn.curves) {
      if (c.visible == visible) {
        nearest = std::min(nearest, distance_to(c, {x, y}));
      }
    }
    gaps += nearest > 0.01 ? 1 : 0;
  }
  return gaps;
}

// The reported view (GitHub #729): a subassembly's projected view at 1:5
// without tangent edges drew its shafts with gaps in their outline lines,
// wholly missing beside a frame's upright and in parts elsewhere. A shaft
// whose round face is split along its outline, or whose seam lies on it,
// through a plate 10 thick (z from 0 to 10, the upright), seen from the
// front and the back: both its outline lines are drawn seen all along
// outside the plate and hidden inside it (with hidden edges), whatever
// small turn its placement gives it. Seen from the isometric view, where
// those edges are no outline, they are not drawn without tangent edges.
void test_outlines_along_split_round_faces() {
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-30.0, -20.0, 0.0), 60.0, 40.0, 10.0).Shape();
  plate = BRepAlgoAPI_Cut(plate, cylinder(0.0, 0.0, -1.0, 11.0, 6.5)).Shape();
  const ShapePtr upright = std::make_shared<Shape>(plate);
  const std::vector<std::pair<gp_Dir, gp_Dir>> sides = {{gp_Dir(0, -1, 0), gp_Dir(1, 0, 0)},
                                                        {gp_Dir(0, 1, 0), gp_Dir(-1, 0, 0)}};
  const double r2 = 1.0 / std::sqrt(2.0);
  const double r3 = 1.0 / std::sqrt(3.0);
  for (const bool split : {true, false}) {
    for (const double turn : {0.0, 1e-9, 1e-7, -1e-7, 1e-6}) {
      const TopoDS_Shape body = shaft_along_z(split, turn);
      CHECK(round_faces(body) == (split ? 2 : 1));
      const ShapePtr shaft = std::make_shared<Shape>(body);
      char name[96];
      std::snprintf(name, sizeof name, "a %s shaft turned %g", split ? "split" : "seamed", turn);
      for (const auto& [eye, x] : sides) {
        for (const bool hidden : {false, true}) {
          DrawingViewSpec spec = view(eye, x, hidden);
          spec.quick = true;
          spec.quick_deviation = 0.1; // 0.02 mm on the sheet at 1:5
          const DrawnView drawn = project_view({upright, shaft}, spec);
          CHECK(!drawn.polygonal);
          for (const double side : {-kShaftRadius, kShaftRadius}) {
            const int seen = gaps_along(drawn, side, -60.0, -0.5, true) + gaps_along(drawn, side, 10.5, 260.0, true);
            const int behind = hidden ? gaps_along(drawn, side, 0.5, 9.5, false) : 0;
            if (seen != 0 || behind != 0) {
              std::fprintf(stderr,
                           "%s from (%.0f, %.0f, %.0f)%s: the outline at x = %.2f has %d seen and %d hidden gaps\n",
                           name, eye.X(), eye.Y(), eye.Z(), hidden ? " with hidden edges" : "", side, seen, behind);
            }
            CHECK(seen == 0);
            CHECK(behind == 0);
          }
        }
      }
      // From the isometric view the edges along x = +-kShaftRadius are no
      // outline: nothing is drawn along them halfway up the shaft.
      const DrawingViewSpec iso = view(gp_Dir(r3, -r3, r3), gp_Dir(r2, r2, 0), false);
      const DrawnView drawn = project_view({upright, shaft}, iso);
      for (const double side : {-kShaftRadius, kShaftRadius}) {
        const gp_Pnt on_edge = gp_Pnt(side * std::cos(turn), side * std::sin(turn), 100.0);
        const Point2 p = projected(on_edge, iso);
        double nearest = 1e300;
        for (const DrawnCurve& c : drawn.curves) {
          if (c.visible) {
            nearest = std::min(nearest, distance_to(c, p));
          }
        }
        if (nearest <= 0.05) {
          std::fprintf(stderr, "%s, isometric: a line along the edge at x = %.2f\n", name, side);
        }
        CHECK(nearest > 0.05);
      }
    }
  }
}

// --- A probe of round faces' outlines at a view's scale (mitcad#2539) -----------

// The stretches (in the view's x) of the outline line from `a` to `b` that
// no seen curve of `drawn` passes within 0.05 mm of, sampled every 0.5 mm.
std::string gaps_on(const DrawnView& drawn, const Point2& a, const Point2& b) {
  const double length = std::hypot(b[0] - a[0], b[1] - a[1]);
  const int n = std::max(2, static_cast<int>(length / 0.5));
  std::string out;
  double from = 0.0;
  bool open = false;
  int missing = 0;
  for (int k = 0; k <= n; ++k) {
    const double t = static_cast<double>(k) / n;
    const Point2 p{a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t};
    double nearest = 1e300;
    for (const DrawnCurve& c : drawn.curves) {
      if (c.visible) {
        nearest = std::min(nearest, distance_to(c, p));
      }
    }
    const bool gap = nearest > 0.05;
    missing += gap ? 1 : 0;
    if (gap && !open) {
      from = p[0];
      open = true;
    } else if (!gap && open) {
      char text[64];
      std::snprintf(text, sizeof text, " [%.1f, %.1f]", from, p[0]);
      out += text;
      open = false;
    }
  }
  if (open) {
    char text[64];
    std::snprintf(text, sizeof text, " [%.1f, %.1f]", from, b[0]);
    out += text;
  }
  char head[64];
  std::snprintf(head, sizeof head, "%d/%d missing", missing, n + 1);
  return head + out;
}

// `test_drawing --round-outlines <file.brep> <scale>`, a probe (mitcad#2539):
// the solids of a file (a design's bodies where it places them) seen from
// the front (toward the eye -y, x to the right) as a view at 1:<1/scale>
// on a sheet draws them: on the quick path with the view's deviation and
// chord angle, without hidden edges and tangent edges. For each round face
// whose axis lies square to the view and that is longer than 50 mm: the
// triangulation the polygonal algorithm meshes it with (its triangles'
// least |n . eye|, how many lie within 1e-3 of edge on, and the nodes
// nearest each outline, in degrees about the axis from it), and the
// stretches of its two outline lines no seen line covers: in the view of
// all solids, of its solid alone polygonal, and of its solid alone exact.
int print_round_outlines(const char* path, double scale) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  if (!BRepTools::Read(shape, path, builder) || shape.IsNull() || !(scale > 0.0)) {
    std::fprintf(stderr, "test_drawing: cannot read %s\n", path);
    return 1;
  }
  std::vector<TopoDS_Shape> solids;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    solids.push_back(it.Current());
  }
  const gp_Dir eye(0.0, -1.0, 0.0);
  DrawingViewSpec spec = view(eye, gp_Dir(1.0, 0.0, 0.0), false);
  spec.quick = true;
  spec.quick_deviation = std::max(0.02 / scale, spec.deflection); // ViewSpec::quick_deviation
  spec.quick_chord_angle = std::min(std::max(0.2 / scale, 0.2), 3.14159265358979 / 6.0);
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  const double size = box.IsVoid() ? 1.0 : std::sqrt(box.SquareExtent());
  const double deflection = std::max(spec.deflection, std::min(size * 5e-4, spec.quick_deviation / 1.5));
  std::printf("scale %g: %zu solids, deviation %.4g, chord angle %.4g, mesh deflection %.4g (all parts quick)\n",
              scale, solids.size(), spec.quick_deviation, spec.quick_chord_angle, deflection);
  std::vector<ShapePtr> all;
  for (const TopoDS_Shape& s : solids) {
    all.push_back(std::make_shared<Shape>(s));
  }
  const DrawnView together = project_view(all, spec);
  std::printf("all solids: %s, %zu lines\n", together.polygonal ? "polygonal" : "exact", together.curves.size());
  DrawingViewSpec polygonal = spec;
  polygonal.exact = false;
  DrawingViewSpec exact = spec;
  exact.quick = false;
  for (std::size_t i = 0; i < solids.size(); ++i) {
    TopoDS_Shape meshed = BRepBuilderAPI_Copy(solids[i], true, false).Shape();
    const BRepMesh_IncrementalMesh mesher(meshed, deflection, false, spec.quick_chord_angle, true);
    bool alone_done = false;
    DrawnView alone_polygonal;
    DrawnView alone_exact;
    int f = 0;
    for (TopExp_Explorer it(meshed, TopAbs_FACE); it.More(); it.Next(), ++f) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      const BRepAdaptor_Surface surface(face);
      if (surface.GetType() != GeomAbs_Cylinder) {
        continue;
      }
      const gp_Cylinder cyl = surface.Cylinder();
      const gp_Dir axis = cyl.Axis().Direction();
      if (std::abs(gp_Vec(axis).Dot(gp_Vec(eye))) > 0.01) {
        continue;
      }
      const double v0 = surface.FirstVParameter();
      const double v1 = surface.LastVParameter();
      if (v1 - v0 < 50.0) {
        continue;
      }
      const gp_Dir side = axis.Crossed(eye); // the radial direction of an outline
      const double r = cyl.Radius();
      // The triangulation.
      double least = 1e300;
      int edge_on = 0;
      int triangles = 0;
      double nearest[2][2] = {{-1e300, 1e300}, {-1e300, 1e300}}; // per outline: below, above (degrees)
      TopLoc_Location location;
      const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
      if (!mesh.IsNull()) {
        const gp_Trsf placement = location.Transformation();
        auto node = [&](int k) {
          gp_Pnt p = mesh->Node(k);
          if (!location.IsIdentity()) {
            p.Transform(placement);
          }
          return p;
        };
        for (int t = 1; t <= mesh->NbTriangles(); ++t) {
          int a = 0;
          int b = 0;
          int c = 0;
          mesh->Triangle(t).Get(a, b, c);
          const gp_Pnt pa = node(a);
          const gp_Vec n = gp_Vec(pa, node(b)).Crossed(gp_Vec(pa, node(c)));
          if (n.Magnitude() < 1e-12) {
            continue;
          }
          const double s = std::abs(n.Normalized().Dot(gp_Vec(eye)));
          ++triangles;
          least = std::min(least, s);
          edge_on += s < 1e-3 ? 1 : 0;
        }
        for (int k = 1; k <= mesh->NbNodes(); ++k) {
          gp_Vec v(cyl.Location(), node(k));
          v -= gp_Vec(axis) * v.Dot(gp_Vec(axis));
          if (v.Magnitude() < 1e-9) {
            continue;
          }
          const double c = v.Dot(gp_Vec(side));
          const double deg = std::asin(std::max(-1.0, std::min(1.0, v.Normalized().Dot(gp_Vec(eye))))) * 180.0 /
                             3.14159265358979;
          double(&at)[2] = nearest[c > 0.0 ? 0 : 1];
          if (deg <= 0.0) {
            at[0] = std::max(at[0], deg);
          }
          if (deg >= 0.0) {
            at[1] = std::min(at[1], deg);
          }
        }
      }
      std::printf("solid %zu face %d: round r=%.3f length %.1f, %d triangles, least |n.eye| %.2e, %d edge on; nodes "
                  "about outline +: %.3f/%.3f deg, outline -: %.3f/%.3f deg\n",
                  i, f, r, v1 - v0, triangles, least, edge_on, nearest[0][0], nearest[0][1], nearest[1][0],
                  nearest[1][1]);
      if (!alone_done) {
        alone_polygonal = project_view({all[i]}, polygonal);
        alone_exact = project_view({all[i]}, exact);
        alone_done = true;
      }
      for (const double sign : {1.0, -1.0}) {
        // The outline line along the face, where the face reaches it.
        const gp_Pnt on = cyl.Location().Translated(gp_Vec(side) * (sign * r));
        double u = 0.0;
        double v = 0.0;
        ElSLib::Parameters(cyl, on, u, v);
        const double u0 = surface.FirstUParameter();
        double uu = u;
        while (uu < u0) {
          uu += 2.0 * 3.14159265358979;
        }
        while (uu > u0 + 2.0 * 3.14159265358979) {
          uu -= 2.0 * 3.14159265358979;
        }
        if (uu > surface.LastUParameter() + 1e-9) {
          continue; // the face does not reach this outline
        }
        const Point2 a = projected(ElSLib::Value(uu, v0, cyl), spec);
        const Point2 b = projected(ElSLib::Value(uu, v1, cyl), spec);
        std::printf("  outline %s from (%.1f, %.2f) to (%.1f, %.2f): all %s; alone polygonal %s; alone exact %s\n",
                    sign > 0 ? "+" : "-", a[0], a[1], b[0], b[1], gaps_on(together, a, b).c_str(),
                    gaps_on(alone_polygonal, a, b).c_str(), gaps_on(alone_exact, a, b).c_str());
      }
    }
  }
  return 0;
}

} // namespace

MITCAD_BUNDLED_TEST_ARGS(drawing) {
  if (argc == 4 && std::string(argv[1]) == "--round-outlines") {
    return print_round_outlines(argv[2], std::atof(argv[3]));
  }
  if (argc == 3 && std::string(argv[1]) == "--tangent-edges") {
    return check_brep_file(argv[2]);
  }
  if (argc == 3 && std::string(argv[1]) == "--edges") {
    return print_edges(argv[2]);
  }
  if (argc == 6 && std::string(argv[1]) == "--project") {
    return print_view(argv[2], gp_Dir(std::atof(argv[3]), std::atof(argv[4]), std::atof(argv[5])));
  }
  test::guarded("three views", test_three_views);
  test::guarded("a section view", test_section_view);
  test::guarded("a section limited to its line", test_bounded_section_view);
  test::guarded("round holes", test_round_holes);
  test::guarded("lines and arcs from segments", test_fit_segments);
  test::guarded("a line into an arc", test_fit_a_line_into_an_arc);
  test::guarded("a closed rounded outline", test_fit_a_closed_rounded_outline);
  test::guarded("the quick path", test_quick_path);
  test::guarded("the quick path for curved edges", test_quick_path_for_curved_edges);
  test::guarded("the quick path at the view's scale", test_quick_path_at_the_view_scale);
  test::guarded("the tangent edges of a filleted block", test_tangent_edges_of_a_filleted_block);
  test::guarded("a box from the iso view", test_box_from_the_iso_view);
  test::guarded("many parts on the quick path", test_many_parts_on_the_quick_path);
  test::guarded("small round edges at a small scale", test_small_round_edges_at_a_small_scale);
  test::guarded("the tangent edges, seams and hidden edges of a bent tube", test_tangent_edges_of_a_bent_tube);
  test::guarded("the tangent edges of a filleted crotch", test_tangent_edges_of_a_filleted_crotch);
  test::guarded("outlines along split round faces", test_outlines_along_split_round_faces);
  if (test::failures == 0) {
    std::puts("test_drawing: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
