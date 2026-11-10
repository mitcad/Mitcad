// SPDX-License-Identifier: MIT
#include "mitcad/geometry/profile.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <optional>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <ElCLib.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Circle.hxx>
#include <NCollection_Array1.hxx>
#include <Precision.hxx>
#include <ShapeExtend_WireData.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Wire.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

using detail::require_finite;
using detail::require_positive;

std::invalid_argument segment_error(const Segment& segment, const std::string& problem) {
  return std::invalid_argument("segment " + (segment.name.empty() ? "?" : segment.name) + ": " +
                               problem);
}

// Local axes at a sketch point, the x direction rotated in the sketch plane.
gp_Ax2 axes_at(const Frame& frame, const gp_Pnt2d& center, double rotation) {
  const gp_Vec x = gp_Vec(frame.x_axis) * std::cos(rotation) + gp_Vec(frame.y_axis) * std::sin(rotation);
  return gp_Ax2(frame.point(center), frame.normal(), gp_Dir(x));
}

// Counter-clockwise end angle after the start angle.
double end_after(double start, double end) {
  while (end <= start) {
    end += kTwoPi;
  }
  return end;
}

TopoDS_Edge edge_from(BRepBuilderAPI_MakeEdge& maker, const Segment& segment) {
  if (!maker.IsDone()) {
    throw segment_error(segment, "the curve could not be built");
  }
  return maker.Edge();
}

TopoDS_Edge make_edge(const Frame& frame, const Segment& s) {
  switch (s.kind) {
  case SegmentKind::Line: {
    const gp_Pnt a = frame.point(s.start);
    const gp_Pnt b = frame.point(s.end);
    if (a.Distance(b) <= Precision::Confusion()) {
      throw segment_error(s, "the line has zero length");
    }
    BRepBuilderAPI_MakeEdge maker(a, b);
    return edge_from(maker, s);
  }
  case SegmentKind::Circle:
  case SegmentKind::Arc: {
    require_positive("radius", s.radius);
    // A whole circle starts where `rotation` says (its seam), an arc's angles are absolute.
    const double turn = s.kind == SegmentKind::Circle ? s.rotation : 0.0;
    const gp_Circ circle(axes_at(frame, s.center, turn), s.radius);
    if (s.kind == SegmentKind::Circle) {
      BRepBuilderAPI_MakeEdge maker(circle);
      return edge_from(maker, s);
    }
    require_finite("start angle", s.start_angle);
    require_finite("end angle", s.end_angle);
    BRepBuilderAPI_MakeEdge maker(circle, s.start_angle, end_after(s.start_angle, s.end_angle));
    return edge_from(maker, s);
  }
  case SegmentKind::Ellipse:
  case SegmentKind::EllipseArc: {
    require_positive("minor radius", s.minor_radius);
    if (s.radius < s.minor_radius) {
      throw segment_error(s, "the major radius is smaller than the minor radius");
    }
    const gp_Elips ellipse(axes_at(frame, s.center, s.rotation), s.radius, s.minor_radius);
    if (s.kind == SegmentKind::Ellipse) {
      BRepBuilderAPI_MakeEdge maker(ellipse);
      return edge_from(maker, s);
    }
    BRepBuilderAPI_MakeEdge maker(ellipse, s.start_angle, end_after(s.start_angle, s.end_angle));
    return edge_from(maker, s);
  }
  case SegmentKind::BSpline: {
    if (s.poles.size() < 2 || s.knots.size() < 2 || s.knots.size() != s.multiplicities.size() ||
        s.degree < 1 || (!s.weights.empty() && s.weights.size() != s.poles.size())) {
      throw segment_error(s, "inconsistent B-spline data");
    }
    NCollection_Array1<gp_Pnt> poles(1, static_cast<int>(s.poles.size()));
    for (std::size_t i = 0; i < s.poles.size(); ++i) {
      poles.SetValue(static_cast<int>(i) + 1, frame.point(s.poles[i]));
    }
    NCollection_Array1<double> knots(1, static_cast<int>(s.knots.size()));
    NCollection_Array1<int> multiplicities(1, static_cast<int>(s.knots.size()));
    for (std::size_t i = 0; i < s.knots.size(); ++i) {
      knots.SetValue(static_cast<int>(i) + 1, s.knots[i]);
      multiplicities.SetValue(static_cast<int>(i) + 1, s.multiplicities[i]);
    }
    occ::handle<Geom_BSplineCurve> curve;
    if (s.weights.empty()) {
      curve = new Geom_BSplineCurve(poles, knots, multiplicities, s.degree, s.periodic);
    } else {
      NCollection_Array1<double> weights(1, static_cast<int>(s.weights.size()));
      for (std::size_t i = 0; i < s.weights.size(); ++i) {
        weights.SetValue(static_cast<int>(i) + 1, s.weights[i]);
      }
      curve = new Geom_BSplineCurve(poles, weights, knots, multiplicities, s.degree, s.periodic);
    }
    BRepBuilderAPI_MakeEdge maker(curve);
    return edge_from(maker, s);
  }
  }
  throw segment_error(s, "unknown segment kind");
}

