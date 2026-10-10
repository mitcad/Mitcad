// SPDX-License-Identifier: MIT
#include "mitcad/analysis/common.hpp"

#include <algorithm>
#include <array>
#include <exception>
#include <map>
#include <optional>
#include <utility>
#include <vector>

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
#include <OSD_Parallel.hxx>
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
#include "face_integral.hpp"

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

using detail::Integral;

// OCCT's fixed rule, for a face reaching to infinity.
template <class Inert>
GProp_GProps unbounded_face(const TopoDS_Face& face, const gp_Pnt& at) {
  BRepGProp_Face surface(face);
  Inert props;
  props.SetLocation(at);
  if (face.NbChildren() == 0) {
    props.Perform(surface);
  } else {
    BRepGProp_Domain domain(face);
    props.Perform(surface, domain);
  }
  return props;
}

// A face's integral about `at`: over its triangles for a mesh face, span by
// span over its surface otherwise (detail::integrate_face).
std::optional<GProp_GProps> face_integral(const TopoDS_Face& face, const gp_Pnt& at, Integral integral) {
  TopLoc_Location location;
  if (BRep_Tool::Surface(face, location).IsNull()) {
    const occ::handle<Poly_Triangulation>& mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull() || mesh->NbNodes() == 0 || mesh->NbTriangles() == 0) {
      return std::nullopt;
    }
    BRepGProp_MeshProps props(integral == Integral::Volume ? BRepGProp_MeshProps::Vinert
                                                           : BRepGProp_MeshProps::Sinert);
    props.SetLocation(at);
    props.Perform(mesh, location, face.Orientation());
    return props;
  }
  if (std::optional<GProp_GProps> props = detail::integrate_face(face, at, integral)) {
    return props;
  }
  if (integral == Integral::Volume) {
    return unbounded_face<BRepGProp_Vinert>(face, at);
  }
  return unbounded_face<BRepGProp_Sinert>(face, at);
}

// The sum of the faces' integrals. The faces are integrated on all cores
// and added in their order, so the sum is the same as one face after
// another: the import measures every body of hundreds of stored states,
// which took half of a large design's time on one core.
GProp_GProps sum_of_faces(const std::vector<TopoDS_Face>& faces, const gp_Pnt& at, Integral integral) {
  std::vector<std::optional<GProp_GProps>> parts(faces.size());
  std::vector<std::exception_ptr> errors(faces.size());
  OSD_Parallel::For(
      0, static_cast<int>(faces.size()),
      [&](int i) {
        const auto k = static_cast<std::size_t>(i);
        try {
          parts[k] = face_integral(faces[k], at, integral);
        } catch (...) {
          errors[k] = std::current_exception();
        }
      },
      faces.size() < 8);
  GProp_GProps total(gp::Origin());
  for (std::size_t k = 0; k < faces.size(); ++k) {
    if (errors[k]) {
      std::rethrow_exception(errors[k]);
    }
    if (parts[k]) {
      total.Add(*parts[k]);
    }
  }
  return total;
}

using ShapeSet = NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>;

} // namespace

GProp_GProps volume_properties(const TopoDS_Shape& shape, bool only_closed, bool skip_shared) {
  const gp_Pnt at = reference_point(shape);
  // A volume integral counts the faces that bound material.
  ShapeSet forward;
  ShapeSet reversed;
  std::vector<TopoDS_Face> faces;
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
      faces.push_back(face);
    }
  };
  if (!only_closed) {
    add_faces(shape);
  } else {
    ShapeSet shells;
    for (TopExp_Explorer it(shape, TopAbs_SHELL); it.More(); it.Next()) {
      if ((!skip_shared || shells.Add(it.Current())) && BRep_Tool::IsClosed(it.Current())) {
        add_faces(it.Current());
      }
    }
  }
  return sum_of_faces(faces, at, Integral::Volume);
}

GProp_GProps surface_properties(const TopoDS_Shape& shape, bool skip_shared) {
  const gp_Pnt at = reference_point(shape);
  ShapeSet seen;
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    if (!skip_shared || seen.Add(it.Current())) {
      faces.push_back(TopoDS::Face(it.Current()));
    }
  }
  return sum_of_faces(faces, at, Integral::Surface);
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
