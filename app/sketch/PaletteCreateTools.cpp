// SPDX-License-Identifier: MIT
#include "PaletteCreateTools.hpp"

#include <algorithm>
#include <utility>

#include <QSettings>

namespace mitcad::sketch {

const QStringList& paletteCreateDefaults() {
  static const QStringList defaults = {
      QStringLiteral("sketch.line"),    QStringLiteral("sketch.rectangle"), QStringLiteral("sketch.circle"),
      QStringLiteral("sketch.arc"),     QStringLiteral("sketch.polygon"),   QStringLiteral("sketch.slot"),
      QStringLiteral("sketch.spline"),  QStringLiteral("sketch.point"),     QStringLiteral("sketch.trim"),
      QStringLiteral("sketch.extend"),  QStringLiteral("sketch.offset"),    QStringLiteral("sketch.fillet"),
      QStringLiteral("sketch.mirror"),  QStringLiteral("sketch.dimension")};
  return defaults;
}

PaletteCreateChoice choosePaletteCreateTools(const QStringList& ribbon, const QStringList& pinned,
                                             const QHash<QString, int>& counts, int limit) {
  PaletteCreateChoice choice;
  const auto take = [&ribbon, &choice, limit](const QString& id) {
    if (static_cast<int>(choice.ids.size()) < limit && ribbon.contains(id) && !choice.ids.contains(id)) {
      choice.ids.append(id);
    }
  };
  for (const QString& id : pinned) {
    take(id);
  }
  // The counted tools, the most started first; a tie in the ribbon's order.
  QList<std::pair<int, qsizetype>> counted; // count, place in the ribbon
  for (qsizetype i = 0; i < ribbon.size(); ++i) {
    const int count = counts.value(ribbon[i], 0);
    if (count > 0) {
      counted.append({count, i});
    }
  }
  std::stable_sort(counted.begin(), counted.end(),
                   [](const std::pair<int, qsizetype>& a, const std::pair<int, qsizetype>& b) {
                     return a.first > b.first;
                   });
  choice.fromCounts = !counted.isEmpty();
  for (const std::pair<int, qsizetype>& entry : counted) {
    take(ribbon[entry.second]);
  }
  for (const QString& id : paletteCreateDefaults()) {
    take(id);
  }
  std::sort(choice.ids.begin(), choice.ids.end(),
            [&ribbon](const QString& a, const QString& b) { return ribbon.indexOf(a) < ribbon.indexOf(b); });
  return choice;
}

QString paletteToolCountsGroup() { return QStringLiteral("sketchToolCounts"); }

QHash<QString, int> paletteToolCounts() {
  QSettings settings;
  settings.beginGroup(paletteToolCountsGroup());
  QHash<QString, int> counts;
  for (const QString& id : settings.childKeys()) {
    bool ok = false;
    const int count = settings.value(id).toInt(&ok);
    if (ok && count > 0) {
      counts.insert(id, count);
    }
  }
  return counts;
}

void countPaletteTool(const QString& id) {
  QSettings settings;
  const QString key = paletteToolCountsGroup() + QLatin1Char('/') + id;
  settings.setValue(key, settings.value(key, 0).toInt() + 1);
}

QString paletteToolName(const QString& id) {
  const QString prefix = QStringLiteral("sketch.");
  return id.startsWith(prefix) ? id.mid(prefix.size()) : id;
}

} // namespace mitcad::sketch
