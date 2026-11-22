// SPDX-License-Identifier: MIT
// Horizontal/Vertical's direction (mitcad#2184, sketch/AutoConstraint.cpp
// horizontalVerticalDirection) against a mock of the sketch's answers: the
// types it would take on the picks, as `sketch.add_constraint` with
// `check` tells. The nearer direction when the sketch takes it; the other
// one when the nearer one is implied or conflicts and the other is taken;
// the nearer one, whose refusal then says why, when neither is taken.
#include "UnitTests.hpp"
#include "sketch/AutoConstraint.hpp"

#include <QSet>
#include <QString>
#include <QStringList>

#include <cstdio>
#include <functional>

namespace {

// The sketch as far as the decision goes: what it takes, and what it was
// asked, in order.
struct MockSketch {
  QSet<QString> takes;
  QStringList asked;

  std::function<bool(const QString&)> check() {
    return [this](const QString& type) {
      asked << type;
      return takes.contains(type);
    };
  }
};

} // namespace

int horizontalVerticalDirectionTests() {
  using namespace mitcad::sketch;
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) {
      std::fprintf(stderr, "horizontal/vertical direction: %s\n", message);
      ++failures;
    }
  };
  const QString h = QStringLiteral("horizontal");
  const QString v = QStringLiteral("vertical");
  const QString hp = QStringLiteral("horizontal_points");
  const QString vp = QStringLiteral("vertical_points");

  check(otherDirection(h) == v && otherDirection(v) == h, "a line's other direction");
  check(otherDirection(hp) == vp && otherDirection(vp) == hp, "two points' other direction");
  check(otherDirection(QStringLiteral("parallel")).isEmpty(), "no other direction for another type");

  {
    // The nearer direction taken: it is added, the other not asked about.
    MockSketch sketch{{hp, vp}, {}};
    check(horizontalVerticalDirection(hp, sketch.check()) == hp, "the nearer direction when it is taken");
    check(sketch.asked == QStringList{hp}, "the other direction is not asked about");
  }
  {
    // Two points already level (implied), or their heights held so that
    // level conflicts: plumb instead, no refusal.
    MockSketch sketch{{vp}, {}};
    check(horizontalVerticalDirection(hp, sketch.check()) == vp, "two points level already: plumb instead");
    check((sketch.asked == QStringList{hp, vp}), "the nearer direction asked first, then the other");
  }
  {
    // A line horizontal by its constraints whose vertical is valid (its
    // ends free to move): vertical.
    MockSketch sketch{{v}, {}};
    check(horizontalVerticalDirection(h, sketch.check()) == v, "a line horizontal already: vertical when valid");
  }
  {
    // A line horizontal by its constraints that cannot be vertical either
    // (its length held): the nearer direction, refused with today's message.
    MockSketch sketch;
    check(horizontalVerticalDirection(h, sketch.check()) == h, "both directions refused: the nearer one");
    check((sketch.asked == QStringList{h, v}), "both directions asked");
  }
  {
    // Two fixed points, neither level nor plumb: the nearer one.
    MockSketch sketch;
    check(horizontalVerticalDirection(vp, sketch.check()) == vp, "two points refused both ways: the nearer one");
  }
  {
    // A type with no other direction is left as it is, nothing asked.
    MockSketch sketch;
    const QString parallel = QStringLiteral("parallel");
    check(horizontalVerticalDirection(parallel, sketch.check()) == parallel, "another type stays");
    check(sketch.asked.isEmpty(), "another type asks nothing");
  }
  // Without a sketch to ask: the nearer one.
  check(horizontalVerticalDirection(hp, {}) == hp, "nothing to ask: the nearer one");
  return failures;
}
MITCAD_UNIT_TEST(horizontalVerticalDirectionTests);
