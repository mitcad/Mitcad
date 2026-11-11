// SPDX-License-Identifier: MIT
// The project picker's New Folder (mitcad#1649): the name proposed (New
// Folder, New Folder 2, ... past the names there, case ignored), the name
// typed, and why the button waits.
#include "projects/PickerNewFolder.hpp"

#include <cstdio>

#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "picker_new_folder.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

const QString kBase = QStringLiteral("New Folder");

void freeNameTests() {
  // An empty folder: the name itself.
  CHECK(freeFolderName(kBase, {}) == QStringLiteral("New Folder"));
  // Other names there do not matter.
  CHECK(freeFolderName(kBase, {QStringLiteral("parts"), QStringLiteral("plate.mitcad")}) == kBase);
  // Taken: the next number, from 2.
  CHECK(freeFolderName(kBase, {QStringLiteral("New Folder")}) == QStringLiteral("New Folder 2"));
  CHECK(freeFolderName(kBase, {QStringLiteral("New Folder"), QStringLiteral("New Folder 2")}) ==
        QStringLiteral("New Folder 3"));
  // A gap is taken first.
  CHECK(freeFolderName(kBase, {QStringLiteral("New Folder"), QStringLiteral("New Folder 3")}) ==
        QStringLiteral("New Folder 2"));
  // The case of the letters ignored.
  CHECK(freeFolderName(kBase, {QStringLiteral("new folder")}) == QStringLiteral("New Folder 2"));
  CHECK(freeFolderName(kBase, {QStringLiteral("NEW FOLDER"), QStringLiteral("new FOLDER 2")}) ==
        QStringLiteral("New Folder 3"));
  // A file of that name takes it too.
  CHECK(freeFolderName(kBase, {QStringLiteral("New Folder")}) != QStringLiteral("New Folder"));
  // Another language's word (the base is translated).
  CHECK(freeFolderName(QStringLiteral("Kansio"), {QStringLiteral("Kansio")}) == QStringLiteral("Kansio 2"));
}

void nameTests() {
  CHECK(folderNameOf(QStringLiteral("Brackets")) == QStringLiteral("Brackets"));
  CHECK(folderNameOf(QStringLiteral("  Brackets  ")) == QStringLiteral("Brackets"));
  CHECK(folderNameOf(QStringLiteral("Left side")) == QStringLiteral("Left side"));
  CHECK(folderNameOf(QStringLiteral("v1.2 parts")) == QStringLiteral("v1.2 parts"));
  // No name of a folder.
  CHECK(folderNameOf(QString()).isEmpty());
  CHECK(folderNameOf(QStringLiteral("   ")).isEmpty());
  CHECK(folderNameOf(QStringLiteral(".")).isEmpty());
  CHECK(folderNameOf(QStringLiteral("..")).isEmpty());
  CHECK(folderNameOf(QStringLiteral(".hidden")).isEmpty());
  CHECK(folderNameOf(QStringLiteral("end.")).isEmpty());
  // A name, not a path.
  CHECK(folderNameOf(QStringLiteral("a/b")).isEmpty());
  CHECK(folderNameOf(QStringLiteral("a\\b")).isEmpty());
  for (const char* bad : {"a<b", "a>b", "a:b", "a\"b", "a|b", "a?b", "a*b"}) {
    CHECK(folderNameOf(QString::fromLatin1(bad)).isEmpty());
  }
  CHECK(folderNameOf(QStringLiteral("a") + QChar(0x09) + QStringLiteral("b")).isEmpty());
}

void blockedTests() {
  // A project or a folder that can be written: no reason, the button works.
  CHECK(newFolderBlocked(FolderHost::Place, true).isEmpty());
  // Each other case says why, each in its own words.
  const QString none = newFolderBlocked(FolderHost::None, true);
  const QString reading = newFolderBlocked(FolderHost::Reading, true);
  const QString naming = newFolderBlocked(FolderHost::Naming, true);
  const QString readOnly = newFolderBlocked(FolderHost::Place, false);
  CHECK(!none.isEmpty());
  CHECK(!reading.isEmpty());
  CHECK(!naming.isEmpty());
  CHECK(!readOnly.isEmpty());
  CHECK(none != reading && none != naming && none != readOnly && reading != naming && reading != readOnly &&
        naming != readOnly);
  CHECK(readOnly.contains(QStringLiteral("read-only")));
  // What is selected decides before the folder on disk.
  CHECK(newFolderBlocked(FolderHost::None, false) == none);
  // Short enough for a tooltip (100 characters).
  for (const QString& text : {none, reading, naming, readOnly}) {
    CHECK(text.size() <= 100);
  }
}

} // namespace

int pickerNewFolderTests() {
  failures = 0;
  freeNameTests();
  nameTests();
  blockedTests();
  return failures;
}
