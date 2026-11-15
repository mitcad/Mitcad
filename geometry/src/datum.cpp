// SPDX-License-Identifier: MIT
#include "mitcad/geometry/datum.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <memory>
#include <mutex>
#include <optional>
#include <stdexcept>
#include <unordered_map>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_CurveToAnaCurve.hxx>
#include <GeomConvert_SurfToAnaSurf.hxx>
#include <GeomLib_IsPlanarSurface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_ElementarySurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Precision.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Vec.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Points closer than this are the same point, mm (as in the model).
constexpr double kLinear = 1e-6;
// Directions closer than this (sine of the angle) are parallel.
constexpr double kAngular = 1e-9;
// Ends of path edges closer than this are connected, mm.
constexpr double kConnected = 1e-4;
// A stored surface or curve within this of a plane, cylinder, cone,
// sphere, torus or circle is that one, mm (mitcad#1855).
constexpr double kAnalytic = 1e-6;

const char* surface_type_name(GeomAbs_SurfaceType type) {
  switch (type) {
  case GeomAbs_BezierSurface:
    return "bezier";
  case GeomAbs_BSplineSurface:
    return "bspline";
  case GeomAbs_SurfaceOfRevolution:
    return "revolution";
  case GeomAbs_SurfaceOfExtrusion:
    return "extrusion";
  case GeomAbs_OffsetSurface:
    return "offset";
  default:
    return "other";
  }
}

const char* curve_type_name(GeomAbs_CurveType type) {
  switch (type) {
  case GeomAbs_Ellipse:
    return "ellipse";
  case GeomAbs_Hyperbola:
    return "hyperbola";
  case GeomAbs_Parabola:
    return "parabola";
  case GeomAbs_BezierCurve:
    return "bezier";
  case GeomAbs_BSplineCurve:
    return "bspline";
  case GeomAbs_OffsetCurve:
    return "offset";
  default:
    return "other";
  }
}

gp_Pnt face_centre(const TopoDS_Face& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.CentreOfMass();
}

gp_Pnt project(const gp_Pnt& point, const gp_Ax1& axis) {
  const gp_Vec along(axis.Direction());
  return axis.Location().Translated(along * gp_Vec(axis.Location(), point).Dot(along));
}

// The outward normal of a planar face (BRepGProp_Face applies the face
// orientation).
gp_Dir plane_normal(const TopoDS_Face& face) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  gp_Pnt point;
  gp_Vec normal;
  BRepGProp_Face(face).Normal(0.5 * (u0 + u1), 0.5 * (v0 + v1), point, normal);
  if (normal.Magnitude() <= gp::Resolution()) {
    throw std::runtime_error("the face has no normal");
  }
  return gp_Dir(normal);
}

// Axes of surfaces of revolution point along their largest component, so
// the direction does not depend on how the kernel built the surface.
gp_Dir canonical(const gp_Dir& axis) {
  const gp_XYZ v = axis.XYZ();
  double largest = v.X();
  for (const double c : {v.Y(), v.Z()}) {
    if (std::abs(c) > std::abs(largest) + 1e-12) {
      largest = c;
    }
  }
  return largest < 0.0 ? axis.Reversed() : axis;
}

// How far from a point the analytic surface is, mm (planes, cylinders,
// cones, spheres and tori; infinite for other surfaces).
double distance_to(const Adaptor3d_Surface& surface, const gp_Pnt& point) {
  // The point's distance along an axis from its origin and from the axis.
  const auto axial = [&point](const gp_Ax1& axis, double& at, double& off) {
    at = gp_Vec(axis.Location(), point).Dot(gp_Vec(axis.Direction()));
    off = gp_Lin(axis).Distance(point);
  };
  double along = 0.0;
  double from_axis = 0.0;
  switch (surface.GetType()) {
  case GeomAbs_Plane:
    return surface.Plane().Distance(point);
  case GeomAbs_Cylinder: {
    const gp_Cylinder cylinder = surface.Cylinder();
    return std::abs(gp_Lin(cylinder.Axis()).Distance(point) - cylinder.Radius());
  }
  case GeomAbs_Cone: {
    // The radius at `along` from the reference plane is R + along tan(a);
    // the generator's slope turns the radial gap into the distance.
    const gp_Cone cone = surface.Cone();
    axial(cone.Axis(), along, from_axis);
    const double radius = cone.RefRadius() + along * std::tan(cone.SemiAngle());
    return std::abs(from_axis - radius) * std::cos(cone.SemiAngle());
  }
  case GeomAbs_Sphere: {
    const gp_Sphere sphere = surface.Sphere();
    return std::abs(point.Distance(sphere.Location()) - sphere.Radius());
  }
  case GeomAbs_Torus: {
    const gp_Torus torus = surface.Torus();
    axial(torus.Axis(), along, from_axis);
    return std::abs(std::hypot(from_axis - torus.MajorRadius(), along) - torus.MinorRadius());
  }
  default:
    return std::numeric_limits<double>::infinity();
  }
}

