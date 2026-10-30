// SPDX-License-Identifier: MIT
#include "mitcad/geometry/drawing.hpp"

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <map>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_ApproxCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <HLRBRep_PolyAlgo.hxx>
#include <HLRBRep_PolyHLRToShape.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Vec.hxx>

#include "mitcad/analysis/section.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;
constexpr double kTurn = 2.0 * kPi;

// --- The trace (mitcad#962) ----------------------------------------------------

// MITCAD_DRAWING_TRACE: a "drawing-trace:" line per projected view on
// standard error, with the parts' faces and edges and the milliseconds of
// each phase. Its value may also ask for more work, done only to measure:
// "compare" times the other algorithm on the same parts (its lines thrown
// away), "parts" projects each part alone with the exact algorithm.
struct TraceMode {
  bool on = false;
  bool compare = false;
  bool parts = false;
};

const TraceMode& trace_mode() {
  static const TraceMode mode = [] {
    TraceMode m;
    std::string text;
#ifdef _MSC_VER
    char* value = nullptr;
    std::size_t length = 0;
    if (_dupenv_s(&value, &length, "MITCAD_DRAWING_TRACE") != 0 || value == nullptr) {
      return m;
    }
    text = value;
    std::free(value);
#else
    const char* value = std::getenv("MITCAD_DRAWING_TRACE");
    if (value == nullptr) {
      return m;
    }
    text = value;
#endif
    m.on = true;
    m.compare = text.find("compare") != std::string::npos;
    m.parts = text.find("parts") != std::string::npos;
    return m;
  }();
  return mode;
}

using Clock = std::chrono::steady_clock;

std::string milliseconds(Clock::duration time) {
  char text[32];
  std::snprintf(text, sizeof text, "%.1f", std::chrono::duration<double, std::milli>(time).count());
  return text;
}

// The phases of one projection, printed when it finishes (trace_mode()).
class ViewTrace {
public:
  explicit ViewTrace(std::string what)
      : m_on(trace_mode().on), m_what(std::move(what)), m_start(Clock::now()), m_last(m_start) {}
  ~ViewTrace() {
    try {
      finish();
    } catch (...) {
      // A trace line that could not be printed.
    }
  }
  ViewTrace(const ViewTrace&) = delete;
  ViewTrace& operator=(const ViewTrace&) = delete;

  bool on() const { return m_on; }
  void mark(const char* phase) {
    if (!m_on) {
      return;
    }
    const Clock::time_point now = Clock::now();
    m_line += std::string(" ") + phase + "=" + milliseconds(now - m_last);
    m_last = now;
  }
  void note(const std::string& text) {
    if (m_on) {
      m_line += " " + text;
    }
  }
  // Prints the line (once).
  void finish() {
    if (!m_on) {
      return;
    }
    m_on = false;
    std::fprintf(stderr, "drawing-trace: %s total=%s ms%s\n", m_what.c_str(),
                 milliseconds(Clock::now() - m_start).c_str(), m_line.c_str());
    std::fflush(stderr);
  }

private:
  bool m_on;
  std::string m_what;
  std::string m_line;
  Clock::time_point m_start;
  Clock::time_point m_last;
};

// The parts' faces (planes, other analytic surfaces and free-form ones:
// B-splines, sweeps, offsets) and edges, for the trace.
std::string shape_counts(const std::vector<TopoDS_Shape>& parts) {
  int planes = 0;
  int analytic = 0;
  int freeform = 0;
  int edges = 0;
  for (const TopoDS_Shape& part : parts) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
    TopExp::MapShapes(part, TopAbs_FACE, faces);
    for (int i = 1; i <= faces.Extent(); ++i) {
      const BRepAdaptor_Surface surface(TopoDS::Face(faces(i)), false);
      switch (surface.GetType()) {
      case GeomAbs_Plane:
        ++planes;
        break;
      case GeomAbs_Cylinder:
      case GeomAbs_Cone:
      case GeomAbs_Sphere:
      case GeomAbs_Torus:
        ++analytic;
        break;
      default:
        ++freeform;
        break;
      }
    }
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edge_map;
    TopExp::MapShapes(part, TopAbs_EDGE, edge_map);
    edges += edge_map.Extent();
  }
  return "parts=" + std::to_string(parts.size()) + " faces=" + std::to_string(planes + analytic + freeform) +
         " (planes=" + std::to_string(planes) + " analytic=" + std::to_string(analytic) +
         " freeform=" + std::to_string(freeform) + ") edges=" + std::to_string(edges);
}

// The view's 2D coordinates of a model point.
struct ViewFrame {
  gp_Dir x;
  gp_Dir y;

  Point2 project(const gp_Pnt& p) const {
    const gp_Vec v(p.XYZ());
    return {v.Dot(gp_Vec(x)), v.Dot(gp_Vec(y))};
  }
};

analysis::Plane analysis_plane(const DrawingSection& section) {
  analysis::Plane plane;
  plane.origin = {section.origin.X(), section.origin.Y(), section.origin.Z()};
  plane.normal = {section.normal.X(), section.normal.Y(), section.normal.Z()};
  return plane;
}

double normalized_angle(double a) {
  a = std::fmod(a, kTurn);
  if (a < 0.0) {
    a += kTurn;
  }
  if (a > kTurn - 1e-12) {
    a = 0.0;
  }
  return a;
}

// Points along a curve within the deflection, in its own direction.
std::vector<gp_Pnt> sample(const BRepAdaptor_Curve& curve, double first, double last, double deflection) {
  std::vector<gp_Pnt> points;
  const GCPnts_TangentialDeflection sampler(curve, first, last, 0.1, deflection, 2);
  for (int i = 1; i <= sampler.NbPoints(); ++i) {
    points.push_back(sampler.Value(i));
  }
  if (points.size() < 2) {
    points = {curve.Value(first), curve.Value(last)};
  }
  return points;
}

