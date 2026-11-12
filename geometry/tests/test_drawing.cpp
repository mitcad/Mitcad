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
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

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
struct EdgeLine {
  std::vector<gp_Pnt> points;
  bool smooth = false;
  bool seam = false;
  // At the smooth edge's points, the face's normal (at right angles to a
  // view all along, the edge lies on that view's outline).
  std::vector<gp_Vec> normals;
};

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
  const double r2 = 1.0 / std::sqrt(2.0);
  const double r3 = 1.0 / std::sqrt(3.0);
  const std::vector<std::pair<gp_Dir, gp_Dir>> directions{
      {gp_Dir(0, -1, 0), gp_Dir(1, 0, 0)},  {gp_Dir(0, 1, 0), gp_Dir(-1, 0, 0)}, {gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)},
      {gp_Dir(0, 0, -1), gp_Dir(1, 0, 0)},  {gp_Dir(1, 0, 0), gp_Dir(0, 1, 0)},  {gp_Dir(-1, 0, 0), gp_Dir(0, -1, 0)},
      {gp_Dir(r3, -r3, r3), gp_Dir(r2, r2, 0)}};
  TangentViewCounts counts;
  for (const auto& [eye, x] : directions) {
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
    counts.off_polygonal += quick_lines.count;
    for (const std::vector<Point2>& line : quick_lines.lines) {
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
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(block, TopAbs_EDGE, TopAbs_FACE, faces);
  BRep_Builder builder;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& around = faces.FindFromIndex(i);
    if (around.Extent() == 2) {
      builder.Continuity(TopoDS::Edge(faces.FindKey(i)), TopoDS::Face(around.First()), TopoDS::Face(around.Last()),
                         GeomAbs_C0);
    }
  }
  const TangentViewCounts counts = check_tangent_edges(std::make_shared<Shape>(block), "filleted block");
  // The test's premise: as it is, the shape's tangent edges are drawn as
  // sharp ones.
  CHECK(counts.raw > counts.raw_seams);
}

// The reported design of mitcad#1438 (a bearing block with fillets), its
// body written by `mitcad-cli export` as a .brep file
// (cli.drawing_tangent_edges_reported): no tangent edges drawn with
// Tangent Edges off.
int check_brep_file(const char* path) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  if (!BRepTools::Read(shape, path, builder) || shape.IsNull()) {
    std::fprintf(stderr, "test_drawing: cannot read %s\n", path);
    return 1;
  }
  test::guarded("the reported design's tangent edges", [&] {
    const TangentViewCounts counts = check_tangent_edges(std::make_shared<Shape>(shape), path);
    // The premise: the body as the model makes it has tangent edges that
    // were drawn as sharp ones.
    CHECK(counts.raw > counts.raw_seams);
  });
  if (test::failures == 0) {
    std::puts("test_drawing: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}

} // namespace

int main(int argc, char* argv[]) {
  if (argc == 3 && std::string(argv[1]) == "--tangent-edges") {
    return check_brep_file(argv[2]);
  }
  test::guarded("three views", test_three_views);
  test::guarded("a section view", test_section_view);
  test::guarded("round holes", test_round_holes);
  test::guarded("lines and arcs from segments", test_fit_segments);
  test::guarded("a line into an arc", test_fit_a_line_into_an_arc);
  test::guarded("the quick path", test_quick_path);
  test::guarded("the quick path for curved edges", test_quick_path_for_curved_edges);
  test::guarded("the quick path at the view's scale", test_quick_path_at_the_view_scale);
  test::guarded("the tangent edges of a filleted block", test_tangent_edges_of_a_filleted_block);
  if (test::failures == 0) {
    std::puts("test_drawing: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
