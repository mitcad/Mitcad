// SPDX-License-Identifier: MIT
// The Start page's project browser (mitcad#1301, files/StartBrowser.hpp)
// over a fixture of two projects with nested folders, as readProjects'
// rows give them: the row and folder remembered and shown again (a folder
// or project gone falls back), a place's folders and design cards with
// their project and folder, the pins of a project's designs, the search
// over every project and the recent list (a recent design in a project
// listed once, by its project key), a preview found on this computer, and
// the logs' lines.
#include "files/StartBrowser.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QVector>

#include "files/RecentDesigns.hpp"
#include "projects/ProjectBrowserData.hpp"
#include "projects/ProjectsTree.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "start_browser.cpp:%d: check failed: %s\n", line, expression);
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
  row.versionTime = 1700000000;
  return row;
}

// Shop (Local): plate at its top, parts/gear and parts/sub/pin; Team
// (Cloud): bolts/hex-bolt and a gear rack at its top; Old without versions
// (not offered). The projects are at `top`.
QVector<ProjectRow> fixture(const QString& top) {
  ProjectRow shop;
  shop.kind = ProjectRow::Kind::Local;
  shop.root = top + QStringLiteral("/Shop");
  shop.folderName = QStringLiteral("Shop");
  shop.name = QStringLiteral("Shop");
  shop.folders = {QStringLiteral("parts"), QStringLiteral("parts/sub")};
  ProjectDesignRow gear = design(QStringLiteral("parts/gear.mitcad"), 2);
  gear.modified = QDateTime::fromString(QStringLiteral("2026-11-01T10:00:00Z"), Qt::ISODate);
  shop.designs << design(QStringLiteral("plate.mitcad"), 1) << gear << design(QStringLiteral("parts/sub/pin.mitcad"), 1);
  ProjectRow team;
  team.kind = ProjectRow::Kind::Cloud;
  team.root = top + QStringLiteral("/Team");
  team.folderName = QStringLiteral("Team");
  team.name = QStringLiteral("Team");
  team.designs << design(QStringLiteral("bolts/hex-bolt.mitcad"), 1) << design(QStringLiteral("gear-rack.mitcad"), 4);
  ProjectRow old;
  old.kind = ProjectRow::Kind::NoHistory;
  old.root = top + QStringLiteral("/Old");
  old.folderName = QStringLiteral("Old");
  old.name = QStringLiteral("Old");
  return browsableProjects({team, old, shop});
}

QStringList keysOf(const QVector<start::Card>& cards) {
  QStringList keys;
  for (const start::Card& card : cards) {
    keys << card.key;
  }
  return keys;
}

void rowTests(const QString& top) {
  const QVector<ProjectRow> projects = fixture(top);
  // Recent when nothing is remembered.
  CHECK(start::loadRow().kind == start::Row::Kind::Recent);
  start::Row pinned;
  pinned.kind = start::Row::Kind::Pinned;
  start::saveRow(pinned);
  CHECK(start::loadRow() == pinned);
  CHECK(QSettings().value(start::rowKey()).toString() == QStringLiteral("pinned"));
  // A project's folder, remembered with its project.
  start::Row place;
  place.kind = start::Row::Kind::Project;
  place.root = top + QStringLiteral("/Shop");
  place.folder = QStringLiteral("parts/sub");
  start::saveRow(place);
  CHECK(start::loadRow() == place);
  CHECK(QSettings().value(start::folderKey()).toString() == QStringLiteral("parts/sub"));
  CHECK(start::shownRow(place, projects) == place);
  // The root spelt otherwise is the same project.
  start::Row spelt = place;
  spelt.root = top + QStringLiteral("/Shop/");
  CHECK(start::shownRow(spelt, projects).root == top + QStringLiteral("/Shop"));
  // A folder gone: the project's top; a project gone: Recent.
  start::Row gone = place;
  gone.folder = QStringLiteral("nothing");
  CHECK(start::shownRow(gone, projects).folder.isEmpty());
  CHECK(start::shownRow(gone, projects).kind == start::Row::Kind::Project);
  gone.root = top + QStringLiteral("/Old");
  CHECK(start::shownRow(gone, projects).kind == start::Row::Kind::Recent);
  CHECK(start::shownRow(pinned, projects) == pinned);
  // Recent again: the project's keys go.
  start::saveRow(start::Row());
  CHECK(start::loadRow().kind == start::Row::Kind::Recent);
  CHECK(!QSettings().contains(start::rootKey()));
  // Rows of Recent and Pinned do not compare their (unused) folders.
  start::Row recentRow;
  recentRow.folder = QStringLiteral("x");
  CHECK(recentRow == start::Row());
  CHECK(place != pinned);
}

