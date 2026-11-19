// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/profile_feature.rs.

#include <memory>
#include <optional>

#include "bridge/shape.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/target.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/profile_feature.h.
struct TargetInput;
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

// The geometry library's target of an input whose body indexes `bodies`
// (Rust's `profile_feature::bound`); none for an unset one. Shared with the
// extrusions of body faces (mitcad#2033).
std::optional<geometry::Target> target_of(const TargetInput& input, const ShapeList& bodies);

// The geometry library's spec of an extrusion (thin extrusions along
// curves add their chains, mitcad#463).
geometry::ExtrudeFeatureSpec extrude_spec(rust::Str feature, const Frame& frame,
                                          rust::Slice<const Region> regions,
                                          const ExtrudeFeatureInput& input, const ShapeList& bodies);

} // namespace mitcad::bridge
