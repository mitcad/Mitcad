// SPDX-License-Identifier: MIT
#include "ToolbarPins.hpp"

#include <algorithm>
#include <utility>
#include <vector>

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
  pins.m_resolve = resolve;
  QStringList tabs;
  {
    QSettings settings;
    settings.beginGroup(kGroup);
    tabs = settings.childKeys();
  }
  // The tabs whose settings are written again.
  QStringList changed;
  const auto change = [&changed](const QString& tab) {
    if (!changed.contains(tab)) {
      changed.append(tab);
    }
  };
  // The commands kept under another tab than their own: (own tab, id),
  // added at the end of their own tab's pins once all are read.
  std::vector<std::pair<QString, QString>> moved;
  for (const QString& tab : std::as_const(tabs)) {
    const QStringList stored = idsOf(QSettings().value(settingsKey(tab)));
    if (stored.isEmpty()) {
      change(tab); // an empty value: the key goes
    }
    for (const QString& id : stored) {
      const Command command = resolve ? resolve(id) : Command{id, tab, QString()};
      if (command.id.isEmpty()) {
        qInfo().noquote() << QStringLiteral("Toolbar pins: dropped %1 from %2: no such command").arg(id, tab);
        change(tab);
        continue;
      }
      if (command.id != id) {
        change(tab);
      }
      const QString own = command.tab.isEmpty() ? tab : command.tab;
      if (own != tab) {
        qInfo().noquote() << QStringLiteral("Toolbar pins: moved %1 from %2 to %3").arg(command.id, tab, own);
        change(tab);
        change(own);
        moved.emplace_back(own, command.id);
        continue;
      }
      QStringList& kept = pins.m_pins[tab];
      if (kept.contains(command.id)) {
        change(tab);
        continue;
      }
      kept.append(command.id);
    }
  }
  for (const auto& [own, id] : moved) {
    QStringList& kept = pins.m_pins[own];
    if (!kept.contains(id)) {
      kept.append(id);
    }
  }
  for (const QString& tab : std::as_const(changed)) {
    pins.save(tab);
  }
  return pins;
}

QStringList ToolbarPins::pins(const QString& tab, const QString& group) const {
  QStringList ids;
  if (!m_resolve) {
    return ids;
  }
  for (const QString& id : m_pins.value(tab)) {
    const Command command = m_resolve(id);
    if (!command.id.isEmpty() && command.group == group && (command.tab.isEmpty() || command.tab == tab)) {
      ids.append(id);
    }
  }
  return ids;
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

bool ToolbarPins::moveInGroup(const QString& tab, const QString& id, int index) {
  if (!m_resolve) {
    return false;
  }
  const Command command = m_resolve(id);
  if (command.id.isEmpty()) {
    return false;
  }
  // The group's other pins; the command goes before the one at `index`,
  // or after the last of them.
  QStringList others = pins(tab, command.group);
  if (!others.removeOne(id) || others.isEmpty()) {
    return false;
  }
  QStringList without = m_pins.value(tab);
  without.removeOne(id);
  const int at = std::clamp(index, 0, static_cast<int>(others.size()));
  const int to = at < static_cast<int>(others.size()) ? static_cast<int>(without.indexOf(others.at(at)))
                                                      : static_cast<int>(without.indexOf(others.last())) + 1;
  return move(tab, id, to);
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