void placeTests(const QString& top) {
  const QVector<ProjectRow> projects = fixture(top);
  const QHash<QString, QString> keys = projectKeys(projects);
  const ProjectRow* shop = findProject(projects, top + QStringLiteral("/Shop"));
  CHECK(shop != nullptr);
  if (shop == nullptr) {
    return;
  }
  const QString shopKey = keys.value(shop->root);
  const QString gearPath = top + QStringLiteral("/Shop/parts/gear.mitcad");
  // The top: its folder, then its design.
  const QVector<BrowserEntry> topEntries = folderContents(*shop, shopKey, QString());
  CHECK(start::placeFolders(topEntries).size() == 1);
  CHECK(start::placeLine(placeKey(shopKey, QString()), topEntries) ==
        QStringLiteral("Start page place: Shop (1 folders, 1 designs)"));
  // parts: a sub-folder and the gear, pinned.
  const QVector<BrowserEntry> parts = folderContents(*shop, shopKey, QStringLiteral("parts"));
  const QVector<BrowserEntry> folders = start::placeFolders(parts);
  CHECK(folders.size() == 1 && folders[0].key() == QStringLiteral("Shop/parts/sub/"));
  const QVector<start::Card> cards = start::placeCards(parts, {gearPath});
  CHECK(keysOf(cards) == QStringList{QStringLiteral("Shop/parts/gear.mitcad")});
  if (cards.size() == 1) {
    const start::Card& gear = cards[0];
    CHECK(gear.path == gearPath);
    CHECK(gear.name == QStringLiteral("gear.mitcad"));
    CHECK(gear.type == QStringLiteral("MITCAD"));
    CHECK(gear.location == QStringLiteral("Shop / parts"));
    CHECK(gear.pinned);
    CHECK(gear.inProject && !gear.missing);
    // When it changed: project_tree's time, before the version's.
    CHECK(gear.when == QDateTime::fromString(QStringLiteral("2026-11-01T10:00:00Z"), Qt::ISODate));
  }
  CHECK(start::placeLine(placeKey(shopKey, QStringLiteral("parts")), parts) ==
        QStringLiteral("Start page place: Shop/parts/ (1 folders, 1 designs)"));
  // The sub-folder: no folders, its design not pinned, the version's time.
  const QVector<start::Card> sub =
      start::placeCards(folderContents(*shop, shopKey, QStringLiteral("parts/sub")), {gearPath});
  CHECK(sub.size() == 1 && !sub[0].pinned && sub[0].location == QStringLiteral("Shop / parts / sub"));
  CHECK(sub.size() == 1 && sub[0].when == QDateTime::fromSecsSinceEpoch(1700000000));
  CHECK(start::projectsLine(projects) == QStringLiteral("Start page projects: Shop (local) | Team (cloud)"));
  CHECK(start::projectsLine({}) == QStringLiteral("Start page projects: none"));
}

RecentDesign recentDesign(const QString& path) {
  RecentDesign design;
  design.path = path;
  design.name = QFileInfo(path).fileName();
  design.type = recent::typeText(path);
  design.folder = recent::folderText(path);
  return design;
}

