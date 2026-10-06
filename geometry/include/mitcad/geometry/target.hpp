// SPDX-License-Identifier: MIT
#pragma once

// What a sweep runs up to or starts from: the "to object" and "from
// object" entities (construction or origin planes, faces and bodies), shared
// by extrusions, revolutions and holes.

#include <string>

#include <gp_Pln.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

struct Target {
  enum class Kind { Plane, Face, Body };
  Kind kind = Kind::Plane;
  // Plane: an unbounded plane; its normal only orients `offset`.
  gp_Pln plane;
  // Face, Body: the body, and for Face a face name reference on it (all
  // pieces of a split face).
  ShapePtr body;
  std::string face;
  // Face: continue the face's surface past its edges (`isChained = false`
  // in .f3d); otherwise the face and the faces next to it.
  bool extend = true;
  // Body: run through the body to its far side instead of stopping at the
  // first face reached (`isMinimumSolution = false` in .f3d).
  bool through = false;
  // Moves the target: a plane or a planar face along its normal, turned to
  // point along the sweep, anything else along the sweep direction. A
  // positive offset makes the sweep longer.
  double offset = 0.0;
};

} // namespace mitcad::geometry
