// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

class QCheckBox;
class QComboBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

// Presets of an installed slicer (mitcad#830): the user presets of a
// Bambu-family slicer (`slicer_presets`' `slicers`) and the slicer's own
// presets the user enabled there (`system`, mitcad#871) to tick for an
// import, in a modal window titled Import Presets.
//
// At the top the slicer, a box when several were found ("Bambu Studio",
// "OrcaSlicer (Flatpak)"), and under it the filter ("Filter presets": the
// rows whose name contains its text, any case; a group without such a row
// hidden, those with one open while it filters, as they were again when it
// is cleared; hidden rows keep their ticks, which count and import, and a
// group's box then ticks only the rows shown). The list groups the user's
// presets under My printers, My processes and My filaments, then the
// slicer's own under Bambu Studio printers, Bambu Studio processes and
// Bambu Studio filaments (collapsed at first unless a row in them has a
// same name, their presets unticked: a user preset mostly inherits from
// them already), each group's box ticking all or none of it, each preset a
// box by its name. A preset whose name the target already has (`same`) is
// in the warning colour with a tooltip naming what it meets ("My library
// has a process of this name") and that and its choices as its accessible
// description, unticked at first, and has its own choice beside it,
// labelled "Same name:" ("<name>: same name"): Copy (the default: a new
// item, "<name> 2") or Override (the existing item takes its values);
// choosing in it ticks the row. "All same names:" above the list sets
// every such row's choice at once (the ticks stay); it shows the rows'
// choice when they agree and no value when they differ, and only when
// there are such rows. New user presets are ticked at first. The name
// column is as wide as the names and the group titles, each choice beside
// its name. Import says how many are ticked ("Import 1 Preset", "Import 3
// Presets"; disabled with nothing ticked) and Cancel.
//
// Keyboard: the list has it at first, its first user preset the current
// row (else its first group), framed while the keyboard is in the row or
// its choice; the arrows move the row (Right opens a closed group), Space
// ticks or unticks it (a group: all of it), Tab goes to
// the row's choice and on to Import, Shift+Tab from the choice back to
// the row and from the list to the filter; Down or Enter in the filter
// goes to the list.
//
// Logs "Slicer presets opened: <slicer>, <n> presets", each row as
// "Slicer presets row: <kind> <name>, ticked|unticked[, system][, same name:
// <tooltip>, copy|override]", "Slicer presets ticked: <n>, <Import's text>"
// when that changes, "Slicer presets tick: <name> ticked|unticked" for a
// row's change, "Slicer presets choice: <name> copy|override", "Slicer
// presets same name: copy|override" (All same names chosen), "Slicer
// presets all same names: copy|override|none" when what it shows changes,
// "Slicer presets filter: <text>, <n> shown" ("Slicer presets filter
// cleared, <n> shown"), "Slicer presets current: <name>" (the current row
// moved), "Slicer presets focus: <name> choice" (a row's choice has the
// keyboard), and the places "Slicer presets <what> at x,y" (`slicer`,
// `same_all`, `import`, `cancel`, `item <name>`: a row's box).
//
// One filament, its presets as variants (mitcad#1054): the presets of a
// filament with two or more in a group (the same name before its `@`,
// `slicer_presets`' `filament`) are one material row named as the
// filament, its box ticking all of them, its same-name choice theirs, and
// under it a row per preset named by the printers and nozzles it is for
// ("Bench X, 0.6 mm nozzle"; the preset's name as its tooltip). Then the
// list starts with Printers and nozzles in use: a box per printer and
// nozzle the slicer's filaments are for (`slicer_presets`' `printers`),
// ticked at first; an unticked one disables and unticks the variant rows
// of it alone, and the import (`printers`) leaves its variants out. When
// the target keeps one filament's presets as separate materials from
// before (`slicer_presets`' `merge`), a box under the list offers to merge
// them ("Merge the separate materials of 1 filament", unticked; the
// filaments and their materials as its tooltip): ticked, the import merges
// them first (`merge_separate`), also with no preset ticked (the button is
// then Merge). A variant row's name in the logs is "<filament> › <row>".
// Logs "Slicer presets printer: <label>, ticked|unticked" for each
// printer row and "Slicer presets printer tick: <label> ticked|unticked"
// for a change, "Slicer presets merge offered: <n>: <filament> (<material>,
// ...); ..." and "Slicer presets merge: ticked|unticked", and the places
// "Slicer presets printer <label> at x,y" and "Slicer presets merge at".
class SlicerPresetsDialog : public QDialog {
  Q_OBJECT

public:
  // `slicers` from `slicer_presets`, `index` the one shown first, `into`
  // the target (`design`, `user`, `project`), which the tooltips name;
  // `merge` the target's separate materials of one filament (its `merge`).
  SlicerPresetsDialog(const QJsonArray& slicers, int index, const QString& into, QWidget* parent = nullptr,
                      const QJsonArray& merge = QJsonArray());

