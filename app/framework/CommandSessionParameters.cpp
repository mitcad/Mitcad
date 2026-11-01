// SPDX-License-Identifier: MIT
// Change Parameters while a command's panel is open (mitcad#564): the
// parameter table's commands run outside the command's undo range, as the
// browser's light bulbs do (CommandSessionVisibility.cpp), so that Cancel
// keeps them and OK adds or edits the feature after them; then the panel
// follows the parameters as they are now.
//
// The panel keeps what the user typed and picked: its expressions are the
// same texts (a rename goes into those that use the old name), so the
// previews cached by their definitions are of the parameters before and
// are dropped; every value input is evaluated again (the values and the
// hints under the fields), the selections are resolved again (a profile
// whose sketch follows a parameter) and the preview is computed again.
#include "CommandSession.hpp"

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QtLogging>

#include "ParameterEdits.hpp"

namespace mitcad {

bool CommandSession::runParameter(const QJsonObject& command, QString& refusal) {
  refusal.clear();
  return runOutsideRange(QJsonArray{command}, tr("Edit Parameters"), true, &refusal);
}

void CommandSession::parametersChanged(const QJsonObject& command, bool done, bool sketchPreviewTakenBack) {
  if (m_done) {
    return;
  }
  if (!done || !parameterCommandChangesValues(command)) {
    // Refused, or a comment or a star: the inputs mean what they meant.
    if (sketchPreviewTakenBack) {
      schedulePreview(); // applied again, after the change
    }
    return;
  }
  const QString name = command.value(QStringLiteral("name")).toString();
  // A table's name is a name of the parameters' namespace (mitcad#667):
  // `Screws.dk` follows a renamed Screws as a parameter's name does.
  if (command.value(QStringLiteral("cmd")).toString() == QStringLiteral("rename_parameter") ||
      command.value(QStringLiteral("cmd")).toString() == QStringLiteral("rename_table")) {
    const QString renamedTo = command.value(QStringLiteral("new_name")).toString();
    for (const Slot& slot : slotsOf(false)) {
      if (slot.def->type != InputDef::Type::Value) {
        continue;
      }
      const QString text = m_state.text(slot.key);
      const QString renamed = renameParameterIn(text, name, renamedTo);
      if (renamed != text) {
        m_state.setText(slot.key, renamed);
        qDebug().noquote() << QStringLiteral("%1 %2: %3 becomes %4 (%5 renamed)")
                                  .arg(m_def.name, slot.key, text, renamed, name);
      }
    }
  }
  ++m_generation;
  m_parameterComments.clear(); // the tooltips' parameters are asked again
  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type == InputDef::Type::Value) {
      evaluate(slot.key);
    }
  }
  canonicalizeSelections();
  // The same definitions mean other shapes now.
  m_cache.clear();
  m_cacheOrder.clear();
  m_shownKey.clear();
  qDebug().noquote() << QStringLiteral("%1: parameter %2 changed; the preview follows").arg(m_def.name, name);
  updateHighlights();
  inputsChanged();
}

} // namespace mitcad
