// SPDX-License-Identifier: MIT
#pragma once

// Revolution of sketch profiles about an axis into solids with named faces.

#include <optional>
#include <string>
#include <vector>

#include <gp_Ax1.hxx>

#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/target.hpp"

namespace mitcad::geometry {

// The regions turned about `axis`: side one by `angle1` (radians, the right
// hand turn about the axis direction), side two by `angle2` the other way,
// or side one up to `target` (a plane, face or body; no offset, not
// through). Together the angles make at most a full turn. The profile must
// not cross the axis; it may touch it.
//
// Faces are named after `feature` as an extrusion's: side(<segment>) from
// the segments off the axis; with one side start(<region>) at the profile
// and end(<region>) at the far end, with two sides start(<region>) at the
// end of side one and end(<region>) at the end of side two. A full turn has
// no caps; its side faces have a seam edge E{side(x)|side(x)}.
struct RevolveSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  gp_Ax1 axis;
  double angle1 = 0.0;
  std::optional<double> angle2;
  std::optional<Target> target;
};

ShapePtr revolve(const RevolveSpec& spec);

} // namespace mitcad::geometry
