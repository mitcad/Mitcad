// SPDX-License-Identifier: MIT
#include "mitcad/geometry/trimmed_box.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <exception>
#include <utility>
#include <vector>

#include <Adaptor3d_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BndLib_Add3dCurve.hxx>
#include <Bnd_Box.hxx>
#include <Geom2dAdaptor_Curve.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomAbs_CurveType.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <OSD_Parallel.hxx>
#include <Precision.hxx>
#include <TopAbs_State.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Elips.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

namespace mitcad::geometry {
namespace {

// A side of OCCT's box farther out than the boundary's by less than this
// is taken as the boundary's, with no search inside the face: a tenth of a
// nanometre, below every tolerance a measure or a test allows.
constexpr double kOpenSide = 1.0e-7;

// The fewest samples of the face's parameter grid that must lie inside the
// face before its surface is searched, and the most along each parameter
// the grid is refined to for a narrow face.
constexpr int kInsideSamples = 16;
constexpr int kMostSamples = 128;

// The most classifications per face handed to OCCT's classifier: points
// near the boundary's polygon in parameters when the polygon is built, and
// every point when it is not (a face whose edges' parameter curves do not
// close up). OCCT's classifier may test a point against every edge, which
// on faces of a hundred edges took seconds for the search (0.66.0-rc1).
constexpr int kMostNearClassifications = 256;
constexpr int kMostClassifications = 30000;

// The most surface evaluations of the climbs inside one face. A search cut
// short by it keeps OCCT's box on the sides it had not finished.
constexpr int kMostClimbSteps = 50000;

// The boundary's polygon in parameters follows each edge's parameter curve
// to within this share of the parameter rectangle's diagonal, with at most
// this many segments per edge.
constexpr double kPolygonDeflection = 1.0e-4;
constexpr int kMostEdgeSegments = 4096;

// Sides as Bnd_Box::Get gives them: xmin, ymin, zmin, xmax, ymax, zmax.
using Sides = std::array<double, 6>;

Sides sides_of(const Bnd_Box& box) {
  Sides s{};
  box.Get(s[0], s[1], s[2], s[3], s[4], s[5]);
  return s;
}

// Samples along a parameter: four per span of the surface where it is
// twice differentiable (a B-spline's knot spans), at least 16, at most 48.
int samples_for(int spans) { return std::clamp(4 * spans, 16, 48); }

// A coordinate of the surface (times +1 or -1, so that its extreme is a
// maximum) and its first and second derivatives in the parameters.
struct Local {
  double f = 0.0;
  double gu = 0.0;
  double gv = 0.0;
  double huu = 0.0;
  double huv = 0.0;
  double hvv = 0.0;
};

// The larger of the parameter resolutions of a distance on the surface.
double resolution(const BRepAdaptor_Surface& surface, double tolerance) {
  return std::max({surface.UResolution(tolerance), surface.VResolution(tolerance), Precision::PConfusion()});
}

// The face's boundary as a polygon in its surface's parameters: each
// edge's parameter curve split until its chords are within the deflection,
// each wire closed across the gaps its vertices' tolerances leave. A point
// farther from the polygon than its band is inside the face when a ray from
// it crosses the polygon an odd number of times, as OCCT's classifier
// would find; a point within the band is left to that classifier. Built
// once per face, it costs a few segments per point (the segments are kept
// in slabs along v) instead of OCCT's pass over every edge.
class BoundaryPolygon {
public:
  // Whether the point is inside (1), outside (0) or within the band (-1).
  int classify(double u, double v) const {
    if (v < m_vlow || v > m_vhigh) {
      return 0;
    }
    const auto slab = static_cast<std::size_t>(
        std::clamp(static_cast<int>((v - m_vlow) / m_slab), 0, static_cast<int>(m_first.size()) - 2));
    int crossings = 0;
    for (std::size_t k = m_first[slab]; k < m_first[slab + 1]; ++k) {
      const Segment& s = m_segments[m_index[k]];
      if (onSegment(s, u, v)) {
        return -1;
      }
      if ((s.v0 > v) != (s.v1 > v) && s.u0 + (v - s.v0) * (s.u1 - s.u0) / (s.v1 - s.v0) > u) {
        ++crossings;
      }
    }
    return crossings % 2;
  }

