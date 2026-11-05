// SPDX-License-Identifier: MIT
// An orbit past the edge of the screen (view/OrbitWrap.hpp, GitHub
// mitcad/Mitcad#139): where the cursor jumps to, on screens with any origin,
// and that the positions the view gets run on without a step across a jump,
// with moves queued before the jump, when the system refuses to move the
// cursor, and with the move the jump itself posts (macOS).
#include <cstdio>
#include <initializer_list>

#include <QPoint>
#include <QPointF>
#include <QRect>

#include "view/OrbitWrap.hpp"

namespace {
using mitcad::OrbitWrap;

int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "orbit_wrap.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

void targetTests() {
  const QRect screen(0, 0, 1920, 1080);
  CHECK(!OrbitWrap::target(QPointF(960, 540), screen));
  CHECK(!OrbitWrap::target(QPointF(17, 17), screen));
  CHECK(!OrbitWrap::target(QPointF(1902, 1062), screen));

  // Each edge to the opposite one, the other axis kept.
  CHECK(OrbitWrap::target(QPointF(1903, 500), screen) == QPoint(17, 500));
  CHECK(OrbitWrap::target(QPointF(1919, 500), screen) == QPoint(17, 500));
  CHECK(OrbitWrap::target(QPointF(16, 500), screen) == QPoint(1902, 500));
  CHECK(OrbitWrap::target(QPointF(0, 500), screen) == QPoint(1902, 500));
  CHECK(OrbitWrap::target(QPointF(700, 1063), screen) == QPoint(700, 17));
  CHECK(OrbitWrap::target(QPointF(700, 0), screen) == QPoint(700, 1062));
  // A corner, both axes.
  CHECK(OrbitWrap::target(QPointF(0, 0), screen) == QPoint(1902, 1062));

  // The target is not at an edge itself, or the cursor would jump back.
  for (const QPointF& edge : {QPointF(0, 0), QPointF(1919, 1079)}) {
    const auto to = OrbitWrap::target(edge, screen);
    CHECK(to && !OrbitWrap::target(*to, screen));
  }

  // A second monitor left of and above the first: negative coordinates.
  const QRect left(-1280, -200, 1280, 1024);
  CHECK(!OrbitWrap::target(QPointF(-640, 300), left));
  CHECK(OrbitWrap::target(QPointF(-1280, 300), left) == QPoint(-18, 300));
  CHECK(OrbitWrap::target(QPointF(-1, 300), left) == QPoint(-1263, 300));
  CHECK(OrbitWrap::target(QPointF(-640, -200), left) == QPoint(-640, 806));

  // A move that passed the edge of the monitor in one step still comes back.
  CHECK(OrbitWrap::target(QPointF(1950, 500), screen) == QPoint(17, 500));

  // Too small to jump on.
  CHECK(!OrbitWrap::target(QPointF(0, 0), QRect(0, 0, 64, 600)));
  CHECK(!OrbitWrap::target(QPointF(0, 0), QRect(0, 0, 800, 64)));
}

void positionTests() {
  // A widget 100,50 from the screen's origin.
  const QPointF widgetAt(100, 50);
  OrbitWrap wrap;
  const auto seen = [&wrap, &widgetAt](const QPointF& global) { return wrap.position(global - widgetAt, global); };

  CHECK(seen(QPointF(1000, 500)) == QPointF(900, 450));
  CHECK(!wrap.pending());
  CHECK(wrap.canJump());

  // A jump: the next moves run on from where the view last was, at the far
  // edge as if the screen went on. No step across it.
  const QPointF from(1917, 500);
  const QPointF to(3, 500);
  CHECK(seen(from) == QPointF(1817, 450));
  wrap.jumped(from, to);
  CHECK(wrap.pending());
  CHECK(!wrap.canJump());
  CHECK(seen(to) == QPointF(1817, 450));
  CHECK(!wrap.pending());
  CHECK(wrap.canJump());
  CHECK(seen(QPointF(13, 500)) == QPointF(1827, 450));
  CHECK(seen(QPointF(13, 510)) == QPointF(1827, 460));

  // A second jump adds to the first, two screens along.
  const QPointF again(1917, 510);
  CHECK(seen(again) == QPointF(3731, 460));
  wrap.jumped(again, QPointF(3, 510));
  CHECK(seen(QPointF(3, 510)) == QPointF(3731, 460));
  CHECK(seen(QPointF(23, 510)) == QPointF(3751, 460));

  // Back the way it came, and over the edge again, runs back to the start.
  wrap.reset();
  CHECK(seen(QPointF(1000, 500)) == QPointF(900, 450));
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500));
  CHECK(seen(QPointF(3, 500)) == QPointF(1817, 450));
  wrap.jumped(QPointF(2, 500), QPointF(1916, 500));
  CHECK(seen(QPointF(1916, 500)) == QPointF(1816, 450));
  CHECK(seen(QPointF(1000, 500)) == QPointF(900, 450));
}

