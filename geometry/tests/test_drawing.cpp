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
#include <memory>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <gp_Ax2.hxx>

#include "check.hpp"
#include "mitcad/geometry/drawing.hpp"

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

bool has_arc_near(const DrawnView& drawn, double cx, double cy, double radius) {
  for (const DrawnCurve& c : drawn.curves) {
    if (c.type == DrawnCurve::Type::Arc && c.visible && std::abs(c.center[0] - cx) < 1e-4 &&
        std::abs(c.center[1] - cy) < 1e-4 && std::abs(c.radius - radius) < 1e-4) {
      return true;
    }
  }
  return false;
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

} // namespace

int main() {
  test::guarded("three views", test_three_views);
  test::guarded("a section view", test_section_view);
  test::guarded("round holes", test_round_holes);
  test::guarded("lines and arcs from segments", test_fit_segments);
  test::guarded("the quick path", test_quick_path);
  if (test::failures == 0) {
    std::puts("test_drawing: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
