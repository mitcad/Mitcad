// SPDX-License-Identifier: MIT
// An orbit past the edge of the screen (view/OrbitWrap.hpp, GitHub
// mitcad/Mitcad#139): where the cursor jumps to, on screens with any origin,
// and that the positions the view gets run on without a step across a jump,
// with moves queued before the jump, when the system refuses to move the
// cursor, when it reports the jump but puts the cursor back (GitHub
// mitcad/Mitcad#546), also after a move at the target, on two screens side
// by side and at another screen's scale (mitcad#2188), and with the move the
// jump itself posts (macOS).
#include "UnitTests.hpp"
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>

#include <QPoint>
#include <QPointF>
#include <QRect>
#include <QtGlobal>

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

// How far the view's position moved between two moves, in either axis.
double step(const QPointF& a, const QPointF& b) {
  return std::max(std::abs(a.x() - b.x()), std::abs(a.y() - b.y()));
}

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
  // A widget 100,50 from the screen's origin; each move 8 ms after the last.
  const QPointF widgetAt(100, 50);
  OrbitWrap wrap;
  quint64 clock = 1000;
  const auto seen = [&wrap, &widgetAt, &clock](const QPointF& global) {
    clock += 8;
    return wrap.position(global - widgetAt, global, clock);
  };

  CHECK(seen(QPointF(1000, 500)) == QPointF(900, 450));
  CHECK(!wrap.pending());
  CHECK(wrap.canJump());

  // A jump: the next moves run on from where the view last was, at the far
  // edge as if the screen went on. No step across it.
  const QPointF from(1917, 500);
  const QPointF to(3, 500);
  CHECK(seen(from) == QPointF(1817, 450));
  wrap.jumped(from, to, clock);
  CHECK(wrap.pending());
  CHECK(!wrap.canJump());
  CHECK(seen(to) == QPointF(1817, 450));
  CHECK(!wrap.pending());
  CHECK(wrap.canJump());
  CHECK(!wrap.takeUndone());
  CHECK(seen(QPointF(13, 500)) == QPointF(1827, 450));
  CHECK(seen(QPointF(13, 510)) == QPointF(1827, 460));

  // A second jump adds to the first, two screens along: the drag crosses
  // the screen move by move (one move from the far side straight to the old
  // edge would be the cursor put back).
  for (double x = 100; x < 1917; x += 100) {
    CHECK(seen(QPointF(x, 510)) == QPointF(x + 1814, 460));
  }
  CHECK(!wrap.takeUndone());
  const QPointF again(1917, 510);
  CHECK(seen(again) == QPointF(3731, 460));
  wrap.jumped(again, QPointF(3, 510), clock);
  CHECK(seen(QPointF(3, 510)) == QPointF(3731, 460));
  CHECK(seen(QPointF(23, 510)) == QPointF(3751, 460));

  // Back the way it came, and over the edge again, runs back to the start.
  wrap.reset();
  CHECK(seen(QPointF(1000, 500)) == QPointF(900, 450));
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500), clock);
  CHECK(seen(QPointF(3, 500)) == QPointF(1817, 450));
  wrap.jumped(QPointF(2, 500), QPointF(1916, 500), clock);
  CHECK(seen(QPointF(1916, 500)) == QPointF(1816, 450));
  CHECK(seen(QPointF(1000, 500)) == QPointF(900, 450));

  // A fast move after the jump lands well past the edge's margin, nearer
  // the target than the old edge: the jump arrived, its distance added.
  wrap.reset();
  CHECK(seen(from) == QPointF(1817, 450));
  wrap.jumped(from, to, clock);
  CHECK(seen(QPointF(400, 500)) == QPointF(2214, 450));
  CHECK(!wrap.pending());
  CHECK(wrap.canJump());
  CHECK(!wrap.takeUndone());
}

