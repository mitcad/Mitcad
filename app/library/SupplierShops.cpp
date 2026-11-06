// SPDX-License-Identifier: MIT
#include "SupplierShops.hpp"

#include <QAction>
#include <QJsonValue>
#include <QLocale>
#include <QMenu>
#include <QPoint>
#include <QSettings>
#include <QUrl>
#include <QtLogging>

#include "../files/Libraries.hpp"
#include "../framework/Dialogs.hpp"
#include "../report/ReportCenter.hpp"
#include "PartSuppliers.hpp"

namespace mitcad {
namespace {

const QString kRegionKey = QStringLiteral("suppliers/region");

QString textOf(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

} // namespace

QJsonObject supplierShops() {
  static const QJsonObject shops = libraryCommand(
      {{QStringLiteral("cmd"), QStringLiteral("supplier_shops")}, {QStringLiteral("locale"), QLocale::system().name()}});
  return shops;
}

ShopSettings ShopSettings::load() {
  ShopSettings out;
  const QSettings settings;
  out.region = settings.contains(kRegionKey) ? settings.value(kRegionKey).toString().trimmed()
                                             : textOf(supplierShops(), "default_region");
  out.shops = preferredShops();
  return out;
}

bool ShopSettings::save() const {
  // Only the shops that sell in the region (the model's list decides).
  const QJsonObject answer =
      libraryCommand({{QStringLiteral("cmd"), QStringLiteral("supplier_shops")},
                      {QStringLiteral("region"), region},
                      {QStringLiteral("shops"), QJsonArray::fromStringList(shops)}});
  QStringList chosen;
  for (const QJsonValue& id : answer.value(QStringLiteral("chosen")).toArray()) {
    chosen << id.toString();
  }
  QSettings().setValue(kRegionKey, region);
  if (chosen == preferredShops()) {
    return false;
  }
  savePreferredShops(chosen);
  return true;
}

QList<RegionShop> regionShops(const QJsonObject& shops, const QString& region) {
  QList<RegionShop> out;
  if (region.trimmed().isEmpty()) {
    return out;
  }
  for (const QJsonValue& value : shops.value(QStringLiteral("suppliers")).toArray()) {
    const QJsonObject supplier = value.toObject();
    for (const QJsonValue& entry : supplier.value(QStringLiteral("shops")).toArray()) {
      const QJsonObject shop = entry.toObject();
      if (textOf(shop, "region").compare(region.trimmed(), Qt::CaseInsensitive) == 0) {
        out.append({textOf(supplier, "id"), textOf(supplier, "name"), textOf(shop, "name"), textOf(shop, "url")});
      }
    }
  }
  return out;
}

QString chosenRegion() { return ShopSettings::load().region; }

QJsonArray withProductLinks(const QJsonArray& parts, const QString& context) {
  const QString region = chosenRegion();
  if (parts.isEmpty() || region.isEmpty()) {
    return parts;
  }
  const QJsonObject answer = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("supplier_links")},
                                             {QStringLiteral("region"), region},
                                             {QStringLiteral("parts"), parts}});
  const QJsonArray linked = answer.value(QStringLiteral("parts")).toArray();
  if (linked.size() != parts.size()) {
    const QJsonObject error = answer.value(QStringLiteral("error")).toObject();
    qWarning().noquote() << QStringLiteral("Supplier links not made: %1").arg(textOf(error, "message"));
    return parts;
  }
  if (!context.isEmpty()) {
    qsizetype count = 0;
    for (const QJsonValue& value : linked) {
      if (!textOf(value.toObject(), "product_url").isEmpty()) {
        ++count;
      }
    }
    qInfo().noquote() << QStringLiteral("%1: %2 of %3 articles linked in %4")
                             .arg(context)
                             .arg(count)
                             .arg(linked.size())
                             .arg(region);
  }
  return linked;
}

QJsonArray rowsWithProductLinks(const QJsonArray& rows) {
  const QString region = chosenRegion();
  if (rows.isEmpty() || region.isEmpty()) {
    return rows;
  }
  const QJsonObject answer = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("supplier_links")},
                                             {QStringLiteral("region"), region},
                                             {QStringLiteral("rows"), rows}});
  const QJsonArray linked = answer.value(QStringLiteral("rows")).toArray();
  return linked.size() == rows.size() ? linked : rows;
}

QStringList productUrls(const QJsonObject& row) {
  QStringList out;
  for (const QJsonValue& url : row.value(QStringLiteral("product_urls")).toArray()) {
    if (!url.toString().isEmpty()) {
      out << url.toString();
    }
  }
  return out;
}

void openProductLinks(QWidget* parent, const QPoint& at, const QStringList& urls) {
  const auto open = [](const QString& url) {
    qInfo().noquote() << QStringLiteral("Product page opened: %1").arg(url);
    openExternalUrl(QUrl(url));
  };
  if (urls.size() == 1) {
    open(urls.first());
    return;
  }
  if (urls.isEmpty()) {
    return;
  }
  QMenu menu(parent);
  menu.setObjectName(QStringLiteral("productLinksMenu"));
  for (const QString& url : urls) {
    QAction* action = menu.addAction(url);
    QObject::connect(action, &QAction::triggered, &menu, [open, url] { open(url); });
  }
  execMenu(&menu, at);
}

} // namespace mitcad
