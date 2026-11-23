// SPDX-License-Identifier: MIT
// CommandSession: a new feature's operation and objects chosen by the
// model from each preview's tool until the user picks them (mitcad#289:
// Extrude's automatic operation; CommandDef::autoOperation); an edit keeps
// its stored operation (mitcad#868, AutoOperationPolicy); Preferences ›
// General › Extrude starts with: New Body keeps a new one's New Body
// (mitcad#2164, ExtrudeStartPreference); a new feature's direction flip
// whose preview fails with an operation chosen by hand tries the model's
// other one (mitcad#2216); Cut or Join chosen by hand that misses one way
// and works the other turns Flip once (mitcad#2283). The rest of the
// session is in CommandSession.cpp.
#include <optional>
#include <utility>

#include <QDebug>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

#include "CommandSession.hpp"
#include "ExtrudeStartPreference.hpp"

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

// Works as acceptFeaturePreview draws it: an ok status, and a cut's
// targets that can be committed.
bool previewWorks(const QJsonObject& report) {
  bool works = report.value(QStringLiteral("status")).toString() == QStringLiteral("ok");
  const QJsonObject targets = report.value(QStringLiteral("cut_targets")).toObject();
  if (!targets.isEmpty()) {
    works = works && targets.value(QStringLiteral("can_commit")).toBool();
  }
  return works;
}

} // namespace

