// SPDX-License-Identifier: MIT
// Send to Slicer's Printer and Process picker (mitcad#1108): the printers
// grouped by vendor, each by its model and nozzle with where it comes from,
// two printers of the same name from two libraries told apart by their
// tags; the processes grouped by the printer their names name.
#include "print/ProfilePickerRows.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "profile_picker_rows.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::LibraryItem;
using mitcad::ProfilePickerRow;

// A row of the `library_items` query as the app reads it; `origin` the
// item's origin library and id (an import's "3mf" and the file).
LibraryItem item(const char* kind, const char* place, const char* id, const char* name, const QJsonObject& parts,
                 const char* originLibrary = nullptr, const char* originId = nullptr) {
  LibraryItem made;
  made.kind = QString::fromLatin1(kind);
  made.place = QString::fromLatin1(place);
  made.id = QString::fromLatin1(id);
  made.name = QString::fromLatin1(name);
  QJsonObject row = parts;
  row.insert(QStringLiteral("name"), made.name);
  if (originLibrary != nullptr) {
    QJsonObject origin;
    origin.insert(QStringLiteral("library"), QString::fromLatin1(originLibrary));
    origin.insert(QStringLiteral("id"), QString::fromLatin1(originId));
    QJsonObject fields;
    fields.insert(QStringLiteral("origin"), origin);
    row.insert(QStringLiteral("item"), fields);
  }
  made.row = row;
  return made;
}

QJsonObject printerParts(const char* bare, double nozzle, double nameNozzle) {
  QJsonObject parts;
  parts.insert(QStringLiteral("bare_name"), QString::fromLatin1(bare));
  parts.insert(QStringLiteral("nozzle"), nozzle > 0.0 ? QJsonValue(nozzle) : QJsonValue());
  parts.insert(QStringLiteral("name_nozzle"), nameNozzle > 0.0 ? QJsonValue(nameNozzle) : QJsonValue());
  return parts;
}

QJsonObject processParts(const char* bare, const char* printer) {
  QJsonObject parts;
  parts.insert(QStringLiteral("bare_name"), QString::fromLatin1(bare));
  parts.insert(QStringLiteral("for_printer"),
               printer != nullptr ? QJsonValue(QString::fromLatin1(printer)) : QJsonValue());
  return parts;
}

QString describe(const ProfilePickerRow& row) {
  return QStringLiteral("%1 | %2 | %3 | %4").arg(row.group, row.shortName, row.source, row.key);
}

void printerRows() {
  // Two printers of the same name, from My library and from a 3MF import
  // into this design; a built-in one of the same vendor whose nozzle only
  // its name says; one of another vendor made in the design; and a new
  // printer's name and number.
  const QVector<LibraryItem> items{
      item("printer", "user", "acme-x1", "Acme Lab X1 Carbon 0.4 nozzle", printerParts("Acme Lab X1 Carbon", 0.4, 0.4)),
      item("printer", "design", "acme-x1", "Acme Lab X1 Carbon 0.4 nozzle",
           printerParts("Acme Lab X1 Carbon", 0.4, 0.4), "3mf", "plate.3mf"),
      item("printer", "mitcad", "acme-p1s", "Acme Lab P1S 0.4 nozzle", printerParts("Acme Lab P1S", 0.0, 0.4)),
      item("printer", "design", "bench-mini", "Bench Mini", printerParts("Bench Mini", 0.0, 0.0)),
      item("printer", "design", "printer1", "Printer 1", printerParts("Printer 1", 0.6, 0.0)),
  };
  const QVector<ProfilePickerRow> rows = mitcad::profilePickerRows(items, QStringLiteral("printer"));
  QStringList listed;
  for (const ProfilePickerRow& row : rows) {
    listed << describe(row);
  }
  const QStringList expected{
      QStringLiteral("Acme Lab | P1S · 0.4 mm nozzle | Built-in | mitcad/acme-p1s"),
      QStringLiteral("Acme Lab | X1 Carbon · 0.4 mm nozzle | plate.3mf | design/acme-x1"),
      QStringLiteral("Acme Lab | X1 Carbon · 0.4 mm nozzle | My library | user/acme-x1"),
      QStringLiteral("Bench | Mini | This design | design/bench-mini"),
      QStringLiteral("Other | Printer 1 · 0.6 mm nozzle | This design | design/printer1"),
  };
  CHECK(listed == expected);
  if (listed != expected) {
    std::fprintf(stderr, "  got: %s\n", qPrintable(listed.join(QStringLiteral(" / "))));
  }
  // The whole name stays the drop-down's label and the tooltip.
  CHECK(rows.size() == 5 && rows.at(1).name == QStringLiteral("Acme Lab X1 Carbon 0.4 nozzle"));
}

