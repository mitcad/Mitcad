// SPDX-License-Identifier: MIT
#include "loft_merge.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <optional>
#include <stdexcept>

#include <Adaptor3d_Curve.hxx>
#include <Approx_CurvilinearParameter.hxx>
#include <BRepAdaptor_CompCurve.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepGProp.hxx>
#include <BRepLib_FindSurface.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_Shape.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Plane.hxx>
#include <Geom_Surface.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

namespace mitcad::geometry::detail {
namespace {

// A section's edges in order round its wire, each as the wire runs it, and
// their names.
struct Loop {
  std::vector<TopoDS_Edge> edges;
  std::vector<std::optional<std::string>> names;
  bool closed = false;
};

Loop loop_of(const NamedWire& named) {
  Loop loop;
  for (BRepTools_WireExplorer it(named.wire); it.More(); it.Next()) {
    const TopoDS_Edge& edge = it.Current();
    loop.edges.push_back(edge);
    std::optional<std::string> name;
    for (const auto& [e, n] : named.edges) {
      if (e.IsSame(edge)) {
        name = n;
        break;
      }
    }
    loop.names.push_back(name);
  }
  loop.closed = !loop.edges.empty() && BRep_Tool::IsClosed(named.wire);
  return loop;
}

bool reversed(const TopoDS_Edge& edge) { return edge.Orientation() == TopAbs_REVERSED; }

// The point and the derivative where the edge starts (`start`) or ends as
// its wire runs it.
std::pair<gp_Pnt, gp_Vec> end_of(const TopoDS_Edge& edge, bool start) {
  const BRepAdaptor_Curve curve(edge);
  const double u = start != reversed(edge) ? curve.FirstParameter() : curve.LastParameter();
  gp_Pnt p;
  gp_Vec d;
  curve.D1(u, p, d);
  if (reversed(edge)) {
    d.Reverse();
  }
  return {p, d};
}

// Edge a runs on into edge b without a corner.
bool tangent_join(const TopoDS_Edge& a, const TopoDS_Edge& b) {
  if (BRep_Tool::Degenerated(a) || BRep_Tool::Degenerated(b)) {
    return false;
  }
  const gp_Vec out = end_of(a, false).second;
  const gp_Vec in = end_of(b, true).second;
  if (out.Magnitude() <= 1.0e-12 || in.Magnitude() <= 1.0e-12) {
    return false;
  }
  return out.Angle(in) <= kTangentJoinAngle;
}

// The edges, as they run one after another (a closed ring when `closed`),
// as one curve by arc length: a polynomial B-spline within kJoinTolerance
// of them, its first derivative continuous across their joins (each piece
// of the approximation meets the next with the unit tangent there times
// the length).
occ::handle<Geom_BSplineCurve> joined_curve(const std::vector<TopoDS_Edge>& edges, bool closed) {
  BRep_Builder builder;
  TopoDS_Wire wire;
  builder.MakeWire(wire);
  for (const TopoDS_Edge& edge : edges) {
    builder.Add(wire, edge);
  }
  wire.Closed(closed);
  const occ::handle<Adaptor3d_Curve> along = new BRepAdaptor_CompCurve(wire);
  const Approx_CurvilinearParameter approximation(along, kJoinTolerance, GeomAbs_C1, 9, 1000);
  if (!approximation.HasResult() || approximation.MaxError3d() > 10 * kJoinTolerance) {
    throw std::runtime_error("the tangent edges of a loft section could not be joined into one curve");
  }
  occ::handle<Geom_BSplineCurve> curve = approximation.Curve3d();
  const gp_Pnt start = end_of(edges.front(), true).first;
  const gp_Pnt end = end_of(edges.back(), false).first;
  if (curve->StartPoint().Distance(start) > 1.0e-6 || curve->EndPoint().Distance(end) > 1.0e-6) {
    throw std::runtime_error("the tangent edges of a loft section could not be joined in order");
  }
  if (closed) {
    // The ring closes exactly where it starts.
    curve->SetPole(curve->NbPoles(), curve->Pole(1));
  }
  return curve;
}

TopoDS_Edge edge_between(const occ::handle<Geom_BSplineCurve>& curve, const TopoDS_Vertex& from,
                         const TopoDS_Vertex& to) {
  BRepBuilderAPI_MakeEdge edge(curve, from, to, curve->FirstParameter(), curve->LastParameter());
  if (!edge.IsDone()) {
    throw std::runtime_error("the tangent edges of a loft section could not be joined into one edge");
  }
  return edge.Edge();
}

// Where a section lies: the centre of its edges, the normal of its plane
// (none when it is not planar) and the point it starts at.
struct Placement {
  gp_Pnt centre;
  std::optional<gp_Dir> normal;
  gp_Pnt seam;
};

Placement placement_of(const TopoDS_Wire& wire, const std::vector<TopoDS_Edge>& edges) {
  Placement placed;
  GProp_GProps props;
  BRepGProp::LinearProperties(wire, props);
  placed.centre = props.CentreOfMass();
  BRepLib_FindSurface finder(wire, -1, true);
  if (finder.Found()) {
    occ::handle<Geom_Surface> surface = finder.Surface();
    if (!finder.Location().IsIdentity()) {
      surface = occ::down_cast<Geom_Surface>(surface->Transformed(finder.Location().Transformation()));
    }
    if (const occ::handle<Geom_Plane> plane = occ::down_cast<Geom_Plane>(surface); !plane.IsNull()) {
      placed.normal = plane->Pln().Axis().Direction();
    }
  }
  placed.seam = end_of(edges.front(), true).first;
  return placed;
}

// +1 when the edges run counter-clockwise round the normal about the centre.
double sense(const std::vector<TopoDS_Edge>& edges, const gp_Pnt& centre, const gp_Dir& normal) {
  double area = 0.0;
  bool started = false;
  gp_Vec first;
  gp_Vec previous;
  for (const TopoDS_Edge& edge : edges) {
    const BRepAdaptor_Curve curve(edge);
    const double a = curve.FirstParameter();
    const double b = curve.LastParameter();
    for (int k = 0; k < 16; ++k) {
      const double t = reversed(edge) ? b - (b - a) * k / 16 : a + (b - a) * k / 16;
      const gp_Vec p(centre, curve.Value(t));
      if (started) {
        area += previous.Crossed(p).Dot(gp_Vec(normal));
      } else {
        first = p;
        started = true;
      }
      previous = p;
    }
  }
  area += previous.Crossed(first).Dot(gp_Vec(normal));
  return area >= 0.0 ? 1.0 : -1.0;
}

// The ring the other way round.
std::vector<TopoDS_Edge> turned(const std::vector<TopoDS_Edge>& ring) {
  std::vector<TopoDS_Edge> other;
  for (std::size_t j = ring.size(); j-- > 0;) {
    other.push_back(TopoDS::Edge(ring[j].Reversed()));
  }
  return other;
}

// The vector `v` (in the plane of normal `from`) carried into the plane of
// normal `to`: turned as the one normal turns into the other, then laid in
// that plane.
gp_Vec carried(const gp_Vec& v, const gp_Dir& from, const gp_Dir& to) {
  gp_Vec result = v;
  const gp_Vec axis = gp_Vec(from).Crossed(gp_Vec(to));
  if (axis.Magnitude() > 1.0e-12) {
    result.Rotate(gp_Ax1(gp::Origin(), gp_Dir(axis)), from.Angle(to));
  }
  return result - gp_Vec(to) * result.Dot(gp_Vec(to));
}

// Where the ray from the centre along `towards` (in the ring's plane of
// `normal`) crosses the ring: the edge and its curve's parameter, the
// crossing nearest to `target` where it crosses more than once.
std::optional<std::pair<std::size_t, double>> crossing(const std::vector<TopoDS_Edge>& ring, const gp_Pnt& centre,
                                                       const gp_Dir& normal, const gp_Vec& towards,
                                                       const gp_Pnt& target) {
  if (towards.Magnitude() <= 1.0e-9) {
    return std::nullopt;
  }
  const gp_Vec along(gp_Dir(towards).XYZ());
  const gp_Vec across = gp_Vec(normal).Crossed(along);
  std::optional<std::pair<std::size_t, double>> best;
  double nearest = 0.0;
  for (std::size_t k = 0; k < ring.size(); ++k) {
    const BRepAdaptor_Curve curve(ring[k]);
    const double a = curve.FirstParameter();
    const double b = curve.LastParameter();
    const auto side = [&](double u) { return gp_Vec(centre, curve.Value(u)).Dot(across); };
    constexpr int kSamples = 64;
    double u0 = a;
    double s0 = side(a);
    for (int j = 1; j <= kSamples; ++j) {
      const double u1 = a + (b - a) * j / kSamples;
      const double s1 = side(u1);
      if ((s0 <= 0.0) != (s1 <= 0.0) || s0 == 0.0) {
        double lo = u0;
        double hi = u1;
        const bool low_negative = s0 <= 0.0;
        for (int i = 0; i < 80; ++i) {
          const double mid = (lo + hi) / 2;
          if ((side(mid) <= 0.0) == low_negative) {
            lo = mid;
          } else {
            hi = mid;
          }
        }
        const double u = s0 == 0.0 ? u0 : (lo + hi) / 2;
        const gp_Pnt p = curve.Value(u);
        if (gp_Vec(centre, p).Dot(along) > 0.0) {
          const double distance = p.Distance(target);
          if (!best || distance < nearest) {
            best = std::pair{k, u};
            nearest = distance;
          }
        }
      }
      u0 = u1;
      s0 = s1;
    }
  }
  return best;
}

// The ring from the point at parameter u of its edge k round to that point
// again, the edge cut there (not cut at its ends).
std::vector<TopoDS_Edge> from_point(const std::vector<TopoDS_Edge>& ring, std::size_t k, double u) {
  const std::size_t n = ring.size();
  const TopoDS_Edge& edge = ring[k];
  double f = 0.0;
  double l = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, f, l);
  const double slack = 1.0e-9 * std::max(1.0, std::abs(l - f));
  const double start = reversed(edge) ? l : f;
  const double end = reversed(edge) ? f : l;
  std::vector<TopoDS_Edge> out;
  const auto rotated = [&](std::size_t from) {
    for (std::size_t j = 0; j < n; ++j) {
      out.push_back(ring[(from + j) % n]);
    }
    return out;
  };
  if (curve.IsNull() || std::abs(u - start) <= slack) {
    return rotated(k);
  }
  if (std::abs(u - end) <= slack) {
    return rotated((k + 1) % n);
  }
  const TopoDS_Vertex at = BRepBuilderAPI_MakeVertex(curve->Value(u)).Vertex();
  const TopoDS_Vertex at_f = TopExp::FirstVertex(edge);
  const TopoDS_Vertex at_l = TopExp::LastVertex(edge);
  BRepBuilderAPI_MakeEdge low(curve, at_f, at, f, u);
  BRepBuilderAPI_MakeEdge high(curve, at, at_l, u, l);
  if (!low.IsDone() || !high.IsDone()) {
    return rotated(k);
  }
  // From the cut on as the ring runs, round to the cut again.
  out.push_back(reversed(edge) ? TopoDS::Edge(low.Edge().Reversed()) : high.Edge());
  for (std::size_t j = 1; j < n; ++j) {
    out.push_back(ring[(k + j) % n]);
  }
  out.push_back(reversed(edge) ? TopoDS::Edge(high.Edge().Reversed()) : low.Edge());
  return out;
}

// A section in the work: its loop, and for one smooth all round the ring
// to join (turned and started where it is to start).
struct Work {
  Loop loop;
  bool smooth = false;
  std::vector<TopoDS_Edge> ring;
  Placement placed;
  std::optional<gp_Dir> normal; // its plane's normal, the same way as its neighbours'
  double turn = 1.0;
};

// Joins the section's runs of tangent edges (not smooth all round): each
// run of two or more edges one edge between its corners.
void join_runs(MergeSection& section, const Loop& loop) {
  const std::size_t n = loop.edges.size();
  std::vector<bool> joins(n, false);
  for (std::size_t k = 0; k + 1 < n; ++k) {
    joins[k] = tangent_join(loop.edges[k], loop.edges[k + 1]);
  }
  if (loop.closed && n > 1) {
    joins[n - 1] = tangent_join(loop.edges[n - 1], loop.edges[0]);
  }
  // From the start of the run the wire's first edge is in.
  std::size_t first = 0;
  if (loop.closed) {
    for (std::size_t guard = 0; guard < n && joins[(first + n - 1) % n]; ++guard) {
      first = (first + n - 1) % n;
    }
  }
  std::vector<std::pair<std::size_t, std::size_t>> runs; // first edge, edges
  bool any = false;
  for (std::size_t done = 0, at = first; done < n;) {
    std::size_t count = 1;
    while (done + count < n && joins[(at + count - 1) % n]) {
      ++count;
    }
    runs.emplace_back(at, count);
    any = any || count > 1;
    done += count;
    at = (at + count) % n;
  }
  if (!any) {
    return;
  }
  BRep_Builder builder;
  TopoDS_Wire wire;
  builder.MakeWire(wire);
  NamedWire named;
  std::vector<std::pair<TopoDS_Edge, std::vector<TopoDS_Edge>>> sources;
  for (const auto& [at, count] : runs) {
    std::vector<TopoDS_Edge> run;
    for (std::size_t j = 0; j < count; ++j) {
      run.push_back(loop.edges[(at + j) % n]);
    }
    TopoDS_Edge edge = run.front();
    if (count > 1) {
      edge = edge_between(joined_curve(run, false), TopExp::FirstVertex(run.front(), true),
                          TopExp::LastVertex(run.back(), true));
      sources.emplace_back(edge, run);
    }
    builder.Add(wire, edge);
    if (const std::optional<std::string>& name = loop.names[at]) {
      named.edges.emplace_back(edge, *name);
    }
  }
  wire.Closed(loop.closed);
  named.wire = wire;
  section.named = named;
  section.sources = sources;
}

} // namespace