void queuedTests() {
  // Moves on their way when the cursor jumped arrive with the old position,
  // stamped up to the move that jumped: they count, without the jump, and
  // the jump is pending until one comes from the far side. Then no step
  // either.
  const QPointF widgetAt(100, 50);
  OrbitWrap wrap;
  const auto seen = [&wrap, &widgetAt](const QPointF& global, quint64 stamp) {
    return wrap.position(global - widgetAt, global, stamp);
  };
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500), 2000);
  CHECK(seen(QPointF(1918, 500), 1996) == QPointF(1818, 450));
  CHECK(wrap.pending());
  CHECK(seen(QPointF(1919, 501), 2000) == QPointF(1819, 451));
  CHECK(wrap.pending());
  CHECK(seen(QPointF(5, 501), 2008) == QPointF(1819, 451));
  CHECK(!wrap.pending());
  CHECK(!wrap.takeUndone());

  // As X11 delivers them with a fast mouse, a move every millisecond
  // (mitcad#2188, mitcad#2205): the move that jumped, then the jump's own
  // move at the target (Qt merged the moves the server had before it
  // carried out the jump into it), then the moves from there. The view goes
  // on by the moves only, and the jump is kept.
  wrap.reset();
  quint64 clock = 3000;
  QPointF last;
  double largest = 0;
  const auto x11 = [&](const QPointF& global) {
    clock += 1;
    const QPointF at = wrap.position(global - widgetAt, global, clock);
    largest = std::max(largest, step(at, last));
    last = at;
    return at;
  };
  const QPointF from(1910, 500);
  const QPointF to(17, 500);
  x11(QPointF(1900, 500));
  CHECK(x11(from) == QPointF(1810, 450));
  largest = 0; // steps from here on
  wrap.jumped(from, to, clock);
  CHECK(x11(to) == QPointF(1810, 450));
  CHECK(!wrap.pending());
  CHECK(x11(QPointF(21, 500)) == QPointF(1814, 450));
  CHECK(x11(QPointF(29, 500)) == QPointF(1822, 450));
  CHECK(!wrap.takeUndone());
  CHECK(wrap.canJump());
  CHECK(largest <= OrbitWrap::kMargin);

  // The timestamp alone decides (mitcad#2205): a move stamped after the
  // jump that still comes from the old edge is the cursor put back, also
  // one stamped a millisecond later, close to the move that jumped.
  wrap.reset();
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500), 2000);
  CHECK(wrap.position(QPointF(1819, 450), QPointF(1919, 500), 2001) == QPointF(1819, 450));
  CHECK(wrap.takeUndone());
  CHECK(!wrap.pending());
  CHECK(!wrap.canJump());
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
  CHECK(wrap.position(QPointF(1817, 450), from, 10) == QPointF(1817, 450));
  CHECK(!wrap.pending());

  // A refusal ends the jumps of this drag (no call to the system on every
  // move at the edge) and adds nothing: the orbit stops at the edge.
  wrap.refused();
  CHECK(!wrap.canJump());
  CHECK(!wrap.pending());
  CHECK(wrap.position(QPointF(1818, 450), QPointF(1918, 500), 20) == QPointF(1818, 450));
  // Until the next drag.
  wrap.reset();
  CHECK(wrap.canJump());
}

void putBackTests() {
  // The system reported the jump (QCursor::pos at the target), but the moves
  // after it still come from the old edge (an absolute pointer puts the
  // cursor back): the jump is undone and the drag makes no other, so the
  // orbit stops at the edge, and coming back past the screen's middle moves
  // the view by the move only, never by a screen's width (GitHub
  // mitcad/Mitcad#546).
  const QPointF widgetAt(100, 50);
  OrbitWrap wrap;
  quint64 clock = 5000;
  QPointF last;
  double largest = 0;
  const auto seen = [&](const QPointF& global) {
    clock += 8;
    const QPointF at = wrap.position(global - widgetAt, global, clock);
    largest = std::max(largest, step(at, last));
    last = at;
    return at;
  };
  const QPointF from(1917, 500);
  const QPointF to(3, 500);
  seen(QPointF(1880, 500));
  CHECK(seen(from) == QPointF(1817, 450));
  largest = 0; // steps from here on
  wrap.jumped(from, to, clock);
  // Queued before the jump: counts without it, nothing undone.
  CHECK(wrap.position(from - widgetAt, from, clock) == QPointF(1817, 450));
  CHECK(wrap.pending());
  // Stamped later, still at the old edge: put back.
  CHECK(seen(QPointF(1919, 500)) == QPointF(1819, 450));
  CHECK(wrap.takeUndone());
  CHECK(!wrap.takeUndone());
  CHECK(!wrap.pending());
  CHECK(!wrap.canJump());
  CHECK(seen(QPointF(1919, 505)) == QPointF(1819, 455));
  // Back across the screen, past its middle and on to the other edge: the
  // view follows the cursor, with no step of a screen's width.
  for (double x = 1900; x >= 0; x -= 100) {
    CHECK(seen(QPointF(x, 505)) == QPointF(x - 100, 455));
  }
  CHECK(largest < 120);
  CHECK(!wrap.takeUndone());

  // A put-back's first move at the edge itself, then straight on past the
  // middle: the view gets the moves, never the jump's distance.
  wrap.reset();
  wrap.jumped(from, to, 7000);
  CHECK(wrap.position(QPointF(1819, 450), QPointF(1919, 500), 7008) == QPointF(1819, 450));
  CHECK(wrap.takeUndone());
  CHECK(!wrap.canJump());
  CHECK(wrap.position(QPointF(860, 450), QPointF(960, 500), 7016) == QPointF(860, 450));
  CHECK(wrap.position(QPointF(700, 450), QPointF(800, 500), 7024) == QPointF(700, 450));
}

