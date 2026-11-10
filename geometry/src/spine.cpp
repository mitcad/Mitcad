// SPDX-License-Identifier: MIT
#include "spine.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <typeinfo>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Status.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepLib_FindSurface.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRepFill_TypeOfContact.hxx>
#include <GeomAPI_PointsToBSpline.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <ShapeExtend_WireData.hxx>
#include <ShapeFix_Wire.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Vec2d.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Curve.hxx>
#include <TopLoc_Location.hxx>
#include <Geom_Surface.hxx>
#include <ShapeConstruct_ProjectCurveOnSurface.hxx>
#include <Standard_Failure.hxx>
#include <Standard_Type.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>

#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

// Distinct knots and multiplicities of a full knot vector.
void compress_knots(const std::vector<double>& full, std::vector<double>& knots, std::vector<int>& mults) {
  for (const double k : full) {
    if (!knots.empty() && std::abs(k - knots.back()) <= 1e-12 * std::max(1.0, std::abs(k))) {
      ++mults.back();
    } else {
      knots.push_back(k);
      mults.push_back(1);
    }
  }
}

TopoDS_Edge edge_of(BRepBuilderAPI_MakeEdge& maker) {
  if (!maker.IsDone()) {
    throw std::invalid_argument("a path curve could not be built");
  }
  return maker.Edge();
}

} // namespace

TopoDS_Edge model_edge(const ModelCurve& c) {
  switch (c.kind) {
  case ModelCurveKind::Line: {
    if (c.start.Distance(c.end) <= Precision::Confusion()) {
      throw std::invalid_argument("a path line has no length");
    }
    BRepBuilderAPI_MakeEdge maker(c.start, c.end);
    return edge_of(maker);
  }
  case ModelCurveKind::Conic: {
    require_positive("radius", c.minor);
    const gp_Ax2 axes(c.center, c.normal, c.x_axis);
    if (std::abs(c.major - c.minor) <= 1e-12 * std::max(1.0, c.major)) {
      const gp_Circ circle(axes, c.major);
      BRepBuilderAPI_MakeEdge maker = c.closed ? BRepBuilderAPI_MakeEdge(circle)
                                               : BRepBuilderAPI_MakeEdge(circle, c.first, c.last);
      return edge_of(maker);
    }
    if (c.major < c.minor) {
      // gp_Elips needs the major radius first: turn the axes a quarter.
      const gp_Ax2 turned(c.center, c.normal, axes.YDirection());
      const gp_Elips ellipse(turned, c.minor, c.major);
      const double quarter = kTwoPi / 4.0;
      BRepBuilderAPI_MakeEdge maker = c.closed ? BRepBuilderAPI_MakeEdge(ellipse)
                                               : BRepBuilderAPI_MakeEdge(ellipse, c.first - quarter,
                                                                         c.last - quarter);
      return edge_of(maker);
    }
    const gp_Elips ellipse(axes, c.major, c.minor);
    BRepBuilderAPI_MakeEdge maker = c.closed ? BRepBuilderAPI_MakeEdge(ellipse)
                                             : BRepBuilderAPI_MakeEdge(ellipse, c.first, c.last);
    return edge_of(maker);
  }
  case ModelCurveKind::BSpline: {
    if (c.degree < 1 || c.poles.size() < 2 ||
        c.knots.size() != c.poles.size() + static_cast<std::size_t>(c.degree) + 1 ||
        (!c.weights.empty() && c.weights.size() != c.poles.size())) {
      throw std::invalid_argument("a path B-spline has inconsistent data");
    }
    std::vector<double> distinct;
    std::vector<int> mults;
    compress_knots(c.knots, distinct, mults);
    NCollection_Array1<gp_Pnt> poles(1, static_cast<int>(c.poles.size()));
    for (std::size_t i = 0; i < c.poles.size(); ++i) {
      poles.SetValue(static_cast<int>(i) + 1, c.poles[i]);
    }
    NCollection_Array1<double> knots(1, static_cast<int>(distinct.size()));
    NCollection_Array1<int> multiplicities(1, static_cast<int>(distinct.size()));
    for (std::size_t i = 0; i < distinct.size(); ++i) {
      knots.SetValue(static_cast<int>(i) + 1, distinct[i]);
      multiplicities.SetValue(static_cast<int>(i) + 1, mults[i]);
    }
    occ::handle<Geom_BSplineCurve> curve;
    if (c.weights.empty()) {
      curve = new Geom_BSplineCurve(poles, knots, multiplicities, c.degree);
    } else {
      NCollection_Array1<double> weights(1, static_cast<int>(c.weights.size()));
      for (std::size_t i = 0; i < c.weights.size(); ++i) {
        weights.SetValue(static_cast<int>(i) + 1, c.weights[i]);
      }
      curve = new Geom_BSplineCurve(poles, weights, knots, multiplicities, c.degree);
    }
    BRepBuilderAPI_MakeEdge maker(curve);
    return edge_of(maker);
  }
  case ModelCurveKind::Helix: {
    // (mitcad#385) On the unrolled cylinder of its radius about its axis
    // the helix is a line, rising a pitch per turn; its 3D curve is
    // approximated from that within 1e-7 mm.
    require_positive("helix radius", c.major);
    if (!(std::abs(c.minor) > Precision::Confusion()) || !(c.last > c.first)) {
      throw std::invalid_argument("a path helix needs a pitch and a turn");
    }
    const occ::handle<Geom_Surface> surface = new Geom_CylindricalSurface(gp_Ax3(c.center, c.normal, c.x_axis), c.major);
    const gp_Vec2d direction(1.0, c.minor / kTwoPi);
    const occ::handle<Geom2d_Line> line =
        new Geom2d_Line(gp_Pnt2d(c.first, c.minor * c.first / kTwoPi), gp_Dir2d(direction));
    BRepBuilderAPI_MakeEdge maker(line, surface, 0.0, (c.last - c.first) * direction.Magnitude());
    TopoDS_Edge edge = edge_of(maker);
    const int samples = std::max(32, static_cast<int>(std::ceil((c.last - c.first) / kTwoPi * 32.0)));
    BRepLib::BuildCurves3d(edge, 1.0e-7, GeomAbs_C1, 14, samples);
    return edge;
  }
  case ModelCurveKind::Point:
    break;
  }
  throw std::invalid_argument("a path curve is a point");
}

