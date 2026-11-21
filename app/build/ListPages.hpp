// SPDX-License-Identifier: MIT
#pragma once

// Lists as pages (mitcad#1089): the BUILD lists' tables (the Cut List, the
// Bill of Materials, the Sheet Metal list) and the Keyboard and Mouse
// Overview's cheat sheet (mitcad#2095), under a title and a line under it,
// painted the same way into a PDF file and on a printer. Plain Qt Gui.

#include <QPageLayout>
#include <QString>
#include <QVector>

#include "ListTables.hpp"

class QPagedPaintDevice;

namespace mitcad::build {

// How the tables are laid out on the pages. The defaults are the BUILD
// lists' pages: each table sizes its columns for its own cells and has its
// column header under its caption, and a table that goes on to the next
// page repeats its caption there and its column header under it
// (mitcad#2116).
struct ListPageOptions {
  // One width per column over all the tables, so that the tables' columns
  // line up, and the column header once at the top of each page instead of
  // under each caption. The tables then have the same columns.
  bool sharedColumns = false;
  // A table that goes on to the next page repeats its caption there,
  // followed by "(continued)".
  bool continuedCaptions = true;
};

// The BUILD lists' page: A4 in landscape, 12 mm margins.
QPageLayout listPageLayout();

// Writes the tables to a PDF in `layout` under a title and a line under it
// (the design's name and the date, or the cheat sheet's mouse scheme and
// date); false with the reason when it cannot. `pages` gets the number of
// pages written.
bool writeListPdf(const QString& path, const QString& title, const QString& subtitle,
                  const QVector<ListTable>& tables, QString& error, const QPageLayout& layout = listPageLayout(),
                  const ListPageOptions& options = ListPageOptions(), int* pages = nullptr);

// The PDF's pages on any paged device (the PDF writer, a printer: File ›
// Print, mitcad#1089), in its paint rectangle: the number of pages; -1
// when the device cannot be painted on, -2 when it could not be finished.
// A page breaks only between rows; a table's caption never stays alone at
// the foot of a page. The pages are laid out in a fixed 1200 dpi reference
// and scaled to the device, so that every device (the PDF at 300 dpi, any
// printer) gives the same rows on each page, the same column widths and
// the same cut cells (mitcad#2096, mitcad#2116).
int paintListPages(QPagedPaintDevice& device, const QString& title, const QString& subtitle,
                   const QVector<ListTable>& tables, const ListPageOptions& options = ListPageOptions());

} // namespace mitcad::build
