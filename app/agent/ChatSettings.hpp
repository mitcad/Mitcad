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

// The API agents (phase 2 of mitcad#947, ApiAgent.hpp): whether each one
// is offered in the panel (chat/api/<agent>/enabled), its endpoint's address
// (chat/api/<agent>/url; empty: its preset) and the requests' timeout, the
// longest wait for the service's next data (chat/api/timeout, seconds).
bool apiAgentEnabled(const QString& agent);
void setApiAgentEnabled(const QString& agent, bool enabled);
QString apiAgentUrl(const QString& agent, const QString& preset);
void setApiAgentUrl(const QString& agent, const QString& url);
constexpr int kDefaultApiTimeout = 300;
int apiRequestTimeout();
void setApiRequestTimeout(int seconds);
// Where the API agents keep their conversations for resuming: the chat
// folder's api-sessions/.
QString apiSessionDirectory();

// The custom command that speaks the Agent Client Protocol (mitcad#1198,
// AcpAgent.hpp): whether it is offered in the panel (chat/acp/enabled), its
// command, a program's name or path (chat/acp/command), and its arguments
// as one line (chat/acp/arguments).
bool customAgentEnabled();
void setCustomAgentEnabled(bool enabled);
QString customAgentCommand();
void setCustomAgentCommand(const QString& command);
QString customAgentArguments();
void setCustomAgentArguments(const QString& arguments);

// Whether the chat panel offers an agent it found (mitcad#1299: the assistant
// menu's checkmark under Outgoing connections); an agent turned off is not
// listed in the panel and never started. Kept as the ids of the agents
// turned off (chat/off). Setting it tells the open panels
// (ChatNotifier::offeredChanged); returns whether it changed.
bool chatAgentOffered(const QString& agent);
bool setChatAgentOffered(const QString& agent, bool offered);

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
  // Preferences changed the API agents' settings (offered, address, key,
  // timeout) or the custom command's (offered, command, arguments).
  void apiSettingsChanged();
  // An agent was turned on or off in the assistant menu (chatAgentOffered).
  void offeredChanged();
};

} // namespace mitcad::agent
