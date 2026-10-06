// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/mesh.rs: triangle meshes of bodies for 3D
// printing (3MF export, mitcad#13).

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/mesh.h.
struct TriangleMesh;

TriangleMesh triangle_mesh(const geometry::Shape& shape, double deviation, double angle);

} // namespace mitcad::bridge
