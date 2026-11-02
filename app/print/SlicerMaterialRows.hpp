// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QVector>

#include "PrintLibrary.hpp"

namespace mitcad {

// Send to Slicer's material rows (mitcad#1141, app/COMMANDS.md "Send to
// Slicer"): one row per filament, each a searchable material picker. Plain
// Qt Core: the panel builds its rows from these, and tests use them
// without one.

// A material as the picker lists it: its key ("<place>/<id>"), its name,
// the heading it is listed under and its colour (invalid without one).
struct MaterialPickerRow {
  QString key;
  QString name;
  QString heading;
  QColor color;
  bool fdm = false; // an FDM plastic (a filament)
};

// The materials of the `library_items` query in the picker's order: the
// FDM plastics first (class `fdm_plastic`, the filaments), then the
// others; each part grouped by where the material lives: This design
// (the design's own, a library's copied in, a built-in one it uses), the
// filaments imported from a 3MF ("Imported from plate.3mf"), This
// project, My library, Built-in. The other materials' headings say so
// ("Other materials: Built-in"). In a heading the query's order stays.
QVector<MaterialPickerRow> materialPickerRows(const QVector<LibraryItem>& items);

// What a body prints with, as the `body_material` query says: its uid
// and name, the material's id and name, whether the body or a component
// of it chose the material (not the design's default), the variant the
// printer picks ("Bench X, 0.4 mm nozzle") and the material's filament
// preset (`filament.filament_settings_id`).
struct BodyMaterial {
  QString body;
  QString bodyName;
  QString id;
  QString name;
  bool chosen = false;
  QString variant;
  QJsonValue preset;
};

// The bodies of one material: one filament of the 3MF. Bodies without a
// material of their own are one group (`chosen` false), whatever the
// default is.
struct FilamentGroup {
  QString id; // the material's id (the default's for the bodies without one)
  QString name;
  bool chosen = false;
  QString variant;
  QJsonValue preset;
  QStringList bodies;     // uids
  QStringList bodyNames;
};

// The groups in the order of the bodies (their first body's place).
QVector<FilamentGroup> filamentGroups(const QVector<BodyMaterial>& bodies);

// A row's label: "Material" for one group, else "Filament 2" (`index`
// from 0).
QString filamentRowLabel(int index, int groups);
// What the row's field shows: the material ("Shop PLA for Bench X, 0.4 mm
// nozzle" with its variant), or "No material: choose one" for the bodies
// without one; with several groups the bodies counted, "PETG (2 bodies)".
QString filamentRowText(const FilamentGroup& group, int groups);
// The field's tooltip: the bodies after the material, "PETG: Body1,
// Body2"; for the bodies without one what the slicer gets meanwhile.
QString filamentRowTooltip(const FilamentGroup& group);

} // namespace mitcad
