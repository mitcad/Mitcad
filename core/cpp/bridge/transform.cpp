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
  case PrimitiveKind::Cone:
    return geometry::PrimitiveKind::Cone;
  default:
    return geometry::PrimitiveKind::Box;
  }
}

geometry::Affine affine_of(const Affine& map) {
  geometry::Affine affine;
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      affine.linear[r][c] = map.linear[3 * r + c];
    }
    affine.translation[r] = map.translation[r];
  }
  return affine;
}

} // namespace

std::shared_ptr<geometry::Shape> transform_shape(const geometry::Shape& shape, const Affine& map,
                                                 rust::Str rename) {
  return geometry::transform_shape(shape, affine_of(map), std::string(rename));
}

std::shared_ptr<geometry::Shape> move_shape(const geometry::Shape& shape, const Affine& map, rust::Str rename) {
  return geometry::move_shape(shape, affine_of(map), std::string(rename));
}

std::unique_ptr<ShapeList> transform_shapes(const ShapeList& shapes, rust::Slice<const Affine> maps) {
  std::vector<geometry::Affine> affines;
  affines.reserve(maps.size());
  for (const Affine& map : maps) {
    affines.push_back(affine_of(map));
  }
  auto out = std::make_unique<ShapeList>();
  for (std::shared_ptr<geometry::Shape>& shape : geometry::transform_shapes(shapes.items(), affines)) {
    out->push(std::move(shape));
  }
  return out;
}

std::shared_ptr<geometry::Shape> unite(const ShapeList& shapes) {
  std::vector<const geometry::Shape*> raw;
  for (const auto& shape : shapes.items()) {
    raw.push_back(shape.get());
  }
  return geometry::unite(raw);
}

std::shared_ptr<geometry::Shape> unite_within(const ShapeList& shapes, double within) {
  std::vector<const geometry::Shape*> raw;
  for (const auto& shape : shapes.items()) {
    raw.push_back(shape.get());
  }
  return geometry::unite(raw, within);
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
