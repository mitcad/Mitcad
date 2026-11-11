// SPDX-License-Identifier: MIT
#pragma once

// The Start page (mitcad#1258, GitHub mitcad/Mitcad#125; app/COMMANDS.md
// "The Start tab"): the content of the ribbon's START tab, which comes
// before SOLID, BUILD and MAKE and is there at any time. It is a child of
// the 3D view and covers all of it, as a drawing's workspace does
// (drawings/DrawingWorkspace.hpp), so it shows the same in the Floating and
// the Docked layout and in the AppKit frontend, whose ribbon shows the
// retained ribbon's tabs; its content starts below the rows that float over
// the view's top (OcctViewer::overlayInsets). While it shows, the window
// hides the cards over the view (MainWindowStart.cpp). Its backdrop is a
// rounded surface inset from the view's edges as the floating cards are,
// with the view's own background around it (StartBackdrop.hpp,
// mitcad#1439).
//
// From the top: "Welcome to MitCad" with What's New in X.Y (the version's
// release page in the browser: nothing is fetched at start-up); the quick
// actions New Design (in the default units for new designs), Open
// Design…, Open Project… and Import File…, each with its key; the designs'
// browser (mitcad#1301, StartBrowser.hpp): one search over every project
// and the recent list, the cards or the compact list, and two columns:
// left Recent and Pinned (the recent list, recent::load, all or the pinned
// ones), the Projects panel's projects with their state icons and Add
// project (New Project), the row shown highlighted; right the recent
// designs' cards, or a project's breadcrumb (project › folder › …, each
// part goes there), its sub-folders as buttons and its designs as cards,
// or the search's results with their project and folder. A card has the
// preview a save writes (else the type's icon), the file's name, type,
// folder (a project's design: its project and folders) and when it was
// opened (changed), a star that pins it; a missing file is shown as such
// with Remove. The projects are read on a thread of their own as Open from
// Project reads them (readProjects, `project_tree`): no network, nothing
// downloaded. The row and folder shown are remembered for the next start.
// Then a drop area (a CAD file dropped anywhere on the page opens or is
// imported); Resources: Component Libraries, User Guide, Keyboard
// Shortcuts and Connect AI Assistant, each running the command that is
// there today; and at the bottom Show start page at launch (on by
// default), Preferences… and Report an Issue (the issue list).
//
// Logs for UI tests (window coordinates, only what changed): "Start page
// action New Design at x,y" (Open Design, Open Project, Import File),
// "Start page filter Recent at x,y" (Pinned), "Start page project <name> at
// x,y", "Start page add project at x,y", "Start page crumb <place key> at
// x,y", "Start page folder <folder key> at x,y", "Start page search at
// x,y", "Start page view Cards at x,y" (List), "Start page card <key> at
// x,y", "Start page star <key> at x,y", "Start page remove <key> at x,y"
// (a card's key: StartBrowser.hpp), "Start page resource User Guide at
// x,y", "Start page link Preferences at x,y" (Report an Issue, What's
// New), "Start page show at launch at x,y"; "Start page recent designs
// (all): a.mitcad (preview) | b.step (no preview) | c.mitcad (missing)"
// ("(pinned)", "none" when empty), "Start page projects: Shop (local) |
// Team (cloud)", "Start page place: Shop/parts/ (1 folders, 2 designs)",
// "Start page search results: 1 (Team/bolts/hex-bolt.mitcad)" with "Start
// page search result: <key> (Team / bolts)" for each, "Start page cards:
// <key> (preview) | <key> (no preview)" (a project's or a search's cards
// once their previews were looked for), "Start page new design units: mm",
// "Start page shown", "Start page hidden", "Start page chosen: <action>",
// "Start page filter: all|pinned" (each choice logs the list again),
// "Start page pinned: <key>", "Start page unpinned: <key>", "Start page
// removed: <key>", "Start page view: cards|list", "Start page dropped:
// <names>" and "Start page at launch: on|off".

#include <functional>

#include <QHash>
#include <QJsonObject>
#include <QList>
#include <QPair>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "../projects/ProjectRowText.hpp"
#include "RecentDesigns.hpp"
#include "StartBrowser.hpp"

class QAbstractButton;
class QCheckBox;
class QDragEnterEvent;
class QDragLeaveEvent;
class QDragMoveEvent;
class QDropEvent;
class QGridLayout;
class QHBoxLayout;
class QLabel;
class QLineEdit;
class QMimeData;
class QPoint;
class QScrollArea;
class QToolButton;
class QVBoxLayout;

namespace mitcad {

class OcctViewer;
class RemoteTask;

class StartPage : public QWidget {
  Q_OBJECT

public:
  // What the page asks the window to do.
  struct Host {
    // Runs a command of the window by its id ("file.new_design").
    std::function<void(const QString& id)> trigger;
    // A command's key as the menus show it ("Ctrl+N"), empty: none.
    std::function<QString(const QString& id)> shortcutText;
    // Opens a recent design or a project's as File › Open Recent does (it
    // joins the recent list).
    std::function<void(const QString& path)> open;
    // Whether MitCad opens or imports a dropped file, and the drop itself
    // (as on the window: a design opens, other files come in).
    std::function<bool(const QString& path)> canOpen;
    std::function<void(const QStringList& paths)> drop;
    // The units of a new design (Preferences › General).
    std::function<QString()> defaultUnits;
    // An address in the browser (logged instead in UI tests).
    std::function<void(const QString& url)> openUrl;
    // The workspace tabs: the page's content starts below them where they
    // float over the view (null: below the view's floating rows).
    std::function<QWidget*()> tabBar;
    // A bar that floats over the view's bottom (the Floating layout's
    // message band): the page's footer stays above it while it shows.
    std::function<QWidget*()> bottomBar;
    // The room another toolkit's band keeps over the view's bottom, in the
    // view's points (the AppKit message band, mitcad#1358): the footer stays
    // above it too; null: none.
    std::function<int()> bottomInset;
    // The edit locks' session of this run, for reading the projects (whose
    // locks are this window's).
    std::function<QString()> session;
    // A project's marks as the Projects panel shows them (its state icon,
    // the words for its tooltip); null: Local or Cloud only.
    std::function<RowMarks(const ProjectRow& row)> projectMarks;
    QString version;    // "0.43.0"
    QString issuesUrl;  // the issue list
    QString releaseUrl; // this version's release page
  };

