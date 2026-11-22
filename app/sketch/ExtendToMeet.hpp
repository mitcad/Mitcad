// SPDX-License-Identifier: MIT
#pragma once

// Extend to Meet (mitcad#2178): the results the model's
// `sketch.extend_to_meet` chooses from, for the tool's preview and its Tab.
// The same list as `meet_results` in core/sketch/src/modify/meet.rs,
// formula for formula and in the same order, so that the result the
// preview shows is the one the command's `result` index makes.

#include <vector>

#include "SketchGeometry.hpp"

namespace mitcad::sketch {

// One curve's part of a result: whether its end (else its start) moves to
// the meeting point, and the curve as it is then.
struct MeetEnd {
  bool movesEnd = false;
  Curve2 curve;
};

// A way two curves meet: the point and each curve after.
struct MeetResult {
  V2 point;
  MeetEnd a;
  MeetEnd b;
};

// Why two curves have no result.
enum class NoMeet { None, NotLineOrArc, Parallel, Collinear, Apart, Concentric, Collapses };

// Every way the line or arc `a`, picked at `atA` (the part around the pick
// stays), and `b`, picked at `atB`, can meet at a point of their whole
// lines and circles, nearest the picks first; empty, with `why`, when
// there is none.
std::vector<MeetResult> meetResults(const Curve2& a, const V2& atA, const Curve2& b, const V2& atB,
                                    NoMeet* why = nullptr);

} // namespace mitcad::sketch
