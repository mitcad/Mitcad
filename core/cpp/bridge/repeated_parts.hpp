// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/repeated_parts.rs.

#include "bridge/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/repeated_parts.h.
struct RigidCopy;

// geometry::rigid_copies of the bodies (mitcad#1277), one entry per body.
rust::Vec<RigidCopy> rigid_copies(const ShapeList& bodies, double tolerance);

} // namespace mitcad::bridge