// Whether every point of a grid over the face's parameter range lies
// within kAnalytic of the analytic surface (in the face's place).
bool face_within(const TopoDS_Face& face, const Adaptor3d_Surface& analytic) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  if (!(u1 > u0) || !(v1 > v0) || Precision::IsInfinite(u0) || Precision::IsInfinite(u1) ||
      Precision::IsInfinite(v0) || Precision::IsInfinite(v1)) {
    return false;
  }
  const BRepAdaptor_Surface own(face, false);
  constexpr int kSamples = 7;
  for (int i = 0; i < kSamples; ++i) {
    for (int j = 0; j < kSamples; ++j) {
      const gp_Pnt point = own.Value(u0 + (u1 - u0) * i / (kSamples - 1), v0 + (v1 - v0) * j / (kSamples - 1));
      if (!(distance_to(analytic, point) <= kAnalytic)) {
        return false;
      }
    }
  }
  return true;
}

// A circle `direction` sweeps along into a cylinder: the circle's axis is
// the direction.
occ::handle<Geom_Surface> extruded_circle(const occ::handle<Geom_SurfaceOfLinearExtrusion>& extrusion) {
  occ::handle<Geom_Curve> basis = extrusion->BasisCurve();
  double first = basis->FirstParameter();
  double last = basis->LastParameter();
  if (!(basis->IsKind(STANDARD_TYPE(Geom_Circle)) || basis->IsKind(STANDARD_TYPE(Geom_TrimmedCurve)))) {
    // A circle stored as a spline (a profile converted for an approximation).
    if (Precision::IsInfinite(first) || Precision::IsInfinite(last)) {
      return {};
    }
    GeomConvert_CurveToAnaCurve convert(basis);
    occ::handle<Geom_Curve> analytic;
    double new_first = 0.0;
    double new_last = 0.0;
    if (!convert.ConvertToAnalytical(kAnalytic, analytic, first, last, new_first, new_last) || analytic.IsNull() ||
        convert.Gap() > kAnalytic) {
      return {};
    }
    basis = analytic;
  }
  while (basis->IsKind(STANDARD_TYPE(Geom_TrimmedCurve))) {
    basis = occ::down_cast<Geom_TrimmedCurve>(basis)->BasisCurve();
  }
  const occ::handle<Geom_Circle> circle = occ::down_cast<Geom_Circle>(basis);
  if (circle.IsNull() || !circle->Axis().Direction().IsParallel(extrusion->Direction(), kAngular * 1000.0)) {
    return {};
  }
  return new Geom_CylindricalSurface(gp_Ax3(circle->Position()), circle->Radius());
}

// The plane, cylinder, cone, sphere or torus a face lies on, in the face's
// place: its own surface, or the one its stored surface (an extrusion, a
// revolution, a B-spline, a Bezier surface) lies within kAnalytic of over
// the face (mitcad#1855: the walls of holes in sheet bodies are B-spline
// cylinders). Null for other faces.
occ::handle<Geom_Surface> recognised_surface(const TopoDS_Face& face) {
  TopLoc_Location location;
  occ::handle<Geom_Surface> stored = BRep_Tool::Surface(face, location);
  if (stored.IsNull()) {
    return {};
  }
  while (stored->IsKind(STANDARD_TYPE(Geom_RectangularTrimmedSurface))) {
    stored = occ::down_cast<Geom_RectangularTrimmedSurface>(stored)->BasisSurface();
  }
  const auto placed = [&location](const occ::handle<Geom_Surface>& surface) -> occ::handle<Geom_Surface> {
    if (location.IsIdentity()) {
      return surface;
    }
    return occ::down_cast<Geom_Surface>(surface->Transformed(location.Transformation()));
  };
  if (stored->IsKind(STANDARD_TYPE(Geom_ElementarySurface))) {
    return placed(stored);
  }
  try {
    OCC_CATCH_SIGNALS
    occ::handle<Geom_Surface> analytic;
    const occ::handle<Geom_SurfaceOfLinearExtrusion> extrusion = occ::down_cast<Geom_SurfaceOfLinearExtrusion>(stored);
    if (!extrusion.IsNull()) {
      analytic = extruded_circle(extrusion);
    }
    if (analytic.IsNull()) {
      occ::handle<Geom_Surface> spline = stored;
      if (!stored->IsKind(STANDARD_TYPE(Geom_BSplineSurface)) && !stored->IsKind(STANDARD_TYPE(Geom_BezierSurface))) {
        // Revolutions, extrusions and the rest as B-splines over the face.
        double u0 = 0.0;
        double u1 = 0.0;
        double v0 = 0.0;
        double v1 = 0.0;
        BRepTools::UVBounds(face, u0, u1, v0, v1);
        if (!(u1 > u0) || !(v1 > v0) || Precision::IsInfinite(u0) || Precision::IsInfinite(u1) ||
            Precision::IsInfinite(v0) || Precision::IsInfinite(v1)) {
          return {};
        }
        spline = GeomConvert::SurfaceToBSplineSurface(new Geom_RectangularTrimmedSurface(stored, u0, u1, v0, v1));
      }
      GeomConvert_SurfToAnaSurf convert(spline);
      analytic = convert.ConvertToAnalytical(kAnalytic);
      if (analytic.IsNull() || convert.Gap() > kAnalytic) {
        return {};
      }
    }
    if (!analytic->IsKind(STANDARD_TYPE(Geom_ElementarySurface))) {
      return {};
    }
    analytic = placed(analytic);
    // Checked over the face itself, whatever the conversion measured.
    if (!face_within(face, GeomAdaptor_Surface(analytic))) {
      return {};
    }
    return analytic;
  } catch (const Standard_Failure&) {
    return {};
  }
}

