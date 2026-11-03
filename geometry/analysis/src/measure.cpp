// SPDX-License-Identifier: MIT
#include "mitcad/analysis/measure.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepClass3d_SolidExplorer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Circ.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
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

// The plane of a planar face (its location applied).
std::optional<gp_Pln> face_plane(const TopoDS_Shape& shape) {
  if (shape.ShapeType() != TopAbs_FACE) {
    return std::nullopt;
  }
  const BRepAdaptor_Surface surface(TopoDS::Face(shape));
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  return surface.Plane();
}

// Whether a point of a face's plane lies in the face or on its boundary.
bool in_face(const TopoDS_Face& face, const gp_Pnt& point) {
  const double tolerance = std::max(BRep_Tool::Tolerance(face), Precision::Confusion());
  const BRepClass_FaceClassifier classifier(face, point, tolerance);
  const TopAbs_State state = classifier.State();
  return state == TopAbs_IN || state == TopAbs_ON;
}

// Points of a face to measure from (mitcad#716): a point inside it, then
// its vertices.
std::vector<gp_Pnt> points_of(const TopoDS_Face& face) {
  std::vector<gp_Pnt> points;
  gp_Pnt inner;
  if (BRepClass3d_SolidExplorer::FindAPointInTheFace(face, inner) && in_face(face, inner)) {
    points.push_back(inner);
  }
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
  TopExp::MapShapes(face, TopAbs_VERTEX, vertices);
  for (int i = 1; i <= vertices.Extent(); ++i) {
    points.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices.FindKey(i))));
  }
  return points;
}

// A point of `from` whose foot on the plane of `to` lies in `to`: the
// point and its foot.
std::optional<std::pair<gp_Pnt, gp_Pnt>> over(const TopoDS_Face& from, const TopoDS_Face& to,
                                               const gp_Pln& plane) {
  const gp_Vec normal(plane.Axis().Direction());
  for (const gp_Pnt& from_point : points_of(from)) {
    const double height = gp_Vec(plane.Location(), from_point).Dot(normal);
    const gp_Pnt foot = from_point.Translated(normal * -height);
    if (in_face(to, foot)) {
      return std::make_pair(from_point, foot);
    }
  }
  return std::nullopt;
}

// Two planar faces in parallel planes (mitcad#716): every point of one is
// at least the planes' gap from the other, so a point of one whose foot on
// the other's plane lies in the other gives the minimum, with that point
// and its foot as the closest points. BRepExtrema_DistShapeShape would
// instead pair every edge and vertex of the two faces, as its bounding
// boxes rule none of them out at the same gap: seconds for two walls with
// rows of holes. None when the faces are not such or no such point is
// found (faces side by side, or overlapping only away from the points
// tried). Parallel within Precision::Angular(): over faces of a few metres
// the planes then differ by nanometres at most.
std::optional<Distance> parallel_faces(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  const std::optional<gp_Pln> plane_a = face_plane(a);
  const std::optional<gp_Pln> plane_b = face_plane(b);
  if (!plane_a || !plane_b ||
      !plane_a->Axis().Direction().IsParallel(plane_b->Axis().Direction(), Precision::Angular())) {
    return std::nullopt;
  }
  const TopoDS_Face& face_a = TopoDS::Face(a);
  const TopoDS_Face& face_b = TopoDS::Face(b);
  gp_Pnt on_a;
  gp_Pnt on_b;
  if (const auto from_a = over(face_a, face_b, *plane_b)) {
    on_a = from_a->first;
    on_b = from_a->second;
  } else if (const auto from_b = over(face_b, face_a, *plane_a)) {
    on_a = from_b->second;
    on_b = from_b->first;
  } else {
    return std::nullopt;
  }
  Distance result;
  result.value = on_a.Distance(on_b);
  result.on_a = detail::vec(on_a);
  result.on_b = detail::vec(on_b);
  result.parallel_faces = true;
  return result;
}

} // namespace

Distance min_distance(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  require(a);
  require(b);
  if (std::optional<Distance> gap = parallel_faces(a, b)) {
    return *gap;
  }
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
    const GProp_GProps props = surface_properties(face);
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