// A projected curve other than a line or a circle as a B-spline (mitcad#853):
// a B-spline or a Bézier curve as itself, cut to the edge's range, any other
// (an ellipse, an offset curve, ...) approximated within 1e-6 mm. Null when
// it cannot be.
occ::handle<Geom_BSplineCurve> as_bspline(const BRepAdaptor_Curve& curve, double first, double last) {
  try {
    occ::handle<Geom_BSplineCurve> spline;
    if (curve.GetType() == GeomAbs_BSplineCurve) {
      spline = occ::handle<Geom_BSplineCurve>::DownCast(curve.BSpline()->Copy());
    } else if (curve.GetType() == GeomAbs_BezierCurve) {
      spline = GeomConvert::CurveToBSplineCurve(curve.Bezier());
    } else {
      GeomConvert_ApproxCurve approx(curve.Trim(first, last, 1e-9), 1e-6, GeomAbs_C2, 64, 9);
      return approx.HasResult() ? approx.Curve() : occ::handle<Geom_BSplineCurve>();
    }
    if (spline.IsNull()) {
      return spline;
    }
    if (spline->IsPeriodic()) {
      spline->SetNotPeriodic();
    }
    if (first > spline->FirstParameter() + 1e-12 || last < spline->LastParameter() - 1e-12) {
      spline->Segment(first, last);
    }
    return spline;
  } catch (const Standard_Failure&) {
    return occ::handle<Geom_BSplineCurve>();
  }
}

// One edge of a projection: its curve lies in the view's plane (x, y, 0).
void add_projected(const TopoDS_Edge& edge, bool visible, DrawnKind kind, double deflection, DrawnView& view) {
  if (BRep_Tool::Degenerated(edge)) {
    return;
  }
  BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  if (!(last > first)) {
    return;
  }
  DrawnCurve drawn;
  drawn.visible = visible;
  drawn.kind = kind;
  const auto flat = [](const gp_Pnt& p) { return Point2{p.X(), p.Y()}; };
  switch (curve.GetType()) {
  case GeomAbs_Line: {
    drawn.type = DrawnCurve::Type::Line;
    drawn.points = {flat(curve.Value(first)), flat(curve.Value(last))};
    if (std::hypot(drawn.points[1][0] - drawn.points[0][0], drawn.points[1][1] - drawn.points[0][1]) <
        Precision::Confusion()) {
      return;
    }
    break;
  }
  case GeomAbs_Circle: {
    const gp_Circ circle = curve.Circle();
    drawn.type = DrawnCurve::Type::Arc;
    drawn.center = flat(circle.Location());
    drawn.radius = circle.Radius();
    const double sweep = last - first;
    if (sweep >= kTurn - 1e-9) {
      drawn.start = 0.0;
      drawn.end = kTurn;
      break;
    }
    const gp_Pnt a = curve.Value(first);
    const gp_Pnt b = curve.Value(last);
    // The parameter runs counter-clockwise in the view when the circle's
    // axis points to the eye (+Z of the view's plane).
    const gp_Pnt from = circle.Axis().Direction().Z() > 0.0 ? a : b;
    drawn.start = normalized_angle(std::atan2(from.Y() - drawn.center[1], from.X() - drawn.center[0]));
    drawn.end = drawn.start + sweep;
    break;
  }
  default: {
    drawn.type = DrawnCurve::Type::Polyline;
    for (const gp_Pnt& p : sample(curve, first, last, deflection)) {
      drawn.points.push_back(flat(p));
    }
    // The curve itself too, as a B-spline (mitcad#853): cut sheets write
    // it as it is.
    const occ::handle<Geom_BSplineCurve> spline = as_bspline(curve, first, last);
    if (!spline.IsNull()) {
      drawn.type = DrawnCurve::Type::Spline;
      drawn.degree = spline->Degree();
      for (int i = 1; i <= spline->NbPoles(); ++i) {
        drawn.poles.push_back(flat(spline->Pole(i)));
      }
      if (spline->IsRational()) {
        for (int i = 1; i <= spline->NbPoles(); ++i) {
          drawn.weights.push_back(spline->Weight(i));
        }
      }
      const auto& knots = spline->KnotSequence();
      for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
        drawn.knots.push_back(knots(i));
      }
    }
    break;
  }
  }
  view.curves.push_back(std::move(drawn));
}

void add_compound(const TopoDS_Shape& compound, bool visible, DrawnKind kind, double deflection, DrawnView& view) {
  if (compound.IsNull()) {
    return;
  }
  for (TopExp_Explorer it(compound, TopAbs_EDGE); it.More(); it.Next()) {
    add_projected(TopoDS::Edge(it.Current()), visible, kind, deflection, view);
  }
}

// --- The polygonal algorithm's lines as lines and arcs (mitcad#962) -----------

using Segment = std::array<Point2, 2>;

double cross2(const Point2& a, const Point2& b) { return a[0] * b[1] - a[1] * b[0]; }
double dot2(const Point2& a, const Point2& b) { return a[0] * b[0] + a[1] * b[1]; }
Point2 minus2(const Point2& a, const Point2& b) { return {a[0] - b[0], a[1] - b[1]}; }

// Whether the points from `from` to `to` lie within `tolerance` of the
// segment between the two, in order along it.
bool on_line(const std::vector<Point2>& points, std::size_t from, std::size_t to, double tolerance) {
  const Point2 d = minus2(points[to], points[from]);
  const double length = std::hypot(d[0], d[1]);
  if (length < 1e-12) {
    return false;
  }
  const Point2 u{d[0] / length, d[1] / length};
  double before = 0.0;
  for (std::size_t k = from + 1; k < to; ++k) {
    const Point2 v = minus2(points[k], points[from]);
    const double along = dot2(v, u);
    if (std::abs(cross2(u, v)) > tolerance || along < before - tolerance || along > length + tolerance) {
      return false;
    }
    before = std::max(before, along);
  }
  return true;
}

