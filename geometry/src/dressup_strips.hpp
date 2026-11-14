// SPDX-License-Identifier: MIT
#pragma once

// The strips a fillet or chamfer built 0.1 % smaller leaves where the faces
// it takes away at its exact size were (mitcad#1106), and their removal
// (dressup.cpp, without_strips).

#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The most faces that may lie beside the strips OCCT's defeaturing removes
// as one (mitcad#1337). It grows every face beside a connected group of
// strips by the group's whole extent (the diagonal of its bounding box)
// and intersects all of those faces with each other: n faces, n(n-1)/2
// intersections of surfaces grown by that much, however narrow the strips.
// A strip 2 µm wide along a rounding of 44 edges round a circle, 124 mm
// across with 88 faces beside it (cylinders and tori), took 3 828 such
// intersections: more than 300 s and 5 GB, never done, and the import's
// watchdog gave the item up as hung while the thread kept its memory.
// A group with more faces beside it stays, as every strip did before
// mitcad#1106: the dressup 0.1 % smaller with its info, the strip far
// below the part's tolerances.
constexpr int kMostFacesBesideStrips = 16;

// The strips of `smaller`, a dressup of `input` built with its sizes 0.1 %
// smaller, that are to be removed: faces narrower than 2e-3 of `contact`
// (how far from its edges the dressup touches its faces at most) that were
// wider in the input, beside a face the dressup made (one whose name the
// input has not), in connected groups with at most kMostFacesBesideStrips
// faces beside them. `left`: the strips in groups with more faces beside
// them, which stay.
struct Strips {
  std::vector<int> faces;              // the strips to remove, by face index
  std::vector<int> left;               // the strips that stay
  std::vector<std::vector<int>> beside; // by face: the faces across its edges
  std::vector<char> made;              // by face: one the dressup made
};
Strips strips_to_remove(const Shape& smaller, const Shape& input, double contact);

// `smaller` without its strips (strips_to_remove): removed by OCCT's
// defeaturing, the faces round each strip extended to meet, the dressup's
// faces beside it taking its names. Null where no strip is removed, or the
// defeaturing fails or gives no valid solid of nearly the same volume. The
// defeaturing stops on this thread's cancel request (throwing Cancelled)
// and reports its progress to it.
ShapePtr without_strips(const Shape& smaller, const Shape& input, double contact);

} // namespace mitcad::geometry::detail
