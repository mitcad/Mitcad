// SPDX-License-Identifier: MIT
#include "mitcad/geometry/datum.hpp"

#include <cmath>
#include <limits>
#include <optional>
#include <stdexcept>
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
#include <GeomLib_IsPlanarSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <ShapeAnalysis_Surface.hxx>
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

SurfaceDescription describe(const TopoDS_Face& face) {
  SurfaceDescription result;
  const BRepAdaptor_Surface surface(face);
  std::optional<gp_Pln> plane;
  switch (surface.GetType()) {
  case GeomAbs_Plane:
    plane = surface.Plane();
    break;
  case GeomAbs_Cylinder: {
    const gp_Cylinder cylinder = surface.Cylinder();
    result.type = "cylinder";
    result.axis = canonical(cylinder.Axis().Direction());
    result.origin = project(face_centre(face), cylinder.Axis());
    result.radius = cylinder.Radius();
    return result;
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
    return result;
  }
  case GeomAbs_Sphere: {
    const gp_Sphere sphere = surface.Sphere();
    result.type = "sphere";
    result.origin = sphere.Location();
    result.radius = sphere.Radius();
    return result;
  }
  case GeomAbs_Torus: {
    const gp_Torus torus = surface.Torus();
    result.type = "torus";
    result.origin = torus.Location();
    result.axis = canonical(torus.Axis().Direction());
    result.radius = torus.MajorRadius();
    result.minor_radius = torus.MinorRadius();
    return result;
  }
  default: {
    // Imported faces may be planes stored as splines.
    GeomLib_IsPlanarSurface planar(BRep_Tool::Surface(face), kLinear);
    if (planar.IsPlanar()) {
      plane = planar.Plan();
    } else {
      result.type = surface_type_name(surface.GetType());
      return result;
    }
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
  return detail::run("edge geometry", [&] {
    const TopoDS_Edge forward_edge = forward(require_edge(shape, edge));
    const BRepAdaptor_Curve curve(forward_edge);
    CurveDescription result;
    result.start = curve.Value(curve.FirstParameter());
    result.end = curve.Value(curve.LastParameter());
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