// The circle an edge runs on, in the direction of its parameter (the edge
// runs counter-clockwise about the circle's axis): its own, or the one its
// stored curve (a B-spline, a Bezier curve, an ellipse with equal radii)
// lies within kAnalytic of (mitcad#1855).
std::optional<gp_Circ> recognised_circle(const TopoDS_Edge& forward_edge) {
  if (BRep_Tool::Degenerated(forward_edge)) {
    return std::nullopt;
  }
  try {
    OCC_CATCH_SIGNALS
    const BRepAdaptor_Curve curve(forward_edge);
    switch (curve.GetType()) {
    case GeomAbs_Circle:
      return curve.Circle();
    case GeomAbs_Line:
      return std::nullopt;
    default:
      break;
    }
    TopLoc_Location location;
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> stored = BRep_Tool::Curve(forward_edge, location, first, last);
    if (stored.IsNull()) {
      return std::nullopt;
    }
    GeomConvert_CurveToAnaCurve convert(stored);
    occ::handle<Geom_Curve> analytic;
    double new_first = 0.0;
    double new_last = 0.0;
    if (!convert.ConvertToAnalytical(kAnalytic, analytic, first, last, new_first, new_last) || analytic.IsNull() ||
        convert.Gap() > kAnalytic) {
      return std::nullopt;
    }
    while (analytic->IsKind(STANDARD_TYPE(Geom_TrimmedCurve))) {
      analytic = occ::down_cast<Geom_TrimmedCurve>(analytic)->BasisCurve();
    }
    const occ::handle<Geom_Circle> found = occ::down_cast<Geom_Circle>(analytic);
    if (found.IsNull()) {
      return std::nullopt;
    }
    gp_Circ circle = found->Circ();
    if (!location.IsIdentity()) {
      circle.Transform(location.Transformation());
    }
    // Checked along the edge itself; the axis turned so that the edge runs
    // counter-clockwise about it.
    constexpr int kSamples = 9;
    for (int i = 0; i < kSamples; ++i) {
      const gp_Pnt point =
          curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / (kSamples - 1));
      if (!(circle.Distance(point) <= kAnalytic)) {
        return std::nullopt;
      }
    }
    gp_Pnt middle;
    gp_Vec tangent;
    curve.D1(0.5 * (curve.FirstParameter() + curve.LastParameter()), middle, tangent);
    const gp_Vec round = gp_Vec(circle.Axis().Direction()).Crossed(gp_Vec(circle.Location(), middle));
    if (tangent.Dot(round) < 0.0) {
      circle.SetAxis(circle.Axis().Reversed());
    }
    return circle;
  } catch (const Standard_Failure&) {
    return std::nullopt;
  }
}

// A cylinder's, cone's, sphere's or torus's description (the other
// surfaces: false).
bool describe_round(const Adaptor3d_Surface& surface, const TopoDS_Face& face, SurfaceDescription& result) {
  switch (surface.GetType()) {
  case GeomAbs_Cylinder: {
    const gp_Cylinder cylinder = surface.Cylinder();
    result.type = "cylinder";
    result.axis = canonical(cylinder.Axis().Direction());
    result.origin = project(face_centre(face), cylinder.Axis());
    result.radius = cylinder.Radius();
    return true;
  }
  case GeomAbs_Cone: {
    const gp_Cone cone = surface.Cone();
    result.type = "cone";
    result.axis = canonical(cone.Axis().Direction());
    result.origin = project(face_centre(face), cone.Axis());
    // The radius grows by tan(half angle) along the axis.
    const double sign = result.axis.Dot(cone.Axis().Direction()) > 0.0 ? 1.0 : -1.0;
    const double along = gp_Vec(cone.Location(), result.origin).Dot(gp_Vec(cone.Axis().Direction()));
    result.radius = cone.RefRadius() + along * std::tan(cone.SemiAngle());
    result.half_angle = sign * cone.SemiAngle();
    return true;
  }
  case GeomAbs_Sphere: {
    const gp_Sphere sphere = surface.Sphere();
    result.type = "sphere";
    result.origin = sphere.Location();
    result.radius = sphere.Radius();
    return true;
  }
  case GeomAbs_Torus: {
    const gp_Torus torus = surface.Torus();
    result.type = "torus";
    result.origin = torus.Location();
    result.axis = canonical(torus.Axis().Direction());
    result.radius = torus.MajorRadius();
    result.minor_radius = torus.MinorRadius();
    return true;
  }
  default:
    return false;
  }
}

