// SPDX-License-Identifier: MIT
#include "bridge/surface_groups.hpp"

#include <cstdint>
#include <vector>

#include "mitcad/geometry/smooth_seams.hpp"

namespace mitcad::bridge {

rust::Vec<std::uint32_t> face_surface_groups(const geometry::Shape& shape) {
  rust::Vec<std::uint32_t> result;
  for (const std::uint32_t group : geometry::smooth_surface_groups(shape)) {
    result.push_back(group);
  }
  return result;
}

} // namespace mitcad::bridge