struct Circle {
  Point2 center{0.0, 0.0};
  double radius = 0.0;
};

// The circle nearest the points from `from` to `to` (algebraic least
// squares: x² + y² + Dx + Ey + F = 0), unless they lie on a line or nearly.
bool fit_circle(const std::vector<Point2>& points, std::size_t from, std::size_t to, Circle& circle) {
  // About the first point, for precision.
  const Point2 o = points[from];
  double sxx = 0.0, sxy = 0.0, syy = 0.0, sx = 0.0, sy = 0.0, sxz = 0.0, syz = 0.0, sz = 0.0;
  double n = 0.0;
  for (std::size_t k = from; k <= to; ++k) {
    const double x = points[k][0] - o[0];
    const double y = points[k][1] - o[1];
    const double z = x * x + y * y;
    sxx += x * x;
    sxy += x * y;
    syy += y * y;
    sx += x;
    sy += y;
    sxz += x * z;
    syz += y * z;
    sz += z;
    n += 1.0;
  }
  // [sxx sxy sx; sxy syy sy; sx sy n] (D, E, F) = -(sxz, syz, sz), by
  // Cramer's rule.
  const auto det3 = [](double a, double b, double c, double d, double e, double f, double g, double h, double i) {
    return a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
  };
  const double det = det3(sxx, sxy, sx, sxy, syy, sy, sx, sy, n);
  if (!(std::abs(det) > 0.0) || !std::isfinite(det)) {
    return false;
  }
  const double d = det3(-sxz, sxy, sx, -syz, syy, sy, -sz, sy, n) / det;
  const double e = det3(sxx, -sxz, sx, sxy, -syz, sy, sx, -sz, n) / det;
  const double f = det3(sxx, sxy, -sxz, sxy, syy, -syz, sx, sy, -sz) / det;
  const double r2 = (d * d + e * e) / 4.0 - f;
  if (!std::isfinite(r2) || r2 <= 0.0) {
    return false;
  }
  circle.center = {o[0] - d / 2.0, o[1] - e / 2.0};
  circle.radius = std::sqrt(r2);
  // A circle much larger than the points' span is a line.
  const Point2 span = minus2(points[to], points[from]);
  const double chord = std::hypot(span[0], span[1]);
  return std::isfinite(circle.radius) && (chord < 1e-12 || circle.radius < 50.0 * chord);
}

// Whether the points from `from` to `to` follow an arc of one circle within
// `tolerance`, turning one way, at most once round, each segment's sagitta
// within it too; `sweep` is the signed angle they turn through.
bool on_arc(const std::vector<Point2>& points, std::size_t from, std::size_t to, double tolerance, Circle& circle,
            double& sweep) {
  if (!fit_circle(points, from, to, circle)) {
    return false;
  }
  for (std::size_t k = from; k <= to; ++k) {
    const Point2 v = minus2(points[k], circle.center);
    if (std::abs(std::hypot(v[0], v[1]) - circle.radius) > tolerance) {
      return false;
    }
  }
  double total = 0.0;
  int turning = 0;
  for (std::size_t k = from; k < to; ++k) {
    const Point2 a = minus2(points[k], circle.center);
    const Point2 b = minus2(points[k + 1], circle.center);
    const double delta = std::atan2(cross2(a, b), dot2(a, b));
    const int sign = delta > 0.0 ? 1 : (delta < 0.0 ? -1 : 0);
    if (sign == 0 || (turning != 0 && sign != turning)) {
      return false;
    }
    turning = sign;
    if (circle.radius * (1.0 - std::cos(delta / 2.0)) > tolerance) {
      return false;
    }
    total += delta;
  }
  if (std::abs(total) > kTurn + 1e-9) {
    return false;
  }
  sweep = total;
  return true;
}

// Chains of segments joined end to end: through the points where exactly
// two of them meet; a closed one ends at its first point.
struct Chain {
  std::vector<Point2> points;
  bool closed = false;
};

std::vector<Chain> chains_of(const std::vector<Segment>& segments) {
  using Key = std::pair<long long, long long>;
  const auto key_of = [](const Point2& p) { return Key{std::llround(p[0] * 1e7), std::llround(p[1] * 1e7)}; };
  std::map<Key, std::size_t> ids;
  std::vector<Point2> nodes;
  std::vector<std::vector<std::size_t>> at; // the segments at each node
  std::vector<std::array<std::size_t, 2>> ends;
  std::set<std::pair<std::size_t, std::size_t>> seen;
  const auto node_of = [&](const Point2& p) {
    const auto found = ids.find(key_of(p));
    if (found != ids.end()) {
      return found->second;
    }
    ids.emplace(key_of(p), nodes.size());
    nodes.push_back(p);
    at.emplace_back();
    return nodes.size() - 1;
  };
  for (const Segment& segment : segments) {
    const std::size_t a = node_of(segment[0]);
    const std::size_t b = node_of(segment[1]);
    // Points and segments given twice once.
    if (a == b || !seen.insert({std::min(a, b), std::max(a, b)}).second) {
      continue;
    }
    at[a].push_back(ends.size());
    at[b].push_back(ends.size());
    ends.push_back({a, b});
  }
  std::vector<bool> used(ends.size(), false);
  const auto walk = [&](std::size_t start, std::size_t first) {
    Chain chain;
    chain.points.push_back(nodes[start]);
    std::size_t node = start;
    std::size_t segment = first;
    for (;;) {
      used[segment] = true;
      node = ends[segment][0] == node ? ends[segment][1] : ends[segment][0];
      chain.points.push_back(nodes[node]);
      if (at[node].size() != 2) {
        break;
      }
      const std::size_t next = at[node][0] == segment ? at[node][1] : at[node][0];
      if (used[next]) {
        break;
      }
      segment = next;
    }
    chain.closed = node == start && chain.points.size() > 3;
    return chain;
  };
  std::vector<Chain> chains;
  for (std::size_t n = 0; n < nodes.size(); ++n) {
    if (at[n].size() == 2) {
      continue;
    }
    for (const std::size_t s : at[n]) {
      if (!used[s]) {
        chains.push_back(walk(n, s));
      }
    }
  }
  // The rest are loops.
  for (std::size_t s = 0; s < ends.size(); ++s) {
    if (!used[s]) {
      chains.push_back(walk(ends[s][0], s));
    }
  }
  return chains;
}

