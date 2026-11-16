// SPDX-License-Identifier: MIT
// File > Open from Project's listing and search (mitcad#530) over a fixture
// project tree, as readProjects' rows give it: only projects with versions,
// a place's sub-folders first and then its designs, folders that only a
// design's path names, the search by part of a design's name in every
// project or in one, the path bar's parts and a step up.
#include "UnitTests.hpp"
#include "projects/ProjectBrowserData.hpp"

#include <cstdio>

#include <QString>
#include <QStringList>
#include <QVector>

#include "projects/ProjectsTree.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "project_browser.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

ProjectDesignRow design(const QString& path, int version) {
  ProjectDesignRow row;
  row.path = path;
  row.version = version;
  row.recorded = version > 0;
  return row;
}

// Two projects with versions (listed out of order), one without versions
// and one whose folder is gone.
QVector<ProjectRow> fixture() {
  ProjectRow team;
  team.kind = ProjectRow::Kind::Cloud;
  team.root = QStringLiteral("/fixture/Team");
  team.folderName = QStringLiteral("Team");
  team.name = QStringLiteral("Team");
  // "bolts" only in a design's path.
  team.designs << design(QStringLiteral("bolts/hex-bolt.mitcad"), 1) << design(QStringLiteral("gear-rack.mitcad"), 4);
  ProjectRow old;
  old.kind = ProjectRow::Kind::NoHistory;
  old.root = QStringLiteral("/fixture/Old");
  old.folderName = QStringLiteral("Old");
  old.name = QStringLiteral("Old");
  ProjectRow shop;
  shop.kind = ProjectRow::Kind::Local;
  shop.root = QStringLiteral("/fixture/Shop");
  shop.folderName = QStringLiteral("Shop");
  shop.name = QStringLiteral("shop");
  shop.folders = {QStringLiteral("empty"), QStringLiteral("parts"), QStringLiteral("parts/sub")};
  ProjectDesignRow gear = design(QStringLiteral("parts/gear.mitcad"), 2);
  gear.changed = true;
  ProjectDesignRow bracket = design(QStringLiteral("Bracket.mitcad"), 3);
  bracket.locked = true;
  bracket.lockOwner = QStringLiteral("Ada Tester");
  bracket.lockSameOwner = true;
  shop.designs << design(QStringLiteral("plate.mitcad"), 1) << gear << design(QStringLiteral("parts/sub/pin.mitcad"), 1)
               << bracket;
  ProjectRow gone;
  gone.kind = ProjectRow::Kind::Missing;
  gone.root = QStringLiteral("/fixture/Gone");
  gone.folderName = QStringLiteral("Gone");
  gone.name = QStringLiteral("Gone");
  return {team, old, shop, gone};
}

QStringList wordsOf(const QVector<BrowserEntry>& entries) {
  QStringList words;
  for (const BrowserEntry& entry : entries) {
    words << entryWords(entry);
  }
  return words;
}

QStringList keysOf(const QVector<BrowserEntry>& entries) {
  QStringList keys;
  for (const BrowserEntry& entry : entries) {
    keys << entry.key();
  }
  return keys;
}