gp_Pln plane_of(const Frame& frame) { return gp_Pln(gp_Ax3(frame.origin, frame.normal(), frame.x_axis)); }

// Check the curve endpoints before OCCT merges nearby vertices using their
// individual tolerances. The explicit gap limit is a distance between curves,
// not the sum of the two vertex tolerances. Segment directions may be reversed.
void require_loop_gap(const Frame& frame, const Loop& loop, double max_gap) {
  if (loop.segments.empty()) {
    throw std::invalid_argument("a profile loop has no segments");
  }
  std::vector<std::pair<gp_Pnt, gp_Pnt>> ends;
  for (const Segment& segment : loop.segments) {
    throw_if_cancelled();
    const BRepAdaptor_Curve curve(make_edge(frame, segment));
    ends.emplace_back(curve.Value(curve.FirstParameter()), curve.Value(curve.LastParameter()));
  }
  const auto closes = [&](bool reversed) {
    const gp_Pnt start = reversed ? ends.front().second : ends.front().first;
    gp_Pnt current = reversed ? ends.front().first : ends.front().second;
    for (std::size_t i = 1; i < ends.size(); ++i) {
      const double forward = current.Distance(ends[i].first);
      const double backward = current.Distance(ends[i].second);
      if (std::min(forward, backward) > max_gap) {
        return false;
      }
      current = forward <= backward ? ends[i].second : ends[i].first;
    }
    return current.Distance(start) <= max_gap;
  };
  if (!closes(false) && !closes(true)) {
    throw std::invalid_argument("a profile loop is not closed within its gap tolerance");
  }
}

// Builds the loop's wire and records the name of each of its edges.
TopoDS_Wire make_wire(const Frame& frame, const Loop& loop,
                      std::vector<std::pair<TopoDS_Edge, std::string>>& names) {
  if (loop.segments.empty()) {
    throw std::invalid_argument("a profile loop has no segments");
  }
  BRepBuilderAPI_MakeWire wire;
  for (const Segment& segment : loop.segments) {
    wire.Add(make_edge(frame, segment));
    if (!wire.IsDone()) {
      throw segment_error(segment, "it does not connect to the previous segment of its loop");
    }
    names.emplace_back(wire.Edge(), segment.name);
  }
  const TopoDS_Wire result = wire.Wire();
  if (!BRep_Tool::IsClosed(result)) {
    throw std::invalid_argument("a profile loop is not closed");
  }
  return result;
}