// A loop from its sharpest corner, so that a line or an arc is not cut
// where the loop happens to start.
void from_sharpest_corner(std::vector<Point2>& loop) {
  const std::size_t n = loop.size() - 1; // the last point is the first
  std::size_t best = 0;
  double sharpest = -1.0;
  for (std::size_t k = 0; k < n; ++k) {
    const Point2 in = minus2(loop[k], loop[(k + n - 1) % n]);
    const Point2 out = minus2(loop[(k + 1) % n], loop[k]);
    const double turn = std::abs(std::atan2(cross2(in, out), dot2(in, out)));
    if (turn > sharpest) {
      sharpest = turn;
      best = k;
    }
  }
  const auto corner = loop.begin() + static_cast<std::ptrdiff_t>(best);
  std::vector<Point2> rotated(corner, loop.begin() + static_cast<std::ptrdiff_t>(n));
  rotated.insert(rotated.end(), loop.begin(), corner);
  rotated.push_back(rotated.front());
  loop = std::move(rotated);
}

// A chain as lines, arcs and polylines (fit_segments).
void add_fitted(Chain chain, bool visible, DrawnKind kind, double tolerance, std::vector<DrawnCurve>& out) {
  std::vector<Point2>& points = chain.points;
  const auto curve = [&](DrawnCurve::Type type) {
    DrawnCurve c;
    c.visible = visible;
    c.kind = kind;
    c.type = type;
    return c;
  };
  const auto add_arc = [&](const Circle& circle, double sweep, const Point2& first, const Point2& last) {
    DrawnCurve c = curve(DrawnCurve::Type::Arc);
    c.center = circle.center;
    c.radius = circle.radius;
    if (std::abs(sweep) > kTurn - 1e-6) {
      c.start = 0.0;
      c.end = kTurn;
    } else {
      // Counter-clockwise: from the first point when the chain turns that
      // way, else from the last.
      const Point2 from = minus2(sweep > 0.0 ? first : last, circle.center);
      c.start = normalized_angle(std::atan2(from[1], from[0]));
      c.end = c.start + std::abs(sweep);
    }
    out.push_back(std::move(c));
  };
  if (points.size() < 2) {
    return;
  }
  if (chain.closed) {
    Circle circle;
    double sweep = 0.0;
    if (points.size() > 4 && on_arc(points, 0, points.size() - 1, tolerance, circle, sweep) &&
        std::abs(sweep) > kTurn - 1e-6) {
      add_arc(circle, sweep, points.front(), points.back());
      return;
    }
    from_sharpest_corner(points);
  }
  // Single segments, drawn as polylines broken at corners sharper than the
  // triangulation turns between its chords (a polygon's sides are lines).
  std::vector<Point2> loose;
  const auto piece = [&](std::size_t from, std::size_t to) {
    if (to == from + 1) {
      DrawnCurve c = curve(DrawnCurve::Type::Line);
      c.points = {loose[from], loose[to]};
      out.push_back(std::move(c));
    } else {
      DrawnCurve c = curve(DrawnCurve::Type::Polyline);
      c.points.assign(loose.begin() + static_cast<std::ptrdiff_t>(from),
                      loose.begin() + static_cast<std::ptrdiff_t>(to) + 1);
      out.push_back(std::move(c));
    }
  };
  const auto flush = [&] {
    constexpr double kCorner = 0.6; // radians
    std::size_t start = 0;
    for (std::size_t k = 1; k < loose.size(); ++k) {
      bool corner = k + 1 == loose.size();
      if (!corner) {
        const Point2 in = minus2(loose[k], loose[k - 1]);
        const Point2 onward = minus2(loose[k + 1], loose[k]);
        corner = std::abs(std::atan2(cross2(in, onward), dot2(in, onward))) > kCorner;
      }
      if (corner) {
        piece(start, k);
        start = k;
      }
    }
    loose.clear();
  };
  const std::size_t last = points.size() - 1;
  std::size_t i = 0;
  while (i < last) {
    std::size_t line_end = i + 1;
    while (line_end < last && on_line(points, i, line_end + 1, tolerance)) {
      ++line_end;
    }
    // An arc of three segments at least.
    std::size_t arc_end = i;
    Circle circle;
    double sweep = 0.0;
    for (std::size_t j = i + 3; j <= last; ++j) {
      Circle longer;
      double turned = 0.0;
      if (!on_arc(points, i, j, tolerance, longer, turned)) {
        break;
      }
      arc_end = j;
      circle = longer;
      sweep = turned;
    }
    if (arc_end > line_end) {
      flush();
      add_arc(circle, sweep, points[i], points[arc_end]);
      i = arc_end;
    } else if (line_end > i + 1) {
      flush();
      DrawnCurve c = curve(DrawnCurve::Type::Line);
      c.points = {points[i], points[line_end]};
      out.push_back(std::move(c));
      i = line_end;
    } else {
      if (loose.empty()) {
        loose.push_back(points[i]);
      }
      loose.push_back(points[i + 1]);
      ++i;
    }
  }
  flush();
}

