// SPDX-License-Identifier: MIT
// A project's other files (mitcad#2204) in the Projects panel's rows and
// File > Open from Project: their marks ("new" when no version holds one,
// "*" when it changed, none when it is recorded as it is) with the full
// words for the tooltip and the log's words, and a place's contents with
// them after its sub-folders and designs, sorted by name.
#include "UnitTests.hpp"
#include "projects/ProjectBrowserData.hpp"
#include "projects/ProjectRowText.hpp"

#include <cstdio>

#include <QHash>
#include <QString>
#include <QStringList>
#include <QVector>

#include "projects/ProjectsTree.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "project_files.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

ProjectFileRow file(const QString& path, bool recorded, bool changed) {
  ProjectFileRow row;
  row.path = path;
  row.size = 1024;
  row.recorded = recorded;
  row.changed = changed;
  return row;
}

void fileMarkTests() {
  RowMarks marks = fileMarks(file(QStringLiteral("photo.png"), false, true));
  CHECK(marks.text == QStringLiteral("new"));
  CHECK(marks.icon.isEmpty());
  CHECK(marks.full == QStringLiteral("Not recorded"));
  CHECK(marks.words == QStringLiteral("not recorded"));
  CHECK(marks.logged() == QStringLiteral("new"));
  marks = fileMarks(file(QStringLiteral("photo.png"), true, true));
  CHECK(marks.text == QStringLiteral("*"));
  CHECK(marks.full == QStringLiteral("Changed"));
  CHECK(marks.words == QStringLiteral("changed"));
  CHECK(marks.logged() == QStringLiteral("*"));
  marks = fileMarks(file(QStringLiteral("photo.png"), true, false));
  CHECK(marks.text.isEmpty());
  CHECK(marks.full == QStringLiteral("Recorded"));
  CHECK(marks.words == QStringLiteral("recorded"));
  CHECK(marks.logged() == QStringLiteral("none"));
}

void browserTests() {
  ProjectRow shop;
  shop.kind = ProjectRow::Kind::Local;
  shop.root = QStringLiteral("/work/Shop");
  shop.name = QStringLiteral("Shop");
  shop.folderName = QStringLiteral("Shop");
  shop.folders = {QStringLiteral("docs")};
  ProjectDesignRow plate;
  plate.path = QStringLiteral("plate.mitcad");
  plate.version = 1;
  plate.recorded = true;
  shop.designs << plate;
  shop.files << file(QStringLiteral("zebra.pdf"), true, false) << file(QStringLiteral("Photo.png"), false, true)
             << file(QStringLiteral("docs/datasheet.pdf"), true, true);
  const QVector<BrowserEntry> top = folderContents(shop, QStringLiteral("Shop"), QString());
  QStringList words;
  for (const BrowserEntry& entry : top) {
    words << entryWords(entry);
  }
  // Sub-folders, designs, then the other files by name (case ignored).
  CHECK(words == (QStringList{QStringLiteral("docs/"), QStringLiteral("plate.mitcad v1"),
                              QStringLiteral("Photo.png file, not recorded"),
                              QStringLiteral("zebra.pdf file, recorded")}));
  CHECK(top.size() == 4 && top[2].kind == BrowserEntry::Kind::File);
  CHECK(top.size() == 4 && top[2].name == QStringLiteral("Photo.png"));
  CHECK(top.size() == 4 && top[2].key() == QStringLiteral("Shop/Photo.png"));
  const QVector<BrowserEntry> docs = folderContents(shop, QStringLiteral("Shop"), QStringLiteral("docs"));
  CHECK(docs.size() == 1 && entryWords(docs[0]) == QStringLiteral("datasheet.pdf file, changed"));
  // The search finds designs only.
  QHash<QString, QString> keys;
  keys.insert(shop.root, QStringLiteral("Shop"));
  CHECK(searchDesigns({shop}, keys, QStringLiteral("photo"), QString()).isEmpty());
}

} // namespace

int projectFileTests() {
  failures = 0;
  fileMarkTests();
  browserTests();
  return failures;
}
MITCAD_UNIT_TEST(projectFileTests);
