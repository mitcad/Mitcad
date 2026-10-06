// SPDX-License-Identifier: MIT
#pragma once

// Moved, mirrored and scaled copies of shapes, for moves, patterns,
// mirrors, alignments and scales. (The geometry that names resolve to is
// datum.hpp's.)

#include <array>
#include <string>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// An affine map p' = linear * p + translation; `linear` is row-major.
struct Affine {
  std::array<std::array<double, 3>, 3> linear{{{1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  std::array<double, 3> translation{0, 0, 0};
};

// A copy of the shape mapped by `map`. Rigid motions, mirrors and uniform
// scales keep the surface types; a non-uniform scale turns the faces into
// B-splines (planes stay flat). Face names stay; with `rename` (e.g.
// "F9:inst2") every face name n becomes "<rename>(n)". Throws for a
// degenerate map.
ShapePtr transform_shape(const Shape& shape, const Affine& map, const std::string& rename = {});

} // namespace mitcad::geometry
