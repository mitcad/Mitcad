// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/intersection_curve.rs (mitcad#1405).

#include <array>

#include "mitcad_bridge/kernel/sweeps.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/intersection_curve.h.
struct CrossingCurve;

// Where the chain `first` swept along `first_direction` crosses the chain
// `second` swept along `second_direction` (geometry::intersection_curves).
rust::Vec<CrossingCurve> intersection_curves(rust::Slice<const SweepCurve> first,
                                             std::array<double, 3> first_direction,
                                             rust::Slice<const SweepCurve> second,
                                             std::array<double, 3> second_direction);

} // namespace mitcad::bridge