// True when the wire runs clockwise around the sketch normal: the signed
// area of points along its edges, in the wire's order and directions. (The
// classification of the plane's infinite point against a face of the wire
// alone misjudges loops with collinear pieces in a row.)
bool is_clockwise(const gp_Pln& plane, const TopoDS_Wire& wire) {
  const gp_Ax3& axes = plane.Position();
  const gp_Vec x(axes.XDirection());
  const gp_Vec y(axes.YDirection());
  const gp_Pnt origin = axes.Location();
  double area = 0.0;
  bool first = true;
  gp_Pnt2d previous;
  gp_Pnt2d start;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    const BRepAdaptor_Curve curve(it.Current());
    const bool forward = it.Current().Orientation() != TopAbs_REVERSED;
    const int n = curve.GetType() == GeomAbs_Line ? 1 : 32;
    for (int k = 0; k <= n; ++k) {
      const double s = static_cast<double>(k) / n;
      const double t = forward ? curve.FirstParameter() + s * (curve.LastParameter() - curve.FirstParameter())
                               : curve.LastParameter() - s * (curve.LastParameter() - curve.FirstParameter());
      const gp_Vec v(origin, curve.Value(t));
      const gp_Pnt2d p(v.Dot(x), v.Dot(y));
      if (first) {
        start = p;
        first = false;
      } else {
        area += previous.X() * p.Y() - p.X() * previous.Y();
      }
      previous = p;
    }
  }
  if (first) {
    throw std::invalid_argument("a profile loop does not bound a planar region");
  }
  area += previous.X() * start.Y() - start.X() * previous.Y();
  return area < 0.0;
}

// The end points of a segment's edge.
std::pair<gp_Pnt, gp_Pnt> edge_ends(const Frame& frame, const Segment& segment) {
  const BRepAdaptor_Curve curve(make_edge(frame, segment));
  return {curve.Value(curve.FirstParameter()), curve.Value(curve.LastParameter())};
}

// How near a point must lie to a hole's boundary to be where it touches the
// outer boundary: sketch points that meet are well inside it.
constexpr double kTouchGap = 1e-5;

// The hole's one segment when it is a whole circle, or an arc that starts and
// ends where the circle was cut by the curve it touches, and the outer
// boundary has a vertex on it; else null. A hole of that kind meets the outer
// wire at the vertex, which they must share: separate vertices at one place
// make a face with intersecting wires.
const Segment* touching_circle(const Frame& frame, const Loop& hole, const gp_Pnt& vertex) {
  if (hole.segments.size() != 1) {
    return nullptr;
  }
  const Segment& s = hole.segments.front();
  if (s.kind == SegmentKind::Circle) {
    return std::abs(vertex.Distance(frame.point(s.center)) - s.radius) <= kTouchGap ? &s : nullptr;
  }
  if (s.kind == SegmentKind::Arc) {
    const auto ends = edge_ends(frame, s);
    return ends.first.Distance(vertex) <= kTouchGap && ends.second.Distance(vertex) <= kTouchGap ? &s
                                                                                                  : nullptr;
  }
  return nullptr;
}

// The hole's wire when it touches the outer wire at one of its vertices: the
// circle's one edge from the vertex round to it, built on the outer wire's
// vertex. Null when it does not.
std::optional<TopoDS_Wire> touching_hole_wire(const Frame& frame, const Loop& hole,
                                              const TopoDS_Wire& outer,
                                              std::vector<std::pair<TopoDS_Edge, std::string>>& names) {
  for (TopExp_Explorer it(outer, TopAbs_VERTEX); it.More(); it.Next()) {
    const TopoDS_Vertex vertex = TopoDS::Vertex(it.Current());
    const gp_Pnt point = BRep_Tool::Pnt(vertex);
    const Segment* s = touching_circle(frame, hole, point);
    if (s == nullptr) {
      continue;
    }
    // The circle's own start at the vertex, so that the edge runs over its
    // parameters 0 to 2 pi: an edge past 2 pi (a touch at 270 degrees, as
    // the sketch's arc gives it) made OCCT's face check clip its pcurve's
    // range and take the wires for crossing (mitcad#235).
    const gp_Circ about(axes_at(frame, s->center, 0.0), s->radius);
    const gp_Circ circle(axes_at(frame, s->center, ElCLib::Parameter(about, point)), s->radius);
    const occ::handle<Geom_Circle> curve = new Geom_Circle(circle);
    BRepBuilderAPI_MakeEdge maker(curve, vertex, vertex, 0.0, kTwoPi);
    const TopoDS_Edge edge = edge_from(maker, *s);
    BRepBuilderAPI_MakeWire wire(edge);
    names.emplace_back(edge, s->name);
    return wire.Wire();
  }
  return std::nullopt;
}

