// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/emboss.rs. The profile types are the
// ones the profile bridge generates.

#include <memory>

#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/emboss.h.
struct EmbossWrap;
struct ConeOutput;
struct EmbossProjection;

// geometry::wrapped_emboss.
std::shared_ptr<geometry::Shape> wrapped_emboss(rust::Str feature, const Frame& frame,
                                                rust::Slice<const Region> regions, const EmbossWrap& wrap);

// geometry::face_cone (mitcad#473).
ConeOutput face_cone(const geometry::Shape& shape, rust::Str face);

// geometry::projected_emboss (mitcad#1091).
std::shared_ptr<geometry::Shape> projected_emboss(rust::Str feature, const Frame& frame,
                                                  rust::Slice<const Region> regions, const geometry::Shape& body,
                                                  rust::Str face, const EmbossProjection& projection);

} // namespace mitcad::bridge
