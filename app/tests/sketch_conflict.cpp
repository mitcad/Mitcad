// SPDX-License-Identifier: MIT
// A sketch edit refused for conflicting constraints (mitcad#1566): the
// message shown without its ids line, the blocking items read from it; and
// when its one blocking item is offered for Replace, or Shift replaces it
// at once (mitcad#2090).
#include "UnitTests.hpp"
#include "sketch/SketchConflict.hpp"

#include <cstdio>

#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_conflict.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::sketch::ConflictText;
using mitcad::sketch::offersReplace;
using mitcad::sketch::RefusedBy;
using mitcad::sketch::replaceable;
using mitcad::sketch::shiftReplaces;
using mitcad::sketch::splitConflict;

void splitTests() {
  const ConflictText blocked = splitConflict(
      QStringLiteral("the Equal between two lines can't hold with the fillet\n[conflicts: k5, k12]"));
  CHECK(blocked.conflict);
  CHECK(blocked.message == QStringLiteral("the Equal between two lines can't hold with the fillet"));
  CHECK(blocked.items == (QStringList{QStringLiteral("k5"), QStringLiteral("k12")}));
  // Only fixed geometry: a conflict without items.
  const ConflictText fixed =
      splitConflict(QStringLiteral("the new dimension can't hold with projected geometry\n[conflicts: none]"));
  CHECK(fixed.conflict);
  CHECK(fixed.message == QStringLiteral("the new dimension can't hold with projected geometry"));
  CHECK(fixed.items.isEmpty());
  // Any other error as it is.
  const ConflictText other = splitConflict(QStringLiteral("the fillet does not fit on the lines"));
  CHECK(!other.conflict);
  CHECK(other.message == QStringLiteral("the fillet does not fit on the lines"));
  CHECK(other.items.isEmpty());
  CHECK(!splitConflict(QStringLiteral("[conflicts: k1]")).conflict);
  // The blocking item's words, before "can't hold with".
  const ConflictText one = splitConflict(
      QStringLiteral("the Horizontal on a line can't hold with the new Vertical\n[conflicts: k5]"));
  CHECK(one.words == QStringLiteral("the Horizontal on a line"));
  CHECK(fixed.words.isEmpty());
}

void replaceTests() {
  const ConflictText one = splitConflict(
      QStringLiteral("the Horizontal on a line can't hold with the new Vertical\n[conflicts: k5]"));
  const ConflictText two = splitConflict(
      QStringLiteral("the Horizontal on a line and the 20.00 mm distance can't hold with the new Vertical\n"
                     "[conflicts: k5, k7]"));
  const ConflictText none =
      splitConflict(QStringLiteral("the new Vertical can't hold with projected geometry\n[conflicts: none]"));
  const ConflictText other = splitConflict(QStringLiteral("the line is too short"));
  // One item in the trailer: that one.
  CHECK(replaceable(one) == QStringLiteral("k5"));
  CHECK(replaceable(two).isEmpty());
  CHECK(replaceable(none).isEmpty());
  CHECK(replaceable(other).isEmpty());
  // The button: one item, from a tool's pick, the selection or a
  // dimension's value; not for an inferred constraint nor one of several
  // sets.
  CHECK(offersReplace(one, RefusedBy::ToolPick, false));
  CHECK(offersReplace(one, RefusedBy::Selection, false));
  CHECK(offersReplace(one, RefusedBy::DimensionValue, false));
  CHECK(!offersReplace(one, RefusedBy::Inferred, false));
  CHECK(!offersReplace(one, RefusedBy::Selection, true));
  CHECK(!offersReplace(two, RefusedBy::ToolPick, false));
  CHECK(!offersReplace(none, RefusedBy::ToolPick, false));
  CHECK(!offersReplace(other, RefusedBy::ToolPick, false));
  // Shift: where the button would be, at a tool's click or a dimension's
  // Enter; never for the selection (a constraint key holds Shift), never
  // without Shift.
  CHECK(shiftReplaces(one, RefusedBy::ToolPick, false, true));
  CHECK(shiftReplaces(one, RefusedBy::DimensionValue, false, true));
  CHECK(!shiftReplaces(one, RefusedBy::ToolPick, false, false));
  CHECK(!shiftReplaces(one, RefusedBy::Selection, false, true));
  CHECK(!shiftReplaces(one, RefusedBy::Inferred, false, true));
  CHECK(!shiftReplaces(two, RefusedBy::ToolPick, false, true));
  CHECK(!shiftReplaces(none, RefusedBy::DimensionValue, false, true));
}

} // namespace

int sketchConflictTests() {
  failures = 0;
  splitTests();
  replaceTests();
  return failures;
}
MITCAD_UNIT_TEST(sketchConflictTests);
