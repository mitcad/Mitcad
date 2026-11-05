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
// from (This design, This project, My library, Built-in, or the 3MF it was
// imported from).
struct ProfilePickerRow {
  QString key;
  QString name;
  QString group;
  QString shortName;
  QString source;
  // A process listed under the chosen printer (mitcad#1216).
  bool forChosen = false;
  // A built-in listed apart, last under "Built-in" (mitcad#1270).
  bool builtIn = false;

  // The Make card shows rows again only when they changed (mitcad#1239).
  bool operator==(const ProfilePickerRow& other) const {
    return key == other.key && name == other.name && group == other.group && shortName == other.shortName &&
           source == other.source && forChosen == other.forChosen && builtIn == other.builtIn;
  }
};

// The rows of the `library_items` of a kind (`printer` or `process`) in
// the picker's order: by group, then by name (the same names by key), so
// that a choice never moves them; rows without a group last, under
// "Other" when others have one. A printer's group is its vendor
// (printerVendor). A process's is its printer (mitcad#1216,
// processPickerGroup): the processes of the chosen printer
// (`chosenPrinter`, the names printerNames gives; empty: none chosen)
// first, then the others under their printers, those for any printer
// last under "Any printer". While My library or This project has one of
// the kind (mitcad#1270, as hasOwnLibraryItems), the built-ins come after
// the user's own, under one "Built-in" heading, each by its whole name
// (a printer with its nozzle); a hidden built-in the design uses stays
// with the user's own, as This design's.
QVector<ProfilePickerRow> profilePickerRows(const QVector<LibraryItem>& items, const QString& kind,
                                            const QStringList& chosenPrinter = QStringList());

// The printers a process is for (mitcad#1216: the model's `printers` of
// its `library_items` row, by the filament presets' rule: its compatible
// printers, else its name's `@` part), each named once without its
// nozzle, in the model's order; empty for any printer. A row without
// `printers` names the printer after its name's `@` (`for_printer`).
QStringList processPrinters(const LibraryItem& process);
// The names a printer goes by (the row's `printer_names`: its model, its
// name, its slicers' preset names, each also without the nozzle); its
// name when the row gives none.
QStringList printerNames(const LibraryItem& printer);
// Whether two names name the same printer, in any case and spacing.
bool samePrinterName(const QString& a, const QString& b);
// The group of the processes for any printer: "Any printer".
QString anyPrinterTitle();
// The printer a picker lists a process under (mitcad#1216): the one of its
// printers the chosen printer goes by (`chosenPrinter`, printerNames;
// `forChosen` set), else its first printer; empty for any printer.
QString processPickerGroup(const LibraryItem& process, const QStringList& chosenPrinter, bool* forChosen = nullptr);

// A printer's vendor from its name without the nozzle (the row's
// `bare_name`): its first word, and the words after it that the other
// printers of that first word share up to the first word with a digit
// ("Acme Lab" for "Acme Lab X1" beside "Acme Lab P1S"); none for a name
// of one word or a name and a number ("Printer 1").
QString printerVendor(const QString& bareName, const QStringList& bareNames);

// Where an item comes from, as its picker row's tag: Built-in, My
// library, This project; the design's own the 3MF it was imported from, else
// This design; a hidden built-in the design uses This design (mitcad#1270).
QString profileSource(const LibraryItem& item);

} // namespace mitcad