  // The `import_slicer_presets` command of the ticked presets (`dialect`,
  // `folder`, `presets` with `same_name`; the printers and nozzles in use
  // and `merge_separate` when the list has them), without its target.
  QJsonObject command() const;
  // How many presets are ticked.
  int tickedCount() const;
  // Whether the separate materials are to be merged (mitcad#1054).
  bool mergeChosen() const;
  // The ticked presets' names, in the list's order.
  QStringList tickedNames() const;
  // The slicer shown ("Bambu Studio").
  QString slicerName() const;
  // A slicer's name in the box and the menus: "OrcaSlicer (Flatpak)" for
  // a Flatpak's.
  static QString slicerTitle(const QJsonObject& slicer);

protected:
  void showEvent(QShowEvent* event) override;
  // The filter's Down and Enter, the list's Space and Tab, the rows' choices'
  // Tab and focus.
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // The slicer shown, of `slicer_presets`' `slicers`.
  QJsonObject shownSlicer() const;
  void showSlicer(int index);
  // The tooltip of a preset whose name the target has.
  QString sameTooltip(const QString& kind, const QJsonObject& same) const;
  // A same-name row's choice, `copy` or `override`.
  void setRowChoice(QTreeWidgetItem* row, const QString& choice);
  // All same names shows the rows' choice, or none when they differ.
  void sameChoicesChanged();
  void ticksChanged();
  // Shows the rows the filter matches (all without one).
  void applyFilter();
  // The variant rows of the printers and nozzles unticked disabled and
  // unticked, the others enabled (mitcad#1054).
  void applyPrinters();
  // Adds a same-name choice beside `row` (a preset's or a filament's).
  void addSameChoice(QTreeWidgetItem* row, const QString& kind, const QString& name, const QJsonObject& same);
  void logRows();
  void scheduleLogPlaces();
  void logPlaces();

  QJsonArray m_slicers;
  int m_index = 0;
  QString m_into;
  QJsonArray m_merge; // the target's separate materials of one filament
  QComboBox* m_slicer = nullptr;
  QLabel* m_slicerLabel = nullptr;
  QLabel* m_sameLabel = nullptr;
  QComboBox* m_sameAll = nullptr;
  QLineEdit* m_filter = nullptr;
  QTreeWidget* m_list = nullptr;
  QCheckBox* m_mergeBox = nullptr;
  QPushButton* m_import = nullptr;
  QPushButton* m_cancel = nullptr;
  QHash<QObject*, QTreeWidgetItem*> m_boxRows; // a same-name row's choice, its row
  bool m_building = false;
  int m_loggedTicked = -1;
  QHash<QTreeWidgetItem*, bool> m_loggedTicks; // each row's tick as last logged
  QString m_loggedSameAll;
  QString m_loggedPlaces;
  bool m_filtering = false;                         // the filter has text
  QHash<QTreeWidgetItem*, bool> m_openBeforeFilter; // each group open before the filter
};

} // namespace mitcad