  // The polygon of `face` (forward) whose parameter rectangle has the
  // diagonal `diagonal`; false when an edge has no parameter curve, is
  // internal or external, or a wire does not close within `gap`.
  bool build(const TopoDS_Face& face, double diagonal, double gap, double tolerance) {
    const double deflection = kPolygonDeflection * diagonal;
    double deviation = 0.0;
    for (TopExp_Explorer wires(face, TopAbs_WIRE); wires.More(); wires.Next()) {
      std::vector<gp_Pnt2d> ends;
      for (TopExp_Explorer edges(wires.Current(), TopAbs_EDGE); edges.More(); edges.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
        if (edge.Orientation() != TopAbs_FORWARD && edge.Orientation() != TopAbs_REVERSED) {
          return false;
        }
        double first = 0.0;
        double last = 0.0;
        const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
        if (pcurve.IsNull() || !(last > first)) {
          return false;
        }
        const Geom2dAdaptor_Curve curve(pcurve, first, last);
        add_curve(curve, first, last, deflection, deviation);
        ends.push_back(curve.Value(first));
        ends.push_back(curve.Value(last));
      }
      if (!close(ends, gap)) {
        return false;
      }
    }
    if (m_segments.empty()) {
      return false;
    }
    // Twice the largest deviation measured, so that a curve between the
    // points measured stays within the band.
    m_band = 2.0 * deviation + gap + 2.0 * tolerance;
    m_band2 = m_band * m_band;
    double low = Precision::Infinite();
    double high = -Precision::Infinite();
    for (const Segment& s : m_segments) {
      low = std::min({low, s.v0, s.v1});
      high = std::max({high, s.v0, s.v1});
    }
    m_vlow = low - m_band;
    m_vhigh = high + m_band;
    slabs();
    return true;
  }

private:
  struct Segment {
    double u0;
    double v0;
    double u1;
    double v1;
  };

  bool onSegment(const Segment& s, double u, double v) const {
    const double du = s.u1 - s.u0;
    const double dv = s.v1 - s.v0;
    const double length2 = du * du + dv * dv;
    double t = length2 > 0.0 ? ((u - s.u0) * du + (v - s.v0) * dv) / length2 : 0.0;
    t = std::clamp(t, 0.0, 1.0);
    const double eu = s.u0 + t * du - u;
    const double ev = s.v0 + t * dv - v;
    return eu * eu + ev * ev <= m_band2;
  }

  static double off_chord(const gp_Pnt2d& p, const gp_Pnt2d& a, const gp_Pnt2d& b) {
    const gp_Vec2d chord(a, b);
    const double length = chord.Magnitude();
    if (!(length > 0.0)) {
      return p.Distance(a);
    }
    return std::abs(chord.Crossed(gp_Vec2d(a, p))) / length;
  }

  // The parameter curve over [first, last] as chords within `deflection`
  // where the edge's segment budget allows; `deviation` is the largest
  // distance measured between a chord and its curve.
  void add_curve(const Geom2dAdaptor_Curve& curve, double first, double last, double deflection, double& deviation) {
    int pieces = 8;
    switch (curve.GetType()) {
    case GeomAbs_Line:
      pieces = 1;
      break;
    case GeomAbs_BezierCurve:
    case GeomAbs_BSplineCurve:
      pieces = std::clamp(2 * curve.NbIntervals(GeomAbs_C2), 4, 256);
      break;
    default:
      break;
    }
    int budget = kMostEdgeSegments;
    gp_Pnt2d a = curve.Value(first);
    for (int k = 1; k <= pieces; ++k) {
      const double t = k == pieces ? last : first + (last - first) * k / pieces;
      const gp_Pnt2d b = curve.Value(t);
      split(curve, first + (last - first) * (k - 1) / pieces, a, t, b, deflection, 0, budget, deviation);
      a = b;
    }
  }

  void split(const Geom2dAdaptor_Curve& curve, double ta, const gp_Pnt2d& a, double tb, const gp_Pnt2d& b,
             double deflection, int depth, int& budget, double& deviation) {
    const double tm = 0.5 * (ta + tb);
    const gp_Pnt2d m = curve.Value(tm);
    const double off = std::max({off_chord(curve.Value(0.75 * ta + 0.25 * tb), a, b), off_chord(m, a, b),
                                 off_chord(curve.Value(0.25 * ta + 0.75 * tb), a, b)});
    if (off <= deflection || depth >= 16 || budget <= 1) {
      deviation = std::max(deviation, off);
      m_segments.push_back({a.X(), a.Y(), b.X(), b.Y()});
      --budget;
      return;
    }
    split(curve, ta, a, tm, m, deflection, depth + 1, budget, deviation);
    split(curve, tm, m, tb, b, deflection, depth + 1, budget, deviation);
  }

  // Pairs each end of the wire's edges with the nearest other free end
  // within `gap` and bridges the two: the wire as a closed polygon.
  bool close(const std::vector<gp_Pnt2d>& ends, double gap) {
    std::vector<bool> paired(ends.size(), false);
    for (std::size_t i = 0; i < ends.size(); ++i) {
      if (paired[i]) {
        continue;
      }
      std::size_t best = ends.size();
      double nearest = gap;
      for (std::size_t j = 0; j < ends.size(); ++j) {
        if (j != i && !paired[j] && ends[i].Distance(ends[j]) <= nearest) {
          nearest = ends[i].Distance(ends[j]);
          best = j;
        }
      }
      if (best == ends.size()) {
        return false;
      }
      paired[i] = true;
      paired[best] = true;
      if (nearest > 0.0) {
        m_segments.push_back({ends[i].X(), ends[i].Y(), ends[best].X(), ends[best].Y()});
      }
    }
    return true;
  }