std::vector<TopoDS_Edge> source_edges(const std::vector<std::pair<TopoDS_Edge, std::vector<TopoDS_Edge>>>& sources,
                                      const TopoDS_Edge& edge) {
  for (const auto& [joined, from] : sources) {
    if (joined.IsSame(edge)) {
      return from;
    }
  }
  return {edge};
}

bool merge_tangent_edges(std::vector<MergeSection>& sections) {
  const std::size_t count = sections.size();
  std::vector<Work> work(count);
  std::optional<std::size_t> anchor;
  std::optional<std::size_t> first_curve;
  bool all_smooth = true;
  for (std::size_t i = 0; i < count; ++i) {
    if (sections[i].point) {
      continue;
    }
    Work& w = work[i];
    w.loop = loop_of(sections[i].named);
    if (w.loop.edges.empty()) {
      all_smooth = false;
      continue;
    }
    if (!first_curve) {
      first_curve = i;
    }
    bool smooth = w.loop.closed;
    for (std::size_t k = 0; smooth && k < w.loop.edges.size(); ++k) {
      smooth = tangent_join(w.loop.edges[k], w.loop.edges[(k + 1) % w.loop.edges.size()]);
    }
    w.smooth = smooth;
    if (smooth) {
      w.ring = w.loop.edges;
    } else {
      all_smooth = false;
      join_runs(sections[i], w.loop);
      if (!anchor) {
        anchor = i;
      }
    }
  }
  if (!first_curve) {
    return false;
  }
  if (!anchor) {
    anchor = first_curve;
  }

  // Each section placed after its neighbour towards the anchor: the same
  // way round it, a smooth one starting where the ray from its centre
  // towards the neighbour's start meets it.
  const auto place = [&](std::size_t i, std::optional<std::size_t> from) {
    Work& w = work[i];
    const std::vector<TopoDS_Edge> edges = w.smooth ? w.ring : loop_of(sections[i].named).edges;
    w.placed = placement_of(sections[i].named.wire, edges);
    w.normal = w.placed.normal;
    if (!w.normal) {
      return;
    }
    if (from && work[*from].normal && w.normal->Dot(*work[*from].normal) < 0.0) {
      w.normal = w.normal->Reversed();
    }
    w.turn = sense(edges, w.placed.centre, *w.normal);
    if (!w.smooth || !from || !work[*from].normal) {
      return;
    }
    const Work& before = work[*from];
    if (w.turn != before.turn) {
      w.ring = turned(w.ring);
      w.turn = before.turn;
    }
    const gp_Vec towards =
        carried(gp_Vec(before.placed.centre, before.placed.seam), *before.normal, *w.normal);
    const gp_Pnt target = w.placed.centre.Translated(towards);
    if (const auto at = crossing(w.ring, w.placed.centre, *w.normal, towards, target)) {
      w.ring = from_point(w.ring, at->first, at->second);
    }
    w.placed.seam = end_of(w.ring.front(), true).first;
  };
  place(*anchor, std::nullopt);
  std::optional<std::size_t> last = anchor;
  for (std::size_t i = *anchor + 1; i < count; ++i) {
    if (!sections[i].point && !work[i].loop.edges.empty()) {
      place(i, last);
      last = i;
    }
  }
  last = anchor;
  for (std::size_t i = *anchor; i-- > 0;) {
    if (!sections[i].point && !work[i].loop.edges.empty()) {
      place(i, last);
      last = i;
    }
  }

  // The smooth sections joined into one closed edge each, named after the
  // edge whose name sorts first.
  for (std::size_t i = 0; i < count; ++i) {
    Work& w = work[i];
    if (sections[i].point || !w.smooth) {
      continue;
    }
    const occ::handle<Geom_BSplineCurve> curve = joined_curve(w.ring, true);
    const TopoDS_Vertex seam = BRepBuilderAPI_MakeVertex(curve->StartPoint()).Vertex();
    const TopoDS_Edge edge = edge_between(curve, seam, seam);
    BRep_Builder builder;
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    builder.Add(wire, edge);
    wire.Closed(true);
    NamedWire named;
    named.wire = wire;
    // A ring has no first edge of its own (the sketch's loop may start
    // anywhere round it): the name that sorts first, whatever the start.
    std::optional<std::string> first_name;
    for (const std::optional<std::string>& name : w.loop.names) {
      if (name && (!first_name || *name < *first_name)) {
        first_name = name;
      }
    }
    if (first_name) {
      named.edges.emplace_back(edge, *first_name);
    }
    sections[i].named = named;
    sections[i].sources = {{edge, w.loop.edges}};
    sections[i].smooth = true;
  }
  return all_smooth;
}

} // namespace mitcad::geometry::detail
