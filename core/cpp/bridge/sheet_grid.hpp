// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/sheet_grid.rs (mitcad#2442): a face
// sampled over its surface's parameters, for an approximate development by
// triangulation.

#include <cstddef>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

struct SheetGrid;

// geometry::sheet_face_grid as x, y, z triples, a grid per piece of the
// face, with its boundary when edge_steps is above 0 (mitcad#2558).
rust::Vec<SheetGrid> sheet_face_grid(const geometry::Shape& shape, rust::Str face, std::size_t columns,
                                     std::size_t rows, std::size_t edge_steps);

} // namespace mitcad::bridge
