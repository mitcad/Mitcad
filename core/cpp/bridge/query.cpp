// SPDX-License-Identifier: MIT
#include "bridge/query.hpp"

#include <array>
#include <string>

#include "mitcad/geometry/query.hpp"
#include "mitcad_bridge/kernel/query.h"

namespace mitcad::bridge {
namespace {

std::array<double, 3> xyz(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

} // namespace

rust::Vec<FaceInfo> faces(const geometry::Shape& shape) {
  rust::Vec<FaceInfo> result;
  for (const geometry::FaceInfo& face : geometry::face_infos(shape)) {
    FaceInfo info;
    for (const std::string& name : face.names) {
      info.names.push_back(rust::String(name));
    }
    info.surface = rust::String(face.surface);
    info.area = face.area;
    result.push_back(std::move(info));
  }
  return result;
}

rust::Vec<EdgeInfo> edges(const geometry::Shape& shape) {
  rust::Vec<EdgeInfo> result;
  for (const geometry::EdgeInfo& edge : geometry::edge_infos(shape)) {
    EdgeInfo info;
    info.name = rust::String(edge.name);
    info.curve = rust::String(edge.curve);
    info.length = edge.length;
    result.push_back(std::move(info));
  }
  return result;
}

MassProperties mass_properties(const geometry::Shape& shape) {
  const geometry::MassProperties props = geometry::mass_properties(shape);
  MassProperties result;
  result.volume = props.volume;
  result.area = props.area;
  result.center = xyz(props.center);
  return result;
}

MassProperties fixed_point_properties(const geometry::Shape& shape) {
  const geometry::MassProperties props = geometry::fixed_point_properties(shape);
  MassProperties result;
  result.volume = props.volume;
  result.area = props.area;
  result.center = xyz(props.center);
  return result;
}

BoundingBox bounding_box(const geometry::Shape& shape) {
  const geometry::BoundingBox box = geometry::bounding_box(shape);
  BoundingBox result;
  result.empty = box.empty;
  result.min = xyz(box.min);
  result.max = xyz(box.max);
  return result;
}

std::size_t count_edges(const geometry::Shape& shape, rust::Str name) {
  return shape.find_edges(std::string(name)).size();
}

std::size_t count_faces(const geometry::Shape& shape, rust::Str name) {
  return shape.find_faces(std::string(name)).size();
}

std::unique_ptr<ShapeList> solids(const geometry::Shape& shape) {
  auto list = new_shape_list();
  for (auto& solid : geometry::solids(shape)) {
    list->push(std::move(solid));
  }
  return list;
}

} // namespace mitcad::bridge
