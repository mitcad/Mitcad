// SPDX-License-Identifier: MIT
#include "ToolbarPins.hpp"

#include <algorithm>
#include <iterator>
#include <utility>
#include <vector>

#include <QtLogging>

#include "CommandLayout.hpp"

namespace mitcad {

ToolbarPins ToolbarPins::load(const Resolve& resolve, CommandLayoutStore* store) {
  ToolbarPins pins;
  pins.m_resolve = resolve;
  pins.m_store = store != nullptr ? store : &CommandLayoutStore::instance();
  const QStringList tabs = pins.m_store->pinTabs();
  // The tabs whose pins are saved again.
  QStringList changed;
  const auto change = [&changed](const QString& tab) {
    if (!changed.contains(tab)) {
      changed.append(tab);
    }
  };
  // The commands kept under another tab than their own: (own tab, id),
  // added at the end of their own tab's pins once all are read.
  std::vector<std::pair<QString, QString>> moved;
  for (const QString& tab : tabs) {
    const QStringList stored = pins.m_store->pins(tab);
    if (stored.isEmpty()) {
      change(tab);
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
  // A tab whose every pin was dropped has no list.
  for (auto it = pins.m_pins.begin(); it != pins.m_pins.end();) {
    it = it->isEmpty() ? pins.m_pins.erase(it) : std::next(it);
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
  if (m_store != nullptr) {
    m_store->setPins(tab, m_pins.value(tab));
  }
}

} // namespace mitcad
