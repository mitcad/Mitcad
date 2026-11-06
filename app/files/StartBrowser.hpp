// SPDX-License-Identifier: MIT
#pragma once

// The Start page's project browser (mitcad#1301, app/COMMANDS.md "The Start
// tab"; the page is files/StartPage): what its two columns and its one
// search show, on top of File › Open from Project's data
// (projects/ProjectBrowserData) and the recent list (files/RecentDesigns).
//
//   left:  Recent, Pinned, then the projects (Local and Cloud, those with
//          versions, by name) and Add project;
//   right: for Recent and Pinned the recent list's cards as before; for a
//          project its breadcrumb (project › folder › …), its sub-folders and
//          its designs as cards; while the search holds text the designs whose
//          name holds it, in every project and in the recent list.
//
// The row and folder shown are remembered for the next start (the local
// settings start/browseRow, start/browseRoot, start/browseFolder). A card's
// preview is the one a save writes (recent::designPreviewPath), looked for
// only on this computer: nothing is downloaded for it. Plain Qt Core, for
// app.unit.
//
// Keys, as the logs have them: a project design's card "<project>/<path>"
// (BrowserEntry::key), a recent design's that is in no listed project its
// file's name ("bracket.mitcad").

#include <QDateTime>
#include <QHash>
#include <QList>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../projects/ProjectBrowserData.hpp"
#include "RecentDesigns.hpp"

namespace mitcad::start {

// A row of the left column, with the folder shown in a project.
struct Row {
  enum class Kind { Recent, Pinned, Project };
  Kind kind = Kind::Recent;
  QString root;   // a project's folder (absolute, '/')
  QString folder; // the folder shown in it, relative ("" its top)

  bool operator==(const Row& other) const;
  bool operator!=(const Row& other) const { return !(*this == other); }
};

// The settings keys of the remembered row.
QString rowKey();    // "start/browseRow": "recent", "pinned" or "project"
QString rootKey();   // "start/browseRoot"
QString folderKey(); // "start/browseFolder"

// The row remembered (Recent when none), and remembering one.
Row loadRow();
void saveRow(const Row& row);

// The row to show for `row` among the listed projects: a project's row with
// the project's listed spelling, at its top when the folder is gone; Recent
// when the project is not listed. Recent and Pinned stay as they are.
Row shownRow(const Row& row, const QVector<ProjectRow>& projects);

// Whether two absolute paths are the same file or folder (case ignored on
// Windows).
bool samePath(const QString& a, const QString& b);

// A design's card on the right side.
struct Card {
  QString path;     // absolute, '/'
  QString name;     // the file's name, "gear.mitcad"
  QString type;     // "MITCAD": the suffix in capitals
  QString location; // the second line: "Shop / parts" (a project's design), "Projects / Robot arm" (recent)
  QString key;      // the logs' key (above)
  QDateTime when;   // a project design's last change, a recent design's last open (invalid: not known)
  bool pinned = false;
  bool missing = false;   // a recent design whose file is gone
  bool inProject = false; // a listed project's design
  QString preview;        // the preview's file, empty when none was found (yet)
};

// A recent design's card (the recent list as before).
Card recentCard(const RecentDesign& design);
// A project design's card; `pinned` the pinned paths (recent::pinnedPaths).
Card projectCard(const BrowserEntry& entry, const QStringList& pinned);
// A project design's absolute path.
QString designPath(const BrowserEntry& entry);
// "Shop / parts / sub": the project's name and the design's folders.
QString locationText(const BrowserEntry& entry);

// The cards of a place's designs (its folders left out), in its order.
QVector<Card> placeCards(const QVector<BrowserEntry>& entries, const QStringList& pinned);
// The folders of a place's contents.
QVector<BrowserEntry> placeFolders(const QVector<BrowserEntry>& entries);

// A card's preview and time looked for on this computer: the preview a save
// wrote, when there is one; the file's own time when none is known.
void findLocally(Card* card);

// The search: the designs whose name (the file's without its suffix) holds
// `text`, case ignored: every listed project's (searchDesigns: by project,
// then path), then the recent list's that are in none of them (in the
// list's order). Nothing for a text that is only spaces.
QVector<Card> searchCards(const QVector<ProjectRow>& projects, const QHash<QString, QString>& keys,
                          const QList<RecentDesign>& recent, const QString& text, const QStringList& pinned);

// The logs' lines: "Start page projects: Shop (local) | Team (cloud)"
// ("none"), "Start page place: Shop/parts/ (1 folders, 2 designs)", "Start
// page search results: 2 (Team/bolts/hex-bolt.mitcad | hex.mitcad)" ("0").
QString projectsLine(const QVector<ProjectRow>& projects);
QString placeLine(const QString& placeKey, const QVector<BrowserEntry>& entries);
QString searchLine(const QVector<Card>& cards);

} // namespace mitcad::start
