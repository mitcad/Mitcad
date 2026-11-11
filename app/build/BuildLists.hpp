// SPDX-License-Identifier: MIT
#pragma once

// BUILD's OUTPUT group (mitcad#375): the cut list (the `build_cut_list`
// query: the cut parts grouped by identical result, their stock and the
// other parts) and the parts list (`parts_list`), each a table that copies
// and saves as CSV and as an aligned plain-text table (mitcad#1641,
// ListTables.hpp). PDFs carry the design's name and the date, with the same
// layout used for printing (mitcad#1089).

#include <QDialog>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include "ListTables.hpp"

class QCheckBox;
class QDoubleSpinBox;
class QPagedPaintDevice;
class QLineEdit;
class QShowEvent;
class QTabWidget;
class QTableWidget;

namespace mitcad {

class CommandContext;

namespace build {

// Writes the tables to a PDF (A4) under a title and a line with the
// design's name and the date; false with the reason when it cannot.
bool writeListPdf(const QString& path, const QString& title, const QString& subtitle,
                  const QVector<ListTable>& tables, QString& error);
// The PDF's pages on any paged device (the PDF writer, a printer: File ›
// Print, mitcad#1089), in its paint rectangle: the number of pages; -1
// when the device cannot be painted on, -2 when it could not be finished.
int paintListPages(QPagedPaintDevice& device, const QString& title, const QString& subtitle,
                   const QVector<ListTable>& tables);

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
// `properties` (the `parts_list` query's `properties`).
QVector<ListTable> partsListTables(const QJsonObject& answer, const QJsonObject& cut = QJsonObject(),
                                   const QStringList& keys = QStringList());
// Property keys as a user types them: separated by commas, trimmed, empty
// ones left out (mitcad#496).
QStringList propertyKeys(const QString& text);

class BuildListsDialog : public QDialog {
  Q_OBJECT

public:
  enum class Page { CutList, PartsList };

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
  QString m_logged;
  QVector<ListTable> m_cut;
  QVector<ListTable> m_parts;
};

} // namespace build
} // namespace mitcad
