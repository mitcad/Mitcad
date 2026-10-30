// SPDX-License-Identifier: MIT
#include "PartSuppliers.hpp"

#include <algorithm>

#include <QJsonArray>
#include <QJsonValue>
#include <QSettings>
#include <QStringList>

namespace mitcad {
namespace {

const QString kSupplierKey = QStringLiteral("partPicker/supplierFilter");

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
  if (id.isEmpty()) {
    return true;
  }
  return std::any_of(suppliers.cbegin(), suppliers.cend(), [&id](const PartSupplier& one) {
    return one.id.compare(id, Qt::CaseInsensitive) == 0;
  });
}

QString supplierNames(const QList<PartSupplier>& suppliers) {
  QStringList names;
  for (const PartSupplier& supplier : suppliers) {
    names << supplier.name;
  }
  return names.join(QStringLiteral(", "));
}

QString savedSupplierFilter() { return QSettings().value(kSupplierKey).toString(); }

void saveSupplierFilter(const QString& id) {
  QSettings settings;
  if (id.isEmpty()) {
    settings.remove(kSupplierKey);
  } else {
    settings.setValue(kSupplierKey, id);
  }
}

} // namespace mitcad
