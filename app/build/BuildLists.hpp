// SPDX-License-Identifier: MIT
#pragma once

// BUILD's OUTPUT group (mitcad#375): the cut list (the `build_cut_list`
// query: the cut parts grouped by identical result, their stock and the
// other parts) and the parts list (`parts_list`), each a table that copies
// and saves as CSV and as an aligned plain-text table (mitcad#1641,
// ListTables.hpp); the Bill of Materials filtered by Kind (mitcad#1980)
// and BOM Structure, as every component or in a structured or parts-only
// view (mitcad#1985), and the Sheet Metal list (`sheet_metal_list`), whose
// parts' Flat Patterns go onto a cut sheet (mitcad#1980). PDFs carry the design's name
// and the date, with the same layout used for printing (mitcad#1089).

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include "ListPages.hpp"
#include "ListTables.hpp"

class QCheckBox;
class QComboBox;
class QLabel;
class QDoubleSpinBox;
class QLineEdit;
class QShowEvent;
class QTabWidget;
class QTableWidget;

namespace mitcad {

class CommandContext;

namespace build {

// The cut list's tables from `build_cut_list`'s answer: the rows, the
// stock, the other parts. A design with timber (the answer's `timber`,
// mitcad#478) adds the timber columns after the others (material, grade,
// finish, nominal and actual section, running metres, volume), a totals
// row, and the stock's size and stock lengths; without timber the tables
// are as before.
QVector<ListTable> cutListTables(const QJsonObject& answer);
// The parts list's table from `parts_list`'s rows (with the suppliers' part
// numbers of each row, mitcad#949, and after them a column Product Pages
// when a row has links in the region chosen in Preferences: its
// `product_urls` separated by spaces, mitcad#950); with timber in the cut
// list (`cut`), the timber parts' material and grade; then a column per
// property key (`keys`, mitcad#496: `Key` or `Set/Key`) with the rows'
// `properties` (the `parts_list` query's `properties`). A structured or
// parts-only view (the answer's `view`, mitcad#1985) adds an Item column
// first and Mass (kg) after the component, indents the component by its
// level, and shows a Reference's quantity as Ref.
QVector<ListTable> partsListTables(const QJsonObject& answer, const QJsonObject& cut = QJsonObject(),
                                   const QStringList& keys = QStringList());
// The Sheet Metal list's table from `sheet_metal_list`'s rows
// (mitcad#1980): quantity, component, body, thickness, material, the
// developed blank's width, height and area, its bends, and why a part's
// Flat Pattern cannot be developed.
QVector<ListTable> sheetMetalTables(const QJsonObject& answer);
// Property keys as a user types them: separated by commas, trimmed, empty
// ones left out (mitcad#496).
QStringList propertyKeys(const QString& text);

class BuildListsDialog : public QDialog {
  Q_OBJECT

public:
  enum class Page { CutList, PartsList, SheetMetal };

  // `design` is the design's name for the PDF.
  BuildListsDialog(const CommandContext& model, const QString& design, Page page, QWidget* parent = nullptr);

  // Prints the tab shown as its PDF lays it out (mitcad#1089): its Print…
  // button, Ctrl+P, and File › Print while the window is open.
  void print();

protected:
  void showEvent(QShowEvent* event) override;

private:
  void load();
  void copyCsv();
  void savePdf();
  void saveCsv();
  // The tab shown as an aligned plain-text table (mitcad#1641): Copy as
  // Text, Save as Text….
  void copyText();
  void saveText();
  // Asks where to save the tab shown (`caption`, `extension` with its
  // `filter`) and writes `contents` there as UTF-8; `what` names the form
  // in the log ("CSV", "text"). False when it was not written.
  bool saveList(const QString& caption, const QString& extension, const QString& filter, const QString& errorTitle,
                const QString& what, const QString& contents);
  QVector<ListTable> shownTables() const;
  // The shown tab's title, for the PDF and printing.
  QString shownTitle() const;
  // The Sheet Metal list's parts that can be developed, each as many
  // times as the design places it, onto the cut sheet chosen (a new one
  // or one of the design's), as Flat Patterns (mitcad#1980).
  void addToCutSheet();
  // The design's cut sheets in the Sheet Metal tab's choice.
  void loadCutSheets();
  // Logs where the property columns field is, for UI tests.
  void logLayout();

  const CommandContext& m_model;
  QString m_design;
  QTabWidget* m_tabs = nullptr;
  QDoubleSpinBox* m_kerf = nullptr;
  QVector<QTableWidget*> m_cutTables; // rows, stock, other
  QTableWidget* m_partsTable = nullptr;
  // The keys of the parts list's property columns (mitcad#496).
  QLineEdit* m_columns = nullptr;
  QCheckBox* m_groupIdentical = nullptr;
  // The Bill of Materials' filters: a kind (mitcad#1980; empty: all) and a
  // BOM Structure (mitcad#1985; empty: all), and its view (`list`,
  // `structured`, `parts`).
  QComboBox* m_kind = nullptr;
  QComboBox* m_structure = nullptr;
  QComboBox* m_view = nullptr;
  // The Sheet Metal tab (mitcad#1980).
  QTableWidget* m_sheetTable = nullptr;
  QComboBox* m_cutSheet = nullptr;
  QLabel* m_sheetNote = nullptr;
  QJsonArray m_sheetRows;
  QString m_logged;
  QVector<ListTable> m_cut;
  QVector<ListTable> m_parts;
  QVector<ListTable> m_sheets;
};

} // namespace build
} // namespace mitcad
