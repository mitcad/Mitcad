// SPDX-License-Identifier: MIT
#pragma once

// Where an arrow kept clear of its origin (Manipulator::clearOfOrigin, an
// Extrude's distance and Start Offset at zero, mitcad#2481, mitcad#2508)
// draws its knob, and back: lifted beyond the origin at zero, less the
// further the value's point lies from the origin on the screen either way,
// as itself from kClearFade lifts on. Continuous and rising, so that a drag
// moves the value whose knob lies under the pointer, without a jump. Plain
// math in pixels along the arrow, unit tested (app/tests/clear_of_origin.cpp).

#include <algorithm>
#include <cmath>

namespace mitcad {

// Within this many lifts of the origin the lift fades out: ahead of the
// origin the knob moves 3/4 as far as the value's point, behind it 5/4.
inline constexpr double kClearFade = 4.0;

// Where the knob of a value `ahead` pixels from the origin along the arrow
// is drawn, lifted by `lift` pixels at zero (none: where it is).
inline double clearDrawn(double ahead, double lift) {
  if (!(lift > 0.0)) {
    return ahead;
  }
  const double fade = kClearFade * lift;
  return ahead + lift * std::max(0.0, 1.0 - std::abs(ahead) / fade);
}

// The value's pixels from the origin whose knob clearDrawn draws `drawn`
// pixels from it.
inline double clearValue(double drawn, double lift) {
  const double fade = kClearFade * lift;
  if (!(lift > 0.0) || drawn >= fade || drawn <= -fade) {
    return drawn;
  }
  return (drawn - lift) / (drawn >= lift ? 1.0 - lift / fade : 1.0 + lift / fade);
}

} // namespace mitcad