void vendors() {
  const QStringList names{QStringLiteral("Acme Lab X1"), QStringLiteral("Acme Lab P1S"), QStringLiteral("Bench X"),
                          QStringLiteral("Bench Mini")};
  CHECK(mitcad::printerVendor(QStringLiteral("Acme Lab X1"), names) == QStringLiteral("Acme Lab"));
  CHECK(mitcad::printerVendor(QStringLiteral("Bench X"), names) == QStringLiteral("Bench"));
  // Alone: the first word.
  CHECK(mitcad::printerVendor(QStringLiteral("Acme Lab X1"), {QStringLiteral("Acme Lab X1")}) ==
        QStringLiteral("Acme"));
  // No vendor for one word or a name and a number.
  CHECK(mitcad::printerVendor(QStringLiteral("Bench"), names).isEmpty());
  CHECK(mitcad::printerVendor(QStringLiteral("Printer 2"), {QStringLiteral("Printer 1"), QStringLiteral("Printer 2")})
            .isEmpty());
  // Two of the same name share every word: a word of the name stays.
  CHECK(mitcad::printerVendor(QStringLiteral("Bench Mini"), {QStringLiteral("Bench Mini"), QStringLiteral("Bench Mini")}) ==
        QStringLiteral("Bench"));
}

void processRows() {
  const QVector<LibraryItem> items{
      item("process", "design", "standard", "0.20mm Standard @Acme X1C", processParts("0.20mm Standard", "Acme X1C")),
      item("process", "project", "draft", "Draft", processParts("Draft", nullptr)),
  };
  const QVector<ProfilePickerRow> rows = mitcad::profilePickerRows(items, QStringLiteral("process"));
  CHECK(rows.size() == 2);
  if (rows.size() == 2) {
    CHECK(describe(rows.at(0)) == QStringLiteral("Acme X1C | 0.20mm Standard | This design | design/standard"));
    CHECK(describe(rows.at(1)) == QStringLiteral("Any printer | Draft | This project | project/draft"));
  }
  // None names a printer: no groups.
  const QVector<ProfilePickerRow> plain = mitcad::profilePickerRows({items.at(1)}, QStringLiteral("process"));
  CHECK(plain.size() == 1 && plain.at(0).group.isEmpty() && plain.at(0).shortName == QStringLiteral("Draft"));
}

// A process row with the model's `printers` (mitcad#1216).
QJsonObject processFor(const char* bare, const QStringList& printers) {
  QJsonObject parts = processParts(bare, nullptr);
  QJsonArray list;
  for (const QString& printer : printers) {
    QJsonObject entry;
    entry.insert(QStringLiteral("printer"), printer);
    entry.insert(QStringLiteral("nozzle"), QJsonValue());
    list.append(entry);
  }
  parts.insert(QStringLiteral("printers"), list);
  return parts;
}

void processRowsByPrinter() {
  // Two processes for Bench X, one for Bench Y by its compatible printers,
  // one for both and one for any printer.
  const QVector<LibraryItem> items{
      item("process", "design", "fine", "0.12mm Fine @Bench X", processFor("0.12mm Fine", {QStringLiteral("Bench X")})),
      item("process", "design", "standard", "0.20mm Standard @Bench X",
           processFor("0.20mm Standard", {QStringLiteral("Bench X")})),
      item("process", "design", "strong", "Strong", processFor("Strong", {QStringLiteral("Bench Y")})),
      item("process", "design", "shared", "Shared",
           processFor("Shared", {QStringLiteral("Bench X"), QStringLiteral("bench  y")})),
      item("process", "design", "draft", "Draft", processFor("Draft", {})),
  };
  const auto listed = [&items](const QStringList& chosen) {
    QStringList out;
    for (const ProfilePickerRow& row : mitcad::profilePickerRows(items, QStringLiteral("process"), chosen)) {
      out << QStringLiteral("%1: %2").arg(row.group, row.shortName);
    }
    return out.join(QStringLiteral(" / "));
  };
  // No printer chosen: by printer, any printer last.
  const QString none = listed({});
  CHECK(none == QStringLiteral("Bench X: 0.12mm Fine / Bench X: 0.20mm Standard / Bench X: Shared / "
                               "Bench Y: Strong / Any printer: Draft"));
  // Bench Y chosen (any case and spacing): its processes first, Shared
  // with them; one heading per printer, as the first process names it.
  const QString benchY = listed({QStringLiteral("Bench Y"), QStringLiteral("Bench Y 0.4 nozzle")});
  CHECK(benchY == QStringLiteral("Bench Y: Shared / Bench Y: Strong / Bench X: 0.12mm Fine / "
                                 "Bench X: 0.20mm Standard / Any printer: Draft"));
  if (none.isEmpty() || benchY.isEmpty()) {
    std::fprintf(stderr, "  got: %s | %s\n", qPrintable(none), qPrintable(benchY));
  }
  // The model's printers, each once; a printer's names.
  CHECK(mitcad::processPrinters(items.at(3)) == QStringList({QStringLiteral("Bench X"), QStringLiteral("bench y")}));
  CHECK(mitcad::processPrinters(items.at(4)).isEmpty());
  CHECK(mitcad::samePrinterName(QStringLiteral("Bench  Y"), QStringLiteral("bench y")));
  CHECK(!mitcad::samePrinterName(QString(), QString()));
  bool forChosen = false;
  CHECK(mitcad::processPickerGroup(items.at(3), {QStringLiteral("BENCH Y")}, &forChosen) == QStringLiteral("bench y") &&
        forChosen);
  CHECK(mitcad::processPickerGroup(items.at(3), {}, &forChosen) == QStringLiteral("Bench X") && !forChosen);
}

} // namespace

int profilePickerRowsTests() {
  printerRows();
  vendors();
  processRows();
  processRowsByPrinter();
  return failures;
}
