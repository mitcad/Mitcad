// SPDX-License-Identifier: MIT
#pragma once

// The BUILD lists' tables (mitcad#375) as text: CSV for a spreadsheet and an
// aligned plain-text table for an email (mitcad#1641). Both end every line
// in CRLF on every platform, as RFC 4180 has it for CSV and as mail editors
// and Windows programs show plain text line by line. Plain Qt Core.

#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad::build {

// A table of a list: its heading, the column names and the rows' cells.
struct ListTable {
  QString heading;
  QStringList columns;
  QVector<QStringList> rows;
  QVector<QString> tooltips{};
};

// The tables as CSV: each table's columns and rows, a blank line between
// tables (and each table's heading before it when there are several),
// fields quoted where they need it, every line ending in CRLF.
QString csvOf(const QVector<ListTable>& tables);

// The tables as a plain-text table: a header row with the column names,
// each column padded with spaces to its widest cell and two spaces between
// columns, numbers right-aligned (a column whose cells are all numbers has
// its name right-aligned too), no spaces at a line's end; a blank line
// between tables, each table's heading before it when there are several (as
// csvOf); every line ending in CRLF. A cell's line breaks and tabs become
// spaces. Widths count characters (combining marks take no room).
QString textTableOf(const QVector<ListTable>& tables);

} // namespace mitcad::build
