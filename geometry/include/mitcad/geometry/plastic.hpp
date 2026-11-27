// SPDX-License-Identifier: MIT
#pragma once

// Plastic part features (mitcad#459): the band of a lip or a groove along a
// rim face. Lengths are millimetres.

#include <string>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The face `face` of `body` must be planar, with an outer loop and at least
// one inner loop that is an opening (the material does not go on over the
// face inside it: the cavity under a shelled body's rim, not the foot of a
// boss standing on the face). The band follows the outer loop (`outside`)
// or the largest opening; a face with two openings alike in size (the
// second more than half the first's area) has no rim and is refused. In
// the face's plane the band runs from the loop `width` into the face to
// `free` beyond the loop, away from the face, the offset loops' edges
// meeting in sharp corners; along the face's outward normal from `start`
// to `end` (offsets from its plane). Faces: band_side(<edge>) along the
// offset into the face, band_edge(<edge>) along the loop (<edge> the
// face's edge they follow, or its number in the loop when it has no name),
// band_top at `end`, band_bottom at `start`; a wall the offset made of no
// edge of the loop is band_wall(<k>). The body is not modified.
//
// The band's walls (mitcad#2511): with `tangent` the wall along the loop
// (band_edge) does not rise along the normal but runs, at each point of
// the loop, along the line that continues the body's other face at that
// edge along its tangent, at right angles to the edge (a ruled wall: a
// plane beside a plane, a cone beside a surface of revolution about the
// loop's axis), `free` beyond it in the plane at every height; sharp
// corners of the loop are mitred where the walls of lines meet and
// continued along their end tangents for curves. The other wall
// (band_side) leans by `side_taper` (radians) about the face's plane: at
// a height z along the normal it lies `width - z tan(side_taper)` into the
// face. A band whose walls would meet, a face beside the loop that runs
// along the rim face, or a corner the walls cannot follow is refused with
// the rim face (and the face beside it) as the error's places.
struct RimBandSpec {
  std::string feature;
  ShapePtr body;
  std::string face;
  bool outside = false;
  double width = 0.0;
  double free = 0.0;
  double start = 0.0;
  double end = 0.0;
  bool tangent = false;
  double side_taper = 0.0;
};

ShapePtr rim_band(const RimBandSpec& spec);

} // namespace mitcad::geometry
