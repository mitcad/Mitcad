// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/edge_continuity.rs (mitcad#2521): the
// continuity across a body's edges for Continuity Analysis.

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/edge_continuity.h.
struct ContinuityParams;
struct EdgeContinuityInfo;

// geometry::edge_continuity of the edges named and every edge of the faces
// named; every edge of the shape when neither names any. A name that finds
// no edge or face of the shape is an error.
rust::Vec<EdgeContinuityInfo> edge_continuity(const geometry::Shape& shape, rust::Slice<const rust::String> edges,
                                              rust::Slice<const rust::String> faces, const ContinuityParams& params);

} // namespace mitcad::bridge
