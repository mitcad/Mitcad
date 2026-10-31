// SPDX-License-Identifier: MIT
#include "bridge/dogbone.hpp"

#include <string>

#include "mitcad/geometry/dogbone.hpp"
#include "mitcad_bridge/kernel/dogbone.h"

namespace mitcad::bridge {

std::shared_ptr<geometry::Shape> dogbone_tool(const geometry::Shape& body, const Dogbone& spec) {
  geometry::DogboneSpec dogbone;
  dogbone.feature = std::string(spec.feature);
  for (const rust::String& edge : spec.edges) {
    dogbone.edges.emplace_back(edge);
  }
  for (const rust::String& face : spec.faces) {
    dogbone.faces.emplace_back(face);
  }
  dogbone.axis = {spec.axis[0], spec.axis[1], spec.axis[2]};
  dogbone.diameter = spec.diameter;
  dogbone.extra = spec.extra;
  dogbone.along_side = spec.along_side;
  return geometry::dogbone_tool(body, dogbone);
}

} // namespace mitcad::bridge
