// SPDX-License-Identifier: MIT
#include "PrintValueChoices.hpp"

#include <QHash>
#include <QJsonArray>
#include <QStringList>

namespace mitcad {
namespace {

QString textOf(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

} // namespace

QString printGroupHeading(const PrintCategory& category, const PrintGroup& group) {
  return category.title == group.title || group.title.isEmpty()
             ? category.title
             : QStringLiteral("%1 › %2").arg(category.title, group.title);
}

QVector<PrintValueChoice> printValueChoices(const PrintKeyTable& keys, const QJsonObject& settings) {
  QVector<PrintValueChoice> choices;
  for (const char* set : {"process", "printer"}) {
    const QString setName = QString::fromLatin1(set);
    // What the body prints with now, by key.
    QHash<QString, QJsonObject> byKey;
    for (const QJsonValue& value :
         settings.value(setName).toObject().value(QStringLiteral("values")).toArray()) {
      const QJsonObject entry = value.toObject();
      byKey.insert(textOf(entry, "key"), entry);
    }
    const QVector<const PrintKey*> setKeys = keys.keys(setName);
    for (const PrintCategory& category : keys.layout(setName)) {
      for (const PrintGroup& group : category.groups) {
        for (const PrintKey* key : setKeys) {
          if (key->perDialect || key->key == QStringLiteral("filament_density") ||
              keys.categoryOf(*key) != category.id || keys.groupOf(*key) != group.id) {
            continue;
          }
          const QJsonObject entry = byKey.value(key->key);
          if (textOf(entry, "from") == QLatin1String("property") && textOf(entry, "level") == QLatin1String("body")) {
            continue; // the body's own already: its row is on the card
          }
          PrintValueChoice choice;
          choice.set = setName;
          choice.key = key->key;
          choice.name = key->name;
          choice.heading = printGroupHeading(category, group);
          choice.value = entry.value(QStringLiteral("value"));
          if (!choice.value.isUndefined() && !choice.value.isNull()) {
            const QString shown = textOf(entry, "text");
            choice.text = shown.isEmpty() ? printValueText(*key, choice.value) : shown;
          } else {
            choice.value = QJsonValue(QJsonValue::Undefined);
          }
          choices.append(choice);
        }
      }
    }
  }
  return choices;
}

bool printValueMatches(const PrintValueChoice& choice, const QString& search) {
  const QStringList words = search.simplified().split(QLatin1Char(' '), Qt::SkipEmptyParts);
  for (const QString& word : words) {
    if (!choice.name.contains(word, Qt::CaseInsensitive) && !choice.key.contains(word, Qt::CaseInsensitive)) {
      return false;
    }
  }
  return true;
}

int printValueBestMatch(const QVector<PrintValueChoice>& choices, const QString& search) {
  const QString wanted = search.simplified();
  int first = -1;
  for (int i = 0; i < static_cast<int>(choices.size()); ++i) {
    if (!printValueMatches(choices.at(i), wanted)) {
      continue;
    }
    if (!wanted.isEmpty() && choices.at(i).name.compare(wanted, Qt::CaseInsensitive) == 0) {
      return i;
    }
    if (first < 0) {
      first = i;
    }
  }
  return first;
}

} // namespace mitcad