  // The segments by slabs of v, each segment in every slab its v-extent
  // widened by the band reaches: the segments a point's ray may cross and
  // those within the band of it.
  void slabs() {
    int count = std::clamp(static_cast<int>(m_segments.size() / 2), 1, 256);
    auto range = [&](const Segment& s, int n, double height) {
      const int a = static_cast<int>((std::min(s.v0, s.v1) - m_band - m_vlow) / height);
      const int b = static_cast<int>((std::max(s.v0, s.v1) + m_band - m_vlow) / height);
      return std::make_pair(std::clamp(a, 0, n - 1), std::clamp(b, 0, n - 1));
    };
    // Fewer slabs where tall segments would fill too many.
    for (;;) {
      const double height = (m_vhigh - m_vlow) / count;
      std::size_t total = 0;
      for (const Segment& s : m_segments) {
        const auto r = range(s, count, height);
        total += static_cast<std::size_t>(r.second - r.first + 1);
      }
      if (total <= 4000000 || count == 1) {
        break;
      }
      count /= 2;
    }
    const auto n = static_cast<std::size_t>(count);
    m_slab = (m_vhigh - m_vlow) / count;
    if (!(m_slab > 0.0)) {
      m_slab = 1.0;
    }
    m_first.assign(n + 1, 0);
    for (const Segment& s : m_segments) {
      const auto r = range(s, count, m_slab);
      for (int k = r.first; k <= r.second; ++k) {
        ++m_first[static_cast<std::size_t>(k) + 1];
      }
    }
    for (std::size_t k = 0; k < n; ++k) {
      m_first[k + 1] += m_first[k];
    }
    m_index.resize(m_first[n]);
    std::vector<std::size_t> next(m_first.begin(), m_first.end() - 1);
    for (std::size_t i = 0; i < m_segments.size(); ++i) {
      const auto r = range(m_segments[i], count, m_slab);
      for (int k = r.first; k <= r.second; ++k) {
        m_index[next[static_cast<std::size_t>(k)]++] = i;
      }
    }
  }

  std::vector<Segment> m_segments;
  std::vector<std::size_t> m_first;
  std::vector<std::size_t> m_index;
  double m_band = 0.0;
  double m_band2 = 0.0;
  double m_vlow = 0.0;
  double m_vhigh = 0.0;
  double m_slab = 1.0;
};

// The largest tolerance of the face, its edges and its vertices.
double largest_tolerance(const TopoDS_Face& face) {
  double largest = BRep_Tool::Tolerance(face);
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    largest = std::max(largest, BRep_Tool::Tolerance(TopoDS::Edge(it.Current())));
  }
  for (TopExp_Explorer it(face, TopAbs_VERTEX); it.More(); it.Next()) {
    largest = std::max(largest, BRep_Tool::Tolerance(TopoDS::Vertex(it.Current())));
  }
  return largest;
}

// The face's surface over the rectangle of parameters its boundary spans,
// and whether a point of that rectangle lies in the face.
class FaceDomain {
public:
  explicit FaceDomain(const TopoDS_Face& face)
      : m_surface(face), m_u0(m_surface.FirstUParameter()), m_u1(m_surface.LastUParameter()),
        m_v0(m_surface.FirstVParameter()), m_v1(m_surface.LastVParameter()),
        m_classify(face, resolution(m_surface, BRep_Tool::Tolerance(face))) {
    const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
    const double tolerance = resolution(m_surface, BRep_Tool::Tolerance(face));
    const double gap = 2.0 * resolution(m_surface, largest_tolerance(face)) + Precision::PConfusion();
    m_polygon = m_polygon_built.build(forward, std::hypot(width(), height()), gap, tolerance);
  }

  double u0() const { return m_u0; }
  double v0() const { return m_v0; }
  double width() const { return m_u1 - m_u0; }
  double height() const { return m_v1 - m_v0; }
  int u_spans() const { return m_surface.NbUIntervals(GeomAbs_C2); }
  int v_spans() const { return m_surface.NbVIntervals(GeomAbs_C2); }

  // Whether a work bound cut the search short: its results are not to be
  // trusted, and the sides it searched keep OCCT's box.
  bool exhausted() const { return m_exhausted; }

