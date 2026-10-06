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
std::shared_ptr<geometry::Shape> unite(const ShapeList& shapes);
std::unique_ptr<FaceTool> face_tool(const geometry::Shape& body,
                                    rust::Slice<const rust::String> faces);
std::shared_ptr<geometry::Shape> primitive(rust::Str feature, const Frame& frame,
                                           const PrimitiveInput& input);

} // namespace mitcad::bridge
