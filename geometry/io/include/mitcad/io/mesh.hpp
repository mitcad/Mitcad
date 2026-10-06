// SPDX-License-Identifier: MIT
#pragma once

// Triangulation of B-rep bodies for mesh export and mesh bodies for display.

#include <array>
#include <cstddef>
#include <cstdint>
#include <vector>

#include <Poly_Triangulation.hxx>
#include <Standard_Handle.hxx>
#include <TopoDS_Shape.hxx>

namespace mitcad::io {

// Mesh refinement presets for export.
enum class Refinement {
  Low,    // 0.1 mm deviation, 30 degrees between normals
  Medium, // 0.03 mm, 15 degrees
  High,   // 0.01 mm, 8 degrees
};

// Tolerances of a triangulation.
struct MeshOptions {
  // Largest distance between the mesh and the surface (surface deviation),
  // in mm; with `relative`, a fraction of each edge's size instead.
  double linear_deflection = 0.03;
  // Largest angle between the normals of neighbouring facets, in radians.
  double angular_deflection = 0.2617993877991494; // 15 degrees
  bool relative = false;

  static MeshOptions from(Refinement refinement);
};

// A triangulated copy of the shape; the shape itself is not modified (OCCT
// keeps triangulations in the faces, so meshing the original would change
// what other users of the shape see). A mesh body, or any shape with a face
// that has no surface, is returned as it is.
TopoDS_Shape triangulate(const TopoDS_Shape& shape, const MeshOptions& options);

// Triangle and node count over the faces' triangulations.
struct MeshStats {
  std::size_t faces = 0;
  std::size_t triangles = 0;
  std::size_t nodes = 0;
};
MeshStats mesh_stats(const TopoDS_Shape& shape);

// A mesh body: one face that carries the triangulation and no surface.
// OCCT's viewer displays it like any other face.
TopoDS_Shape mesh_body(const occ::handle<Poly_Triangulation>& triangulation);

// True when the shape has faces and none of them has a surface.
bool is_mesh(const TopoDS_Shape& shape);

// A shape as one triangle mesh for 3D printing (3MF, mitcad#13): the
// vertices of all faces with those that coincide (within 1e-6 mm) merged,
// so the triangles of neighbouring faces share their vertices along the
// edges, and a closed solid gives a closed mesh; triangles that collapse
// in the merge (slivers at degenerate edges) are left out. Triangles are
// counter-clockwise seen from outside (a reversed face's are turned
// round). Millimetres, in the shape's own placement.
struct IndexedMesh {
  std::vector<std::array<double, 3>> vertices;
  std::vector<std::array<std::uint32_t, 3>> triangles;
};

// The shape triangulated with `options` on a copy (see triangulate); a
// mesh body's own triangles. Throws Error when a face has no triangulation.
IndexedMesh indexed_mesh(const TopoDS_Shape& shape, const MeshOptions& options);

} // namespace mitcad::io