TopoDS_Wire path_wire(const Path& path) {
  if (path.empty()) {
    throw std::invalid_argument("the path has no curves");
  }
  std::vector<TopoDS_Edge> edges;
  edges.reserve(path.size());
  BRepBuilderAPI_MakeWire wire;
  bool joined = true;
  for (const PathCurve& piece : path) {
    edges.push_back(model_edge(piece.curve));
    if (joined) {
      wire.Add(edges.back());
      joined = wire.IsDone();
    }
  }
  if (joined) {
    return wire.Wire();
  }
  // (mitcad#641) The model chains the curves within 1e-6 of the path's size
  // (features/path.rs), the wire joins them within their vertices' 1e-7: the
  // curves of a body's edges whose vertices are wider than that (a sketch's
  // curves placed by its solve within its tolerance and extruded) end apart
  // by more. Ends within the model's tolerance share one vertex.
  double size = 1.0;
  for (const TopoDS_Edge& edge : edges) {
    for (const TopoDS_Vertex& v : {TopExp::FirstVertex(edge), TopExp::LastVertex(edge)}) {
      const gp_Pnt p = BRep_Tool::Pnt(v);
      size = std::max({size, std::abs(p.X()), std::abs(p.Y()), std::abs(p.Z())});
    }
  }
  const double tolerance = 1.0e-6 * size;
  for (std::size_t i = 1; i < edges.size(); ++i) {
    const double gap = BRep_Tool::Pnt(TopExp::LastVertex(edges[i - 1])).Distance(BRep_Tool::Pnt(TopExp::FirstVertex(edges[i])));
    if (gap > tolerance) {
      throw std::invalid_argument(placed_error("a path curve does not join the curve before it",
                                               "path curve " + path[i].name + " does not join the curve before it"));
    }
  }
  occ::handle<ShapeExtend_WireData> data = new ShapeExtend_WireData();
  for (const TopoDS_Edge& edge : edges) {
    data->Add(edge);
  }
  ShapeFix_Wire fix;
  fix.Load(data);
  fix.ClosedWireMode() = BRep_Tool::Pnt(TopExp::LastVertex(edges.back())).Distance(BRep_Tool::Pnt(TopExp::FirstVertex(edges.front()))) <= tolerance;
  fix.FixConnected(tolerance);
  return data->Wire();
}

Spine::Spine(const TopoDS_Wire& wire) : m_wire(wire) {
  double start = 0.0;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    const TopoDS_Edge& edge = it.Current();
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    Piece piece;
    piece.edge = edge;
    piece.curve = BRep_Tool::Curve(edge, piece.first, piece.last);
    if (piece.curve.IsNull()) {
      throw std::invalid_argument("a path edge has no curve");
    }
    piece.forward = edge.Orientation() != TopAbs_REVERSED;
    piece.start = start;
    piece.length = GCPnts_AbscissaPoint::Length(GeomAdaptor_Curve(piece.curve, piece.first, piece.last));
    start += piece.length;
    m_pieces.push_back(piece);
  }
  if (m_pieces.empty() || start <= Precision::Confusion()) {
    throw std::invalid_argument("the path has no length");
  }
  m_length = start;
  m_closed = point(0.0).Distance(point(m_length)) <= 1e-6 * std::max(1.0, m_length);
}

const Spine::Piece& Spine::piece_at(double s, double& local) const {
  s = std::clamp(s, 0.0, m_length);
  for (const Piece& piece : m_pieces) {
    if (s <= piece.start + piece.length || &piece == &m_pieces.back()) {
      local = std::clamp(s - piece.start, 0.0, piece.length);
      return piece;
    }
  }
  local = 0.0;
  return m_pieces.back();
}

double Spine::parameter(const Piece& piece, double local) const {
  const GeomAdaptor_Curve curve(piece.curve, piece.first, piece.last);
  if (piece.forward) {
    if (local <= 0.0) {
      return piece.first;
    }
    if (local >= piece.length) {
      return piece.last;
    }
    return GCPnts_AbscissaPoint(curve, local, piece.first).Parameter();
  }
  if (local <= 0.0) {
    return piece.last;
  }
  if (local >= piece.length) {
    return piece.first;
  }
  return GCPnts_AbscissaPoint(curve, -local, piece.last).Parameter();
}

gp_Pnt Spine::point(double s) const {
  double local = 0.0;
  const Piece& piece = piece_at(s, local);
  return piece.curve->Value(parameter(piece, local));
}

gp_Dir Spine::tangent(double s) const {
  double local = 0.0;
  const Piece& piece = piece_at(s, local);
  gp_Pnt p;
  gp_Vec v;
  piece.curve->D1(parameter(piece, local), p, v);
  if (v.Magnitude() <= Precision::Confusion()) {
    throw std::invalid_argument("the path has no direction at a point");
  }
  return piece.forward ? gp_Dir(v) : gp_Dir(v.Reversed());
}

std::optional<gp_Dir> Spine::plane_normal() const {
  BRepLib_FindSurface find(m_wire, 1.0e-7, true);
  if (!find.Found()) {
    return std::nullopt;
  }
  const occ::handle<Geom_Plane> plane = occ::handle<Geom_Plane>::DownCast(find.Surface());
  if (plane.IsNull()) {
    return std::nullopt;
  }
  // A straight wire lies in many planes: none of them is its plane.
  bool straight = true;
  for (const Piece& piece : m_pieces) {
    straight = straight && GeomAdaptor_Curve(piece.curve).GetType() == GeomAbs_Line;
  }
  if (straight) {
    return std::nullopt;
  }
  gp_Pln found = plane->Pln();
  if (!find.Location().IsIdentity()) {
    found.Transform(find.Location().Transformation());
  }
  const gp_Dir normal = found.Axis().Direction();
  // (mitcad#1555) The plane is the wire's only when the wire's points lie
  // in it within 1e-7 mm and its tangents within 1e-6 radians, checked
  // here: BRepLib_FindSurface widens its tolerance to the edges' own. A
  // planar path is swept with the plane's normal as a constant binormal,
  // which keeps the section's angle to that normal, so along a path that
  // leaves the plane the section would lean from square by the angle its
  // tangent makes with the plane (0.98 degrees along a path 0.3 mm out of
  // its plane over 100 mm).
  constexpr int samples = 64;
  for (const Piece& piece : m_pieces) {
    for (int i = 0; i <= samples; ++i) {
      gp_Pnt p;
      gp_Vec v;
      piece.curve->D1(piece.first + (piece.last - piece.first) * i / samples, p, v);
      if (found.Distance(p) > 1.0e-7 ||
          (v.Magnitude() > Precision::Confusion() && std::abs(gp_Dir(v).Dot(normal)) > 1.0e-6)) {
        return std::nullopt;
      }
    }
  }
  return normal;
}

