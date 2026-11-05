// SPDX-License-Identifier: MIT
// The Start tab's recent designs (mitcad#1258, files/RecentDesigns.hpp): a
// file opened or saved goes first with its time, at most ten are kept; a
// pinned design stays listed when it drops out of them and keeps its pin
// until it is unpinned or removed; a missing file is shown as such; All,
// Pinned and the search filter the list; the opened text, the folder and
// the type; each design's preview has a place of its own by its path in
// the previews' folder; Show start page at launch is on by default; and an
// earlier list without times reads with the files' own.
#include "files/RecentDesigns.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDate>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QTime>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "recent_designs.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

namespace recent = mitcad::recent;
using mitcad::RecentDesign;

// An empty file at `name` in `folder`.
QString touch(const QDir& folder, const QString& name) {
  const QString path = folder.filePath(name);
  QFile file(path);
  CHECK(file.open(QIODevice::WriteOnly));
  file.write("{}");
  return QFileInfo(path).absoluteFilePath();
}

QStringList names(const QList<RecentDesign>& designs) {
  QStringList list;
  for (const RecentDesign& design : designs) {
    list << design.name;
  }
  return list;
}

void listTests(const QDir& folder) {
  const QString a = touch(folder, QStringLiteral("alpha.mitcad"));
  const QString b = touch(folder, QStringLiteral("beta.step"));
  const QString c = touch(folder, QStringLiteral("gamma.mitcad"));
  const QDateTime now = QDateTime::currentDateTime();
  recent::noteOpened(a, now.addDays(-3));
  recent::noteOpened(b, now.addDays(-1));
  recent::noteOpened(c, now);
  QList<RecentDesign> designs = recent::load();
  CHECK((names(designs) == QStringList{QStringLiteral("gamma.mitcad"), QStringLiteral("beta.step"),
                                       QStringLiteral("alpha.mitcad")}));
  CHECK(designs.size() == 3);
  if (designs.size() == 3) {
    CHECK(designs.at(0).type == QStringLiteral("MITCAD"));
    CHECK(designs.at(1).type == QStringLiteral("STEP"));
    CHECK(!designs.at(0).missing && !designs.at(0).pinned);
    CHECK(designs.at(0).preview.isEmpty()); // none saved yet
    CHECK(recent::openedText(designs.at(0).opened, now) == QStringLiteral("Today"));
    CHECK(recent::openedText(designs.at(1).opened, now) == QStringLiteral("Yesterday"));
    CHECK(recent::openedText(designs.at(2).opened, now) == QStringLiteral("3 days ago"));
  }
  // Opened again: first, the others after it in their order.
  recent::noteOpened(a, now);
  CHECK((names(recent::load()) == QStringList{QStringLiteral("alpha.mitcad"), QStringLiteral("gamma.mitcad"),
                                              QStringLiteral("beta.step")}));

  // Pinned: in the Pinned filter, and still listed after ten others.
  recent::setPinned(b, true);
  designs = recent::load();
  CHECK((names(recent::filter(designs, true, QString())) == QStringList{QStringLiteral("beta.step")}));
  CHECK(recent::filter(designs, false, QString()).size() == 3);
  for (int i = 0; i < recent::kRecentCount; ++i) {
    recent::noteOpened(touch(folder, QStringLiteral("other%1.mitcad").arg(i)), now);
  }
  designs = recent::load();
  CHECK(designs.size() == recent::kRecentCount + 1);
  CHECK(!designs.isEmpty() && designs.constLast().name == QStringLiteral("beta.step") && designs.constLast().pinned);
  CHECK(!names(designs).contains(QStringLiteral("alpha.mitcad")));
  // Its time is kept with its pin.
  CHECK(!designs.isEmpty() && recent::openedText(designs.constLast().opened, now) == QStringLiteral("Yesterday"));

  // The search: names and folders, case-insensitive; with Pinned.
  CHECK((names(recent::filter(designs, false, QStringLiteral("OTHER3"))) == QStringList{QStringLiteral("other3.mitcad")}));
  CHECK(recent::filter(designs, true, QStringLiteral("other")).isEmpty());
  CHECK(recent::filter(designs, false, QStringLiteral("  ")).size() == designs.size());
  CHECK(recent::filter(designs, false, folder.dirName()).size() == designs.size());

  // Unpinned: gone from the list once it is no recent file.
  recent::setPinned(b, false);
  CHECK(!names(recent::load()).contains(QStringLiteral("beta.step")));

  // A missing file says so; Remove takes it off.
  const QString gone = touch(folder, QStringLiteral("gone.mitcad"));
  recent::noteOpened(gone, now);
  recent::setPinned(gone, true);
  CHECK(QFile::remove(gone));
  designs = recent::load();
  CHECK(!designs.isEmpty() && designs.constFirst().name == QStringLiteral("gone.mitcad") &&
        designs.constFirst().missing);
  recent::remove(gone);
  designs = recent::load();
  CHECK(!names(designs).contains(QStringLiteral("gone.mitcad")));
  CHECK(!QSettings().value(recent::pinnedKey()).toStringList().contains(gone));

  // Clear Recent Files: the pinned designs stay.
  const QString kept = touch(folder, QStringLiteral("kept.mitcad"));
  recent::noteOpened(kept, now);
  recent::setPinned(kept, true);
  recent::clearRecent();
  CHECK((names(recent::load()) == QStringList{QStringLiteral("kept.mitcad")}));
  recent::setPinned(kept, false);
  CHECK(recent::load().isEmpty());

  // An earlier list without times: the file's own.
  QSettings().setValue(recent::recentFilesKey(), QStringList{a});
  QSettings().remove(recent::openedKey());
  designs = recent::load();
  CHECK(designs.size() == 1 && designs.constFirst().opened.isValid());
}

