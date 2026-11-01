// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "PrintItemsHost.hpp"
#include "PrintKeys.hpp"
#include "PrintLibrary.hpp"

class QComboBox;
class QHBoxLayout;
class QLabel;
class QLineEdit;
class QPushButton;
class QTreeWidget;

namespace mitcad {

class DocumentHost;
class SettingsForm;
class SlicerImportBar;

// The printers' and processes' list and editor (mitcad#655), shared by
// the Printers and Processes window and My Library's Printers and
// Processes tabs (mitcad#704), laid out as Materials.
//
// On the left the items under Printers and Processes (the `library_items`
// query), the design's choice marked; no places, no filter: the window's
// one search is the settings form's. On the right the selected one with
// the values the design prints with: Name (its rename; F2 on a row),
// "Built-in" with a lock for a built-in one, Save to for the user's own
// that is in no library yet, and the settings form of the printer set or
// the process set. Each field is saved at Enter or when it is left: one
// `edit_print_profile` and undo step, with the item's library when it is
// saved in one (the library's file and the design's copy together; Undo
// restores both); a refusal shows under the field. A saved item another
// design changed is asked about once (askChangedElsewhere). Built-in ones
// are read only.
//
// Under the list, as in Materials: New (a printer or a process) and
// Duplicate (the user's own copy) make it in this design as the design's
// choice, in one undo step; Delete (always asked, naming what uses it and
// the outcome); Use in this design.
//
// In a library (setLibrary, My Library): the library's items of the
// kinds shown (setKinds), then the built-in ones under Built-in to
// duplicate; New (of the one kind shown, no menu) and Duplicate make the
// item in the library, edits and Delete change its file. No Save to, no
// Use in this design, no saved state.
class ProfilesPanel : public QWidget {
  Q_OBJECT

public:
  // `logName` heads the panel's list and place log lines: "Profiles",
  // "My Library printers".
  ProfilesPanel(PrintItemsHost& host, const QString& logName, QWidget* parent = nullptr);
  ~ProfilesPanel() override;

  // The kinds listed: printer and process (the default), or one of them.
  void setKinds(const QStringList& kinds);
  // The library the panel works in: `user` or `project` (My Library);
  // empty: the design (the default). Its items are read at once, or with
  // `read` false shown empty until refresh (My Library reads them off the
  // UI thread, mitcad#1018).
  void setLibrary(const QString& library, bool read = true);
  // Shows the item `key` ("<place>/<id>") of `kind` (printer or process);
  // an empty key selects nothing (3D Print's Slicer default).
  void showItem(const QString& kind, const QString& key);
  // Shows the items and the selected one as the model has them now.
  void refresh();
  // A button of the window at the end of the row under the list (Close),
  // its place logged as "<logName> <name> at x,y".
  void addButton(QPushButton* button, const QString& name);
  // A button of the window that is no result of it, after the list's
  // actions (New, Duplicate, Delete) with a gap (Edit Library…,
  // mitcad#1019), logged as addButton's.
  void addListButton(QPushButton* button, const QString& name);

signals:
  // A command of the panel changed the model (My Library: a library).
  void changed();

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  const LibraryItem* current() const;
  bool inLibrary() const { return !m_libraryPlace.isEmpty(); }
  // The project commands and queries name: none in My library.
  QString itemsProject() const;
  // A row's key in the list: "<kind>|<place>/<id>".
  static QString rowKey(const LibraryItem& item);
  static QString kindTitle(const QString& kind);
  void rebuildList();
  void showCurrent();
  void selectKey(const QString& key);
  void edit(const QJsonObject& change, const QString& what, const QString& field = QString());
  // A new item's `library` and project in My Library, else `choose`.
  void inPlace(QJsonObject& command) const;
  void newProfile();
  void duplicate();
  // Use in this design: the selected one the design's printer or process.
  void useInDesign();
  void remove();
  void saveTo();
  // Asks once about the selected item when another design changed it.
  void askChanges();
  bool canChange();
  bool run(const QJsonObject& command, QJsonObject* result = nullptr);
  void setMessage(const QString& text, bool error = false);
  void logRows();
  void scheduleLogPlaces();
  void logPlaces();

  PrintItemsHost& m_host;
  QString m_log;          // "Profiles"
  QString m_libraryPlace; // user or project in My Library; empty: the design
  QStringList m_kinds;    // printer, process
  PrintKeyTable m_keys;
  QString m_dialect; // the design's slicer (designDialectOf)
  QVector<LibraryItem> m_items; // printers, then processes
  QJsonArray m_profiles;
  QJsonObject m_library;
  QString m_currentKey;      // rowKey of the selected row
  QString m_currentDesignId; // its design copy, which a row keeps when its place changes
  QString m_currentKind;
  QString m_pendingFocus; // a row made just now, whose Name takes the keyboard
  bool m_building = false;
  QTreeWidget* m_list = nullptr;
  QWidget* m_editor = nullptr;
  QLineEdit* m_name = nullptr;
  QWidget* m_builtIn = nullptr; // "Built-in" with its lock and Duplicate
  QPushButton* m_headerDuplicate = nullptr;
  QLabel* m_saved = nullptr; // "In My library", a saved item's state
  QLabel* m_usedState = nullptr; // "● Used in this design"
  QLabel* m_emptyState = nullptr; // "No printer or process selected"
  QPushButton* m_saveTo = nullptr;
  SettingsForm* m_form = nullptr;
  QLabel* m_message = nullptr;
  QHBoxLayout* m_buttons = nullptr;
  QPushButton* m_new = nullptr;
  QPushButton* m_duplicate = nullptr;
  QPushButton* m_use = nullptr;
  QPushButton* m_delete = nullptr;
  QVector<std::pair<QString, QPushButton*>> m_windowButtons; // addButton's
  QString m_loggedRows;
  QString m_loggedValues;
  QString m_loggedPlaces;
};

// Printers and Processes (mitcad#655): every printer and process the user
// can choose, the panel above on the open design. Edit Printers and
// Processes (MAKE, next to 3D Print) and Edit… of 3D Print's and Property
// Sets' Printer and Process rows open it; a non-modal window that follows
// the model, with Edit Library… (My Library, mitcad#704) after the list's
// actions and Close on the right (mitcad#1019). At the
// top Import from Slicer… (mitcad#829, SlicerImportBar: 3D Print's import
// into the design), its line and Details… before it; the items read show
// selected in the list.
class ProfilesDialog : public QDialog {
  Q_OBJECT

public:
  ProfilesDialog(DocumentHost& host, QWidget* parent = nullptr);
  ~ProfilesDialog() override;

  void showItem(const QString& kind, const QString& key);
  void refresh();
  // Edit Library…: My Library (mitcad#704).
  std::function<void()> openMyLibrary;

private:
  DesignItemsHost m_designHost;
  SlicerImportBar* m_import = nullptr; // Import from Slicer… (mitcad#829)
  ProfilesPanel* m_panel = nullptr;
};

} // namespace mitcad