void searchTests(const QString& top) {
  const QVector<ProjectRow> projects = fixture(top);
  const QHash<QString, QString> keys = projectKeys(projects);
  // The recent list: a design in the work folder, Shop's gear (opened from
  // a card) and a STEP file.
  const QList<RecentDesign> recentList{recentDesign(top + QStringLiteral("/work/hex-key.mitcad")),
                                       recentDesign(top + QStringLiteral("/Shop/parts/gear.mitcad")),
                                       recentDesign(top + QStringLiteral("/work/gearbox.step"))};
  // A design of the other project, with its project and folder.
  const QVector<start::Card> hex = start::searchCards(projects, keys, recentList, QStringLiteral("HEX"), {});
  CHECK(keysOf(hex) ==
        (QStringList{QStringLiteral("Team/bolts/hex-bolt.mitcad"), QStringLiteral("hex-key.mitcad")}));
  CHECK(hex.size() == 2 && hex[0].location == QStringLiteral("Team / bolts") && hex[0].inProject);
  CHECK(hex.size() == 2 && !hex[1].inProject && hex[1].location == QStringLiteral("test / work"));
  // Every project, then the recent list; the gear in Shop once.
  const QVector<start::Card> gear = start::searchCards(projects, keys, recentList, QStringLiteral("gear"), {});
  CHECK(keysOf(gear) == (QStringList{QStringLiteral("Shop/parts/gear.mitcad"), QStringLiteral("Team/gear-rack.mitcad"),
                                     QStringLiteral("gearbox.step")}));
  // A name without its suffix; only spaces find nothing.
  CHECK(start::searchCards(projects, keys, recentList, QStringLiteral("mitcad"), {}).isEmpty());
  CHECK(start::searchCards(projects, keys, recentList, QStringLiteral("step"), {}).isEmpty());
  CHECK(start::searchCards(projects, keys, recentList, QStringLiteral("   "), {}).isEmpty());
  CHECK(start::searchCards(projects, keys, recentList, QStringLiteral("nothing"), {}).isEmpty());
  // The lines.
  CHECK(start::searchLine(hex) ==
        QStringLiteral("Start page search results: 2 (Team/bolts/hex-bolt.mitcad | hex-key.mitcad)"));
  CHECK(start::searchLine({}) == QStringLiteral("Start page search results: 0"));
  // Two designs of the same name told apart by their second line.
  ProjectRow other = projects[0];
  other.root = top + QStringLiteral("/Shop2");
  other.folderName = QStringLiteral("Shop2");
  other.name = QStringLiteral("Shop 2");
  QVector<ProjectRow> both = projects;
  both << other;
  both = browsableProjects(both);
  const QVector<start::Card> plates =
      start::searchCards(both, projectKeys(both), {}, QStringLiteral("plate"), {});
  CHECK(plates.size() == 2 && plates[0].name == plates[1].name && plates[0].location != plates[1].location);
}

void previewTests(const QDir& folder) {
  qputenv("MITCAD_THUMBNAIL_DIR", folder.filePath(QStringLiteral("thumbnails")).toUtf8());
  const QString path = folder.filePath(QStringLiteral("Shop/plate.mitcad"));
  CHECK(QDir().mkpath(QFileInfo(path).absolutePath()));
  QFile file(path);
  CHECK(file.open(QIODevice::WriteOnly));
  file.write("{}");
  file.close();
  start::Card card;
  card.path = QFileInfo(path).absoluteFilePath();
  card.inProject = true;
  // No preview yet: none found; the file's own time.
  start::findLocally(&card);
  CHECK(card.preview.isEmpty());
  CHECK(card.when.isValid());
  // A save's preview is found.
  const QString preview = recent::designPreviewPath(card.path);
  CHECK(QDir().mkpath(QFileInfo(preview).absolutePath()));
  QFile png(preview);
  CHECK(png.open(QIODevice::WriteOnly));
  png.write("png");
  png.close();
  start::findLocally(&card);
  CHECK(card.preview == preview);
  qunsetenv("MITCAD_THUMBNAIL_DIR");
}

} // namespace

int startBrowserTests() {
  // The settings of the test's own, as recent_designs.cpp has them.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadStartBrowserTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadStartBrowserTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  QSettings().clear();
  const QString top = QDir::fromNativeSeparators(QDir(directory.path()).filePath(QStringLiteral("test")));

  rowTests(top);
  placeTests(top);
  searchTests(top);
  previewTests(QDir(top));

  QSettings().clear();
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
  return failures;
}
