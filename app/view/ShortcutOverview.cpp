// SPDX-License-Identifier: MIT
#include "ShortcutOverview.hpp"

#include <QHash>
#include <QObject>
#include <QStringList>

#include "../framework/CommandRegistry.hpp"

namespace mitcad {

QString overviewKeysText(const QList<QKeySequence>& keys) {
  QStringList labels;
  bool sequence = false;
  for (const QKeySequence& key : keys) {
    labels.append(key.toString(QKeySequence::NativeText));
    sequence = sequence || key.count() > 1;
  }
  return labels.join(sequence ? QStringLiteral("; ") : QStringLiteral(", "));
}

QVector<OverviewGroup> commandShortcutGroups(const CommandRegistry& registry,
                                             const std::function<QString(const QString& id)>& groupOf) {
  QVector<OverviewGroup> groups;
  QHash<QString, int> index;
  for (const auto& def : registry.commands()) {
    const QList<QKeySequence> keys = registry.effectiveShortcuts(def->id);
    if (keys.isEmpty()) {
      continue;
    }
    const QString caption = groupOf ? groupOf(def->id) : QString();
    auto found = index.constFind(caption);
    if (found == index.cend()) {
      found = index.insert(caption, static_cast<int>(groups.size()));
      groups.append({caption, {}});
    }
    groups[found.value()].rows.append({overviewKeysText(keys), def->name, def->id, def->icon});
  }
  return groups;
}

bool overviewRowMatches(const OverviewRow& row, const QString& filter) {
  return filter.isEmpty() || row.keys.contains(filter, Qt::CaseInsensitive) ||
         row.does.contains(filter, Qt::CaseInsensitive);
}

CheatSheet shortcutCheatSheet(const QVector<OverviewGroup>& groups, const QString& filter, const QString& scheme,
                              const QDate& date) {
  CheatSheet sheet;
  sheet.title = QObject::tr("Keyboard and Mouse Overview");
  // The parts set apart by " · ": the scheme's name has commas of its own
  // ("Middle pans, Shift + middle orbits"), and a sheet of a search says
  // which search it is (mitcad#2096).
  QStringList about{QObject::tr("Mouse: %1").arg(scheme)};
  if (!filter.isEmpty()) {
    about.append(QObject::tr("Search: %1").arg(filter));
  }
  about.append(date.toString(Qt::ISODate));
  sheet.subtitle = about.join(QStringLiteral(" · "));
  const QStringList columns{QObject::tr("Keys or mouse"), QObject::tr("Does")};
  for (const OverviewGroup& group : groups) {
    build::ListTable table{group.caption, columns, {}};
    table.nameColumn = 1;
    for (const OverviewRow& row : group.rows) {
      if (overviewRowMatches(row, filter)) {
        table.rows.append({row.keys, row.does});
      }
    }
    if (!table.rows.isEmpty()) {
      sheet.tables.append(table);
    }
  }
  return sheet;
}

QString cheatSheetFileName() { return QObject::tr("MitCad keyboard shortcuts") + QStringLiteral(".pdf"); }

} // namespace mitcad
