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
#include "MaterialProperties.hpp"
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
class MaterialPropertiesBox;
class MaterialVariantsPage;
class SettingsForm;
class SlicerImportBar;

// The list and editor of the Materials manager (mitcad#655, mitcad#1135).
// On the left a search and the materials grouped by where they live (the
// `library_items` query by place, queryPlaceItems): This design, This
// project (in a project only), My library and Built-in, each a heading
// with its count, and in each place by class (Metals, General Plastics,
// FDM Plastics, Resins, Other), each with its appearance's swatch; the
// search across the places. A design's copy of a library material and that
// material are two rows, each in its place. It manages the materials and
// assigns none: the Material tool assigns (mitcad#1059).
//
// On the right the selected material with its own values where it lives:
// its name as a title, "Built-in" with a lock for a built-in one, else
// Copy to ▾ (the other places), then one line that says where it is edited
// and whom that reaches (editingLine), then the tabs: General (Name, Class,
// Density in g/cm³, Appearances) and, for a plastic only, the print
// settings' (Filament, Cooling, ...).
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
// The list of materials is shallow: the places' headings flush left,
// their classes and materials small steps in; it is as wide as its names
// need (at most under half the window) until the user moves the divider,
// and a name is elided only when it is narrowed, its full name the tooltip.
// Each field is saved at Enter or when it is left, in the item's own
// place: one `edit_material` and undo step (a library item's file with the
// design's copies of it, as before; a design's copy alone). A copy whose
// library item another design changed is asked about once
// (askChangedElsewhere: Use the changes, Keep these). Built-in ones are
// read only: Duplicate makes the user's own copy. New asks the class
// (Metal, Plastic, Other), then where (This design when a design is open,
// else My library, first); Duplicate asks where; both select it. Delete
// removes the item, always asked (confirmDelete). A refusal of a setting
// shows under its field, others on the message line. Read only is
// disabled, never hidden.
//
// Material properties (mitcad#1213, MaterialProperties.hpp): General shows
// Density and the mechanical, thermal and, for an FDM plastic, printing
// values in the design's units (the `material_units` query; temperatures
// in the preferred unit), each saved as Name is; its source and a line
// that the values are typical. New ▾ › Browse Material Dataset… picks a
// material of the bundled dataset (MaterialDatasetDialog), then where it
// goes: `create_material` with `dataset`. Import… reads a JSON or CSV
// file into the place chosen (`import_materials`; a name there already is
// asked once: Keep Existing, Replace Values, Keep Both) and says on the
// message line what it did and each row it did not import, with the
// reason; Export… writes the selected one's place's materials (none
// selected: the design's) in the design's units. The settings' Search
// finds General's fields by name too and shows only those it finds there
// (mitcad#1228), so the print settings it finds stay in sight below them.
//
// Without a design (ManagerHosts) the panel works through a model of its
// own: This design is no group and no place to ask for.
class MaterialsPanel : public QWidget {
  Q_OBJECT

public:
  // `logName` heads the panel's log lines: "Materials".
  MaterialsPanel(PrintItemsHost& host, const QString& logName, QWidget* parent = nullptr);
  ~MaterialsPanel() override;

  // Another model to work through (a manager opened with or without a
  // design); nothing selected, read at once or with `read` false shown
  // empty until refresh (read off the UI thread, mitcad#1018).
  void setHost(PrintItemsHost& host, bool read = true);
  // Shows the materials and the selected one as the model has them now.
  void refresh();
  // Shows the materials with the item `key` ("<place>/<id>") selected and
  // in sight (after an import), logged as "Material selected: <name>
  // (<place>)". A library item the design has a copy of stands for that
  // copy, what the design uses.
  void showItem(const QString& key);
  // A button of the window at the end of the row under the list (Close),
  // its place logged as "<logName> <name> at x,y".
  void addButton(QPushButton* button, const QString& name);
  // The places a material can be made in now.
  QStringList places() const;
  // Edit Appearances… of the Appearance box (the controller's window);
  // none: the box has no such entry.
  std::function<void()> editAppearances;

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
  // The project commands and queries name.
  QString project() const;
  void rebuildList();
  void showCurrent();
  void selectKey(const QString& key);
  // Runs `change` (fields of edit_material) on the selected item, later.
  void edit(const QJsonObject& change, const QString& what, const QString& field = QString());
  // Asks where a new item goes (New, Duplicate); empty when not chosen.
  QString askWhere(QPushButton* anchor, const QString& what);
  // A new item's `library` and project when `place` is a library.
  void inPlace(QJsonObject& command, const QString& place) const;
  void newMaterial();
  // Browse Material Dataset… of New ▾ (mitcad#1213): a dataset's material
  // made the user's own where they choose, selected.
  void newFromDataset();
  // Import… and Export… (mitcad#1213): materials as JSON or CSV files.
  void importMaterials();
  void exportMaterials();
  // Says on the message line and in the log what an import did.
  void reportImport(const QJsonObject& result, const QString& file);
  void duplicate(QPushButton* anchor);
  void remove();
  void copyTo();
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

