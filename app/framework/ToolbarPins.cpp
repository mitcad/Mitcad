// SPDX-License-Identifier: MIT
#include "ToolbarPins.hpp"

#include <algorithm>
#include <utility>

#include <QMetaType>
#include <QSettings>
#include <QVariant>
#include <QtLogging>

namespace mitcad {
namespace {

const QString kGroup = QStringLiteral("toolbarPins");

// The ids of a settings value: a string of ids joined by commas, or a list
// (an INI file's unquoted "a,b", a platform store's string list).
QStringList idsOf(const QVariant& value) {
  const QStringList parts = value.metaType().id() == QMetaType::QStringList ? value.toStringList()
                                                                            : QStringList{value.toString()};
  QStringList ids;
  for (const QString& part : parts) {
    for (const QString& id : part.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
      const QString trimmed = id.trimmed();
      if (!trimmed.isEmpty()) {
        ids.append(trimmed);
      }
    }
  }
  return ids;
}

} // namespace

QString ToolbarPins::settingsKey(const QString& tab) { return kGroup + QLatin1Char('/') + tab; }

ToolbarPins ToolbarPins::load(const Resolve& resolve) {
  ToolbarPins pins;
  QStringList tabs;
  {
    QSettings settings;
    settings.beginGroup(kGroup);
    tabs = settings.childKeys();
  }
  for (const QString& tab : std::as_const(tabs)) {
    const QStringList stored = idsOf(QSettings().value(settingsKey(tab)));
    QStringList kept;
    bool changed = false;
    for (const QString& id : stored) {
      const QString command = resolve ? resolve(id) : id;
      if (command.isEmpty()) {
        qInfo().noquote() << QStringLiteral("Toolbar pins: dropped %1 from %2: no such command").arg(id, tab);
        changed = true;
        continue;
      }
      if (command != id || kept.contains(command)) {
        changed = true;
      }
      if (!kept.contains(command)) {
        kept.append(command);
      }
    }
    if (!kept.isEmpty()) {
      pins.m_pins.insert(tab, kept);
    }
    if (changed || kept.isEmpty()) {
      pins.save(tab);
    }
  }
  return pins;
}

bool ToolbarPins::isPinned(const QString& tab, const QString& id) const {
  return m_pins.value(tab).contains(id);
}

bool ToolbarPins::pin(const QString& tab, const QString& id) {
  if (id.isEmpty() || isPinned(tab, id)) {
    return false;
  }
  m_pins[tab].append(id);
  save(tab);
  return true;
}

bool ToolbarPins::unpin(const QString& tab, const QString& id) {
  const auto found = m_pins.find(tab);
  if (found == m_pins.end() || !found->removeOne(id)) {
    return false;
  }
  if (found->isEmpty()) {
    m_pins.erase(found);
  }
  save(tab);
  return true;
}

bool ToolbarPins::move(const QString& tab, const QString& id, int index) {
  const auto found = m_pins.find(tab);
  if (found == m_pins.end()) {
    return false;
  }
  const int from = static_cast<int>(found->indexOf(id));
  if (from < 0) {
    return false;
  }
  const int to = std::clamp(index, 0, static_cast<int>(found->size()) - 1);
  if (to == from) {
    return false;
  }
  found->move(from, to);
  save(tab);
  return true;
}

void ToolbarPins::save(const QString& tab) const {
  QSettings settings;
  const QStringList ids = m_pins.value(tab);
  if (ids.isEmpty()) {
    settings.remove(settingsKey(tab));
  } else {
    settings.setValue(settingsKey(tab), ids.join(QLatin1Char(',')));
  }
}

} // namespace mitcad
