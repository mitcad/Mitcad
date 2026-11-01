// SPDX-License-Identifier: MIT
#pragma once

#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

// The context menus' Material ▸ submenu (mitcad#1046, app/COMMANDS.md
// "Material ▸ in the context menus"): which materials it offers, and the
// materials last assigned from the libraries, kept in the local settings
// ("materials/recent", the items' keys "<place>/<id>", the newest first).
// Plain Qt Core: the controller builds the menu from these lists, and tests
// use them without one.

// How many library materials the submenu remembers and offers.
constexpr int kRecentMaterials = 8;

// A material the user can choose, as the `library_items` query gives it.
struct MaterialChoice {
  QString key;           // "<place>/<id>" (LibraryItem::key)
  bool inDesign = false; // the design's own, a copy of a library item, or a built-in its bodies or components use
};

// The submenu's rows, as indices into the items: the design's materials
// (in the items' order) and, after them, the recent library materials not
// in the design (the newest first, at most `limit`).
struct MaterialMenuRows {
  QVector<int> design;
  QVector<int> recent;
};

// The rows: an item is the design's when it is in the design or one of the
// targets is made of it (`current`, keys); a recent key whose item is
// gone, or is the design's, is left out.
MaterialMenuRows materialMenuRows(const QVector<MaterialChoice>& items, const QStringList& current,
                                  const QStringList& recent, int limit = kRecentMaterials);

// The material all targets have (keys, one per target); empty when they
// differ, when one has none, or for no targets: then nothing is checked.
QString commonMaterial(const QStringList& perTarget);

// The recent list with `key` first (moved there when it was in it), at
// most `limit` long.
QStringList withRecentMaterial(const QStringList& recent, const QString& key, int limit = kRecentMaterials);

// Whether an item of `place` comes from a library (built-in, the base
// library, My library, the project's): the design's own (`design`) does not.
bool isLibraryPlace(const QString& place);

// The recent list in the settings, and a material assigned from a library
// put first in it (none for the design's own items).
QStringList recentMaterials();
void rememberRecentMaterial(const QString& place, const QString& key);

// The appearances a material offers (mitcad#1055), its default first, from
// its fields: `appearance` and `appearances` (the `materials` query's, a
// `library_items` row's or its `item`'s; the list with or without the
// default).
QStringList materialAppearances(const QJsonObject& fields);

// Whether assigning a material that offers `offered` asks about a body's
// appearance (a row of the `bodies` query): only one set by hand that the
// material does not offer (mitcad#1067); one it offers stays the body's.
bool asksToReplaceAppearance(const QJsonObject& body, const QStringList& offered);

} // namespace mitcad
