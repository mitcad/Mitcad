// SPDX-License-Identifier: MIT
#include "bridge/edge_continuity.hpp"

#include <array>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "mitcad/geometry/edge_continuity.hpp"
#include "mitcad_bridge/kernel/edge_continuity.h"

namespace mitcad::bridge {
namespace {

std::uint8_t sides_code(geometry::EdgeSides sides) {
  switch (sides) {
  case geometry::EdgeSides::Shared:
    return 0;
  case geometry::EdgeSides::Seam:
    return 1;
  case geometry::EdgeSides::Open:
    break;
  }
  return 2;
}

std::uint8_t class_code(geometry::ContinuityClass reached) {
  switch (reached) {
  case geometry::ContinuityClass::None:
    return 0;
  case geometry::ContinuityClass::G0:
    return 1;
  case geometry::ContinuityClass::G1:
    return 2;
  case geometry::ContinuityClass::G2:
    return 3;
  case geometry::ContinuityClass::Unmeasured:
    break;
  }
  return 4;
}

std::array<double, 3> xyz(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

} // namespace

rust::Vec<EdgeContinuityInfo> edge_continuity(const geometry::Shape& shape, rust::Slice<const rust::String> edges,
                                              rust::Slice<const rust::String> faces, const ContinuityParams& params) {
  std::vector<int> wanted;
  for (const rust::String& name : edges) {
    const std::vector<int> found = shape.find_edges(std::string(name));
    if (found.empty()) {
      throw std::invalid_argument("an edge asked about is not on the body");
    }
    wanted.insert(wanted.end(), found.begin(), found.end());
  }
  std::vector<int> face_indices;
  for (const rust::String& name : faces) {
    const std::vector<int> found = shape.find_faces(std::string(name));
    if (found.empty()) {
      throw std::invalid_argument("a face asked about is not on the body");
    }
    face_indices.insert(face_indices.end(), found.begin(), found.end());
  }
  if (!face_indices.empty()) {
    const std::vector<int> bounding = geometry::edges_of_faces(shape, face_indices);
    wanted.insert(wanted.end(), bounding.begin(), bounding.end());
    if (wanted.empty()) {
      throw std::invalid_argument("the faces asked about have no edges");
    }
  }
  geometry::ContinuityTolerances tolerances;
  tolerances.gap = params.gap;
  tolerances.angle = params.angle;
  tolerances.curvature = params.curvature;
  rust::Vec<EdgeContinuityInfo> out;
  for (const geometry::EdgeContinuity& measured :
       geometry::edge_continuity(shape, wanted, static_cast<int>(params.samples), tolerances)) {
    EdgeContinuityInfo info{};
    info.name = rust::String(measured.name);
    info.sides = sides_code(measured.sides);
    info.reached = class_code(measured.reached);
    info.samples = static_cast<std::uint32_t>(measured.samples);
    info.gap = measured.gap;
    info.angle = measured.angle;
    info.curvature = measured.curvature;
    info.curvature_share = measured.curvature_share;
    info.gap_at = xyz(measured.gap_at);
    info.angle_at = xyz(measured.angle_at);
    info.curvature_at = xyz(measured.curvature_at);
    for (const gp_Pnt& p : measured.points) {
      info.points.push_back(p.X());
      info.points.push_back(p.Y());
      info.points.push_back(p.Z());
    }
    out.push_back(std::move(info));
  }
  return out;
}

} // namespace mitcad::bridge
