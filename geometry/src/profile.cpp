// SPDX-License-Identifier: MIT
#include "mitcad/geometry/profile.hpp"

#include <cmath>
#include <exception>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <Precision.hxx>
#include <ShapeExtend_WireData.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Wire.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
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
    const gp_Circ circle(axes_at(frame, s.center, 0.0), s.radius);
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

// The region's face from its segments as they are.
ProfileFace exact_profile(const Frame& frame, const Region& region) {
  const gp_Pln plane = plane_of(frame);
  std::vector<std::pair<TopoDS_Edge, std::string>> names;
  std::vector<TopoDS_Wire> wires;
  for (std::size_t i = 0; i < region.loops.size(); ++i) {
    TopoDS_Wire wire = make_wire(frame, region.loops[i], names);
    // Material on the left: the outer boundary counter-clockwise, holes clockwise.
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
    throw std::invalid_argument("profile " + region.name + " could not be built");
  }
  ProfileFace profile;
  profile.face = maker.Face();
  detail::require_valid(profile.face, "the profile");
  ShapeMap edges;
  for (TopExp_Explorer it(profile.face, TopAbs_EDGE); it.More(); it.Next()) {
    edges.Add(it.Current());
  }
  for (const auto& [edge, name] : names) {
    const int index = edges.FindIndex(edge);
    if (index == 0) {
      throw std::runtime_error("profile " + region.name + ": edge " + name + " is not on the face");
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
ProfileFace healed_profile(const Frame& frame, const Region& region) {
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
    fix.SetPrecision(kHealGap);
    fix.SetMaxTolerance(kHealGap);
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
    throw std::invalid_argument("profile " + region.name + " could not be built");
  }
  ShapeFix_Face fix(maker.Face());
  fix.SetPrecision(kHealGap);
  fix.SetMaxTolerance(kHealGap);
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
    if (edge_middles[best].Distance(point) > 1e3 * kHealGap) {
      throw std::invalid_argument("segment " + name + " is not on the healed profile");
    }
    profile.edges.emplace_back(edges[best], name);
  }
  return profile;
}

} // namespace

gp_Pnt Frame::point(const gp_Pnt2d& p) const {
  return origin.Translated(gp_Vec(x_axis) * p.X() + gp_Vec(y_axis) * p.Y());
}

ProfileFace make_profile(const Frame& frame, const Region& region) {
  if (region.loops.empty()) {
    throw std::invalid_argument("profile " + region.name + " has no boundary");
  }
  return detail::run("profile", [&] {
    try {
      return exact_profile(frame, region);
    } catch (const std::exception&) {
      // Segments that meet only within the sketch's tolerance; the first
      // error when that does not help either.
      const std::exception_ptr error = std::current_exception();
      try {
        return healed_profile(frame, region);
      } catch (...) {
        std::rethrow_exception(error);
      }
    }
  });
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
