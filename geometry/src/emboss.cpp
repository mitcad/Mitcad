// SPDX-License-Identifier: MIT
#include "mitcad/geometry/emboss.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomAPI.hxx>
#include <GeomConvert.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom_Circle.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Line.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_DataMap.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>

#include "emboss_taper.hpp"
#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 2.0 * 3.14159265358979323846;

// The cylinder the profiles wrap onto and the radii they grow between.
struct Wrap {
  gp_Ax3 frame; // on the axis: Z the axis, X the sketch side's radial direction
  double radius = 0.0;
  double to = 0.0;
  double tolerance = 0.0; // of what is built on it
  occ::handle<Geom_Surface> base;
  occ::handle<Geom_Surface> top;
  // A cone's development (mitcad#473): the reference circle at the frame's
  // origin, rho0 from the apex along a generator.
  bool cone = false;
  double rho0 = 0.0;
  double sin_angle = 0.0;
  // The taper (mitcad#1547): how far the top's boundary moves within the
  // top surface, outwards when positive; on a cone, the top's distance
  // from its apex along a generator at the reference circle's parameters.
  double shift = 0.0;
  double top_rho = 0.0;
};

// A flat point (s, t) as the cone's parameters: the angle about the axis
// and the distance along the generator from the reference circle.
gp_Pnt2d cone_uv(const Wrap& wrap, const gp_Pnt2d& p) {
  const double along = p.Y() + wrap.rho0;
  return gp_Pnt2d(std::atan2(p.X(), along) / wrap.sin_angle, std::hypot(p.X(), along) - wrap.rho0);
}

// The sketch's frame in flat coordinates: s across the axis (along the
// frame's Y direction, axis × radial), z along it, 0.
Frame flat_frame(const Frame& frame, const gp_Ax3& cylinder) {
  const gp_Vec across(cylinder.YDirection());
  const gp_Vec along(cylinder.Direction());
  const gp_Vec origin(cylinder.Location(), frame.origin);
  const auto flat = [&](const gp_Dir& d) {
    return gp_Dir(gp_Vec(d).Dot(across), gp_Vec(d).Dot(along), 0.0);
  };
  Frame result;
  result.origin = gp_Pnt(origin.Dot(across), origin.Dot(along), 0.0);
  result.x_axis = flat(frame.x_axis);
  result.y_axis = flat(frame.y_axis);
  return result;
}

// A curve in the cylinders' (angle, height) parameters over its range.
struct UvCurve {
  occ::handle<Geom2d_Curve> curve;
  double first = 0.0;
  double last = 0.0;
};

// A flat point (s, z) as the cylinders' parameters (s / radius, z).
gp_Pnt2d uv_of(const gp_Pnt2d& p, double radius) { return gp_Pnt2d(p.X() / radius, p.Y()); }

// The edge of a flat profile face (in the plane z = 0) wrapped: its curve
// in the cylinders' parameters. Lines stay lines; other curves are their
// B-splines with the poles moved, which an affine map of a (rational)
// B-spline is exactly.
UvCurve uv_curve(const TopoDS_Edge& edge, double radius) {
  double first = 0.0;
  double last = 0.0;
  TopLoc_Location location;
  occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
  if (curve.IsNull()) {
    throw std::runtime_error("a profile edge has no curve");
  }
  if (!location.IsIdentity()) {
    curve = occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
  }
  const occ::handle<Geom2d_Curve> flat = GeomAPI::To2d(curve, gp_Pln(gp::XOY()));
  occ::handle<Geom2d_Curve> basis = flat;
  for (occ::handle<Geom2d_TrimmedCurve> trimmed = occ::down_cast<Geom2d_TrimmedCurve>(basis); !trimmed.IsNull();
       trimmed = occ::down_cast<Geom2d_TrimmedCurve>(basis)) {
    basis = trimmed->BasisCurve();
  }
  if (basis->IsKind(STANDARD_TYPE(Geom2d_Line))) {
    const gp_Pnt2d a = uv_of(flat->Value(first), radius);
    const gp_Pnt2d b = uv_of(flat->Value(last), radius);
    const double length = a.Distance(b);
    if (!(length > gp::Resolution())) {
      throw std::invalid_argument("a profile edge has no length");
    }
    return {new Geom2d_Line(a, gp_Dir2d(gp_Vec2d(a, b))), 0.0, length};
  }
  const occ::handle<Geom2d_BSplineCurve> spline =
      Geom2dConvert::CurveToBSplineCurve(new Geom2d_TrimmedCurve(flat, first, last));
  for (int i = 1; i <= spline->NbPoles(); ++i) {
    spline->SetPole(i, uv_of(spline->Pole(i), radius));
  }
  return {occ::handle<Geom2d_Curve>(spline), spline->FirstParameter(), spline->LastParameter()};
}

