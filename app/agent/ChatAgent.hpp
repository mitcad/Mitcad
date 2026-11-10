// SPDX-License-Identifier: MIT
#pragma once

// An AI agent the chat panel talks to (mitcad#947), through the agent's own
// native interface: one class per agent (ClaudeCodeAgent, CodexAgent; the
// API endpoints and local models of phase 2 as ApiAgent; a custom command
// over the Agent Client Protocol as AcpAgent, mitcad#1198) behind this one
// interface, so that a change of an agent's protocol touches its class
// only. The agent is the user's own installation, found
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
#include <QStringList>

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
    QString context; // the attached selection and open comments, as text
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
  // Where it was found when several of one kind are (mitcad#1289: a second
  // Claude Code's folder, "~/.local/bin"); empty for the first or only one.
  virtual QString place() const { return QString(); }
  // The account it is signed in to as its own check tells it (an email
  // address), and that account's plan; empty when it does not tell.
  virtual QString account() const { return QString(); }
  virtual QString subscription() const { return QString(); }
  // The configuration folder its sign-in lives in (mitcad#1297: Claude
  // Code's CLAUDE_CONFIG_DIR or ~/.claude); agents with one folder share one
  // sign-in. Empty: its own, or not known.
  virtual QString signInFolder() const { return QString(); }

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

  // The setup's words and steps where an agent differs from an installed
  // program with a sign-in of its own (the API agents, ApiAgent.hpp:
  // phase 2 of mitcad#947).
  // The Missing state's button (it opens installPage(); without one, the
  // chat's Preferences).
  virtual QString installAction() const { return tr("Install %1").arg(name()); }
  // The SignedOut state's line and button.
  virtual QString signedOutText() const { return tr("%1 is not signed in.").arg(name()); }
  virtual QString signInAction() const { return tr("Sign in to %1").arg(accountName()); }
  // Signed out means a missing API key: the panel asks for it in place
  // (setKey) instead of starting a sign-in.
  virtual bool asksForKey() const { return false; }
  virtual void setKey(const QString& key) { Q_UNUSED(key); }
  // The one notice before the first message: what leaves the computer and
  // where to.
  virtual QString privacyNotice() const {
    return tr("What you write here, the selection, the open comments and pictures of the view go to %1's service, "
              "whose terms apply. The agent changes this design through MitCad's tools.")
        .arg(name());
  }
  // How to fix an account failure (the panel's help); empty: the panel's
  // own text for the agent.
  virtual QString accountHelp() const { return QString(); }
  // Sign in Again (mitcad#1585): the agent's own sign-out and sign-in, each
  // a command line whose program is the executable the chat runs, run in a
  // terminal for a known account error. Empty: not offered (a custom
  // command, whose sign-in MitCad does not know; an API agent's key).
  virtual QList<QStringList> signInAgainCommands() const { return {}; }
  // Whether it is started with MitCad's mitcad-cli bridge (a program of
  // its own); an agent whose tool loop runs in MitCad connects to the
  // listener itself.
  virtual bool needsBridge() const { return true; }
  // Its settings are Preferences' (an endpoint's address and key, a custom
  // command): it is looked for again when they change.
  virtual bool configuredInPreferences() const { return !needsBridge(); }
  // The history of a design was cleared: what the agent kept of that
  // conversation goes too (the API agents keep it in MitCad); an empty
  // session: every conversation.
  virtual void forgetSession(const QString& session) { Q_UNUSED(session); }

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
