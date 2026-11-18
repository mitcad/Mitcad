// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/query.rs.

#include <cstddef>
#include <cstdint>
#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/query.h.
struct FaceInfo;
struct EdgeInfo;
struct MassProperties;
struct BoundingBox;

rust::Vec<FaceInfo> faces(const geometry::Shape& shape);
rust::Vec<EdgeInfo> edges(const geometry::Shape& shape);
MassProperties mass_properties(const geometry::Shape& shape);
MassProperties fixed_point_properties(const geometry::Shape& shape);
BoundingBox bounding_box(const geometry::Shape& shape);
BoundingBox enclosing_box(const geometry::Shape& shape);
std::size_t count_edges(const geometry::Shape& shape, rust::Str name);
std::size_t count_faces(const geometry::Shape& shape, rust::Str name);
std::unique_ptr<ShapeList> solids(const geometry::Shape& shape);
std::size_t solid_count(const geometry::Shape& shape);
rust::Vec<BoundingBox> face_bounding_boxes(const geometry::Shape& shape, rust::Slice<const std::uint32_t> faces);
rust::Vec<BoundingBox> edge_bounding_boxes(const geometry::Shape& shape, rust::Slice<const std::uint32_t> edges);
rust::Vec<BoundingBox> bounding_boxes(const ShapeList& shapes);

} // namespace mitcad::bridge
