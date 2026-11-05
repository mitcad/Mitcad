// SPDX-License-Identifier: MIT
#pragma once

// Repeated parts (mitcad#1277): which bodies are copies of one another moved
// by a rigid motion, a rotation and a translation, so that a design can hold
// them as occurrences of one component. A mirror image is not a rigid motion:
// a body without a mirror symmetry and its mirror image are two parts.
//
// Bodies are compared first by their topology counts (faces, edges,
// vertices), their volume, area and principal moments of inertia; then a
// frame of principal axes through the centre of mass of the first body is
// laid on the candidate's frames (a body with symmetries has several: the
// axes' signs, and where two or three moments are equal, the directions to
// its farthest vertices and face centroids), each fitted to the matching
// points by a least-squares rotation, until one puts every vertex and every
// face centroid of the first body within the tolerance (plus the vertices'
// own tolerances) of one of the candidate's. Lengths are millimetres. The
// bodies are not modified.

#include <cstddef>
#include <vector>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/transform.hpp"

namespace mitcad::geometry {

struct RigidCopy {
  // The body this one is a copy of: the first body of its kind in the
  // order given; the body itself for the first and for one that is a copy
  // of none.
  std::size_t of = 0;
  // From body `of` onto this one, a rotation and a translation; the
  // identity for `of` itself.
  Affine placement;
};

// One entry per body, in order. `tolerance` must be positive.
std::vector<RigidCopy> rigid_copies(const std::vector<const Shape*>& bodies, double tolerance);

} // namespace mitcad::geometry
