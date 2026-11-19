// SPDX-License-Identifier: MIT
#pragma once

// A planar face of a body extruded as a profile (mitcad#535): Extrude
// with a body face instead of sketch regions.

#include <optional>
#include <string>

#include <gp_Dir.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/target.hpp"

namespace mitcad::geometry {

// The face `face` of the body (one face, planar, with a surface) swept
// along its outward normal (against it when `reversed`): its bounded area
// with its holes, from the face moved `start_offset` along the outward
// normal, `distance1` along the sweep and, with `distance2`, that much
// against it (both > 0, or one of the two negative, as an extrusion's two
// sides may be, mitcad#765: the solid then lies between the two ends only;
// `distance1 + distance2 > 0`). The body is left as it is: the prism shares
// none of its sub-shapes.
//
// mitcad#541: with `direction` the sweep goes along it (against it when
// `reversed`) instead of the normal, the distances measured along it; it
// may not lie in the face's plane. `taper1` and `taper2` (radians, each
// side's) lean the sides as an extrusion's taper does: positive widens the
// face away from it (its outside grows, its holes shrink), along a
// direction by t tan(taper) in the face's plane at distance t. A face that
// is not planar (a curved face) needs a direction, and takes no taper and
// no start offset: its prism along the direction.
//
// mitcad#2033: with `target1` side one runs up to that target instead of
// `distance1` (which is then not used), and with `target2` side two (two
// sides: `distance2` or `target2`) up to its own, as a side of an extrusion
// of sketch profiles does (see Target: a plane, a face or a body, moved by
// its offset): the face's prism past the target, trimmed where it reaches
// it. The other side may be a distance, negative too (it ends on the
// target's side). A side up to a target takes a taper only along the face's
// normal; a curved face's side has no taper and no start offset.
//
// Faces are named after `feature`, as an extrusion's:
//   side(<edge>)   swept from an edge of the face, by the edge's name in
//                  the body; an edge without a name (a surface body's free
//                  edge) by its index among the face's edges
//   start(<key>)   one side: the cap at the start; two sides: the far cap
//                  of side one
//   end(<key>)     the far cap (of side two with two sides); the faces of a
//                  target where a side ends are named as its far cap
//   mid(<key>)     two sides built apart (a taper or a target): what
//                  remains of the face between them
// `key` is the face reference the feature stores (`face` is that reference
// as found in the body now).
struct FaceExtrudeSpec {
  std::string feature;
  std::string face;
  std::string key;
  bool reversed = false;
  double start_offset = 0.0;
  double distance1 = 0.0;
  std::optional<double> distance2;
  std::optional<gp_Dir> direction;
  double taper1 = 0.0;
  double taper2 = 0.0;
  std::optional<Target> target1;
  std::optional<Target> target2;
};

ShapePtr extrude_face(const Shape& body, const FaceExtrudeSpec& spec);

} // namespace mitcad::geometry