void CommandSession::startAutoOperation() {
  const bool enabled = m_def.autoOperation && m_analysis.isEmpty() && slotOf(kOperation).has_value();
  // Preferences › General › Extrude starts with (mitcad#2164): New Body
  // keeps the panel's New Body and lets the model choose nothing.
  m_autoOperation.start(enabled, isEditing(), enabled ? savedExtrudeStart() : NewFeatureOperation::Automatic);
  if (m_autoOperation.offByPreference()) {
    qDebug().noquote() << QStringLiteral("%1: automatic operation off by preference").arg(m_def.name);
  }
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
  // The automatic choice, or a flip whose preview may fail (mitcad#2216).
  if (m_autoOperation.asksSuggestion()) {
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
  const Selection objects = suggestedObjects(choice);
  const bool hasObjects = slotOf(kObjects).has_value();
  if (m_state.choice(kOperation) == operation &&
      (!hasObjects || ownersOf(m_state.items(kObjects)) == ownersOf(objects))) {
    return false;
  }
  // What the UI tests read: the operation the model chose.
  qDebug().noquote() << QStringLiteral("%1: automatic %2 = %3").arg(m_def.name, slot->def->label, label);
  setOperation(operation, objects, true);
  return true;
}

Selection CommandSession::suggestedObjects(const QJsonObject& choice) const {
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
  return objects;
}

void CommandSession::setOperation(const QString& operation, const Selection& objects, bool react) {
  const bool changedOperation = m_state.choice(kOperation) != operation;
  m_state.setChoice(kOperation, operation);
  if (slotOf(kObjects).has_value()) {
    m_state.setItems(kObjects, objects);
    logSelection(kObjects);
  }
  if (react && changedOperation && m_def.autoOperationApplied) {
    m_def.autoOperationApplied(m_state, m_host);
  }
  if (m_presenter) {
    m_presenter->refresh(m_state);
  }
  updateHighlights();
}

void CommandSession::observeToolDirection() {
  if (m_def.toolDirection && !m_done) {
    m_autoOperation.observeDirection(m_def.toolDirection(m_state));
  }
}

bool CommandSession::retryFlippedOperation(const QJsonObject& report) {
  if (m_done || !m_autoOperation.flipWaiting()) {
    return false;
  }
  const auto slot = slotOf(kOperation);
  if (!slot) {
    m_autoOperation.cancelFlip();
    return false;
  }
  const bool works = previewWorks(report);
  const QString current = m_state.choice(kOperation);
  const QJsonObject choice = report.value(QStringLiteral("auto_operation")).toObject();
  const QString suggested = choice.value(kOperation).toString();
  const AutoOperationPolicy::FlipStep step =
      m_autoOperation.flipPreviewed(works, current.toStdString(), suggested.toStdString());
  const auto labelOf = [&](const QString& operation) {
    for (const auto& [value, text] : slot->def->choicesIn(m_state)) {
      if (value == operation) {
        return text;
      }
    }
    return QString();
  };
  switch (step.action) {
  case AutoOperationPolicy::FlipAction::Keep:
    return false;
  case AutoOperationPolicy::FlipAction::Taken:
    // What the UI tests read: the flip's operation stays.
    qDebug().noquote() << QStringLiteral("%1: flipped %2 the body, %3 = %4")
                              .arg(m_def.name,
                                   current == QStringLiteral("cut") ? QStringLiteral("into")
                                                                    : QStringLiteral("out of"),
                                   slot->def->label, labelOf(current));
    return false;
  case AutoOperationPolicy::FlipAction::Restore:
    // Neither works: the user's operation and its failure, as before.
    qDebug().noquote() << QStringLiteral("%1: flipped, %2 does not work either, %3 = %4 kept")
                              .arg(m_def.name, labelOf(current), slot->def->label, labelOf(m_flipOperation));
    setOperation(m_flipOperation, m_flipObjects, false);
    runPreview();
    return true;
  case AutoOperationPolicy::FlipAction::Try:
    break;
  }
  const QString operation = QString::fromStdString(step.operation);
  const QString label = labelOf(operation);
  if (label.isEmpty()) {
    // Only what the input offers.
    m_autoOperation.cancelFlip();
    return false;
  }
  m_flipOperation = current;
  m_flipObjects = m_state.items(kObjects);
  qDebug().noquote() << QStringLiteral("%1: flipped, %2 fails, trying %3")
                            .arg(m_def.name, labelOf(current), label);
  setOperation(operation, suggestedObjects(choice), true);
  runPreview();
  return true;
}

bool CommandSession::turnMissedOperation(const QJsonObject& report) {
  if (m_done || !m_autoOperation.turnWaiting()) {
    return false;
  }
  const auto flip = slotOf(m_def.toolFlip);
  const auto operation = slotOf(kOperation);
  if (!flip || !operation) {
    m_autoOperation.cancelFlip();
    return false;
  }
  const QString current = m_state.choice(kOperation);
  // The model's answer for the other direction: a scoped Cut's (mitcad#1676);
  // a Join's and an unscoped Cut's is the turned preview's.
  std::optional<bool> otherWay;
  const QJsonObject targets = report.value(QStringLiteral("cut_targets")).toObject();
  if (current == QStringLiteral("cut") && !targets.isEmpty()) {
    otherWay = targets.value(QStringLiteral("flip_would_cut")).toBool();
  }
  const AutoOperationPolicy::TurnAction action = m_autoOperation.operationPreviewed(previewWorks(report), otherWay);
  QString label;
  for (const auto& [value, text] : operation->def->choicesIn(m_state)) {
    if (value == current) {
      label = text;
    }
  }
  const bool on = m_state.checked(m_def.toolFlip);
  const auto onOff = [](bool checked) { return checked ? QStringLiteral("on") : QStringLiteral("off"); };
  switch (action) {
  case AutoOperationPolicy::TurnAction::Keep:
    return false;
  case AutoOperationPolicy::TurnAction::Taken:
    // What the UI tests read: the turned direction stays.
    qDebug().noquote() << QStringLiteral("%1: the flipped %2 works, %3 = %4 stays")
                              .arg(m_def.name, label, flip->def->label, onOff(on));
    return false;
  case AutoOperationPolicy::TurnAction::Restore:
    // Neither direction works: the user's own and its failure, as before.
    qDebug().noquote() << QStringLiteral("%1: %2 misses both ways, %3 = %4 again")
                              .arg(m_def.name, label, flip->def->label, onOff(!on));
    break;
  case AutoOperationPolicy::TurnAction::Turn:
    qDebug().noquote() << QStringLiteral("%1: %2 misses, flipped %3, %4 = %5")
                              .arg(m_def.name, label,
                                   current == QStringLiteral("cut") ? QStringLiteral("into the body")
                                                                    : QStringLiteral("toward the bodies"),
                                   flip->def->label, onOff(!on));
    break;
  }
  // As the user's own click on it does (the panel shows it, the arrow
  // follows), without arming a flip's retry: the policy kept the direction.
  m_state.setChecked(m_def.toolFlip, !on);
  if (flip->def->onChange) {
    flip->def->onChange(m_state, m_host);
  }
  if (m_presenter) {
    m_presenter->refresh(m_state);
  }
  updateManipulators();
  runPreview();
  return true;
}

void CommandSession::autoOperationOverruled(const QString& key) {
  const auto slot = slotOf(key);
  const bool targets = slot && slot->def->type == InputDef::Type::CutTargets;
  if (key != kOperation && key != kObjects && !targets) {
    return;
  }
  // A flip's retry ends too (mitcad#2216): the user's own choice stays.
  m_autoOperation.cancelFlip();
  // Cut or Join chosen by hand: its preview may turn the direction once
  // (mitcad#2283).
  if (key == kOperation && !m_done && m_def.toolDirection && !m_def.toolFlip.isEmpty() &&
      slotOf(m_def.toolFlip).has_value()) {
    m_autoOperation.operationChosen(m_state.choice(kOperation).toStdString(), m_def.toolDirection(m_state));
  }
  if (!m_autoOperation.active()) {
    return;
  }
  // The user's own choice stays for the rest of the command.
  m_autoOperation.disable();
  qDebug().noquote() << QStringLiteral("%1: operation chosen by hand").arg(m_def.name);
}

} // namespace mitcad
