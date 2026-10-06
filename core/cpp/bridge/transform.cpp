// SPDX-License-Identifier: MIT
#include "bridge/transform.hpp"

#include <string>
#include <vector>

#include "bridge/profile.hpp"
#include "mitcad/geometry/primitive.hpp"
#include "mitcad/geometry/transform.hpp"
#include "mitcad_bridge/kernel/transform.h"

namespace mitcad::bridge {
namespace {

geometry::PrimitiveKind geometry_kind(PrimitiveKind kind) {
  switch (kind) {
  case PrimitiveKind::Cylinder:
    return geometry::PrimitiveKind::Cylinder;
  case PrimitiveKind::Sphere:
    return geometry::PrimitiveKind::Sphere;
  case PrimitiveKind::Torus:
    return geometry::PrimitiveKind::Torus;
  default:
    return geometry::PrimitiveKind::Box;
  }
}

} // namespace

std::shared_ptr<geometry::Shape> transform_shape(const geometry::Shape& shape, const Affine& map,
                                                 rust::Str rename) {
  geometry::Affine affine;
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      affine.linear[r][c] = map.linear[3 * r + c];
    }
    affine.translation[r] = map.translation[r];
  }
  return geometry::transform_shape(shape, affine, std::string(rename));
}

std::shared_ptr<geometry::Shape> unite(const ShapeList& shapes) {
  std::vector<const geometry::Shape*> raw;
  for (const auto& shape : shapes.items()) {
    raw.push_back(shape.get());
  }
  return geometry::unite(raw);
}

std::unique_ptr<FaceTool> face_tool(const geometry::Shape& body,
                                    rust::Slice<const rust::String> faces) {
  std::vector<std::string> names;
  for (const rust::String& face : faces) {
    names.emplace_back(face);
  }
  return std::make_unique<FaceTool>(geometry::face_tool(body, names));
}

std::shared_ptr<geometry::Shape> primitive(rust::Str feature, const Frame& frame,
                                           const PrimitiveInput& input) {
  geometry::PrimitiveSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.kind = geometry_kind(input.kind);
  spec.a = input.sizes[0];
  spec.b = input.sizes[1];
  spec.c = input.sizes[2];
  return geometry::primitive(spec);
}

} // namespace mitcad::bridge
