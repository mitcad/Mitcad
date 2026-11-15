// SPDX-License-Identifier: MIT
#pragma once

// Fill Cavity (mitcad#1818): the empty spaces of a body below a cap plane,
// for the model to choose which to fill (core/model/src/features/
// fill_cavity.rs). Lengths are millimetres.

#include <array>
#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// A box in the cap's frame: the points origin + u x_axis + v y_axis +
// w normal with min[i] <= (u, v, w)[i] <= max[i], where y_axis is
// normal x x_axis (x_axis and normal unit vectors at right angles). Its top
// is at w = max[2], its bottom at w = min[2].
struct CavitySpec {
  std::string feature;
  // The name of the box's top face (the face on the cap).
  std::string top;
  std::array<double, 3> origin{0.0, 0.0, 0.0};
  std::array<double, 3> x_axis{1.0, 0.0, 0.0};
  std::array<double, 3> normal{0.0, 0.0, 1.0};
  std::array<double, 3> min{0.0, 0.0, 0.0};
  std::array<double, 3> max{0.0, 0.0, 0.0};
};

// The box less the body: one shape per solid, in the boolean's geometric
// order. A piece's faces keep the names of the body faces they are pieces
// of; the box's faces are named `top` (the top), <feature>:fill_side(k)
// (k 0 to 3, the sides) and <feature>:fill_bottom, pieces of one face
// numbered "#k". A box the body does not reach is one piece, the box whole;
// a box the body fills gives none. Throws std::invalid_argument for a box
// without volume or a frame whose axes are not at right angles. The body is
// not modified.
std::vector<ShapePtr> cavity_pieces(const Shape& body, const CavitySpec& spec);

} // namespace mitcad::geometry
