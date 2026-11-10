// SPDX-License-Identifier: MIT
#include "mitcad/io/face_mesh.hpp"

#include <cstddef>
#include <limits>
#include <utility>

#include <BRep_Tool.hxx>
#include <GeomLProp_SLProps.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/io/body.hpp"

namespace mitcad::io {

namespace {

std::array<double, 3> xyz(const gp_XYZ& p) { return {p.X(), p.Y(), p.Z()}; }

std::uint32_t index_of(std::size_t i) {
  if (i > std::numeric_limits<std::uint32_t>::max()) {
    throw Error("the mesh has too many vertices");
  }
  return static_cast<std::uint32_t>(i);
}

} // namespace

FaceMeshes face_meshes(const TopoDS_Shape& shape, const MeshOptions& options) {
  const TopoDS_Shape meshed = triangulate(shape, options);
  FaceMeshes mesh;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(meshed, TopAbs_FACE, faces);
  mesh.face_triangles.reserve(static_cast<std::size_t>(faces.Extent()));
  for (int f = 1; f <= faces.Extent(); ++f) {
    const TopoDS_Face& face = TopoDS::Face(faces(f));
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      throw untriangulated_face(meshed, face);
    }
    const gp_Trsf placement = location.Transformation();
    const bool reversed = face.Orientation() == TopAbs_REVERSED;
    // The triangles follow the surface's normal: turned round where the
    // face is reversed, or mirrored by its placement.
    const bool turned = reversed != placement.IsNegative();
    const int nodes = triangulation->NbNodes();
    const int count = triangulation->NbTriangles();
    std::vector<gp_Pnt> points(static_cast<std::size_t>(nodes));
    for (int n = 1; n <= nodes; ++n) {
      points[static_cast<std::size_t>(n - 1)] = triangulation->Node(n).Transformed(placement);
    }
    std::vector<std::array<int, 3>> corners;
    corners.reserve(static_cast<std::size_t>(count));
    for (int t = 1; t <= count; ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      triangulation->Triangle(t).Get(a, b, c);
      if (turned) {
        std::swap(b, c);
      }
      corners.push_back({a - 1, b - 1, c - 1});
    }
    const auto facet = [&](const std::array<int, 3>& t) {
      const gp_Pnt& pa = points[static_cast<std::size_t>(t[0])];
      return gp_Vec(pa, points[static_cast<std::size_t>(t[1])])
          .Crossed(gp_Vec(pa, points[static_cast<std::size_t>(t[2])]));
    };
    TopLoc_Location surface_location;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, surface_location);
    if (surface.IsNull()) {
      // A mesh body: faceted, each triangle with its own vertices.
      for (const std::array<int, 3>& t : corners) {
        gp_Vec normal = facet(t);
        const double length = normal.Magnitude();
        if (length > 0.0) {
          normal /= length;
        }
        const std::uint32_t first = index_of(mesh.vertices.size());
        for (const int node : t) {
          mesh.vertices.push_back(xyz(points[static_cast<std::size_t>(node)].XYZ()));
          mesh.normals.push_back(xyz(normal.XYZ()));
        }
        mesh.triangles.push_back({first, first + 1, first + 2});
      }
      mesh.face_triangles.push_back(static_cast<std::uint32_t>(count));
      continue;
    }
    // The normals of the face's triangles around each vertex (area
    // weighted), for points where the surface has none.
    std::vector<gp_Vec> around(static_cast<std::size_t>(nodes), gp_Vec(0.0, 0.0, 0.0));
    for (const std::array<int, 3>& t : corners) {
      const gp_Vec normal = facet(t);
      for (const int node : t) {
        around[static_cast<std::size_t>(node)] += normal;
      }
    }
    const std::uint32_t first = index_of(mesh.vertices.size());
    const gp_Trsf to_shape = surface_location.Transformation();
    GeomLProp_SLProps props(surface, 1, 1e-9);
    for (int n = 1; n <= nodes; ++n) {
      gp_Vec normal = around[static_cast<std::size_t>(n - 1)];
      if (triangulation->HasUVNodes()) {
        const gp_Pnt2d uv = triangulation->UVNode(n);
        props.SetParameters(uv.X(), uv.Y());
        if (props.IsNormalDefined()) {
          gp_Vec surface_normal(props.Normal());
          surface_normal.Transform(to_shape);
          if (reversed) {
            surface_normal.Reverse();
          }
          normal = surface_normal;
        }
      }
      const double length = normal.Magnitude();
      if (length > 0.0) {
        normal /= length;
      }
      mesh.vertices.push_back(xyz(points[static_cast<std::size_t>(n - 1)].XYZ()));
      mesh.normals.push_back(xyz(normal.XYZ()));
    }
    for (const std::array<int, 3>& t : corners) {
      mesh.triangles.push_back({first + static_cast<std::uint32_t>(t[0]), first + static_cast<std::uint32_t>(t[1]),
                                first + static_cast<std::uint32_t>(t[2])});
    }
    mesh.face_triangles.push_back(static_cast<std::uint32_t>(count));
  }
  return mesh;
}

} // namespace mitcad::io