  // Strictly inside the face: a point on its boundary (within the face's
  // tolerance) is the edges' and counted with them. Decided by the
  // boundary's polygon away from it; near it, and on a face without a
  // polygon, by OCCT's classifier within a bounded number of calls. Past
  // that bound a point near the polygon counts as outside (the climbs stop
  // a band's width short of the boundary, whose own box holds the face's
  // extremes there, and an extreme inside the face that close to the
  // boundary is missed by the square of that width at most), and on a face
  // without a polygon the search is cut short.
  bool inside(double u, double v) const {
    if (!(u >= m_u0 && u <= m_u1 && v >= m_v0 && v <= m_v1)) {
      return false;
    }
    if (m_polygon) {
      const int state = m_polygon_built.classify(u, v);
      if (state >= 0) {
        return state == 1;
      }
      if (m_near >= kMostNearClassifications) {
        return false;
      }
      ++m_near;
    } else {
      if (m_calls >= kMostClassifications) {
        m_exhausted = true;
        return false;
      }
      ++m_calls;
    }
    return m_classify.Perform(gp_Pnt2d(u, v)) == TopAbs_IN;
  }

  gp_Pnt point(double u, double v) const { return m_surface.Value(u, v); }

  Local local(double u, double v, int axis, double sign) const {
    gp_Pnt p;
    gp_Vec du, dv, duu, dvv, duv;
    m_surface.D2(u, v, p, du, dv, duu, dvv, duv);
    const int k = axis + 1;
    return {sign * p.Coord(k),   sign * du.Coord(k),  sign * dv.Coord(k),
            sign * duu.Coord(k), sign * duv.Coord(k), sign * dvv.Coord(k)};
  }

  // The largest value of the coordinate `axis` (times `sign`) reached by
  // climbing from (u, v), a point inside the face, without leaving it:
  // Newton's steps where the surface is locally a cap, else along the
  // gradient; each step shortened until it stays inside the face and the
  // value does not fall. `cell` is a step along the gradient as a share of
  // the rectangle's sides (the sampling grid's spacing).
  double climb(double u, double v, int axis, double sign, double cell) const {
    const double ru = width();
    const double rv = height();
    Local at = local(u, v, axis, sign);
    for (int iteration = 0; iteration < 100; ++iteration) {
      // In the rectangle's own scale (both sides 1), so that the steps of
      // a long narrow rectangle are alike in both directions.
      const double ga = at.gu * ru;
      const double gb = at.gv * rv;
      const double haa = at.huu * ru * ru;
      const double hab = at.huv * ru * rv;
      const double hbb = at.hvv * rv * rv;
      const double gg = ga * ga + gb * gb;
      if (!(gg > 0.0)) {
        break;
      }
      double sa = 0.0;
      double sb = 0.0;
      const double det = haa * hbb - hab * hab;
      if (haa < 0.0 && det > 0.0) {
        sa = -(hbb * ga - hab * gb) / det;
        sb = -(haa * gb - hab * ga) / det;
      } else {
        // Along the gradient: to the top of the value's parabola on that
        // line where it bends down, else one cell.
        const double bend = ga * (haa * ga + hab * gb) + gb * (hab * ga + hbb * gb);
        const double t = bend < 0.0 ? -gg / bend : cell / std::sqrt(gg);
        sa = t * ga;
        sb = t * gb;
      }
      bool moved = false;
      for (int half = 0; half < 40; ++half) {
        if (++m_steps > kMostClimbSteps) {
          m_exhausted = true;
          return at.f;
        }
        const double un = u + sa * ru;
        const double vn = v + sb * rv;
        if (inside(un, vn)) {
          const Local next = local(un, vn, axis, sign);
          if (next.f >= at.f) {
            u = un;
            v = vn;
            at = next;
            moved = true;
            break;
          }
        }
        sa *= 0.5;
        sb *= 0.5;
      }
      if (!moved || std::hypot(sa, sb) < 1.0e-13) {
        break;
      }
    }
    return at.f;
  }

private:
  BRepAdaptor_Surface m_surface;
  double m_u0;
  double m_u1;
  double m_v0;
  double m_v1;
  BRepTopAdaptor_FClass2d m_classify;
  BoundaryPolygon m_polygon_built;
  bool m_polygon = false;
  // The work spent: calls of OCCT's classifier and climbing steps.
  mutable int m_near = 0;
  mutable int m_calls = 0;
  mutable int m_steps = 0;
  mutable bool m_exhausted = false;
};

// The face's parameter grid: each sample's point and whether it lies
// inside the face.
struct Grid {
  int nu = 0;
  int nv = 0;
  std::vector<gp_Pnt> points;
  std::vector<bool> in;
  int inside = 0;