// A 2D curve through `exact` over [first, last]: a B-spline through its
// points at evenly spaced parameters, more of them until it is within 1e-8
// mm of it, as `error` measures the difference of two points.
occ::handle<Geom2d_BSplineCurve> fitted(const std::function<gp_Pnt2d(double)>& exact, double first, double last,
                                        const std::function<double(const gp_Pnt2d&, const gp_Pnt2d&)>& error,
                                        const char* what) {
  for (int n = 16; n <= 4096; n *= 2) {
    occ::handle<NCollection_HArray1<gp_Pnt2d>> points = new NCollection_HArray1<gp_Pnt2d>(1, n + 1);
    occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n + 1);
    for (int i = 0; i <= n; ++i) {
      const double t = first + (last - first) * i / n;
      points->SetValue(i + 1, exact(t));
      params->SetValue(i + 1, t);
    }
    Geom2dAPI_Interpolate interpolate(points, params, false, 1.0e-12);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error(what);
    }
    const occ::handle<Geom2d_BSplineCurve> curve = interpolate.Curve();
    double worst = 0.0;
    for (int i = 0; i < n; ++i) {
      const double t = first + (last - first) * (i + 0.5) / n;
      worst = std::max(worst, error(curve->Value(t), exact(t)));
    }
    if (worst <= 1.0e-8 || n == 4096) {
      return curve;
    }
  }
  throw std::runtime_error(what);
}

// How far apart two points of a cone's parameters are on it, near enough.
double cone_error(const Wrap& wrap, const gp_Pnt2d& a, const gp_Pnt2d& b) {
  const double radius = wrap.rho0 * wrap.sin_angle;
  return std::abs(a.X() - b.X()) * (radius + std::abs(a.Y())) + std::abs(a.Y() - b.Y());
}

// The edge of a flat profile face wrapped onto a cone: its curve in the
// cone's parameters, through points of the exact development at the
// edge's own parameters, more of them until it is within 1e-8 mm of it.
UvCurve cone_uv_curve(const TopoDS_Edge& edge, const Wrap& wrap) {
  double first = 0.0;
  double last = 0.0;
  TopLoc_Location location;
  occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
  if (curve.IsNull()) {
    throw std::runtime_error("a profile edge has no curve");
  }
  if (!location.IsIdentity()) {
    curve = occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
  }
  const occ::handle<Geom2d_Curve> flat = GeomAPI::To2d(curve, gp_Pln(gp::XOY()));
  const occ::handle<Geom2d_BSplineCurve> uv = fitted(
      [&](double t) { return cone_uv(wrap, flat->Value(t)); }, first, last,
      [&](const gp_Pnt2d& a, const gp_Pnt2d& b) { return cone_error(wrap, a, b); },
      "an edge of the profile could not be developed onto the cone");
  return {occ::handle<Geom2d_Curve>(uv), first, last};
}

// The point moved radially from the axis by the factor k.
gp_Pnt scaled(const gp_Ax3& frame, double k, const gp_Pnt& p) {
  const gp_Vec a(frame.Direction());
  const gp_Vec d(frame.Location(), p);
  const gp_Vec along = a * d.Dot(a);
  return frame.Location().Translated(along + (d - along) * k);
}

// A B-spline with its knots moved linearly onto [first, last].
occ::handle<Geom_BSplineCurve> reparametrized(const occ::handle<Geom_BSplineCurve>& spline, double first,
                                              double last) {
  NCollection_Array1<double> knots(spline->Knots());
  const double a = spline->FirstParameter();
  const double b = spline->LastParameter();
  for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
    knots(i) = first + (knots(i) - a) * (last - first) / (b - a);
  }
  const NCollection_Array1<gp_Pnt>& poles = spline->Poles();
  const NCollection_Array1<int>& multiplicities = spline->Multiplicities();
  const NCollection_Array1<double>* weights = spline->Weights();
  if (spline->IsRational() && weights != nullptr) {
    return new Geom_BSplineCurve(poles, *weights, knots, multiplicities, spline->Degree(), spline->IsPeriodic());
  }
  return new Geom_BSplineCurve(poles, knots, multiplicities, spline->Degree(), spline->IsPeriodic());
}

// The edge on the base cylinder and its radial image on the top cylinder,
// both with the same parameters: the walls between them are ruled along
// the radii.
struct WrappedEdge {
  TopoDS_Edge base;
  TopoDS_Edge top;
  UvCurve uv;
  // A taper's top shrank the edge to nothing (mitcad#1547): no top edge,
  // the corner its neighbours meet at instead.
  TopoDS_Vertex apex;
};

