// SPDX-License-Identifier: MIT
#include "render/RenderMesh.hpp"

#include <cmath>

#include <BRepBndLib.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

namespace mitcad::render {

MeshData meshOf(const TopoDS_Shape& shape) {
  MeshData mesh;
  // A face without a triangulation (not drawn yet) is meshed as finely as
  // the view meshes: a deflection of a thousandth of the shape's size.
  double deflection = 0.0;
  // The faces in the order the application's shapes number them
  // (geometry::Shape: TopExp::MapShapes), so that faces with appearances of
  // their own are found by their index.
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  mesh.faceTriangles.assign(static_cast<std::size_t>(faces.Extent()), 0);
  for (int f = 1; f <= faces.Extent(); ++f) {
    const TopoDS_Face& face = TopoDS::Face(faces(f));
    TopLoc_Location location;
    occ::handle<Poly_Triangulation> triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      if (deflection <= 0.0) {
        Bnd_Box box;
        BRepBndLib::Add(shape, box, false);
        deflection = box.IsVoid() ? 0.1 : std::max(1e-3, std::sqrt(box.SquareExtent()) * 1e-3);
      }
      BRepMesh_IncrementalMesh(face, deflection, false, 0.5);
      triangulation = BRep_Tool::Triangulation(face, location);
      if (triangulation.IsNull()) {
        continue;
      }
    }
    const gp_Trsf toShape = location.Transformation();
    const bool reversed = face.Orientation() == TopAbs_REVERSED;
    const auto first = static_cast<std::uint32_t>(mesh.positions.size() / 3);
    const int nodes = triangulation->NbNodes();
    for (int n = 1; n <= nodes; ++n) {
      const gp_Pnt p = triangulation->Node(n).Transformed(toShape);
      mesh.positions.insert(mesh.positions.end(),
                            {static_cast<float>(p.X()), static_cast<float>(p.Y()), static_cast<float>(p.Z())});
    }
    // Normals averaged over the face's triangles, weighted by their area
    // (the cross product's length), pointing out of the solid.
    std::vector<gp_Vec> normals(static_cast<std::size_t>(nodes), gp_Vec(0, 0, 0));
    for (int t = 1; t <= triangulation->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      triangulation->Triangle(t).Get(a, b, c);
      if (reversed) {
        std::swap(b, c);
      }
      const gp_Pnt pa = triangulation->Node(a).Transformed(toShape);
      const gp_Vec normal = gp_Vec(pa, triangulation->Node(b).Transformed(toShape))
                                .Crossed(gp_Vec(pa, triangulation->Node(c).Transformed(toShape)));
      for (const int node : {a, b, c}) {
        normals[static_cast<std::size_t>(node - 1)] += normal;
      }
      mesh.indices.insert(mesh.indices.end(), {first + static_cast<std::uint32_t>(a - 1),
                                               first + static_cast<std::uint32_t>(b - 1),
                                               first + static_cast<std::uint32_t>(c - 1)});
    }
    mesh.faceTriangles[static_cast<std::size_t>(f - 1)] = static_cast<std::uint32_t>(triangulation->NbTriangles());
    for (gp_Vec& normal : normals) {
      const double length = normal.Magnitude();
      if (length > 0.0) {
        normal /= length;
      }
      mesh.normals.insert(mesh.normals.end(), {static_cast<float>(normal.X()), static_cast<float>(normal.Y()),
                                               static_cast<float>(normal.Z())});
    }
  }
  return mesh;
}

} // namespace mitcad::render