std::vector<DrawnCurve> fitted(const std::vector<Segment>& segments, bool visible, DrawnKind kind, double tolerance) {
  std::vector<DrawnCurve> out;
  for (Chain& chain : chains_of(segments)) {
    add_fitted(std::move(chain), visible, kind, tolerance, out);
  }
  return out;
}

// A polygonal algorithm's compound: its segments fitted, any other edge as
// it is.
void add_polygonal(const TopoDS_Shape& compound, bool visible, DrawnKind kind, double deflection, double tolerance,
                   DrawnView& view) {
  if (compound.IsNull()) {
    return;
  }
  std::vector<Segment> segments;
  for (TopExp_Explorer it(compound, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() != GeomAbs_Line) {
      add_projected(edge, visible, kind, deflection, view);
      continue;
    }
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    segments.push_back({Point2{a.X(), a.Y()}, Point2{b.X(), b.Y()}});
  }
  for (DrawnCurve& c : fitted(segments, visible, kind, tolerance)) {
    view.curves.push_back(std::move(c));
  }
}

// The deviation the quick path's triangulation may have: a 2000th of the
// shapes' size (the diagonal of their box), 0.06 mm on a 120 mm assembly,
// a tenth of a thin line at the scale a sheet shows such a view; at least
// the view's deflection.
double quick_deflection(const std::vector<TopoDS_Shape>& parts, double deflection) {
  Bnd_Box box;
  for (const TopoDS_Shape& part : parts) {
    BRepBndLib::Add(part, box);
  }
  const double size = box.IsVoid() ? 1.0 : std::sqrt(box.SquareExtent());
  return std::max(deflection, size * 5e-4);
}

// Whether a face is free-form: not a plane, a cylinder, a cone, a sphere or
// a torus. The exact algorithm is slow on such faces (mitcad#962: a modelled
// thread's took seconds alone).
bool has_free_form_faces(const std::vector<TopoDS_Shape>& parts) {
  for (const TopoDS_Shape& part : parts) {
    for (TopExp_Explorer it(part, TopAbs_FACE); it.More(); it.Next()) {
      const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()), false);
      switch (surface.GetType()) {
      case GeomAbs_Plane:
      case GeomAbs_Cylinder:
      case GeomAbs_Cone:
      case GeomAbs_Sphere:
      case GeomAbs_Torus:
        break;
      default:
        return true;
      }
    }
  }
  return false;
}

// The algorithm's lines (its compounds, made when they are asked for:
// "extract" in the trace), then each as a drawn curve ("convert"): fitted
// (add_polygonal) when `tolerance` is given.
template <class Output>
void collect(Output& out, const DrawingViewSpec& spec, DrawnView& view, ViewTrace& trace, double tolerance = 0.0) {
  struct Lines {
    TopoDS_Shape compound;
    bool visible;
    DrawnKind kind;
  };
  std::vector<Lines> lines;
  lines.push_back({out.VCompound(), true, DrawnKind::Sharp});
  lines.push_back({out.OutLineVCompound(), true, DrawnKind::Outline});
  if (spec.smooth) {
    lines.push_back({out.Rg1LineVCompound(), true, DrawnKind::Smooth});
  }
  if (spec.hidden) {
    lines.push_back({out.HCompound(), false, DrawnKind::Sharp});
    lines.push_back({out.OutLineHCompound(), false, DrawnKind::Outline});
    if (spec.smooth) {
      lines.push_back({out.Rg1LineHCompound(), false, DrawnKind::Smooth});
    }
  }
  trace.mark("extract");
  for (const Lines& l : lines) {
    if (tolerance > 0.0) {
      add_polygonal(l.compound, l.visible, l.kind, spec.deflection, tolerance, view);
    } else {
      add_compound(l.compound, l.visible, l.kind, spec.deflection, view);
    }
  }
  trace.mark("convert");
  trace.note("lines=" + std::to_string(view.curves.size()));
}

void project_exact(const std::vector<TopoDS_Shape>& parts, const HLRAlgo_Projector& projector,
                   const DrawingViewSpec& spec, DrawnView& view, ViewTrace& trace) {
  occ::handle<HLRBRep_Algo> algo = new HLRBRep_Algo();
  for (const TopoDS_Shape& part : parts) {
    algo->Add(part);
  }
  algo->Projector(projector);
  trace.mark("load");
  algo->Update();
  trace.mark("update");
  algo->Hide();
  trace.mark("hide");
  HLRBRep_HLRToShape out(algo);
  collect(out, spec, view, trace);
}

// The polygonal algorithm works on triangles: of copies of the shapes, so
// that the shapes themselves stay as they are. The triangulation deviates
// at most quick_deflection from the surfaces (their faces meshed in
// parallel), and its lines are fitted within one and a half times that
// (mitcad#962).
void project_polygonal(const std::vector<TopoDS_Shape>& parts, const HLRAlgo_Projector& projector,
                       const DrawingViewSpec& spec, DrawnView& view, ViewTrace& trace) {
  const double deflection = quick_deflection(parts, spec.deflection);
  occ::handle<HLRBRep_PolyAlgo> algo = new HLRBRep_PolyAlgo();
  for (const TopoDS_Shape& part : parts) {
    const bool mesh = detail::has_mesh_faces(part);
    TopoDS_Shape copy = BRepBuilderAPI_Copy(part, true, mesh).Shape();
    if (!mesh) {
      BRepMesh_IncrementalMesh mesher(copy, deflection, false, 0.2, true);
    }
    algo->Load(copy);
  }
  trace.mark("mesh");
  algo->Projector(projector);
  algo->Update();
  trace.mark("update");
  HLRBRep_PolyHLRToShape out;
  out.Update(algo);
  collect(out, spec, view, trace, 1.5 * deflection);
}

