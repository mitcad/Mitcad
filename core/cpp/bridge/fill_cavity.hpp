// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/fill_cavity.rs.

#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/fill_cavity.h.
struct Cavity;

// geometry::cavity_pieces of `body` (mitcad#1818).
std::unique_ptr<ShapeList> cavity_pieces(const geometry::Shape& body, const Cavity& spec);

} // namespace mitcad::bridge
