// SPDX-License-Identifier: MIT
// The Projects panel's rows (mitcad#529): a design's and a project's state
// as compact marks with the full words for the tooltip and the log's words
// as before, and a long name elided in the middle, its start and end kept.
#include "UnitTests.hpp"
#include "projects/ProjectRowText.hpp"

#include <cstdio>

#include <QChar>
#include <QFont>
#include <QFontMetrics>
#include <QString>

#include "projects/ProjectsTree.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "project_rows.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

const QString kDot = QStringLiteral(" ") + QChar(0x00B7) + QStringLiteral(" ");

void designMarkTests() {
  ProjectDesignRow design;
  design.path = QStringLiteral("parts/bracket.mitcad");
  design.version = 2;
  design.recorded = true;
  RowMarks marks = designMarks(design);
  CHECK(marks.text == QStringLiteral("v2"));
  CHECK(marks.icon.isEmpty());
  CHECK(marks.full == QStringLiteral("v2"));
  CHECK(marks.words == QStringLiteral("v2"));
  CHECK(marks.logged() == QStringLiteral("v2"));
  // Changed since its version: a star, the word in full.
  design.changed = true;
  marks = designMarks(design);
  CHECK(marks.text == QStringLiteral("v2*"));
  CHECK(marks.full == QStringLiteral("v2") + kDot + QStringLiteral("Changed"));
  CHECK(marks.words == QStringLiteral("v2, changed"));
  // Editing elsewhere: the lock icon, the words in the tooltip.
  design.changed = false;
  design.locked = true;
  design.lockOwner = QStringLiteral("Ada Tester");
  design.lockSameOwner = true;
  marks = designMarks(design);
  CHECK(marks.text == QStringLiteral("v2"));
  CHECK(marks.icon == QStringLiteral("lock"));
  CHECK(marks.full == QStringLiteral("v2") + kDot + QStringLiteral("Editing elsewhere"));
  CHECK(marks.words == QStringLiteral("v2, editing elsewhere"));
  CHECK(marks.logged() == QStringLiteral("v2 (lock)"));
  // Locked by someone else: the lock too, the holder in the words.
  design.lockSameOwner = false;
  design.lockOwner = QStringLiteral("Alex");
  marks = designMarks(design);
  CHECK(marks.icon == QStringLiteral("lock"));
  CHECK(marks.full == QStringLiteral("v2") + kDot + QStringLiteral("Locked: Alex"));
  CHECK(marks.words == QStringLiteral("v2, locked by Alex"));
  // This session's: the pencil.
  design.lockMine = true;
  marks = designMarks(design);
  CHECK(marks.icon == QStringLiteral("edit"));
  CHECK(marks.words == QStringLiteral("v2, editing"));
  // No version yet: "new", not "Changed" (it is not recorded at all).
  ProjectDesignRow fresh;
  fresh.path = QStringLiteral("new.mitcad");
  fresh.changed = true;
  marks = designMarks(fresh);
  CHECK(marks.text == QStringLiteral("new"));
  CHECK(marks.full == QStringLiteral("Not recorded"));
  CHECK(marks.words == QStringLiteral("not recorded"));
  CHECK(designDisplayName(QStringLiteral("parts/bracket.mitcad")) == QStringLiteral("bracket"));
  CHECK(designDisplayName(QStringLiteral("Plate.MITCAD")) == QStringLiteral("Plate"));
}