// MITCAD_DRAWING_TRACE=compare: the time of the algorithm the view did not
// use, on the same parts; its lines are thrown away.
void trace_other(bool exact_used, const std::vector<TopoDS_Shape>& parts, const HLRAlgo_Projector& projector,
                 const DrawingViewSpec& spec) {
  ViewTrace trace(exact_used ? "compare polygonal" : "compare exact");
  DrawnView scratch;
  try {
    if (exact_used) {
      project_polygonal(parts, projector, spec, scratch, trace);
    } else {
      project_exact(parts, projector, spec, scratch, trace);
    }
  } catch (const Standard_Failure&) {
    trace.note("failed");
  } catch (const std::exception&) {
    trace.note("failed");
  }
}

// MITCAD_DRAWING_TRACE=parts: each part alone with the exact algorithm, its
// index the shape's among those the view was given.
void trace_parts(const std::vector<TopoDS_Shape>& parts, const std::vector<std::size_t>& indices,
                 const HLRAlgo_Projector& projector, const DrawingViewSpec& spec) {
  for (std::size_t i = 0; i < parts.size(); ++i) {
    const std::vector<TopoDS_Shape> one{parts[i]};
    ViewTrace trace("part " + std::to_string(indices[i]));
    trace.note(shape_counts(one));
    trace.mark("count");
    DrawnView scratch;
    try {
      project_exact(one, projector, spec, scratch, trace);
    } catch (const Standard_Failure&) {
      trace.note("failed");
    } catch (const std::exception&) {
      trace.note("failed");
    }
  }
}

