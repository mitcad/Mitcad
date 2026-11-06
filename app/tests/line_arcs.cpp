// SPDX-License-Identifier: MIT
// The arcs of the sketch Line tool's drag (mitcad#1393): a drag along the
// curve (within 45 degrees of the way it leaves its end) draws a tangent
// arc, one across or back a normal arc, from a curve that takes one; a
// normal arc leaves the end square to the curve on the end's side, its
// centre on the curve's line; typed radius and sweep place the arc's end.
#include "sketch/LineArcs.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <optional>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "line_arcs.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::sketch::Curve2;
using mitcad::sketch::kPi;
using mitcad::sketch::LineArcKind;
using mitcad::sketch::V2;

bool same(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(1.0, std::abs(b)); }
bool same(const V2& a, const V2& b) { return same(a.x, b.x) && same(a.y, b.y); }

// The direction the arc runs at `p` (one of its ends), from `p` into it.
V2 leavingAt(const Curve2& arc, const V2& p) {
  using mitcad::sketch::dist;
  using mitcad::sketch::unit;
  if (dist(arc.startPoint(), p) < dist(arc.endPoint(), p)) {
    return unit(arc.tangent(arc.t0()));
  }
  return unit(arc.tangent(arc.t1())) * -1.0;
}

void kindTests() {
  using mitcad::sketch::lineArcKind;
  const V2 along{1.0, 0.0};
  CHECK(lineArcKind(along, {1.0, 0.3}, true) == LineArcKind::Tangent);
  CHECK(lineArcKind(along, {1.0, -0.9}, true) == LineArcKind::Tangent);
  CHECK(lineArcKind(along, {0.3, 1.0}, true) == LineArcKind::Normal);
  CHECK(lineArcKind(along, {0.3, -1.0}, true) == LineArcKind::Normal);
  // Back over the curve: normal too.
  CHECK(lineArcKind(along, {-1.0, 0.1}, true) == LineArcKind::Normal);
  // 45 degrees is the boundary.
  CHECK(lineArcKind(along, {1.0, 0.99}, true) == LineArcKind::Tangent);
  CHECK(lineArcKind(along, {1.0, 1.01}, true) == LineArcKind::Normal);
  // A curve that takes no normal arc (a spline): always tangent.
  CHECK(lineArcKind(along, {0.0, 1.0}, false) == LineArcKind::Tangent);
  // Leaving at an angle: measured from that direction, any length.
  const V2 diagonal{3.0, 3.0};
  CHECK(lineArcKind(diagonal, {1.0, 5.0}, true) == LineArcKind::Tangent);
  CHECK(lineArcKind(diagonal, {-5.0, 5.0}, true) == LineArcKind::Normal);
}

void arcTests() {
  using mitcad::sketch::dot;
  using mitcad::sketch::lineArc;
  using mitcad::sketch::lineArcDirection;
  const V2 start{10.0, 0.0};
  const V2 along{1.0, 0.0};
  // Tangent: leaves along the line, centre beside it.
  const auto tangent = lineArc(LineArcKind::Tangent, start, along, {20.0, 10.0});
  CHECK(tangent.has_value());
  if (tangent) {
    CHECK(same(tangent->center, V2{10.0, 10.0}));
    CHECK(same(tangent->radius, 10.0));
    CHECK(same(leavingAt(*tangent, start), along));
  }
  // Normal: leaves square to the line toward the end's side, its centre on
  // the line, as `sketch.arc` with `normal` makes it.
  const auto up = lineArc(LineArcKind::Normal, start, along, {15.0, 5.0});
  CHECK(up.has_value());
  if (up) {
    CHECK(same(up->center, V2{15.0, 0.0}));
    CHECK(same(up->radius, 5.0));
    CHECK(same(leavingAt(*up, start), V2{0.0, 1.0}));
    CHECK(same(dot(leavingAt(*up, start), along), 0.0));
  }
  const auto down = lineArc(LineArcKind::Normal, start, along, {15.0, -5.0});
  CHECK(down.has_value());
  if (down) {
    CHECK(same(down->center, V2{15.0, 0.0}));
    CHECK(same(leavingAt(*down, start), V2{0.0, -1.0}));
  }
  CHECK(same(lineArcDirection(LineArcKind::Normal, along, start, {12.0, -3.0}), V2{0.0, -1.0}));
  CHECK(same(lineArcDirection(LineArcKind::Tangent, along, start, {12.0, -3.0}), along));
  // Straight: none (a tangent arc straight ahead, a normal one straight
  // across).
  CHECK(!lineArc(LineArcKind::Tangent, start, along, {30.0, 0.0}).has_value());
  CHECK(!lineArc(LineArcKind::Normal, start, along, {10.0, 8.0}).has_value());
}

void endTests() {
  using mitcad::sketch::dist;
  using mitcad::sketch::lineArcEnd;
  using mitcad::sketch::tangentArc;
  const V2 start{0.0, 0.0};
  const V2 along{1.0, 0.0};
  // Nothing typed: the cursor.
  CHECK(same(lineArcEnd(start, along, {3.0, 8.0}, std::nullopt, std::nullopt), V2{3.0, 8.0}));
  // A radius: on that circle, on the cursor's side, toward the cursor.
  const V2 onCircle = lineArcEnd(start, along, {3.0, 8.0}, 5.0, std::nullopt);
  CHECK(same(dist(onCircle, V2{0.0, 5.0}), 5.0));
  const auto arc = tangentArc(start, along, onCircle);
  CHECK(arc.has_value() && same(arc->radius, 5.0));
  const V2 below = lineArcEnd(start, along, {3.0, -8.0}, 5.0, std::nullopt);
  CHECK(same(dist(below, V2{0.0, -5.0}), 5.0));
  // A radius and a sweep: exactly there, round either way.
  CHECK(same(lineArcEnd(start, along, {3.0, 8.0}, 5.0, kPi / 2.0), V2{5.0, 5.0}));
  CHECK(same(lineArcEnd(start, along, {3.0, -8.0}, 5.0, kPi / 2.0), V2{5.0, -5.0}));
  CHECK(same(lineArcEnd(start, along, {3.0, 8.0}, 5.0, kPi), V2{0.0, 10.0}));
  // A sweep alone: the chord at half the sweep, as long as the cursor
  // reaches along it.
  const V2 quarter = lineArcEnd(start, along, {10.0, 0.0}, std::nullopt, kPi / 2.0);
  CHECK(same(quarter, V2{5.0, 5.0}));
  const auto swept = tangentArc(start, along, quarter);
  CHECK(swept.has_value() && same(swept->radius, 5.0));
}

} // namespace

int lineArcsTests() {
  kindTests();
  arcTests();
  endTests();
  return failures;
}
