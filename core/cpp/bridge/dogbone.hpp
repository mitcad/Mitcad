// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/dogbone.rs.

#include <memory>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/dogbone.h.
struct Dogbone;

// geometry::dogbone_tool on `body` (mitcad#1011).
std::shared_ptr<geometry::Shape> dogbone_tool(const geometry::Shape& body, const Dogbone& spec);

} // namespace mitcad::bridge