// The loops of the faces where the plane cuts the shape, in view
// coordinates.
void add_cuts(const TopoDS_Shape& shape, const DrawingSection& section, const ViewFrame& frame,
              double deflection, DrawnView& view) {
  const analysis::Section cut = analysis::section(shape, analysis_plane(section));
  if (cut.faces.IsNull()) {
    return;
  }
  for (TopExp_Explorer faces(cut.faces, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face face = TopoDS::Face(faces.Current());
    CutFace loops;
    for (TopExp_Explorer wires(face, TopAbs_WIRE); wires.More(); wires.Next()) {
      std::vector<Point2> loop;
      for (BRepTools_WireExplorer edges(TopoDS::Wire(wires.Current()), face); edges.More(); edges.Next()) {
        const TopoDS_Edge& edge = edges.Current();
        if (BRep_Tool::Degenerated(edge)) {
          continue;
        }
        const BRepAdaptor_Curve curve(edge);
        std::vector<gp_Pnt> points = sample(curve, curve.FirstParameter(), curve.LastParameter(), deflection);
        if (edge.Orientation() == TopAbs_REVERSED) {
          std::reverse(points.begin(), points.end());
        }
        for (const gp_Pnt& p : points) {
          const Point2 q = frame.project(p);
          if (loop.empty() || std::hypot(q[0] - loop.back()[0], q[1] - loop.back()[1]) > 1e-9) {
            loop.push_back(q);
          }
        }
      }
      if (loop.size() > 2 &&
          std::hypot(loop.front()[0] - loop.back()[0], loop.front()[1] - loop.back()[1]) <= 1e-9) {
        loop.pop_back();
      }
      if (loop.size() > 2) {
        loops.push_back(std::move(loop));
      }
    }
    if (!loops.empty()) {
      view.cuts.push_back(std::move(loops));
    }
  }
}

// --- Round holes -----------------------------------------------------------

constexpr double kAxisTolerance = 1e-6;

// An axis line in a canonical form: its direction's first non-zero
// component of (z, y, x) positive, through the foot of the origin.
struct Line {
  gp_Dir direction;
  gp_Pnt foot;

  static Line of(const gp_Ax1& axis) {
    gp_Dir d = axis.Direction();
    const double key = std::abs(d.Z()) > 1e-9 ? d.Z() : (std::abs(d.Y()) > 1e-9 ? d.Y() : d.X());
    if (key < 0.0) {
      d.Reverse();
    }
    const gp_Vec v(axis.Location().XYZ());
    const gp_Pnt foot(axis.Location().XYZ() - d.XYZ() * v.Dot(gp_Vec(d)));
    return {d, foot};
  }

  bool same(const Line& other) const {
    return direction.IsParallel(other.direction, 1e-9) && foot.Distance(other.foot) < kAxisTolerance;
  }
  double at(const gp_Pnt& p) const { return gp_Vec(foot, p).Dot(gp_Vec(direction)); }
  gp_Pnt point(double t) const { return foot.Translated(gp_Vec(direction) * t); }
  double distance(const gp_Pnt& p) const {
    const gp_Pnt on = point(at(p));
    return on.Distance(p);
  }
};

// A coaxial cylinder or cone with the material outside it: the faces of one
// surface over one range along the axis.
struct Wall {
  bool cone = false;
  double low = 0.0;  // along the line
  double high = 0.0;
  double radius_low = 0.0;
  double radius_high = 0.0;
  double span = 0.0; // the angle the faces cover together
  double half_angle = 0.0; // a cone's
  int face = -1;     // the first face
};

struct Axis {
  Line line;
  std::vector<Wall> walls;
};

// Whether the material lies outside the surface at the face's middle.
bool material_outside(const TopoDS_Face& face, const Line& line) {
  const BRepAdaptor_Surface surface(face);
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  gp_Pnt point;
  gp_Vec du;
  gp_Vec dv;
  surface.D1((u0 + u1) / 2.0, (v0 + v1) / 2.0, point, du, dv);
  const gp_Vec cross = du.Crossed(dv);
  if (cross.Magnitude() < Precision::Confusion()) {
    return false;
  }
  gp_Dir normal(cross);
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  const gp_Vec radial(line.point(line.at(point)), point);
  return radial.Dot(gp_Vec(normal)) < 0.0;
}

std::vector<Axis> hole_walls(const Shape& shape) {
  std::vector<Axis> axes;
  for (int i = 0; i < shape.face_count(); ++i) {
    const TopoDS_Face& face = shape.face(i);
    const BRepAdaptor_Surface surface(face);
    const GeomAbs_SurfaceType type = surface.GetType();
    if (type != GeomAbs_Cylinder && type != GeomAbs_Cone) {
      continue;
    }
    const gp_Ax1 axis = type == GeomAbs_Cylinder ? surface.Cylinder().Axis() : surface.Cone().Axis();
    const Line line = Line::of(axis);
    if (!material_outside(face, line)) {
      continue;
    }
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    Wall wall;
    wall.cone = type == GeomAbs_Cone;
    wall.span = u1 - u0;
    wall.face = i;
    const gp_Pnt a = surface.Value(u0, v0);
    const gp_Pnt b = surface.Value(u0, v1);
    double ta = line.at(a);
    double tb = line.at(b);
    double ra = line.distance(a);
    double rb = line.distance(b);
    if (ta > tb) {
      std::swap(ta, tb);
      std::swap(ra, rb);
    }
    wall.low = ta;
    wall.high = tb;
    wall.radius_low = ra;
    wall.radius_high = rb;
    if (wall.cone) {
      wall.half_angle = std::abs(surface.Cone().SemiAngle());
    }
    auto found = std::find_if(axes.begin(), axes.end(), [&](const Axis& other) { return other.line.same(line); });
    if (found == axes.end()) {
      axes.push_back(Axis{line, {}});
      found = axes.end() - 1;
    }
    // The pieces of a split surface over the same range are one wall.
    auto same = std::find_if(found->walls.begin(), found->walls.end(), [&](const Wall& other) {
      return other.cone == wall.cone && std::abs(other.low - wall.low) < kAxisTolerance &&
             std::abs(other.high - wall.high) < kAxisTolerance &&
             std::abs(other.radius_low - wall.radius_low) < kAxisTolerance &&
             std::abs(other.radius_high - wall.radius_high) < kAxisTolerance;
    });
    if (same != found->walls.end()) {
      same->span += wall.span;
    } else {
      found->walls.push_back(wall);
    }
  }
  for (Axis& axis : axes) {
    axis.walls.erase(std::remove_if(axis.walls.begin(), axis.walls.end(),
                                    [](const Wall& w) { return w.span < kTurn - 1e-6; }),
                     axis.walls.end());
  }
  return axes;
}

// Whether material lies on the line within `reach` beyond `end` along
// `outward`.
bool closed_beyond(const std::vector<TopoDS_Shape>& solids, const gp_Pnt& end, const gp_Dir& outward, double reach) {
  constexpr int kSteps = 16;
  for (int k = 1; k <= kSteps; ++k) {
    const gp_Pnt p = end.Translated(gp_Vec(outward) * (reach * k / kSteps));
    for (const TopoDS_Shape& solid : solids) {
      BRepClass3d_SolidClassifier classifier(solid, p, Precision::Confusion());
      if (classifier.State() == TopAbs_IN) {
        return true;
      }
    }
  }
  return false;
}

// What lies on one side of a drilled wall: a counterbore or a countersink
// meeting it end to end, and where the hole then ends.
struct Side {
  const Wall* wider = nullptr;
  double end = 0.0; // along the line
  bool closed = false;
};

Side side_of(const Axis& axis, const Wall& drill, bool high, const std::vector<bool>& used,
             const std::vector<TopoDS_Shape>& solids) {
  Side side;
  const double at = high ? drill.high : drill.low;
  const double radius = high ? drill.radius_high : drill.radius_low;
  side.end = at;
  for (std::size_t i = 0; i < axis.walls.size(); ++i) {
    const Wall& w = axis.walls[i];
    if (&w == &drill || used[i]) {
      continue;
    }
    const double meet = high ? w.low : w.high;
    const double meet_radius = high ? w.radius_low : w.radius_high;
    const double outer_radius = high ? w.radius_high : w.radius_low;
    if (std::abs(meet - at) > kAxisTolerance) {
      continue;
    }
    // A wider cylinder (a counterbore, its floor between them) or a cone
    // widening from the wall (a countersink).
    const bool wider = w.cone ? std::abs(meet_radius - radius) < 1e-4 && outer_radius > radius + 1e-6
                              : meet_radius > radius + 1e-6;
    if (wider) {
      side.wider = &w;
      side.end = high ? w.high : w.low;
      break;
    }
  }
  gp_Dir outward = axis.line.direction;
  if (!high) {
    outward.Reverse();
  }
  side.closed = closed_beyond(solids, axis.line.point(side.end), outward, 4.0 * radius);
  return side;
}

} // namespace

