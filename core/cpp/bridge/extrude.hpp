// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/extrude.rs. The profile types are the
// ones the profile bridge generates.

#include <memory>

#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/extrude.h.
struct Sweep;

std::shared_ptr<geometry::Shape> extrude(rust::Str feature, const Frame& frame,
                                         rust::Slice<const Region> regions, const Sweep& sweep);

} // namespace mitcad::bridge