// A face's surface; without `planar_splines`, a face on another surface
// than the analytic ones gets only its type. With it, a face stored as
// another surface that is a plane, cylinder, cone, sphere or torus within
// kAnalytic is that one (planes: mitcad#1537; the rest: mitcad#1855).
SurfaceDescription describe(const TopoDS_Face& face, bool planar_splines = true) {
  SurfaceDescription result;
  TopLoc_Location location;
  if (BRep_Tool::Surface(face, location).IsNull()) {
    // A mesh body's triangles (mitcad#399): no surface to describe.
    result.type = "mesh";
    return result;
  }
  const BRepAdaptor_Surface surface(face);
  std::optional<gp_Pln> plane;
  switch (surface.GetType()) {
  case GeomAbs_Plane:
    plane = surface.Plane();
    break;
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
  case GeomAbs_Sphere:
  case GeomAbs_Torus:
    describe_round(surface, face, result);
    return result;
  default: {
    if (!planar_splines) {
      result.type = surface_type_name(surface.GetType());
      return result;
    }
    // Imported faces may be planes stored as splines.
    GeomLib_IsPlanarSurface planar(BRep_Tool::Surface(face), kLinear);
    if (planar.IsPlanar()) {
      plane = planar.Plan();
      break;
    }
    // And holes in sheet bodies cylinders stored as splines.
    if (const occ::handle<Geom_Surface> analytic = recognised_surface(face);
        !analytic.IsNull() && describe_round(GeomAdaptor_Surface(analytic), face, result)) {
      return result;
    }
    result.type = surface_type_name(surface.GetType());
    return result;
  }
  }
  result.type = "plane";
  result.axis = plane_normal(face);
  const gp_XYZ normal = result.axis.XYZ();
  result.origin = gp_Pnt(normal * normal.Dot(plane->Location().XYZ()));
  return result;
}

bool parallel(const gp_Dir& a, const gp_Dir& b) {
  return a.XYZ().Crossed(b.XYZ()).Modulus() < kAngular && a.Dot(b) > 0.0;
}

double distance_to_line(const gp_Pnt& point, const gp_Pnt& origin, const gp_Dir& direction) {
  return gp_Lin(origin, direction).Distance(point);
}

// The pieces of a split face lie on one surface.
bool same_surface(const SurfaceDescription& a, const SurfaceDescription& b) {
  if (a.type != b.type) {
    return false;
  }
  if (a.type == "plane") {
    return parallel(a.axis, b.axis) && a.origin.Distance(b.origin) < kLinear;
  }
  if (a.type == "cylinder" || a.type == "cone") {
    const double along = gp_Vec(a.origin, b.origin).Dot(gp_Vec(a.axis));
    const double radius = a.radius + along * std::tan(a.half_angle);
    return parallel(a.axis, b.axis) && distance_to_line(b.origin, a.origin, a.axis) < kLinear &&
           std::abs(radius - b.radius) < kLinear && std::abs(a.half_angle - b.half_angle) < kAngular;
  }
  if (a.type == "sphere" || a.type == "torus") {
    const bool axes = a.type == "sphere" || parallel(a.axis, b.axis);
    return axes && a.origin.Distance(b.origin) < kLinear && std::abs(a.radius - b.radius) < kLinear &&
           std::abs(a.minor_radius - b.minor_radius) < kLinear;
  }
  return false;
}

std::vector<int> require_faces(const Shape& shape, const std::string& face) {
  std::vector<int> found = shape.find_faces(face);
  if (found.empty()) {
    throw std::runtime_error("the body has no such face");
  }
  return found;
}

const TopoDS_Edge& require_edge(const Shape& shape, const std::string& edge) {
  const std::vector<int> found = shape.find_edges(edge);
  if (found.empty()) {
    throw std::runtime_error("the body has no such edge");
  }
  if (found.size() > 1) {
    throw std::runtime_error("the name means " + std::to_string(found.size()) +
                             " edges; add #k to name one");
  }
  return shape.edge(found.front());
}

