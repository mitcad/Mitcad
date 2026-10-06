// SPDX-License-Identifier: MIT
#include "mitcad/analysis/measure.hpp"

#include <algorithm>
#include <cmath>
#include <optional>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopExp.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Circ.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Sphere.hxx>

#include "convert.hpp"

namespace mitcad::analysis {

namespace {

constexpr double kPi = 3.14159265358979323846;

void require(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    throw Error("cannot measure a null shape");
  }
}

// Outward normal of a planar face.
std::optional<gp_Dir> plane_normal(const TopoDS_Shape& shape) {
  if (shape.ShapeType() != TopAbs_FACE) {
    return std::nullopt;
  }
  const TopoDS_Face& face = TopoDS::Face(shape);
  if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  // BRepGProp_Face gives the normal with the face orientation applied.
  gp_Pnt point;
  gp_Vec normal;
  BRepGProp_Face(face).Normal(0.5 * (u0 + u1), 0.5 * (v0 + v1), point, normal);
  if (normal.Magnitude() <= gp::Resolution()) {
    return std::nullopt;
  }
  return gp_Dir(normal);
}

// Direction of a linear edge, along its orientation.
std::optional<gp_Dir> line_direction(const TopoDS_Shape& shape) {
  if (shape.ShapeType() != TopAbs_EDGE) {
    return std::nullopt;
  }
  const TopoDS_Edge& edge = TopoDS::Edge(shape);
  if (BRep_Tool::Degenerated(edge)) {
    return std::nullopt;
  }
  const BRepAdaptor_Curve curve(edge);
  if (curve.GetType() != GeomAbs_Line) {
    return std::nullopt;
  }
  gp_Dir direction = curve.Line().Direction();
  if (edge.Orientation() == TopAbs_REVERSED) {
    direction.Reverse();
  }
  return direction;
}

// The vertex two edges share, if any.
std::optional<gp_Pnt> common_vertex(const TopoDS_Edge& a, const TopoDS_Edge& b, gp_Pnt& far_a,
                                    gp_Pnt& far_b) {
  TopoDS_Vertex a0;
  TopoDS_Vertex a1;
  TopoDS_Vertex b0;
  TopoDS_Vertex b1;
  TopExp::Vertices(a, a0, a1);
  TopExp::Vertices(b, b0, b1);
  const TopoDS_Vertex as[2] = {a0, a1};
  const TopoDS_Vertex bs[2] = {b0, b1};
  for (int i = 0; i < 2; ++i) {
    for (int j = 0; j < 2; ++j) {
      if (!as[i].IsNull() && as[i].IsSame(bs[j])) {
        far_a = BRep_Tool::Pnt(as[1 - i]);
        far_b = BRep_Tool::Pnt(bs[1 - j]);
        return BRep_Tool::Pnt(as[i]);
      }
    }
  }
  return std::nullopt;
}

double acute(double angle) { return angle > 0.5 * kPi ? kPi - angle : angle; }

} // namespace

Distance min_distance(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  require(a);
  require(b);
  BRepExtrema_DistShapeShape distance(a, b);
  if (!distance.IsDone() || distance.NbSolution() == 0) {
    throw Error("the distance could not be computed");
  }
  Distance result;
  result.value = distance.Value();
  result.on_a = detail::vec(distance.PointOnShape1(1));
  result.on_b = detail::vec(distance.PointOnShape2(1));
  result.inside = distance.InnerSolution();
  return result;
}

double angle(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  require(a);
  require(b);
  const std::optional<gp_Dir> normal_a = plane_normal(a);
  const std::optional<gp_Dir> normal_b = plane_normal(b);
  const std::optional<gp_Dir> line_a = line_direction(a);
  const std::optional<gp_Dir> line_b = line_direction(b);
  if (normal_a && normal_b) {
    return normal_a->Angle(*normal_b);
  }
  if (line_a && line_b) {
    gp_Pnt far_a;
    gp_Pnt far_b;
    if (const std::optional<gp_Pnt> corner =
            common_vertex(TopoDS::Edge(a), TopoDS::Edge(b), far_a, far_b)) {
      const gp_Vec to_a(*corner, far_a);
      const gp_Vec to_b(*corner, far_b);
      if (to_a.Magnitude() > gp::Resolution() && to_b.Magnitude() > gp::Resolution()) {
        return to_a.Angle(to_b);
      }
    }
    return acute(line_a->Angle(*line_b));
  }
  const std::optional<gp_Dir> normal = normal_a ? normal_a : normal_b;
  const std::optional<gp_Dir> line = line_a ? line_a : line_b;
  if (normal && line) {
    return std::abs(0.5 * kPi - normal->Angle(*line));
  }
  throw Error("an angle needs planar faces or linear edges");
}

double length(const TopoDS_Shape& shape) {
  require(shape);
  GProp_GProps props;
  BRepGProp::LinearProperties(shape, props, true);
  return props.Mass();
}

double area(const TopoDS_Shape& shape) {
  require(shape);
  return surface_properties(shape, true).Mass();
}

std::optional<Circle> circle(const TopoDS_Shape& shape) {
  require(shape);
  if (shape.ShapeType() == TopAbs_EDGE) {
    const TopoDS_Edge& edge = TopoDS::Edge(shape);
    if (BRep_Tool::Degenerated(edge)) {
      return std::nullopt;
    }
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() != GeomAbs_Circle) {
      return std::nullopt;
    }
    const gp_Circ c = curve.Circle();
    Circle result;
    result.center = detail::vec(c.Location());
    result.axis = detail::vec(c.Axis().Direction());
    result.radius = c.Radius();
    result.sweep = std::min(2.0 * kPi, curve.LastParameter() - curve.FirstParameter());
    return result;
  }
  if (shape.ShapeType() != TopAbs_FACE) {
    return std::nullopt;
  }
  const TopoDS_Face& face = TopoDS::Face(shape);
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() == GeomAbs_Sphere) {
    const gp_Sphere sphere = surface.Sphere();
    Circle result;
    result.center = detail::vec(sphere.Location());
    result.radius = sphere.Radius();
    return result;
  }
  if (surface.GetType() == GeomAbs_Cylinder) {
    const gp_Cylinder cylinder = surface.Cylinder();
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    const gp_Ax1 axis = cylinder.Axis();
    const gp_Vec offset(axis.Location(), props.CentreOfMass());
    const gp_Pnt center =
        axis.Location().Translated(gp_Vec(axis.Direction()) * offset.Dot(gp_Vec(axis.Direction())));
    Circle result;
    result.center = detail::vec(center);
    result.axis = detail::vec(axis.Direction());
    result.radius = cylinder.Radius();
    return result;
  }
  return std::nullopt;
}

Vec3 point(const TopoDS_Shape& vertex) {
  require(vertex);
  if (vertex.ShapeType() != TopAbs_VERTEX) {
    throw Error("a point needs a vertex");
  }
  return detail::vec(BRep_Tool::Pnt(TopoDS::Vertex(vertex)));
}

} // namespace mitcad::analysis
