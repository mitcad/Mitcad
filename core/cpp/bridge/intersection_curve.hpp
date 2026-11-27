// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/intersection_curve.rs (mitcad#1405; the
// curve tools' helices and sections of shapes, mitcad#1777; isoparametric
// curves, mitcad#2522).

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

// The isoparametric curves of a face (mitcad#2522, geometry::iso_curves):
// holding u (`hold_u`) or v, at each of `fractions`, or through the face's
// point nearest `through` (three values; none: the fractions). The pieces
// of all curves in order; `curves` gets the index of each piece's curve.
rust::Vec<CrossingCurve> iso_curves(const geometry::Shape& shape, rust::Str face, bool hold_u,
                                    rust::Slice<const double> fractions, rust::Slice<const double> through,
                                    rust::Vec<std::size_t>& curves);

} // namespace mitcad::bridge
