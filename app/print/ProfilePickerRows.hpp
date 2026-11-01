// SPDX-License-Identifier: MIT
#pragma once

#include <QString>
#include <QStringList>
#include <QVector>

#include "PrintLibrary.hpp"

namespace mitcad {

// A printer or process as Send to Slicer's picker lists it (mitcad#1108,
// app/COMMANDS.md "Send to Slicer"): its key and whole name (the
// drop-down's label and the tooltip), the group it is listed under (a
// printer's vendor, a process's printer), its name without the group with
// a printer's nozzle as "X1 Carbon · 0.4 mm nozzle", and where it comes
// from (This design, My library, Project, Built-in, or the 3MF it was
// imported from).
struct ProfilePickerRow {
  QString key;
  QString name;
  QString group;
  QString shortName;
  QString source;
};

// The rows of the `library_items` of a kind (`printer` or `process`) in
// the picker's order: by group, then by name (the same names by key), so
// that a choice never moves them; rows without a group last, under
// "Other" when others have one. A printer's group is its vendor
// (printerVendor), a process's the printer its name names after an `@`.
QVector<ProfilePickerRow> profilePickerRows(const QVector<LibraryItem>& items, const QString& kind);

// A printer's vendor from its name without the nozzle (the row's
// `bare_name`): its first word, and the words after it that the other
// printers of that first word share up to the first word with a digit
// ("Acme Lab" for "Acme Lab X1" beside "Acme Lab P1S"); none for a name
// of one word or a name and a number ("Printer 1").
QString printerVendor(const QString& bareName, const QStringList& bareNames);

// Where an item comes from, as its picker row's tag: Built-in, My
// library, Project; the design's own the 3MF it was imported from, else
// This design.
QString profileSource(const LibraryItem& item);

} // namespace mitcad
