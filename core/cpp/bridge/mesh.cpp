// SPDX-License-Identifier: MIT
#include "bridge/mesh.hpp"

#include <cmath>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

#include <Poly_Triangle.hxx>
#include <Poly_Triangulation.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/user_error.hpp"
#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/kernel/mesh.h"

namespace mitcad::bridge {

std::string untriangulated_face_message(const geometry::Shape& shape, int index, const std::string& surface) {
  std::string names;
  std::vector<geometry::ErrorPlace> places;
  if (index >= 0 && index < shape.face_count()) {
    for (const std::string& name : shape.face_names(index)) {
      names += (names.empty() ? "" : ", ") + name;
      places.push_back(geometry::face_place(name));
    }
  }
  // A person reads the face's kind, the face painted; agents read its
  // names (mitcad#1612).
  return geometry::placed_error("a face could not be triangulated (its surface: " + surface + ")",
                                io::face_error_message(names.empty() ? "face " + std::to_string(index) : names,
                                                       surface),
                                places);
}

TriangleMesh triangle_mesh(const geometry::Shape& shape, double deviation, double angle,
                           std::uint64_t face_triangles) {
  io::MeshOptions options;
  options.linear_deflection = deviation;
  options.angular_deflection = angle;
  options.face_triangles = static_cast<std::size_t>(face_triangles);
  io::IndexedMesh mesh;
  try {
    mesh = io::indexed_mesh(shape.occt(), options);
  } catch (const io::FaceError& error) {
    throw std::runtime_error(untriangulated_face_message(shape, error.index(), error.surface()));
  }
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
  // Each triangle's face (mitcad#2385).
  out.faces.reserve(mesh.faces.size());
  for (const std::uint32_t face : mesh.faces) {
    out.faces.push_back(face);
  }
  return out;
}

namespace {

// The triangulation of a mesh body; with `uvs` (u, v per vertex, or empty)
// as its UV nodes.
occ::handle<Poly_Triangulation> triangulation_of(rust::Slice<const double> vertices,
                                                 rust::Slice<const std::uint32_t> triangles,
                                                 rust::Slice<const double> uvs) {
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
  const bool textured = !uvs.empty();
  if (textured && uvs.size() != 2 * nodes) {
    throw std::invalid_argument(std::to_string(uvs.size() / 2) + " texture coordinates for " +
                                std::to_string(nodes) + " vertices");
  }
  const occ::handle<Poly_Triangulation> mesh =
      new Poly_Triangulation(static_cast<int>(nodes), static_cast<int>(count), textured);
  for (std::size_t i = 0; i < nodes; ++i) {
    mesh->SetNode(static_cast<int>(i) + 1,
                  gp_Pnt(vertices[3 * i], vertices[3 * i + 1], vertices[3 * i + 2]));
    if (textured) {
      const double u = uvs[2 * i];
      const double v = uvs[2 * i + 1];
      if (!std::isfinite(u) || !std::isfinite(v)) {
        throw std::invalid_argument("a texture coordinate is not a number");
      }
      mesh->SetUVNode(static_cast<int>(i) + 1, gp_Pnt2d(u, v));
    }
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
  return mesh;
}

} // namespace

std::shared_ptr<geometry::Shape> mesh_from_triangles(rust::Slice<const double> vertices,
                                                     rust::Slice<const std::uint32_t> triangles) {
  return std::make_shared<geometry::Shape>(
      io::mesh_body(triangulation_of(vertices, triangles, rust::Slice<const double>())));
}

std::shared_ptr<geometry::Shape> mesh_from_textured_triangles(rust::Slice<const double> vertices,
                                                              rust::Slice<const std::uint32_t> triangles,
                                                              rust::Slice<const double> uvs) {
  if (uvs.empty()) {
    throw std::invalid_argument("a textured mesh without texture coordinates");
  }
  return std::make_shared<geometry::Shape>(io::mesh_body(triangulation_of(vertices, triangles, uvs)));
}

} // namespace mitcad::bridge
