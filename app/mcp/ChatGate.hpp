// SPDX-License-Identifier: MIT
#pragma once

// The chat panel's approvals, enforced by the live server (mitcad#947): the
// chat starts its agent with a bridge whose connection carries the chat's
// marker (`mitcad-cli mcp --connect --chat-session MARKER`); the listener
// asks this gate before each tool call of such a connection, and runs,
// refuses or holds the call until the user answers the panel's card. Other
// clients' calls never come here.

#include <functional>

#include <QJsonObject>
#include <QString>

namespace mitcad::mcp {

class ChatGate {
public:
  enum class Verdict { Run, Deny, Ask };
  struct Decision {
    Verdict verdict = Verdict::Deny;
    QString reason; // Deny: what the agent is told
  };

  virtual ~ChatGate() = default;

  // Before a call of the connection marked `marker`. Ask: the gate calls
  // `answer` once, later, with the user's answer (true: run it) and the
  // reason of a refusal; it may never call it when the connection ends
  // first (connectionClosed).
  virtual Decision decide(const QString& marker, const QString& tool, const QJsonObject& arguments,
                          std::function<void(bool allowed, const QString& reason)> answer) = 0;
  // Around a call that runs: the chat tells its own changes of the design
  // from the user's.
  virtual void toolStarted(const QString& marker, const QString& tool) = 0;
  virtual void toolFinished(const QString& marker, const QString& tool, bool error) = 0;
  // The connection marked `marker` ended: a card waiting for it goes.
  virtual void connectionClosed(const QString& marker) = 0;
};

} // namespace mitcad::mcp