void textTests() {
  const QDateTime now(QDate(2026, 11, 4), QTime(12, 0));
  CHECK(recent::openedText(QDateTime(), now).isEmpty());
  CHECK(recent::openedText(now.addSecs(-3600), now) == QStringLiteral("Today"));
  CHECK(recent::openedText(now.addDays(-6), now) == QStringLiteral("6 days ago"));
  CHECK(!recent::openedText(now.addDays(-30), now).contains(QStringLiteral("ago")));
  CHECK(recent::folderText(QStringLiteral("/home/user/Projects/Robot arm/base.mitcad")) ==
        QStringLiteral("Projects / Robot arm"));
  CHECK(recent::folderText(QStringLiteral("/a/designs/base.mitcad")) == QStringLiteral("a / designs"));
  CHECK(recent::typeText(QStringLiteral("/a/b.Step")) == QStringLiteral("STEP"));
  CHECK(recent::typeText(QStringLiteral("/a/b")) == QStringLiteral("FILE"));
}

void previewTests(const QDir& folder) {
  qputenv("MITCAD_THUMBNAIL_DIR", folder.filePath(QStringLiteral("thumbnails")).toUtf8());
  const QString design = touch(folder, QStringLiteral("shown.mitcad"));
  const QString preview = recent::designPreviewPath(design);
  CHECK(preview.startsWith(folder.filePath(QStringLiteral("thumbnails/recent/"))));
  CHECK(preview.endsWith(QStringLiteral(".png")));
  CHECK(preview == recent::designPreviewPath(design));
  CHECK(preview != recent::designPreviewPath(folder.filePath(QStringLiteral("other.mitcad"))));
  // A preview there shows on the design's card.
  CHECK(QDir().mkpath(QFileInfo(preview).absolutePath()));
  QFile file(preview);
  CHECK(file.open(QIODevice::WriteOnly));
  file.write("png");
  file.close();
  recent::noteOpened(design);
  const QList<RecentDesign> designs = recent::load();
  CHECK(!designs.isEmpty() && designs.constFirst().preview == preview);
  qunsetenv("MITCAD_THUMBNAIL_DIR");
}

void launchTests() {
  QSettings().remove(recent::showAtLaunchKey());
  CHECK(recent::showAtLaunch()); // on by default
  recent::setShowAtLaunch(false);
  CHECK(!recent::showAtLaunch());
  recent::setShowAtLaunch(true);
  CHECK(recent::showAtLaunch());
}

} // namespace

int recentDesignsTests() {
  // The settings of the test's own, as quick_access.cpp has them.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadRecentDesignsTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadRecentDesignsTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  QSettings().clear();
  const QDir folder(directory.path());

  listTests(folder);
  textTests();
  previewTests(folder);
  launchTests();

  QSettings().clear();
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
  return failures;
}