// A drag of moves at `global` positions in a widget 100,50 from the screen's
// origin, each 8 ms after the last, with the largest step of the position
// the view gets between two moves since `measure`.
struct Drag {
  OrbitWrap wrap;
  QPointF widgetAt{100, 50};
  quint64 clock = 9000;
  QPointF last;
  bool started = false;
  double largest = 0;

  QPointF move(const QPointF& global) { return move(global - widgetAt, global); }
  // A move whose widget position is not the global one moved (another
  // screen's scale and origin).
  QPointF move(const QPointF& widget, const QPointF& global) {
    clock += 8;
    const QPointF at = wrap.position(widget, global, clock);
    if (started) {
      largest = std::max(largest, step(at, last));
    }
    started = true;
    last = at;
    return at;
  }
  void measure() { largest = 0; }
};

void landedPutBackTests() {
  // The system delivers a move at the jump's target first and only then
  // puts the cursor back at the old edge (XWayland: the jump's own move,
  // then the real pointer's next move; mitcad#2188). The jump is undone as
  // when the put-back comes first: the orbit stops at the edge, and coming
  // back past the screen's middle moves the view by the move only.
  {
    Drag drag;
    const QPointF from(1910, 500);
    const QPointF to(17, 500);
    drag.move(QPointF(1880, 500));
    CHECK(drag.move(from) == QPointF(1810, 450));
    drag.measure();
    drag.wrap.jumped(from, to, drag.clock);
    CHECK(drag.move(to) == QPointF(1810, 450));
    CHECK(!drag.wrap.pending());
    CHECK(drag.wrap.canJump());
    CHECK(drag.move(QPointF(1916, 500)) == QPointF(1816, 450));
    CHECK(drag.wrap.takeUndone());
    CHECK(!drag.wrap.canJump());
    CHECK(drag.move(QPointF(1919, 500)) == QPointF(1819, 450));
    for (double x = 1900; x >= 0; x -= 100) {
      CHECK(drag.move(QPointF(x, 500)) == QPointF(x - 100, 450));
    }
    CHECK(drag.largest < 120);
    CHECK(!drag.wrap.takeUndone());
  }

  // Also after more moves from the target (the put-back's move waited) and
  // on a corner, both axes put back.
  {
    Drag drag;
    const QPointF from(1915, 1070);
    const QPointF to(17, 17);
    drag.move(QPointF(1900, 1060));
    drag.move(from);
    drag.measure();
    drag.wrap.jumped(from, to, drag.clock);
    drag.move(to);
    drag.move(QPointF(22, 21));
    CHECK(!drag.wrap.takeUndone());
    drag.move(QPointF(1917, 1073));
    CHECK(drag.wrap.takeUndone());
    CHECK(drag.largest < 20);
  }

  // Put back onto the old side but short of the edge (the user moved back a
  // little meanwhile), and back beyond it, on a screen to the right (the
  // shared edge does not hold the cursor): both the edge's.
  CHECK(OrbitWrap::atOldEdge(QPointF(1900, 500), QPointF(1910, 500), QPointF(17, 500)));
  CHECK(OrbitWrap::atOldEdge(QPointF(2400, 700), QPointF(1910, 500), QPointF(17, 500)));
  CHECK(!OrbitWrap::atOldEdge(QPointF(1890, 500), QPointF(1910, 500), QPointF(17, 500)));
  CHECK(OrbitWrap::atOldEdge(QPointF(-30, 500), QPointF(5, 500), QPointF(1902, 500)));
  CHECK(!OrbitWrap::atOldEdge(QPointF(40, 500), QPointF(5, 500), QPointF(1902, 500)));
  CHECK(OrbitWrap::atOldEdge(QPointF(700, 10), QPointF(700, 4), QPointF(700, 1062)));
  CHECK(!OrbitWrap::atOldEdge(QPointF(700, 120), QPointF(700, 4), QPointF(700, 1062)));
  CHECK(!OrbitWrap::atOldEdge(QPointF(700, 4), QPointF(700, 4), QPointF(700, 4)));

  // A cube drag over the top edge with the cursor then sent straight back
  // to where it was pressed (a test step): not at the edge, so the jump is
  // kept and the release there is a drag, not a click.
  {
    Drag drag;
    const QPointF press(1700, 140);
    CHECK(drag.move(press) == QPointF(1600, 90));
    drag.move(QPointF(1700, 100));
    drag.move(QPointF(1700, 60));
    drag.move(QPointF(1700, 20));
    const QPointF from(1700, 0);
    drag.move(from);
    drag.wrap.jumped(from, QPointF(1700, 1062), drag.clock);
    drag.move(QPointF(1700, 1062));
    drag.move(QPointF(1700, 1022));
    CHECK(drag.move(press) != QPointF(1600, 90));
    CHECK(!drag.wrap.takeUndone());
  }

  // A drag that goes on across the whole screen, move by move, back to the
  // edge it jumped from, jumps again: it is not put back.
  {
    Drag drag;
    drag.move(QPointF(1910, 500));
    drag.measure();
    drag.wrap.jumped(QPointF(1910, 500), QPointF(17, 500), drag.clock);
    for (double x = 17; x <= 1915; x += 14) {
      drag.move(QPointF(x, 500));
    }
    CHECK(!drag.wrap.takeUndone());
    CHECK(drag.wrap.canJump());
    CHECK(drag.largest <= 14);
  }
}

