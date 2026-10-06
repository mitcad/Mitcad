// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/dressup.rs.

#include <cstdint>
#include <memory>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/dressup.h.
struct FilletSet;
struct ChamferSet;
enum class ChamferCornerKind : std::uint8_t;

std::shared_ptr<geometry::Shape> fillet(rust::Str feature, const geometry::Shape& body,
                                        rust::Slice<const FilletSet> sets,
                                        bool rolling_ball_corners);
std::shared_ptr<geometry::Shape> chamfer(rust::Str feature, const geometry::Shape& body,
                                         rust::Slice<const ChamferSet> sets, ChamferCornerKind corner);

rust::Vec<rust::String> shape_notes(const geometry::Shape& shape);

} // namespace mitcad::bridge
