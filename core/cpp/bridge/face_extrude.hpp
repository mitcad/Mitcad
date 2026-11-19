// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/face_extrude.rs (mitcad#535).

#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile_feature.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/face_extrude.h.
struct FaceExtrusion;

// geometry::extrude_face; each side up to its target when it has one
// (mitcad#2033), the targets' bodies indexing `bodies`.
std::shared_ptr<geometry::Shape> extrude_face(rust::Str feature, const geometry::Shape& body, rust::Str face,
                                              rust::Str key, const FaceExtrusion& extrusion,
                                              const TargetInput& target1, const TargetInput& target2,
                                              const ShapeList& bodies);

} // namespace mitcad::bridge
