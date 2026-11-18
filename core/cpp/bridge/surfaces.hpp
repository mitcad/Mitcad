// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/surfaces.rs. The profile and curve
// types are the ones the profile and sweep bridges generate.

#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "mitcad_bridge/kernel/sweeps.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/surfaces.h.
struct OpenBoundaryData;
struct PatchLoopData;

// geometry::patch_regions.
std::unique_ptr<ShapeList> patch_regions(rust::Str feature, const Frame& frame,
                                         rust::Slice<const Region> regions);
// geometry::open_boundaries.
rust::Vec<OpenBoundaryData> open_boundaries(const geometry::Shape& body);
// geometry::patch_loop; `curves` are the sketch curves in the loop
// (mitcad#1972).
std::shared_ptr<geometry::Shape> patch_loop(rust::Str feature, const geometry::Shape& body,
                                            const PatchLoopData& spec, rust::Slice<const SweepCurve> curves);
// geometry::stitch of the shapes in the list.
std::shared_ptr<geometry::Shape> stitch(const ShapeList& shapes, double tolerance);
// geometry::thicken.
std::shared_ptr<geometry::Shape> thicken(rust::Str feature, const geometry::Shape& sheet,
                                         double thickness, bool symmetric);

} // namespace mitcad::bridge
