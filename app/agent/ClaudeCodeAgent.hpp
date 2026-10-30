// SPDX-License-Identifier: MIT
#pragma once

// Claude Code through its own print mode (mitcad#947): one long-lived
// `claude -p` process per conversation, stream-json in and out, the live
// MCP bridge of this window as its only tool source (`--mcp-config` as a
// JSON string, `--strict-mcp-config`, `--tools ""`), MitCad's tools allowed
// by `--allowedTools mcp__mitcad` (the window's own approvals are enforced
// by the live server), a session id MitCad chooses and `--resume` to go on
// with it (core/mcp/src/chat/claude.rs builds the command line). The
// process runs in an empty folder of its own; nothing is written into the
// user's Claude Code configuration.
//
// Setup: `claude --version` (at least CLAUDE_MINIMUM_VERSION), then `claude
// auth status` (exit status 0: signed in). Sign in runs `claude auth login`,
// Claude Code's own sign-in, which opens the browser; MitCad never sees a
// credential. A Windows launcher script (claude.cmd of an npm installation)
// is not started: only the native program.
//
// Stop: on Linux and macOS SIGINT, which ends the running turn; if the
// process is still running 5 s later it is ended. Windows has no way to
// deliver a console Ctrl+C to a program started without a console of its
// own, so the process is ended there; either way the next message goes on
// with `--resume` of the same session.

#include <functional>
#include <memory>
#include <optional>

#include <QByteArray>
#include <QProcess>
#include <QString>
#include <QStringList>

#include "ChatAgent.hpp"
#include "mitcad_bridge/lib.h"

class QTimer;

namespace mitcad::agent {

class ClaudeCodeAgent : public ChatAgent {
  Q_OBJECT

public:
  explicit ClaudeCodeAgent(QObject* parent = nullptr);
  ~ClaudeCodeAgent() override;

  QString id() const override { return QStringLiteral("claude"); }
  QString name() const override { return tr("Claude Code"); }
  QString installPage() const override { return QStringLiteral("https://code.claude.com/docs/en/setup"); }
  QString accountName() const override { return QStringLiteral("Claude"); }
  // The aliases --model takes (core/mcp/src/chat/claude.rs).
  QList<Model> models() const override;

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
  // A short run of the program (--version, auth status) with a time limit;
  // `done` gets its exit code (-1: it did not run) and standard output.
  void probe(const QStringList& arguments, std::function<void(int code, const QByteArray& output)> done);
  void setReadiness(Readiness readiness, const QString& detail = QString());
  void readOutput();
  void processEnded(int code);
  // The program found, or why it cannot be used.
  QString findProgram(QString* reason) const;

  QString m_program;
  Readiness m_readiness = Readiness::Checking;
  QString m_detail;
  int m_generation = 0; // stale probes' answers are dropped

  QProcess* m_process = nullptr;
  std::optional<rust::Box<ChatStream>> m_stream;
  QByteArray m_pending; // stdout after the last newline
  QByteArray m_errors;  // the end of stderr
  bool m_stopping = false;
  QTimer* m_stopTimer = nullptr;
  QProcess* m_signIn = nullptr;
};

} // namespace mitcad::agent