TopoDS_Edge forward(const TopoDS_Edge& edge) {
  if (BRep_Tool::Degenerated(edge)) {
    throw std::runtime_error("the edge is degenerate");
  }
  return TopoDS::Edge(edge.Oriented(TopAbs_FORWARD));
}

// Whether every pole lies within kLinear of the line from `start` to `end`
// and between them: the curve, inside its poles' hull, runs on that
// segment.
template <class Curve>
bool poles_on_segment(const Curve& curve, const gp_Pnt& start, const gp_Pnt& end) {
  const gp_Vec along(start, end);
  const double length = along.Magnitude();
  if (length <= kLinear) {
    return false;
  }
  const gp_Dir direction(along);
  const gp_Lin line(start, direction);
  for (int i = 1; i <= curve->NbPoles(); ++i) {
    const gp_Pnt pole = curve->Pole(i);
    const double at = gp_Vec(start, pole).Dot(gp_Vec(direction));
    if (line.Distance(pole) > kLinear || at < -kLinear || at > length + kLinear) {
      return false;
    }
  }
  return true;
}

// The curve of an edge, in the direction of its parameter; with
// `straight_splines`, a straight spline or Bezier curve is a line, and a
// curve within kAnalytic of a circle that circle (mitcad#1855).
CurveDescription describe_edge(const TopoDS_Edge& forward_edge, bool straight_splines = false) {
  const BRepAdaptor_Curve curve(forward_edge);
  CurveDescription result;
  result.start = curve.Value(curve.FirstParameter());
  result.end = curve.Value(curve.LastParameter());
  if (straight_splines && curve.GetType() != GeomAbs_Line && curve.GetType() != GeomAbs_Circle) {
    if (is_straight(forward_edge)) {
      result.type = "line";
      return result;
    }
    if (const std::optional<gp_Circ> circle = recognised_circle(forward_edge)) {
      result.type = "circle";
      result.center = circle->Location();
      result.normal = circle->Axis().Direction();
      result.radius = circle->Radius();
      return result;
    }
  }
  switch (curve.GetType()) {
  case GeomAbs_Line:
    result.type = "line";
    break;
  case GeomAbs_Circle: {
    const gp_Circ circle = curve.Circle();
    result.type = "circle";
    result.center = circle.Location();
    result.normal = circle.Axis().Direction();
    result.radius = circle.Radius();
    break;
  }
  default:
    result.type = curve_type_name(curve.GetType());
  }
  return result;
}

// An edge of a path, run forwards or backwards.
struct PathEdge {
  TopoDS_Edge edge;
  bool reversed = false;
  double length = 0.0;
};

std::pair<gp_Pnt, gp_Pnt> ends(const PathEdge& piece) {
  const BRepAdaptor_Curve curve(piece.edge);
  const gp_Pnt first = curve.Value(curve.FirstParameter());
  const gp_Pnt last = curve.Value(curve.LastParameter());
  return piece.reversed ? std::make_pair(last, first) : std::make_pair(first, last);
}

// The point at `along` mm from the start of the piece, in its direction.
PathPoint on_piece(const PathEdge& piece, double along) {
  const BRepAdaptor_Curve curve(piece.edge);
  const double start = piece.reversed ? curve.LastParameter() : curve.FirstParameter();
  const GCPnts_AbscissaPoint at(curve, piece.reversed ? -along : along, start);
  if (!at.IsDone()) {
    throw std::runtime_error("the point along the path could not be found");
  }
  gp_Pnt point;
  gp_Vec tangent;
  curve.D1(at.Parameter(), point, tangent);
  if (tangent.Magnitude() <= gp::Resolution()) {
    throw std::runtime_error("the path has no tangent there");
  }
  return {point, gp_Dir(piece.reversed ? -tangent : tangent)};
}

std::vector<PathEdge> chain(const Shape& shape, const std::vector<std::string>& names) {
  if (names.empty()) {
    throw std::invalid_argument("the path has no edges");
  }
  std::vector<PathEdge> pieces;
  for (const std::string& name : names) {
    PathEdge piece;
    piece.edge = forward(require_edge(shape, name));
    piece.length = GCPnts_AbscissaPoint::Length(BRepAdaptor_Curve(piece.edge));
    pieces.push_back(piece);
  }
  const auto touches = [](const gp_Pnt& point, const PathEdge& piece) {
    const auto [first, last] = ends(piece);
    return point.Distance(first) < kConnected || point.Distance(last) < kConnected;
  };
  if (pieces.size() > 1 && !touches(ends(pieces[0]).second, pieces[1])) {
    pieces[0].reversed = true;
  }
  for (std::size_t i = 0; i + 1 < pieces.size(); ++i) {
    const gp_Pnt end = ends(pieces[i]).second;
    if (end.Distance(ends(pieces[i + 1]).first) < kConnected) {
      continue;
    }
    pieces[i + 1].reversed = true;
    if (end.Distance(ends(pieces[i + 1]).first) >= kConnected) {
      throw std::runtime_error("the path's edges are not connected in the order given");
    }
  }
  return pieces;
}

} // namespace

