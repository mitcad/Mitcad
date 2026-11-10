// SPDX-License-Identifier: MIT
#include "NavigationBarPins.hpp"

#include <algorithm>

#include <QMetaType>
#include <QSettings>
#include <QVariant>
#include <QtLogging>

namespace mitcad {
namespace {

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

QString NavigationBarPins::settingsKey() { return QStringLiteral("navigationBarPins"); }

QStringList NavigationBarPins::defaults() { return {QStringLiteral("solid.parameters")}; }

NavigationBarPins NavigationBarPins::load(const Resolve& resolve) {
  NavigationBarPins pins;
  QSettings settings;
  const bool stored = settings.contains(settingsKey());
  const QStringList ids = stored ? idsOf(settings.value(settingsKey())) : defaults();
  bool changed = false;
  for (const QString& id : ids) {
    const QString command = resolve ? resolve(id) : id;
    if (command.isEmpty()) {
      if (stored) {
        qInfo().noquote() << QStringLiteral("Navigation bar pins: dropped %1: no such command").arg(id);
        changed = true;
      }
      continue;
    }
    if (command != id || pins.m_pins.contains(command)) {
      changed = true;
    }
    if (!pins.m_pins.contains(command)) {
      pins.m_pins.append(command);
    }
  }
  // The defaults are not written: they stay the defaults until the user
  // changes the pins.
  if (stored && changed) {
    pins.save();
  }
  return pins;
}

bool NavigationBarPins::pin(const QString& id) {
  if (id.isEmpty() || isPinned(id)) {
    return false;
  }
  m_pins.append(id);
  save();
  return true;
}

bool NavigationBarPins::unpin(const QString& id) {
  if (!m_pins.removeOne(id)) {
    return false;
  }
  save();
  return true;
}

bool NavigationBarPins::move(const QString& id, int index) {
  const int from = static_cast<int>(m_pins.indexOf(id));
  if (from < 0) {
    return false;
  }
  const int to = std::clamp(index, 0, static_cast<int>(m_pins.size()) - 1);
  if (to == from) {
    return false;
  }
  m_pins.move(from, to);
  save();
  return true;
}

void NavigationBarPins::save() const {
  // Also without pins: an empty value, so that the defaults do not come
  // back.
  QSettings().setValue(settingsKey(), m_pins.join(QLatin1Char(',')));
}

} // namespace mitcad
