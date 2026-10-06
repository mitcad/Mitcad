// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/sweeps.rs: sweeps, lofts, pipes, coils,
// ribs and webs (F3).

#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/sweeps.h.
struct SweepCurve;
struct SweepInput;
struct LoftInput;
struct PipeInput;
struct CoilInput;
struct RibInput;
struct HelixInput;

std::shared_ptr<geometry::Shape> sweep_solid(rust::Str feature, const Frame& frame,
                                             rust::Slice<const Region> regions, const SweepInput& input);
std::shared_ptr<geometry::Shape> loft_solid(rust::Str feature, const LoftInput& input,
                                            rust::Slice<const Frame> frames,
                                            rust::Slice<const Region> regions, const ShapeList& bodies);
std::shared_ptr<geometry::Shape> pipe_solid(rust::Str feature, const PipeInput& input);
std::shared_ptr<geometry::Shape> coil_solid(rust::Str feature, const Frame& frame, const CoilInput& input);
std::shared_ptr<geometry::Shape> rib_solid(rust::Str feature, const Frame& frame, const RibInput& input,
                                           const ShapeList& bodies);
// FreeCAD's helices (mitcad#4).
std::shared_ptr<geometry::Shape> helix_solid(rust::Str feature, const Frame& frame,
                                             rust::Slice<const Region> regions, const HelixInput& input);

} // namespace mitcad::bridge
