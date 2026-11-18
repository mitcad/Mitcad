// SPDX-License-Identifier: MIT
#include "bridge/query.hpp"

#include <array>
#include <cstdint>
#include <string>
#include <vector>

#include <TopExp_Explorer.hxx>

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

BoundingBox enclosing_box(const geometry::Shape& shape) {
  const geometry::BoundingBox box = geometry::enclosing_box(shape);
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

std::size_t solid_count(const geometry::Shape& shape) {
  std::size_t count = 0;
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

namespace {

std::vector<int> indices(rust::Slice<const std::uint32_t> items) {
  std::vector<int> result;
  result.reserve(items.size());
  for (const std::uint32_t i : items) {
    result.push_back(static_cast<int>(i));
  }
  return result;
}

rust::Vec<BoundingBox> boxes(const std::vector<geometry::BoundingBox>& found) {
  rust::Vec<BoundingBox> result;
  for (const geometry::BoundingBox& box : found) {
    BoundingBox out;
    out.empty = box.empty;
    out.min = xyz(box.min);
    out.max = xyz(box.max);
    result.push_back(out);
  }
  return result;
}

} // namespace

rust::Vec<BoundingBox> face_bounding_boxes(const geometry::Shape& shape, rust::Slice<const std::uint32_t> faces) {
  return boxes(geometry::face_bounding_boxes(shape, indices(faces)));
}

rust::Vec<BoundingBox> edge_bounding_boxes(const geometry::Shape& shape, rust::Slice<const std::uint32_t> edges) {
  return boxes(geometry::edge_bounding_boxes(shape, indices(edges)));
}

rust::Vec<BoundingBox> bounding_boxes(const ShapeList& shapes) {
  return boxes(geometry::bounding_boxes(shapes.items()));
}

} // namespace mitcad::bridge