void twoScreenTests() {
  // Two screens side by side, 1600 x 1000 each (mitcad#2188). The orbit is
  // on the left one, so its shared edge jumps like an outer edge: the
  // cursor goes on from the left screen's left edge, and a move that passed
  // the shared edge in one step comes back too.
  const QRect left(0, 0, 1600, 1000);
  CHECK(OrbitWrap::target(QPointF(1583, 500), left) == QPoint(17, 500));
  CHECK(OrbitWrap::target(QPointF(1599, 500), left) == QPoint(17, 500));
  CHECK(OrbitWrap::target(QPointF(1625, 500), left) == QPoint(17, 500));
  CHECK(!OrbitWrap::target(QPointF(1582, 500), left));
  // The outer edges and the top and bottom of the same screen.
  CHECK(OrbitWrap::target(QPointF(0, 500), left) == QPoint(1582, 500));
  CHECK(OrbitWrap::target(QPointF(800, 0), left) == QPoint(800, 982));
  CHECK(OrbitWrap::target(QPointF(800, 999), left) == QPoint(800, 17));

  // Over the shared edge and back past the middle: the view's position
  // follows the moves only.
  {
    Drag drag;
    for (double x = 800; x < 1583; x += 12) {
      drag.move(QPointF(x, 500));
    }
    drag.move(QPointF(1586, 500));
    const auto to = OrbitWrap::target(QPointF(1586, 500), left);
    CHECK(to && *to == QPoint(17, 500));
    drag.wrap.jumped(QPointF(1586, 500), QPointF(*to), drag.clock);
    for (double x = 17; x < 600; x += 12) {
      drag.move(QPointF(x, 500));
    }
    for (double x = 600; x > 30; x -= 12) {
      drag.move(QPointF(x, 500));
    }
    // Back over the left edge, to the shared edge's side again.
    drag.move(QPointF(25, 500));
    drag.move(QPointF(14, 500));
    const auto back = OrbitWrap::target(QPointF(14, 500), left);
    CHECK(back && *back == QPoint(1582, 500));
    drag.wrap.jumped(QPointF(14, 500), QPointF(*back), drag.clock);
    for (double x = 1582; x > 800; x -= 12) {
      drag.move(QPointF(x, 500));
    }
    CHECK(drag.largest <= 12);
    CHECK(!drag.wrap.takeUndone());
  }

  // A move past the shared edge at a device pixel ratio of 2: Qt keeps each
  // screen's origin in device pixels, so the right screen's global
  // positions start at 3200 while the widget's run on from 1600. The jump's
  // shift is the widget's (OcctViewer::wrapOrbitCursor): the view's
  // position does not move by a screen's width.
  {
    Drag drag;
    drag.widgetAt = QPointF(0, 0);
    drag.move(QPointF(1570, 500));
    drag.move(QPointF(1580, 500));
    drag.measure();
    // On the right screen: global 3220, the widget's 1610.
    CHECK(drag.move(QPointF(1610, 500), QPointF(3220, 500)) == QPointF(1610, 500));
    const auto to = OrbitWrap::target(QPointF(3220, 500), left);
    CHECK(to && *to == QPoint(17, 500));
    drag.wrap.jumped(QPointF(3220, 500), QPointF(*to), QPointF(1610, 500) - QPointF(*to), drag.clock);
    CHECK(drag.move(QPointF(*to)) == QPointF(1610, 500));
    CHECK(drag.move(QPointF(29, 500)) == QPointF(1622, 500));
    CHECK(drag.largest <= 30);
    // The system puts it back onto the right screen: undone, and the view
    // goes on from there by the widget's position.
    CHECK(drag.move(QPointF(1625, 500), QPointF(3250, 500)) == QPointF(1625, 500));
    CHECK(drag.wrap.takeUndone());
    CHECK(drag.largest <= 30);
  }

  // The cursor put back onto the right screen after a move at the target
  // (XWayland): undone, and the drag goes on onto the right screen and back
  // past the left one's middle without a step.
  {
    Drag drag;
    drag.move(QPointF(1570, 500));
    drag.move(QPointF(1585, 500));
    drag.measure();
    drag.wrap.jumped(QPointF(1585, 500), QPointF(17, 500), drag.clock);
    drag.move(QPointF(17, 500));
    drag.move(QPointF(1601, 500));
    CHECK(drag.wrap.takeUndone());
    for (double x = 1612; x < 2400; x += 12) {
      drag.move(QPointF(x, 500));
    }
    for (double x = 2400; x > 300; x -= 12) {
      drag.move(QPointF(x, 500));
    }
    CHECK(drag.largest <= 16);
    CHECK(!drag.wrap.canJump());
  }
}

