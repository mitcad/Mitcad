// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/form.rs (mitcad#1757).

#include <memory>

#include "bridge/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/form.h.
struct FormPatch;

// The patches as faces sewn within `tolerance`, with `closed` one solid
// (geometry::patch_body).
std::shared_ptr<geometry::Shape> patch_body(rust::Slice<const FormPatch> patches, bool closed, double tolerance);

} // namespace mitcad::bridge
