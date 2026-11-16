// SPDX-License-Identifier: MIT
#include "bridge/step_tree.hpp"

#include <cstddef>
#include <stdexcept>
#include <string>
#include <utility>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/io/step.hpp"
#include "mitcad_bridge/kernel/step_tree.h"

namespace mitcad::bridge {
namespace {

io::Placement placement_of(const TreePlacement& from) {
  io::Placement placement;
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      placement.rotation[r][c] = from.rotation[3 * r + c];
    }
    placement.translation[r] = from.translation[r];
  }
  placement.name = std::string(from.name);
  return placement;
}

io::LengthUnit length_unit(TreeUnit unit) {
  switch (unit) {
  case TreeUnit::Centimeter:
    return io::LengthUnit::Centimeter;
  case TreeUnit::Meter:
    return io::LengthUnit::Meter;
  case TreeUnit::Inch:
    return io::LengthUnit::Inch;
  case TreeUnit::Foot:
    return io::LengthUnit::Foot;
  default:
    return io::LengthUnit::Millimeter;
  }
}

} // namespace

void write_step_tree(rust::Str path, const ShapeList& shapes, rust::Slice<const TreeBody> bodies,
                     rust::Slice<const TreeDefinition> definitions, bool ap242, TreeUnit unit) {
  if (shapes.size() != bodies.size()) {
    throw std::invalid_argument("write_step_tree: one body per shape is needed");
  }
  io::StepTree tree;
  for (const TreeDefinition& from : definitions) {
    if (from.first_body > bodies.size() || from.body_count > bodies.size() - from.first_body) {
      throw std::invalid_argument("write_step_tree: a definition's bodies are not in the list");
    }
    io::StepTree::Definition definition;
    definition.name = std::string(from.name);
    for (std::size_t i = from.first_body; i < from.first_body + from.body_count; ++i) {
      const TreeBody& info = bodies[i];
      io::Body body;
      body.name = std::string(info.name);
      body.shape = shapes.at(i)->occt();
      if (info.has_color) {
        body.color = io::Color{info.color[0], info.color[1], info.color[2]};
      }
      for (const TreePlacement& placement : info.placements) {
        body.placements.push_back(placement_of(placement));
      }
      definition.bodies.push_back(std::move(body));
    }
    for (const TreeInstance& instance : from.instances) {
      definition.instances.push_back(io::StepTree::Instance{instance.definition, placement_of(instance.placement)});
    }
    tree.definitions.push_back(std::move(definition));
  }
  io::write_step_tree(std::string(path), tree,
                      io::StepWriteOptions{ap242 ? io::StepSchema::AP242 : io::StepSchema::AP214, length_unit(unit)});
}

} // namespace mitcad::bridge
