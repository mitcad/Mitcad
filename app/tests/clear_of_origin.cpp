// SPDX-License-Identifier: MIT
// An arrow kept clear of its origin (mitcad#2481, mitcad#2508,
// framework/ClearOfOrigin.hpp): at zero its knob is drawn a lift beyond the
// origin, the lift fades out either way and is gone four lifts away; the
// knob's place rises with the value without a step, and clearValue gives
// back the value whose knob is drawn at a place, so that a drag's first
// move changes the value by about the pointer's movement, not by the lift.
#include "UnitTests.hpp"
#include "framework/ClearOfOrigin.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "clear_of_origin.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool nearly(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(1.0, std::abs(b)); }

void drawnTests() {
  // The Extrude distance's lift (twice a 9 px reach and 10 px) and one past
  // the Start Offset's arrow (that and its 34 px length further out).
  for (const double lift : {28.0, 90.0}) {
    // At zero a lift beyond the origin, clear of a click on the profile.
    CHECK(nearly(mitcad::clearDrawn(0.0, lift), lift));
    // Gone four lifts away, either way, and beyond.
    CHECK(nearly(mitcad::clearDrawn(4.0 * lift, lift), 4.0 * lift));
    CHECK(nearly(mitcad::clearDrawn(-4.0 * lift, lift), -4.0 * lift));
    CHECK(nearly(mitcad::clearDrawn(500.0, lift), 500.0));
    CHECK(nearly(mitcad::clearDrawn(-500.0, lift), -500.0));
    // Half way, half the lift.
    CHECK(nearly(mitcad::clearDrawn(2.0 * lift, lift), 2.5 * lift));
    CHECK(nearly(mitcad::clearDrawn(-2.0 * lift, lift), -1.5 * lift));
    // Rising everywhere, without a step: 3/4 of a pixel a pixel ahead of
    // the origin, 5/4 behind it.
    double last = mitcad::clearDrawn(-6.0 * lift, lift);
    for (int i = -599; i <= 600; ++i) {
      const double ahead = lift * i / 100.0;
      const double drawn = mitcad::clearDrawn(ahead, lift);
      CHECK(drawn > last);
      CHECK(drawn - last <= 1.25 * lift / 100.0 + 1e-9);
      last = drawn;
    }
    CHECK(nearly(mitcad::clearDrawn(1.0, lift) - mitcad::clearDrawn(0.0, lift), 0.75));
    CHECK(nearly(mitcad::clearDrawn(0.0, lift) - mitcad::clearDrawn(-1.0, lift), 1.25));
  }
  // No lift: where it is.
  CHECK(nearly(mitcad::clearDrawn(0.0, 0.0), 0.0));
  CHECK(nearly(mitcad::clearDrawn(-7.0, 0.0), -7.0));
}

void valueTests() {
  for (const double lift : {28.0, 90.0}) {
    // clearValue undoes clearDrawn, in the fade and beyond it.
    for (int i = -600; i <= 600; i += 7) {
      const double ahead = lift * i / 100.0;
      CHECK(std::abs(mitcad::clearValue(mitcad::clearDrawn(ahead, lift), lift) - ahead) < 1e-9 * lift);
    }
    // A drag from zero, grabbed where the knob is drawn: a first move of
    // 3 px ahead gives 4 px of value, 3 px back gives -2.4 px; not the
    // lift and the move.
    CHECK(nearly(mitcad::clearValue(lift + 3.0, lift), 4.0));
    CHECK(nearly(mitcad::clearValue(lift - 3.0, lift), -2.4));
    // A knob drawn on the origin stands for a value behind it.
    CHECK(mitcad::clearValue(0.0, lift) < 0.0);
  }
  CHECK(nearly(mitcad::clearValue(12.0, 0.0), 12.0));
}

} // namespace

int clearOfOriginTests() {
  drawnTests();
  valueTests();
  return failures;
}
MITCAD_UNIT_TEST(clearOfOriginTests);
