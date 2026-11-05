// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "PrintItemsHost.hpp"
#include "PrintKeys.hpp"
#include "PrintLibrary.hpp"

class QCheckBox;
class QHBoxLayout;
class QLabel;
class QLineEdit;
class QPushButton;
class QTreeWidget;

namespace mitcad {

class DocumentHost;
class SettingsForm;
class SlicerImportBar;

// The list and editor of the Printers or the Processes manager (mitcad#655,
// mitcad#1135), laid out as Materials'; one kind each.
//
// On the left a search and the items grouped by where they live (the
// `library_items` query by place, queryPlaceItems): This design, This
// project (in a project only), My library and Built-in, each a heading
// with its count ("This design (2)"), the search across the groups. A
// design's copy of a library item and that item are two rows, each in its
// place. The design's printer or process is marked (bold, ●). In the
// Processes manager each place's processes are in sub-groups by printer
// (mitcad#1216: processPrinters, the model's `printers`), foldable as the
// places, by name, then Any printer; a process for several printers is
// under each, its row under the printer it was chosen under staying the
// selected one. Logged "Processes printers: My library: Bench X 2 | Any
// printer 1" and "Processes printer group: My library › Bench X: Fine,
// Shared"; each row under a printer also "Processes user › Bench X item
// Fine at x,y".
//
// On the right the selected item: its Name (F2 on a row), "Built-in" with
// a lock and Duplicate for a built-in one, else Copy to ▾ (the other
// places), then one line that says where it is edited and whom that
// reaches (editingLine: "Editing: this project's library — every design of
// the project uses it"), then the settings form of the printer set or the
// process set (a process's tabs Quality, Strength, Speed, Support,
// Others). Each field is saved at Enter or when it is left, in the item's
// own place: one `edit_print_profile` and undo step (a library item's
// file, with the design's copies of it as before; a design's copy alone).
// A copy whose library item another design changed is asked about once
// (askChangedElsewhere). Built-in ones are read only.
//
// Under the list: New and Duplicate, which ask where (choosePlaceAmong:
// This design when a design is open, else My library, first); a new one
// in the design is its printer or process at once, in one undo step;
// Delete (always asked, naming what uses it and the outcome); Use in this
// design for an item the design does not use.
//
// Hidden built-ins (mitcad#1270): a base library item has Hide beside
// Delete and in its row's context menu (Duplicate, Hide; the user's own
// Duplicate, Delete); a hidden one is listed only with Show hidden (the
// box under the list, shown while one is hidden), grayed, with Unhide.
// While My library or This project has one of the kind, the Built-in
// group is folded at each opening (unless its item is the one shown); a
// search shows every group's matches. Logged "Printers hidden: <name>",
// "Printers unhidden: <name>", "Printers show hidden: on", "Printers
// grayed: <names>", "Printers folded: Built-in" ("none"), and rows of
// hidden items end ", hidden".
//
// Without a design (ManagerHosts) the panel works through a model of its
// own: This design is no group and no place to ask for.
class ProfilesPanel : public QWidget {
  Q_OBJECT

public:
  // `kind`: printer or process. `logName` heads the panel's log lines:
  // "Printers", "Processes".
  ProfilesPanel(PrintItemsHost& host, const QString& kind, const QString& logName, QWidget* parent = nullptr);
  ~ProfilesPanel() override;

  // Another model to work through (a manager opened with or without a
  // design); nothing selected, read at once or with `read` false shown
  // empty until refresh (read off the UI thread, mitcad#1018).
  void setHost(PrintItemsHost& host, bool read = true);
  // Shows the item `key` ("<place>/<id>"); an empty key selects the
  // design's printer or process, none when it has none (Slicer default). A
  // library item the design has a copy of stands for that copy, what the
  // design prints with (the pickers' keys).
  void showItem(const QString& key);
  // The group of `place` ("user": My library; mitcad#1250) at the top of
  // the list, its first item selected (none when it has none), logged as
  // "Printers on My library: <name>" ("nothing selected").
  void showPlace(const QString& place);
  // The place New and Duplicate offer first while it is one of places()
  // (mitcad#1250: My library from Tools › Libraries); empty: This design
  // with a design open, else My library.
  void setHomePlace(const QString& place) { m_homePlace = place; }
  // Shows the items and the selected one as the model has them now.
  void refresh();
  // A button of the window at the end of the row under the list (Close),
  // its place logged as "<logName> <name> at x,y".
  void addButton(QPushButton* button, const QString& name);
  // The places an item of this panel can be made in now.
  QStringList places() const;
  QString kind() const { return m_kind; }

signals:
  // A command of the panel changed the model or a library.
  void changed();

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  const LibraryItem* current() const;
  bool hasDesign() const { return m_host->hasDesign(); }
  QString project() const { return m_host->projectFolder(); }
  void rebuildList();
  void showCurrent();
  void edit(const QJsonObject& change, const QString& what, const QString& field = QString());
  // Asks where a new item goes (New, Duplicate); empty when not chosen.
  QString askWhere(QPushButton* anchor, const QString& what);
  // Selects the item `id` made in `place` and gives its Name the keyboard.
  void selectMade(const QString& place, const QString& id);
  void newProfile();
  void duplicate(QPushButton* anchor);
  void copyTo();
  // Use in this design: the selected one the design's printer or process.
  void useInDesign();
  void remove();
  // Hide or Unhide of the selected base item (mitcad#1270).
  void toggleHidden();
  // A row's context menu (itemContextMenu): Duplicate, Hide or Delete.
  void itemMenu(const QPoint& at);
  // Whether the list shows `item` now: the search, and a hidden one only
  // with Show hidden.
  bool listed(const LibraryItem& item) const;
  // Asks once about the selected copy when another design changed its
  // library item.
  void askChanges();
  bool canChange();
  bool run(const QJsonObject& command, QJsonObject* result = nullptr);
  void setMessage(const QString& text, bool error = false);
  void logRows();
  void scheduleLogPlaces();
  void logPlaces();

