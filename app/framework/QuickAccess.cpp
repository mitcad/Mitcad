// SPDX-License-Identifier: MIT
#include "QuickAccess.hpp"

#include <QChar>
#include <QMetaType>
#include <QSettings>
#include <QVariant>

namespace mitcad {
namespace {

// The longest tooltip a command's control may have (repo.tooltips).
constexpr int kTooltipLimit = 80;

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
  // Print comes with mitcad#1089, Design Properties with mitcad#1088: a
  // build without them shows the other four.
  return {QStringLiteral("file.new_design"),       QStringLiteral("file.open"),
          QStringLiteral("file.save"),             QStringLiteral("file.print"),
          QStringLiteral("file.design_properties"), QStringLiteral("tools.preferences")};
}

QString QuickAccessSettings::settingsKey() { return QStringLiteral("quickAccess/commands"); }

QuickAccessSettings QuickAccessSettings::load() {
  QuickAccessSettings loaded;
  const QSettings settings;
  loaded.chosen = settings.contains(settingsKey()) ? idsOf(settings.value(settingsKey())) : defaults();
  return loaded;
}

void QuickAccessSettings::save() const {
  QSettings settings;
  settings.setValue(settingsKey(), chosen.join(QLatin1Char(',')));
}

void QuickAccessSettings::reset() {
  QSettings settings;
  settings.remove(settingsKey());
}

bool QuickAccessSettings::setChosen(const QString& id, bool on) {
  if (id.isEmpty() || chosen.contains(id) == on) {
    return false;
  }
  if (on) {
    chosen.append(id);
  } else {
    chosen.removeAll(id);
  }
  return true;
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
