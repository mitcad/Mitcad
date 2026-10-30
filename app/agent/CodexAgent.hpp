// SPDX-License-Identifier: MIT
#pragma once

// Codex through its own app server (mitcad#947): one long-lived `codex
// app-server` process per conversation, JSON-RPC over stdio. The protocol
// is the chat logic's (core/mcp/src/chat/codex.rs): this class runs the
// process, gives the stream its controls (start, message, interrupt) and
// Codex's lines, writes the lines the stream answers with and passes the
// panel events on. The live MCP bridge of this window is the thread's only
// MCP server, given as the thread's configuration overrides; nothing is
// written into the user's Codex configuration (config.toml). The process
// runs in an empty folder of its own.
//
// Setup: `codex --version` (at least CODEX_MINIMUM_VERSION), then `codex
// login status` (exit status 0: signed in). Sign in runs `codex login`,
// Codex's own sign-in, which opens the browser; MitCad never sees a
// credential. On Windows an npm launcher script (codex.cmd) is not
// started; the native program its package carries is, when it is there.
//
// Stop: turn/interrupt, on every platform; if the turn has not ended 5 s
// later the process is ended, and the next message resumes the thread.
// The models Codex offers (model/list) are kept in the settings, so that
// the panel offers them before Codex runs again.

#include <functional>
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

class CodexAgent : public ChatAgent {
  Q_OBJECT

public:
  explicit CodexAgent(QObject* parent = nullptr);
  ~CodexAgent() override;

  QString id() const override { return QStringLiteral("codex"); }
  QString name() const override { return tr("Codex"); }
  QString installPage() const override { return QStringLiteral("https://github.com/openai/codex"); }
  QString accountName() const override { return QStringLiteral("ChatGPT"); }
  QList<Model> models() const override { return m_models; }

  void check() override;
  bool installedNow() const override;
  Readiness readiness() const override { return m_readiness; }
  QString readinessDetail() const override { return m_detail; }
  void signIn() override;

  bool running() const override;
  bool start(const Start& start, QString* error) override;
  bool send(const Message& message, QString* error) override;
  void stop() override;
  void shutdown() override;

private:
  // A short run of the program with a time limit; `done` gets its exit code
  // (-1: it did not run) and its output (both channels).
  void probe(const QStringList& arguments, std::function<void(int code, const QByteArray& output)> done);
  void setReadiness(Readiness readiness, const QString& detail = QString());
  // A control of the stream; false with `error` when it refused it.
  bool control(const QJsonObject& request, QString* error);
  // The stream's events: lines to write, the end, models, panel events.
  void handle(const QJsonArray& events);
  void readOutput();
  void processEnded(int code);
  QString findProgram(QString* reason) const;
  void setModels(const QJsonArray& listed);

  QString m_program;
  Readiness m_readiness = Readiness::Checking;
  QString m_detail;
  int m_generation = 0;
  QList<Model> m_models;

  QProcess* m_process = nullptr;
  std::optional<rust::Box<ChatStream>> m_stream;
  QByteArray m_pending;
  QByteArray m_errors;
  bool m_stopping = false;
  QTimer* m_stopTimer = nullptr;
  QProcess* m_signIn = nullptr;
};

} // namespace mitcad::agent
