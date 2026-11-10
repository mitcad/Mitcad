// SPDX-License-Identifier: MIT
#pragma once

// Sign in Again (mitcad#1585): the agent's own sign-out and sign-in run in a
// terminal window, so that the user signs in there as the agent asks
// (its browser page, a code to paste). MitCad writes the commands into a
// script of its own in the chat folder (`<agent>-sign-in-again.sh`, `.cmd`
// on Windows, `.command` on macOS) and starts it in the platform's terminal
// (SignInCommands.hpp chooses it): the commands are the agent's, with the
// same executable the chat runs; MitCad never sees what is typed there.
//
// ended() comes when the terminal's process ends; where the terminal
// program hands the window over and ends at once (Windows Terminal,
// Terminal.app's `open`, a terminal whose server keeps the window), when
// the user comes back to MitCad's window afterwards. The chat then runs
// Check again. The UI tests give a stand-in terminal through
// MITCAD_TEST_TERMINAL (a program given the script) or $TERMINAL on Linux.
//
// Logged: `Chat: Sign in Again: terminal <kind> at <program>` (or `no
// terminal found`), `Chat: Sign in Again: started <program> <arguments>;
// it runs <command> | <command>`, `Chat: Sign in Again: the terminal ended
// (exit <code>)`, `... returned at once; checking again when MitCad's
// window is active again`, `... back in MitCad's window`, `... the terminal
// for <agent> left open; starting one for <agent>`.

#include <QElapsedTimer>
#include <QList>
#include <QMetaObject>
#include <QObject>
#include <QString>
#include <QStringList>

#include "SignInCommands.hpp"

class QProcess;

namespace mitcad::agent {

class SignInTerminal : public QObject {
  Q_OBJECT

public:
  explicit SignInTerminal(QObject* parent = nullptr);
  ~SignInTerminal() override;

  // The shell of this platform's scripts.
  static ScriptShell shell();
  // The script's file for an agent in the chat folder.
  static QString scriptPath(const QString& agentId);
  // The terminal this computer offers for `script` (a file check); its
  // program empty: none.
  static TerminalLaunch find(const QString& script);

  // The terminal's process runs.
  bool open() const { return m_process != nullptr; }
  // It ended at once: the user's return to MitCad's window is waited for.
  bool waitingForReturn() const { return m_waiting; }
  // The agent it runs for.
  const QString& agentId() const { return m_agentId; }

  // Writes the script that shows `heading`, runs `commands` and shows
  // `closing`, and starts it in the terminal; false with `error` (no
  // terminal found, the script not written). True at once while this
  // agent's terminal is open; another agent's open terminal is let go
  // first (signInStart, mitcad#1610).
  bool start(const QString& agentId, const QString& heading, const QString& closing,
             const QList<QStringList>& commands, QString* error);
  // Forgets the terminal (it stays open for the user): no ended() comes.
  void forget();

signals:
  // Time for Check again.
  void ended();
  // The terminal could not run the script: why.
  void failed(const QString& reason);

private:
  void waitForReturn();

  QProcess* m_process = nullptr;
  QString m_agentId;
  QString m_kind;
  bool m_returnsAtOnce = false;
  bool m_waiting = false;
  bool m_sawInactive = false;
  QElapsedTimer m_clock;
  QMetaObject::Connection m_stateWatch;
};

} // namespace mitcad::agent
