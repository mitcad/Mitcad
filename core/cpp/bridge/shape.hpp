// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/shape.rs: lists of shape handles, so Rust
// can pass several shapes to the geometry library at once.

#include <cstddef>
#include <memory>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::bridge {

class ShapeList {
public:
  void push(std::shared_ptr<geometry::Shape> shape) { m_items.push_back(std::move(shape)); }
  std::size_t size() const { return m_items.size(); }
  std::shared_ptr<geometry::Shape> at(std::size_t index) const { return m_items.at(index); }
  const std::vector<std::shared_ptr<geometry::Shape>>& items() const { return m_items; }

private:
  std::vector<std::shared_ptr<geometry::Shape>> m_items;
};

inline std::unique_ptr<ShapeList> new_shape_list() { return std::make_unique<ShapeList>(); }

} // namespace mitcad::bridge