double Spine::nearest(const gp_Pnt& near) const {
  // Samples, then golden-section refinement in the best sample's bracket.
  const int count = 64 * static_cast<int>(m_pieces.size());
  const double step = m_length / count;
  int best = 0;
  double distance = std::numeric_limits<double>::infinity();
  for (int i = 0; i <= count; ++i) {
    const double d = point(i * step).Distance(near);
    if (d < distance) {
      distance = d;
      best = i;
    }
  }
  double a = std::max(0.0, (best - 1) * step);
  double b = std::min(m_length, (best + 1) * step);
  const double ratio = (std::sqrt(5.0) - 1.0) / 2.0;
  for (int k = 0; k < 80 && b - a > 1e-12 * std::max(1.0, m_length); ++k) {
    const double c = b - ratio * (b - a);
    const double d = a + ratio * (b - a);
    if (point(c).Distance(near) < point(d).Distance(near)) {
      b = d;
    } else {
      a = c;
    }
  }
  return (a + b) / 2.0;
}

std::vector<double> Spine::crossings(const gp_Pln& plane, double tolerance) const {
  const gp_Vec normal(plane.Axis().Direction());
  const auto side = [&](double s) { return gp_Vec(plane.Location(), point(s)).Dot(normal); };
  const int count = std::max(64, 32 * static_cast<int>(m_pieces.size()));
  const double step = m_length / count;
  const double precision = 1e-14 * std::max(1.0, m_length);
  std::vector<double> found;
  double previous = side(0.0);
  if (previous == 0.0) {
    found.push_back(0.0);
  }
  for (int i = 1; i <= count; ++i) {
    const double s = i == count ? m_length : i * step;
    const double value = side(s);
    if (value == 0.0) {
      // (A closed wire's end is its start.)
      if (!(m_closed && i == count && !found.empty() && found.front() == 0.0)) {
        found.push_back(s);
      }
    } else if (previous != 0.0 && (previous < 0.0) != (value < 0.0)) {
      // Bisection: the sign changes once in this bracket of the samples.
      double a = s - step;
      double b = s;
      double fa = previous;
      for (int k = 0; k < 200 && b - a > precision; ++k) {
        const double m = (a + b) / 2.0;
        const double fm = side(m);
        if (fm == 0.0) {
          a = b = m;
          break;
        }
        if ((fa < 0.0) == (fm < 0.0)) {
          a = m;
          fa = fm;
        } else {
          b = m;
        }
      }
      found.push_back((a + b) / 2.0);
    }
    previous = value;
  }
  if (!m_closed) {
    const gp_Pnt first = point(0.0);
    const gp_Pnt last = point(m_length);
    if (std::abs(side(0.0)) <= tolerance &&
        (found.empty() || point(found.front()).Distance(first) > tolerance)) {
      found.insert(found.begin(), 0.0);
    }
    if (std::abs(side(m_length)) <= tolerance &&
        (found.empty() || point(found.back()).Distance(last) > tolerance)) {
      found.push_back(m_length);
    }
  }
  return found;
}

double Spine::locate(const gp_Pln& plane, const gp_Pnt& near) const {
  const gp_Dir normal = plane.Axis().Direction();
  const auto side = [&](double s) { return gp_Vec(plane.Location(), point(s)).Dot(gp_Vec(normal)); };
  const int count = 64 * static_cast<int>(m_pieces.size());
  const double step = m_length / count;
  const double tolerance = 1e-9 * std::max(1.0, m_length);
  double best = -1.0;
  double distance = std::numeric_limits<double>::infinity();
  const auto consider = [&](double s) {
    const double d = point(s).Distance(near);
    if (d < distance) {
      distance = d;
      best = s;
    }
  };
  double previous = side(0.0);
  if (std::abs(previous) <= tolerance) {
    consider(0.0);
  }
  for (int i = 1; i <= count; ++i) {
    const double s = std::min(m_length, i * step);
    const double value = side(s);
    if (std::abs(value) <= tolerance) {
      consider(s);
    } else if ((previous < 0.0) != (value < 0.0) && std::abs(previous) > tolerance) {
      double a = s - step;
      double b = s;
      double fa = previous;
      for (int k = 0; k < 100 && b - a > 1e-13 * std::max(1.0, m_length); ++k) {
        const double m = (a + b) / 2.0;
        const double fm = side(m);
        if ((fa < 0.0) == (fm < 0.0)) {
          a = m;
          fa = fm;
        } else {
          b = m;
        }
      }
      consider((a + b) / 2.0);
    }
    previous = value;
  }
  return best >= 0.0 ? best : nearest(near);
}

void Spine::add_part(std::vector<TopoDS_Edge>& edges, double from, double to) const {
  // An end nearer than the modelling tolerance to a vertex of the wire is
  // taken at the vertex: no edge that short can be built.
  const double snap = std::max(1e-9 * std::max(1.0, m_length), Precision::Confusion());
  for (const Piece& piece : m_pieces) {
    const double end = piece.start + piece.length;
    if (from <= piece.start + snap && to >= end - snap) {
      edges.push_back(piece.edge);
      continue;
    }
    double a = std::max(from, piece.start);
    double b = std::min(to, end);
    if (a <= piece.start + snap) {
      a = piece.start;
    }
    if (b >= end - snap) {
      b = end;
    }
    if (b - a <= snap) {
      continue;
    }
    double u = parameter(piece, a - piece.start);
    double v = parameter(piece, b - piece.start);
    if (u > v) {
      std::swap(u, v);
    }
    BRepBuilderAPI_MakeEdge maker(piece.curve, u, v);
    if (!maker.IsDone()) {
      throw std::runtime_error("a part of the path could not be built");
    }
    edges.push_back(piece.forward ? maker.Edge() : TopoDS::Edge(maker.Edge().Reversed()));
  }
}

TopoDS_Wire Spine::part(double from, double to) const {
  const double tolerance = 1e-9 * std::max(1.0, m_length);
  if (to - from <= std::max(tolerance, Precision::Confusion())) {
    throw std::invalid_argument("the swept part of the path has no length");
  }
  if (from >= -tolerance && from <= tolerance && to >= m_length - tolerance) {
    return m_wire;
  }
  std::vector<TopoDS_Edge> edges;
  if (from < -tolerance) {
    if (!m_closed) {
      throw std::invalid_argument("the path is not closed");
    }
    add_part(edges, m_length + from, m_length);
    add_part(edges, 0.0, to);
  } else {
    add_part(edges, from, to);
  }
  if (edges.empty()) {
    throw std::invalid_argument("the swept part of the path has no length");
  }
  BRepBuilderAPI_MakeWire wire;
  for (const TopoDS_Edge& edge : edges) {
    wire.Add(edge);
    if (!wire.IsDone()) {
      throw std::runtime_error("the swept part of the path does not join up");
    }
  }
  return wire.Wire();
}

