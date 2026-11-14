// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/intersection_curve.rs (mitcad#1405; the
// curve tools' helices and sections of shapes, mitcad#1777).

#include <array>
#include <cstddef>

#include "bridge/shape.hpp"
#include "mitcad_bridge/kernel/sweeps.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/intersection_curve.h.
struct CrossingCurve;
struct HelixCurveInput;

// Where the chain `first` swept along `first_direction` crosses the chain
// `second` swept along `second_direction` (geometry::intersection_curves).
rust::Vec<CrossingCurve> intersection_curves(rust::Slice<const SweepCurve> first,
                                             std::array<double, 3> first_direction,
                                             rust::Slice<const SweepCurve> second,
                                             std::array<double, 3> second_direction);

// A growing helix (geometry::helix_curve).
rust::Vec<CrossingCurve> helix_curve(const HelixCurveInput& input);

// Where the shapes of `first` meet those of `second` (geometry::shape_section):
// shape i of a side with the next counts[i] of its face names (none: the
// whole shape).
rust::Vec<CrossingCurve> shape_section(const ShapeList& first, rust::Slice<const rust::String> first_faces,
                                       rust::Slice<const std::size_t> first_counts, const ShapeList& second,
                                       rust::Slice<const rust::String> second_faces,
                                       rust::Slice<const std::size_t> second_counts);

} // namespace mitcad::bridge
