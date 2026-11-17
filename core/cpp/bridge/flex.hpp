// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/flex.rs (mitcad#1713): the routes of
// Flex items, the solids of their sections that are not round and the end
// ports of pipes and Flex items (mitcad/geometry/flex.hpp).

#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "mitcad_bridge/kernel/sweeps.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/flex.h.
struct FlexRouteInput;
struct FlexRouteData;
struct FlexSolidInput;
struct FlexPortData;

// The route of the Flex item `feature` through the nodes of `input`,
// around `obstacles` (geometry::flex_route).
FlexRouteData flex_route(rust::Str feature, const ShapeList& obstacles, const FlexRouteInput& input);

// The solid of a rectangle, or of the one region of `outline`, swept along
// `path` and turned as the samples of `input` say (geometry::flex_solid).
std::shared_ptr<geometry::Shape> flex_solid(rust::Str feature, rust::Slice<const SweepCurve> path,
                                            rust::Slice<const Region> outline, const FlexSolidInput& input);

// The end port at the cap face `cap` of `shape` (geometry::end_port).
FlexPortData flex_end_port(const geometry::Shape& shape, rust::Str cap);

} // namespace mitcad::bridge