gp_Dir any_normal(const gp_Dir& direction) {
  const gp_Dir axis = std::abs(direction.X()) < 0.6 ? gp_Dir(1, 0, 0)
                      : std::abs(direction.Y()) < 0.6 ? gp_Dir(0, 1, 0)
                                                      : gp_Dir(0, 0, 1);
  return direction.Crossed(axis);
}

std::vector<gp_Ax3> follow_frames(const Spine& spine, double s0, const gp_Dir& normal,
                                  const std::vector<double>& stations) {
  const std::optional<gp_Dir> plane = spine.plane_normal();
  std::vector<gp_Ax3> frames;
  if (plane) {
    // A planar path: the frame keeps its angle to the plane's normal.
    const gp_Dir t0 = spine.tangent(s0);
    const gp_Dir b0 = *plane;
    const gp_Dir n0 = b0.Crossed(t0);
    const double c = gp_Vec(normal).Dot(gp_Vec(n0));
    const double s = gp_Vec(normal).Dot(gp_Vec(b0));
    for (const double station : stations) {
      const gp_Dir t = spine.tangent(station);
      const gp_Vec n = gp_Vec(b0.Crossed(t)) * c + gp_Vec(b0) * s;
      frames.emplace_back(spine.point(station), t, gp_Dir(n));
    }
    return frames;
  }
  // Double reflection (Wang et al. 2008) in small steps from s0 to each
  // station.
  const auto carry = [&spine](double from, double to, gp_Vec r) {
    const int steps = std::max(1, static_cast<int>(std::ceil(std::abs(to - from) / spine.length() * 512.0)));
    gp_Pnt x0 = spine.point(from);
    gp_Vec t0(spine.tangent(from));
    for (int k = 1; k <= steps; ++k) {
      const double s = from + (to - from) * k / steps;
      const gp_Pnt x1 = spine.point(s);
      const gp_Vec t1(spine.tangent(s));
      const gp_Vec v1(x0, x1);
      const double c1 = v1.Dot(v1);
      if (c1 > 0.0) {
        const gp_Vec rl = r - v1 * (2.0 / c1 * v1.Dot(r));
        const gp_Vec tl = t0 - v1 * (2.0 / c1 * v1.Dot(t0));
        const gp_Vec v2 = t1 - tl;
        const double c2 = v2.Dot(v2);
        r = c2 > 0.0 ? rl - v2 * (2.0 / c2 * v2.Dot(rl)) : rl;
      }
      // Keep it at right angles to the tangent and of unit length.
      r = r - t1 * r.Dot(t1);
      r.Normalize();
      x0 = x1;
      t0 = t1;
    }
    return r;
  };
  // From s0 out along the stations both ways (they are in increasing order).
  std::vector<gp_Vec> normals(stations.size(), gp_Vec(normal));
  double at = s0;
  gp_Vec r(normal);
  for (std::size_t i = 0; i < stations.size(); ++i) {
    if (stations[i] >= s0) {
      r = carry(at, stations[i], r);
      at = stations[i];
      normals[i] = r;
    }
  }
  at = s0;
  r = gp_Vec(normal);
  for (std::size_t i = stations.size(); i-- > 0;) {
    if (stations[i] < s0) {
      r = carry(at, stations[i], r);
      at = stations[i];
      normals[i] = r;
    }
  }
  for (std::size_t i = 0; i < stations.size(); ++i) {
    frames.emplace_back(spine.point(stations[i]), spine.tangent(stations[i]), gp_Dir(normals[i]));
  }
  return frames;
}

std::vector<NamedWire> face_loops(const TopoDS_Face& face,
                                  const std::vector<std::pair<TopoDS_Edge, std::string>>& names) {
  const TopoDS_Wire outer = BRepTools::OuterWire(face);
  std::vector<TopoDS_Wire> wires{outer};
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    if (!w.Current().IsSame(outer)) {
      wires.push_back(TopoDS::Wire(w.Current()));
    }
  }
  std::vector<NamedWire> loops;
  for (const TopoDS_Wire& wire : wires) {
    NamedWire loop{wire, {}};
    for (TopExp_Explorer e(wire, TopAbs_EDGE); e.More(); e.Next()) {
      for (const auto& [edge, name] : names) {
        if (edge.IsSame(e.Current())) {
          loop.edges.emplace_back(TopoDS::Edge(e.Current()), name);
        }
      }
    }
    loops.push_back(std::move(loop));
  }
  return loops;
}

PipeMode perpendicular_mode(const Spine& spine) {
  PipeMode mode;
  if (const std::optional<gp_Dir> normal = spine.plane_normal()) {
    mode.kind = PipeMode::Kind::Binormal;
    mode.binormal = *normal;
  }
  return mode;
}

PipeMode perpendicular_mode(const Spine& spine, const Path& path) {
  if (path.size() == 1 && path.front().curve.kind == ModelCurveKind::Helix) {
    PipeMode mode;
    mode.kind = PipeMode::Kind::Binormal;
    mode.binormal = path.front().curve.normal;
    return mode;
  }
  return perpendicular_mode(spine);
}

namespace {

// The curve on the face's surface that the 3D curve lies on within
// `tolerance` over [first, last] (same parameters), or none.
occ::handle<Geom2d_Curve> curve_on(const TopoDS_Face& face, const occ::handle<Geom_Curve>& curve, double first,
                                   double last, double tolerance) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  // Quick rejection: the curve's middle must be near the face.
  const gp_Pnt middle = curve->Value((first + last) / 2.0);
  BRepExtrema_DistShapeShape near(BRepBuilderAPI_MakeVertex(middle).Vertex(), face);
  if (!near.IsDone() || near.Value() > tolerance) {
    return {};
  }
  ShapeConstruct_ProjectCurveOnSurface project;
  project.Init(surface, Precision::Confusion());
  occ::handle<Geom2d_Curve> pcurve;
  try {
    if (!project.Perform(curve, first, last, pcurve) || pcurve.IsNull()) {
      return {};
    }
  } catch (const Standard_Failure&) {
    return {};
  }
  constexpr int kSamples = 64;
  for (int i = 0; i <= kSamples; ++i) {
    const double t = first + (last - first) * i / kSamples;
    const gp_Pnt2d uv = pcurve->Value(t);
    if (curve->Value(t).Distance(surface->Value(uv.X(), uv.Y())) > tolerance) {
      return {};
    }
  }
  return pcurve;
}

} // namespace

