// SPDX-License-Identifier: MIT
#include "mitcad/analysis/common.hpp"

#include <array>
#include <map>
#include <utility>

#include <BRepAdaptor_Curve2d.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Domain.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepGProp_MeshProps.hxx>
#include <BRepGProp_Sinert.hxx>
#include <BRepGProp_Vinert.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_Map.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp.hxx>
#include <gp_Trsf.hxx>

#include "convert.hpp"

namespace mitcad::analysis {

namespace detail {

bool has_mesh_faces(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

bool closed_mesh(const TopoDS_Shape& shape) {
  // Each triangle edge, keyed by its end points, must be used exactly twice.
  using Point = std::array<double, 3>;
  std::map<std::pair<Point, Point>, int> edges;
  bool any = false;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh =
        BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location);
    if (mesh.IsNull()) {
      return false;
    }
    const gp_Trsf transform = location.Transformation();
    for (int i = 1; i <= mesh->NbTriangles(); ++i) {
      int n[3] = {0, 0, 0};
      mesh->Triangle(i).Get(n[0], n[1], n[2]);
      Point p[3];
      for (int k = 0; k < 3; ++k) {
        const gp_Pnt q = mesh->Node(n[k]).Transformed(transform);
        p[k] = {q.X(), q.Y(), q.Z()};
      }
      // Degenerate triangles (at poles of the surface) enclose nothing.
      if (p[0] == p[1] || p[1] == p[2] || p[0] == p[2]) {
        continue;
      }
      for (int k = 0; k < 3; ++k) {
        const Point& a = p[k];
        const Point& b = p[(k + 1) % 3];
        ++edges[a < b ? std::make_pair(a, b) : std::make_pair(b, a)];
      }
      any = true;
    }
  }
  if (!any) {
    return false;
  }
  for (const auto& [edge, count] : edges) {
    if (count != 2) {
      return false;
    }
  }
  return true;
}

} // namespace detail

Bounds bounds(const TopoDS_Shape& shape) {
  Bounds result;
  if (shape.IsNull()) {
    return result;
  }
  Bnd_Box box;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    if (!BRep_Tool::Surface(face, location).IsNull()) {
      BRepBndLib::AddOptimal(face, box, false, false);
      continue;
    }
    // A mesh face: its nodes, without the face tolerance.
    const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
    if (!mesh.IsNull()) {
      const gp_Trsf transform = location.Transformation();
      for (int i = 1; i <= mesh->NbNodes(); ++i) {
        box.Add(mesh->Node(i).Transformed(transform));
      }
    }
  }
  // Edges and vertices outside faces (wires, points).
  for (TopExp_Explorer it(shape, TopAbs_EDGE, TopAbs_FACE); it.More(); it.Next()) {
    BRepBndLib::AddOptimal(it.Current(), box, false, false);
  }
  for (TopExp_Explorer it(shape, TopAbs_VERTEX, TopAbs_EDGE); it.More(); it.Next()) {
    BRepBndLib::AddOptimal(it.Current(), box, false, false);
  }
  if (box.IsVoid()) {
    return result;
  }
  box.Get(result.min.x, result.min.y, result.min.z, result.max.x, result.max.y, result.max.z);
  result.empty = false;
  return result;
}

namespace {

// Faces OCCT's fixed Gauss points integrate well: planes, quadrics, tori
// and polynomial B-spline and Bezier patches, bounded by lines, conics and
// polynomial curves in their parameter space. Rational patches and curves
// (a cylinder scaled non-uniformly has both), surfaces of revolution and
// extrusion and offset surfaces need the adaptive integration.
bool fixed_points_suffice(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  switch (surface.GetType()) {
  case GeomAbs_Plane:
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
  case GeomAbs_Sphere:
  case GeomAbs_Torus:
    break;
  case GeomAbs_BezierSurface:
  case GeomAbs_BSplineSurface:
    if (surface.IsURational() || surface.IsVRational()) {
      return false;
    }
    break;
  default:
    return false;
  }
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const BRepAdaptor_Curve2d curve(TopoDS::Edge(it.Current()), face);
    switch (curve.GetType()) {
    case GeomAbs_BezierCurve:
    case GeomAbs_BSplineCurve:
      if (curve.IsRational()) {
        return false;
      }
      break;
    case GeomAbs_OffsetCurve:
    case GeomAbs_OtherCurve:
      return false;
    default:
      break;
    }
  }
  return true;
}

