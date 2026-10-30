// SPDX-License-Identifier: MIT
#pragma once

// The suppliers' articles of a library part's row behind a button
// (mitcad#979; app/COMMANDS.md, "Options of library parts";
// docs/libraries.md, "Standards and suppliers of rows"): Insert from
// Library, Library Parts and Edit Options… show **Supplier Articles
// (n)…** (Alt+A) under the size choosers while the chosen row has articles,
// in place of one long line of part numbers. It opens a table with one
// line per article and the columns its articles fill: Supplier (a link to
// the supplier's site when the library gives one), Part number, Material,
// Finish, Checked and Product page (the row's or the component's
// `supplier_url`). Every column sorts by a click on its heading. **Copy
// as TSV** (Alt+T) copies the selected lines, else all, as tab-separated
// text with the headings; **Copy Part Number** (Alt+P) and Ctrl+C on a
// single cell copy a part number on its own; **Open Link** (Alt+L), a
// double click or Return opens the link of the cell (else the line's
// supplier's site). The articles of the supplier the Supplier filter chose
// come first; the others follow.
//
// Logged for the UI tests: "Supplier parts of <row>: <n> articles, columns
// <list>" when it opens, "Supplier articles copied: <n> rows", "Supplier
// part number copied: <number>", "Supplier cells copied: <text|n lines>"
// and "Supplier link opened: <url>" (the tests' MITCAD_TEST_LOG_URLS then
// logs the address instead of opening it).

#include <QDialog>
#include <QJsonArray>
#include <QList>
#include <QString>

#include "PartOptions.hpp"

class QPushButton;
class QTableWidget;

namespace mitcad {

// What the table shows: a row's articles and where they link.
struct SupplierPartsRequest {
  QString row;           // the row's name, for the log ("M5x16")
  QString title;         // the part's designation or name, for the title
  QJsonArray parts;      // the row's `supplier_parts`
  QJsonArray suppliers;  // the library's suppliers ({id, name, url})
  QString productPage;   // the row's or the component's `supplier_url`
  QString firstSupplier; // the Supplier filter's id: its articles first
};

class SupplierPartsDialog : public QDialog {
  Q_OBJECT

public:
  explicit SupplierPartsDialog(const SupplierPartsRequest& request, QWidget* parent = nullptr);

  // The articles in the table's order before any sorting.
  const QList<SupplierArticle>& articles() const { return m_articles; }
  const QList<SupplierColumn>& columns() const { return m_columns; }

private:
  // The article of a table line (the lines may be sorted), or nullptr.
  const SupplierArticle* articleAt(int line) const;
  // The selected lines in the table's order; all lines when none is.
  QList<int> chosenLines() const;
  void copyLines();
  void copyPartNumber();
  // Ctrl+C: one cell's text alone, else the selected cells by line.
  void copyCells();
  void openLink();
  // The current cell's link, else its line's supplier's site.
  QString currentLink() const;
  void updateButtons();

  QList<SupplierArticle> m_articles;
  QList<SupplierColumn> m_columns;
  QTableWidget* m_table = nullptr;
  QPushButton* m_copyNumber = nullptr;
  QPushButton* m_openLink = nullptr;
};

// The button under the size choosers: "Supplier Articles (n)…" (Alt+A)
// with its tooltip, hidden until setSupplierPartsCount gives it articles.
QPushButton* makeSupplierPartsButton(QWidget* parent = nullptr);
// The button's count, shown only when the row has articles.
void setSupplierPartsCount(QPushButton* button, qsizetype count);
// Opens the table of `request`, modal to `parent`.
void showSupplierParts(QWidget* parent, const SupplierPartsRequest& request);

} // namespace mitcad