TopoDS_Wire on_faces(const TopoDS_Wire& wire, const std::vector<TopoDS_Face>& faces, double tolerance) {
  BRepBuilderAPI_Copy copy(wire, true, false);
  const TopoDS_Wire result = TopoDS::Wire(copy.Shape());
  BRep_Builder builder;
  for (TopExp_Explorer e(result, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      throw std::invalid_argument("a path curve has no geometry");
    }
    bool found = false;
    for (const TopoDS_Face& face : faces) {
      const occ::handle<Geom2d_Curve> pcurve = curve_on(face, curve, first, last, tolerance);
      if (!pcurve.IsNull()) {
        builder.UpdateEdge(edge, pcurve, face, std::max(tolerance, BRep_Tool::Tolerance(edge)));
        found = true;
        break;
      }
    }
    if (!found) {
      throw std::invalid_argument("the path must lie on the guide surface: a path curve lies on none of its faces");
    }
  }
  return result;
}

// Paths beside guide surfaces (mitcad#161).

bool lies_on_faces(const TopoDS_Wire& wire, const std::vector<TopoDS_Face>& faces, double tolerance) {
  for (TopExp_Explorer e(wire, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      return false;
    }
    const bool on = std::any_of(faces.begin(), faces.end(), [&](const TopoDS_Face& face) {
      return !curve_on(face, curve, first, last, tolerance).IsNull();
    });
    if (!on) {
      return false;
    }
  }
  return true;
}

// The unit normal of the faces (with their orientation) at their point
// nearest to `point`, or nothing where none is found.
std::optional<gp_Dir> guide_normal(const TopoDS_Shape& faces, const gp_Pnt& point) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), faces);
  if (!distance.IsDone() || distance.NbSolution() < 1) {
    return std::nullopt;
  }
  const gp_Pnt on = distance.PointOnShape2(1);
  // The face the nearest point lies on (one of those at an edge or a
  // vertex), by the gap to its surface.
  std::optional<gp_Dir> best;
  double gap = std::numeric_limits<double>::infinity();
  for (TopExp_Explorer f(faces, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
    if (surface.IsNull()) {
      continue;
    }
    ShapeAnalysis_Surface analysis(surface);
    const gp_Pnt2d uv = analysis.ValueOfUV(on, Precision::Confusion());
    if (analysis.Gap() >= gap) {
      continue;
    }
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    surface->D1(uv.X(), uv.Y(), p, du, dv);
    const gp_Vec n = du.Crossed(dv);
    if (n.Magnitude() <= gp::Resolution()) {
      continue;
    }
    gap = analysis.Gap();
    best = face.Orientation() == TopAbs_REVERSED ? gp_Dir(n.Reversed()) : gp_Dir(n);
  }
  return best;
}

TopoDS_Wire normal_spine(const TopoDS_Wire& wire, const std::vector<TopoDS_Face>& faces, double offset) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const TopoDS_Face& face : faces) {
    builder.Add(compound, face);
  }
  std::vector<gp_Pnt> points;
  for (BRepTools_WireExplorer e(wire); e.More(); e.Next()) {
    const TopoDS_Edge& edge = e.Current();
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    BRepAdaptor_Curve curve(edge);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    const double length = GCPnts_AbscissaPoint::Length(curve);
    // Every half millimetre, at least 16 points an edge.
    const int n = std::clamp(static_cast<int>(std::ceil(length / 0.5)), 16, 4000);
    const bool reversed = edge.Orientation() == TopAbs_REVERSED;
    // The points of each edge after the first start after its first point
    // (the previous edge's last).
    for (int i = points.empty() ? 0 : 1; i <= n; ++i) {
      const double s = static_cast<double>(i) / n;
      const double t = reversed ? last - (last - first) * s : first + (last - first) * s;
      gp_Pnt p;
      gp_Vec d;
      curve.D1(t, p, d);
      if (d.Magnitude() <= gp::Resolution()) {
        continue;
      }
      const gp_Dir tangent(reversed ? d.Reversed() : d);
      const std::optional<gp_Dir> normal = guide_normal(compound, p);
      if (!normal) {
        throw std::invalid_argument("the guide surface's normal is not found beside the path");
      }
      // At right angles to the path.
      const gp_Vec across = gp_Vec(*normal) - gp_Vec(tangent) * normal->Dot(tangent);
      if (across.Magnitude() <= 1.0e-6) {
        throw std::invalid_argument("the path runs along the guide surface's normal");
      }
      points.push_back(p.Translated(gp_Vec(gp_Dir(across)) * offset));
    }
  }
  if (points.size() < 2) {
    throw std::invalid_argument("the path is too short for its guide surface");
  }
  NCollection_Array1<gp_Pnt> array(1, static_cast<int>(points.size()));
  for (std::size_t i = 0; i < points.size(); ++i) {
    array.SetValue(static_cast<int>(i) + 1, points[i]);
  }
  GeomAPI_PointsToBSpline fit(array, 3, 8, GeomAbs_C2, 1.0e-6);
  if (!fit.IsDone()) {
    throw std::runtime_error("the guide surface's normal along the path could not be built");
  }
  return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(fit.Curve()).Edge()).Wire();
}

void name_caps(FaceNamer& namer, const TopoDS_Shape& section, const NameList& names) {
  if (names.empty() || section.IsNull()) {
    return;
  }
  ShapeMap section_edges;
  TopExp::MapShapes(section, TopAbs_EDGE, section_edges);
  std::vector<TopoDS_Shape> caps;
  for (TopExp_Explorer f(namer.result(), TopAbs_FACE); f.More(); f.Next()) {
    if (namer.named(f.Current())) {
      continue;
    }
    bool bounded = f.Current().IsSame(section);
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); !bounded && e.More(); e.Next()) {
      bounded = section_edges.Contains(e.Current());
    }
    if (bounded) {
      caps.push_back(f.Current());
    }
  }
  if (caps.empty()) {
    // The nearest unnamed face.
    double best = std::numeric_limits<double>::infinity();
    for (TopExp_Explorer f(namer.result(), TopAbs_FACE); f.More(); f.Next()) {
      if (namer.named(f.Current())) {
        continue;
      }
      BRepExtrema_DistShapeShape distance(section, f.Current());
      if (distance.IsDone() && distance.Value() < best) {
        best = distance.Value();
        caps = {f.Current()};
      }
    }
  }
  for (const TopoDS_Shape& cap : caps) {
    for (const std::string& name : names) {
      namer.add(cap, name);
    }
  }
}