// The point the integrals of a shape are taken about: the mean of its
// vertices, as OCCT takes it (any point gives the same totals; a near one
// keeps them accurate).
gp_Pnt reference_point(const TopoDS_Shape& shape) {
  gp_XYZ sum(0.0, 0.0, 0.0);
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_VERTEX); it.More(); it.Next()) {
    sum += BRep_Tool::Pnt(TopoDS::Vertex(it.Current())).XYZ();
    ++count;
  }
  return gp_Pnt(count > 0 ? sum / count : sum);
}

enum class Integral { Volume, Surface };

// Adds a face's integral about `at`: over its triangles for a mesh face,
// with fixed or adaptive Gauss points for a surface.
template <class Inert>
void add_surface_face(GProp_GProps& total, const TopoDS_Face& face, const gp_Pnt& at) {
  BRepGProp_Face surface(face);
  Inert props;
  props.SetLocation(at);
  const bool natural = face.NbChildren() == 0;
  BRepGProp_Domain domain;
  if (!natural) {
    domain.Init(face);
  }
  if (fixed_points_suffice(face)) {
    if (natural) {
      props.Perform(surface);
    } else {
      props.Perform(surface, domain);
    }
  } else if (natural) {
    props.Perform(surface, kIntegrationTolerance);
  } else {
    props.Perform(surface, domain, kIntegrationTolerance);
  }
  total.Add(props);
}

void add_face(GProp_GProps& total, const TopoDS_Face& face, const gp_Pnt& at, Integral integral) {
  TopLoc_Location location;
  if (BRep_Tool::Surface(face, location).IsNull()) {
    const occ::handle<Poly_Triangulation>& mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull() || mesh->NbNodes() == 0 || mesh->NbTriangles() == 0) {
      return;
    }
    BRepGProp_MeshProps props(integral == Integral::Volume ? BRepGProp_MeshProps::Vinert
                                                           : BRepGProp_MeshProps::Sinert);
    props.SetLocation(at);
    props.Perform(mesh, location, face.Orientation());
    total.Add(props);
  } else if (integral == Integral::Volume) {
    add_surface_face<BRepGProp_Vinert>(total, face, at);
  } else {
    add_surface_face<BRepGProp_Sinert>(total, face, at);
  }
}

using ShapeSet = NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>;

} // namespace

GProp_GProps volume_properties(const TopoDS_Shape& shape, bool only_closed, bool skip_shared) {
  GProp_GProps total(gp::Origin());
  const gp_Pnt at = reference_point(shape);
  // A volume integral counts the faces that bound material.
  ShapeSet forward;
  ShapeSet reversed;
  const auto add_faces = [&](const TopoDS_Shape& part) {
    for (TopExp_Explorer it(part, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      const TopAbs_Orientation orientation = face.Orientation();
      if (orientation != TopAbs_FORWARD && orientation != TopAbs_REVERSED) {
        continue;
      }
      if (skip_shared && !(orientation == TopAbs_FORWARD ? forward : reversed).Add(face)) {
        continue;
      }
      add_face(total, face, at, Integral::Volume);
    }
  };
  if (!only_closed) {
    add_faces(shape);
    return total;
  }
  ShapeSet shells;
  for (TopExp_Explorer it(shape, TopAbs_SHELL); it.More(); it.Next()) {
    if ((!skip_shared || shells.Add(it.Current())) && BRep_Tool::IsClosed(it.Current())) {
      add_faces(it.Current());
    }
  }
  return total;
}

GProp_GProps surface_properties(const TopoDS_Shape& shape, bool skip_shared) {
  GProp_GProps total(gp::Origin());
  const gp_Pnt at = reference_point(shape);
  ShapeSet seen;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    if (!skip_shared || seen.Add(it.Current())) {
      add_face(total, TopoDS::Face(it.Current()), at, Integral::Surface);
    }
  }
  return total;
}

TopoDS_Shape oriented_solids(const TopoDS_Shape& shape) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    GProp_GProps props;
    BRepGProp::VolumeProperties(it.Current(), props);
    builder.Add(compound, props.Mass() < 0.0 ? it.Current().Reversed() : it.Current());
  }
  return compound;
}

} // namespace mitcad::analysis
