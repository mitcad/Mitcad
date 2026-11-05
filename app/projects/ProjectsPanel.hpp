// SPDX-License-Identifier: MIT
#pragma once

// The Projects panel (mitcad#145, #173): the projects, their folders and
// designs in one tree, project -> folders -> designs. A project row is its
// repository's root; nothing groups the projects, and below a project are
// only the folders and designs it holds:
//
//   github.com/acme/gripper     (cloud) ↑2
//     finger                     v4 (lock)
//   Robot arm
//     parts
//       bracket                  v3*
//     base                       v12
//
// A project row has its name (a display name set in Project Settings, else
// a Cloud project's repository address without the scheme, else the
// folder's name; sorted by it), an icon and a tooltip that tell Local from
// Cloud, and its sync state (as the project indicator shows it); a design
// row has its version, whether it changed since then and its edit lock;
// the open design is bold. A double-click on a Cloud project asks for its
// settings. The rows' keys (the logs, the expanded rows, the selection)
// start with the project's folder's name (its whole path when two listed
// projects have folders of the same name), which a display name does not
// change. The projects are the known ones (ProjectsSettings) and those of
// the projects folder; their trees come from the core's `project_tree` on
// a thread of their own (ProjectsTree).
//
// The panel only shows and asks: what to open, make, rename, move or
// delete goes to the window through the signals (MainWindow, in
// projects/MainWindowProjectsPanel.cpp), which runs the core's commands
// and calls refresh(). New Design, New Folder and Rename are named in place;
// a drag moves designs and folders within their project. The AppKit
// frontend presents the same rows natively (mitcad#816): its outline's
// keys, its own name editor and its drags come back through treeKey,
// setNativeEditor and dropRow to the same actions.
//
// The names get the width (mitcad#529): the state column holds compact
// marks (ProjectRowText.hpp: "v2*", a lock, a cloud with a check) with the
// full text in the tooltip, a name that does not fit is elided in the
// middle (its start and end kept; the tooltip has the full name and path),
// and the panel's width follows a drag of its edge: the card's grip in the
// Floating layout (GlassCard::setWidthAdjustable), the dock's edge in the
// Docked one (dockWidthChosen); the window remembers it.

#include <functional>

#include <QHash>
#include <QJsonObject>
#include <QPointer>
#include <QSet>
#include <QString>
#include <QVector>
#include <QWidget>

#include "../files/ProjectIndicator.hpp"
#include "ProjectsTree.hpp"

class QAction;
class QModelIndex;
class QTimer;
class QToolBar;
class QTreeWidgetItem;

namespace mitcad {

class ProjectsTreeWidget;
class RemoteTask;

class ProjectsPanel : public QWidget {
  Q_OBJECT

public:
  // What the panel asks the window.
  struct Host {
    // The edit locks' session of this run (whose locks are this session's).
    std::function<QString()> session;
    // The open design's file (absolute; empty when untitled).
    std::function<QString()> openDesign;
    // The current project's folder ("" none), and its remote's state as the
    // project indicator has it.
    std::function<QString()> currentRoot;
    std::function<ProjectIndicator::Remote()> currentRemote;
    // Where a name is typed is in sight (mitcad#1268): true before New
    // Design, New Folder or Rename opens its editor (the window expands the
    // Projects card, or shows the panel or its dock), false once the naming
    // has ended (the window puts the panel back as it was).
    std::function<void(bool)> showForNaming;
  };

  // The kinds of rows. Message: the refusal of a name typed, below the row
  // whose editor is open again (mitcad#1268).
  enum class RowType { Project = 1, Folder, Design, NewItem, Message };

  // The narrowest the Floating card can be dragged to (mitcad#529).
  static constexpr int kMinimumCardWidth = 180;

  explicit ProjectsPanel(Host host, QWidget* parent = nullptr);
  ~ProjectsPanel() override;