WrappedEdge wrapped_edge(const Wrap& wrap, const UvCurve& uv, const TopoDS_Vertex (&base)[2],
                         const TopoDS_Vertex (&top)[2]) {
  BRep_Builder builder;
  WrappedEdge result;
  result.uv = uv;
  builder.MakeEdge(result.base);
  builder.UpdateEdge(result.base, uv.curve, wrap.base, TopLoc_Location(), wrap.tolerance);
  builder.Add(result.base, base[0].Oriented(TopAbs_FORWARD));
  builder.Add(result.base, base[1].Oriented(TopAbs_REVERSED));
  builder.Range(result.base, uv.first, uv.last);
  if (!BRepLib::BuildCurve3d(result.base, wrap.tolerance, GeomAbs_C1, 14, 10000)) {
    throw std::runtime_error("an edge of the profile could not be laid onto the cylinder");
  }
  if (wrap.cone) {
    // On a cone both edges lie on their surfaces by the same parameters:
    // a point of the top edge is its base point moved along the normal.
    builder.MakeEdge(result.top);
    builder.UpdateEdge(result.top, uv.curve, wrap.top, TopLoc_Location(), wrap.tolerance);
    builder.Add(result.top, top[0].Oriented(TopAbs_FORWARD));
    builder.Add(result.top, top[1].Oriented(TopAbs_REVERSED));
    builder.Range(result.top, uv.first, uv.last);
    if (!BRepLib::BuildCurve3d(result.top, wrap.tolerance, GeomAbs_C1, 14, 10000)) {
      throw std::runtime_error("an edge of the profile could not be laid onto the cone");
    }
    return result;
  }
  double first = 0.0;
  double last = 0.0;
  occ::handle<Geom_Curve> curve = BRep_Tool::Curve(result.base, first, last);
  for (occ::handle<Geom_TrimmedCurve> trimmed = occ::down_cast<Geom_TrimmedCurve>(curve); !trimmed.IsNull();
       trimmed = occ::down_cast<Geom_TrimmedCurve>(curve)) {
    curve = trimmed->BasisCurve();
  }
  const double k = wrap.to / wrap.radius;
  const gp_Dir axis = wrap.frame.Direction();
  occ::handle<Geom_Curve> image;
  const occ::handle<Geom_Line> line = occ::down_cast<Geom_Line>(curve);
  const occ::handle<Geom_Circle> circle = occ::down_cast<Geom_Circle>(curve);
  if (!line.IsNull() && line->Lin().Direction().IsParallel(axis, 1.0e-9)) {
    image = new Geom_Line(scaled(wrap.frame, k, line->Lin().Location()), line->Lin().Direction());
  } else if (!circle.IsNull() && circle->Circ().Axis().Direction().IsParallel(axis, 1.0e-9)) {
    gp_Ax2 position = circle->Position();
    position.SetLocation(scaled(wrap.frame, k, position.Location()));
    image = new Geom_Circle(position, circle->Radius() * k);
  } else {
    occ::handle<Geom_BSplineCurve> spline = occ::down_cast<Geom_BSplineCurve>(curve);
    if (spline.IsNull()) {
      spline = GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
    } else {
      spline = occ::down_cast<Geom_BSplineCurve>(spline->Copy());
    }
    if (std::abs(spline->FirstParameter() - first) > 1.0e-12 || std::abs(spline->LastParameter() - last) > 1.0e-12) {
      // The base edge takes the same curve (SameParameter below mends the
      // parameters of its curve on the cylinder).
      spline = reparametrized(spline, first, last);
      builder.UpdateEdge(result.base, occ::down_cast<Geom_Curve>(spline->Copy()), BRep_Tool::Tolerance(result.base));
    }
    for (int i = 1; i <= spline->NbPoles(); ++i) {
      spline->SetPole(i, scaled(wrap.frame, k, spline->Pole(i)));
    }
    image = spline;
  }
  const double tolerance = BRep_Tool::Tolerance(result.base) * std::max(1.0, k);
  builder.MakeEdge(result.top, image, tolerance);
  builder.UpdateEdge(result.top, uv.curve, wrap.top, TopLoc_Location(), tolerance);
  builder.Add(result.top, top[0].Oriented(TopAbs_FORWARD));
  builder.Add(result.top, top[1].Oriented(TopAbs_REVERSED));
  builder.Range(result.top, first, last);
  return result;
}

using EdgeMap = NCollection_DataMap<TopoDS_Shape, WrappedEdge, TopTools_ShapeMapHasher>;
using VertexMap = NCollection_DataMap<TopoDS_Shape, std::pair<TopoDS_Vertex, TopoDS_Vertex>, TopTools_ShapeMapHasher>;

// The signed area the wrapped wire encloses in the cylinders' parameters.
double uv_area(const TopoDS_Wire& wire, const TopoDS_Face& face, const EdgeMap& edges) {
  std::vector<gp_Pnt2d> points;
  for (BRepTools_WireExplorer e(wire, face); e.More(); e.Next()) {
    const UvCurve& uv = edges.Find(e.Current()).uv;
    constexpr int kSamples = 16;
    for (int i = 0; i < kSamples; ++i) {
      double t = static_cast<double>(i) / kSamples;
      if (e.Current().Orientation() == TopAbs_REVERSED) {
        t = 1.0 - t;
      }
      points.push_back(uv.curve->Value(uv.first + t * (uv.last - uv.first)));
    }
  }
  double area = 0.0;
  for (std::size_t i = 0; i < points.size(); ++i) {
    const gp_Pnt2d& p = points[i];
    const gp_Pnt2d& q = points[(i + 1) % points.size()];
    area += p.X() * q.Y() - q.X() * p.Y();
  }
  return area / 2.0;
}

// The orientation of `edge` in the face as it lies in the shell.
TopAbs_Orientation orientation_in(const TopoDS_Face& face, const TopoDS_Edge& edge) {
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    if (e.Current().IsSame(edge)) {
      return e.Current().Orientation();
    }
  }
  throw std::runtime_error("a wall of the emboss does not meet its profile's face");
}