// The tolerance of a vertex where a hole touches the outer boundary: where
// two curves touch tangentially (a circle on a line, a circle inside
// another), OCCT's face check finds them within its intersection tolerance
// (1e-10) for some micrometres on both sides of the point and takes the
// wires for crossing unless that lies within the shared vertex's tolerance
// (mitcad#235: 2e-5 mm for a circle of 1.75 mm on a line, 5e-5 mm for
// circles of 5 and 8 mm).
constexpr double kTouchTolerance = 1e-4;

// The hole's wire with its vertices that lie on vertices of the outer wire
// (within kTouchGap) replaced by those (a hole of arcs whose vertex is where
// it touches the outer circle inside, mitcad#235): wires meeting at
// separate vertices at one place make a face with intersecting wires. The
// names follow the edges. False when no vertex is shared.
bool share_touching_vertices(TopoDS_Wire& hole, const TopoDS_Wire& outer,
                             std::vector<std::pair<TopoDS_Edge, std::string>>& names) {
  std::vector<TopoDS_Vertex> targets;
  for (TopExp_Explorer it(outer, TopAbs_VERTEX); it.More(); it.Next()) {
    targets.push_back(TopoDS::Vertex(it.Current()));
  }
  occ::handle<BRepTools_ReShape> reshape = new BRepTools_ReShape();
  bool shared = false;
  ShapeMap seen;
  for (TopExp_Explorer it(hole, TopAbs_VERTEX); it.More(); it.Next()) {
    const TopoDS_Vertex vertex = TopoDS::Vertex(it.Current());
    if (seen.Contains(vertex)) {
      continue;
    }
    seen.Add(vertex);
    const gp_Pnt point = BRep_Tool::Pnt(vertex);
    for (const TopoDS_Vertex& target : targets) {
      if (target.IsSame(vertex)) {
        break;
      }
      if (BRep_Tool::Pnt(target).Distance(point) <= kTouchGap) {
        reshape->Replace(vertex.Oriented(TopAbs_FORWARD), target.Oriented(TopAbs_FORWARD));
        shared = true;
        break;
      }
    }
  }
  if (!shared) {
    return false;
  }
  for (auto& [edge, name] : names) {
    const TopoDS_Shape image = reshape->Apply(edge);
    if (!image.IsNull() && image.ShapeType() == TopAbs_EDGE) {
      edge = TopoDS::Edge(image);
    }
  }
  hole = TopoDS::Wire(reshape->Apply(hole));
  return true;
}

// Gives the vertices that wires share (the outer wire with holes, holes
// with each other) kTouchTolerance.
void widen_touching_vertices(const std::vector<TopoDS_Wire>& wires) {
  if (wires.size() < 2) {
    return;
  }
  ShapeMap earlier;
  BRep_Builder builder;
  for (std::size_t i = 0; i < wires.size(); ++i) {
    ShapeMap own;
    for (TopExp_Explorer it(wires[i], TopAbs_VERTEX); it.More(); it.Next()) {
      const TopoDS_Vertex& vertex = TopoDS::Vertex(it.Current());
      if (earlier.Contains(vertex) && BRep_Tool::Tolerance(vertex) < kTouchTolerance) {
        builder.UpdateVertex(vertex, kTouchTolerance);
      }
      own.Add(vertex);
    }
    for (int k = 1; k <= own.Extent(); ++k) {
      earlier.Add(own(k));
    }
  }
}

