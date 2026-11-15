// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/mesh.rs: triangle meshes of bodies for 3D
// printing (3MF export, mitcad#13).

#include <cstdint>
#include <memory>
#include <string>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/mesh.h.
struct TriangleMesh;

TriangleMesh triangle_mesh(const geometry::Shape& shape, double deviation, double angle,
                           std::uint64_t face_triangles);

// The message of a face of `shape` that a meshing left without a
// triangulation (io::FaceError, mitcad#1568), the face named by its
// topological names when it has any, else by its index; with its surface's
// kind: "a face could not be triangulated: F4:top (plane)". That is the
// agents' detail (mitcad#1612): a person reads "a face could not be
// triangulated (its surface: plane)", the face painted (placed_error).
std::string untriangulated_face_message(const geometry::Shape& shape, int index, const std::string& surface);

std::shared_ptr<geometry::Shape> mesh_from_triangles(rust::Slice<const double> vertices,
                                                     rust::Slice<const std::uint32_t> triangles);

// As mesh_from_triangles, each vertex with its texture coordinates (`uvs`:
// u, v per vertex; mitcad#1862) as the triangulation's UV nodes. A mesh
// body's face has no surface, so its UV nodes are no surface parameters:
// they say where the texture's image lies on it.
std::shared_ptr<geometry::Shape> mesh_from_textured_triangles(rust::Slice<const double> vertices,
                                                              rust::Slice<const std::uint32_t> triangles,
                                                              rust::Slice<const double> uvs);

} // namespace mitcad::bridge
