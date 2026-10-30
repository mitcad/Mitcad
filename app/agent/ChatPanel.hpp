// SPDX-License-Identifier: MIT
#pragma once

// The chat panel (mitcad#947), Qt on every platform: a card on the right
// edge of the Floating layout, a dock in the Docked one (MainWindowChat.cpp
// places it). From the top:
//
//   Claude Code (a choice of the agents found) [New conversation] [...]
//   Model: [Default model ▾] reported model                the header
//   the conversation: your messages, the answers (Markdown, streamed),
//     the tool calls as rows that open to their input and result (failed
//     ones marked), approval cards (Allow once, Always allow, Deny),
//     notices; calls that follow each other are one group row
//     (mitcad#983): "3 calls (1 failed) · the latest call", which opens
//     to their rows; the agent's text, a card or a notice starts the next
//     group (core/mcp/src/chat/groups.rs)
//   one state line
//   the setup's one action (Install Claude Code, Sign in to Claude, ...)
//   or the message field with Selection and View (what goes with the
//   message) and Send (Stop while the agent works)
//
// Enter sends (a message written while the agent works waits for it),
// Shift+Enter starts a new line, Esc stops. Before the first message to an
// agent, one notice says what goes to the agent's service.
//
// For UI tests the panel's "chatState" property and the log line "Chat
// panel: <state>" give its state (unavailable, checking, missing,
// outdated, signed-out, signing-in, ready, starting, working, stopping,
// notice); its controls have object names (chatInput, chatSend,
// chatAttachSelection, chatAttachView, chatSetup, chatNew, chatAllowOnce,
// chatAlwaysAllow, chatDeny, chatContinue, chatAgentChoice,
// chatModelChoice, chatToolGroupHead); the log lines "Chat tool group: 3
// calls, 1 failed" (when a group's counts change) and "Chat tool group
// opened: 3 calls, 1 failed, 3 rows shown" ("closed") follow the groups.
//
// The agent choice lists the agents found on this computer (and the chosen
// one); choosing another starts a new conversation in it. The model choice
// offers the agent's default, the models it names (Codex's once it ran) and
// any name typed in; it is kept per agent.

#include <QHash>
#include <QJsonObject>
#include <QList>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QWidget>

class QComboBox;
class QFrame;
class QLabel;
class QPlainTextEdit;
class QPushButton;
class QScrollArea;
class QTimer;
class QToolButton;
class QVBoxLayout;

namespace mitcad::agent {

class ChatController;

class ChatPanel : public QWidget {
  Q_OBJECT

public:
  explicit ChatPanel(ChatController& chat, QWidget* parent = nullptr);

  // The message field gets the keyboard (the panel was opened).
  void focusInput();

signals:
  // Connect AI Assistant (Claude Desktop, which the chat does not use).
  void connectAssistantRequested();
  // Preferences › AI Assistants.
  void preferencesRequested();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  // A group row of tool calls (mitcad#983): its head (hidden while it has
  // one call) and the calls' rows below it.
  struct ToolGroup {
    QWidget* widget = nullptr;
    QToolButton* head = nullptr;
    QWidget* calls = nullptr;
    QString logged; // the counts last logged
  };

  void rebuild();
  // `group`: the tool call's group when known (tool_groups), else asked.
  void addRow(int index, const QJsonObject& group = QJsonObject());
  void updateRow(int index);
  // The group row that starts at entry `first`, made at the end of the
  // conversation when it is new.
  ToolGroup& toolGroup(int first);
  // A group's head, its rows shown or not, its log line.
  void applyGroup(const QJsonObject& group);
  void fillRow(QWidget* row, const QJsonObject& entry);
  QWidget* makeRow(const QJsonObject& entry);
  void updateState();
  void submit();
  void send(const QString& text);
  void keepAtEnd(bool force);
  QString cardText(const QJsonObject& entry) const;
  // The switcher's agents and the model choice's items, when they changed.
  void updateChoices();
  void applyModel();

  ChatController& m_chat;
  QLabel* m_agentName = nullptr;
  QComboBox* m_agentChoice = nullptr;
  QComboBox* m_modelChoice = nullptr;
  QLabel* m_model = nullptr;
  QStringList m_agentItems; // the switcher's agent ids as shown
  QString m_modelItems;     // the model choice's items as shown (agent, ids)
  QToolButton* m_new = nullptr;
  QToolButton* m_more = nullptr;

  QScrollArea* m_scroll = nullptr;
  QWidget* m_list = nullptr;
  QVBoxLayout* m_listLayout = nullptr;
  QList<QWidget*> m_rows; // a tool call's row sits in its group's widget
  QHash<int, ToolGroup> m_groups; // by the index of their first call

  QLabel* m_state = nullptr;
  QFrame* m_notice = nullptr; // the privacy notice before the first message
  QLabel* m_noticeText = nullptr;
  QWidget* m_setupRow = nullptr;
  QPushButton* m_setup = nullptr;
  QPushButton* m_secondary = nullptr;
  QWidget* m_inputRow = nullptr;
  QPlainTextEdit* m_input = nullptr;
  QToolButton* m_attachSelection = nullptr;
  QToolButton* m_attachView = nullptr;
  QPushButton* m_send = nullptr;

  QTimer* m_poll = nullptr;   // a missing agent looked for again
  QTimer* m_render = nullptr; // streamed answers drawn a few times a second
  QSet<int> m_dirty;
  QString m_waiting; // the message held by the privacy notice
  QString m_setupAction;
  QString m_loggedState;
  bool m_checked = false; // the agent was looked for since the panel opened
};

} // namespace mitcad::agent
