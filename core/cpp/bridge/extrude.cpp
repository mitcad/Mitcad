// SPDX-License-Identifier: MIT
#include "bridge/extrude.hpp"

#include <string>

#include "mitcad/geometry/extrude.hpp"
#include "mitcad_bridge/kernel/extrude.h"

namespace mitcad::bridge {

std::shared_ptr<geometry::Shape> extrude(rust::Str feature, const Frame& frame,
                                         rust::Slice<const Region> regions, const Sweep& sweep) {
  geometry::ExtrudeSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.regions = to_geometry(regions);
  spec.direction = gp_Dir(sweep.direction[0], sweep.direction[1], sweep.direction[2]);
  spec.start = sweep.start;
  spec.end = sweep.end;
  return geometry::extrude(spec);
}

} // namespace mitcad::bridge
