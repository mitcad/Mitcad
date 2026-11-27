// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/min_radius.rs (mitcad#2518): each face's
// largest principal curvature and where it is, for Minimum Radius.

#include "bridge/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/min_radius.h.
struct MinRadiusFace;

// Each face's largest principal curvature (its magnitude, 1/mm; 0 for a
// face that does not bend) and the point of the face where it is: every
// face of the shape when `faces` is empty, else every piece of each name,
// each face once, by its index in the shape's face order. Evaluated on the
// exact surface at a grid of points inside the face (its loops' inside),
// refined about the largest; a plane, cylinder or sphere exactly. A name
// that finds no face is an error.
rust::Vec<MinRadiusFace> face_curvatures(const geometry::Shape& shape, rust::Slice<const rust::String> faces);

} // namespace mitcad::bridge
