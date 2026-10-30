// SPDX-License-Identifier: MIT
#include "ClaudeCodeAgent.hpp"

#include <cstddef>
#include <string>
#include <utility>

#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QTimer>
#include <QtLogging>

#ifndef Q_OS_WIN
#include <csignal>
#include <sys/types.h>
#endif

#include "../mcp/AssistantSetup.hpp"
#include "ChatLogic.hpp"
#include "ChatSettings.hpp"

namespace mitcad::agent {
namespace {

// The longest output line read: a tool result with a picture of the view
// is a few megabytes.
constexpr qsizetype kMaxLine = 32 * 1024 * 1024;
// The end of the error output kept for the panel.
constexpr qsizetype kErrorTail = 4096;
constexpr int kProbeMs = 20000;
constexpr int kStopMs = 5000;

QString lastLines(const QByteArray& errors) {
  QStringList lines = QString::fromUtf8(errors).split(QLatin1Char('\n'), Qt::SkipEmptyParts);
  while (lines.size() > 3) {
    lines.removeFirst();
  }
  return lines.join(QLatin1Char(' ')).simplified();
}

} // namespace

ClaudeCodeAgent::ClaudeCodeAgent(QObject* parent) : ChatAgent(parent) {
  m_stopTimer = new QTimer(this);
  m_stopTimer->setSingleShot(true);
  connect(m_stopTimer, &QTimer::timeout, this, [this] {
    if (running()) {
      qInfo().noquote() << QStringLiteral("Chat: Claude Code did not stop its turn in %1 ms; ending it").arg(kStopMs);
      m_process->kill();
    }
  });
}

ClaudeCodeAgent::~ClaudeCodeAgent() {
  if (m_process != nullptr) {
    m_process->disconnect(this);
    m_process->kill();
    m_process->waitForFinished(1000);
  }
  if (m_signIn != nullptr) {
    m_signIn->disconnect(this);
  }
}

QString ClaudeCodeAgent::findProgram(QString* reason) const {
  const QString program = mcp::assistantExecutable(QStringLiteral("claude"));
  if (program.isEmpty()) {
    return QString();
  }
#ifdef Q_OS_WIN
  // An npm installation's launcher script: its arguments would go through
  // cmd.exe's own quoting. Only the native program is started.
  const QString suffix = QFileInfo(program).suffix().toLower();
  if (suffix == QStringLiteral("cmd") || suffix == QStringLiteral("bat") || suffix == QStringLiteral("ps1")) {
    if (reason != nullptr) {
      *reason = tr("Only an npm launcher of Claude Code (%1) was found; the chat needs Claude Code's native "
                   "installation.")
                    .arg(QFileInfo(program).fileName());
    }
    return QString();
  }
#else
  Q_UNUSED(reason);
#endif
  return program;
}

bool ClaudeCodeAgent::installedNow() const { return !findProgram(nullptr).isEmpty(); }

QList<ChatAgent::Model> ClaudeCodeAgent::models() const {
  QList<Model> found;
  const QJsonObject answer = chatRequest({{QStringLiteral("op"), QStringLiteral("claude_models")}});
  for (const QJsonValue& value : answer.value(QStringLiteral("models")).toArray()) {
    const QJsonObject model = value.toObject();
    found.append(Model{model.value(QStringLiteral("id")).toString(), model.value(QStringLiteral("name")).toString()});
  }
  return found;
}

void ClaudeCodeAgent::setReadiness(Readiness readiness, const QString& detail) {
  if (readiness == m_readiness && detail == m_detail) {
    return;
  }
  m_readiness = readiness;
  m_detail = detail;
  emit readinessChanged();
}

void ClaudeCodeAgent::probe(const QStringList& arguments, std::function<void(int code, const QByteArray& output)> done) {
  auto* process = new QProcess(this);
  process->setProgram(m_program);
  process->setArguments(arguments);
  process->setProcessChannelMode(QProcess::SeparateChannels);
  process->setStandardInputFile(QProcess::nullDevice());
  const QString folder = agentWorkDirectory();
  if (!folder.isEmpty() && QDir().mkpath(folder)) {
    process->setWorkingDirectory(folder);
  }
  const int generation = m_generation;
  // Once, whichever comes first: the end or a failure to start.
  const auto handled = std::make_shared<bool>(false);
  const auto finish = [this, process, generation, done, handled](int code) {
    if (*handled) {
      return;
    }
    *handled = true;
    const QByteArray output = code < 0 ? QByteArray() : process->readAllStandardOutput();
    process->deleteLater();
    if (generation == m_generation) {
      done(code, output);
    }
  };
  connect(process, &QProcess::finished, this, [finish](int code, QProcess::ExitStatus status) {
    finish(status == QProcess::NormalExit ? code : -1);
  });
  connect(process, &QProcess::errorOccurred, this, [finish](QProcess::ProcessError error) {
    if (error == QProcess::FailedToStart) {
      finish(-1);
    }
  });
  QTimer::singleShot(kProbeMs, process, [process] { process->kill(); });
  process->start();
}

void ClaudeCodeAgent::check() {
  ++m_generation;
  QString reason;
  m_program = findProgram(&reason);
  if (m_program.isEmpty()) {
    setReadiness(Readiness::Missing, reason);
    return;
  }
  if (m_readiness != Readiness::SigningIn) {
    setReadiness(Readiness::Checking);
  }
  probe({QStringLiteral("--version")}, [this](int code, const QByteArray& output) {
    const QJsonObject version =
        chatRequest({{QStringLiteral("op"), QStringLiteral("claude_version")},
                     {QStringLiteral("output"), QString::fromUtf8(output)}});
    if (code != 0 || version.value(QStringLiteral("version")).isNull()) {
      qInfo().noquote() << QStringLiteral("Chat: Claude Code at %1 did not tell its version (exit %2)")
                               .arg(QDir::toNativeSeparators(m_program))
                               .arg(code);
      setReadiness(Readiness::Missing, tr("Claude Code at %1 did not start.").arg(QDir::toNativeSeparators(m_program)));
      return;
    }
    const QString found = version.value(QStringLiteral("version")).toString();
    if (!version.value(QStringLiteral("ok")).toBool()) {
      qInfo().noquote() << QStringLiteral("Chat: Claude Code %1 is older than %2")
                               .arg(found, version.value(QStringLiteral("minimum")).toString());
      setReadiness(Readiness::Outdated, tr("Claude Code %1 is older than %2, which the chat needs.")
                                            .arg(found, version.value(QStringLiteral("minimum")).toString()));
      return;
    }
    probe({QStringLiteral("auth"), QStringLiteral("status")}, [this, found](int status, const QByteArray& answer) {
      const QJsonObject auth = chatRequest({{QStringLiteral("op"), QStringLiteral("claude_auth")},
                                            {QStringLiteral("exit_code"), status},
                                            {QStringLiteral("output"), QString::fromUtf8(answer)}});
      const bool signedIn = auth.value(QStringLiteral("signed_in")).toBool();
      qInfo().noquote() << QStringLiteral("Chat: Claude Code %1 found, %2")
                               .arg(found, signedIn ? QStringLiteral("signed in") : QStringLiteral("not signed in"));
      if (signedIn) {
        setReadiness(Readiness::Ready, found);
      } else if (m_signIn == nullptr) {
        setReadiness(Readiness::SignedOut);
      }
    });
  });
}

void ClaudeCodeAgent::signIn() {
  if (m_program.isEmpty() || m_signIn != nullptr) {
    return;
  }
  // Claude Code's own sign-in: it opens the browser and keeps what it gets.
  m_signIn = new QProcess(this);
  m_signIn->setProgram(m_program);
  m_signIn->setArguments({QStringLiteral("auth"), QStringLiteral("login")});
  m_signIn->setProcessChannelMode(QProcess::MergedChannels);
  m_signIn->setStandardInputFile(QProcess::nullDevice());
  const QString folder = agentWorkDirectory();
  if (!folder.isEmpty() && QDir().mkpath(folder)) {
    m_signIn->setWorkingDirectory(folder);
  }
  connect(m_signIn, &QProcess::readyReadStandardOutput, this, [this] {
    // The address it asks the user to open, in case the browser did not
    // open: shown in the panel.
    static const QRegularExpression address(QStringLiteral("https://[^\\s\"'<>]+"));
    const QString text = QString::fromUtf8(m_signIn->readAllStandardOutput());
    const QRegularExpressionMatch match = address.match(text);
    if (match.hasMatch()) {
      setReadiness(Readiness::SigningIn, match.captured(0));
    }
  });
  const auto ended = [this] {
    if (m_signIn == nullptr) {
      return;
    }
    m_signIn->deleteLater();
    m_signIn = nullptr;
    qInfo().noquote() << QStringLiteral("Chat: Claude Code's sign-in ended");
    setReadiness(Readiness::Checking);
    check();
  };
  connect(m_signIn, &QProcess::finished, this, ended);
  connect(m_signIn, &QProcess::errorOccurred, this, [ended](QProcess::ProcessError error) {
    if (error == QProcess::FailedToStart) {
      ended();
    }
  });
  qInfo().noquote() << QStringLiteral("Chat: Claude Code's sign-in started");
  setReadiness(Readiness::SigningIn);
  m_signIn->start();
}

bool ClaudeCodeAgent::running() const {
  return m_process != nullptr && m_process->state() != QProcess::NotRunning;
}

bool ClaudeCodeAgent::start(const Start& start, QString* error) {
  if (running()) {
    return true;
  }
  if (m_program.isEmpty()) {
    m_program = findProgram(error);
    if (m_program.isEmpty()) {
      return false;
    }
  }
  const QJsonObject answer = chatRequest({{QStringLiteral("op"), QStringLiteral("claude_arguments")},
                                         {QStringLiteral("session"), start.session},
                                         {QStringLiteral("resume"), start.resume},
                                         {QStringLiteral("bridge"), start.bridge},
                                         {QStringLiteral("instance"), start.instance},
                                         {QStringLiteral("chat"), start.chat},
                                         {QStringLiteral("model"), start.model}});
  if (answer.contains(QStringLiteral("error"))) {
    if (error != nullptr) {
      *error = answer.value(QStringLiteral("error")).toString();
    }
    return false;
  }
  QStringList arguments;
  for (const QJsonValue& argument : answer.value(QStringLiteral("arguments")).toArray()) {
    arguments << argument.toString();
  }
  const QString folder = agentWorkDirectory();
  if (folder.isEmpty() || !QDir().mkpath(folder)) {
    if (error != nullptr) {
      *error = tr("The chat's working folder could not be made.");
    }
    return false;
  }
  try {
    m_stream.emplace(new_chat_stream("claude"));
  } catch (const std::exception& failure) {
    if (error != nullptr) {
      *error = QString::fromUtf8(failure.what());
    }
    return false;
  }
  m_pending.clear();
  m_errors.clear();
  m_stopping = false;
  m_process = new QProcess(this);
  m_process->setProgram(m_program);
  m_process->setArguments(arguments);
  m_process->setWorkingDirectory(folder);
  m_process->setProcessChannelMode(QProcess::SeparateChannels);
  QProcess* process = m_process;
  connect(process, &QProcess::readyReadStandardOutput, this, [this, process] {
    if (process == m_process) {
      readOutput();
    }
  });
  connect(process, &QProcess::readyReadStandardError, this, [this, process] {
    if (process == m_process) {
      m_errors += m_process->readAllStandardError();
      if (m_errors.size() > kErrorTail) {
        m_errors = m_errors.right(kErrorTail);
      }
    }
  });
  connect(process, &QProcess::finished, this, [this, process](int code, QProcess::ExitStatus status) {
    if (process == m_process) {
      processEnded(status == QProcess::NormalExit ? code : -1);
    }
  });
  connect(process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError failure) {
    if (process == m_process && failure == QProcess::FailedToStart) {
      m_errors = m_process->errorString().toUtf8();
      processEnded(-1);
    }
  });
  qInfo().noquote() << QStringLiteral("Chat: Claude Code started (%1 session %2)")
                           .arg(start.resume ? QStringLiteral("resuming") : QStringLiteral("new"), start.session);
  m_process->start();
  return true;
}

