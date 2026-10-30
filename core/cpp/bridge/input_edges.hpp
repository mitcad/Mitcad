// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/input_edges.rs: a Fillet's or Chamfer's
// picked edges as polylines, kept with its result (mitcad#957).

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/input_edges.h.
struct EdgePolyline;

// geometry::edge_polylines with the names as given.
rust::Vec<EdgePolyline> edge_polylines(const geometry::Shape& shape, rust::Slice<const rust::String> edges,
                                       rust::Slice<const rust::String> faces, double deflection, double angle);

} // namespace mitcad::bridge
