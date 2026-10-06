// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/profile.rs: profile regions from Rust
// converted to the geometry library's types.

#include <memory>
#include <vector>

#include "mitcad/geometry/profile.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/profile.h.
struct Frame;
struct Region;

geometry::Frame to_geometry(const Frame& frame);
std::vector<geometry::Region> to_geometry(rust::Slice<const Region> regions);

std::shared_ptr<geometry::Shape> profile_shape(const Frame& frame, rust::Slice<const Region> regions);

} // namespace mitcad::bridge
