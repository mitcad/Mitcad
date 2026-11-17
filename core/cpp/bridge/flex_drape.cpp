// SPDX-License-Identifier: MIT
#include "bridge/flex_drape.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/flex_drape.hpp"
#include "mitcad_bridge/kernel/flex_drape.h"

namespace mitcad::bridge {
namespace {

std::vector<gp_Pnt> points_of(const rust::Vec<double>& values) {
  if (values.size() % 3 != 0) {
    throw std::invalid_argument("a hanging item's points are three numbers each");
  }
  std::vector<gp_Pnt> out;
  for (std::size_t i = 0; i + 2 < values.size(); i += 3) {
    out.emplace_back(values[i], values[i + 1], values[i + 2]);
  }
  return out;
}

std::optional<gp_Dir> direction_of(bool has, const std::array<double, 3>& d) {
  const gp_Vec v(d[0], d[1], d[2]);
  if (!has || !(v.Magnitude() > 1e-12)) {
    return std::nullopt;
  }
  return gp_Dir(v);
}

std::optional<double> limit(double value) {
  if (value > 0.0) {
    return value;
  }
  return std::nullopt;
}

geometry::FlexDrapeContact contact_of(const DrapeContactData& data) {
  geometry::FlexDrapeContact out;
  out.obstacle = static_cast<int>(data.obstacle);
  out.face = std::string(data.face);
  if (const std::optional<gp_Dir> normal = direction_of(true, data.normal)) {
    out.normal = *normal;
  }
  return out;
}

DrapeContactData contact_data(const geometry::FlexDrapeContact& contact) {
  DrapeContactData out{};
  out.obstacle = static_cast<std::int32_t>(contact.obstacle);
  out.face = rust::String(contact.face);
  out.normal = {contact.normal.X(), contact.normal.Y(), contact.normal.Z()};
  return out;
}

} // namespace

DrapeStepData flex_drape_step(const ShapeList& obstacles, const DrapeStepInput& input) {
  geometry::FlexDrapeSpec spec;
  spec.points = points_of(input.points);
  for (const gp_Pnt& v : points_of(input.velocities)) {
    spec.velocities.emplace_back(v.XYZ());
  }
  for (const std::uint8_t pinned : input.pinned) {
    spec.pinned.push_back(pinned != 0 ? 1 : 0);
  }
  spec.segment = input.segment;
  spec.resting = input.resting;
  spec.bend_radius = input.bend_radius > 0.0 ? input.bend_radius : 0.0;
  const std::optional<gp_Dir> gravity = direction_of(true, input.gravity);
  if (!gravity) {
    throw std::invalid_argument("a hanging item needs the direction of gravity");
  }
  spec.gravity = *gravity;
  spec.seconds = input.seconds;
  if (input.has_grab) {
    spec.grab = static_cast<int>(input.grab);
    spec.target = gp_Pnt(input.target[0], input.target[1], input.target[2]);
  }
  spec.still = input.still;
  spec.carry = input.carry;
  spec.damping = input.damping;
  const geometry::FlexDrapeResult result = geometry::flex_drape_step(obstacles.items(), spec);
  DrapeStepData out{};
  for (const gp_Pnt& p : result.points) {
    out.points.push_back(p.X());
    out.points.push_back(p.Y());
    out.points.push_back(p.Z());
  }
  for (const gp_Vec& v : result.velocities) {
    out.velocities.push_back(v.X());
    out.velocities.push_back(v.Y());
    out.velocities.push_back(v.Z());
  }
  for (const gp_Dir& d : result.normals) {
    out.normals.push_back(d.X());
    out.normals.push_back(d.Y());
    out.normals.push_back(d.Z());
  }
  for (const geometry::FlexDrapeContact& contact : result.contacts) {
    out.contacts.push_back(contact_data(contact));
  }
  out.still = result.still;
  out.at_rest = result.at_rest;
  out.time = result.time;
  out.carry = result.carry;
  return out;
}

DrapeNodesData flex_drape_nodes(const ShapeList& obstacles, const DrapeNodesInput& input) {
  geometry::FlexDrapeNodesSpec spec;
  spec.points = points_of(input.points);
  for (const DrapeContactData& contact : input.contacts) {
    spec.contacts.push_back(contact_of(contact));
  }
  for (const std::uint32_t k : input.keep) {
    spec.keep.push_back(static_cast<int>(k));
  }
  if (input.has_start_port) {
    spec.start_direction = direction_of(true, input.start_direction);
    spec.start_x_axis = direction_of(input.has_start_x_axis, input.start_x_axis);
  }
  if (input.has_end_port) {
    spec.end_direction = direction_of(true, input.end_direction);
    spec.end_x_axis = direction_of(input.has_end_x_axis, input.end_x_axis);
  }
  spec.resting = input.resting;
  spec.half_width = input.half_width;
  spec.reach = input.reach;
  spec.bend_radius = limit(input.bend_radius);
  spec.bend_radius_edge = limit(input.bend_radius_edge);
  spec.max_twist = limit(input.max_twist);
  spec.tolerance = input.tolerance;
  const geometry::FlexDrapeNodes nodes = geometry::flex_drape_nodes(obstacles.items(), spec);
  DrapeNodesData out{};
  for (const int k : nodes.nodes) {
    out.nodes.push_back(static_cast<std::uint32_t>(k));
  }
  out.deviation = nodes.deviation;
  return out;
}

} // namespace mitcad::bridge