void queuedTests() {
  // Moves on their way when the cursor jumped arrive with the old position:
  // they count, without the jump, and the jump is pending until one comes
  // from the far side. Then no step either.
  const QPointF widgetAt(100, 50);
  OrbitWrap wrap;
  const auto seen = [&wrap, &widgetAt](const QPointF& global) { return wrap.position(global - widgetAt, global); };
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500));
  CHECK(seen(QPointF(1918, 500)) == QPointF(1818, 450));
  CHECK(wrap.pending());
  CHECK(seen(QPointF(1919, 501)) == QPointF(1819, 451));
  CHECK(wrap.pending());
  CHECK(seen(QPointF(5, 501)) == QPointF(1819, 451));
  CHECK(!wrap.pending());
}

void refusedTests() {
  // After a refused jump the cursor is where it was, not where it was sent.
  const QPointF from(1917, 500);
  const QPointF to(3, 500);
  CHECK(OrbitWrap::landed(to, from, to));
  CHECK(OrbitWrap::landed(QPointF(5, 502), from, to));
  CHECK(!OrbitWrap::landed(from, from, to));
  CHECK(!OrbitWrap::landed(QPointF(1919, 500), from, to));

  // Without a jump nothing is added: the view's position is the event's.
  OrbitWrap wrap;
  CHECK(wrap.position(QPointF(1817, 450), from) == QPointF(1817, 450));
  CHECK(!wrap.pending());

  // A refusal ends the jumps of this drag (no call to the system on every
  // move at the edge) and adds nothing: the orbit stops at the edge.
  wrap.refused();
  CHECK(!wrap.canJump());
  CHECK(!wrap.pending());
  CHECK(wrap.position(QPointF(1818, 450), QPointF(1918, 500)) == QPointF(1818, 450));
  // Until the next drag.
  wrap.reset();
  CHECK(wrap.canJump());
}

void jumpMoveTests() {
  // macOS posts the cursor's move as a move without buttons: it is known by
  // the jump's target, once per jump, and brings the view nothing new.
  const QPointF widgetAt(100, 50);
  OrbitWrap wrap;
  CHECK(!wrap.takeJumpMove(QPointF(3, 500)));
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500));
  CHECK(!wrap.takeJumpMove(QPointF(40, 500)));
  CHECK(!wrap.takeJumpMove(QPointF(1917, 500)));
  CHECK(wrap.takeJumpMove(QPointF(3, 500)));
  CHECK(!wrap.takeJumpMove(QPointF(3, 500)));
  CHECK(wrap.position(QPointF(3, 500) - widgetAt, QPointF(3, 500)) == QPointF(1817, 450));
  CHECK(!wrap.pending());

  // Also when a move with the buttons came first.
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500));
  CHECK(wrap.position(QPointF(4, 500) - widgetAt, QPointF(4, 500)) == QPointF(3732, 450));
  CHECK(wrap.takeJumpMove(QPointF(3, 500)));

  // Not after the drag.
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500));
  wrap.reset();
  CHECK(!wrap.takeJumpMove(QPointF(3, 500)));
}

void resetTests() {
  OrbitWrap wrap;
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500));
  wrap.reset();
  CHECK(!wrap.pending());
  CHECK(wrap.position(QPointF(10, 20), QPointF(110, 70)) == QPointF(10, 20));
}
} // namespace

int orbitWrapTests() {
  targetTests();
  positionTests();
  queuedTests();
  refusedTests();
  jumpMoveTests();
  resetTests();
  return failures;
}
