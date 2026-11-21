// SPDX-License-Identifier: MIT
#include "RibDraft.hpp"

#include <QObject>
#include <QString>

namespace mitcad::cmd {

namespace {

const QString kDraft = QStringLiteral("draft_angle");

} // namespace

InputDef ribDraftInput() {
  // As Boss's Draft Angle in name and sign: positive narrows the wall away
  // from the body. Here 0 is no draft.
  return valueInput(kDraft, QObject::tr("Draft Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
      .withTooltip(QObject::tr("0: no draft"));
}

void insertRibDraft(QVector<InputDef>& inputs) {
  for (qsizetype i = 0; i < inputs.size(); ++i) {
    if (inputs.at(i).id == QStringLiteral("thickness")) {
      inputs.insert(i + 1, ribDraftInput());
      return;
    }
  }
  inputs.append(ribDraftInput());
}

void writeRibDraft(const CommandState& state, QJsonObject& def) {
  const QString expression = state.expression(kDraft);
  const double value = state.value(kDraft);
  if (expression.isEmpty() || value == 0.0) {
    def.remove(kDraft);
    return;
  }
  def.insert(kDraft, expression);
}

} // namespace mitcad::cmd
