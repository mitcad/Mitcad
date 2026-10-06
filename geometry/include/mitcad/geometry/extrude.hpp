// SPDX-License-Identifier: MIT
#pragma once

// Extrusion of sketch profiles into solids with named faces.

#include <optional>
#include <string>
#include <vector>

#include <gp_Dir.hxx>

#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/target.hpp"

namespace mitcad::geometry {

// The regions swept along `direction` between the offsets `start` and `end`
// from the sketch plane (start < end; e.g. -d/2 and d/2 for a symmetric
// extent). Faces are named after `feature`:
//   <feature>:side(<segment>)  swept from a profile segment
//   <feature>:start(<region>)  the cap at `start`
//   <feature>:end(<region>)    the cap at `end`
// Several regions are fused; touching regions become one solid whose merged
// faces carry the names of all of them. The result may hold several solids.
struct ExtrudeSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  gp_Dir direction{0.0, 0.0, 1.0};
  double start = 0.0;
  double end = 0.0;
};

// Throws std::exception subclasses on invalid input or kernel failure, as do
// all operations of the geometry library.
ShapePtr extrude(const ExtrudeSpec& spec);

// Where a thin extrusion's wall lies relative to the profile curves: on
// side 1 (outside a region, away from its material), centred on them, or on
// side 2 (inside).
enum class WallLocation { Side1, Center, Side2 };

struct ThinWall {
  WallLocation location = WallLocation::Center;
  double thickness = 0.0;
};

// One side of an extrusion: a distance from the start, or up to a target.
struct ExtrudeSide {
  std::optional<Target> target;
  double distance = 0.0; // without a target; > 0
  // Radians. A positive taper widens the profile away from the start: every
  // loop moves outwards from the material by t tan(taper) at distance t, so
  // the outer boundary grows and holes shrink.
  double taper = 0.0;
  // A thin extrusion sweeps walls along the profile curves instead of the
  // regions. Both sides are thin or neither.
  std::optional<ThinWall> thin;
};

// The extrude feature. Side one runs along `direction`, side two (two
// sides or symmetric) against it, both from the start: the profile plane
// moved by `start_offset` along the sketch normal, or the plane of `start`
// (a plane or a planar face; the profile is projected onto it along the
// sketch normal and needs no taper).
//
// Faces are named after `feature`:
//   side(<segment>)            swept from a profile segment
//   outer(<segment>), inner(<segment>)
//                              the wall faces of a thin extrusion, on side 1
//                              and side 2 of the curve
//   start(<region>)            one side: the cap at the start; two sides:
//                              the far cap of side one (`startFaces` in .f3d)
//   end(<region>)              the far cap (of side two with two sides); the
//                              faces of a target where it ends
//   mid(<region>)              what remains of the start cap between two
//                              sides of different shape
// A side face that a taper bends at the start is two pieces (#0, #1).
struct ExtrudeFeatureSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  gp_Dir direction{0.0, 0.0, 1.0};
  double start_offset = 0.0;
  std::optional<Target> start;
  ExtrudeSide side1;
  std::optional<ExtrudeSide> side2;
};

ShapePtr extrude_feature(const ExtrudeFeatureSpec& spec);

} // namespace mitcad::geometry
