// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/transform.rs.

#include <memory>
#include <utility>

#include "bridge/shape.hpp"
#include "mitcad/geometry/pattern.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/transform.h.
struct Affine;
struct PrimitiveInput;

class FaceTool {
public:
  explicit FaceTool(geometry::FaceTool tool) : m_tool(std::move(tool)) {}

  std::shared_ptr<geometry::Shape> tool() const { return m_tool.tool; }
  bool material() const { return m_tool.material; }

private:
  geometry::FaceTool m_tool;
};

std::shared_ptr<geometry::Shape> transform_shape(const geometry::Shape& shape, const Affine& map,
                                                 rust::Str rename);
// A Move's motion: a rigid one as a location on the same B-rep (mitcad#838).
std::shared_ptr<geometry::Shape> move_shape(const geometry::Shape& shape, const Affine& map, rust::Str rename);
// transform_shape of each shape by its map, beside each other (mitcad#2021).
std::unique_ptr<ShapeList> transform_shapes(const ShapeList& shapes, rust::Slice<const Affine> maps);
std::shared_ptr<geometry::Shape> unite(const ShapeList& shapes);
// The union within a tolerance (mm, mitcad#397).
std::shared_ptr<geometry::Shape> unite_within(const ShapeList& shapes, double within);
std::unique_ptr<FaceTool> face_tool(const geometry::Shape& body,
                                    rust::Slice<const rust::String> faces);
std::shared_ptr<geometry::Shape> primitive(rust::Str feature, const Frame& frame,
                                           const PrimitiveInput& input);

} // namespace mitcad::bridge
