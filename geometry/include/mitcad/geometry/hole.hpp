// SPDX-License-Identifier: MIT
#pragma once

// Drill holes as the hole feature makes them: the tool to cut, one
// solid of revolution per position.

#include <optional>
#include <string>
#include <vector>

#include <gp_Ax1.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/target.hpp"

namespace mitcad::geometry {

enum class HoleType { Simple, Counterbore, Countersink, Counterdrill };
enum class HoleExtent { Distance, ThroughAll, Target };

// Lengths in millimetres, angles in radians (full included angles).
// Measured along the hole's direction from its start point:
// - `depth` (Distance) ends the cylindrical part; the drill point is
//   beyond it (the depth is measured to the shoulder);
// - a counterbore's depth is from the start, and so is the countersink's
//   cone, which narrows from its diameter to the hole's; a counterdrill is
//   a counterbore (its diameter and depth) whose floor is a cone of the
//   countersink angle narrowing to the hole;
// - `taper` leans the wall: its angle to the axis, positive narrowing the
//   hole from its diameter at the start as it goes deeper; the drill point
//   starts from the wall's end;
// - ThroughAll runs past every body in `bodies` and has a flat end;
// - Target ends the cylindrical part where the axis first meets the target
//   (moved by its offset), with the drill point beyond.
struct HoleSpec {
  std::string feature;
  // The start points, pointing into the material.
  std::vector<gp_Ax1> positions;
  HoleType type = HoleType::Simple;
  double diameter = 0.0;
  double counterbore_diameter = 0.0;
  double counterbore_depth = 0.0;
  double countersink_diameter = 0.0;
  double countersink_angle = 0.0;
  // The drill point, or a flat bottom.
  double tip_angle = 118.0 * 3.14159265358979323846 / 180.0;
  bool flat = false;
  HoleExtent extent = HoleExtent::Distance;
  double depth = 0.0;
  std::optional<Target> target;
  std::vector<ShapePtr> bodies;
  double taper = 0.0;
};

// One solid per position, faces named <feature>:hole<i>.<part>, i the
// position's index: `wall` (the hole's cylinder, a cone when tapered),
// `tip` (the drill point or the flat bottom), `cbore_wall` and
// `cbore_floor` (counterbore; a counterdrill's wall too), `csink`
// (countersink cone), `cdrill` (a counterdrill's cone) and `top` (the disc
// at the start).
ShapePtr hole_tool(const HoleSpec& spec);

} // namespace mitcad::geometry
