// SPDX-License-Identifier: MIT
// CommandSession: a new feature's operation and objects chosen by the
// model from each preview's tool until the user picks them (mitcad#289:
// Extrude's automatic operation; CommandDef::autoOperation); an edit keeps
// its stored operation (mitcad#868, AutoOperationPolicy). The rest of the
// session is in CommandSession.cpp.
#include <utility>

#include <QDebug>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

#include "CommandSession.hpp"

namespace mitcad {
namespace {

const QString kOperation = QStringLiteral("operation");
const QString kObjects = QStringLiteral("objects");

QStringList ownersOf(const Selection& items) {
  QStringList owners;
  for (const SelectionItem& item : items) {
    owners << item.owner;
  }
  return owners;
}

} // namespace

void CommandSession::startAutoOperation() {
  const bool enabled = m_def.autoOperation && m_analysis.isEmpty() && slotOf(kOperation).has_value();
  m_autoOperation.start(enabled, isEditing());
  // Repeat (mitcad#1686) gave the operation the last OK had chosen by
  // hand: it stays, as the user's own choice.
  if (std::exchange(m_operationRepeated, false) && m_autoOperation.active()) {
    m_autoOperation.disable();
    qDebug().noquote() << QStringLiteral("%1: operation repeated as chosen by hand").arg(m_def.name);
  }
  if (enabled && isEditing()) {
    // An edit keeps the operation stored with the feature (mitcad#868):
    // only the user changes it.
    qDebug().noquote() << QStringLiteral("%1: editing keeps the stored operation").arg(m_def.name);
  }
}

void CommandSession::addAutoOperation(QJsonObject& command) const {
  if (m_autoOperation.active()) {
    command.insert(QStringLiteral("auto_operation"), true);
  }
}

bool CommandSession::takeAutoOperation(const QJsonObject& report) {
  if (!m_autoOperation.active() || m_done) {
    return false;
  }
  const QJsonObject choice = report.value(QStringLiteral("auto_operation")).toObject();
  const QString operation = choice.value(kOperation).toString();
  const auto slot = slotOf(kOperation);
  if (operation.isEmpty() || !slot) {
    return false;
  }
  // Only what the input offers (an operation it lacks leaves it as it is).
  QString label;
  for (const auto& [value, text] : slot->def->choicesIn(m_state)) {
    if (value == operation) {
      label = text;
    }
  }
  if (label.isEmpty()) {
    return false;
  }
  // The bodies in the feature's component, placed where its first input's
  // items are (a sketch in a component placed off the origin).
  QString occurrence;
  if (const InputDef* primary = m_def.primarySelection()) {
    const Selection& first = m_state.items(primary->id);
    if (!first.isEmpty()) {
      occurrence = first.first().occurrence;
    }
  }
  Selection objects;
  for (const QJsonValue& uid : choice.value(kObjects).toArray()) {
    objects.append({SelectKind::Body, uid.toString(), QString(), QString(), occurrence});
  }
  const bool hasObjects = slotOf(kObjects).has_value();
  if (m_state.choice(kOperation) == operation &&
      (!hasObjects || ownersOf(m_state.items(kObjects)) == ownersOf(objects))) {
    return false;
  }
  const bool changedOperation = m_state.choice(kOperation) != operation;
  m_state.setChoice(kOperation, operation);
  // What the UI tests read: the operation the model chose.
  qDebug().noquote() << QStringLiteral("%1: automatic %2 = %3").arg(m_def.name, slot->def->label, label);
  if (hasObjects) {
    m_state.setItems(kObjects, objects);
    logSelection(kObjects);
  }
  if (changedOperation && m_def.autoOperationApplied) {
    m_def.autoOperationApplied(m_state, m_host);
  }
  if (m_presenter) {
    m_presenter->refresh(m_state);
  }
  updateHighlights();
  return true;
}

void CommandSession::autoOperationOverruled(const QString& key) {
  const auto slot = slotOf(key);
  const bool targets = slot && slot->def->type == InputDef::Type::CutTargets;
  if (!m_autoOperation.active() || (key != kOperation && key != kObjects && !targets)) {
    return;
  }
  // The user's own choice stays for the rest of the command.
  m_autoOperation.disable();
  qDebug().noquote() << QStringLiteral("%1: operation chosen by hand").arg(m_def.name);
}

} // namespace mitcad
