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

#include "../framework/Appearances.hpp"
#include "PrintItemsHost.hpp"
#include "PrintKeys.hpp"
#include "PrintLibrary.hpp"

class QAction;
class QComboBox;
class QHBoxLayout;
class QLabel;
class QLineEdit;
class QPoint;
class QPushButton;
class QSplitter;
class QToolButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

class AppearancePopover;
class DocumentHost;
class MaterialVariantsPage;
class SettingsForm;

// The materials' list and editor (mitcad#655), shared by the Materials
// window and My Library's Materials tab (mitcad#704). On the left the
// materials the user can choose (the `library_items` query, one row with
// the design's copy), grouped by their class in Metals, Plastics (the
// filaments too) and Other, each with its appearance's swatch; no places:
// an item is built-in or the user's own. It manages the materials and
// assigns none: the Material tool assigns (mitcad#1059).
//
// On the right the selected material with the values the design prints
// with: its name as a title, "Built-in" with a lock for a built-in one,
// and Save to for the user's own that is in no library yet (My library,
// in a project a menu with Project), then the tabs: General (Name, Class,
// Density in g/cm³, Appearances) and, for a plastic only, the print
// settings' (Filament, Cooling, ...; the window's one search is theirs).
// The Filament tab's Slicer presets name, per slicer, the slicer's preset
// the material builds on (mitcad#1062: `filament_settings_id`, which the
// 3MF export writes with the preset it inherits from), each an editable
// drop-down of that slicer's installed filament presets.
// The material's appearances (mitcad#1055, mitcad#1083): General's
// Appearances is one list of every appearance it offers, each row its
// swatch and name (never the id), the default marked "Default" in its
// row, as tall as its rows; beside it Add… (the appearance library's
// popover, AppearancePicker.hpp), Remove and Make Default, the last two
// also in a row's context menu (Delete removes too). A filament's colour
// is its appearance: the print settings have no colour field, the export
// writes the colour of a body's appearance.
// The list of materials is shallow: the groups' headings flush left,
// their materials one small step in; it is as wide as its names need
// (at most under half the window) until the user moves the divider, and
// a name is elided only when it is narrowed, its full name the tooltip.
// Each field is saved at Enter or when it is left: one model
// command and undo step, `edit_material` with the item's library when it
// is saved in one (the library's file and the design's copy together;
// Undo restores both). A saved item another design changed is asked
// about once (askChangedElsewhere: Use the changes, Keep these). Built-in
// ones are read only: Duplicate makes the user's own copy. New asks the
// class (Metal, Plastic, Other); New and Duplicate make the material in
// this design and select it. Delete removes the item (a saved one from
// its library, the design keeping its copy), always asked
// (confirmDelete). A refusal of a setting shows under its field, others
// on the message line. Read only is disabled, never hidden.
//
// In a library (setLibrary, My Library): the library's items by class,
// then the built-in ones under Built-in to duplicate; New and Duplicate
// make the item in the library, edits and Delete change its file. No Save
// to, no saved state (every item is the library's).
class MaterialsPanel : public QWidget {
  Q_OBJECT

public:
  // `logName` heads the panel's log lines: "Materials", "My Library
  // materials".
  MaterialsPanel(PrintItemsHost& host, const QString& logName, QWidget* parent = nullptr);
  ~MaterialsPanel() override;

  // The library the panel works in: `user` or `project` (My Library);
  // empty: the design (the default). Its items are read at once, or with
  // `read` false shown empty until refresh (My Library reads them off the
  // UI thread, mitcad#1018).
  void setLibrary(const QString& library, bool read = true);
  // Shows the materials and the selected one as the model has them now.
  void refresh();
  // Shows the materials with the item `key` ("<place>/<id>") selected and
  // in sight (My Library after an import), logged as "Material selected:
  // <name> (<place>)".
  void showItem(const QString& key);
  // A button of the window at the end of the row under the list (Close),
  // its place logged as "<logName> <name> at x,y".
  void addButton(QPushButton* button, const QString& name);
  // A button of the window that is no result of it, after the list's
  // actions (New, Duplicate, Delete) with a gap (Edit Library…,
  // mitcad#1019), logged as addButton's.
  void addListButton(QPushButton* button, const QString& name);
  // Edit Appearances… of the Appearance box (the controller's window);
  // none: the box has no such entry.
  std::function<void()> editAppearances;

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
  void rebuildList();
  void showCurrent();
  void selectKey(const QString& key);
  // Runs `change` (fields of edit_material) on the selected item, later.
  void edit(const QJsonObject& change, const QString& what, const QString& field = QString());
  // A new item's `library` and project in My Library.
  void inPlace(QJsonObject& command) const;
  void newMaterial();
  void duplicate();
  void remove();
  void saveTo();
  // Asks once about the selected item when another design changed it.
  void askChanges();
  // One of the material's appearances made its default (mitcad#1055).
  void makeDefaultAppearance(const QString& id);
  // The appearances the selected material offers, its default first.
  QStringList offeredAppearances(const QJsonObject& fields) const;
  // An appearance's name; one not known here (a library material's
  // carried one) in words, never its id.
  QString appearanceName(const QString& id) const;
  // The rows of its list and the buttons.
  void showAppearances(const QJsonObject& fields, bool editable);
  // The appearance list as tall as its rows and as wide as its names.
  void fitAppearanceList();
  // A row's context menu: Make Default, Remove.
  void appearanceMenu(const QPoint& at);
  void addAppearance(const QString& id);
  void removeAppearance();
  void setDefaultAppearance();
  // The selected row's appearance id; empty for none.
  QString selectedAppearance() const;
  // The width the list of materials needs to show every name in full.
  int listWidthNeeded() const;
  // The list that wide, until the user moves the divider (mitcad#1083).
  void fitListWidth();
  // The names the list cuts at its width now.
  QStringList cutNames() const;
  // The names a slicer's preset field offers (mitcad#1062): the filament
  // presets of that slicer installed here, then the names the design's
  // materials give it.
  QStringList presetChoices(const QString& key, const QString& slicer);
  // The model can change now; else says why on the message line.
  bool canChange();
  bool run(const QJsonObject& command, QJsonObject* result = nullptr);
  void setMessage(const QString& text, bool error = false);
  void logRows();
  void scheduleLogPlaces();
  void logPlaces();

