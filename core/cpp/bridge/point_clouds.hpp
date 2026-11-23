// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/point_clouds.rs (mitcad#2240): the exact
// distance of points from a body's faces and the nearest points of them,
// for Point Cloud Deviation.

#include <string>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Per point (x, y, z triples) its distance from the faces named (every
// face of the shape when none is named) and the nearest point of them:
// four values each, the distance then x, y, z. A name that finds no face
// is an error.
rust::Vec<double> boundary_nearest(const geometry::Shape& shape, rust::Slice<const rust::String> faces,
                                   rust::Slice<const double> points);

} // namespace mitcad::bridge
