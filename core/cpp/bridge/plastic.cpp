// SPDX-License-Identifier: MIT
#include "bridge/plastic.hpp"

#include <string>

#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/plastic.hpp"
#include "mitcad_bridge/kernel/plastic.h"

namespace mitcad::bridge {

std::shared_ptr<geometry::Shape> rim_band(const ShapeList& body, const RimBand& spec) {
  geometry::RimBandSpec band;
  band.feature = std::string(spec.feature);
  band.body = body.at(0);
  band.face = std::string(spec.face);
  band.outside = spec.outside;
  band.width = spec.width;
  band.free = spec.free;
  band.start = spec.start;
  band.end = spec.end;
  band.tangent = spec.tangent;
  band.side_taper = spec.side_taper;
  std::shared_ptr<geometry::Shape> made;
  geometry::run_operation("band", [&] { made = geometry::rim_band(band); });
  return made;
}

} // namespace mitcad::bridge
