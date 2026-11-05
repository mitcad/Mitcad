// SPDX-License-Identifier: MIT
#include "bridge/file_structure.hpp"

#include <cmath>
#include <cstddef>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/io/step.hpp"
#include "mitcad_bridge/kernel/file_structure.h"

namespace mitcad::bridge {
namespace {

// Colours pass through float and colour space conversions in files; 1e-6
// is far below what a display shows (as read_file rounds them).
double rounded(double c) {
  return std::round(c * 1e6) / 1e6;
}

void add_body(FileStructure& out, io::Body& body, ShapeList& shapes) {
  StructureBody info;
  info.name = rust::String(body.name);
  info.has_color = body.color.has_value();
  if (body.color) {
    info.color = {rounded(body.color->r), rounded(body.color->g), rounded(body.color->b)};
  }
  out.bodies.push_back(std::move(info));
  shapes.push(std::make_shared<geometry::Shape>(std::move(body.shape)));
}

StructureInstance instance_of(const io::StepAssembly::Instance& instance) {
  StructureInstance out;
  out.node = instance.node;
  out.name = rust::String(instance.placement.name);
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      out.rotation[3 * r + c] = instance.placement.rotation[r][c];
    }
    out.translation[r] = instance.placement.translation[r];
  }
  return out;
}

} // namespace

FileStructure read_step_structure(rust::Str path, bool keep_assembly, ShapeList& shapes) {
  io::StepRead read = io::read_step_structured(std::string(path), keep_assembly);
  FileStructure out;
  out.assembly = read.assembly.has_value();
  if (!read.assembly) {
    for (io::Body& body : read.bodies) {
      add_body(out, body, shapes);
    }
    return out;
  }
  for (io::StepAssembly::Node& node : read.assembly->nodes) {
    StructureNode info;
    info.name = rust::String(node.name);
    info.first_body = out.bodies.size();
    info.body_count = node.bodies.size();
    info.first_instance = out.instances.size();
    info.instance_count = node.instances.size();
    for (io::Body& body : node.bodies) {
      add_body(out, body, shapes);
    }
    for (const io::StepAssembly::Instance& instance : node.instances) {
      out.instances.push_back(instance_of(instance));
    }
    out.nodes.push_back(std::move(info));
  }
  for (const io::StepAssembly::Instance& instance : read.assembly->roots) {
    out.roots.push_back(instance_of(instance));
  }
  return out;
}

} // namespace mitcad::bridge
