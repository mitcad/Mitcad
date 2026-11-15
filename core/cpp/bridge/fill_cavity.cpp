// SPDX-License-Identifier: MIT
#include "bridge/fill_cavity.hpp"

#include <cstddef>
#include <string>

#include "mitcad/geometry/fill_cavity.hpp"
#include "mitcad_bridge/kernel/fill_cavity.h"

namespace mitcad::bridge {

std::unique_ptr<ShapeList> cavity_pieces(const geometry::Shape& body, const Cavity& spec) {
  geometry::CavitySpec cavity;
  cavity.feature = std::string(spec.feature);
  cavity.top = std::string(spec.top);
  for (std::size_t k = 0; k < 3; ++k) {
    cavity.origin[k] = spec.origin[k];
    cavity.x_axis[k] = spec.x_axis[k];
    cavity.normal[k] = spec.normal[k];
    cavity.min[k] = spec.min[k];
    cavity.max[k] = spec.max[k];
  }
  auto pieces = std::make_unique<ShapeList>();
  for (const geometry::ShapePtr& piece : geometry::cavity_pieces(body, cavity)) {
    pieces->push(piece);
  }
  return pieces;
}

} // namespace mitcad::bridge
