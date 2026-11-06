// SPDX-License-Identifier: MIT
#pragma once

// The suppliers' shops in the application (mitcad#950; docs/libraries.md,
// "Shops and product links"; app/COMMANDS.md, "Suppliers' shops"): the
// region and the shops chosen in Preferences, and the product links of the
// suppliers' articles. MitCad's list of shops and the links are the
// model's (the library commands `supplier_shops` and `supplier_links`,
// core/model-api/src/commands.md, "Suppliers' shops and product links");
// here they are only asked for and shown.
//
// - The region (settings "suppliers/region"): an ISO 3166-1 alpha-2 code,
//   empty for none (no links). Until the user chooses one, the system's
//   territory when a shop sells there.
// - The shops (settings "suppliers/shops", PartSuppliers.hpp): the
//   suppliers' ids the user buys from, those with a shop in the region.
//   The part grid's and the size choosers' Supplier filter start with
//   them instead of All.
//
// Logged for the UI tests: "<context>: <n> of <m> articles linked in
// <region>" when articles get their links.

#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QString>
#include <QStringList>

class QPoint;
class QWidget;

namespace mitcad {

struct ShopSettings {
  QString region;
  QStringList shops;

  // The settings; the region the system's territory gives until one is kept.
  static ShopSettings load();
  // Keeps the region and those of the shops that sell in it; true when
  // the shops changed (the part grid's kept Supplier filter then goes).
  bool save() const;
  bool operator==(const ShopSettings& other) const { return region == other.region && shops == other.shops; }
  bool operator!=(const ShopSettings& other) const { return !(*this == other); }
};

// The library command `supplier_shops` for the system's locale (read once).
QJsonObject supplierShops();

// A shop as Preferences lists it: its supplier's id and name, its own name.
struct RegionShop {
  QString supplier;
  QString supplierName;
  QString name;
  QString url;
};
// The shops of `region` in the list of `supplier_shops`, in its order.
QList<RegionShop> regionShops(const QJsonObject& shops, const QString& region);

// The region chosen in Preferences, empty for none.
QString chosenRegion();

// Articles (as `supplier_parts` has them) with their `product_url` and
// `shop` in the chosen region (`supplier_links`); as they are without a
// region. `context` names them in the log line, empty for none.
QJsonArray withProductLinks(const QJsonArray& parts, const QString& context = QString());
// Parts list rows (as `parts_list` gives them) with their articles'
// links and each row's `product_urls`.
QJsonArray rowsWithProductLinks(const QJsonArray& rows);
// The product links of a parts list row (`product_urls`).
QStringList productUrls(const QJsonObject& row);

// Opens a product link: one at once, several by a menu at `at` (global
// coordinates) to choose from. Logs "Product page opened: <url>".
void openProductLinks(QWidget* parent, const QPoint& at, const QStringList& urls);

} // namespace mitcad