SurfaceDescription face_geometry(const Shape& shape, const std::string& face) {
  return detail::run("face geometry", [&] {
    const std::vector<int> found = require_faces(shape, face);
    const SurfaceDescription first = describe(shape.face(found.front()));
    for (std::size_t i = 1; i < found.size(); ++i) {
      if (!same_surface(first, describe(shape.face(found[i])))) {
        throw std::runtime_error("the name means " + std::to_string(found.size()) +
                                 " faces on different surfaces; add #k to name one");
      }
    }
    return first;
  });
}

CurveDescription edge_geometry(const Shape& shape, const std::string& edge) {
  return detail::run("edge geometry",
                     [&] { return describe_edge(forward(require_edge(shape, edge)), true); });
}

bool is_straight(const TopoDS_Edge& edge) {
  if (edge.IsNull() || BRep_Tool::Degenerated(edge)) {
    return false;
  }
  try {
    const BRepAdaptor_Curve curve(edge);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    const gp_Pnt start = curve.Value(first);
    const gp_Pnt end = curve.Value(last);
    switch (curve.GetType()) {
    case GeomAbs_Line:
      return start.Distance(end) > kLinear;
    case GeomAbs_BSplineCurve: {
      // The edge's own part of the spline, in the edge's location.
      Handle(Geom_BSplineCurve) spline = curve.BSpline();
      if (spline.IsNull()) {
        return false;
      }
      const double low = std::max(first, spline->FirstParameter());
      const double high = std::min(last, spline->LastParameter());
      if (!spline->IsPeriodic() && high > low &&
          (low > spline->FirstParameter() || high < spline->LastParameter())) {
        spline = Handle(Geom_BSplineCurve)::DownCast(spline->Copy());
        spline->Segment(low, high);
      }
      return poles_on_segment(spline, start, end);
    }
    case GeomAbs_BezierCurve: {
      Handle(Geom_BezierCurve) bezier = curve.Bezier();
      if (bezier.IsNull()) {
        return false;
      }
      const double low = std::max(first, bezier->FirstParameter());
      const double high = std::min(last, bezier->LastParameter());
      if (high > low && (low > bezier->FirstParameter() || high < bezier->LastParameter())) {
        bezier = Handle(Geom_BezierCurve)::DownCast(bezier->Copy());
        bezier->Segment(low, high);
      }
      return poles_on_segment(bezier, start, end);
    }
    default:
      return false;
    }
  } catch (const Standard_Failure&) {
    return false;
  }
}

std::optional<gp_Pln> face_plane(const TopoDS_Face& face) {
  TopLoc_Location location;
  if (face.IsNull() || BRep_Tool::Surface(face, location).IsNull()) {
    return std::nullopt; // a mesh body's triangles
  }
  try {
    const BRepAdaptor_Surface surface(face, false);
    gp_Pnt on;
    switch (surface.GetType()) {
    case GeomAbs_Plane:
      on = surface.Plane().Location();
      break;
    case GeomAbs_Cylinder:
    case GeomAbs_Cone:
    case GeomAbs_Sphere:
    case GeomAbs_Torus:
      return std::nullopt;
    default: {
      // As describe() reads it: imported faces and the walls of sheet
      // bodies may be planes stored as splines.
      GeomLib_IsPlanarSurface planar(BRep_Tool::Surface(face), kLinear);
      if (!planar.IsPlanar()) {
        return std::nullopt;
      }
      on = planar.Plan().Location();
    }
    }
    // The outward normal (a spline's plane may point either way).
    return gp_Pln(on, plane_normal(face));
  } catch (const Standard_Failure&) {
    return std::nullopt;
  } catch (const std::runtime_error&) {
    return std::nullopt; // no normal
  }
}

occ::handle<Geom_Surface> analytic_surface(const TopoDS_Face& face) {
  if (face.IsNull()) {
    return {};
  }
  return recognised_surface(face);
}

