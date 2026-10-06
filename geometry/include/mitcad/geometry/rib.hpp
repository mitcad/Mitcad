// SPDX-License-Identifier: MIT
#pragma once

// Ribs and webs (F3): thin walls grown from open sketch curves, up to the
// next faces of bodies or a depth. Lengths are millimetres.

#include <optional>
#include <string>
#include <vector>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// Side 1 is along the sketch normal for a rib, left of each curve (looking
// along it with the normal up) for a web.
enum class ThicknessLocation { Symmetric, Side1, Side2 };

// A rib grows from one open chain in the sketch plane at right angles to
// the chain's chord (to its left, or right with `flip`), its thickness
// across the plane. A web grows from each chain (lines and arcs) along the
// sketch normal (against it with `flip`), its thickness in the plane.
// Without a depth they run up to the faces of `bodies` they reach first,
// which must close them off. The result is the tool to join.
//
// Faces of a rib: side(<curve>) along the curves, wall1 and wall2, tip0 and
// tip1 at the chain's ends, end at the depth. Of a web, per curve:
// wall1(<curve>), wall2(<curve>), tip0(<curve>), tip1(<curve>),
// start(<curve>) in the sketch plane and end(<curve>) at the depth.
struct RibSpec {
  std::string feature;
  Frame frame;
  bool web = false;
  std::vector<Path> chains;
  double thickness = 0.0;
  ThicknessLocation location = ThicknessLocation::Symmetric;
  std::optional<double> depth;
  bool flip = false;
  std::vector<ShapePtr> bodies;
};

ShapePtr rib(const RibSpec& spec);

} // namespace mitcad::geometry
