// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/sketch_ref.rs.

#include <cstddef>
#include <cstdint>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/sketch_ref.h.
struct ModelCurve;
struct IndexedElement;

rust::Vec<ModelCurve> model_curves(const geometry::Shape& shape, rust::Str name);

// FreeCAD import: a face, edge or vertex by its index ('F', 'E', 'V').
IndexedElement indexed_element(const geometry::Shape& shape, std::uint8_t kind, std::size_t index);

} // namespace mitcad::bridge
