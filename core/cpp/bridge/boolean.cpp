// SPDX-License-Identifier: MIT
#include "bridge/boolean.hpp"

#include <optional>
#include <utility>
#include <vector>

#include "mitcad/geometry/removed.hpp"
#include "mitcad_bridge/kernel/boolean.h"

namespace mitcad::bridge {
namespace {

std::unique_ptr<BooleanResult> run(geometry::BooleanOp op, const ShapeList& targets,
                                   const geometry::Shape& tool) {
  std::vector<const geometry::Shape*> shapes;
  for (const auto& target : targets.items()) {
    shapes.push_back(target.get());
  }
  return std::make_unique<BooleanResult>(geometry::boolean(op, shapes, tool));
}

} // namespace

std::shared_ptr<geometry::Shape> BooleanResult::piece(std::size_t index) const {
  return m_result.pieces.at(index).shape;
}

rust::Vec<std::size_t> BooleanResult::piece_sources(std::size_t index) const {
  rust::Vec<std::size_t> sources;
  for (const std::size_t source : m_result.pieces.at(index).sources) {
    sources.push_back(source);
  }
  return sources;
}

bool BooleanResult::touched(std::size_t target) const {
  return target < m_result.touched.size() && m_result.touched[target];
}

std::unique_ptr<BooleanResult> boolean_join(const ShapeList& targets, const geometry::Shape& tool) {
  return run(geometry::BooleanOp::Join, targets, tool);
}

std::unique_ptr<BooleanResult> boolean_cut(const ShapeList& targets, const geometry::Shape& tool) {
  return run(geometry::BooleanOp::Cut, targets, tool);
}

std::unique_ptr<BooleanResult> boolean_intersect(const ShapeList& targets,
                                                 const geometry::Shape& tool) {
  return run(geometry::BooleanOp::Intersect, targets, tool);
}

std::unique_ptr<BooleanResult> removed_material(const geometry::Shape& before, const geometry::Shape& after,
                                                double slack) {
  return std::make_unique<BooleanResult>(geometry::removed_material(before, after, slack));
}

std::unique_ptr<BooleanResult> join_near_copy(const geometry::Shape& body, const geometry::Shape& copy,
                                              double slack) {
  std::optional<geometry::BooleanResult> joined = geometry::join_near_copy(body, copy, slack);
  if (!joined) {
    geometry::BooleanResult none;
    none.touched = {false};
    return std::make_unique<BooleanResult>(std::move(none));
  }
  joined->touched.assign(1, true);
  return std::make_unique<BooleanResult>(std::move(*joined));
}

} // namespace mitcad::bridge
