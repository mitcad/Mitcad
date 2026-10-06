// SPDX-License-Identifier: MIT
#include "bridge/mesh.hpp"

#include <cstdint>

#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/kernel/mesh.h"

namespace mitcad::bridge {

TriangleMesh triangle_mesh(const geometry::Shape& shape, double deviation, double angle) {
  io::MeshOptions options;
  options.linear_deflection = deviation;
  options.angular_deflection = angle;
  const io::IndexedMesh mesh = io::indexed_mesh(shape.occt(), options);
  TriangleMesh out;
  out.vertices.reserve(mesh.vertices.size() * 3);
  for (const auto& vertex : mesh.vertices) {
    for (const double c : vertex) {
      out.vertices.push_back(c);
    }
  }
  out.triangles.reserve(mesh.triangles.size() * 3);
  for (const auto& triangle : mesh.triangles) {
    for (const std::uint32_t index : triangle) {
      out.triangles.push_back(index);
    }
  }
  return out;
}

} // namespace mitcad::bridge