// A point of the top surface's parameters laid out flat on its development
// (mitcad#1547): a cylinder's unrolled, a cone's about its apex. Lengths
// and angles on the top surface are the development's.
gp_Pnt2d top_development(const Wrap& wrap, const gp_Pnt2d& uv) {
  if (!wrap.cone) {
    return gp_Pnt2d(uv.X() * wrap.to, uv.Y());
  }
  const double rho = wrap.top_rho + uv.Y();
  const double phi = uv.X() * wrap.sin_angle;
  return gp_Pnt2d(rho * std::sin(phi), rho * std::cos(phi));
}

// A point of the top's development as the top surface's parameters.
gp_Pnt2d top_parameters(const Wrap& wrap, const gp_Pnt2d& flat) {
  if (!wrap.cone) {
    return gp_Pnt2d(flat.X() / wrap.to, flat.Y());
  }
  return gp_Pnt2d(std::atan2(flat.X(), flat.Y()) / wrap.sin_angle,
                  std::hypot(flat.X(), flat.Y()) - wrap.top_rho);
}

// The top's edges of a tapered emboss (mitcad#1547): the profile face laid
// out on the top surface's development, its loops moved there by the
// taper's shift with OCCT's offset (exact on the development, whose lengths
// are the surface's), and the moved edges laid onto the top surface. Each
// wrapped edge's top is replaced by its moved edge, with the same
// parameters' direction as its base; the base edges stay.
void tapered_tops(const Wrap& wrap, const detail::SweepFace& profile, EdgeMap& edges) {
  const TopoDS_Face& planar = profile.face;
  BRep_Builder builder;
  const occ::handle<Geom_Surface> plane = BRep_Tool::Surface(planar);
  const auto in_plane = [](const gp_Pnt2d& p) { return gp_Pnt(p.X(), p.Y(), 0.0); };
  // The profile face on the development, edge for edge.
  NCollection_DataMap<TopoDS_Shape, TopoDS_Shape, TopTools_ShapeMapHasher> laid;
  NCollection_DataMap<TopoDS_Shape, TopoDS_Vertex, TopTools_ShapeMapHasher> laid_vertices;
  const auto laid_vertex = [&](const TopoDS_Vertex& flat) {
    if (!laid_vertices.IsBound(flat)) {
      const gp_Pnt p = BRep_Tool::Pnt(flat);
      const gp_Pnt2d uv =
          wrap.cone ? cone_uv(wrap, gp_Pnt2d(p.X(), p.Y())) : uv_of(gp_Pnt2d(p.X(), p.Y()), wrap.radius);
      TopoDS_Vertex vertex;
      builder.MakeVertex(vertex, in_plane(top_development(wrap, uv)), wrap.tolerance);
      laid_vertices.Bind(flat, vertex);
    }
    return laid_vertices.Find(flat);
  };
  for (TopExp_Explorer e(planar, TopAbs_EDGE); e.More(); e.Next()) {
    if (laid.IsBound(e.Current())) {
      continue;
    }
    const UvCurve& uv = edges.Find(e.Current()).uv;
    occ::handle<Geom2d_Curve> flat;
    double first = uv.first;
    double last = uv.last;
    if (!wrap.cone && uv.curve->IsKind(STANDARD_TYPE(Geom2d_Line))) {
      const gp_Pnt2d a = top_development(wrap, uv.curve->Value(uv.first));
      const gp_Pnt2d b = top_development(wrap, uv.curve->Value(uv.last));
      flat = new Geom2d_Line(a, gp_Dir2d(gp_Vec2d(a, b)));
      first = 0.0;
      last = a.Distance(b);
    } else if (!wrap.cone) {
      // An affine map of a B-spline moves its poles.
      const occ::handle<Geom2d_BSplineCurve> spline =
          occ::down_cast<Geom2d_BSplineCurve>(Geom2dConvert::CurveToBSplineCurve(
              new Geom2d_TrimmedCurve(uv.curve, uv.first, uv.last)));
      for (int i = 1; i <= spline->NbPoles(); ++i) {
        spline->SetPole(i, top_development(wrap, spline->Pole(i)));
      }
      flat = spline;
      first = spline->FirstParameter();
      last = spline->LastParameter();
    } else {
      flat = fitted([&](double t) { return top_development(wrap, uv.curve->Value(t)); }, uv.first, uv.last,
                    [](const gp_Pnt2d& a, const gp_Pnt2d& b) { return a.Distance(b); },
                    "an edge of the profile could not be laid out on the top's development");
    }
    TopoDS_Vertex v0;
    TopoDS_Vertex v1;
    TopExp::Vertices(TopoDS::Edge(e.Current()), v0, v1);
    TopoDS_Edge edge;
    builder.MakeEdge(edge, GeomAPI::To3d(flat, gp_Pln(gp::XOY())), wrap.tolerance);
    builder.Add(edge, laid_vertex(v0).Oriented(TopAbs_FORWARD));
    builder.Add(edge, laid_vertex(v1).Oriented(TopAbs_REVERSED));
    builder.Range(edge, first, last);
    laid.Bind(e.Current(), edge);
  }
  // Its loops as the profile face's, on the same plane: the development
  // keeps the plane's orientation (a stretch across the axis, or along the
  // cone's generators).
  TopoDS_Face face;
  builder.MakeFace(face, plane, wrap.tolerance);
  for (TopoDS_Iterator w(planar, false, false); w.More(); w.Next()) {
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    for (TopoDS_Iterator e(w.Value(), false, false); e.More(); e.Next()) {
      builder.Add(wire, laid.Find(e.Value()).Oriented(e.Value().Orientation()));
    }
    wire.Closed(true);
    wire.Orientation(w.Value().Orientation());
    builder.Add(face, wire);
  }
  face.Orientation(planar.Orientation());
  const detail::MovedLoops loops = detail::moved_loops(face, wrap.shift, detail::region_list(profile.regions));

  // The moved edges on the top surface; corners shared where they meet.
  std::vector<std::pair<gp_Pnt2d, TopoDS_Vertex>> corners;
  const double tolerance = wrap.tolerance * std::max(1.0, wrap.to / wrap.radius);
  const auto corner = [&](const gp_Pnt2d& uv) {
    const gp_Pnt2d at = top_development(wrap, uv);
    for (const auto& [place, vertex] : corners) {
      if (place.Distance(at) <= 1.0e-6 * std::max(1.0, wrap.to)) {
        return vertex;
      }
    }
    TopoDS_Vertex vertex;
    builder.MakeVertex(vertex, wrap.top->Value(uv.X(), uv.Y()), tolerance);
    corners.emplace_back(at, vertex);
    return vertex;
  };
  for (EdgeMap::Iterator it(edges); it.More(); it.Next()) {
    const TopoDS_Shape& on_development = laid.Find(it.Key());
    if (loops.vanished.IsBound(on_development)) {
      const gp_Pnt p = BRep_Tool::Pnt(loops.vanished.Find(on_development));
      WrappedEdge& wrapped = edges.ChangeFind(it.Key());
      wrapped.top.Nullify();
      wrapped.apex = corner(top_parameters(wrap, gp_Pnt2d(p.X(), p.Y())));
      continue;
    }
    // The moved edge's curve runs as the base edge's.
    const BRepAdaptor_Curve curve(loops.moved.Find(on_development));
    const double f = curve.FirstParameter();
    const double l = curve.LastParameter();
    const auto at = [&](double t) {
      const gp_Pnt p = curve.Value(t);
      return gp_Pnt2d(p.X(), p.Y());
    };
    UvCurve top;
    occ::handle<Geom_Curve> exact;
    if (!wrap.cone && curve.GetType() == GeomAbs_Line) {
      const gp_Pnt2d a = top_parameters(wrap, at(f));
      const gp_Pnt2d b = top_parameters(wrap, at(l));
      const gp_Dir2d along(gp_Vec2d(a, b));
      top = {new Geom2d_Line(a, along), 0.0, a.Distance(b)};
      // Around the axis or along it: an arc or a line, exactly.
      const gp_Dir axis = wrap.frame.Direction();
      if (std::abs(along.Y()) < 1.0e-12) {
        const gp_Pnt centre = wrap.frame.Location().Translated(gp_Vec(axis) * a.Y());
        const gp_Dir start = wrap.frame.XDirection().Rotated(gp_Ax1(wrap.frame.Location(), axis), a.X());
        exact = new Geom_Circle(gp_Ax2(centre, along.X() > 0.0 ? axis : axis.Reversed(), start), wrap.to);
      } else if (std::abs(along.X()) < 1.0e-12) {
        exact = new Geom_Line(wrap.top->Value(a.X(), a.Y()), along.Y() > 0.0 ? axis : axis.Reversed());
      }
    } else {
      const occ::handle<Geom2d_BSplineCurve> uv = fitted(
          [&](double t) { return top_parameters(wrap, at(t)); }, f, l,
          [&](const gp_Pnt2d& a, const gp_Pnt2d& b) {
            return wrap.cone ? cone_error(wrap, a, b)
                             : std::abs(a.X() - b.X()) * wrap.to + std::abs(a.Y() - b.Y());
          },
          "a tapered edge of the profile could not be laid onto the top");
      top = {occ::handle<Geom2d_Curve>(uv), f, l};
    }
    WrappedEdge& wrapped = edges.ChangeFind(it.Key());
    TopoDS_Edge edge;
    builder.MakeEdge(edge);
    builder.UpdateEdge(edge, top.curve, wrap.top, TopLoc_Location(), tolerance);
    builder.Add(edge, corner(top.curve->Value(top.first)).Oriented(TopAbs_FORWARD));
    builder.Add(edge, corner(top.curve->Value(top.last)).Oriented(TopAbs_REVERSED));
    builder.Range(edge, top.first, top.last);
    if (!exact.IsNull()) {
      builder.UpdateEdge(edge, exact, tolerance);
    } else if (!BRepLib::BuildCurve3d(edge, std::min(tolerance, 1.0e-8), GeomAbs_C1, 14, 10000)) {
      throw std::runtime_error("a tapered edge of the profile could not be laid onto the top");
    }
    wrapped.top = edge;
  }
}

