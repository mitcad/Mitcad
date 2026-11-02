// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/sketch_ref.rs.

#include <array>
#include <cstddef>
#include <cstdint>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/sketch_ref.h.
struct ModelCurve;
struct IndexedElement;
struct SectionPiece;
struct NamedBoundaryCurve;

rust::Vec<ModelCurve> model_curves(const geometry::Shape& shape, rust::Str name);

rust::Vec<NamedBoundaryCurve> face_boundary(const geometry::Shape& shape, rust::Str face);

// FreeCAD import: a face, edge or vertex by its index ('F', 'E', 'V').
IndexedElement indexed_element(const geometry::Shape& shape, std::uint8_t kind, std::size_t index);

// Section curves of sketches (mitcad#389).
rust::Vec<SectionPiece> section_curves(const geometry::Shape& shape, rust::Str name, std::array<double, 3> origin,
                                       std::array<double, 3> normal);

// Crossing points of a sketch's Intersect (mitcad#736): x0, y0, z0, x1, ...
rust::Vec<double> section_points(const geometry::Shape& shape, rust::Str name, std::array<double, 3> origin,
                                 std::array<double, 3> normal);

} // namespace mitcad::bridge
