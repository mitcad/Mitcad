// SPDX-License-Identifier: MIT
#pragma once

// Primitive solids (Box, Cylinder, Sphere and Torus) on a frame.

#include <string>

#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class PrimitiveKind { Box, Cylinder, Sphere, Torus };

// Sizes in millimetres, all greater than zero:
//   Box       a = length (frame x), b = width (frame y), c = height (normal),
//             the first corner at the frame origin;
//   Cylinder  a = radius, c = height, the base centre at the origin;
//   Sphere    a = radius, the centre at the origin;
//   Torus     a = ring radius, b = section radius (b < a), the centre at the
//             origin and the axis along the normal.
struct PrimitiveSpec {
  std::string feature;
  Frame frame;
  PrimitiveKind kind = PrimitiveKind::Box;
  double a = 0.0;
  double b = 0.0;
  double c = 0.0;
};

// Faces "<feature>:bottom" (on the frame's plane) and "top", the box's
// sides "side0".."side3" counter-clockwise from the one at the corner's y
// (side1 at x + length), and the curved face of the others "side0".
ShapePtr primitive(const PrimitiveSpec& spec);

} // namespace mitcad::geometry
