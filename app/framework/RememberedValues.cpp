// SPDX-License-Identifier: MIT
#include "RememberedValues.hpp"

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

} // namespace mitcad
