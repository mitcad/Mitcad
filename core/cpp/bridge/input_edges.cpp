// SPDX-License-Identifier: MIT
#include "bridge/input_edges.hpp"

#include <string>
#include <utility>
#include <vector>

#include "mitcad/geometry/input_edges.hpp"
#include "mitcad_bridge/kernel/input_edges.h"

namespace mitcad::bridge {
namespace {

std::vector<std::string> names(rust::Slice<const rust::String> given) {
  std::vector<std::string> result;
  result.reserve(given.size());
  for (const rust::String& name : given) {
    result.emplace_back(name.data(), name.size());
  }
  return result;
}

} // namespace

rust::Vec<EdgePolyline> edge_polylines(const geometry::Shape& shape, rust::Slice<const rust::String> edges,
                                       rust::Slice<const rust::String> faces, double deflection, double angle) {
  const std::vector<std::vector<gp_Pnt>> lines =
      geometry::edge_polylines(shape, names(edges), names(faces), deflection, angle);
  rust::Vec<EdgePolyline> result;
  for (const std::vector<gp_Pnt>& line : lines) {
    EdgePolyline polyline;
    for (const gp_Pnt& p : line) {
      polyline.points.push_back(p.X());
      polyline.points.push_back(p.Y());
      polyline.points.push_back(p.Z());
    }
    result.push_back(std::move(polyline));
  }
  return result;
}

} // namespace mitcad::bridge
