// SPDX-License-Identifier: MIT
#include "bridge/face_mesh.hpp"

#include <cstddef>
#include <cstdint>
#include <stdexcept>

#include "bridge/mesh.hpp"
#include "mitcad/io/face_mesh.hpp"
#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/kernel/face_mesh.h"

namespace mitcad::bridge {

FaceMeshes face_meshes(const geometry::Shape& shape, double deviation, double angle,
                       std::uint64_t face_triangles) {
  io::MeshOptions options;
  options.linear_deflection = deviation;
  options.angular_deflection = angle;
  options.face_triangles = static_cast<std::size_t>(face_triangles);
  io::FaceMeshes mesh;
  try {
    mesh = io::face_meshes(shape.occt(), options);
  } catch (const io::FaceError& error) {
    throw std::runtime_error(untriangulated_face_message(shape, error.index(), error.surface()));
  }
  FaceMeshes out;
  out.vertices.reserve(mesh.vertices.size() * 3);
  out.normals.reserve(mesh.normals.size() * 3);
  for (std::size_t i = 0; i < mesh.vertices.size(); ++i) {
    for (int k = 0; k < 3; ++k) {
      out.vertices.push_back(mesh.vertices[i][static_cast<std::size_t>(k)]);
      out.normals.push_back(mesh.normals[i][static_cast<std::size_t>(k)]);
    }
  }
  out.triangles.reserve(mesh.triangles.size() * 3);
  for (const auto& triangle : mesh.triangles) {
    for (const std::uint32_t index : triangle) {
      out.triangles.push_back(index);
    }
  }
  out.face_triangles.reserve(mesh.face_triangles.size());
  for (const std::uint32_t count : mesh.face_triangles) {
    out.face_triangles.push_back(count);
  }
  return out;
}

} // namespace mitcad::bridge