// The solid of a tapered profile (mitcad#1547) from its foot and top faces
// on the two surfaces: each wall ruled between its edge on the foot and
// its edge on the top (or the top's corner where the taper shrank the edge
// to nothing), and the faces sewn into one closed shell.
ShapePtr tapered_solid(const std::string& feature, const Wrap& wrap, const detail::SweepFace& profile,
                       const EdgeMap& edges, const TopoDS_Face& base, const TopoDS_Face& top) {
  BRepBuilderAPI_Sewing sewing(std::max(10.0 * wrap.tolerance, 1.0e-6));
  sewing.Add(base);
  sewing.Add(top);
  std::vector<std::pair<TopoDS_Face, std::string>> walls;
  for (EdgeMap::Iterator it(edges); it.More(); it.Next()) {
    const WrappedEdge& wrapped = it.Value();
    BRepOffsetAPI_ThruSections sections(false, true, wrap.tolerance);
    sections.AddWire(BRepBuilderAPI_MakeWire(wrapped.base).Wire());
    if (wrapped.top.IsNull()) {
      sections.AddVertex(wrapped.apex);
    } else {
      sections.AddWire(BRepBuilderAPI_MakeWire(wrapped.top).Wire());
    }
    detail::build(sections);
    if (!sections.IsDone()) {
      throw std::runtime_error("the tapered walls of the emboss could not be built");
    }
    for (TopExp_Explorer f(sections.Shape(), TopAbs_FACE); f.More(); f.Next()) {
      sewing.Add(f.Current());
      for (const auto& [source, name] : profile.edges) {
        if (source.IsSame(it.Key())) {
          walls.emplace_back(TopoDS::Face(f.Current()), name);
        }
      }
    }
  }
  sewing.Perform();
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer e(sewing.SewedShape(), TopAbs_SHELL); e.More(); e.Next()) {
    shell = TopoDS::Shell(e.Current());
    ++shells;
  }
  if (shells != 1 || sewing.NbFreeEdges() != 0) {
    throw std::runtime_error("the tapered walls of the profile " + detail::region_list(profile.regions) +
                             " do not close up with its top and foot");
  }
  BRep_Builder builder;
  shell.Closed(true);
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  builder.Add(solid, shell);
  BRepLib::OrientClosedSolid(solid);
  const auto image = [&sewing](const TopoDS_Shape& face) {
    return sewing.IsModified(face) ? sewing.Modified(face) : face;
  };
  detail::FaceNamer namer(solid);
  for (const std::string& name : detail::cap_names(feature, "start", profile)) {
    namer.add(image(base), name);
  }
  for (const std::string& name : detail::cap_names(feature, "end", profile)) {
    namer.add(image(top), name);
  }
  for (const auto& [wall, name] : walls) {
    namer.add(image(wall), name);
  }
  return namer.shape();
}

