// SPDX-License-Identifier: MIT
// Linked designs (mitcad#1416): the model's refusal of an edit to a linked
// component read from an error message (alone, after a command's words, a
// name with spaces, a path in a folder), what is no such refusal, and the
// file a link's path names from the design's folder.
#include "files/LinkedFiles.hpp"

#include <cstdio>

#include <QDir>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "linked_files.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

void refusalTests() {
  const auto alone = mitcad::linkedRefusal(
      QStringLiteral("Electronics is linked from board.mitcad; it changes only with that file"));
  CHECK(alone.has_value());
  if (alone) {
    CHECK(alone->component == QStringLiteral("Electronics"));
    CHECK(alone->path == QStringLiteral("board.mitcad"));
  }
  const auto after = mitcad::linkedRefusal(
      QStringLiteral("Edit Base1: Motor housing is linked from ../parts/motor.mitcad; it changes only with that file"));
  CHECK(after.has_value());
  if (after) {
    CHECK(after->component == QStringLiteral("Motor housing"));
    CHECK(after->path == QStringLiteral("../parts/motor.mitcad"));
  }
  CHECK(!mitcad::linkedRefusal(QStringLiteral("component C9 does not exist")).has_value());
  CHECK(!mitcad::linkedRefusal(QStringLiteral("Electronics is linked from x.mitcad")).has_value());
  CHECK(!mitcad::linkedRefusal(QString()).has_value());
}

void pathTests() {
  const QString design = QDir::cleanPath(QDir::temp().absoluteFilePath(QStringLiteral("project/designs/holder.mitcad")));
  const QString folder = QDir::cleanPath(QDir::temp().absoluteFilePath(QStringLiteral("project")));
  CHECK(mitcad::linkedFilePath(design, QStringLiteral("part.mitcad")) == folder + QStringLiteral("/designs/part.mitcad"));
  CHECK(mitcad::linkedFilePath(design, QStringLiteral("../parts/part.mitcad")) ==
        folder + QStringLiteral("/parts/part.mitcad"));
  CHECK(mitcad::linkedFilePath(QString(), QStringLiteral("part.mitcad")).isEmpty());
  CHECK(mitcad::linkedFilePath(design, QString()).isEmpty());
  const QString absolute = folder + QStringLiteral("/parts/./part.mitcad");
  CHECK(mitcad::linkedFilePath(QString(), absolute) == folder + QStringLiteral("/parts/part.mitcad"));
  // Not on disk: the cleaned absolute paths.
  CHECK(mitcad::sameFile(folder + QStringLiteral("/parts/part.mitcad"),
                         folder + QStringLiteral("/designs/../parts/part.mitcad")));
  CHECK(!mitcad::sameFile(folder + QStringLiteral("/parts/part.mitcad"), folder + QStringLiteral("/parts/other.mitcad")));
  CHECK(!mitcad::sameFile(QString(), folder));
}

} // namespace

int linkedFilesTests() {
  refusalTests();
  pathTests();
  return failures;
}
