// SPDX-License-Identifier: MIT
#include "SignInTerminal.hpp"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QProcess>
#include <QSaveFile>
#include <QStandardPaths>
#include <QtLogging>

#include "ChatSettings.hpp"

#ifdef Q_OS_WIN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

namespace mitcad::agent {
namespace {

// A terminal that ends sooner than this handed its window to another
// process (a terminal server) or failed: no one signs out and in again
// this fast.
constexpr qint64 kAtOnceMs = 2000;

#ifndef Q_OS_MACOS
QString findProgram(const QString& name) { return QStandardPaths::findExecutable(name); }
#endif

QString logged(const QString& program, const QStringList& arguments) {
  QStringList parts{QDir::toNativeSeparators(program)};
  parts += arguments;
  return parts.join(QLatin1Char(' '));
}

} // namespace

SignInTerminal::SignInTerminal(QObject* parent) : QObject(parent) {}

SignInTerminal::~SignInTerminal() {
  // The terminal stays open for the user (QProcess would end it): only its
  // end is no longer heard.
  if (m_process != nullptr) {
    m_process->disconnect(this);
    m_process->setParent(nullptr);
  }
  QObject::disconnect(m_stateWatch);
}

ScriptShell SignInTerminal::shell() {
#ifdef Q_OS_WIN
  return ScriptShell::WindowsCmd;
#else
  return ScriptShell::Posix;
#endif
}

QString SignInTerminal::scriptPath(const QString& agentId) {
  // Its name from the agent's id (letters, digits and dashes only), so that
  // no command line needs to quote it.
  QString name;
  for (const QChar c : agentId) {
    name += (c.isLetterOrNumber() && c.unicode() < 128) ? c.toLower() : QLatin1Char('-');
  }
#if defined(Q_OS_WIN)
  const QString suffix = QStringLiteral(".cmd");
#elif defined(Q_OS_MACOS)
  const QString suffix = QStringLiteral(".command");
#else
  const QString suffix = QStringLiteral(".sh");
#endif
  return QDir(chatDataDirectory()).filePath(name + QStringLiteral("-sign-in-again") + suffix);
}

TerminalLaunch SignInTerminal::find(const QString& script) {
  const QString test = qEnvironmentVariable("MITCAD_TEST_TERMINAL");
  if (!test.isEmpty()) {
    return testTerminal(test, QDir::toNativeSeparators(script));
  }
#if defined(Q_OS_WIN)
  return windowsTerminal(findProgram, qEnvironmentVariable("ComSpec"), QDir::toNativeSeparators(script));
#elif defined(Q_OS_MACOS)
  return macTerminal(script);
#else
  return linuxTerminal(qEnvironmentVariable("TERMINAL"), findProgram, script);
#endif
}

bool SignInTerminal::start(const QString& agentId, const QString& heading, const QString& closing,
                           const QList<QStringList>& commands, QString* error) {
  // One terminal at a time, for one agent (mitcad#1610): another agent's is
  // let go (left open for the user) and this agent's starts.
  const SignInStart how = signInStart(m_process != nullptr, m_agentId, agentId);
  if (how == SignInStart::AlreadyOpen) {
    return true;
  }
  if (how == SignInStart::ReplaceOther) {
    qInfo().noquote() << QStringLiteral("Chat: Sign in Again: the terminal for %1 left open; starting one for %2")
                             .arg(m_agentId, agentId);
  }
  forget();
  const QString script = scriptPath(agentId);
  const TerminalLaunch launch = find(script);
  if (launch.program.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Chat: Sign in Again: no terminal found");
    if (error != nullptr) {
      *error = tr("No terminal program was found.");
    }
    return false;
  }
  qInfo().noquote() << QStringLiteral("Chat: Sign in Again: terminal %1 at %2")
                           .arg(launch.kind, QDir::toNativeSeparators(launch.program));
  QSaveFile file(script);
  const bool written = QDir().mkpath(QFileInfo(script).absolutePath()) && file.open(QIODevice::WriteOnly) &&
                       file.write(signInScript(heading, closing, commands, shell()).toUtf8()) >= 0 &&
                       file.commit();
  if (!written ||
      !QFile::setPermissions(script, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner |
                                         QFileDevice::ReadUser | QFileDevice::WriteUser | QFileDevice::ExeUser)) {
    qInfo().noquote() << QStringLiteral("Chat: Sign in Again: the script %1 could not be written")
                             .arg(QDir::toNativeSeparators(script));
    if (error != nullptr) {
      *error = tr("The script %1 could not be written.").arg(QDir::toNativeSeparators(script));
    }
    return false;
  }
  m_agentId = agentId;
  m_kind = launch.kind;
  m_returnsAtOnce = launch.returnsAtOnce;
  m_process = new QProcess(this);
  m_process->setProgram(launch.program);
  m_process->setArguments(launch.arguments);
  m_process->setWorkingDirectory(launch.workingDirectory.isEmpty() ? QDir::homePath() : launch.workingDirectory);
  m_process->setStandardInputFile(QProcess::nullDevice());
  m_process->setProcessChannelMode(QProcess::ForwardedChannels);
#ifdef Q_OS_WIN
  if (launch.newConsole) {
    // A console window of its own, which its programs read and write (not
    // MitCad's channels).
    m_process->setCreateProcessArgumentsModifier([](QProcess::CreateProcessArguments* created) {
      created->flags &= ~static_cast<DWORD>(CREATE_NO_WINDOW);
      created->flags |= CREATE_NEW_CONSOLE;
      created->startupInfo->dwFlags &= ~static_cast<DWORD>(STARTF_USESTDHANDLES);
    });
  }
#endif
  QProcess* const process = m_process;
  connect(process, &QProcess::finished, this, [this, process](int code, QProcess::ExitStatus status) {
    if (process != m_process) {
      return;
    }
    const qint64 took = m_clock.elapsed();
    m_process->deleteLater();
    m_process = nullptr;
    const bool normal = status == QProcess::NormalExit;
    qInfo().noquote() << QStringLiteral("Chat: Sign in Again: the terminal ended (exit %1)")
                             .arg(normal ? QString::number(code) : QStringLiteral("crashed"));
    if (normal && code == 0 && (m_returnsAtOnce || took < kAtOnceMs)) {
      waitForReturn();
      return;
    }
    if (took < kAtOnceMs) {
      emit failed(tr("The terminal (%1) ended at once.").arg(m_kind));
      return;
    }
    emit ended();
  });
  connect(process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError failure) {
    if (failure != QProcess::FailedToStart || process != m_process) {
      return;
    }
    const QString reason = m_process->errorString();
    m_process->deleteLater();
    m_process = nullptr;
    qInfo().noquote() << QStringLiteral("Chat: Sign in Again: the terminal did not start: %1").arg(reason);
    emit failed(tr("The terminal (%1) did not start: %2").arg(m_kind, reason));
  });
  QStringList shown;
  for (const QStringList& command : commands) {
    shown << commandLine(command, shell());
  }
  qInfo().noquote() << QStringLiteral("Chat: Sign in Again: started %1; it runs %2")
                           .arg(logged(launch.program, launch.arguments), shown.join(QStringLiteral(" | ")));
  m_clock.start();
  m_process->start();
  return true;
}