bool ClaudeCodeAgent::send(const Message& message, QString* error) {
  if (!running()) {
    if (error != nullptr) {
      *error = tr("Claude Code is not running.");
    }
    return false;
  }
  QJsonArray pictures;
  for (const Picture& picture : message.pictures) {
    pictures.append(QJsonObject{{QStringLiteral("media_type"), picture.mediaType},
                                {QStringLiteral("data"), QString::fromLatin1(picture.base64)}});
  }
  const QJsonObject answer = chatRequest({{QStringLiteral("op"), QStringLiteral("claude_message")},
                                         {QStringLiteral("text"), message.text},
                                         {QStringLiteral("context"), message.context},
                                         {QStringLiteral("images"), pictures}});
  if (answer.contains(QStringLiteral("error"))) {
    if (error != nullptr) {
      *error = answer.value(QStringLiteral("error")).toString();
    }
    return false;
  }
  const QByteArray line = answer.value(QStringLiteral("line")).toString().toUtf8();
  if (m_process->write(line) != line.size()) {
    if (error != nullptr) {
      *error = tr("The message could not be given to Claude Code.");
    }
    return false;
  }
  return true;
}

void ClaudeCodeAgent::readOutput() {
  QProcess* const reading = m_process;
  m_pending += reading->readAllStandardOutput();
  qsizetype from = 0;
  for (qsizetype newline = m_pending.indexOf('\n'); newline >= 0; newline = m_pending.indexOf('\n', from)) {
    const qsizetype length = newline - from;
    if (length <= kMaxLine) {
      // Any bytes that are no UTF-8 replaced: the stream reads text.
      const QByteArray line = QString::fromUtf8(m_pending.constData() + from, length).toUtf8();
      const std::string events((*m_stream)->feed_line(rust::Str(line.constData(), static_cast<std::size_t>(line.size()))));
      const QJsonArray panelEvents = QJsonDocument::fromJson(QByteArray::fromStdString(events)).array();
      for (const QJsonValue& value : panelEvents) {
        const QJsonObject item = value.toObject();
        if (item.value(QStringLiteral("kind")).toString() == QStringLiteral("turn_end")) {
          m_stopTimer->stop();
          m_stopping = false;
        }
        emit panelEvent(item);
        if (m_process != reading) {
          return; // the conversation was ended meanwhile
        }
      }
    } else {
      qWarning().noquote() << QStringLiteral("Chat: an output line of Claude Code of %1 bytes was skipped").arg(length);
    }
    from = newline + 1;
  }
  m_pending.remove(0, from);
  if (m_pending.size() > kMaxLine) {
    qWarning().noquote() << QStringLiteral("Chat: an output line of Claude Code over %1 bytes was skipped").arg(kMaxLine);
    m_pending.clear();
  }
}

