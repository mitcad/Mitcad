// SPDX-License-Identifier: MIT
#pragma once

// The part grid's Supplier filter (mitcad#949; app/COMMANDS.md, "Part
// picker"): beside the search, All and each supplier of the fetched
// libraries' parts; a part no row of which that supplier sells is hidden.
// A part's suppliers are those with articles on its rows (`library_search`'s
// `suppliers`: {id, name}) and the component's own `supplier` (a BUILD
// part's maker, by its name). The choice is kept in the settings
// ("partPicker/supplierFilter"). Until the user chooses one there, it is
// the shops chosen in Preferences (mitcad#950, "suppliers/shops"): one
// shop's supplier, or Your shops for several (their ids separated by
// '|', PartOptions.hpp's supplierFilterIds); without shops All. Plain Qt
// Core, so that the unit tests (tests/part_picker_data.cpp) build it on
// its own.

#include <QJsonObject>
#include <QList>
#include <QString>
#include <QStringList>

namespace mitcad {

// A supplier: its id in its library (the name for a component's own
// `supplier`) and its name for people.
struct PartSupplier {
  QString id;
  QString name;
};

// The suppliers of a component as library_search, library_show or
// library_build_candidates gives it: its rows' (`suppliers`), then its own
// `supplier`; each once.
QList<PartSupplier> componentSuppliers(const QJsonObject& component);
// The suppliers of one configuration row of a component's table (a tile of
// one row): those with articles on that row (`supplier_parts`), then the
// component's own `supplier`.
QList<PartSupplier> rowPartSuppliers(const QJsonObject& configurations, const QString& row,
                                     const QString& ownSupplier);
// The filter's choices: every supplier of the lists once (by id), sorted
// by name.
QList<PartSupplier> supplierChoices(const QList<QList<PartSupplier>>& lists);
// Whether a part of these suppliers passes the filter `id` (empty: All;
// several ids separated by '|': any of them).
bool soldBy(const QList<PartSupplier>& suppliers, const QString& id);
// The entry for several shops' suppliers (mitcad#950): "Your shops" with
// the filter of those of `preferred` (ids) that `suppliers` has; null
// (an empty id) unless two or more of them are there.
PartSupplier yourShopsChoice(const QList<PartSupplier>& suppliers, const QStringList& preferred);
// The entry a wanted filter selects among `suppliers`: the filter itself
// for one supplier; for several, Your shops of those the list has, or the
// one of them it has (empty when it has none: All).
QString supplierChoiceFor(const QList<PartSupplier>& suppliers, const QString& wanted);
// The suppliers' names, joined by ", " (the hover's line).
QString supplierNames(const QList<PartSupplier>& suppliers);

// The Supplier filter kept in the settings: the supplier's id (or the
// ids of Your shops), empty for All; until one is kept, the shops chosen
// in Preferences.
QString savedSupplierFilter();
void saveSupplierFilter(const QString& id);
// The shops chosen in Preferences (mitcad#950): their suppliers' ids, and
// as a Supplier filter (empty: All).
QStringList preferredShops();
QString preferredSupplierFilter();
// Keeps the shops chosen in Preferences; the part grid's kept filter goes,
// so that the filters start with the new shops.
void savePreferredShops(const QStringList& ids);

} // namespace mitcad
