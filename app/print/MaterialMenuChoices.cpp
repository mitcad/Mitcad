// SPDX-License-Identifier: MIT
#include "MaterialMenuChoices.hpp"

#include <QJsonArray>
#include <QJsonValue>
#include <QSettings>
#include <QVariant>

namespace mitcad {
namespace {

const char* kRecentKey = "materials/recent";

int indexOf(const QVector<MaterialChoice>& items, const QString& key) {
  for (int i = 0; i < items.size(); ++i) {
    if (items[i].key == key) {
      return i;
    }
  }
  return -1;
}

} // namespace

MaterialMenuRows materialMenuRows(const QVector<MaterialChoice>& items, const QStringList& current,
                                  const QStringList& recent, int limit) {
  MaterialMenuRows rows;
  for (int i = 0; i < items.size(); ++i) {
    if (items[i].inDesign || current.contains(items[i].key)) {
      rows.design.append(i);
    }
  }
  for (const QString& key : recent) {
    if (rows.recent.size() >= limit) {
      break;
    }
    const int index = indexOf(items, key);
    if (index >= 0 && !rows.design.contains(index) && !rows.recent.contains(index)) {
      rows.recent.append(index);
    }
  }
  return rows;
}

QString commonMaterial(const QStringList& perTarget) {
  if (perTarget.isEmpty() || perTarget.first().isEmpty()) {
    return QString();
  }
  for (const QString& key : perTarget) {
    if (key != perTarget.first()) {
      return QString();
    }
  }
  return perTarget.first();
}

QStringList withRecentMaterial(const QStringList& recent, const QString& key, int limit) {
  QStringList list = recent;
  list.removeAll(key);
  if (!key.isEmpty()) {
    list.prepend(key);
  }
  while (list.size() > limit) {
    list.removeLast();
  }
  return list;
}

bool isLibraryPlace(const QString& place) { return !place.isEmpty() && place != QStringLiteral("design"); }

QStringList recentMaterials() {
  return QSettings().value(QLatin1String(kRecentKey)).toStringList();
}

void rememberRecentMaterial(const QString& place, const QString& key) {
  if (!isLibraryPlace(place) || key.isEmpty()) {
    return;
  }
  QSettings settings;
  settings.setValue(QLatin1String(kRecentKey), withRecentMaterial(recentMaterials(), key));
}

QStringList materialAppearances(const QJsonObject& fields) {
  QStringList list;
  const auto add = [&list](const QString& id) {
    if (!id.isEmpty() && !list.contains(id)) {
      list << id;
    }
  };
  add(fields.value(QStringLiteral("appearance")).toString());
  const QJsonArray others = fields.value(QStringLiteral("appearances")).toArray();
  for (const QJsonValue& value : others) {
    add(value.toString());
  }
  return list;
}

bool asksToReplaceAppearance(const QJsonObject& body, const QStringList& offered) {
  if (!body.value(QStringLiteral("appearance_set_by_hand")).toBool()) {
    return false;
  }
  return !offered.contains(body.value(QStringLiteral("appearance")).toString());
}

} // namespace mitcad
