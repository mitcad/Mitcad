// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/step_tree.rs (mitcad#1915).

#include <cstdint>

#include "bridge/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated
// mitcad_bridge/kernel/step_tree.h.
struct TreeBody;
struct TreeDefinition;
enum class TreeUnit : std::uint8_t;

// Writes a design's component tree to a STEP file (io::write_step_tree);
// each body's shape is at its index of `shapes`.
void write_step_tree(rust::Str path, const ShapeList& shapes, rust::Slice<const TreeBody> bodies,
                     rust::Slice<const TreeDefinition> definitions, bool ap242, TreeUnit unit);

} // namespace mitcad::bridge