const char* analytic_kind(const TopoDS_Face& face) {
  TopLoc_Location location;
  if (face.IsNull() || BRep_Tool::Surface(face, location).IsNull()) {
    return nullptr; // a mesh body's triangles
  }
  try {
    switch (BRepAdaptor_Surface(face, false).GetType()) {
    case GeomAbs_Plane:
      return "plane";
    case GeomAbs_Cylinder:
      return "cylinder";
    case GeomAbs_Cone:
      return "cone";
    case GeomAbs_Sphere:
      return "sphere";
    case GeomAbs_Torus:
      return "torus";
    default:
      break;
    }
  } catch (const Standard_Failure&) {
    return nullptr;
  }
  if (face_plane(face)) {
    return "plane";
  }
  // The recognition of a large spline takes a while, and a pick asks on
  // every move of the pointer: the answers are kept by the face's shape
  // (its location does not change the kind; the kept face keeps the
  // shape alive, so its address is not reused). Never freed: a static's
  // destructor would release OCCT handles after OCCT's own statics.
  struct Kinds {
    std::mutex mutex;
    std::unordered_map<const void*, std::pair<TopoDS_Face, const char*>> known;
  };
  static Kinds* const kinds = new Kinds();
  const void* key = face.TShape().get();
  {
    const std::lock_guard<std::mutex> lock(kinds->mutex);
    const auto found = kinds->known.find(key);
    if (found != kinds->known.end()) {
      return found->second.second;
    }
  }
  const char* kind = nullptr;
  if (const occ::handle<Geom_Surface> analytic = recognised_surface(face); !analytic.IsNull()) {
    switch (GeomAdaptor_Surface(analytic).GetType()) {
    case GeomAbs_Cylinder:
      kind = "cylinder";
      break;
    case GeomAbs_Cone:
      kind = "cone";
      break;
    case GeomAbs_Sphere:
      kind = "sphere";
      break;
    case GeomAbs_Torus:
      kind = "torus";
      break;
    default:
      break;
    }
  }
  constexpr std::size_t kKept = 4096;
  const std::lock_guard<std::mutex> lock(kinds->mutex);
  if (kinds->known.size() >= kKept) {
    kinds->known.clear();
  }
  kinds->known.emplace(key, std::make_pair(face, kind));
  return kind;
}

bool is_circular(const TopoDS_Edge& edge) {
  if (edge.IsNull()) {
    return false;
  }
  return recognised_circle(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD))).has_value();
}

std::vector<SurfaceDescription> face_geometries(const Shape& shape) {
  return detail::run("face geometries", [&] {
    std::vector<SurfaceDescription> result;
    result.reserve(static_cast<std::size_t>(shape.face_count()));
    for (int i = 0; i < shape.face_count(); ++i) {
      result.push_back(describe(shape.face(i), false));
    }
    return result;
  });
}

std::vector<CurveDescription> edge_geometries(const Shape& shape) {
  // Kept by the shape once worked out (Shape::kept_edge_curves); kept only
  // when the run did not fail.
  if (const auto kept = std::static_pointer_cast<const std::vector<CurveDescription>>(shape.kept_edge_curves())) {
    return *kept;
  }
  auto curves = std::make_shared<const std::vector<CurveDescription>>(detail::run("edge geometries", [&] {
    std::vector<CurveDescription> result;
    result.reserve(static_cast<std::size_t>(shape.edge_count()));
    for (int i = 0; i < shape.edge_count(); ++i) {
      const TopoDS_Edge& edge = shape.edge(i);
      if (BRep_Tool::Degenerated(edge)) {
        CurveDescription degenerate;
        degenerate.type = "degenerate";
        result.push_back(degenerate);
      } else {
        result.push_back(describe_edge(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD))));
      }
    }
    return result;
  }));
  shape.keep_edge_curves(curves);
  return *curves;
}

std::vector<SurfaceDescription> recognised_face_geometries(const Shape& shape, const std::vector<int>& faces) {
  return detail::run("recognised face geometries", [&] {
    std::vector<SurfaceDescription> result;
    result.reserve(faces.size());
    for (const int i : faces) {
      if (i < 0 || i >= shape.face_count()) {
        throw std::out_of_range("a face asked for is past the body's faces");
      }
      const TopoDS_Face& face = shape.face(i);
      // analytic_kind keeps its answer for faces on none of the surfaces,
      // so that a body of many free-form faces is quick to ask again; a
      // face it finds on one is read in full.
      if (analytic_kind(face) == nullptr) {
        result.push_back(describe(face, false));
      } else {
        result.push_back(describe(face, true));
      }
    }
    return result;
  });
}

std::vector<CurveDescription> recognised_edge_geometries(const Shape& shape, const std::vector<int>& edges) {
  return detail::run("recognised edge geometries", [&] {
    std::vector<CurveDescription> result;
    result.reserve(edges.size());
    for (const int i : edges) {
      if (i < 0 || i >= shape.edge_count()) {
        throw std::out_of_range("an edge asked for is past the body's edges");
      }
      const TopoDS_Edge& edge = shape.edge(i);
      if (BRep_Tool::Degenerated(edge)) {
        CurveDescription degenerate;
        degenerate.type = "degenerate";
        result.push_back(degenerate);
      } else {
        result.push_back(describe_edge(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD)), true));
      }
    }
    return result;
  });
}

