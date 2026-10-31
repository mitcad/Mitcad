// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include "Command.hpp"

namespace mitcad {

// A command's controls show the state and send actions; neither the session
// nor the declarative command definitions need to know their UI toolkit.
class CommandPresenter {
public:
  struct Actions {
    std::function<void(const QString&, const QString&)> textEdited;
    std::function<void(const QString&)> valueCommitted;
    std::function<void(const QString&, const QString&)> choiceChanged;
    std::function<void(const QString&, bool)> checkChanged;
    std::function<void(const QString&, const QJsonObject&)> objectChanged;
    std::function<void(const Selection&)> highlightRequested;
    std::function<void(const QString&)> selectionActivated;
    std::function<void(const QString&)> selectionCleared;
    std::function<void(const QString&, int, int)> itemMoved;
    std::function<void(const QString&, int)> itemRemoved;
    std::function<void(const QString&)> rowAdded;
    std::function<void(const QString&, int)> rowRemoved;
    std::function<void()> accepted;
    std::function<void()> rejected;
    std::function<void()> detached;
    // A value field took the keyboard or lost it (mitcad#250: the view's
    // labels then put parameter names into it).
    std::function<void(const QString&, bool)> valueFocused;
    // The button beside a choice was clicked (InputDef::onButton).
    std::function<void(const QString&)> buttonClicked;
    // A note row's button was clicked: the row's key and the button's index
    // (InputDef::onNoteAction, mitcad#655).
    std::function<void(const QString&, int)> noteAction;
    // A value field's mark was clicked: the input's key (InputDef::onMark,
    // mitcad#980: Reset to size).
    std::function<void(const QString&)> markClicked;
    // A link in an inspection's result was clicked ("point:x,y,z": Print
    // Analysis' thinnest wall, mitcad#620).
    std::function<void(const QString&)> resultLinkActivated;
  };

  virtual ~CommandPresenter() {
    if (m_actions.detached) {
      m_actions.detached();
    }
  }
  void setActions(Actions actions) { m_actions = std::move(actions); }
  const Actions& actions() const { return m_actions; }

  virtual void refresh(const CommandState& state) = 0;
  // Latest accepted feature preview, including recoverable error metadata;
  // empty means previous candidates are stale while inputs change.
  virtual void setPreviewReport(const QJsonObject&) {}
  // A structural input action has applied, including a rejected no-op.
  // Frontends may hold the affected controls until this acknowledgement.
  virtual void inputActionApplied(const QString&) {}
  virtual bool inputEnabled(const QString&) const { return true; }
  virtual void setActiveSelection(const QString& key) = 0;
  virtual void setInputError(const QString& key, const QString& message) = 0;
  virtual void setValueHint(const QString& key, const QString& hint) = 0;
  // What an empty value field shows (InputDef::autoText, mitcad#656).
  virtual void setPlaceholder(const QString& /*key*/, const QString& /*text*/) {}
  virtual void setMessage(const QString& message, bool error) = 0;
  virtual void setResult(const QString& text) = 0;
  // An inspection's answer as a whole: its rows of names and values where
  // it has them (Inspection::values and groups, mitcad#1010), else its
  // text. A frontend without rows shows the text.
  virtual void setInspection(const Inspection& result) { setResult(result.text); }
  virtual void setOkEnabled(bool enabled) = 0;
  virtual QColor colorOf(const QString& key) const = 0;
  virtual bool focusFirstValue() = 0;
  virtual bool focusValue(const QString& key) = 0;
  // Inserts text (a parameter's name) at a value field's cursor, gives the
  // field the keyboard again and reports the edit as typed text
  // (textEdited); false when the field is not shown (mitcad#250).
  virtual bool insertIntoValue(const QString& /*key*/, const QString& /*text*/) { return false; }
  // What the controls show, for live MCP (mitcad#265): `message` (the status
  // line), `result` (an inspection's answer), `ok_enabled` and `notes` (by
  // input key: `error` or `hint`). Empty when the frontend cannot tell.
  virtual QJsonObject shownState() const { return {}; }
  virtual void logLayout() const = 0;

private:
  Actions m_actions;
};

} // namespace mitcad