// The region's face from its segments as they are.
ProfileFace exact_profile(const Frame& frame, const Region& region) {
  const gp_Pln plane = plane_of(frame);
  std::vector<std::pair<TopoDS_Edge, std::string>> names;
  std::vector<TopoDS_Wire> wires;
  bool pinched = false;
  for (std::size_t i = 0; i < region.loops.size(); ++i) {
    std::optional<TopoDS_Wire> touching;
    if (i > 0) {
      touching = touching_hole_wire(frame, region.loops[i], wires.front(), names);
    }
    TopoDS_Wire wire = touching ? *touching : make_wire(frame, region.loops[i], names);
    if (i > 0 && !touching) {
      // The outer wire's vertices, and those of the holes before it: holes
      // touch each other at corners too (a QR code's light modules,
      // mitcad#473).
      bool shares = false;
      for (const TopoDS_Wire& earlier : wires) {
        shares = share_touching_vertices(wire, earlier, names) || shares;
      }
      if (shares) {
        touching = wire;
      }
    }
    pinched = pinched || touching.has_value();
    // Material on the left: the outer boundary counter-clockwise, holes clockwise.
    if (is_clockwise(plane, wire) == (i == 0)) {
      wire = TopoDS::Wire(wire.Reversed());
    }
    wires.push_back(wire);
  }
  if (pinched) {
    widen_touching_vertices(wires);
  }
  BRepBuilderAPI_MakeFace maker(plane, wires.front(), true);
  for (std::size_t i = 1; i < wires.size(); ++i) {
    maker.Add(wires[i]);
  }
  if (!maker.IsDone()) {
    throw std::invalid_argument(
        placed_error("a profile could not be built", "profile " + region.name + " could not be built"));
  }
  ProfileFace profile;
  profile.face = maker.Face();
  profile.pinched = pinched;
  detail::require_valid(profile.face, "the profile");
  ShapeMap edges;
  for (TopExp_Explorer it(profile.face, TopAbs_EDGE); it.More(); it.Next()) {
    edges.Add(it.Current());
  }
  for (const auto& [edge, name] : names) {
    const int index = edges.FindIndex(edge);
    if (index == 0) {
      throw std::runtime_error(placed_error("an edge of a profile is not on its face",
                                            "profile " + region.name + ": edge " + name + " is not on the face"));
    }
    profile.edges.emplace_back(TopoDS::Edge(edges(index)), name);
  }
  return profile;
}

// The largest gap between the segments of a loop that shape healing closes:
// sketch points that meet within the sketch's tolerance but a little over
// OCCT's (imported sketches).
constexpr double kHealGap = 1e-5;

gp_Pnt middle(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
}

// The region's face with small gaps between its segments closed by shape
// healing; each segment's edge is found again by its middle point.
ProfileFace healed_profile(const Frame& frame, const Region& region, double max_gap) {
  const gp_Pln plane = plane_of(frame);
  std::vector<std::pair<gp_Pnt, std::string>> middles;
  std::vector<TopoDS_Wire> wires;
  for (std::size_t i = 0; i < region.loops.size(); ++i) {
    occ::handle<ShapeExtend_WireData> data = new ShapeExtend_WireData();
    for (const Segment& segment : region.loops[i].segments) {
      const TopoDS_Edge edge = make_edge(frame, segment);
      middles.emplace_back(middle(edge), segment.name);
      data->Add(edge);
    }
    ShapeFix_Wire fix;
    fix.Load(data);
    fix.SetPrecision(max_gap);
    fix.SetMaxTolerance(max_gap);
    fix.ClosedWireMode() = true;
    fix.FixReorder();
    fix.FixConnected();
    TopoDS_Wire wire = fix.WireAPIMake();
    if (wire.IsNull() || !BRep_Tool::IsClosed(wire)) {
      throw std::invalid_argument("a profile loop is not closed");
    }
    if (is_clockwise(plane, wire) == (i == 0)) {
      wire = TopoDS::Wire(wire.Reversed());
    }
    wires.push_back(wire);
  }
  BRepBuilderAPI_MakeFace maker(plane, wires.front(), true);
  for (std::size_t i = 1; i < wires.size(); ++i) {
    maker.Add(wires[i]);
  }
  if (!maker.IsDone()) {
    throw std::invalid_argument(
        placed_error("a profile could not be built", "profile " + region.name + " could not be built"));
  }
  ShapeFix_Face fix(maker.Face());
  fix.SetPrecision(max_gap);
  fix.SetMaxTolerance(max_gap);
  fix.Perform();
  ProfileFace profile;
  profile.face = fix.Face();
  detail::require_valid(profile.face, "the profile");
  std::vector<TopoDS_Edge> edges;
  for (TopExp_Explorer it(profile.face, TopAbs_EDGE); it.More(); it.Next()) {
    edges.push_back(TopoDS::Edge(it.Current()));
  }
  if (edges.size() != middles.size()) {
    throw std::invalid_argument("the profile's edges changed when its gaps were closed");
  }
  std::vector<gp_Pnt> edge_middles;
  for (const TopoDS_Edge& edge : edges) {
    edge_middles.push_back(middle(edge));
  }
  for (const auto& [point, name] : middles) {
    std::size_t best = 0;
    for (std::size_t k = 1; k < edges.size(); ++k) {
      if (edge_middles[k].Distance(point) < edge_middles[best].Distance(point)) {
        best = k;
      }
    }
    if (edge_middles[best].Distance(point) > 1e3 * max_gap) {
      throw std::invalid_argument(placed_error("a segment of the profile is not on the healed profile",
                                               "segment " + name + " is not on the healed profile"));
    }
    profile.edges.emplace_back(edges[best], name);
  }
  return profile;
}

} // namespace

