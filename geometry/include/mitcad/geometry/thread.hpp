// SPDX-License-Identifier: MIT
#pragma once

// Modelled screw threads: a helical groove of a 60-degree (ISO metric,
// Unified) basic profile cut into cylindrical faces.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// Lengths in millimetres. The profile is the basic profile of ISO 68-1
// (H = 0.866025 P, flanks at 30 degrees from the radial direction): on an
// external thread the crest is the cylinder, a flat P/8 wide, and the root
// `depth` further in is P/4 wide; on an internal thread the cylinder is the
// minor diameter, the crest P/4 wide, and the root `depth` further out P/8
// wide. The basic depth is 5H/8 = 0.541266 P.
struct ThreadSpec {
  std::string feature;
  // Cylindrical faces of the body (references; all pieces of each).
  std::vector<std::string> faces;
  double pitch = 0.0;
  double depth = 0.0;
  bool right_handed = true;
  // Over the whole face, running out through both ends; otherwise `length`
  // from `offset` measured from the end the cylinder's axis points to
  // (`high_end`) or from the other end. An end of the thread at an end of
  // the face runs out through it.
  bool full_length = true;
  double length = 0.0;
  double offset = 0.0;
  bool high_end = true;
};

// The body with the threads cut in. The groove's faces are named
// <feature>:thread(<face reference>).
ShapePtr modeled_thread(const Shape& body, const ThreadSpec& spec);

} // namespace mitcad::geometry
