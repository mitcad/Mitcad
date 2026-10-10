// SPDX-License-Identifier: MIT
#include "bridge/mesh.hpp"

#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>

#include <Poly_Triangle.hxx>
#include <Poly_Triangulation.hxx>
#include <gp_Pnt.hxx>

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

std::shared_ptr<geometry::Shape> mesh_from_triangles(rust::Slice<const double> vertices,
                                                     rust::Slice<const std::uint32_t> triangles) {
  const std::size_t nodes = vertices.size() / 3;
  const std::size_t count = triangles.size() / 3;
  if (vertices.size() % 3 != 0 || triangles.size() % 3 != 0) {
    throw std::invalid_argument("vertices and triangles come in threes");
  }
  if (nodes == 0 || count == 0) {
    throw std::invalid_argument("a mesh without triangles");
  }
  if (nodes > static_cast<std::size_t>(std::numeric_limits<int>::max()) ||
      count > static_cast<std::size_t>(std::numeric_limits<int>::max())) {
    throw std::invalid_argument("a mesh too large");
  }
  const occ::handle<Poly_Triangulation> mesh =
      new Poly_Triangulation(static_cast<int>(nodes), static_cast<int>(count), false);
  for (std::size_t i = 0; i < nodes; ++i) {
    mesh->SetNode(static_cast<int>(i) + 1,
                  gp_Pnt(vertices[3 * i], vertices[3 * i + 1], vertices[3 * i + 2]));
  }
  for (std::size_t t = 0; t < count; ++t) {
    int corner[3] = {0, 0, 0};
    for (std::size_t k = 0; k < 3; ++k) {
      const std::uint32_t index = triangles[3 * t + k];
      if (index >= nodes) {
        throw std::invalid_argument("vertex index " + std::to_string(index) + " of " +
                                    std::to_string(nodes) + " vertices");
      }
      corner[k] = static_cast<int>(index) + 1;
    }
    mesh->SetTriangle(static_cast<int>(t) + 1, Poly_Triangle(corner[0], corner[1], corner[2]));
  }
  return std::make_shared<geometry::Shape>(io::mesh_body(mesh));
}

} // namespace mitcad::bridge
