// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/surface_groups.rs (mitcad#2148): each
// face's surface number for model_render's edges.

#include <cstdint>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// A surface number for each face, by face index
// (geometry::smooth_surface_groups).
rust::Vec<std::uint32_t> face_surface_groups(const geometry::Shape& shape);

} // namespace mitcad::bridge