void projectMarkTests() {
  ProjectRow local;
  local.kind = ProjectRow::Kind::Local;
  RowMarks marks = projectMarks(local, -1, -1, QString(), false);
  // The row's own icon tells Local: no marks.
  CHECK(marks.text.isEmpty() && marks.icon.isEmpty());
  CHECK(marks.full == QStringLiteral("Local"));
  CHECK(marks.logged() == QStringLiteral("none"));
  ProjectRow cloud;
  cloud.kind = ProjectRow::Kind::Cloud;
  marks = projectMarks(cloud, 0, 0, QString(), false);
  CHECK(marks.icon == QStringLiteral("cloud-synced"));
  CHECK(marks.text.isEmpty());
  CHECK(marks.full == QStringLiteral("Cloud") + kDot + QString(QChar(0x2713)));
  CHECK(marks.words == QStringLiteral("cloud, synced"));
  marks = projectMarks(cloud, 2, 1, QString(), false);
  CHECK(marks.icon == QStringLiteral("cloud"));
  CHECK(marks.text == QString(QChar(0x2191)) + QStringLiteral("2 ") + QString(QChar(0x2193)) + QStringLiteral("1"));
  CHECK(marks.words == QStringLiteral("cloud, ahead 2, behind 1"));
  marks = projectMarks(cloud, -1, -1, QString(), false);
  CHECK(marks.icon == QStringLiteral("cloud"));
  CHECK(marks.words == QStringLiteral("cloud, not synced"));
  marks = projectMarks(cloud, 3, 0, QStringLiteral("network"), false);
  CHECK(marks.icon == QStringLiteral("cloud-offline"));
  CHECK(marks.text == QString(QChar(0x2191)) + QStringLiteral("3"));
  CHECK(marks.words == QStringLiteral("cloud, offline"));
  marks = projectMarks(cloud, 0, 0, QStringLiteral("auth_failed"), false);
  CHECK(marks.icon == QStringLiteral("warning"));
  CHECK(marks.words == QStringLiteral("cloud, sign-in needed"));
  marks = projectMarks(cloud, 0, 0, QString(), true);
  CHECK(marks.icon == QStringLiteral("warning"));
  CHECK(marks.words == QStringLiteral("cloud, conflict"));
  ProjectRow unchosen;
  unchosen.kind = ProjectRow::Kind::Local;
  unchosen.noRemoteChosen = true;
  marks = projectMarks(unchosen, -1, -1, QString(), false);
  CHECK(marks.icon == QStringLiteral("cloud") && marks.text == QStringLiteral("?"));
  CHECK(marks.words == QStringLiteral("cloud, no remote chosen"));
  ProjectRow missing;
  missing.kind = ProjectRow::Kind::Missing;
  CHECK(projectMarks(missing, -1, -1, QString(), false).icon == QStringLiteral("warning"));
  CHECK(projectMarks(missing, -1, -1, QString(), false).words == QStringLiteral("missing"));
  ProjectRow none;
  none.kind = ProjectRow::Kind::NoHistory;
  CHECK(projectMarks(none, -1, -1, QString(), false).words == QStringLiteral("no versions"));
  CHECK(projectMarks(none, -1, -1, QString(), false).full == QStringLiteral("No versions"));
}

void elisionTests() {
  const QFontMetrics metrics{QFont()};
  // A design's name of 55 characters.
  const QString name = QStringLiteral("mounting-bracket-for-the-left-side-motor-assembly-rev-b");
  CHECK(name.size() >= 40);
  const int whole = metrics.horizontalAdvance(name);
  // Wide enough: the whole name.
  CHECK(elideMiddle(name, metrics, whole) == name);
  CHECK(elideMiddle(name, metrics, whole + 50) == name);
  // A third of it: elided in the middle, its start and end kept, within the
  // width.
  const QString elided = elideMiddle(name, metrics, whole / 3);
  CHECK(elided != name);
  // Qt writes the ellipsis character, or three dots where the font lacks
  // its glyph (the default font of the Windows test run).
  const QString mark = elided.contains(QChar(0x2026)) ? QString(QChar(0x2026)) : QStringLiteral("...");
  CHECK(elided.contains(mark));
  CHECK(!elided.startsWith(mark) && !elided.endsWith(mark));
  const qsizetype cut = elided.indexOf(mark);
  CHECK(cut > 0 && name.startsWith(elided.left(cut)));
  CHECK(name.endsWith(elided.mid(cut + mark.size())));
  CHECK(metrics.horizontalAdvance(elided) <= whole / 3);
  // A short name fits as it is.
  CHECK(elideMiddle(QStringLiteral("plate"), metrics, whole / 3) == QStringLiteral("plate"));
  // No room: nothing, never more than the width.
  CHECK(metrics.horizontalAdvance(elideMiddle(name, metrics, 0)) <= metrics.horizontalAdvance(QChar(0x2026)));
}

} // namespace

int projectRowTests() {
  failures = 0;
  designMarkTests();
  projectMarkTests();
  elisionTests();
  return failures;
}
MITCAD_UNIT_TEST(projectRowTests);
