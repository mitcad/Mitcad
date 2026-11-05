// SPDX-License-Identifier: MIT
#include "bridge/repeated_parts.hpp"

#include <cstddef>
#include <memory>
#include <vector>

#include "mitcad/geometry/repeated_parts.hpp"
#include "mitcad_bridge/kernel/repeated_parts.h"

namespace mitcad::bridge {

rust::Vec<RigidCopy> rigid_copies(const ShapeList& bodies, double tolerance) {
  std::vector<const geometry::Shape*> shapes;
  shapes.reserve(bodies.size());
  for (const std::shared_ptr<geometry::Shape>& body : bodies.items()) {
    shapes.push_back(body.get());
  }
  rust::Vec<RigidCopy> out;
  for (const geometry::RigidCopy& copy : geometry::rigid_copies(shapes, tolerance)) {
    RigidCopy item{};
    item.of = copy.of;
    for (std::size_t r = 0; r < 3; ++r) {
      for (std::size_t c = 0; c < 3; ++c) {
        item.linear[r * 3 + c] = copy.placement.linear[r][c];
      }
      item.translation[r] = copy.placement.translation[r];
    }
    out.push_back(item);
  }
  return out;
}

} // namespace mitcad::bridge
