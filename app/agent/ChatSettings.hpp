// SPDX-License-Identifier: MIT
#pragma once

// The chat panel's settings (mitcad#947), Preferences › AI Assistants ›
// Chat, in the user's settings (chat/...): the agent (the last used one),
// the approval mode, whether the history is kept; what the user allowed
// always, per agent; the model chosen in the panel, per agent; whether the
// one privacy notice was shown, per agent. Plain Qt Core.

#include <QObject>
#include <QString>
#include <QStringList>

namespace mitcad::agent {

// Which calls of the chat's agent ask first (core/mcp/src/chat/policy.rs):
// Ask every time (model edits too), Model edits allowed (file operations
// and an undo past the message's start ask; the default), Allow everything.
enum class ApprovalMode { Ask, ModelEdits, Everything };

// "ask", "model", "all": the policy's names.
QString approvalModeKey(ApprovalMode mode);
ApprovalMode approvalModeFromKey(const QString& key);

struct ChatSettings {
  QString agent = QStringLiteral("claude"); // chat/agent
  ApprovalMode approvals = ApprovalMode::ModelEdits;
  bool history = true; // chat/history

  static ChatSettings load();
  void save() const;
  bool operator==(const ChatSettings& other) const {
    return agent == other.agent && approvals == other.approvals && history == other.history;
  }
  bool operator!=(const ChatSettings& other) const { return !(*this == other); }
};

// The kinds of call (core/mcp/src/chat/policy.rs: document_save, edit, ...)
// the user allowed always for an agent (Always Allow on a card); reset in
// Preferences.
QStringList alwaysAllowed(const QString& agent);
void allowAlways(const QString& agent, const QString& kind);
bool anyAlwaysAllowed();
void resetAlwaysAllowed();

// The model chosen in the panel for an agent (chat/model/<agent>); empty:
// the agent's default.
QString chatModel(const QString& agent);
void setChatModel(const QString& agent, const QString& model);

// The one notice that the message, the selection and pictures of the view
// go to the agent's service, shown before the first message to an agent.
bool privacyNoticeShown(const QString& agent);
void setPrivacyNoticeShown(const QString& agent);

// Where the histories are: MITCAD_CHAT_DIR, else the user's application
// data folder's chat/.
QString chatDataDirectory();
// The agent's own working folder, empty: its sessions are kept per folder
// by the agent, so it stays the same. MITCAD_CHAT_AGENT_DIR, else the local
// application data folder's chat-agent/.
QString agentWorkDirectory();

// Tells the open panels that Preferences cleared every history or reset
// what was allowed always.
class ChatNotifier : public QObject {
  Q_OBJECT

public:
  static ChatNotifier& instance();

signals:
  void historyCleared();
  // Preferences chose another agent for the chat.
  void agentChosen(const QString& agent);
};

} // namespace mitcad::agent
