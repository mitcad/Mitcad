// SPDX-License-Identifier: MIT
#include "mitcad/io/mesh.hpp"

#include <cmath>
#include <unordered_map>
#include <utility>

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <IMeshTools_Parameters.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "mitcad/io/body.hpp"

namespace mitcad::io {

namespace {
constexpr double kDegree = 3.14159265358979323846 / 180.0;
}

MeshOptions MeshOptions::from(Refinement refinement) {
  switch (refinement) {
  case Refinement::Low:
    return MeshOptions{0.1, 30.0 * kDegree, false};
  case Refinement::Medium:
    return MeshOptions{0.03, 15.0 * kDegree, false};
  case Refinement::High:
    return MeshOptions{0.01, 8.0 * kDegree, false};
  }
  return MeshOptions{};
}

namespace {

bool has_surfaceless_face(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

} // namespace

bool is_mesh(const TopoDS_Shape& shape) {
  bool any = false;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (!BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return false;
    }
    any = true;
  }
  return any;
}

TopoDS_Shape triangulate(const TopoDS_Shape& shape, const MeshOptions& options) {
  if (shape.IsNull()) {
    throw Error("cannot triangulate a null shape");
  }
  if (!(options.linear_deflection > 0.0) || !(options.angular_deflection > 0.0)) {
    throw Error("mesh deflections must be greater than zero");
  }
  if (has_surfaceless_face(shape)) {
    return shape;
  }
  // A topology copy without triangulations: the original keeps its own.
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(shape, false, false).Shape();
  IMeshTools_Parameters parameters;
  parameters.Deflection = options.linear_deflection;
  parameters.Angle = options.angular_deflection;
  parameters.Relative = options.relative;
  parameters.InParallel = true;
  BRepMesh_IncrementalMesh mesher(copy, parameters);
  if (!mesher.IsDone()) {
    throw Error("triangulation failed");
  }
  return copy;
}

MeshStats mesh_stats(const TopoDS_Shape& shape) {
  MeshStats stats;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    ++stats.faces;
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh =
        BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location);
    if (!mesh.IsNull()) {
      stats.triangles += static_cast<std::size_t>(mesh->NbTriangles());
      stats.nodes += static_cast<std::size_t>(mesh->NbNodes());
    }
  }
  return stats;
}

TopoDS_Shape mesh_body(const occ::handle<Poly_Triangulation>& triangulation) {
  if (triangulation.IsNull() || triangulation->NbTriangles() == 0) {
    throw Error("empty triangulation");
  }
  TopoDS_Face face;
  BRep_Builder().MakeFace(face, triangulation);
  return face;
}

namespace {

// Vertices merged within a tolerance: each goes into a grid cell of the
// tolerance's size and is looked up in the cells around its own.
class VertexMerger {
public:
  VertexMerger(double tolerance, std::vector<std::array<double, 3>>& vertices)
      : m_tolerance(tolerance), m_vertices(vertices) {}

  std::uint32_t add(const gp_Pnt& p) {
    const Cell cell{key(p.X()), key(p.Y()), key(p.Z())};
    for (std::int64_t dx = -1; dx <= 1; ++dx) {
      for (std::int64_t dy = -1; dy <= 1; ++dy) {
        for (std::int64_t dz = -1; dz <= 1; ++dz) {
          const auto found = m_cells.find(Cell{cell.x + dx, cell.y + dy, cell.z + dz});
          if (found == m_cells.end()) {
            continue;
          }
          for (const std::uint32_t index : found->second) {
            const std::array<double, 3>& q = m_vertices[index];
            const double d2 = (p.X() - q[0]) * (p.X() - q[0]) + (p.Y() - q[1]) * (p.Y() - q[1]) +
                              (p.Z() - q[2]) * (p.Z() - q[2]);
            if (d2 <= m_tolerance * m_tolerance) {
              return index;
            }
          }
        }
      }
    }
    const auto index = static_cast<std::uint32_t>(m_vertices.size());
    m_vertices.push_back({p.X(), p.Y(), p.Z()});
    m_cells[cell].push_back(index);
    return index;
  }

private:
  struct Cell {
    std::int64_t x;
    std::int64_t y;
    std::int64_t z;
    bool operator==(const Cell& other) const { return x == other.x && y == other.y && z == other.z; }
  };
  struct CellHash {
    std::size_t operator()(const Cell& c) const {
      const auto mix = [](std::uint64_t h, std::int64_t v) {
        return (h ^ static_cast<std::uint64_t>(v)) * 0x100000001b3ull;
      };
      return static_cast<std::size_t>(mix(mix(mix(0xcbf29ce484222325ull, c.x), c.y), c.z));
    }
  };

  std::int64_t key(double v) const { return static_cast<std::int64_t>(std::floor(v / m_tolerance)); }

  double m_tolerance;
  std::vector<std::array<double, 3>>& m_vertices;
  std::unordered_map<Cell, std::vector<std::uint32_t>, CellHash> m_cells;
};

} // namespace

IndexedMesh indexed_mesh(const TopoDS_Shape& shape, const MeshOptions& options) {
  const TopoDS_Shape meshed = triangulate(shape, options);
  IndexedMesh mesh;
  VertexMerger merger(1e-6, mesh.vertices);
  std::vector<std::uint32_t> nodes;
  for (TopExp_Explorer it(meshed, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      throw Error("a face could not be triangulated");
    }
    const gp_Trsf placement = location.Transformation();
    nodes.resize(static_cast<std::size_t>(triangulation->NbNodes()));
    for (int i = 1; i <= triangulation->NbNodes(); ++i) {
      gp_Pnt p = triangulation->Node(i);
      if (!location.IsIdentity()) {
        p.Transform(placement);
      }
      nodes[static_cast<std::size_t>(i - 1)] = merger.add(p);
    }
    // A face's triangles follow its surface's normal: turned round where
    // the face is reversed, or mirrored by its placement.
    const bool turned = (face.Orientation() == TopAbs_REVERSED) != placement.IsNegative();
    for (int t = 1; t <= triangulation->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      triangulation->Triangle(t).Get(a, b, c);
      if (turned) {
        std::swap(b, c);
      }
      const std::array<std::uint32_t, 3> triangle{nodes[static_cast<std::size_t>(a - 1)],
                                                  nodes[static_cast<std::size_t>(b - 1)],
                                                  nodes[static_cast<std::size_t>(c - 1)]};
      if (triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0]) {
        continue; // collapsed by the merge
      }
      mesh.triangles.push_back(triangle);
    }
  }
  return mesh;
}

} // namespace mitcad::io
