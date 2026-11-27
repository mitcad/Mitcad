// SPDX-License-Identifier: MIT
#pragma once

// The Automatic row of the Constraint panels' Type list
// (commands/JointCommands.cpp), header only so that app.unit reads it
// (tests/constraint_type_label.cpp).

#include <QByteArray>
#include <QCoreApplication>
#include <QObject>
#include <QString>

namespace mitcad {

// The row's label: "Automatic" while the picks give no type, else
// "Automatic (<type>)" with the inferred type in the interface language.
// `typeLabel` is the type's fixed choice label in English, as the rows
// below it give it; it is translated in the context Choice, where the
// catalogues have it (QT_TRANSLATE_NOOP in framework/CommandPanel.cpp), as
// the panel cannot translate the formatted text (choiceLabel, mitcad#2527).
inline QString automaticTypeLabel(const QString& typeLabel) {
  if (typeLabel.isEmpty()) {
    return QObject::tr("Automatic");
  }
  const QByteArray source = typeLabel.toUtf8();
  return QObject::tr("Automatic (%1)").arg(QCoreApplication::translate("Choice", source.constData()));
}

} // namespace mitcad
