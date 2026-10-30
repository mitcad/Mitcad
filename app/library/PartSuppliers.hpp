// SPDX-License-Identifier: MIT
#pragma once

// The part grid's Supplier filter (mitcad#949; app/COMMANDS.md, "Part
// picker"): beside the search, All and each supplier of the fetched
// libraries' parts; a part no row of which that supplier sells is hidden.
// A part's suppliers are those with articles on its rows (`library_search`'s
// `suppliers`: {id, name}) and the component's own `supplier` (a BUILD
// part's maker, by its name). The choice is kept in the settings
// ("partPicker/supplierFilter"). Plain Qt Core, so that the unit tests
// (tests/part_picker_data.cpp) build it on its own.

#include <QJsonObject>
#include <QList>
#include <QString>

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
// Whether a part of these suppliers passes the filter `id` (empty: All).
bool soldBy(const QList<PartSupplier>& suppliers, const QString& id);
// The suppliers' names, joined by ", " (the hover's line).
QString supplierNames(const QList<PartSupplier>& suppliers);

// The Supplier filter kept in the settings: the supplier's id, empty for All.
QString savedSupplierFilter();
void saveSupplierFilter(const QString& id);

} // namespace mitcad