  StartPage(OcctViewer* viewer, Host host);

  // Shows it over the view with the list read again, or hides it.
  void showPage();
  void hidePage();
  // The list read again (a design opened, saved or pinned elsewhere).
  void refresh();
  // Logs the places of its controls again when they changed.
  void logPlaces();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void changeEvent(QEvent* event) override;
  void dragEnterEvent(QDragEnterEvent* event) override;
  void dragMoveEvent(QDragMoveEvent* event) override;
  void dragLeaveEvent(QDragLeaveEvent* event) override;
  void dropEvent(QDropEvent* event) override;

private:
  // Its place: the whole view, its rounded backdrop and its content below
  // the floating rows and the workspace tabs, its footer above the message
  // band.
  void place();
  // place() once the event loop is back (the tabs or the band moved).
  void placeSoon();
  // The right side for the row shown, or the search's results.
  void rebuildList();
  // The row shown (a click on it, a crumb, a folder); remembered for the
  // next start. A row chosen ends the search.
  void selectRow(const start::Row& row);
  void setCompact(bool compact);
  void togglePin(const start::Card& card);
  void removeDesign(const start::Card& card);
  void showCardMenu(const start::Card& card, const QPoint& global);
  // The projects read again on a thread (readProjects), and their answer.
  void loadProjects();
  void projectsRead(const QJsonObject& answer);
  // The left column's project rows, and their highlight.
  void rebuildRows();
  void markRows();
  // The cards of `cards` in the grid (the empty text when there are none).
  void showCards(const QVector<start::Card>& cards);
  // The breadcrumb and the folder buttons of a project's place (none
  // otherwise).
  void rebuildPlace(const ProjectRow* project, const QVector<BrowserEntry>& folders);
  // The cards' and the folders' columns for the right side's width.
  void layoutGrids();
  // The previews of the cards shown, looked for on this computer.
  void findPreviews();
  // The droppable files of a drag.
  QStringList droppedPaths(const QMimeData* mime) const;
  void setDropActive(bool active);
  void choose(const QString& what, const std::function<void()>& run);

  OcctViewer* m_viewer;
  Host m_host;
  QVBoxLayout* m_column = nullptr;   // the page's margins follow the view's insets
  QScrollArea* m_scroll = nullptr;
  QAbstractButton* m_newDesign = nullptr;
  QAbstractButton* m_whatsNew = nullptr;
  QAbstractButton* m_recentRow = nullptr;
  QAbstractButton* m_pinnedRow = nullptr;
  QAbstractButton* m_addProject = nullptr;
  QVBoxLayout* m_projectRowsLayout = nullptr;
  QLabel* m_projectsNote = nullptr; // reading, or no projects
  QList<QPointer<QAbstractButton>> m_projectRows; // by the projects' order
  QToolButton* m_cardsView = nullptr;
  QToolButton* m_listView = nullptr;
  QLineEdit* m_search = nullptr;
  QWidget* m_right = nullptr;        // the right side
  QLabel* m_title = nullptr;         // Recent designs, Pinned designs, Search results
  QWidget* m_crumbs = nullptr;
  QHBoxLayout* m_crumbLayout = nullptr;
  QList<QPointer<QWidget>> m_crumbWidgets;
  QList<QPair<QPointer<QAbstractButton>, QString>> m_crumbButtons; // with their place keys
  QWidget* m_folders = nullptr;
  QGridLayout* m_folderGrid = nullptr;
  QList<QPair<QPointer<QAbstractButton>, QString>> m_folderButtons; // with their folder keys
  QWidget* m_cards = nullptr;        // the cards' or the rows' container
  QGridLayout* m_cardGrid = nullptr;
  QLabel* m_empty = nullptr;         // no designs, or none match
  QWidget* m_dropArea = nullptr;
  QCheckBox* m_atLaunch = nullptr;
  QList<QPointer<QWidget>> m_cardWidgets;
  int m_cardColumns = 2;
  int m_folderColumns = 4;
  QList<RecentDesign> m_designs; // all, as last read
  start::Row m_row;              // the row shown
  bool m_compact = false;
  // The projects as last read (browsableProjects), their keys by root.
  QVector<ProjectRow> m_projects;
  QHash<QString, QString> m_keys;
  bool m_projectsRead = false;
  QPointer<RemoteTask> m_reading;
  bool m_readAgain = false;
  QString m_loggedList;
  QString m_loggedProjects;
  QString m_loggedPlace;
  QString m_loggedSearch;
  QString m_loggedCards;
  bool m_previewsQueued = false;
  QStringList m_loggedPlaces;
  bool m_placesQueued = false;
  bool m_placeQueued = false;
  // The tabs and the band whose shows, hides, moves and resizes place it
  // again (eventFilter).
  QList<QPointer<QWidget>> m_bars;
  // The rounded backdrop's room at the top (below the rows over the view)
  // and at the bottom (the band over it), as place() found them
  // (StartBackdrop.hpp, mitcad#1439).
  int m_backdropTop = 0;
  int m_backdropBelow = 0;
};

} // namespace mitcad