// The solid of one profile face wrapped and grown.
ShapePtr wrapped_solid(const std::string& feature, const Wrap& wrap, const detail::SweepFace& profile) {
  const TopoDS_Face& planar = profile.face;
  BRep_Builder builder;
  VertexMap vertices;
  for (TopExp_Explorer e(planar, TopAbs_VERTEX); e.More(); e.Next()) {
    if (vertices.IsBound(e.Current())) {
      continue;
    }
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(e.Current()));
    const gp_Pnt2d uv =
        wrap.cone ? cone_uv(wrap, gp_Pnt2d(p.X(), p.Y())) : uv_of(gp_Pnt2d(p.X(), p.Y()), wrap.radius);
    TopoDS_Vertex base;
    TopoDS_Vertex top;
    builder.MakeVertex(base, wrap.base->Value(uv.X(), uv.Y()), wrap.tolerance);
    builder.MakeVertex(top, wrap.top->Value(uv.X(), uv.Y()), wrap.tolerance * std::max(1.0, wrap.to / wrap.radius));
    vertices.Bind(e.Current(), {base, top});
  }
  EdgeMap edges;
  for (TopExp_Explorer e(planar, TopAbs_EDGE); e.More(); e.Next()) {
    if (edges.IsBound(e.Current())) {
      continue;
    }
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(edge, first, last);
    const auto& [base0, top0] = vertices.Find(first);
    const auto& [base1, top1] = vertices.Find(last);
    const TopoDS_Vertex base[2] = {base0, base1};
    const TopoDS_Vertex top[2] = {top0, top1};
    edges.Bind(edge, wrapped_edge(wrap, wrap.cone ? cone_uv_curve(edge, wrap) : uv_curve(edge, wrap.radius), base,
                                  top));
  }
  if (wrap.shift != 0.0) {
    tapered_tops(wrap, profile, edges);
  }

  // The loops on both cylinders, edge for edge in the same order.
  std::vector<std::pair<TopoDS_Wire, TopoDS_Wire>> loops;
  const TopoDS_Wire outer = BRepTools::OuterWire(planar);
  bool flip = false;
  for (TopExp_Explorer w(planar, TopAbs_WIRE); w.More(); w.Next()) {
    const TopoDS_Wire& wire = TopoDS::Wire(w.Current());
    TopoDS_Wire base;
    TopoDS_Wire top;
    builder.MakeWire(base);
    builder.MakeWire(top);
    for (BRepTools_WireExplorer e(wire, planar); e.More(); e.Next()) {
      const WrappedEdge& wrapped = edges.Find(e.Current());
      builder.Add(base, wrapped.base.Oriented(e.Current().Orientation()));
      if (!wrapped.top.IsNull()) {
        builder.Add(top, wrapped.top.Oriented(e.Current().Orientation()));
      }
    }
    base.Closed(true);
    top.Closed(true);
    if (wire.IsSame(outer)) {
      // The outer loop runs counter-clockwise in the parameters of a face
      // whose normal points out from the axis.
      flip = uv_area(wire, planar, edges) < 0.0;
    }
    loops.emplace_back(base, top);
  }
  TopoDS_Face base;
  TopoDS_Face top;
  builder.MakeFace(base, wrap.base, wrap.tolerance);
  builder.MakeFace(top, wrap.top, wrap.tolerance);
  for (const auto& [b, t] : loops) {
    builder.Add(base, flip ? TopoDS::Wire(b.Reversed()) : b);
    builder.Add(top, flip ? TopoDS::Wire(t.Reversed()) : t);
  }
  // Both point out from the axis; the solid's faces point out of it.
  if (wrap.to > wrap.radius) {
    base.Reverse();
  } else {
    top.Reverse();
  }

  if (wrap.shift != 0.0) {
    return tapered_solid(feature, wrap, profile, edges, base, top);
  }
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  builder.Add(shell, base);
  builder.Add(shell, top);
  std::vector<std::pair<TopoDS_Face, std::string>> walls;
  for (const auto& [b, t] : loops) {
    BRepOffsetAPI_ThruSections sections(false, true, wrap.tolerance);
    sections.CheckCompatibility(false);
    sections.AddWire(b);
    sections.AddWire(t);
    detail::build(sections);
    if (!sections.IsDone()) {
      throw std::runtime_error("the walls of the emboss could not be built");
    }
    for (TopExp_Explorer e(b, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Shape generated = sections.GeneratedFace(e.Current());
      if (generated.IsNull() || generated.ShapeType() != TopAbs_FACE) {
        throw std::runtime_error("an edge of the profile has no wall");
      }
      TopoDS_Face wall = TopoDS::Face(generated);
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      // Each edge runs one way in one face and the other way in the other.
      if (orientation_in(wall, edge) == orientation_in(base, edge)) {
        wall.Reverse();
      }
      builder.Add(shell, wall);
      for (const auto& [source, name] : profile.edges) {
        if (edges.Find(source).base.IsSame(edge)) {
          walls.emplace_back(wall, name);
        }
      }
    }
  }
  shell.Closed(true);
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  builder.Add(solid, shell);
  BRepLib::SameParameter(solid, wrap.tolerance, true);
  BRepLib::OrientClosedSolid(solid);

  detail::FaceNamer namer(solid);
  for (const std::string& name : detail::cap_names(feature, "start", profile)) {
    namer.add(base, name);
  }
  for (const std::string& name : detail::cap_names(feature, "end", profile)) {
    namer.add(top, name);
  }
  for (const auto& [wall, name] : walls) {
    namer.add(wall, name);
  }
  return namer.shape();
}

} // namespace