  PrintItemsHost* m_host = nullptr;
  QString m_kind; // printer or process
  QString m_log;  // "Printers"
  QString m_homePlace; // setHomePlace's
  PrintKeyTable m_keys;
  QString m_dialect; // the design's slicer (designDialectOf)
  QVector<LibraryItem> m_items; // by place
  QStringList m_places;         // the groups, in their order
  QJsonArray m_profiles;
  QJsonObject m_library;
  QString m_currentKey; // the selected row's key, "<place>/<id>"
  QString m_currentPrinter; // the printer it was chosen under (mitcad#1216); empty: its first row
  QString m_pendingFocus; // a row made just now, whose Name takes the keyboard
  bool m_building = false;
  QLineEdit* m_search = nullptr;
  QTreeWidget* m_list = nullptr;
  QWidget* m_editor = nullptr;
  QLineEdit* m_name = nullptr;
  QWidget* m_builtIn = nullptr; // "Built-in" with its lock and Duplicate
  QPushButton* m_headerDuplicate = nullptr;
  QLabel* m_usedState = nullptr;  // "● Used in this design"
  QLabel* m_editing = nullptr;    // editingLine
  QLabel* m_emptyState = nullptr; // "No printer selected"
  QPushButton* m_copyTo = nullptr;
  SettingsForm* m_form = nullptr;
  QLabel* m_message = nullptr;
  QHBoxLayout* m_buttons = nullptr;
  QPushButton* m_new = nullptr;
  QPushButton* m_duplicate = nullptr;
  QPushButton* m_use = nullptr;
  QPushButton* m_delete = nullptr;
  QPushButton* m_hide = nullptr;      // Hide or Unhide of a base item (mitcad#1270)
  QCheckBox* m_showHidden = nullptr;  // Show hidden, under the list
  QSet<QString> m_folded;             // the places whose groups are folded
  bool m_startFold = false;           // an opening: Built-in folded when the user has their own
  QVector<std::pair<QString, QPushButton*>> m_windowButtons; // addButton's
  QString m_loggedRows;
  QString m_loggedValues;
  QString m_loggedEditing;
  QString m_loggedPlaces;
};

// The Printers or the Processes manager (mitcad#1135; mitcad#655 had one
// window for both): Make › 3D Print › Printers… and Processes…, Edit… of
// Property Sets' Printer and Process rows, and with no design open the
// libraries' own (ManagerHosts). A non-modal window that follows the
// model: Import from Slicer… at the top (SlicerImportBar, mitcad#829),
// the box before it saying where the items go (the default as New's), its line and Details… before
// it; the panel above; Close at the end of the row under the list. The
// item of its kind that an import brought shows selected. Logs "Printers
// opened", "Printers groups: This design 1 | My library 2 | Built-in 4",
// and on an opening without a design "Printers loaded: read <n> ms, list
// <n> ms" once the items were read off the UI thread.
class ProfilesDialog : public QDialog {
  Q_OBJECT

public:
  ProfilesDialog(DocumentHost& host, const QString& kind, QWidget* parent = nullptr);
  ~ProfilesDialog() override;

  // An opening: the model chosen (with or without a design), the item
  // `key` selected (ProfilesPanel::showItem).
  using QDialog::open;
  void open(const QString& key);
  // An opening on the group of `place` (mitcad#1250: "user", My library,
  // from Tools › Libraries › Printer and Process Library…):
  // ProfilesPanel::showPlace, and `place` the one New, Duplicate and Import
  // offer first until it is opened otherwise.
  void openPlace(const QString& place);
  void refresh();
  // Whether it works on the open design.
  bool onDesign() const { return m_hosts.onDesign(); }

signals:
  // Closed after it changed a library without a design: the other
  // managers read the libraries again.
  void libraryChanged();

protected:
  void hideEvent(QHideEvent* event) override;

private:
  // An opening that shows the panel with `show` once the model's items
  // can be read (at once with a design, else after readAhead).
  void openWith(const std::function<void()>& show);
  // The place the import offers first: the home place while writable,
  // else This design with a design open, else My library.
  QString homePlace() const;

  ManagerHosts m_hosts;
  QString m_kind;
  QString m_homePlace; // openPlace's
  QString m_log;
  SlicerImportBar* m_import = nullptr;
  ProfilesPanel* m_panel = nullptr;
  int m_reads = 0;        // openings read ahead: only the latest one's answers are shown
  bool m_changed = false; // a library changed without a design
};

} // namespace mitcad
