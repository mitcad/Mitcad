// SPDX-License-Identifier: MIT
// A sketch edit refused for conflicting constraints (mitcad#1566): the
// message shown without its ids line, the blocking items read from it.
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
}

} // namespace

int sketchConflictTests() {
  failures = 0;
  splitTests();
  return failures;
}
