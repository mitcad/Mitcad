// SPDX-License-Identifier: MIT
#pragma once

// An AI agent the chat panel talks to (mitcad#947), through the agent's own
// native interface: one class per agent (ClaudeCodeAgent, CodexAgent)
// behind this one interface, so that a change of an agent's protocol
// touches its class only. The agent is the user's own installation, found
// by assistantExecutable (mitcad#682); MitCad never installs, changes or
// signs in to it, and never reads its credentials.
//
// Setup: check() finds the agent, its version and whether it is signed in
// (readinessChanged). Conversation: one process per conversation, started
// with a session id MitCad chooses (or resuming it); send() writes a user
// message; the agent's output comes as panel events in the chat logic's
// vocabulary (core/mcp/src/chat.rs: init, text, tool_start, tool,
// tool_result, turn_end, status, output, bridge); stop() ends the running
// turn; shutdown() ends the process. Start::session is the session the
// agent resumes; an agent that names its sessions itself (Codex's threads)
// reports the one it uses in its init event, and the chat goes on with that.

#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QString>

namespace mitcad::agent {

class ChatAgent : public QObject {
  Q_OBJECT

public:
  enum class Readiness {
    Checking,   // looking for it
    Missing,    // not installed (or not usable here: readinessDetail says why)
    Outdated,   // older than the chat needs (readinessDetail: the versions)
    SignedOut,  // installed, not signed in
    SigningIn,  // its own sign-in runs
    Ready,
  };

  struct Start {
    QString session;  // a UUID MitCad chose
    bool resume = false;
    QString bridge;   // the installed mitcad-cli
    QString instance; // the live MCP listener's instance id
    QString chat;     // the chat's marker of its bridge connection
    QString model;    // empty: the agent's default
  };

  struct Picture {
    QString mediaType; // image/png
    QByteArray base64;
  };

  struct Message {
    QString text;
    QString context; // the attached selection, as text
    QList<Picture> pictures;
  };

  // A model the panel offers for Start::model (any other name can be
  // typed): its name for the agent and for people.
  struct Model {
    QString id;
    QString name;
  };

  using QObject::QObject;

  virtual QString id() const = 0;          // "claude", "codex"
  virtual QString name() const = 0;        // "Claude Code", "Codex"
  virtual QString installPage() const = 0; // where its installation is explained
  // The account its own sign-in signs in to ("Claude", "ChatGPT").
  virtual QString accountName() const = 0;
  // The models it offers; Codex's come from its app server once it ran.
  virtual QList<Model> models() const = 0;

  virtual void check() = 0;
  // Only a file check (no process): the agent's program appeared since
  // it was Missing.
  virtual bool installedNow() const = 0;
  virtual Readiness readiness() const = 0;
  virtual QString readinessDetail() const = 0;
  // Starts the agent's own sign-in (it opens the browser); check() again
  // when it ends.
  virtual void signIn() = 0;

  virtual bool running() const = 0;
  virtual bool start(const Start& start, QString* error) = 0;
  virtual bool send(const Message& message, QString* error) = 0;
  virtual void stop() = 0;
  virtual void shutdown() = 0;

signals:
  void readinessChanged();
  void modelsChanged();
  // A panel event (core/mcp/src/chat.rs).
  void panelEvent(const QJsonObject& event);
  // The process ended: its exit code, the end of what it wrote to its error
  // output, whether stop() ended it.
  void exited(int code, const QString& errors, bool stopped);
};

} // namespace mitcad::agent
