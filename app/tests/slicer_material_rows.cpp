// SPDX-License-Identifier: MIT
// Send to Slicer's material rows (mitcad#1141): the picker's materials,
// the FDM plastics first, each part grouped by where the material lives;
// the bodies grouped by material into the 3MF's filaments, those without
// a material of their own as one row that says so.
#include "print/SlicerMaterialRows.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "slicer_material_rows.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::BodyMaterial;
using mitcad::FilamentGroup;
using mitcad::LibraryItem;
using mitcad::MaterialPickerRow;

// A material row of the `library_items` query as the app reads it: its
// class, the library a command names, whether bodies use it, and the 3MF
// it was imported from.
LibraryItem material(const char* place, const char* id, const char* name, const char* materialClass,
                     const char* library = "", bool usedByBodies = false, const char* importedFrom = nullptr) {
  LibraryItem made;
  made.kind = QStringLiteral("material");
  made.place = QString::fromLatin1(place);
  made.id = QString::fromLatin1(id);
  made.name = QString::fromLatin1(name);
  made.materialClass = QString::fromLatin1(materialClass);
  made.library = QString::fromLatin1(library);
  QJsonObject row;
  row.insert(QStringLiteral("name"), made.name);
  if (usedByBodies) {
    row.insert(QStringLiteral("bodies"), QJsonArray{QStringLiteral("F1.b0")});
  }
  if (importedFrom != nullptr) {
    QJsonObject origin;
    origin.insert(QStringLiteral("library"), QStringLiteral("3mf"));
    origin.insert(QStringLiteral("id"), QString::fromLatin1(importedFrom));
    QJsonObject fields;
    fields.insert(QStringLiteral("origin"), origin);
    row.insert(QStringLiteral("item"), fields);
  }
  made.row = row;
  return made;
}

void pickerRows() {
  const QVector<LibraryItem> items{
      material("mitcad", "steel", "Steel", "metal"),
      material("mitcad", "pla", "PLA", "fdm_plastic"),
      material("user", "shop-petg", "Shop PETG", "fdm_plastic", "user"),
      material("design", "shop-tpu", "Shop TPU", "fdm_plastic"),
      material("design", "bench-pla", "Bench PLA", "fdm_plastic", "", false, "plate.3mf"),
      material("project", "nylon", "Nylon", "fdm_plastic", "project"),
      material("mitcad", "petg", "PETG", "fdm_plastic", "", true), // used by a body: the design's
      material("user", "brass", "Brass", "metal", "user"),
  };
  QStringList listed;
  for (const MaterialPickerRow& row : mitcad::materialPickerRows(items)) {
    listed << QStringLiteral("%1 | %2").arg(row.heading, row.name);
  }
  const QStringList expected{
      QStringLiteral("This design | Shop TPU"),
      QStringLiteral("This design | PETG"),
      QStringLiteral("Imported from plate.3mf | Bench PLA"),
      QStringLiteral("This project | Nylon"),
      QStringLiteral("My library | Shop PETG"),
      QStringLiteral("Built-in | PLA"),
      QStringLiteral("Other materials: My library | Brass"),
      QStringLiteral("Other materials: Built-in | Steel"),
  };
  CHECK(listed == expected);
  if (listed != expected) {
    std::fprintf(stderr, "  got: %s\n", qPrintable(listed.join(QStringLiteral(" / "))));
  }
}

BodyMaterial body(const char* uid, const char* name, const char* id, const char* material, bool chosen) {
  BodyMaterial made;
  made.body = QString::fromLatin1(uid);
  made.bodyName = QString::fromLatin1(name);
  made.id = QString::fromLatin1(id);
  made.name = QString::fromLatin1(material);
  made.chosen = chosen;
  return made;
}

void groups() {
  // Two bodies of PETG, one of TPU between them, and one without a
  // material (the design's default, Steel, which the slicer gets as the
  // print default, PLA: mitcad#1727).
  const QVector<FilamentGroup> made = mitcad::filamentGroups({
      body("F1.b0", "Body1", "petg", "PETG", true),
      body("F2.b0", "Body2", "tpu", "TPU", true),
      body("F3.b0", "Body3", "petg", "PETG", true),
      body("F4.b0", "Body4", "steel", "PLA Plastic", false),
  });
  CHECK(made.size() == 3);
  if (made.size() == 3) {
    CHECK(made.at(0).bodies == QStringList({QStringLiteral("F1.b0"), QStringLiteral("F3.b0")}));
    CHECK(mitcad::filamentRowLabel(0, 3) == QStringLiteral("Filament 1"));
    CHECK(mitcad::filamentRowText(made.at(0), 3) == QStringLiteral("PETG (2 bodies)"));
    CHECK(mitcad::filamentRowText(made.at(1), 3) == QStringLiteral("TPU (1 body)"));
    CHECK(mitcad::filamentRowText(made.at(2), 3) == QStringLiteral("No material: choose one (1 body)"));
    CHECK(mitcad::filamentRowTooltip(made.at(0)) == QStringLiteral("PETG: Body1, Body3"));
    CHECK(mitcad::filamentRowTooltip(made.at(2)) ==
          QStringLiteral("Body4 has no material of its own: until one is chosen, the slicer gets PLA Plastic."));
  }
  // One material: a row "Material" without the count; a body without one
  // says so, not as if the default were chosen.
  const QVector<FilamentGroup> none = mitcad::filamentGroups({body("F1.b0", "Body1", "steel", "Steel", false)});
  CHECK(none.size() == 1);
  if (none.size() == 1) {
    CHECK(mitcad::filamentRowLabel(0, 1) == QStringLiteral("Material"));
    CHECK(mitcad::filamentRowText(none.at(0), 1) == QStringLiteral("No material: choose one"));
  }
  // A chosen Steel is a group of its own beside the bodies without one.
  CHECK(mitcad::filamentGroups({body("F1.b0", "Body1", "steel", "Steel", true),
                                body("F2.b0", "Body2", "steel", "Steel", false)})
            .size() == 2);
  // The variant the printer picks after the name.
  BodyMaterial variant = body("F1.b0", "Body1", "bench-pla", "Bench PLA", true);
  variant.variant = QStringLiteral("Bench X, 0.4 mm nozzle");
  const QVector<FilamentGroup> withVariant = mitcad::filamentGroups({variant});
  CHECK(withVariant.size() == 1 &&
        mitcad::filamentRowText(withVariant.at(0), 1) == QStringLiteral("Bench PLA for Bench X, 0.4 mm nozzle"));
}

} // namespace

int slicerMaterialRowsTests() {
  pickerRows();
  groups();
  return failures;
}