namespace {

// A taper's angle: finite and less than a right angle either way.
void require_taper(double taper) {
  if (!(std::abs(taper) < kTwoPi / 4.0 - 1.0e-9)) {
    throw std::invalid_argument("the emboss's taper angle must lie between -90 and 90 degrees");
  }
}

// The profiles developed onto a cone (EmbossWrapSpec, mitcad#473).
ShapePtr wrapped_cone(const EmbossWrapSpec& spec) {
  const double angle = spec.half_angle;
  if (!(angle > 1.0e-6 && angle < kTwoPi / 4.0 - 1.0e-6)) {
    throw std::invalid_argument("the cone's half angle must lie between 0 and 90 degrees");
  }
  detail::require_positive("cone radius", spec.radius);
  const double offset = spec.to_radius - spec.radius;
  if (!(std::abs(offset) > 1.0e-7)) {
    throw std::invalid_argument("the emboss has no depth");
  }
  if (!(spec.radius + offset * std::cos(angle) > 1.0e-7 * spec.radius)) {
    throw std::invalid_argument("the emboss reaches the cone's axis");
  }
  const gp_Dir axis = spec.axis.Direction();
  gp_Vec normal(spec.frame.normal());
  if (std::abs(normal.Dot(gp_Vec(axis))) > 1.0e-6) {
    throw std::invalid_argument("to wrap onto a cone, the sketch plane must be parallel to its axis");
  }
  return detail::run("emboss", [&] {
    const gp_Pnt apex = spec.axis.Location();
    // Towards the sketch plane from the axis.
    const double height = gp_Vec(apex, spec.frame.origin).Dot(gp_Vec(axis));
    const gp_Pnt centre = apex.Translated(gp_Vec(axis) * height);
    if (gp_Vec(centre, spec.frame.origin).Dot(normal) < 0.0) {
      normal.Reverse();
    }
    normal -= gp_Vec(axis) * normal.Dot(gp_Vec(axis));
    Wrap wrap;
    wrap.frame = gp_Ax3(centre, axis, gp_Dir(normal));
    wrap.radius = spec.radius;
    wrap.to = spec.to_radius;
    wrap.tolerance = 1.0e-7 * std::max(1.0, std::max(spec.radius, std::abs(offset)));
    wrap.cone = true;
    wrap.sin_angle = std::sin(angle);
    wrap.rho0 = spec.radius / wrap.sin_angle;
    wrap.top_rho = wrap.rho0 + offset * std::cos(angle) / wrap.sin_angle;
    wrap.shift = std::abs(offset) * std::tan(spec.taper);
    wrap.base = new Geom_ConicalSurface(wrap.frame, angle, spec.radius);
    // The cone moved along its normals by the offset: the same angle, the
    // radius and the place along the axis it has at the same parameters.
    gp_Ax3 shifted = wrap.frame;
    shifted.SetLocation(centre.Translated(gp_Vec(axis) * (-offset * wrap.sin_angle)));
    wrap.top = new Geom_ConicalSurface(shifted, angle, spec.radius + offset * std::cos(angle));

    const Frame flat = flat_frame(spec.frame, wrap.frame);
    std::vector<ShapePtr> parts;
    for (const detail::SweepFace& face : detail::region_faces(spec.feature, flat, spec.regions)) {
      Bnd_Box box;
      BRepBndLib::Add(face.face, box);
      double xmin = 0.0, ymin = 0.0, zmin = 0.0, xmax = 0.0, ymax = 0.0, zmax = 0.0;
      box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
      if (ymin + wrap.rho0 <= 0.0) {
        throw std::invalid_argument("the profiles reach past the cone's apex");
      }
      double low = kTwoPi;
      double high = -kTwoPi;
      for (const double s : {xmin, xmax}) {
        for (const double t : {ymin, ymax}) {
          const double psi = std::atan2(s, t + wrap.rho0);
          low = std::min(low, psi);
          high = std::max(high, psi);
        }
      }
      if ((high - low) / wrap.sin_angle >= kTwoPi) {
        throw std::invalid_argument("the profiles reach around the cone");
      }
      parts.push_back(wrapped_solid(spec.feature, wrap, face));
    }
    return detail::finished(*detail::fused(parts, "the emboss"), "the emboss");
  });
}

} // namespace