void SignInTerminal::forget() {
  if (m_process != nullptr) {
    // Left open for the user; gone with its end.
    QProcess* const left = m_process;
    m_process = nullptr;
    left->disconnect(this);
    left->setParent(nullptr);
    connect(left, &QProcess::finished, left, &QObject::deleteLater);
  }
  QObject::disconnect(m_stateWatch);
  m_waiting = false;
  m_sawInactive = false;
}

void SignInTerminal::waitForReturn() {
  qInfo().noquote() << QStringLiteral(
      "Chat: Sign in Again: the terminal returned at once; checking again when MitCad's window is active again");
  m_waiting = true;
  // The terminal's window takes the focus: MitCad's window active again
  // after that is the user's return.
  m_sawInactive = QGuiApplication::applicationState() != Qt::ApplicationActive;
  QObject::disconnect(m_stateWatch);
  m_stateWatch = connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState now) {
    if (now != Qt::ApplicationActive) {
      m_sawInactive = true;
      return;
    }
    if (!m_waiting || !m_sawInactive) {
      return;
    }
    m_waiting = false;
    QObject::disconnect(m_stateWatch);
    qInfo().noquote() << QStringLiteral("Chat: Sign in Again: back in MitCad's window");
    emit ended();
  });
}

} // namespace mitcad::agent