  PrintItemsHost* m_host = nullptr;
  QString m_log; // "Materials"
  PrintKeyTable m_keys;
  QString m_dialect; // the design's slicer (designDialectOf)
  QVector<LibraryItem> m_items; // by place
  QStringList m_places;         // the groups, in their order
  QJsonArray m_designMaterials;
  QVector<Appearance> m_appearances;
  QJsonArray m_bodies;
  QString m_currentKey;      // the selected row's key; empty for none
  QString m_pendingFocus;    // a row made just now, whose Name takes the keyboard
  bool m_pendingDensity = false; // ... its Density instead (a new material of no typical one)
  bool m_building = false;
  QLineEdit* m_search = nullptr; // across the places (mitcad#1135)
  QTreeWidget* m_list = nullptr;
  QSplitter* m_splitter = nullptr;  // the list and the editor
  bool m_listWidthByUser = false;   // the user moved the divider: no fitListWidth
  QString m_loggedNames;            // the names and the cut ones, as logged
  QWidget* m_editor = nullptr;
  QLineEdit* m_name = nullptr;
  QComboBox* m_class = nullptr;
  QWidget* m_builtIn = nullptr; // "Built-in" with its lock and Duplicate
  QPushButton* m_headerDuplicate = nullptr;
  QLabel* m_editing = nullptr; // editingLine (mitcad#1135)
  QLabel* m_emptyState = nullptr;  // "No material selected"
  QLabel* m_usedState = nullptr; // "● Used in this design"
  QStringList m_usedIds;         // the design's materials its bodies use
  QPushButton* m_copyTo = nullptr;
  QLineEdit* m_density = nullptr;
  QLabel* m_densityUnit = nullptr; // the design's density unit (mitcad#1213)
  MaterialUnits m_units;           // the design's units of materials' values
  MaterialPropertiesBox* m_properties = nullptr; // Mechanical, Thermal, Printing (mitcad#1213)
  QString m_loggedProperties;
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
  QPushButton* m_import = nullptr; // Import… of materials files (mitcad#1213)
  QPushButton* m_export = nullptr;
  QVector<std::pair<QString, QPushButton*>> m_windowButtons; // addButton's
  QString m_loggedRows;
  QString m_loggedEditing;
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

// The Materials manager (mitcad#655, mitcad#1135): a library window like
// Appearances, opened by Make › Materials › Edit Materials…, and with no
// design open on the libraries' own model (ManagerHosts). Import from
// Slicer… at the top (SlicerImportBar, mitcad#829), the box before it
// saying where the materials go (the default as New's), its line and Details… before it; the panel above;
// Close at the end of the row under the list. It assigns nothing: the
// Material tool does (mitcad#1059). Logs "Materials groups: ..." and, on
// an opening without a design, "Materials loaded: read <n> ms, list <n>
// ms" once the items were read off the UI thread.
class MaterialsDialog : public QDialog {
  Q_OBJECT

public:
  MaterialsDialog(DocumentHost& host, QWidget* parent = nullptr);
  ~MaterialsDialog() override;

  // An opening: the model chosen (with or without a design), the item
  // `key` selected, none for an empty key.
  void open() override { open(QString()); }
  void open(const QString& key);
  void refresh();
  // The item `key` selected and in sight (MaterialsPanel::showItem).
  void showItem(const QString& key);
  // Whether it works on the open design.
  bool onDesign() const { return m_hosts.onDesign(); }
  // Edit Appearances… of the Appearance box (the controller's window).
  std::function<void()> editAppearances;

signals:
  // Closed after it changed a library without a design: the other
  // managers read the libraries again.
  void libraryChanged();

protected:
  void hideEvent(QHideEvent* event) override;

private:
  ManagerHosts m_hosts;
  SlicerImportBar* m_import = nullptr;
  MaterialsPanel* m_panel = nullptr;
  int m_reads = 0;        // openings read ahead: only the latest one's answers are shown
  bool m_changed = false; // a library changed without a design
};

} // namespace mitcad
