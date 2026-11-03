// SPDX-License-Identifier: MIT
#pragma once

// A custom command that speaks the Agent Client Protocol itself
// (mitcad#1198): the command line set in Preferences › AI Assistants › Chat
// (a program's name, looked for as an assistant's program is, or its path,
// and its arguments), one long-lived process per conversation, JSON-RPC
// over stdio. MitCad ships and installs no adapter of any kind: the command
// must speak the protocol itself.
//
// The protocol is the chat logic's (core/mcp/src/chat/acp.rs): this class
// runs the process, gives the stream its controls (start, message,
// interrupt) and the agent's lines, writes the lines the stream answers
// with and passes the panel events on, as CodexAgent does. The window's
// live MCP bridge is the session's only MCP server, so that the chat's
// approvals, undo merge and history are those of the other agents. The
// process runs in the chat's own working folder.
//
// Setup: the command is found and answers the protocol's handshake
// (initialize) within 20 s; a command that does not is Missing, with the
// reason in the panel's state line, and is not tried again until it, its
// arguments or its program change (or the panel is opened again). An
// npm launcher script on Windows is not started.
//
// Stop: session/cancel; if the prompt has not ended 5 s later the process
// is ended, and the next message loads the session again.

#include <memory>
#include <optional>

#include <QByteArray>
#include <QJsonArray>
#include <QProcess>
#include <QString>
#include <QStringList>

#include "ChatAgent.hpp"
#include "mitcad_bridge/lib.h"

class QTimer;

namespace mitcad::agent {

class AcpAgent : public ChatAgent {
  Q_OBJECT

public:
  explicit AcpAgent(QObject* parent = nullptr);
  ~AcpAgent() override;

  QString id() const override { return QStringLiteral("acp"); }
  QString name() const override { return tr("Custom command"); }
  QString installPage() const override { return QString(); }
  QString accountName() const override { return name(); }
  QList<Model> models() const override { return m_models; }

  void check() override;
  // Offered (Preferences), its program found, and not the command that
  // last failed the handshake.
  bool installedNow() const override;
  Readiness readiness() const override { return m_readiness; }
  QString readinessDetail() const override { return m_detail; }
  void signIn() override {}

  bool running() const override;
  bool start(const Start& start, QString* error) override;
  bool send(const Message& message, QString* error) override;
  void stop() override;
  void shutdown() override;

  QString installAction() const override { return tr("Chat Preferences…"); }
  QString privacyNotice() const override;
  QString accountHelp() const override;
  bool configuredInPreferences() const override { return true; }

private:
  // The command of Preferences: its program found and its arguments, or
  // why not (`reason`).
  struct Command {
    QString program;
    QStringList arguments;
    QString reason;
  };
  Command command() const;
  // The command, its arguments and its program file's state, to tell a
  // changed command from the one that failed.
  static QString signature(const Command& found);
  void setReadiness(Readiness readiness, const QString& detail = QString());
  void probe(const Command& found, const QString& key);
  // The handshake's outcome: `program` for the log, `shown` (the command
  // as it was set) for the state line.
  void finishProbe(QProcess* process, const QString& key, const QString& program, const QString& shown, bool ok,
                   const QString& text);
  bool control(const QJsonObject& request, QString* error);
  void handle(const QJsonArray& events);
  void readOutput();
  void processEnded(int code);
  void setModels(const QJsonArray& listed);

  Readiness m_readiness = Readiness::Checking;
  QString m_detail;
  int m_generation = 0;
  QList<Model> m_models;
  QString m_passed; // the signature of the command that answered
  QString m_failed; // the signature of the command that did not
  QProcess* m_probe = nullptr;

  QProcess* m_process = nullptr;
  std::optional<rust::Box<ChatStream>> m_stream;
  QByteArray m_pending;
  QByteArray m_errors;
  bool m_stopping = false;
  QTimer* m_stopTimer = nullptr;
};

} // namespace mitcad::agent