TopoDS_Shape oriented_solid(const TopoDS_Shape& shape, const char* what) {
  TopoDS_Solid solid;
  if (shape.ShapeType() == TopAbs_SOLID) {
    solid = TopoDS::Solid(shape);
  } else {
    TopExp_Explorer s(shape, TopAbs_SOLID);
    if (s.More()) {
      solid = TopoDS::Solid(s.Current());
    } else {
      BRep_Builder builder;
      TopoDS_Shell shell;
      TopExp_Explorer sh(shape, TopAbs_SHELL);
      if (sh.More()) {
        shell = TopoDS::Shell(sh.Current());
      } else {
        // Faces closed on themselves (a tube of one face).
        builder.MakeShell(shell);
        for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
          builder.Add(shell, f.Current());
        }
      }
      if (!BRep_Tool::IsClosed(shell)) {
        throw std::runtime_error(std::string(what) + ": no closed shell was built");
      }
      shell.Closed(true);
      builder.MakeSolid(solid);
      builder.Add(solid, shell);
    }
  }
  BRepLib::OrientClosedSolid(solid);
  return solid;
}

void set_mode(BRepOffsetAPI_MakePipeShell& pipe, const PipeMode& mode) {
  switch (mode.kind) {
  case PipeMode::Kind::CorrectedFrenet:
    pipe.SetMode(false);
    break;
  case PipeMode::Kind::Binormal:
    pipe.SetMode(mode.binormal);
    break;
  case PipeMode::Kind::Fixed:
    pipe.SetMode(mode.fixed);
    break;
  case PipeMode::Kind::Support:
    if (!pipe.SetMode(mode.support)) {
      throw std::invalid_argument("the path does not lie on the guide surface");
    }
    break;
  case PipeMode::Kind::Auxiliary:
    // The normal towards the curve beside the spine, met in the plane at
    // right angles to the spine (`normal_spine` puts its points there), or
    // at the same fraction of its length (mitcad#539).
    pipe.SetMode(mode.auxiliary, mode.curvilinear, BRepFill_NoContact);
    break;
  case PipeMode::Kind::Frenet:
    pipe.SetMode(true);
    break;
  }
}

PipeMode frame_mode(bool frenet, const Path& auxiliary, bool curvilinear,
                    const std::optional<gp_Dir>& constant_binormal, const PipeMode& fallback) {
  PipeMode mode = fallback;
  if (frenet) {
    mode.kind = PipeMode::Kind::Frenet;
  } else if (!auxiliary.empty()) {
    mode.kind = PipeMode::Kind::Auxiliary;
    mode.auxiliary = path_wire(auxiliary);
    mode.curvilinear = curvilinear;
  } else if (constant_binormal) {
    mode.kind = PipeMode::Kind::Binormal;
    mode.binormal = *constant_binormal;
  }
  return mode;
}

namespace {

// The loop swept along the spine (pipe_shell), with corners of the spine
// made by `transition`.
ShapePtr pipe_shell_with(const NamedWire& loop, const TopoDS_Wire& spine, const PipeMode& mode,
                         const NameList& first, const NameList& last, double tolerance,
                         BRepBuilderAPI_TransitionMode transition) {
  BRepOffsetAPI_MakePipeShell pipe(spine);
  pipe.SetTolerance(tolerance, tolerance, 1.0e-2);
  set_mode(pipe, mode);
  pipe.SetTransitionMode(transition);
  pipe.Add(loop.wire, false, false);
  detail::build(pipe);
  if (!pipe.IsDone()) {
    throw std::runtime_error("the profile could not be swept along the path");
  }
  if (!pipe.MakeSolid()) {
    throw std::runtime_error("the swept profile does not close into a solid");
  }
  FaceNamer namer(oriented_solid(pipe.Shape(), "the sweep"));
  for (const auto& [edge, name] : loop.edges) {
    namer.generated(pipe, edge, name);
  }
  name_caps(namer, pipe.FirstShape(), first);
  name_caps(namer, pipe.LastShape(), last);
  return namer.shape();
}

// A valid solid that encloses a volume (mitcad#385: corners whose sides
// fold through each other can pass the shape check, the solid then
// enclosing a negative volume once oriented).
bool sound(const Shape& shape) {
  if (!is_valid(shape.occt())) {
    return false;
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.Mass() > 0.0;
}

// (mitcad#1564) Whether pipe_shell says what each try of the corners gave
// on the standard error, as port patch 1666's corners do: while
// MITCAD_TRACE_PIPE_CORNERS is set (read each time, so that a test can set
// it round one pipe).
bool tracing_corners() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, "MITCAD_TRACE_PIPE_CORNERS") != 0 || value == nullptr) {
    return false;
  }
  const bool on = value[0] != '\0';
  std::free(value);
  return on;
#else
  const char* value = std::getenv("MITCAD_TRACE_PIPE_CORNERS");
  return value != nullptr && value[0] != '\0';
#endif
}

