// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/face_outline.rs (mitcad#1265).

#include <array>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/face_outline.h.
struct OutlineCurve;

// The faces a face name resolves to seen along `direction`
// (geometry::face_outline).
rust::Vec<OutlineCurve> face_outline(const geometry::Shape& shape, rust::Str face, std::array<double, 3> direction);

} // namespace mitcad::bridge
