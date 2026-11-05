// SPDX-License-Identifier: MIT
#include "RememberedValues.hpp"

#include <cmath>

#include <QChar>
#include <QRegularExpression>
#include <QSettings>
#include <QtGlobal>

namespace mitcad {

bool panelValuesRemembered() { return qgetenv("MITCAD_REMEMBER_PANEL_VALUES").trimmed() != "0"; }

QString rememberedValueKey(const QString& command, const QString& input) {
  return QStringLiteral("commands/%1/%2").arg(command, input);
}

QString rememberedText(const QString& text, const QString& expression) {
  QString kept = expression.trimmed().isEmpty() ? text.trimmed() : expression.trimmed();
  // `name = value` names the field's parameter (mitcad#231): only the value.
  static const QRegularExpression named(QStringLiteral(R"(^(\w+)\s*=(?!=)\s*(.*)$)"),
                                        QRegularExpression::UseUnicodePropertiesOption);
  const QRegularExpressionMatch match = named.match(kept);
  if (match.hasMatch()) {
    kept = match.captured(2).trimmed();
  }
  return kept;
}

QString rememberedValue(const QString& command, const QString& input) {
  return QSettings().value(rememberedValueKey(command, input)).toString().trimmed();
}

PanelStart panelStartText(const QString& command, const QString& input, const QString& defaultText,
                          const std::function<bool(const QString&)>& evaluates) {
  PanelStart start;
  start.text = defaultText;
  start.remembered = rememberedValue(command, input);
  if (!start.remembered.isEmpty() && (!evaluates || evaluates(start.remembered))) {
    start.text = start.remembered;
    start.fromMemory = true;
  }
  return start;
}

void rememberPanelValues(const QString& command, const QVector<QPair<QString, QString>>& fields,
                         bool committed, bool editing) {
  if (!committed || editing || command.isEmpty()) {
    return;
  }
  QSettings settings;
  for (const QPair<QString, QString>& field : fields) {
    const QString kept = field.second.trimmed();
    if (!field.first.isEmpty() && !kept.isEmpty()) {
      settings.setValue(rememberedValueKey(command, field.first), kept);
    }
  }
}

QString PanelSizeKey::text() const {
  return QStringLiteral("%1%2 %3")
      .arg(QChar(0x00D8))
      .arg(QString::number(std::round(diameter * 1000.0) / 1000.0, 'g', 12), side);
}

bool PanelSizeKey::matches(const PanelSizeKey& other) const {
  // (A hair over 0.001 mm for the rounding of the diameters' bits.)
  return !side.isEmpty() && side == other.side && std::isfinite(diameter) && std::isfinite(other.diameter) &&
         std::abs(diameter - other.diameter) <= 0.001 + 1e-9;
}

QString panelSettingsGroup(const QString& command) {
  return QStringLiteral("commands/%1/last_settings").arg(command);
}

void keepPanelSettings(const QString& command, const PanelSettings& settings, bool committed, bool editing) {
  if (!committed || editing || command.isEmpty() || settings.key.side.isEmpty() ||
      !std::isfinite(settings.key.diameter)) {
    return;
  }
  QSettings store;
  // The last OK's set replaces the one before, entries it lacks included.
  store.remove(panelSettingsGroup(command));
  store.beginGroup(panelSettingsGroup(command));
  store.setValue(QStringLiteral("key/diameter"), settings.key.diameter);
  store.setValue(QStringLiteral("key/side"), settings.key.side);
  for (auto it = settings.values.cbegin(); it != settings.values.cend(); ++it) {
    if (!it.key().isEmpty() && !it.value().trimmed().isEmpty()) {
      store.setValue(QStringLiteral("values/") + it.key(), it.value().trimmed());
    }
  }
  for (auto it = settings.choices.cbegin(); it != settings.choices.cend(); ++it) {
    if (!it.key().isEmpty()) {
      store.setValue(QStringLiteral("choices/") + it.key(), it.value());
    }
  }
  for (auto it = settings.checks.cbegin(); it != settings.checks.cend(); ++it) {
    if (!it.key().isEmpty()) {
      store.setValue(QStringLiteral("checks/") + it.key(), it.value());
    }
  }
  store.endGroup();
}

std::optional<PanelSettings> keptPanelSettings(const QString& command) {
  QSettings store;
  store.beginGroup(panelSettingsGroup(command));
  PanelSettings settings;
  bool number = false;
  settings.key.diameter = store.value(QStringLiteral("key/diameter")).toDouble(&number);
  settings.key.side = store.value(QStringLiteral("key/side")).toString();
  if (!number || settings.key.side.isEmpty()) {
    return std::nullopt;
  }
  store.beginGroup(QStringLiteral("values"));
  for (const QString& id : store.childKeys()) {
    settings.values.insert(id, store.value(id).toString());
  }
  store.endGroup();
  store.beginGroup(QStringLiteral("choices"));
  for (const QString& id : store.childKeys()) {
    settings.choices.insert(id, store.value(id).toString());
  }
  store.endGroup();
  store.beginGroup(QStringLiteral("checks"));
  for (const QString& id : store.childKeys()) {
    settings.checks.insert(id, store.value(id).toBool());
  }
  store.endGroup();
  return settings;
}

QStringList changedSettings(const PanelSettings& from, const PanelSettings& to) {
  QStringList changed;
  const auto compare = [&changed](const auto& a, const auto& b) {
    for (auto it = a.cbegin(); it != a.cend(); ++it) {
      if (!b.contains(it.key()) || b.value(it.key()) != it.value()) {
        changed.append(it.key());
      }
    }
    for (auto it = b.cbegin(); it != b.cend(); ++it) {
      if (!a.contains(it.key())) {
        changed.append(it.key());
      }
    }
  };
  compare(from.values, to.values);
  compare(from.choices, to.choices);
  compare(from.checks, to.checks);
  changed.sort();
  changed.removeDuplicates();
  return changed;
}

} // namespace mitcad