// The trace of a try that built a shape: the checker's first problem with
// it (what makes it unsound) and its volume.
void trace_try(const char* what, const Shape& shape) {
  if (!tracing_corners()) {
    return;
  }
  std::string verdict = "valid";
  try {
    require_valid(shape.occt(), "it");
  } catch (const Cancelled&) {
    throw;
  } catch (const std::exception& problem) {
    verdict = problem.what();
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  std::fprintf(stderr, "pipe try, %s corners: %s, volume %.6f\n", what, verdict.c_str(), props.Mass());
  if (verdict != "valid") {
    // The edges whose curves on a face the checker refuses: where they lie,
    // their curve's and the face's surface's types, their tolerance and how
    // far the curve on the face lies from the edge's curve.
    const BRepCheck_Analyzer analyzer(shape.occt(), true, true);
    int listed = 0;
    for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More() && listed < 8; f.Next()) {
      const TopoDS_Face face = TopoDS::Face(f.Current());
      for (TopExp_Explorer e(face, TopAbs_EDGE); e.More() && listed < 8; e.Next()) {
        const TopoDS_Edge edge = TopoDS::Edge(e.Current());
        const occ::handle<BRepCheck_Result>& result = analyzer.Result(edge);
        if (result.IsNull() || !result->IsStatusOnShape(face)) {
          continue;
        }
        bool bad = false;
        for (const BRepCheck_Status status : result->StatusOnShape(face)) {
          bad = bad || status != BRepCheck_NoError;
        }
        if (!bad) {
          continue;
        }
        ++listed;
        const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
        double first = 0.0;
        double last = 0.0;
        const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
        double off = 0.0;
        gp_Pnt middle;
        if (!curve.IsNull() && !BRep_Tool::Degenerated(edge)) {
          const BRepAdaptor_Curve on_face(edge, face);
          const BRepAdaptor_Curve in_space(edge);
          middle = in_space.Value(0.5 * (in_space.FirstParameter() + in_space.LastParameter()));
          for (int k = 0; k <= 16; ++k) {
            const double t =
                in_space.FirstParameter() + (in_space.LastParameter() - in_space.FirstParameter()) * k / 16.0;
            off = std::max(off, in_space.Value(t).Distance(on_face.Value(t)));
          }
        }
        const Geom_Curve *curve_object = curve.get();
        const Geom_Surface *surface_object = surface.get();
        std::fprintf(stderr,
                     "  refused edge on a face: middle (%.4f, %.4f, %.4f), "
                     "curve %s%s, face %s, tolerance %.3e, "
                     "curve on the face off by %.3e\n",
                     middle.X(), middle.Y(), middle.Z(),
                     curve.IsNull() ? "none" : typeid(*curve_object).name(),
                     BRep_Tool::Degenerated(edge) ? " (degenerated)" : "",
                     surface.IsNull() ? "none" : typeid(*surface_object).name(),
                     BRep_Tool::Tolerance(edge), off);
      }
    }
    // The wires the checker refuses on their face: the face's surface, and
    // each edge's ends (as the wire runs it) in space and on the face.
    int wires = 0;
    for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More() && wires < 3; f.Next()) {
      const TopoDS_Face face = TopoDS::Face(f.Current());
      for (TopExp_Explorer w(face, TopAbs_WIRE); w.More() && wires < 3; w.Next()) {
        const occ::handle<BRepCheck_Result>& result = analyzer.Result(w.Current());
        if (result.IsNull() || !result->IsStatusOnShape(face)) {
          continue;
        }
        bool bad = false;
        for (const BRepCheck_Status status : result->StatusOnShape(face)) {
          bad = bad || status != BRepCheck_NoError;
        }
        if (!bad) {
          continue;
        }
        ++wires;
        const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
        const Geom_Surface *surface_object = surface.get();
        std::fprintf(stderr, "  refused wire on a face %s (%s):\n",
                     surface.IsNull() ? "none" : typeid(*surface_object).name(),
                     face.Orientation() == TopAbs_REVERSED ? "reversed"
                                                           : "forward");
        for (TopoDS_Iterator e(w.Current()); e.More(); e.Next()) {
          const TopoDS_Edge edge = TopoDS::Edge(e.Value());
          double first = 0.0;
          double last = 0.0;
          const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
          TopoDS_Vertex start;
          TopoDS_Vertex end;
          TopExp::Vertices(edge, start, end, true);
          const bool reversed = edge.Orientation() == TopAbs_REVERSED;
          gp_Pnt2d uv0;
          gp_Pnt2d uv1;
          if (!pcurve.IsNull()) {
            uv0 = pcurve->Value(reversed ? last : first);
            uv1 = pcurve->Value(reversed ? first : last);
          }
          const gp_Pnt p0 = start.IsNull() ? gp_Pnt() : BRep_Tool::Pnt(start);
          const gp_Pnt p1 = end.IsNull() ? gp_Pnt() : BRep_Tool::Pnt(end);
          std::fprintf(stderr,
                       "    edge%s%s: from (%.4f, %.4f, %.4f) tol %.2e to (%.4f, %.4f, %.4f) tol %.2e, on the face "
                       "(%.6f, %.6f) to (%.6f, %.6f)%s\n",
                       BRep_Tool::Degenerated(edge) ? " (degenerated)" : "", reversed ? " reversed" : "", p0.X(),
                       p0.Y(), p0.Z(), start.IsNull() ? 0.0 : BRep_Tool::Tolerance(start), p1.X(), p1.Y(), p1.Z(),
                       end.IsNull() ? 0.0 : BRep_Tool::Tolerance(end), uv0.X(), uv0.Y(), uv1.X(), uv1.Y(),
                       pcurve.IsNull() ? " (no curve on the face)" : "");
        }
      }
    }
  }
  std::fflush(stderr);
}

// The trace of a try that threw.
void trace_try_error(const char* what, const std::exception_ptr& error) {
  if (!tracing_corners() || !error) {
    return;
  }
  std::string message = "an unknown error";
  try {
    std::rethrow_exception(error);
  } catch (const Standard_Failure& failure) {
    message = std::string(typeid(failure).name()) + ": " + failure.what();
  } catch (const std::exception& failure) {
    message = failure.what();
  } catch (...) {
  }
  std::fprintf(stderr, "pipe try, %s corners: threw %s\n", what, message.c_str());
  std::fflush(stderr);
}

// The loop of one closed edge on a periodic curve as two halves of it,
// their joints a quarter of the period on from its seam, each half with
// the edge's name; none for another loop.
std::optional<NamedWire> halved(const NamedWire& loop) {
  std::vector<TopoDS_Edge> edges;
  for (TopExp_Explorer e(loop.wire, TopAbs_EDGE); e.More(); e.Next()) {
    edges.push_back(TopoDS::Edge(e.Current()));
  }
  if (edges.size() != 1) {
    return std::nullopt;
  }
  TopLoc_Location location;
  double from = 0.0;
  double to = 0.0;
  occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edges.front(), location, from, to);
  if (curve.IsNull() || !curve->IsPeriodic() || std::abs(to - from - curve->Period()) > 1.0e-9) {
    return std::nullopt;
  }
  if (!location.IsIdentity()) {
    curve = occ::handle<Geom_Curve>::DownCast(curve->Transformed(location.Transformation()));
  }
  // Two halves from a quarter of the period on: the ends of their shared
  // vertices off the seam's line.
  const double quarter = curve->Period() / 4.0;
  BRepBuilderAPI_MakeEdge one(curve, from + quarter, from + 3.0 * quarter);
  if (!one.IsDone()) {
    return std::nullopt;
  }
  BRepBuilderAPI_MakeEdge two(curve, one.Vertex2(), one.Vertex1(), from + 3.0 * quarter, to + quarter);
  if (!two.IsDone()) {
    return std::nullopt;
  }
  BRepBuilderAPI_MakeWire wire(one.Edge(), two.Edge());
  if (!wire.IsDone()) {
    return std::nullopt;
  }
  NamedWire out;
  out.wire = wire.Wire();
  if (edges.front().Orientation() == TopAbs_REVERSED) {
    out.wire.Reverse();
  }
  for (const auto& [old, name] : loop.edges) {
    if (old.IsSame(edges.front())) {
      out.edges.emplace_back(one.Edge(), name);
      out.edges.emplace_back(two.Edge(), name);
    } else {
      out.edges.emplace_back(old, name);
    }
  }
  return out;
}

} // namespace