gp_Pnt Frame::point(const gp_Pnt2d& p) const {
  return origin.Translated(gp_Vec(x_axis) * p.X() + gp_Vec(y_axis) * p.Y());
}

namespace {

ProfileFace profile_with_gap(const Frame& frame, const Region& region, double max_gap, bool strict) {
  require_positive("profile gap tolerance", max_gap);
  if (region.loops.empty()) {
    throw std::invalid_argument(placed_error("a profile has no boundary", "profile " + region.name + " has no boundary"));
  }
  return detail::run("profile", [&] {
    if (strict) {
      for (const Loop& loop : region.loops) {
        require_loop_gap(frame, loop, max_gap);
      }
    }
    try {
      return exact_profile(frame, region);
    } catch (const Cancelled&) {
      throw;
    } catch (const std::bad_alloc&) {
      throw;
    } catch (const std::exception&) {
      // Segments that meet only within the sketch's tolerance; the first
      // error when that does not help either.
      const std::exception_ptr error = std::current_exception();
      try {
        return healed_profile(frame, region, max_gap);
      } catch (const Cancelled&) {
        throw;
      } catch (const std::bad_alloc&) {
        throw;
      } catch (const Standard_OutOfMemory&) {
        throw;
      } catch (...) {
        std::rethrow_exception(error);
      }
    }
  });
}

} // namespace

ProfileFace make_profile(const Frame& frame, const Region& region) {
  return profile_with_gap(frame, region, kHealGap, false);
}

ProfileFace make_profile(const Frame& frame, const Region& region, double max_gap) {
  return profile_with_gap(frame, region, max_gap, true);
}

ShapePtr profile_shape(const Frame& frame, const std::vector<Region>& regions) {
  if (regions.size() == 1) {
    return std::make_shared<Shape>(make_profile(frame, regions.front()).face);
  }
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const Region& region : regions) {
    builder.Add(compound, make_profile(frame, region).face);
  }
  return std::make_shared<Shape>(compound);
}

Region rectangle_region(double x, double y, double width, double height) {
  require_positive("width", width);
  require_positive("height", height);
  const gp_Pnt2d corners[4] = {{x, y}, {x + width, y}, {x + width, y + height}, {x, y + height}};
  Region region;
  region.loops.emplace_back();
  for (int i = 0; i < 4; ++i) {
    Segment line;
    line.kind = SegmentKind::Line;
    line.start = corners[i];
    line.end = corners[(i + 1) % 4];
    region.loops.front().segments.push_back(line);
  }
  return region;
}

Region circle_region(double cx, double cy, double radius) {
  require_positive("radius", radius);
  Segment circle;
  circle.kind = SegmentKind::Circle;
  circle.center = gp_Pnt2d(cx, cy);
  circle.radius = radius;
  Region region;
  region.loops.push_back(Loop{{circle}});
  return region;
}

} // namespace mitcad::geometry