gp_Pnt vertex_point(const Shape& shape, const std::string& vertex) {
  const std::vector<int> found = shape.find_vertices(vertex);
  if (found.empty()) {
    throw std::runtime_error("the body has no such vertex");
  }
  if (found.size() > 1) {
    throw std::runtime_error("the name means " + std::to_string(found.size()) +
                             " vertices; add #k to name one");
  }
  return BRep_Tool::Pnt(shape.vertex(found.front()));
}

PathPoint path_point(const Shape& shape, const std::vector<std::string>& edges, PathAt at,
                     double value, const gp_Pnt& near) {
  return detail::run("path", [&] {
    const std::vector<PathEdge> pieces = chain(shape, edges);
    if (at == PathAt::Near) {
      double best = std::numeric_limits<double>::infinity();
      PathPoint result;
      for (const PathEdge& piece : pieces) {
        double first = 0.0;
        double last = 0.0;
        const Handle(Geom_Curve) curve = BRep_Tool::Curve(piece.edge, first, last);
        std::vector<double> candidates{first, last};
        GeomAPI_ProjectPointOnCurve projector(near, curve, first, last);
        if (projector.NbPoints() > 0) {
          candidates.push_back(projector.LowerDistanceParameter());
        }
        for (const double parameter : candidates) {
          const double distance = near.Distance(curve->Value(parameter));
          if (distance < best) {
            best = distance;
            gp_Pnt point;
            gp_Vec tangent;
            BRepAdaptor_Curve(piece.edge).D1(parameter, point, tangent);
            if (tangent.Magnitude() <= gp::Resolution()) {
              continue;
            }
            result = {point, gp_Dir(piece.reversed ? -tangent : tangent)};
          }
        }
      }
      return result;
    }
    double total = 0.0;
    for (const PathEdge& piece : pieces) {
      total += piece.length;
    }
    detail::require_finite("path distance", value);
    double along = at == PathAt::Fraction ? value * total : value;
    if (along < 0.0) {
      // Before the start, straight along the start tangent.
      const PathPoint start = on_piece(pieces.front(), 0.0);
      return PathPoint{start.point.Translated(gp_Vec(start.tangent) * along), start.tangent};
    }
    for (const PathEdge& piece : pieces) {
      if (along <= piece.length) {
        return on_piece(piece, along);
      }
      along -= piece.length;
    }
    const PathPoint end = on_piece(pieces.back(), pieces.back().length);
    return PathPoint{end.point.Translated(gp_Vec(end.tangent) * along), end.tangent};
  });
}

SurfacePoint face_point_normal(const Shape& shape, const std::string& face, const gp_Pnt& near) {
  return detail::run("face point", [&] {
    const TopoDS_Shape vertex = BRepBuilderAPI_MakeVertex(near).Vertex();
    double best = std::numeric_limits<double>::infinity();
    std::optional<SurfacePoint> result;
    for (const int index : require_faces(shape, face)) {
      const TopoDS_Face& candidate = shape.face(index);
      BRepExtrema_DistShapeShape distance(vertex, candidate);
      if (!distance.IsDone() || distance.NbSolution() == 0 || distance.Value() >= best) {
        continue;
      }
      const gp_Pnt on = distance.PointOnShape2(1);
      const Handle(ShapeAnalysis_Surface) surface =
          new ShapeAnalysis_Surface(BRep_Tool::Surface(candidate));
      const gp_Pnt2d uv = surface->ValueOfUV(on, kLinear);
      gp_Pnt point;
      gp_Vec normal;
      BRepGProp_Face(candidate).Normal(uv.X(), uv.Y(), point, normal);
      if (normal.Magnitude() <= gp::Resolution()) {
        continue;
      }
      best = distance.Value();
      result = SurfacePoint{on, gp_Dir(normal)};
    }
    if (!result) {
      throw std::runtime_error("the face has no normal near the point");
    }
    return *result;
  });
}

ShapePtr plane_shape(const gp_Pnt& origin, const gp_Dir& normal, const gp_Dir& x_axis,
                     double size) {
  detail::require_positive("datum size", size);
  return detail::run("datum display", [&] {
    const double half = 0.5 * size;
    BRepBuilderAPI_MakeFace face(gp_Pln(gp_Ax3(origin, normal, x_axis)), -half, half, -half, half);
    return std::make_shared<Shape>(face.Face());
  });
}

ShapePtr axis_shape(const gp_Pnt& origin, const gp_Dir& direction, double size) {
  detail::require_positive("datum size", size);
  return detail::run("datum display", [&] {
    const double half = 0.5 * size;
    return std::make_shared<Shape>(
        BRepBuilderAPI_MakeEdge(gp_Lin(origin, direction), -half, half).Edge());
  });
}

ShapePtr point_shape(const gp_Pnt& point) {
  return detail::run("datum display", [&] {
    return std::make_shared<Shape>(BRepBuilderAPI_MakeVertex(point).Vertex());
  });
}

} // namespace mitcad::geometry
