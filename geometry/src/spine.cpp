// SPDX-License-Identifier: MIT
#include "spine.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepLib_FindSurface.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
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
  case ModelCurveKind::Point:
    break;
  }
  throw std::invalid_argument("a path curve is a point");
}

TopoDS_Wire path_wire(const Path& path) {
  if (path.empty()) {
    throw std::invalid_argument("the path has no curves");
  }
  BRepBuilderAPI_MakeWire wire;
  for (const PathCurve& piece : path) {
    wire.Add(model_edge(piece.curve));
    if (!wire.IsDone()) {
      throw std::invalid_argument("path curve " + piece.name + " does not join the curve before it");
    }
  }
  return wire.Wire();
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
  gp_Dir normal = plane->Pln().Axis().Direction();
  if (!find.Location().IsIdentity()) {
    normal.Transform(find.Location().Transformation());
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
  const double tolerance = 1e-9 * std::max(1.0, m_length);
  for (const Piece& piece : m_pieces) {
    const double a = std::max(from, piece.start);
    const double b = std::min(to, piece.start + piece.length);
    if (b - a <= tolerance) {
      continue;
    }
    if (a <= piece.start + tolerance && b >= piece.start + piece.length - tolerance) {
      edges.push_back(piece.edge);
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
  if (to - from <= tolerance) {
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

ShapePtr pipe_shell(const NamedWire& loop, const TopoDS_Wire& spine, const PipeMode& mode,
                    const NameList& first, const NameList& last, double tolerance) {
  BRepOffsetAPI_MakePipeShell pipe(spine);
  pipe.SetTolerance(tolerance, tolerance, 1.0e-2);
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
  }
  pipe.SetTransitionMode(BRepBuilderAPI_RightCorner);
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

ShapePtr through_sections(const std::vector<TopoDS_Shape>& sections, const NamedWire& named,
                          const NameList& first, const NameList& last, bool ruled, bool compatible) {
  BRepOffsetAPI_ThruSections loft(true, ruled, 1.0e-6);
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
  FaceNamer namer(oriented_solid(loft.Shape(), "the loft"));
  for (const auto& [edge, name] : named.edges) {
    namer.generated(loft, edge, name);
  }
  if (!first.empty() && sections.front().ShapeType() != TopAbs_VERTEX) {
    name_caps(namer, loft.FirstShape(), first);
  }
  if (!last.empty() && sections.back().ShapeType() != TopAbs_VERTEX) {
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