void jumpMoveTests() {
  // macOS posts the cursor's move as a move without buttons: it is known by
  // the jump's target, once per jump, and brings the view nothing new.
  const QPointF widgetAt(100, 50);
  OrbitWrap wrap;
  CHECK(!wrap.takeJumpMove(QPointF(3, 500)));
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500), 100);
  CHECK(!wrap.takeJumpMove(QPointF(40, 500)));
  CHECK(!wrap.takeJumpMove(QPointF(1917, 500)));
  CHECK(wrap.takeJumpMove(QPointF(3, 500)));
  CHECK(!wrap.takeJumpMove(QPointF(3, 500)));
  CHECK(wrap.position(QPointF(3, 500) - widgetAt, QPointF(3, 500), 101) == QPointF(1817, 450));
  CHECK(!wrap.pending());

  // Also when a move with the buttons came first.
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500), 110);
  CHECK(wrap.position(QPointF(4, 500) - widgetAt, QPointF(4, 500), 111) == QPointF(3732, 450));
  CHECK(wrap.takeJumpMove(QPointF(3, 500)));

  // Not after the drag.
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500), 120);
  wrap.reset();
  CHECK(!wrap.takeJumpMove(QPointF(3, 500)));
}

void resetTests() {
  OrbitWrap wrap;
  wrap.jumped(QPointF(1917, 500), QPointF(3, 500), 10);
  CHECK(wrap.position(QPointF(1819, 450), QPointF(1919, 500), 20) == QPointF(1819, 450));
  wrap.reset();
  CHECK(!wrap.pending());
  CHECK(wrap.canJump());
  CHECK(!wrap.takeUndone());
  CHECK(wrap.position(QPointF(10, 20), QPointF(110, 70), 30) == QPointF(10, 20));
}
} // namespace

int orbitWrapTests() {
  targetTests();
  positionTests();
  queuedTests();
  refusedTests();
  putBackTests();
  landedPutBackTests();
  twoScreenTests();
  jumpMoveTests();
  resetTests();
  return failures;
}
MITCAD_UNIT_TEST(orbitWrapTests);