DrawnView project_view(const std::vector<ShapePtr>& shapes, const DrawingViewSpec& spec) {
  return detail::run("drawing view", [&] {
    if (std::abs(spec.toward_eye.Dot(spec.x_axis)) > 1e-9) {
      throw std::invalid_argument("a view's x axis must be at right angles to its direction");
    }
    const gp_Ax2 axes(gp::Origin(), spec.toward_eye, spec.x_axis);
    const ViewFrame frame{axes.XDirection(), axes.YDirection()};
    const HLRAlgo_Projector projector(axes);
    const TraceMode& mode = trace_mode();
    ViewTrace trace("view");
    if (trace.on()) {
      char eye[96];
      std::snprintf(eye, sizeof eye, "eye=(%.3f,%.3f,%.3f)", spec.toward_eye.X(), spec.toward_eye.Y(),
                    spec.toward_eye.Z());
      trace.note(std::string(eye) + " exact=" + (spec.exact ? "1" : "0") + " hidden=" + (spec.hidden ? "1" : "0") +
                 " smooth=" + (spec.smooth ? "1" : "0") + " section=" + (spec.section ? "1" : "0"));
    }
    DrawnView view;
    std::vector<TopoDS_Shape> parts;
    std::vector<std::size_t> indices; // of the parts among the shapes, for the trace
    bool meshes = false;
    for (std::size_t i = 0; i < shapes.size(); ++i) {
      const ShapePtr& shape = shapes[i];
      if (!shape) {
        continue;
      }
      TopoDS_Shape part = shape->occt();
      if (spec.section) {
        part = analysis::clip(part, analysis_plane(*spec.section));
        add_cuts(shape->occt(), *spec.section, frame, spec.deflection, view);
      }
      bool empty = true;
      for (TopExp_Explorer it(part, TopAbs_EDGE); it.More(); it.Next()) {
        empty = false;
        break;
      }
      if (part.IsNull() || empty) {
        continue;
      }
      meshes = meshes || detail::has_mesh_faces(part);
      parts.push_back(part);
      indices.push_back(i);
    }
    trace.mark("prepare");
    if (parts.empty()) {
      return view;
    }
    if (trace.on()) {
      trace.note(shape_counts(parts));
      trace.mark("count");
    }
    // The quick path where the exact algorithm is slow (mitcad#962).
    const bool quick = spec.exact && spec.quick && !meshes && has_free_form_faces(parts);
    if (spec.exact && !meshes && !quick) {
      DrawnView exact = view;
      try {
        project_exact(parts, projector, spec, exact, trace);
        trace.note("result=exact");
        trace.finish();
        if (mode.compare) {
          trace_other(true, parts, projector, spec);
        }
        if (mode.parts) {
          trace_parts(parts, indices, projector, spec);
        }
        return exact;
      } catch (const Standard_Failure&) {
        // The polygonal algorithm below.
        trace.mark("failed");
        trace.note("exact-failed");
      }
    }
    project_polygonal(parts, projector, spec, view, trace);
    view.polygonal = true;
    trace.note(quick ? "result=quick" : "result=polygonal");
    trace.finish();
    if (mode.compare && !meshes) {
      trace_other(false, parts, projector, spec);
    }
    if (mode.parts && !meshes) {
      trace_parts(parts, indices, projector, spec);
    }
    return view;
  });
}

std::vector<DrawnCurve> fit_segments(const std::vector<std::array<Point2, 2>>& segments, double tolerance) {
  return fitted(segments, true, DrawnKind::Sharp, tolerance);
}

std::vector<RoundHole> round_holes(const Shape& shape) {
  return detail::run("round holes", [&] {
    std::vector<TopoDS_Shape> solids;
    for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
      solids.push_back(it.Current());
    }
    std::vector<RoundHole> holes;
    if (solids.empty()) {
      return holes;
    }
    for (const Axis& axis : hole_walls(shape)) {
      // The narrowest cylinders first: each is a drilled wall unless a
      // narrower one took it as its counterbore.
      std::vector<std::size_t> order(axis.walls.size());
      for (std::size_t i = 0; i < order.size(); ++i) {
        order[i] = i;
      }
      std::sort(order.begin(), order.end(), [&](std::size_t a, std::size_t b) {
        return axis.walls[a].radius_low < axis.walls[b].radius_low;
      });
      std::vector<bool> used(axis.walls.size(), false);
      for (const std::size_t i : order) {
        const Wall& drill = axis.walls[i];
        if (used[i] || drill.cone) {
          continue;
        }
        const Side low = side_of(axis, drill, false, used, solids);
        const Side high = side_of(axis, drill, true, used, solids);
        if (low.closed && high.closed) {
          continue; // a cavity inside the body
        }
        used[i] = true;
        // The side it opens to: the open one; of a hole open at both ends
        // the side of its counterbore or countersink, else the high end
        // (towards +Z, +Y or +X).
        bool from_high = !high.closed;
        if (!low.closed && !high.closed) {
          from_high = high.wider != nullptr || low.wider == nullptr;
        }
        const Side& open = from_high ? high : low;
        const Side& other = from_high ? low : high;
        RoundHole hole;
        const NameList& names = shape.face_names(drill.face);
        hole.face = names.empty() ? std::string() : names.front();
        hole.opening = axis.line.point(open.end);
        hole.direction = from_high ? axis.line.direction.Reversed() : axis.line.direction;
        hole.diameter = 2.0 * drill.radius_low;
        hole.through = !other.closed;
        const double bottom = from_high ? drill.low : drill.high;
        hole.depth = std::abs(open.end - bottom);
        if (open.wider != nullptr) {
          const Wall& wide = *open.wider;
          used[static_cast<std::size_t>(&wide - axis.walls.data())] = true;
          if (wide.cone) {
            hole.csink_diameter = 2.0 * std::max(wide.radius_low, wide.radius_high);
            hole.csink_angle = 2.0 * wide.half_angle;
          } else {
            hole.cbore_diameter = 2.0 * wide.radius_low;
            hole.cbore_depth = wide.high - wide.low;
          }
        }
        holes.push_back(hole);
      }
    }
    std::sort(holes.begin(), holes.end(), [](const RoundHole& a, const RoundHole& b) {
      const auto key = [](const RoundHole& h) {
        return std::array<long long, 4>{std::llround(h.opening.X() * 1e6), std::llround(h.opening.Y() * 1e6),
                                        std::llround(h.opening.Z() * 1e6), std::llround(h.diameter * 1e6)};
      };
      return key(a) < key(b);
    });
    return holes;
  });
}

} // namespace mitcad::geometry
