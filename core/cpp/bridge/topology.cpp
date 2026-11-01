// SPDX-License-Identifier: MIT
#include "bridge/topology.hpp"

#include <cstddef>

#include "mitcad_bridge/kernel/topology.h"

namespace mitcad::bridge {

TopologyCounts topology_counts(const geometry::Shape& shape) {
  TopologyCounts counts{};
  counts.faces = static_cast<std::size_t>(shape.face_count());
  counts.edges = static_cast<std::size_t>(shape.edge_count());
  counts.vertices = static_cast<std::size_t>(shape.vertex_count());
  return counts;
}

} // namespace mitcad::bridge
