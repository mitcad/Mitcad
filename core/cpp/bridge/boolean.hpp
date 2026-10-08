// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/boolean.rs.

#include <cstddef>
#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

class BooleanResult {
public:
  explicit BooleanResult(geometry::BooleanResult result) : m_result(std::move(result)) {}

  std::size_t piece_count() const { return m_result.pieces.size(); }
  std::shared_ptr<geometry::Shape> piece(std::size_t index) const;
  rust::Vec<std::size_t> piece_sources(std::size_t index) const;
  bool touched(std::size_t target) const;

private:
  geometry::BooleanResult m_result;
};

std::unique_ptr<BooleanResult> boolean_join(const ShapeList& targets, const geometry::Shape& tool);
std::unique_ptr<BooleanResult> boolean_cut(const ShapeList& targets, const geometry::Shape& tool);
std::unique_ptr<BooleanResult> boolean_intersect(const ShapeList& targets,
                                                 const geometry::Shape& tool);
std::unique_ptr<BooleanResult> removed_material(const geometry::Shape& before, const geometry::Shape& after,
                                                double slack);
// The join of a body with a near copy; not touching the body when the copy
// is not a near copy (mitcad#88).
std::unique_ptr<BooleanResult> join_near_copy(const geometry::Shape& body, const geometry::Shape& copy,
                                              double slack);

} // namespace mitcad::bridge
