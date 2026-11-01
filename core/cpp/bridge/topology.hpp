// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/topology.rs (mitcad#1095).

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/topology.h.
struct TopologyCounts;

// The shape's distinct faces, edges and vertices (geometry::Shape's maps).
TopologyCounts topology_counts(const geometry::Shape& shape);

} // namespace mitcad::bridge
