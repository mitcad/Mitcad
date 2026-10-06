// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/profile_feature.rs.

#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/profile_feature.h.
struct ExtrudeFeatureInput;
struct RevolveInput;
struct HoleInput;
struct ThreadInput;
struct PlaneOutput;
struct AxisOutput;
struct CylinderOutput;

std::shared_ptr<geometry::Shape> extrude_feature(rust::Str feature, const Frame& frame,
                                                 rust::Slice<const Region> regions,
                                                 const ExtrudeFeatureInput& input,
                                                 const ShapeList& bodies);
std::shared_ptr<geometry::Shape> revolve(rust::Str feature, const Frame& frame,
                                         rust::Slice<const Region> regions, const RevolveInput& input,
                                         const ShapeList& bodies);
std::shared_ptr<geometry::Shape> hole_tool(rust::Str feature, const HoleInput& input,
                                           const ShapeList& bodies);
std::shared_ptr<geometry::Shape> modeled_thread(rust::Str feature, const geometry::Shape& body,
                                                const ThreadInput& input);
PlaneOutput face_plane(const geometry::Shape& shape, rust::Str face);
CylinderOutput face_cylinder(const geometry::Shape& shape, rust::Str face);

} // namespace mitcad::bridge