  double u(const FaceDomain& d, int i) const { return d.u0() + d.width() * (i + 0.5) / nu; }
  double v(const FaceDomain& d, int j) const { return d.v0() + d.height() * (j + 0.5) / nv; }
};

Grid sample(const FaceDomain& domain, int nu, int nv) {
  Grid grid;
  grid.nu = nu;
  grid.nv = nv;
  grid.points.resize(static_cast<std::size_t>(nu) * static_cast<std::size_t>(nv));
  grid.in.resize(grid.points.size());
  for (int i = 0; i < nu; ++i) {
    for (int j = 0; j < nv; ++j) {
      const std::size_t k = static_cast<std::size_t>(i) * static_cast<std::size_t>(nv) + static_cast<std::size_t>(j);
      const double u = grid.u(domain, i);
      const double v = grid.v(domain, j);
      grid.in[k] = domain.inside(u, v);
      if (grid.in[k]) {
        grid.points[k] = domain.point(u, v);
        ++grid.inside;
      }
    }
  }
  return grid;
}

// The largest value of the coordinate `axis` (times `sign`) over the
// face's interior: the best sample, and the tops reached by climbing from
// the samples that are highest among their neighbours inside the face.
double interior_top(const FaceDomain& domain, const Grid& grid, int axis, double sign) {
  const int k1 = axis + 1;
  auto at = [&](int i, int j) {
    return static_cast<std::size_t>(i) * static_cast<std::size_t>(grid.nv) + static_cast<std::size_t>(j);
  };
  double best = -Precision::Infinite();
  std::vector<std::pair<double, std::size_t>> starts;
  for (int i = 0; i < grid.nu; ++i) {
    for (int j = 0; j < grid.nv; ++j) {
      const std::size_t k = at(i, j);
      if (!grid.in[k]) {
        continue;
      }
      const double f = sign * grid.points[k].Coord(k1);
      best = std::max(best, f);
      bool top = true;
      for (int di = -1; di <= 1 && top; ++di) {
        for (int dj = -1; dj <= 1 && top; ++dj) {
          const int ni = i + di;
          const int nj = j + dj;
          if ((di == 0 && dj == 0) || ni < 0 || nj < 0 || ni >= grid.nu || nj >= grid.nv) {
            continue;
          }
          const std::size_t n = at(ni, nj);
          top = !grid.in[n] || sign * grid.points[n].Coord(k1) <= f;
        }
      }
      if (top) {
        starts.emplace_back(f, k);
      }
    }
  }
  // The eight highest starts: a face has few caps along an axis.
  std::sort(starts.begin(), starts.end(), [](const auto& a, const auto& b) { return a.first > b.first; });
  if (starts.size() > 8) {
    starts.resize(8);
  }
  const double cell = 1.0 / std::max(grid.nu, grid.nv);
  for (const auto& start : starts) {
    const int i = static_cast<int>(start.second / static_cast<std::size_t>(grid.nv));
    const int j = static_cast<int>(start.second % static_cast<std::size_t>(grid.nv));
    best = std::max(best, domain.climb(grid.u(domain, i), grid.v(domain, j), axis, sign, cell));
  }
  return best;
}

// Whether the surface is ruled along one of its parameters: straight along
// it, so that a coordinate is linear on each such line. Through any point
// inside a face the line runs to the face's boundary both ways, and the
// coordinate is as large at one end: the face's extremes along every axis
// lie on its boundary, and its box is its boundary's. A plane, a cylinder,
// a cone, an extrusion, a spline of one straight span along a parameter.
// The search inside such a face is skipped: on a planar face of a gear's
// 120 edges it took 7 s (each step classifies the point against every
// edge), and a design with a gear and a pulley opened in 8 s instead of
// under one (0.66.0-rc1).
bool ruled(const BRepAdaptor_Surface& surface) {
  switch (surface.GetType()) {
  case GeomAbs_Plane:
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
  case GeomAbs_SurfaceOfExtrusion:
    return true;
  case GeomAbs_BezierSurface:
  case GeomAbs_BSplineSurface:
    return (surface.UDegree() == 1 && surface.NbUPoles() == 2) ||
           (surface.VDegree() == 1 && surface.NbVPoles() == 2);
  default:
    return false;
  }
}

constexpr double kPi = 3.14159265358979323846;

// Margin of the parameter ranges in the tests below, in radians.
constexpr double kAngleMargin = 1.0e-7;

// Whether the angle `phi` plus a multiple of `period` lies in [low, high].
bool angle_in(double phi, double period, double low, double high) {
  const double k = std::ceil((low - kAngleMargin - phi) / period);
  return phi + k * period <= high + kAngleMargin;
}

// Whether a torus or a sphere whose coordinate along a direction is
// c + (R + r cos v)(a cos u + b sin u) + r z sin v (R = 0 on a sphere) has a
// point where both its derivatives vanish within [u0, u1] x [v0, v1]: where
// a sin u = b cos u (u = atan2(b, a) + k pi; R + r cos v > 0 when r < R, and
// cos v = 0 only at a sphere's poles, which the caller excludes) and
// tan v = z / (a cos u + b sin u).
bool round_critical_in(double a, double b, double z, double u0, double u1, double v0, double v1) {
  if (std::hypot(a, b) < 1.0e-12) {
    // Along the axis: every u, v where cos v = 0.
    return angle_in(0.5 * kPi, kPi, v0, v1);
  }
  const double start = std::atan2(b, a);
  const double first = std::ceil((u0 - kAngleMargin - start) / kPi);
  const double last = std::floor((u1 + kAngleMargin - start) / kPi);
  if (last - first > 8.0) {
    return true;
  }
  for (double k = first; k <= last; k += 1.0) {
    const double u = start + k * kPi;
    if (angle_in(std::atan2(z, a * std::cos(u) + b * std::sin(u)), kPi, v0, v1)) {
      return true;
    }
  }
  return false;
}

// Whether all the points are within `tolerance` of the plane through
// `origin` with the normal `normal`.
bool in_plane(const gp_Pnt& p, const gp_Pnt& origin, const gp_Dir& normal, double tolerance) {
  return std::abs(gp_Vec(origin, p).Dot(gp_Vec(normal))) <= tolerance;
}

// Whether the conic of position `position` lies in the plane.
bool conic_in_plane(const gp_Ax2& position, const gp_Pnt& origin, const gp_Dir& normal, double tolerance) {
  return in_plane(position.Location(), origin, normal, tolerance) &&
         std::abs(std::abs(position.Direction().Dot(normal)) - 1.0) <= 1.0e-12;
}

// Whether a surface of revolution has no point within [u0, u1] x [v0, v1]
// where its coordinate along `d` has both derivatives zero, as far as the
// rotation shows it: its meridian lies in a plane through the axis, along
// the axis Z and the radial direction m, and keeps off the axis (s(v) != 0),
// so the coordinate d.O + h(v) d.Z + s(v) (a cos u + b sin u) with
// a = d.m, b = d.(Z x m) turns in u only where u = atan2(b, a) + k pi.
bool revolution_free(const GeomAdaptor_Surface& surface, const gp_Dir& d, double u0, double u1, double v0,
                     double v1) {
  const occ::handle<Adaptor3d_Curve> meridian = surface.BasisCurve();
  if (meridian.IsNull()) {
    return false;
  }
  const gp_Ax1 axis = surface.AxeOfRevolution();
  const gp_Pnt origin = axis.Location();
  const gp_Vec along(axis.Direction());
  if (!meridian->IsPeriodic()) {
    v0 = std::max(v0, meridian->FirstParameter());
    v1 = std::min(v1, meridian->LastParameter());
  }
  if (!(v1 > v0) || Precision::IsInfinite(v0) || Precision::IsInfinite(v1)) {
    return false;
  }
  // The radial direction: from the axis to the meridian's farthest sample.
  gp_Vec radial;
  double reach = 0.0;
  double size = 0.0;
  for (int k = 0; k <= 8; ++k) {
    const gp_Vec p(origin, meridian->Value(v0 + (v1 - v0) * k / 8.0));
    const gp_Vec r = p - along * p.Dot(along);
    size = std::max(size, p.Magnitude());
    if (r.Magnitude() > reach) {
      reach = r.Magnitude();
      radial = r;
    }
  }
  if (!(reach > 1.0e-9 * std::max(size, 1.0))) {
    return false;
  }
  const gp_Dir m(radial);
  const gp_Dir normal(along.Crossed(gp_Vec(m)));
  const double tolerance = 1.0e-9 + 1.0e-12 * size;
  bool planar = false;
  switch (meridian->GetType()) {
  case GeomAbs_Line: {
    const gp_Lin line = meridian->Line();
    planar = in_plane(line.Location(), origin, normal, tolerance) &&
             std::abs(line.Direction().Dot(normal)) <= 1.0e-12;
    break;
  }
  case GeomAbs_Circle:
    planar = conic_in_plane(meridian->Circle().Position(), origin, normal, tolerance);
    break;
  case GeomAbs_Ellipse:
    planar = conic_in_plane(meridian->Ellipse().Position(), origin, normal, tolerance);
    break;
  case GeomAbs_BezierCurve: {
    const occ::handle<Geom_BezierCurve> curve = meridian->Bezier();
    planar = !curve.IsNull();
    for (int i = 1; planar && i <= curve->NbPoles(); ++i) {
      planar = in_plane(curve->Pole(i), origin, normal, tolerance);
    }
    break;
  }
  case GeomAbs_BSplineCurve: {
    const occ::handle<Geom_BSplineCurve> curve = meridian->BSpline();
    planar = !curve.IsNull();
    for (int i = 1; planar && i <= curve->NbPoles(); ++i) {
      planar = in_plane(curve->Pole(i), origin, normal, tolerance);
    }
    break;
  }
  default:
    break;
  }
  if (!planar) {
    return false;
  }
  // Off the axis: s(v) = (C(v) - O).m keeps one sign over the meridian's
  // box (a linear function over a box is extreme at its corners).
  Bnd_Box box;
  BndLib_Add3dCurve::Add(*meridian, v0, v1, 0.0, box);
  if (box.IsVoid() || box.IsOpen()) {
    return false;
  }
  double xs[2];
  double ys[2];
  double zs[2];
  box.Get(xs[0], ys[0], zs[0], xs[1], ys[1], zs[1]);
  double low = Precision::Infinite();
  double high = -Precision::Infinite();
  for (int corner = 0; corner < 8; ++corner) {
    const gp_Pnt p(xs[corner & 1], ys[(corner >> 1) & 1], zs[(corner >> 2) & 1]);
    const double s = gp_Vec(origin, p).Dot(gp_Vec(m));
    low = std::min(low, s);
    high = std::max(high, s);
  }
  if (low <= 0.0 && high >= 0.0) {
    return false;
  }
  const double a = gp_Vec(d).Dot(gp_Vec(m));
  const double b = gp_Vec(d).Dot(along.Crossed(gp_Vec(m)));
  if (std::hypot(a, b) < 1.0e-12) {
    // Along the axis: every point turns in u.
    return false;
  }
  return !angle_in(std::atan2(b, a), kPi, u0, u1);
}

// Whether the face's surface provably has no point inside the parameter
// rectangle of `surface` (the face's) where the coordinate `axis` has both
// derivatives zero: then the coordinate has no extreme inside the face, and
// its extremes lie on the boundary. A torus or a sphere (whose extremes along
// a direction are known), a surface of revolution of a planar meridian
// clear of its axis (where the coordinate turns in u at two angles only).
// Other surfaces are searched.
bool no_interior_extreme(const BRepAdaptor_Surface& surface, int axis) {
  // The world's axis in the surface's own frame (the face's location
  // undone): the coordinate along it is the world's, up to a constant and
  // a positive scale.
  gp_Vec direction(axis == 0 ? 1.0 : 0.0, axis == 1 ? 1.0 : 0.0, axis == 2 ? 1.0 : 0.0);
  direction.Transform(surface.Trsf().Inverted());
  if (!(direction.Magnitude() > 0.0)) {
    return false;
  }
  const gp_Dir d(direction);
  const GeomAdaptor_Surface& own = surface.AdaptorSurfaceOriginal();
  const double u0 = surface.FirstUParameter();
  const double u1 = surface.LastUParameter();
  const double v0 = surface.FirstVParameter();
  const double v1 = surface.LastVParameter();
  if (Precision::IsInfinite(u0) || Precision::IsInfinite(u1) || Precision::IsInfinite(v0) ||
      Precision::IsInfinite(v1)) {
    return false;
  }
  switch (own.GetType()) {
  case GeomAbs_Sphere: {
    // A pole within the range: the coordinate's extreme may be there.
    if (v1 >= 0.5 * kPi - kAngleMargin || v0 <= -0.5 * kPi + kAngleMargin) {
      return false;
    }
    const gp_Ax3 frame = own.Sphere().Position();
    return !round_critical_in(d.Dot(frame.XDirection()), d.Dot(frame.YDirection()), d.Dot(frame.Direction()), u0,
                              u1, v0, v1);
  }
  case GeomAbs_Torus: {
    const gp_Torus torus = own.Torus();
    if (torus.MinorRadius() >= torus.MajorRadius()) {
      return false;
    }
    const gp_Ax3 frame = torus.Position();
    return !round_critical_in(d.Dot(frame.XDirection()), d.Dot(frame.YDirection()), d.Dot(frame.Direction()), u0,
                              u1, v0, v1);
  }
  case GeomAbs_SurfaceOfRevolution:
    return revolution_free(own, d, u0, u1, v0, v1);
  default:
    return false;
  }
}

} // namespace