  PrintItemsHost& m_host;
  QString m_log;          // "Materials"
  QString m_libraryPlace; // user or project in My Library; empty: the design
  PrintKeyTable m_keys;
  QString m_dialect; // the design's slicer (designDialectOf)
  QVector<LibraryItem> m_items;
  QJsonArray m_designMaterials;
  QJsonObject m_library;
  QVector<Appearance> m_appearances;
  QJsonArray m_bodies;
  QString m_currentKey;      // the selected row's key; empty for none
  QString m_currentDesignId; // its design copy, which a row keeps when its place changes
  QString m_pendingFocus;    // a row made just now, whose Name takes the keyboard
  bool m_pendingDensity = false; // ... its Density instead (a new material of no typical one)
  bool m_building = false;
  QTreeWidget* m_list = nullptr;
  QSplitter* m_splitter = nullptr;  // the list and the editor
  bool m_listWidthByUser = false;   // the user moved the divider: no fitListWidth
  QString m_loggedNames;            // the names and the cut ones, as logged
  QWidget* m_editor = nullptr;
  QLineEdit* m_name = nullptr;
  QComboBox* m_class = nullptr;
  QWidget* m_builtIn = nullptr; // "Built-in" with its lock and Duplicate
  QPushButton* m_headerDuplicate = nullptr;
  QLabel* m_saved = nullptr; // "In My library", a saved item's state
  QLabel* m_emptyState = nullptr;  // "No material selected"
  QLabel* m_usedState = nullptr; // "● Used in this design"
  QStringList m_usedIds;         // the design's materials its bodies use
  QPushButton* m_saveTo = nullptr;
  QLineEdit* m_density = nullptr;
  QLabel* m_densityError = nullptr;
  QTreeWidget* m_appearanceList = nullptr; // every one it offers, the default marked
  QString m_selectAfterEdit;               // Add…'s, or the new default after Remove, selected once listed
  QString m_loggedSelection;               // the selected row's id, as logged
  QPushButton* m_addAppearance = nullptr;
  QPushButton* m_removeAppearance = nullptr;
  QPushButton* m_defaultAppearance = nullptr;
  AppearancePopover* m_addPopover = nullptr; // Add…'s
  QString m_loggedAppearances;
  QString m_loggedClass; // the shown item's class as logged (mitcad#1102)
  SettingsForm* m_form = nullptr;
  MaterialVariantsPage* m_variants = nullptr; // the Variants tab (mitcad#1054)
  QLabel* m_message = nullptr;
  QHBoxLayout* m_buttons = nullptr;
  QPushButton* m_new = nullptr;
  QPushButton* m_duplicate = nullptr;
  QPushButton* m_delete = nullptr;
  QVector<std::pair<QString, QPushButton*>> m_windowButtons; // addButton's
  QString m_loggedRows;
  QString m_loggedPlaces;
};

// What the context menus' Material ▸ gives bodies (mitcad#655,
// mitcad#1046): `set_body_material` for each of `bodies`
// (uids) naming `item` (null: none of their own), a library's item with
// its `library` and `project`, so that the design gets a copy of it in the
// same undo step. The material's appearance replaces the bodies' unless a
// body's was set by hand (`bodyRows`, rows of the `bodies` query) and is
// not one of the appearances the material offers (`offered`, mitcad#1067):
// then the user is asked once, Replace or Keep (logged "Material: replace
// the appearance of Body1?"); Cancel gives no commands and `cancelled`
// (logged "Material cancelled").
struct BodyMaterialCommands {
  QJsonArray commands;
  QString appearance; // replace or keep
  bool cancelled = false;
};
BodyMaterialCommands bodyMaterialCommands(QWidget* parent, const LibraryItem* item, const QStringList& offered,
                                          const QJsonArray& bodyRows, const QStringList& bodies,
                                          const QString& project);

// Materials (mitcad#655): a library window like Appearances, opened by
// Edit Materials: the panel above on the open design, with Edit Library…
// (My Library, mitcad#704) after the list's actions and Close (mitcad#1019).
// It assigns nothing: the Material tool does (mitcad#1059).
class MaterialsDialog : public QDialog {
  Q_OBJECT

public:
  MaterialsDialog(DocumentHost& host, QWidget* parent = nullptr);
  ~MaterialsDialog() override;

  void refresh();
  // The item `key` selected and in sight (MaterialsPanel::showItem).
  void showItem(const QString& key);
  // Edit Appearances… of the Appearance box (the controller's window).
  std::function<void()> editAppearances;
  // Edit Library…: My Library (mitcad#704).
  std::function<void()> openMyLibrary;

private:
  DesignItemsHost m_designHost;
  MaterialsPanel* m_panel = nullptr;
};

} // namespace mitcad
