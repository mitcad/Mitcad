// SPDX-License-Identifier: MIT
#include "PartSuppliers.hpp"

#include <algorithm>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonValue>
#include <QSettings>
#include <QStringList>

#include "PartOptions.hpp"

namespace mitcad {
namespace {

const QString kSupplierKey = QStringLiteral("partPicker/supplierFilter");
const QString kShopsKey = QStringLiteral("suppliers/shops");

QString textOf(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

void addOnce(QList<PartSupplier>& out, const QString& id, const QString& name) {
  const QString trimmed = id.trimmed();
  if (trimmed.isEmpty()) {
    return;
  }
  const bool known = std::any_of(out.cbegin(), out.cend(), [&trimmed](const PartSupplier& one) {
    return one.id.compare(trimmed, Qt::CaseInsensitive) == 0;
  });
  if (!known) {
    out.append({trimmed, name.trimmed().isEmpty() ? trimmed : name.trimmed()});
  }
}

} // namespace

QList<PartSupplier> componentSuppliers(const QJsonObject& component) {
  QList<PartSupplier> out;
  for (const QJsonValue& value : component.value(QStringLiteral("suppliers")).toArray()) {
    const QJsonObject supplier = value.toObject();
    addOnce(out, textOf(supplier, "id"), textOf(supplier, "name"));
  }
  // The component's own supplier (a BUILD part's maker), by its name.
  addOnce(out, textOf(component, "supplier"), textOf(component, "supplier"));
  return out;
}

QList<PartSupplier> rowPartSuppliers(const QJsonObject& configurations, const QString& row,
                                     const QString& ownSupplier) {
  QList<PartSupplier> out;
  for (const QJsonValue& value : configurations.value(QStringLiteral("rows")).toArray()) {
    const QJsonObject one = value.toObject();
    if (textOf(one, "name") != row) {
      continue;
    }
    for (const QJsonValue& part : one.value(QStringLiteral("supplier_parts")).toArray()) {
      const QJsonObject article = part.toObject();
      addOnce(out, textOf(article, "supplier"), textOf(article, "supplier_name"));
    }
  }
  addOnce(out, ownSupplier, ownSupplier);
  return out;
}

QList<PartSupplier> supplierChoices(const QList<QList<PartSupplier>>& lists) {
  QList<PartSupplier> out;
  for (const QList<PartSupplier>& list : lists) {
    for (const PartSupplier& supplier : list) {
      addOnce(out, supplier.id, supplier.name);
    }
  }
  std::stable_sort(out.begin(), out.end(), [](const PartSupplier& a, const PartSupplier& b) {
    return a.name.compare(b.name, Qt::CaseInsensitive) < 0;
  });
  return out;
}

bool soldBy(const QList<PartSupplier>& suppliers, const QString& id) {
  if (supplierFilterIds(id).isEmpty()) {
    return true;
  }
  return std::any_of(suppliers.cbegin(), suppliers.cend(),
                     [&id](const PartSupplier& one) { return supplierInFilter(id, one.id); });
}

PartSupplier yourShopsChoice(const QList<PartSupplier>& suppliers, const QStringList& preferred) {
  QStringList ids;
  QStringList names;
  for (const QString& wanted : preferred) {
    for (const PartSupplier& one : suppliers) {
      if (one.id.compare(wanted.trimmed(), Qt::CaseInsensitive) == 0 && !ids.contains(one.id)) {
        ids << one.id;
        names << one.name;
      }
    }
  }
  if (ids.size() < 2) {
    return {};
  }
  return {supplierFilterOf(ids),
          QCoreApplication::translate("PartSuppliers", "Your shops (%1)").arg(names.join(QStringLiteral(", ")))};
}

QString supplierNames(const QList<PartSupplier>& suppliers) {
  QStringList names;
  for (const PartSupplier& supplier : suppliers) {
    names << supplier.name;
  }
  return names.join(QStringLiteral(", "));
}

QString supplierChoiceFor(const QList<PartSupplier>& suppliers, const QString& wanted) {
  const QStringList ids = supplierFilterIds(wanted);
  if (ids.size() < 2) {
    return wanted;
  }
  const PartSupplier yours = yourShopsChoice(suppliers, ids);
  if (!yours.id.isEmpty()) {
    return yours.id;
  }
  for (const QString& id : ids) {
    for (const PartSupplier& one : suppliers) {
      if (one.id.compare(id, Qt::CaseInsensitive) == 0) {
        return one.id;
      }
    }
  }
  return {};
}

QString savedSupplierFilter() {
  const QSettings settings;
  // All is kept as an empty text: chosen, unlike a filter never chosen.
  return settings.contains(kSupplierKey) ? settings.value(kSupplierKey).toString() : preferredSupplierFilter();
}

void saveSupplierFilter(const QString& id) { QSettings().setValue(kSupplierKey, id); }

QStringList preferredShops() {
  QStringList out;
  for (const QString& id : QSettings().value(kShopsKey).toStringList()) {
    if (!id.trimmed().isEmpty()) {
      out << id.trimmed();
    }
  }
  return out;
}

QString preferredSupplierFilter() { return supplierFilterOf(preferredShops()); }

void savePreferredShops(const QStringList& ids) {
  QSettings settings;
  if (ids.isEmpty()) {
    settings.remove(kShopsKey);
  } else {
    settings.setValue(kShopsKey, ids);
  }
  settings.remove(kSupplierKey);
}

} // namespace mitcad