ShapePtr pipe_shell(const NamedWire& loop, const TopoDS_Wire& spine, const PipeMode& mode,
                    const NameList& first, const NameList& last, double tolerance, PipeCorners corners) {
  int edges = 0;
  for (TopExp_Explorer e(spine, TopAbs_EDGE); e.More(); e.Next()) {
    ++edges;
  }
  // Corners of the spine are mitered (the sides run on until they meet) or,
  // asked for (mitcad#1448), rounded (the section turns about the corner).
  // Where that cannot be built (a slight kink, whose mitre is a sliver)
  // the other kind is tried, else the section turns through them.
  // (mitcad#1555) Turning through a corner, the section keeps the frame it
  // had before it, so along the next edge it leans from square by the
  // corner's angle. OCCT's mitres and roundings trim the two sides against
  // each other; between sides swept along curves other than lines and
  // circles this works since port patch 1661 (mitcad#1559) where their
  // intersection comes whole, and since 1663 and 1664 a mitre is cut along
  // the bisector plane where it does not; since 1666 (mitcad#1564) a
  // rounded corner's sides likewise where they overlap inside the corner.
  const bool rounded = corners == PipeCorners::Rounded;
  const BRepBuilderAPI_TransitionMode asked = rounded ? BRepBuilderAPI_RoundCorner : BRepBuilderAPI_RightCorner;
  const BRepBuilderAPI_TransitionMode instead = rounded ? BRepBuilderAPI_RightCorner : BRepBuilderAPI_RoundCorner;
  if (edges < 2) {
    return pipe_shell_with(loop, spine, mode, first, last, tolerance, asked);
  }
  ShapePtr built;
  std::exception_ptr error;
  const char* const first_try = rounded ? "rounded" : "mitred";
  try {
    built = pipe_shell_with(loop, spine, mode, first, last, tolerance, asked);
    trace_try(first_try, *built);
    if (sound(*built)) {
      return built;
    }
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    throw_if_cancelled();
    error = std::current_exception();
    trace_try_error(first_try, error);
  }
  // (mitcad#385) The mitres of a section of one closed edge can fail to
  // build from it (an ellipse whose seam vertex, at an end of its major
  // axis, lies along the normal of the path's plane): the same section as
  // two halves joined a quarter turn from that seam builds them (the
  // corners asked for, rounded ones too).
  if (const std::optional<NamedWire> other = halved(loop)) {
    const char* const halved_try = rounded ? "rounded, halved section" : "mitred, halved section";
    try {
      ShapePtr again = pipe_shell_with(*other, spine, mode, first, last, tolerance, asked);
      trace_try(halved_try, *again);
      if (sound(*again)) {
        return again;
      }
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
      throw_if_cancelled();
      trace_try_error(halved_try, std::current_exception());
    }
  }
  for (const BRepBuilderAPI_TransitionMode transition : {instead, BRepBuilderAPI_Transformed}) {
    const char* const next_try = transition == BRepBuilderAPI_RoundCorner   ? "rounded"
                                 : transition == BRepBuilderAPI_RightCorner ? "mitred"
                                                                            : "turned through";
    try {
      ShapePtr other = pipe_shell_with(loop, spine, mode, first, last, tolerance, transition);
      trace_try(next_try, *other);
      if (sound(*other)) {
        return other;
      }
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
      throw_if_cancelled();
      trace_try_error(next_try, std::current_exception());
    }
  }
  if (error) {
    std::rethrow_exception(error);
  }
  return built;
}

ShapePtr through_sections(const std::vector<TopoDS_Shape>& sections, const NamedWire& named,
                          const NameList& first, const NameList& last, bool ruled, bool compatible, bool solid) {
  BRepOffsetAPI_ThruSections loft(solid, ruled, 1.0e-6);
  loft.CheckCompatibility(compatible);
  loft.SetMutableInput(false);
  for (const TopoDS_Shape& section : sections) {
    if (section.ShapeType() == TopAbs_VERTEX) {
      loft.AddVertex(TopoDS::Vertex(section));
    } else {
      loft.AddWire(TopoDS::Wire(section));
    }
  }
  detail::build(loft);
  if (!loft.IsDone()) {
    throw std::runtime_error("no surface could be built through the sections");
  }
  FaceNamer namer(solid ? oriented_solid(loft.Shape(), "the loft") : loft.Shape());
  for (const auto& [edge, name] : named.edges) {
    namer.generated(loft, edge, name);
  }
  if (solid && !first.empty() && sections.front().ShapeType() != TopAbs_VERTEX) {
    name_caps(namer, loft.FirstShape(), first);
  }
  if (solid && !last.empty() && sections.back().ShapeType() != TopAbs_VERTEX) {
    name_caps(namer, loft.LastShape(), last);
  }
  return namer.shape();
}

ShapePtr with_holes(const ShapePtr& outer, const std::vector<ShapePtr>& holes, const char* what) {
  if (holes.empty()) {
    return outer;
  }
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(outer->occt());
  NCollection_List<TopoDS_Shape> tools;
  for (const ShapePtr& hole : holes) {
    tools.Append(hole->occt());
  }
  cut.SetArguments(arguments);
  cut.SetTools(tools);
  cut.SetNonDestructive(true);
  detail::build(cut);
  if (!cut.IsDone() || cut.HasErrors()) {
    throw std::runtime_error(std::string(what) + ": the holes of the profile could not be cut");
  }
  FaceNamer namer(cut.Shape());
  namer.carry(cut, *outer);
  for (const ShapePtr& hole : holes) {
    namer.carry(cut, *hole);
  }
  return namer.shape();
}

} // namespace mitcad::geometry::detail
