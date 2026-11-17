// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/flex_drape.rs (mitcad#1713): the chain of
// a Flex item let hang and the nodes that keep its shape
// (mitcad/geometry/flex_drape.hpp).

#include "bridge/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/flex_drape.h.
struct DrapeStepInput;
struct DrapeStepData;
struct DrapeNodesInput;
struct DrapeNodesData;

// One step of the chain of `input` among `obstacles`
// (geometry::flex_drape_step).
DrapeStepData flex_drape_step(const ShapeList& obstacles, const DrapeStepInput& input);

// The nodes of the resting chain of `input` (geometry::flex_drape_nodes).
DrapeNodesData flex_drape_nodes(const ShapeList& obstacles, const DrapeNodesInput& input);

} // namespace mitcad::bridge