ShapePtr wrapped_emboss(const EmbossWrapSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to emboss");
  }
  require_taper(spec.taper);
  if (spec.half_angle != 0.0) {
    return wrapped_cone(spec);
  }
  detail::require_positive("cylinder radius", spec.radius);
  detail::require_positive("emboss radius", spec.to_radius);
  if (!(std::abs(spec.to_radius - spec.radius) > 1.0e-7)) {
    throw std::invalid_argument("the emboss has no depth");
  }
  const gp_Dir axis = spec.axis.Direction();
  gp_Vec normal(spec.frame.normal());
  if (std::abs(normal.Dot(gp_Vec(axis))) > 1.0e-6) {
    throw std::invalid_argument("to wrap onto a cylinder, the sketch plane must be parallel to its axis");
  }
  return detail::run("emboss", [&] {
    // Towards the sketch plane from the axis.
    if (gp_Vec(spec.axis.Location(), spec.frame.origin).Dot(normal) < 0.0) {
      normal.Reverse();
    }
    normal -= gp_Vec(axis) * normal.Dot(gp_Vec(axis));
    Wrap wrap;
    wrap.frame = gp_Ax3(spec.axis.Location(), axis, gp_Dir(normal));
    wrap.radius = spec.radius;
    wrap.to = spec.to_radius;
    wrap.tolerance = 1.0e-7 * std::max(1.0, std::max(spec.radius, spec.to_radius));
    wrap.base = new Geom_CylindricalSurface(wrap.frame, spec.radius);
    wrap.top = new Geom_CylindricalSurface(wrap.frame, spec.to_radius);
    wrap.shift = std::abs(spec.to_radius - spec.radius) * std::tan(spec.taper);

    const Frame flat = flat_frame(spec.frame, wrap.frame);
    std::vector<ShapePtr> parts;
    for (const detail::SweepFace& face : detail::region_faces(spec.feature, flat, spec.regions)) {
      Bnd_Box box;
      BRepBndLib::Add(face.face, box);
      double xmin = 0.0, ymin = 0.0, zmin = 0.0, xmax = 0.0, ymax = 0.0, zmax = 0.0;
      box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
      if (xmax - xmin >= kTwoPi * spec.radius) {
        throw std::invalid_argument("the profiles reach around the cylinder: they are wider than its circumference");
      }
      parts.push_back(wrapped_solid(spec.feature, wrap, face));
    }
    return detail::finished(*detail::fused(parts, "the emboss"), "the emboss");
  });
}

} // namespace mitcad::geometry
