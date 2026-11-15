// SPDX-License-Identifier: MIT
#include "render/RenderMesh.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <map>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomLProp_SLProps.hxx>
#include <Geom_Surface.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

namespace mitcad::render {

namespace {

// Whether a mesh's triangles close up: each edge between two of them, once
// each way. Vertices at the same place count as one (a file's triangulation
// may keep the sides' corners apart).
bool closed(const MeshData& mesh) {
  std::map<std::array<float, 3>, std::uint32_t> places;
  std::vector<std::uint32_t> same(mesh.positions.size() / 3);
  for (std::size_t v = 0; v < same.size(); ++v) {
    const std::array<float, 3> at{mesh.positions[v * 3], mesh.positions[v * 3 + 1], mesh.positions[v * 3 + 2]};
    same[v] = places.emplace(at, static_cast<std::uint32_t>(places.size())).first->second;
  }
  const std::vector<std::uint32_t>& indices = mesh.indices;
  std::map<std::pair<std::uint32_t, std::uint32_t>, int> edges;
  for (std::size_t i = 0; i + 2 < indices.size(); i += 3) {
    for (std::size_t k = 0; k < 3; ++k) {
      const std::uint32_t a = same[indices[i + k]];
      const std::uint32_t b = same[indices[i + (k + 1) % 3]];
      edges[{std::min(a, b), std::max(a, b)}] += a < b ? 1 : 100;
    }
  }
  return std::all_of(edges.begin(), edges.end(), [](const auto& edge) { return edge.second == 101; });
}

// The outward normal of a face's surface at a node of its triangulation, in
// the shape's coordinates (a zero vector where the surface has none there):
// the surface's normal, turned round where the face is reversed, moved with
// the surface's location. A mirroring location mirrors the solid's inside
// too, so the moved normal still points out of it.
gp_Vec surfaceNormalAt(GeomLProp_SLProps& props, const gp_Pnt2d& uv, const gp_Trsf& toShape, bool reversed) {
  props.SetParameters(uv.X(), uv.Y());
  if (!props.IsNormalDefined()) {
    return gp_Vec(0, 0, 0);
  }
  gp_Vec normal(props.Normal());
  normal.Transform(toShape);
  return reversed ? normal.Reversed() : normal;
}

// The outward normal of a face's surface at the point nearest to a point
// of the shape (for a triangulation without the nodes' surface parameters):
// a zero vector where it cannot be found.
gp_Vec surfaceNormalNear(const occ::handle<Geom_Surface>& surface, const gp_Trsf& toShape, bool reversed,
                         const gp_Pnt& point) {
  GeomAPI_ProjectPointOnSurf projection(point.Transformed(toShape.Inverted()), surface);
  if (!projection.IsDone() || projection.NbPoints() == 0) {
    return gp_Vec(0, 0, 0);
  }
  double u = 0.0;
  double v = 0.0;
  projection.LowerDistanceParameters(u, v);
  GeomLProp_SLProps props(surface, 1, 1e-9);
  return surfaceNormalAt(props, gp_Pnt2d(u, v), toShape, reversed);
}

} // namespace

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
  bool anySurface = false;
  std::vector<float>& uvs = mesh.uvs;
  // Whether a face had texture coordinates yet: from then on every face's
  // vertices have a pair (an empty `uvs` cannot say so when the first such
  // face is the shape's first).
  bool anyUvs = false;
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
    // The winding OCCT's triangulations have: along the surface's normal,
    // turned round where the face is reversed or mirrored by its location.
    const bool turned = reversed != toShape.IsNegative();
    const auto first = static_cast<std::uint32_t>(mesh.positions.size() / 3);
    const int nodes = triangulation->NbNodes();
    std::vector<gp_Pnt> points(static_cast<std::size_t>(nodes));
    for (int n = 1; n <= nodes; ++n) {
      const gp_Pnt p = triangulation->Node(n).Transformed(toShape);
      points[static_cast<std::size_t>(n - 1)] = p;
      mesh.positions.insert(mesh.positions.end(),
                            {static_cast<float>(p.X()), static_cast<float>(p.Y()), static_cast<float>(p.Z())});
    }
    // Where the face has a surface, its outward normals decide which way
    // each triangle faces (mitcad#319): a triangulation made elsewhere (an
    // imported file's, one kept on a shape that moved) may wind the other
    // way, and glass, which tells inside from outside by the winding, would
    // trap the light (render black).
    TopLoc_Location surfaceLocation;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, surfaceLocation);
    anySurface = anySurface || !surface.IsNull();
    // A mesh body's UV nodes are its texture coordinates (mitcad#1862; a
    // face with a surface has its surface's parameters there instead).
    // Faces before the first that has them get zeros, as faces after
    // without them do.
    const bool texcoords = surface.IsNull() && triangulation->HasUVNodes();
    if (texcoords && !anyUvs) {
      anyUvs = true;
      uvs.assign(static_cast<std::size_t>(first) * 2, 0.0f);
    }
    if (anyUvs) {
      for (int n = 1; n <= nodes; ++n) {
        const gp_Pnt2d uv = texcoords ? triangulation->UVNode(n) : gp_Pnt2d(0.0, 0.0);
        uvs.insert(uvs.end(), {static_cast<float>(uv.X()), static_cast<float>(uv.Y())});
      }
    }
    std::vector<gp_Vec> outward;
    if (!surface.IsNull() && triangulation->HasUVNodes()) {
      GeomLProp_SLProps props(surface, 1, 1e-9);
      outward.reserve(static_cast<std::size_t>(nodes));
      for (int n = 1; n <= nodes; ++n) {
        outward.push_back(
            surfaceNormalAt(props, triangulation->UVNode(n), surfaceLocation.Transformation(), reversed));
      }
    }
    std::vector<std::array<int, 3>> corners;
    std::vector<gp_Vec> facets; // each triangle's normal by its winding (twice its area)
    corners.reserve(static_cast<std::size_t>(triangulation->NbTriangles()));
    facets.reserve(static_cast<std::size_t>(triangulation->NbTriangles()));
    for (int t = 1; t <= triangulation->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      triangulation->Triangle(t).Get(a, b, c);
      if (turned) {
        std::swap(b, c);
      }
      std::array<int, 3> corner{a - 1, b - 1, c - 1};
      const gp_Pnt& pa = points[static_cast<std::size_t>(corner[0])];
      gp_Vec facet = gp_Vec(pa, points[static_cast<std::size_t>(corner[1])])
                         .Crossed(gp_Vec(pa, points[static_cast<std::size_t>(corner[2])]));
      if (!outward.empty()) {
        const gp_Vec out = outward[static_cast<std::size_t>(corner[0])] +
                           outward[static_cast<std::size_t>(corner[1])] +
                           outward[static_cast<std::size_t>(corner[2])];
        if (out.SquareMagnitude() > 0.0 && facet.Dot(out) < 0.0) {
          std::swap(corner[1], corner[2]);
          facet.Reverse();
          ++mesh.turned;
        }
      }
      corners.push_back(corner);
      facets.push_back(facet);
    }
    // Without the nodes' surface parameters: the face as a whole, by the
    // surface's normal at its largest triangle.
    if (!surface.IsNull() && outward.empty() && !corners.empty()) {
      std::size_t largest = 0;
      for (std::size_t t = 1; t < facets.size(); ++t) {
        if (facets[t].SquareMagnitude() > facets[largest].SquareMagnitude()) {
          largest = t;
        }
      }
      const std::array<int, 3>& corner = corners[largest];
      const gp_XYZ middle = (points[static_cast<std::size_t>(corner[0])].XYZ() +
                             points[static_cast<std::size_t>(corner[1])].XYZ() +
                             points[static_cast<std::size_t>(corner[2])].XYZ()) /
                            3.0;
      const gp_Vec out = surfaceNormalNear(surface, surfaceLocation.Transformation(), reversed, gp_Pnt(middle));
      if (out.SquareMagnitude() > 0.0 && facets[largest].Dot(out) < 0.0) {
        for (std::size_t t = 0; t < corners.size(); ++t) {
          std::swap(corners[t][1], corners[t][2]);
          facets[t].Reverse();
        }
        mesh.turned += static_cast<std::uint32_t>(corners.size());
      }
    }
    // Normals averaged over the face's triangles, weighted by their area
    // (the cross product's length), pointing out of the solid.
    std::vector<gp_Vec> normals(static_cast<std::size_t>(nodes), gp_Vec(0, 0, 0));
    for (std::size_t t = 0; t < corners.size(); ++t) {
      for (const int node : corners[t]) {
        normals[static_cast<std::size_t>(node)] += facets[t];
      }
      mesh.indices.insert(mesh.indices.end(), {first + static_cast<std::uint32_t>(corners[t][0]),
                                               first + static_cast<std::uint32_t>(corners[t][1]),
                                               first + static_cast<std::uint32_t>(corners[t][2])});
    }
    mesh.faceTriangles[static_cast<std::size_t>(f - 1)] = static_cast<std::uint32_t>(corners.size());
    for (gp_Vec& normal : normals) {
      const double length = normal.Magnitude();
      if (length > 0.0) {
        normal /= length;
      }
      mesh.normals.insert(mesh.normals.end(), {static_cast<float>(normal.X()), static_cast<float>(normal.Y()),
                                               static_cast<float>(normal.Z())});
    }
  }
  // A mesh body (faces without surfaces, such as an imported STL file's)
  // has only its winding to tell its outside: a closed one wound inwards
  // has a negative volume, and is turned round as a whole.
  if (!anySurface && !mesh.indices.empty() && closed(mesh)) {
    double volume = 0.0;
    const auto at = [&mesh](std::uint32_t index) {
      return gp_XYZ(mesh.positions[index * 3], mesh.positions[index * 3 + 1], mesh.positions[index * 3 + 2]);
    };
    for (std::size_t i = 0; i + 2 < mesh.indices.size(); i += 3) {
      volume += at(mesh.indices[i]).Dot(at(mesh.indices[i + 1]).Crossed(at(mesh.indices[i + 2])));
    }
    if (volume < 0.0) {
      for (std::size_t i = 0; i + 2 < mesh.indices.size(); i += 3) {
        std::swap(mesh.indices[i + 1], mesh.indices[i + 2]);
      }
      for (float& component : mesh.normals) {
        component = -component;
      }
      mesh.turned += static_cast<std::uint32_t>(mesh.indices.size() / 3);
    }
  }
  return mesh;
}

} // namespace mitcad::render