Bnd_Box trimmed_face_box(const TopoDS_Face& face) {
  Bnd_Box occt;
  BRepBndLib::AddOptimal(face, occt, false, false);
  TopLoc_Location location;
  if (occt.IsVoid() || BRep_Tool::Surface(face, location).IsNull()) {
    return occt;
  }
  // The boundary: its edges' curves, exact.
  Bnd_Box boundary;
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    if (!BRep_Tool::IsGeometric(edge)) {
      // An edge with no curve in space: its points are not known here.
      return occt;
    }
    BRepBndLib::AddOptimal(edge, boundary, false, false);
  }
  if (boundary.IsVoid()) {
    // A closed surface without boundary (a whole sphere): its own box.
    return occt;
  }
  const Sides outer = sides_of(occt);
  Sides result = sides_of(boundary);
  // The sides where OCCT's box reaches past the boundary: the face reaches
  // there only through an interior point where its surface turns back
  // along that axis, if at all.
  std::array<bool, 6> open{};
  bool any = false;
  for (std::size_t k = 0; k < 3; ++k) {
    open[k] = result[k] - outer[k] > kOpenSide;
    open[k + 3] = outer[k + 3] - result[k + 3] > kOpenSide;
    any = any || open[k] || open[k + 3];
  }
  if (!any) {
    return occt;
  }
  // The axes to search inside the face: those with an open side, unless
  // the surface is ruled or provably has no extreme along the axis inside
  // the face's parameter rectangle. The others take the boundary's sides
  // where OCCT's box reaches past them.
  std::array<bool, 3> search{};
  bool any_search = false;
  {
    const BRepAdaptor_Surface surface(face);
    const bool straight = ruled(surface);
    for (std::size_t axis = 0; axis < 3; ++axis) {
      search[axis] = (open[axis] || open[axis + 3]) && !straight &&
                     !no_interior_extreme(surface, static_cast<int>(axis));
      any_search = any_search || search[axis];
    }
  }
  for (std::size_t k = 0; k < 6; ++k) {
    if (!open[k]) {
      // A side where OCCT's box is the boundary's within kOpenSide keeps it.
      result[k] = outer[k];
    }
  }
  if (!any_search) {
    Bnd_Box box;
    box.Update(result[0], result[1], result[2], result[3], result[4], result[5]);
    return box;
  }
  const FaceDomain domain(face);
  if (!(domain.width() > 0.0) || !(domain.height() > 0.0)) {
    return occt;
  }
  int nu = samples_for(domain.u_spans());
  int nv = samples_for(domain.v_spans());
  Grid grid = sample(domain, nu, nv);
  // A narrow face: a finer grid until enough samples fall inside it.
  while (grid.inside < kInsideSamples && std::max(nu, nv) < kMostSamples && !domain.exhausted()) {
    nu *= 2;
    nv *= 2;
    grid = sample(domain, nu, nv);
  }
  for (int axis = 0; axis < 3; ++axis) {
    const auto low = static_cast<std::size_t>(axis);
    const auto high = static_cast<std::size_t>(axis + 3);
    if (!search[low]) {
      continue;
    }
    if (open[low] && grid.inside > 0) {
      result[low] = std::min(result[low], -interior_top(domain, grid, axis, -1.0));
    }
    // A search cut short by its work bound keeps OCCT's side.
    if (open[low] && domain.exhausted()) {
      result[low] = outer[low];
    }
    if (open[high] && grid.inside > 0) {
      result[high] = std::max(result[high], interior_top(domain, grid, axis, 1.0));
    }
    if (open[high] && domain.exhausted()) {
      result[high] = outer[high];
    }
  }
  Bnd_Box box;
  box.Update(result[0], result[1], result[2], result[3], result[4], result[5]);
  return box;
}