void listingTests() {
  const QVector<ProjectRow> projects = browsableProjects(fixture());
  // Only the projects with versions, by name (case ignored).
  CHECK(projects.size() == 2);
  CHECK(projects.size() == 2 && projects[0].name == QStringLiteral("shop") && projects[1].name == QStringLiteral("Team"));
  const QHash<QString, QString> keys = projectKeys(projects);
  CHECK(keys.value(QStringLiteral("/fixture/Shop")) == QStringLiteral("Shop"));
  CHECK(keys.value(QStringLiteral("/fixture/Team")) == QStringLiteral("Team"));
  const ProjectRow* shop = findProject(projects, QStringLiteral("/fixture/Shop/"));
  CHECK(shop != nullptr);
  CHECK(findProject(projects, QStringLiteral("/fixture/Old")) == nullptr);
  if (shop == nullptr) {
    return;
  }
  CHECK(allFolders(*shop) == (QStringList{QStringLiteral("empty"), QStringLiteral("parts"),
                                          QStringLiteral("parts/sub")}));
  CHECK(subFolders(*shop, QString()) == (QStringList{QStringLiteral("empty"), QStringLiteral("parts")}));
  CHECK(subFolders(*shop, QStringLiteral("parts")) == QStringList{QStringLiteral("parts/sub")});
  // The project's top: its folders first, then its designs, each by name.
  const QVector<BrowserEntry> top = folderContents(*shop, QStringLiteral("Shop"), QString());
  CHECK(wordsOf(top) == (QStringList{QStringLiteral("empty/"), QStringLiteral("parts/"),
                                     QStringLiteral("Bracket.mitcad v3, editing elsewhere"),
                                     QStringLiteral("plate.mitcad v1")}));
  CHECK(keysOf(top) == (QStringList{QStringLiteral("Shop/empty/"), QStringLiteral("Shop/parts/"),
                                    QStringLiteral("Shop/Bracket.mitcad"), QStringLiteral("Shop/plate.mitcad")}));
  CHECK(top.size() == 4 && top[2].name == QStringLiteral("Bracket") && top[0].name == QStringLiteral("empty"));
  CHECK(top.size() == 4 && top[2].design.lockSameOwner && top[2].folder().isEmpty());
  // A folder with a sub-folder and a design.
  const QVector<BrowserEntry> parts = folderContents(*shop, QStringLiteral("Shop"), QStringLiteral("parts"));
  CHECK(wordsOf(parts) == (QStringList{QStringLiteral("sub/"), QStringLiteral("gear.mitcad v2, changed")}));
  CHECK(keysOf(parts) == (QStringList{QStringLiteral("Shop/parts/sub/"), QStringLiteral("Shop/parts/gear.mitcad")}));
  CHECK(parts.size() == 2 && parts[0].kind == BrowserEntry::Kind::Folder && parts[0].folder() == QStringLiteral("parts/sub"));
  CHECK(parts.size() == 2 && parts[1].folder() == QStringLiteral("parts"));
  CHECK(folderContents(*shop, QStringLiteral("Shop"), QStringLiteral("empty")).isEmpty());
  // A folder only a design's path names is a place too.
  const ProjectRow* team = findProject(projects, QStringLiteral("/fixture/Team"));
  CHECK(team != nullptr);
  if (team != nullptr) {
    CHECK(allFolders(*team) == QStringList{QStringLiteral("bolts")});
    CHECK(wordsOf(folderContents(*team, QStringLiteral("Team"), QString())) ==
          (QStringList{QStringLiteral("bolts/"), QStringLiteral("gear-rack.mitcad v4")}));
  }
  // Places: listed or not, a step up, the keys, the path bar.
  CHECK(placeExists(projects, {QStringLiteral("/fixture/Shop"), QStringLiteral("parts/sub")}));
  CHECK(placeExists(projects, {QStringLiteral("/fixture/Team"), QStringLiteral("bolts")}));
  CHECK(!placeExists(projects, {QStringLiteral("/fixture/Shop"), QStringLiteral("nothing")}));
  CHECK(!placeExists(projects, {QStringLiteral("/fixture/Gone"), QString()}));
  CHECK(placeUp({QStringLiteral("/fixture/Shop"), QStringLiteral("parts/sub")}) ==
        (BrowserPlace{QStringLiteral("/fixture/Shop"), QStringLiteral("parts")}));
  CHECK(placeUp({QStringLiteral("/fixture/Shop"), QStringLiteral("parts")}) ==
        (BrowserPlace{QStringLiteral("/fixture/Shop"), QString()}));
  CHECK(placeKey(QStringLiteral("Shop"), QString()) == QStringLiteral("Shop"));
  CHECK(placeKey(QStringLiteral("Shop"), QStringLiteral("parts/sub")) == QStringLiteral("Shop/parts/sub/"));
  const QVector<BrowserPathPart> path = pathParts(*shop, QStringLiteral("parts/sub"));
  CHECK(path.size() == 3);
  if (path.size() == 3) {
    CHECK(path[0].text == QStringLiteral("shop") && path[0].place.folder.isEmpty());
    CHECK(path[1].text == QStringLiteral("parts") && path[1].place.folder == QStringLiteral("parts"));
    CHECK(path[2].text == QStringLiteral("sub") && path[2].place.folder == QStringLiteral("parts/sub"));
    CHECK(path[2].place.root == QStringLiteral("/fixture/Shop"));
  }
  CHECK(pathParts(*shop, QString()).size() == 1);
}

void searchTests() {
  const QVector<ProjectRow> projects = browsableProjects(fixture());
  const QHash<QString, QString> keys = projectKeys(projects);
  // By part of the name, case ignored, in every project: by project, then
  // path.
  CHECK(keysOf(searchDesigns(projects, keys, QStringLiteral("GEAR"), QString())) ==
        (QStringList{QStringLiteral("Shop/parts/gear.mitcad"), QStringLiteral("Team/gear-rack.mitcad")}));
  // In another project than the one shown.
  const QVector<BrowserEntry> hex = searchDesigns(projects, keys, QStringLiteral("hex"), QString());
  CHECK(keysOf(hex) == QStringList{QStringLiteral("Team/bolts/hex-bolt.mitcad")});
  CHECK(hex.size() == 1 && hex[0].projectName == QStringLiteral("Team") && hex[0].folder() == QStringLiteral("bolts"));
  // Only in one project.
  CHECK(searchDesigns(projects, keys, QStringLiteral("hex"), QStringLiteral("/fixture/Shop")).isEmpty());
  CHECK(keysOf(searchDesigns(projects, keys, QStringLiteral("gear"), QStringLiteral("/fixture/Team"))) ==
        QStringList{QStringLiteral("Team/gear-rack.mitcad")});
  // A design's name, not its folder's nor its suffix.
  CHECK(searchDesigns(projects, keys, QStringLiteral("parts"), QString()).isEmpty());
  CHECK(searchDesigns(projects, keys, QStringLiteral("mitcad"), QString()).isEmpty());
  // Spaces around are left out; only spaces find nothing.
  CHECK(searchDesigns(projects, keys, QStringLiteral("  pin "), QString()).size() == 1);
  CHECK(searchDesigns(projects, keys, QStringLiteral("   "), QString()).isEmpty());
  // Every listed project's designs with an "e": plate, gear, Bracket,
  // hex-bolt, gear-rack (pin has none).
  CHECK(searchDesigns(projects, keys, QStringLiteral("e"), QString()).size() == 5);
}

} // namespace

int projectBrowserTests() {
  failures = 0;
  listingTests();
  searchTests();
  return failures;
}
MITCAD_UNIT_TEST(projectBrowserTests);
