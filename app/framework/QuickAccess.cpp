// SPDX-License-Identifier: MIT
#include "QuickAccess.hpp"

#include <QChar>
#include <QDebug>
#include <QMetaType>
#include <QSettings>
#include <QVariant>

namespace mitcad {
namespace {

// The longest tooltip a command's control may have (repo.tooltips).
constexpr int kTooltipLimit = 80;

// QuickAccessSettings::setTopBarDefaults.
bool g_topBarDefaults = false;

// The ids of a settings value: a string of ids joined by commas, or a list
// (an INI file's unquoted "a,b" reads back as a list).
QStringList idsOf(const QVariant& value) {
  const QStringList parts = value.metaType().id() == QMetaType::QStringList ? value.toStringList()
                                                                            : QStringList{value.toString()};
  QStringList ids;
  for (const QString& part : parts) {
    for (const QString& id : part.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
      const QString trimmed = id.trimmed();
      if (!trimmed.isEmpty() && !ids.contains(trimmed)) {
        ids.append(trimmed);
      }
    }
  }
  return ids;
}

} // namespace

QStringList QuickAccessSettings::defaults() {
  if (g_topBarDefaults) {
    // The top bar's one left group (mitcad#2129).
    return topBarItems();
  }
  // Print comes with mitcad#1089, Design Properties with mitcad#1088: a
  // build without them shows the other four.
  return {QStringLiteral("file.new_design"),       QStringLiteral("file.open"),
          QStringLiteral("file.save"),             QStringLiteral("file.print"),
          QStringLiteral("file.design_properties"), QStringLiteral("tools.preferences")};
}

void QuickAccessSettings::setTopBarDefaults(bool topBar) { g_topBarDefaults = topBar; }

bool QuickAccessSettings::topBarDefaults() { return g_topBarDefaults; }

QString QuickAccessSettings::settingsKey() { return QStringLiteral("quickAccess/commands"); }

QStringList QuickAccessSettings::topBarItems() {
  return {fileMenuId(),
          QStringLiteral("file.new_design"),
          QStringLiteral("file.open"),
          QStringLiteral("file.save"),
          QStringLiteral("file.design_properties"),
          QStringLiteral("file.print"),
          QStringLiteral("edit.undo"),
          QStringLiteral("edit.redo")};
}

QString QuickAccessSettings::fileMenuId() { return QStringLiteral("menu.file"); }

bool QuickAccessSettings::isTopBarItem(const QString& id) { return topBarItems().contains(id); }

QString QuickAccessSettings::itemsKey() { return QStringLiteral("quickAccess/items"); }

QString QuickAccessSettings::migratedKey() { return QStringLiteral("quickAccess/itemsMigrated"); }

QStringList QuickAccessSettings::migratedItems(const QStringList& defaults, const QStringList& old) {
  QStringList items = defaults;
  for (const QString& id : old) {
    if (!id.isEmpty() && !items.contains(id)) {
      items.append(id);
    }
  }
  return items;
}

QStringList QuickAccessSettings::withDefaultItem(const QStringList& items, const QString& id) {
  if (items.contains(id)) {
    return items;
  }
  const QStringList order = topBarItems();
  const qsizetype rank = order.indexOf(id);
  QStringList result = items;
  if (rank < 0) {
    result.append(id);
    return result;
  }
  qsizetype at = result.size();
  for (qsizetype i = 0; i < result.size(); ++i) {
    const qsizetype other = order.indexOf(result.at(i));
    if (other < 0 || other > rank) {
      at = i;
      break;
    }
  }
  result.insert(at, id);
  return result;
}

QuickAccessSettings QuickAccessSettings::load() {
  QuickAccessSettings loaded;
  QSettings settings;
  if (!g_topBarDefaults) {
    loaded.chosen = settings.contains(settingsKey()) ? idsOf(settings.value(settingsKey())) : defaults();
    return loaded;
  }
  // The top bar's left group (mitcad#2129): its own ordered list.
  if (settings.contains(itemsKey())) {
    loaded.chosen = idsOf(settings.value(itemsKey()));
    return loaded;
  }
  loaded.chosen = topBarItems();
  if (settings.value(migratedKey(), false).toBool() || !settings.contains(settingsKey())) {
    return loaded;
  }
  // The commands chosen before the one group: after the defaults, once.
  const QStringList old = idsOf(settings.value(settingsKey()));
  if (old.isEmpty()) {
    return loaded;
  }
  loaded.chosen = migratedItems(loaded.chosen, old);
  settings.setValue(itemsKey(), loaded.chosen.join(QLatin1Char(',')));
  settings.setValue(migratedKey(), true);
  qDebug().noquote() << QStringLiteral("Quick access migrated: %1 commands (%2)")
                            .arg(old.size())
                            .arg(old.join(QStringLiteral(", ")));
  return loaded;
}

void QuickAccessSettings::save() const {
  QSettings settings;
  settings.setValue(g_topBarDefaults ? itemsKey() : settingsKey(), chosen.join(QLatin1Char(',')));
}

void QuickAccessSettings::reset() {
  QSettings settings;
  if (g_topBarDefaults) {
    settings.remove(itemsKey());
    // The old choice is not migrated again after the reset.
    settings.setValue(migratedKey(), true);
    return;
  }
  settings.remove(settingsKey());
}

bool QuickAccessSettings::setChosen(const QString& id, bool on) {
  if (id.isEmpty() || chosen.contains(id) == on) {
    return false;
  }
  if (on && g_topBarDefaults) {
    // At the end, a default item back at its default place (mitcad#2129).
    chosen = withDefaultItem(chosen, id);
  } else if (on) {
    chosen.append(id);
  } else {
    chosen.removeAll(id);
  }
  return true;
}

bool QuickAccessSettings::isChosen(const QString& id) { return load().chosen.contains(id); }

bool QuickAccessSettings::choose(const QString& id, bool on) {
  QuickAccessSettings settings = load();
  if (!settings.setChosen(id, on)) {
    return false;
  }
  settings.save();
  qDebug().noquote()
      << QStringLiteral("Quick access %1 %2").arg(on ? QStringLiteral("added") : QStringLiteral("removed"), id);
  emit QuickAccessNotifier::instance().commandsChanged();
  return true;
}

QString QuickAccessSettings::visibleKey() { return QStringLiteral("quickAccess/visible"); }

bool QuickAccessSettings::loadVisible() { return QSettings().value(visibleKey(), true).toBool(); }

bool QuickAccessSettings::saveVisible(bool visible) {
  if (loadVisible() == visible) {
    return false;
  }
  {
    QSettings settings;
    settings.setValue(visibleKey(), visible);
  }
  emit QuickAccessNotifier::instance().visibilityChanged(visible);
  return true;
}

QuickAccessNotifier& QuickAccessNotifier::instance() {
  static QuickAccessNotifier notifier;
  return notifier;
}

QStringList quickAccessShown(const QStringList& chosen, const QStringList& available) {
  QStringList shown;
  for (const QString& id : available) {
    if (chosen.contains(id) && !shown.contains(id)) {
      shown.append(id);
    }
  }
  return shown;
}

QStringList quickAccessItems(const QStringList& items, const QStringList& available, const QStringList& rowCommands) {
  QStringList shown;
  for (const QString& id : items) {
    if (available.contains(id) && !rowCommands.contains(id) && !shown.contains(id)) {
      shown.append(id);
    }
  }
  return shown;
}

QString quickAccessLabel(const QString& text) {
  QString label;
  label.reserve(text.size());
  for (qsizetype i = 0; i < text.size(); ++i) {
    // "&&" is a literal ampersand, a single one marks the mnemonic.
    if (text.at(i) == QLatin1Char('&')) {
      if (i + 1 < text.size() && text.at(i + 1) == QLatin1Char('&')) {
        label.append(QLatin1Char('&'));
        ++i;
      }
      continue;
    }
    label.append(text.at(i));
  }
  return label.trimmed();
}

QString quickAccessTooltip(const QString& name, const QKeySequence& key, const QString& reason) {
  QString plain = quickAccessLabel(name);
  // (The single ellipsis and three dots.)
  while (plain.endsWith(QChar(0x2026)) || plain.endsWith(QLatin1Char('.'))) {
    plain.chop(1);
  }
  QString tip = plain;
  if (!key.isEmpty()) {
    tip += QStringLiteral(" (%1)").arg(key.toString(QKeySequence::NativeText));
  }
  if (!reason.isEmpty()) {
    tip += QStringLiteral(": ") + reason;
  }
  if (tip.size() > kTooltipLimit) {
    tip = tip.left(kTooltipLimit - 1).trimmed() + QChar(0x2026);
  }
  return tip;
}

} // namespace mitcad