Bnd_Box trimmed_box(const TopoDS_Shape& shape) {
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    faces.push_back(TopoDS::Face(it.Current()));
  }
  // On all cores: the optimal box of a spline face is a search over its
  // surface, seconds for a body of thousands of faces.
  std::vector<Bnd_Box> boxes(faces.size());
  std::vector<std::exception_ptr> errors(faces.size());
  OSD_Parallel::For(
      0, static_cast<int>(faces.size()),
      [&](int i) {
        const auto k = static_cast<std::size_t>(i);
        try {
          boxes[k] = trimmed_face_box(faces[k]);
        } catch (...) {
          errors[k] = std::current_exception();
        }
      },
      faces.size() < 8);
  Bnd_Box box;
  for (std::size_t k = 0; k < faces.size(); ++k) {
    if (errors[k]) {
      std::rethrow_exception(errors[k]);
    }
    if (!boxes[k].IsVoid()) {
      const Sides s = sides_of(boxes[k]);
      box.Update(s[0], s[1], s[2], s[3], s[4], s[5]);
    }
  }
  // The edges not in faces and the vertices not in edges, as
  // BRepBndLib::AddOptimal adds them.
  BRep_Builder builder;
  TopoDS_Compound rest;
  builder.MakeCompound(rest);
  bool any = false;
  for (TopExp_Explorer it(shape, TopAbs_EDGE, TopAbs_FACE); it.More(); it.Next()) {
    builder.Add(rest, it.Current());
    any = true;
  }
  for (TopExp_Explorer it(shape, TopAbs_VERTEX, TopAbs_EDGE); it.More(); it.Next()) {
    builder.Add(rest, it.Current());
    any = true;
  }
  if (any) {
    BRepBndLib::AddOptimal(rest, box, false, false);
  }
  return box;
}

} // namespace mitcad::geometry