  // Reads the projects again (on a thread; requests while one runs make one
  // more read). Only while the panel is shown; showing it reads them.
  void refresh();
  // A toolkit-independent presenter may be visible in an AppKit host while
  // its QWidget remains hidden. It still needs project refreshes.
  void setNativePresented(bool shown);
  // The open design or the current project's remote changed: the marks
  // (bold, sync state) again, nothing read.
  void updateMarks();
  // Expands and selects a project (once it is read).
  void reveal(const QString& root);
  // Expands to and selects a design (`path` relative to the project at
  // `root`) once it is listed: a design moved or saved into a project from
  // outside the panel (mitcad#174).
  void revealDesign(const QString& root, const QString& path);
  // Names a new design or folder in place, in `folder` (relative, "" the
  // project's top) of the project at `root`; once named, newDesignRequested
  // or newFolderRequested. The name proposed (Design1, Folder1, ...) is
  // free among the folder's rows and on disk, picked once a read under way
  // has finished (mitcad#1268); the panel is shown for it (Host::
  // showForNaming).
  void startNewDesign(const QString& root, const QString& folder);
  void startNewFolder(const QString& root, const QString& folder);
  // Closes an open name editor without taking the name (Esc); false when
  // none is open.
  bool cancelEditing();
  // The window's command refused the name the panel's last request gave
  // (it is there already, or not a valid name; mitcad#1268): the editor
  // opens again on its row with the name selected and `message` shown
  // below the row, so that another name can be typed. `path` is the
  // request's (relative), `from` a rename's old path ("" a new design or
  // folder). False when the request was not the panel's last one (the
  // window tells the refusal itself then).
  bool nameRefused(const QString& root, const QString& path, const QString& from, const QString& message);

  // An AppKit presentation of the panel (mitcad#816) gives its rows the
  // same keys, names and drags as the tree, through these. The tree's own
  // keys on the current row: Return opens (a folder or project folds), F2
  // renames, Delete deletes (on macOS also Backspace); true when the key
  // is the tree's.
  bool treeKey(int key);
  // Its own name editor, shown in its row in place of the hidden tree's:
  // `openRow` starts it on the row of `index` with `text` (false: it could
  // not, the naming is cancelled), `closeRow` ends it without a result.
  void setNativeEditor(std::function<bool(const QModelIndex&, const QString&)> openRow,
                       std::function<void()> closeRow);
  // That editor ended: `text` taken (Return, Tab, the editor left) or not
  // (Esc), as the tree's editor's end.
  void finishNativeEditing(const QString& text, bool accepted);
  // Its row drags, with the tree's rules: a design or a folder moves onto a
  // folder, a project or a design (its folder) of the same project.
  // dropRow takes the drop as a release does, a refused one with its
  // feedback (`over` invalid: outside the rows); while `dragged` is
  // dragged, the rows read wait as for the tree's own drag.
  bool canDragRow(const QModelIndex& index) const;
  bool canDropRow(const QModelIndex& dragged, const QModelIndex& over) const;
  void dropRow(const QModelIndex& dragged, const QModelIndex& over);
  void setNativeDragging(const QModelIndex& dragged, bool dragging);

  // The selected row's project ("" none) and folder in it (relative: a
  // folder's own path, a design's folder, "" for the project's top).
  QString selectedRoot() const;
  QString selectedFolder() const;
  // A project as last read; null when it is not listed.
  const ProjectRow* project(const QString& root) const;
  // A project's sync state as its row shows it ("Cloud · ✓"; "" not listed).
  QString stateText(const QString& root) const;

