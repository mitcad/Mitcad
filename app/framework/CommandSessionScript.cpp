// SPDX-License-Identifier: MIT
// A command panel filled and read by an agent through live MCP (mitcad#265):
// the inputs go through the panel's own actions, as the user's typing,
// choosing and picking would.
#include "CommandSession.hpp"

#include <cmath>

#include <QJsonArray>
#include <QJsonObject>
#include <QLineEdit>
#include <QSignalBlocker>
#include <QStringList>

namespace mitcad {
namespace {

QString typeName(InputDef::Type type) {
  switch (type) {
  case InputDef::Type::Selection:
    return QStringLiteral("selection");
  case InputDef::Type::Value:
    return QStringLiteral("value");
  case InputDef::Type::Choice:
    return QStringLiteral("choice");
  case InputDef::Type::Check:
    return QStringLiteral("check");
  case InputDef::Type::Flip:
    return QStringLiteral("flip");
  case InputDef::Type::Text:
    return QStringLiteral("text");
  case InputDef::Type::CutTargets:
    return QStringLiteral("cut_targets");
  case InputDef::Type::List:
    return QStringLiteral("list");
  case InputDef::Type::Note:
    return QStringLiteral("note");
  }
  return QStringLiteral("value");
}

} // namespace

bool CommandSession::scriptInputs(const QJsonObject& inputs, const ItemReader& read, QString& error) {
  if (m_done || m_presenter == nullptr) {
    error = tr("%1 has ended").arg(m_def.name);
    return false;
  }
  for (auto it = inputs.constBegin(); it != inputs.constEnd(); ++it) {
    const QString& key = it.key();
    const QJsonValue value = it.value();
    const auto slot = slotOf(key);
    if (!slot) {
      QStringList keys;
      for (const Slot& shown : slotsOf(true)) {
        keys << shown.key;
      }
      error = tr("%1 has no input %2 (its inputs: %3)").arg(m_def.name, key, keys.join(QStringLiteral(", ")));
      return false;
    }
    if (!m_presenter->inputEnabled(key)) {
      error = tr("%1: the input %2 is not available now").arg(m_def.name, key);
      return false;
    }
    // The actions as the panel set them (setPresenter).
    const CommandPresenter::Actions& actions = m_presenter->actions();
    const InputDef& input = *slot->def;
    switch (input.type) {
    case InputDef::Type::Value:
    case InputDef::Type::Text: {
      if (!value.isString() && !value.isDouble()) {
        error = tr("%1: %2 takes a number or a text").arg(m_def.name, key);
        return false;
      }
      const QString text = value.isDouble() ? QString::number(value.toDouble(), 'g', 15) : value.toString();
      // The Qt panel's field shows it (the AppKit form follows the state).
      if (m_panel) {
        if (auto* field = m_panel->findChild<QLineEdit*>(QStringLiteral("input_") + key)) {
          const QSignalBlocker block(field);
          field->setText(text);
        }
      }
      if (!actions.textEdited) {
        error = tr("%1: %2 cannot be typed into").arg(m_def.name, key);
        return false;
      }
      actions.textEdited(key, text);
      if (actions.valueCommitted) {
        actions.valueCommitted(key);
      }
      break;
    }
    case InputDef::Type::Choice: {
      const QString chosen = value.toString();
      QStringList offered;
      bool known = false;
      for (const auto& option : input.choicesIn(scopeOf(*slot))) {
        if (isChoiceSeparator(option.first)) {
          continue; // a line between the options, no choice (mitcad#828)
        }
        offered << option.first;
        known = known || option.first == chosen;
      }
      if (!value.isString() || !known) {
        error = tr("%1: %2 takes one of %3").arg(m_def.name, key, offered.join(QStringLiteral(", ")));
        return false;
      }
      if (actions.choiceChanged) {
        actions.choiceChanged(key, chosen);
      }
      break;
    }
    case InputDef::Type::Check:
    case InputDef::Type::Flip:
      if (!value.isBool()) {
        error = tr("%1: %2 takes true or false").arg(m_def.name, key);
        return false;
      }
      if (actions.checkChanged) {
        actions.checkChanged(key, value.toBool());
      }
      break;
    case InputDef::Type::CutTargets:
      if (!value.isObject()) {
        error = tr("%1: %2 takes an object").arg(m_def.name, key);
        return false;
      }
      if (actions.objectChanged) {
        actions.objectChanged(key, value.toObject());
      }
      break;
    case InputDef::Type::Selection: {
      if (!value.isArray()) {
        error = tr("%1: %2 takes an array of entities").arg(m_def.name, key);
        return false;
      }
      Selection items;
      for (const QJsonValue& entry : value.toArray()) {
        QString problem;
        const std::optional<SelectionItem> item = read(entry, problem);
        if (!item) {
          error = tr("%1: %2: %3").arg(m_def.name, key, problem);
          return false;
        }
        if (!input.acceptsIn(*item, m_state)) {
          error = tr("%1: %2 does not take %3").arg(m_def.name, input.label, item->describe());
          return false;
        }
        items.append(input.convert ? input.convert(*item) : *item);
      }
      if (input.max > 0 && items.size() > input.max) {
        error = tr("%1: %2 takes at most %3").arg(m_def.name, input.label).arg(input.max);
        return false;
      }
      m_state.setItems(key, uniqueProfiles(items, resolver()));
      activate(key);
      selectionChanged(key);
      break;
    }
    case InputDef::Type::List:
      error = tr("%1: %2 is a list: give its rows' inputs by their keys (%2.0.<input>); rows are added in the "
                 "application")
                  .arg(m_def.name, key);
      return false;
    case InputDef::Type::Note:
      error = tr("%1: %2 is a note: it takes no value").arg(m_def.name, key);
      return false;
    }
  }
  m_presenter->refresh(m_state);
  return true;
}

bool CommandSession::scriptBusy() const {
  return !m_done && (m_previewTimer.isActive() || m_asyncPending || m_commitRequested || m_committing ||
                     m_cancelRequested || m_host.modelBusy());
}

QJsonObject CommandSession::scriptState() {
  QJsonObject inputs;
  for (const Slot& slot : slotsOf(true)) {
    const InputDef& input = *slot.def;
    QJsonObject entry{{QStringLiteral("label"), input.label}, {QStringLiteral("type"), typeName(input.type)}};
    switch (input.type) {
    case InputDef::Type::Value: {
      entry.insert(QStringLiteral("text"), m_state.text(slot.key));
      entry.insert(QStringLiteral("expression"), m_state.expression(slot.key));
      const double number = m_state.value(slot.key);
      entry.insert(QStringLiteral("value"), std::isfinite(number) ? QJsonValue(number) : QJsonValue());
      break;
    }
    case InputDef::Type::Text:
      entry.insert(QStringLiteral("text"), m_state.text(slot.key));
      break;
    case InputDef::Type::Choice:
      entry.insert(QStringLiteral("choice"), m_state.choice(slot.key));
      break;
    case InputDef::Type::Check:
    case InputDef::Type::Flip:
      entry.insert(QStringLiteral("checked"), m_state.checked(slot.key));
      break;
    case InputDef::Type::CutTargets:
      entry.insert(QStringLiteral("object"), m_state.object(slot.key));
      break;
    case InputDef::Type::Selection: {
      QJsonArray items;
      for (const SelectionItem& item : m_state.items(slot.key)) {
        items.append(item.describe());
      }
      entry.insert(QStringLiteral("items"), items);
      entry.insert(QStringLiteral("active"), slot.key == m_active);
      break;
    }
    case InputDef::Type::List:
      entry.insert(QStringLiteral("rows"), m_state.rows(slot.key));
      break;
    case InputDef::Type::Note:
      entry.insert(QStringLiteral("text"), input.noteFor ? input.noteFor(m_state).text : QString());
      break;
    }
    inputs.insert(slot.key, entry);
  }
  QString message;
  bool error = false;
  const bool ready = previewReady(message, error);
  QJsonObject answer{{QStringLiteral("id"), m_def.id},
                    {QStringLiteral("name"), m_def.name},
                    {QStringLiteral("editing"), m_editUid.isEmpty() ? QJsonValue() : QJsonValue(m_editUid)},
                    {QStringLiteral("inputs"), inputs},
                    {QStringLiteral("ready"), ready},
                    {QStringLiteral("preview_ok"), m_previewOk},
                    {QStringLiteral("preview_failed"), m_failed},
                    {QStringLiteral("busy"), scriptBusy()}};
  if (!ready) {
    answer.insert(QStringLiteral("message"), message);
    answer.insert(QStringLiteral("message_is_error"), error);
  }
  const QJsonValue warnings = m_shownReport.value(QStringLiteral("warnings"));
  if (warnings.isArray() && !warnings.toArray().isEmpty()) {
    answer.insert(QStringLiteral("preview_warnings"), warnings);
  }
  if (!m_activationOffer.isEmpty()) {
    // The component to activate for the picks (mitcad#1634): an agent's
    // pick never activates it by itself.
    answer.insert(QStringLiteral("activate"), m_activationOffer);
  }
  if (m_presenter != nullptr) {
    answer.insert(QStringLiteral("panel"), m_presenter->shownState());
  }
  return answer;
}

bool CommandSession::restoreInputs(const QHash<QString, QString>& choices,
                                   const QHash<QString, Selection>& selections) {
  if (m_done || m_presenter == nullptr) {
    return false;
  }
  const CommandPresenter::Actions& actions = m_presenter->actions();
  for (auto it = choices.constBegin(); it != choices.constEnd(); ++it) {
    const auto slot = slotOf(it.key());
    if (slot && slot->def->type == InputDef::Type::Choice && m_state.choice(it.key()) != it.value() &&
        actions.choiceChanged) {
      actions.choiceChanged(it.key(), it.value());
    }
  }
  QString last;
  for (const Slot& slot : slotsOf(false)) {
    if (slot.def->type != InputDef::Type::Selection) {
      continue;
    }
    const Selection items = selections.value(slot.key);
    if (!items.isEmpty() && isShown(slot)) {
      last = slot.key;
    }
    if (items == m_state.items(slot.key)) {
      continue;
    }
    m_state.setItems(slot.key, items);
    selectionChanged(slot.key);
  }
  if (!last.isEmpty()) {
    activate(last);
  }
  m_presenter->refresh(m_state);
  return true;
}

} // namespace mitcad
