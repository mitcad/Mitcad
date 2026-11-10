// SPDX-License-Identifier: MIT
#include "UsedParts.hpp"
#include "framework/ModelMessages.hpp"

#include <exception>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QtLogging>

#include "../framework/Command.hpp"

namespace mitcad {

namespace {

QString componentId(const QString& library, const QString& component) {
  return library + QLatin1Char('|') + component;
}

} // namespace

void UsedCounts::set(const QList<UsedPart>& used) {
  m_components.clear();
  m_rows.clear();
  m_unrowed.clear();
  for (const UsedPart& part : used) {
    if (part.count <= 0) {
      continue;
    }
    const QString id = componentId(part.library, part.component);
    m_components[id] += part.count;
    int inRows = 0;
    for (const UsedPart::Row& row : part.rows) {
      m_rows[id + QLatin1Char('|') + row.config] += row.count;
      inRows += row.count;
    }
    if (part.count > inRows) {
      m_unrowed[id] += part.count - inRows;
    }
  }
}

int UsedCounts::of(const QString& library, const QString& component, const QString& config,
                   bool defaultRow) const {
  const QString id = componentId(library, component);
  if (config.isEmpty()) {
    return m_components.value(id);
  }
  int uses = m_rows.value(id + QLatin1Char('|') + config);
  if (defaultRow) {
    uses += m_unrowed.value(id);
  }
  return uses;
}

QList<UsedPart> usedLibraryParts(const CommandContext& context) {
  QList<UsedPart> parts;
  QJsonObject answer;
  try {
    answer = context.query(QJsonObject{{QStringLiteral("query"), QStringLiteral("library_usage")}}).toObject();
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Part picker: the design's library parts not read: %1")
                                .arg(mitcad::modelMessage(error.what()));
    return parts;
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("parts")).toArray()) {
    const QJsonObject entry = value.toObject();
    UsedPart part;
    part.library = entry.value(QStringLiteral("library")).toString();
    part.url = entry.value(QStringLiteral("url")).toString();
    part.component = entry.value(QStringLiteral("component")).toString();
    part.count = entry.value(QStringLiteral("count")).toInt();
    if (part.library.isEmpty() || part.component.isEmpty() || part.count <= 0) {
      continue;
    }
    for (const QJsonValue& rowValue : entry.value(QStringLiteral("rows")).toArray()) {
      const QJsonObject rowEntry = rowValue.toObject();
      UsedPart::Row row;
      row.config = rowEntry.value(QStringLiteral("config")).toString();
      row.count = rowEntry.value(QStringLiteral("count")).toInt();
      if (!row.config.isEmpty() && row.count > 0) {
        part.rows.append(row);
      }
    }
    parts.append(part);
  }
  return parts;
}

} // namespace mitcad