  // The height of the toolbar and the rows in sight (the floating card is
  // as high as they, up to a share of the view).
  int contentHeight() const;
  QSize sizeHint() const override;
  // Logs where the rows and the toolbar's buttons are, for UI tests (when
  // it changed); the dock or card the panel is in moving logs them again.
  // With them (mitcad#529): "Projects width <w>", "Projects edge at x,y"
  // (the card's grip, the dock's edge), and per row, when it changed,
  // "Projects text <key>: "<the name as shown>" <marks>; tooltip: <the
  // tooltip's lines joined by " / ">".
  void logLayout();
  // The dock or card the panel is in.
  void watchContainer(QWidget* container);

signals:
  void openRequested(const QString& path);
  // `path` relative to the project at `root`.
  void newDesignRequested(const QString& root, const QString& path);
  void newFolderRequested(const QString& root, const QString& path);
  // A rename (in place) or a drag: a design or a folder moves.
  void moveRequested(const QString& root, const QString& from, const QString& to, bool folder);
  void deleteRequested(const QString& root, const QString& path, bool folder);
  void versionHistoryRequested(const QString& root, const QString& path);
  void showInFolderRequested(const QString& folder);
  void projectSettingsRequested(const QString& root);
  void syncRequested(const QString& root);
  void openProjectRequested(const QString& root);
  // A command of the window's (file.new_project, file.open_remote).
  void commandRequested(const QString& id);
  void addFolderRequested();
  void removeFromListRequested(const QString& root);
  // A message for the status bar (a name refused).
  void statusMessage(const QString& message, bool error);
  void contentHeightChanged();
  // The Docked panel's dock was resized by a drag of its edge (the left
  // button held), and the drag rests: its width (mitcad#529).
  void dockWidthChosen(int width);

protected:
  void showEvent(QShowEvent* event) override;
  // A name editor that goes out of sight (the card collapsed, the dock
  // hidden) is cancelled: what nobody sees is never taken (mitcad#1268).
  void hideEvent(QHideEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  bool m_nativePresented = false;
  friend class ProjectsTreeWidget;

  // A name being typed in place.
  struct Editing {
    RowType kind = RowType::NewItem; // NewItem: a new design or folder; else a rename
    bool folder = false;             // a folder (new or renamed)
    QString root;
    QString parent; // the folder it is in (relative)
    QString from;   // a rename: the path it had
    QTreeWidgetItem* item = nullptr;
  };
  struct Pending {
    bool folder = false;
    QString root;
    QString parent;
  };

  void startRead();
  void readFinished(RemoteTask* task);
  void rebuild();
  // A project's row at the top of the tree, with its folders and designs.
  QTreeWidgetItem* addProjectRow(const ProjectRow& row, const QString& projectKey);
  // The key of a project's row: its folder's name, the whole path when
  // another listed project's folder has the same name.
  QString projectKeyOf(const ProjectRow& row) const;
  void applyMarks();
  void logTree();
  void scheduleLayoutLog();
  void updateActions();

  // Rows and what they stand for.
  RowType typeOf(const QTreeWidgetItem* item) const;
  QString rootOf(const QTreeWidgetItem* item) const;
  QString pathOf(const QTreeWidgetItem* item) const;
  QString keyOf(const QTreeWidgetItem* item) const;
  QTreeWidgetItem* projectItem(const QString& root) const;
  QTreeWidgetItem* folderItem(const QString& root, const QString& folder) const;
  const ProjectDesignRow* designOf(const QTreeWidgetItem* item) const;
  // The folder a new design or folder goes in for a row.
  QString folderFor(const QTreeWidgetItem* item) const;
  // The project whose settings the settings button opens for a row: the
  // project of a project, folder or design row; "" none (or not a Local or
  // Cloud project).
  QString settingsRoot(const QTreeWidgetItem* item) const;
  // A double-click on a Cloud project: its settings asked for (true); else
  // false (the row expands).
  bool settingsOnDoubleClick(QTreeWidgetItem* item);

  void startNew(bool folder, const QString& root, const QString& parent);
  void startRename(QTreeWidgetItem* item);
  // Opens the name editor on the row once the event loop is back, then
  // logs `line` and the editor's state ("Projects panel: name editor on
  // <key>: "<text>", focused, all selected, in sight").
  void openEditor(QTreeWidgetItem* item, const QString& line);
  // The editor closed: the name taken (`accepted`) or not.
  void editingClosed(bool accepted);
  // The tree's editor closed, by any way (Return, Esc, the focus elsewhere,
  // another row made current); `hidden`: it was not in sight then. Handled
  // once the view is done with it.
  void treeEditorClosed(bool accepted, bool hidden);

  // Names typed in place (mitcad#1268): a free name for a new design or
  // folder in `parent` of the project at `root`, among `host`'s rows and
  // the folder's entries on disk.
  QString freeName(bool folder, const QString& root, const QString& parent, const QTreeWidgetItem* host) const;
  // A new design's or folder's row in `host`, named `name`.
  QTreeWidgetItem* addNewRow(QTreeWidgetItem* host, bool folder, const QString& parent, const QString& name);
  // The panel shown for naming (Host::showForNaming), and put back once no
  // name is typed or waits for the rows (after the event loop is back, so
  // that a refusal reopening the editor keeps it shown).
  void revealForNaming();
  void scheduleNamingEnd();
  // A refusal below `item`, or none.
  void showRefusal(QTreeWidgetItem* item, const QString& message);
  void clearRefusal();
  void logEditor(QTreeWidgetItem* item);
  // Rows read while a name was typed or a row dragged: shown now.
  void applyWaitingRows();

  // The actions on the current row.
  void openCurrent();
  void renameCurrent();
  void deleteCurrent();
  void contextMenu(const QPoint& at);
  // A row dragged onto another: the folder it moves to, or none.
  bool dropTarget(const QTreeWidgetItem* dragged, const QTreeWidgetItem* over, QString* folder) const;
  void dropped(QTreeWidgetItem* dragged, QTreeWidgetItem* over);

  Host m_host;
  ProjectsTreeWidget* m_tree = nullptr;
  QToolBar* m_toolbar = nullptr;
  QAction* m_newDesign = nullptr;
  QAction* m_newFolder = nullptr;
  QAction* m_delete = nullptr;
  QAction* m_history = nullptr;
  QAction* m_sync = nullptr;
  QAction* m_settings = nullptr;
  QAction* m_more = nullptr;

  QVector<ProjectRow> m_rows;
  QPointer<RemoteTask> m_task;
  bool m_readAgain = false;
  bool m_rowsWaiting = false; // read while a name was typed: rebuilt after it
  QVector<ProjectRow> m_waitingRows;
  bool m_building = false;
  QHash<QString, bool> m_expanded; // by row key, as the user left them
  QSet<QString> m_revealed;        // projects revealed (expanded by default)
  QString m_reveal;                // a project to select once read
  Pending m_pending;               // a new design or folder to name once read
  bool m_hasPending = false;
  Editing m_editing;
  bool m_isEditing = false;
  QHash<QString, QString> m_loggedTrees; // by project key
  QString m_loggedProjects;
  QString m_loggedNames;
  QString m_loggedTopRows;
  QString m_loggedLayout;
  QString m_loggedSelection;
  QHash<QString, QString> m_loggedTexts; // "Projects text" lines by row key
  bool m_layoutLogPending = false;
  QPointer<QWidget> m_container; // the dock or card
  QTimer* m_widthTimer = nullptr; // a drag of the dock's edge rests
  // A design to select once it is listed (revealDesign).
  QString m_revealDesignRoot;
  QString m_revealDesignPath;
  void selectRevealedDesign();
  // The AppKit presentation's name editor and drag (mitcad#816).
  std::function<bool(const QModelIndex&, const QString&)> m_openNativeEditor;
  std::function<void()> m_closeNativeEditor;
  bool m_nativeEditing = false;  // its editor is open
  bool m_nativeDragging = false; // it drags a row
  bool m_marksWaiting = false;   // marks changed under its editor
  // Rows read now wait (a name typed, a row dragged).
  bool rowsHeld() const;
  // Names typed in place (mitcad#1268): each editor opened counts, so that
  // an end that comes late is not taken for a later editor's.
  int m_editSerial = 0;
  bool m_namingShown = false;                 // the host showed the panel for it
  QTreeWidgetItem* m_messageItem = nullptr;   // a refusal's row
  struct Request {
    QString root;
    QString path;
    QString from; // a rename's
    bool folder = false;
  };
  Request m_request; // the panel's last name request, until it is answered
  bool m_hasRequest = false;
};

} // namespace mitcad
