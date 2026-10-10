// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/mesh.rs: triangle meshes of bodies for 3D
// printing (3MF export, mitcad#13).

#include <cstdint>
#include <memory>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/mesh.h.
struct TriangleMesh;

TriangleMesh triangle_mesh(const geometry::Shape& shape, double deviation, double angle);

std::shared_ptr<geometry::Shape> mesh_from_triangles(rust::Slice<const double> vertices,
                                                     rust::Slice<const std::uint32_t> triangles);

} // namespace mitcad::bridge
