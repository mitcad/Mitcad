// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/print_gaps.rs.

#include "bridge/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/print_gaps.h.
struct PrintFaceGap;

// Every pair of a face of `a` and a face of `b` less than `within` mm apart
// (mitcad#1672): the faces by index in the shapes' face order, the exact
// minimum distance, the closest points and the faces' outward unit normals
// there (zero where a face has none). Only faces whose boxes lie within
// `within` of each other are measured.
rust::Vec<PrintFaceGap> face_separations(const geometry::Shape& a, const geometry::Shape& b, double within);

} // namespace mitcad::bridge
