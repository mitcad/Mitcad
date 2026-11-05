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
// hides the cards over the view (MainWindowStart.cpp).
//
// From the top: "Welcome to MitCad" with What's New in X.Y (the version's
// release page in the browser: nothing is fetched at start-up); the quick
// actions New Design (in the default units for new designs), Open
// Design…, Open Project… and Import File…, each with its key; Recent
// designs (recent::load): cards with the preview a save writes, the file's
// name, type, folder and when it was opened, a star that pins it, All or
// Pinned, a search field and a compact list instead of the cards; a missing
// file is shown as such with Remove; a drop area (a CAD file dropped
// anywhere on the page opens or is imported); Resources: Component
// Libraries, User Guide, Keyboard Shortcuts and Connect AI Assistant, each
// running the command that is there today; and at the bottom Show start
// page at launch (on by default), Preferences… and Report an Issue (the
// issue list).
//
// Logs for UI tests (window coordinates, only what changed): "Start page
// action New Design at x,y" (Open Design, Open Project, Import File),
// "Start page filter All at x,y" (Pinned), "Start page search at x,y",
// "Start page view Cards at x,y" (List), "Start page card <name> at x,y",
// "Start page star <name> at x,y", "Start page remove <name> at x,y",
// "Start page resource User Guide at x,y", "Start page link Preferences at
// x,y" (Report an Issue, What's New), "Start page show at launch at x,y";
// "Start page recent designs (all): a.mitcad (preview) | b.step (no
// preview) | c.mitcad (missing)" ("(pinned, "text")" when filtered, "none"
// when empty), "Start page new design units: mm", "Start page shown",
// "Start page hidden", "Start page chosen: <action>", "Start page filter:
// all|pinned" (each choice logs the list again), "Start page pinned:
// <name>", "Start page unpinned: <name>", "Start page removed: <name>",
// "Start page view: cards|list", "Start page dropped: <names>" and "Start
// page at launch: on|off".

#include <functional>

#include <QList>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QWidget>

#include "RecentDesigns.hpp"

class QAbstractButton;
class QCheckBox;
class QDragEnterEvent;
class QDragLeaveEvent;
class QDragMoveEvent;
class QDropEvent;
class QGridLayout;
class QLabel;
class QLineEdit;
class QMimeData;
class QPoint;
class QScrollArea;
class QToolButton;
class QVBoxLayout;

namespace mitcad {

class OcctViewer;

class StartPage : public QWidget {
  Q_OBJECT

public:
  // What the page asks the window to do.
  struct Host {
    // Runs a command of the window by its id ("file.new_design").
    std::function<void(const QString& id)> trigger;
    // A command's key as the menus show it ("Ctrl+N"), empty: none.
    std::function<QString(const QString& id)> shortcutText;
    // Opens a recent design as File › Open Recent does.
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
  // Its place: the whole view, its content below the floating rows and the
  // workspace tabs, its footer above the message band.
  void place();
  // place() once the event loop is back (the tabs or the band moved).
  void placeSoon();
  // The cards (or the list's rows) of the designs shown.
  void rebuildList();
  void setFilterPinned(bool pinned);
  void setCompact(bool compact);
  void togglePin(const RecentDesign& design);
  void removeDesign(const RecentDesign& design);
  void showCardMenu(const RecentDesign& design, const QPoint& global);
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
  QToolButton* m_all = nullptr;
  QToolButton* m_pinned = nullptr;
  QToolButton* m_cardsView = nullptr;
  QToolButton* m_listView = nullptr;
  QLineEdit* m_search = nullptr;
  QWidget* m_cards = nullptr;        // the cards' or the rows' container
  QGridLayout* m_cardGrid = nullptr;
  QLabel* m_empty = nullptr;         // no designs, or none match
  QWidget* m_dropArea = nullptr;
  QCheckBox* m_atLaunch = nullptr;
  QList<QPointer<QWidget>> m_cardWidgets;
  QList<RecentDesign> m_designs; // all, as last read
  bool m_pinnedOnly = false;
  bool m_compact = false;
  QString m_loggedList;
  QStringList m_loggedPlaces;
  bool m_placesQueued = false;
  bool m_placeQueued = false;
  // The tabs and the band whose shows, hides, moves and resizes place it
  // again (eventFilter).
  QList<QPointer<QWidget>> m_bars;
};

} // namespace mitcad
