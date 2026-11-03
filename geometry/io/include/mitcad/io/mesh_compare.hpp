// SPDX-License-Identifier: MIT
#pragma once

// Two display meshings of the same bodies compared face by face
// (mitcad#644): how each face's triangulation meets the face (its
// deviation from the surface, its area against the face's own) and how
// long each face took, for the trial of other mesh options on real
// designs (mitcad-cli mesh-compare).

#include <cstddef>
#include <string>
#include <vector>

#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>

#include "mitcad/io/mesh.hpp"

namespace mitcad::io {

// How a face's triangulation meets the face.
struct FaceMeshQuality {
  bool triangulated = false; // false: no triangulation (the rest is 0)
  std::size_t triangles = 0;
  std::size_t nodes = 0;
  // The largest distance (mm) between a triangle's centroid or an edge's
  // midpoint and the surface's point at the same parameters (the
  // triangulation's parameter nodes): the chordal deviation a triangle
  // shows; 0 for a triangulation without parameters.
  double deviation = 0.0;
  // The deviation OCCT's mesher recorded in the triangulation.
  double recorded = 0.0;
  double area = 0.0; // the triangles' areas (mm²)
  // Triangles turned over against the others in the parameter plane (a
  // fold: triangles over each other instead of side by side).
  std::size_t folded = 0;
};

FaceMeshQuality face_mesh_quality(const TopoDS_Face& face);

// One face meshed two ways.
struct FaceMeshComparison {
  std::size_t body = 0; // index in the shapes given
  std::size_t face = 0; // index in the body's faces (TopExp::MapShapes order, from 0)
  std::string surface;  // "plane", "cylinder", ... (MeshReport's names)
  // The deviation the options ask of this face (mm): the linear deflection
  // raised to the face's floor (face_deviation_floor), `a`'s options.
  double tolerance = 0.0;
  double area = 0.0; // the face's true area (analysis::surface_properties, mm²)
  FaceMeshQuality a;
  FaceMeshQuality b;
  // The face's triangulation time (ms) in each meshing; -1 when the mesher
  // did not triangulate it (a face it failed on before that step).
  double ms_a = -1.0;
  double ms_b = -1.0;
};

struct MeshComparison {
  std::vector<FaceMeshComparison> faces; // body by body, face by face
  std::size_t surfaceless = 0;           // faces without a surface (mesh bodies), not compared
  // What each meshing did (MeshReport::total: its time), its faces let go
  // (every_face_time empty, `slowest` without faces).
  MeshReport a;
  MeshReport b;
};

// Meshes copies of the shapes (the shapes keep their own triangulations, or
// none) with `a` and with `b`, each meshing all of them together as
// mesh_all_in_place does (the view's display meshes, in parallel on every
// core), `b` first when `b_first`, and compares their faces; each
// meshing's copies are measured and let go before the other meshing. A shape whose
// faces have no surface is not meshed (counted in `surfaceless`).
MeshComparison compare_meshing(const std::vector<TopoDS_Shape>& shapes, const MeshOptions& a, const MeshOptions& b,
                               bool b_first = false);

} // namespace mitcad::io
