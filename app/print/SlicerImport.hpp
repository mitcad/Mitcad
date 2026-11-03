// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QWidget>

#include "PrintImportSummary.hpp"

class QComboBox;
class QHBoxLayout;
class QLabel;
class QPushButton;

namespace mitcad {

class PrintItemsHost;

// Import from Slicer (mitcad#829): one implementation for 3D Print's
// panel and the print managers (mitcad#1135). The file dialog (a
// slicer's 3MF, preset, bundle or .ini; the import folder remembered),
// `import_print_profile` with `into` (`design`, whose root then chooses
// the printer and process read in the same undo step, `user` or
// `project`), the summary for the line and Details…, and the log:
//   "Imported print settings from <file>: printers: …" (into the design),
//   "<log> imported into <library>: <kind name, …>" (into a library),
//   "<log> import: <line>", or "<log> import refused: <error>".
// The presets of an installed slicer (mitcad#830, SlicerPresetsDialog)
// come in the same way through `import_slicer_presets`.

// Asks for a slicer's file; empty when the dialog was cancelled.
QString chooseSlicerFile(QWidget* parent);

// What one import did.
struct SlicerImportResult {
  QString file;               // what was imported: the file's name, "3 presets", one preset's name
  QString into;               // design, user or project
  QJsonObject answer;         // the import command's answer
  PrintImportSummary summary; // the line and the report behind Details…
};

// Runs an import command (`import_print_profile`, `import_slicer_presets`)
// into `into` through `host` (the command gets `into` and the project),
// `file` naming what it imports in the line and the report, `log` heading
// the log lines ("3D Print", "Printers", "Materials"). False when the
// model refused it or it was cancelled: `error` (if given) then has the
// refusal in plain words, empty for a cancel.
bool runPrintImport(PrintItemsHost& host, QJsonObject command, const QString& file, const QString& into,
                    const QString& log, SlicerImportResult* result, QString* error = nullptr);

// Imports the slicer's file `path` (runPrintImport of
// `import_print_profile`).
bool runSlicerImport(PrintItemsHost& host, const QString& path, const QString& into, const QString& log,
                     SlicerImportResult* result, QString* error = nullptr);

// The installed Bambu-family slicers with their user presets
// (`slicer_presets`), each preset with the item of its name `into` has;
// empty when there are none or the query failed. `merge` (when given)
// gets the target's separate materials of one filament (mitcad#1054, the
// query's `merge`).
QJsonArray installedSlicers(PrintItemsHost& host, const QString& into, QJsonArray* merge = nullptr);

// The row of a print manager (Materials, Printers, Processes): the last
// import's line, Details… beside it once an import has a report, and
// Import from Slicer…. With an installed Bambu-family slicer found
// (mitcad#830) the button is "Import from Slicer ▾", a menu of "From a
// file…" and "Presets of <slicer>…" for each slicer found, which opens
// SlicerPresetsDialog on that slicer's presets. Logs its buttons' places
// as "<log> import at x,y" and "<log> details at x,y" (the main window's
// coordinates) and the menu as "<log> import menu: <entry> | <entry>".
class SlicerImportBar : public QWidget {
  Q_OBJECT

public:
  SlicerImportBar(PrintItemsHost& host, const QString& logName, QWidget* parent = nullptr);
  ~SlicerImportBar() override;

  // Where an import goes now: `design`, `user` or `project`.
  std::function<QString()> target;
  // The places an import can go (the managers, mitcad#1135:
  // writablePlaces), `current` chosen unless the user chose another of
  // them: a box before the button, "Import into", shown with more than
  // one place, which then stands for `target`. Logs "<log> import into:
  // My library" when the user chooses and its place as "<log> import_into
  // at x,y".
  void setPlaces(const QStringList& places, const QString& current);
  // Another model to import through (a manager with or without a design);
  // the line goes.
  void setHost(PrintItemsHost& host);
  // Hides the line and Details….
  void clear();
  // The row's layout, for a widget of the window before the line (My
  // Library's place).
  QHBoxLayout* row() const { return m_row; }

signals:
  // An import changed the model or a library; `result` says what.
  void imported(const SlicerImportResult& result);

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  QString into() const;
  // Looks for installed slicers again: the button's text follows.
  void findSlicers();
  // The same off the UI thread (an opening, mitcad#1018); logs
  // "<log> slicers: read <n> ms, <n> found".
  void findSlicersAhead();
  void showSlicers(const QJsonArray& slicers);
  void importClicked();
  void importFile();
  // Opens the presets of the slicer `index` of m_slicers and imports
  // those ticked.
  void importPresets(int index);
  // Shows an import's outcome.
  void showImported(const SlicerImportResult& result);
  void showRefusal(const QString& error);
  void scheduleLogPlaces();
  void logPlaces();

  PrintItemsHost* m_host = nullptr;
  QString m_log;
  QHBoxLayout* m_row = nullptr;
  QLabel* m_line = nullptr;
  QPushButton* m_details = nullptr;
  QPushButton* m_import = nullptr;
  QComboBox* m_into = nullptr; // setPlaces's box
  PrintImportSummary m_summary;
  QJsonArray m_slicers; // installedSlicers when last looked
  QJsonArray m_merge;   // ... and the target's separate materials of one filament (mitcad#1054)
  int m_finds = 0;      // looks so far: only the latest one's answer is shown
  QString m_loggedPlaces;
};

} // namespace mitcad