void ClaudeCodeAgent::processEnded(int code) {
  QProcess* process = m_process;
  if (process == nullptr) {
    return;
  }
  m_stopTimer->stop();
  if (process->bytesAvailable() > 0) {
    readOutput();
    if (m_process != process) {
      return; // the conversation was ended meanwhile
    }
  }
  const bool stopped = m_stopping;
  const QString errors = lastLines(m_errors);
  m_process = nullptr;
  m_stream.reset();
  m_stopping = false;
  process->deleteLater();
  qInfo().noquote() << QStringLiteral("Chat: Claude Code ended (exit %1%2)%3")
                           .arg(code)
                           .arg(stopped ? QStringLiteral(", stopped") : QString())
                           .arg(errors.isEmpty() ? QString() : QStringLiteral(": ") + errors);
  emit exited(code, errors, stopped);
}

void ClaudeCodeAgent::stop() {
  if (!running()) {
    return;
  }
  m_stopping = true;
#ifdef Q_OS_WIN
  // No console Ctrl+C reaches a program started without a console of its
  // own: the process ends, and the next message resumes its session.
  qInfo().noquote() << QStringLiteral("Chat: stopping Claude Code (ending the process)");
  m_process->kill();
#else
  // SIGINT ends Claude Code's running turn.
  qInfo().noquote() << QStringLiteral("Chat: stopping Claude Code (SIGINT)");
  ::kill(static_cast<pid_t>(m_process->processId()), SIGINT);
  m_stopTimer->start(kStopMs);
#endif
}

void ClaudeCodeAgent::shutdown() {
  if (m_process == nullptr) {
    return;
  }
  QProcess* process = m_process;
  m_process = nullptr;
  m_stream.reset();
  m_stopTimer->stop();
  process->disconnect(this);
  if (process->state() == QProcess::NotRunning) {
    process->deleteLater();
    return;
  }
  // End of input ends Claude Code; one that does not end soon is ended.
  process->closeWriteChannel();
  connect(process, &QProcess::finished, process, &QObject::deleteLater);
  QTimer::singleShot(3000, process, [process] {
    if (process->state() != QProcess::NotRunning) {
      process->kill();
    }
  });
  qInfo().noquote() << QStringLiteral("Chat: Claude Code's conversation closed");
}

} // namespace mitcad::agent
