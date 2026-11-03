// SPDX-License-Identifier: MIT
#include "AcpAgent.hpp"

#include <cstddef>
#include <string>

#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QTimer>
#include <QtLogging>

#include "../mcp/AssistantSetup.hpp"
#include "ChatLogic.hpp"
#include "ChatSettings.hpp"

namespace mitcad::agent {
namespace {

// The longest output line read: a tool result with a picture of the view
// is a few megabytes.
constexpr qsizetype kMaxLine = 32 * 1024 * 1024;
constexpr qsizetype kErrorTail = 4096;
// The most a command may write before it answers the handshake.
constexpr qsizetype kMaxHandshake = 1024 * 1024;
constexpr int kHandshakeMs = 20000;
constexpr int kStopMs = 5000;
// The models the agent offered last ("id\tname" each), and the command
// line they belong to.
const char* const kModelsKey = "chat/models/acp";
const char* const kModelsCommandKey = "chat/models/acp-command";

QString lastLines(const QByteArray& errors) {
  QStringList lines = QString::fromUtf8(errors).split(QLatin1Char('\n'), Qt::SkipEmptyParts);
  while (lines.size() > 3) {
    lines.removeFirst();
  }
  return lines.join(QLatin1Char(' ')).simplified();
}

QJsonArray eventsOf(const std::string& text) {
  const QJsonDocument document = QJsonDocument::fromJson(QByteArray::fromStdString(text));
  if (document.isArray()) {
    return document.array();
  }
  // A refused control: {"error": ...} as one event.
  QJsonArray events;
  events.append(QJsonObject{{QStringLiteral("kind"), QStringLiteral("refused")},
                            {QStringLiteral("text"), document.object().value(QStringLiteral("error")).toString()}});
  return events;
}

QString commandLine() { return customAgentCommand() + QLatin1Char('\n') + customAgentArguments(); }

// A process that is no longer needed: deleted once it ended.
void discard(QProcess* process) {
  if (process->state() == QProcess::NotRunning) {
    process->deleteLater();
    return;
  }
  QObject::connect(process, &QProcess::finished, process, &QObject::deleteLater);
  process->closeWriteChannel();
  process->kill();
}

} // namespace

AcpAgent::AcpAgent(QObject* parent) : ChatAgent(parent) {
  m_stopTimer = new QTimer(this);
  m_stopTimer->setSingleShot(true);
  connect(m_stopTimer, &QTimer::timeout, this, [this] {
    if (running()) {
      qInfo().noquote() << QStringLiteral("Chat: the custom command did not stop its prompt in %1 ms; ending it").arg(kStopMs);
      m_process->kill();
    }
  });
  // The models it offered last, before it runs again: only for the same
  // command line.
  const QSettings settings;
  if (settings.value(QLatin1String(kModelsCommandKey)).toString() == commandLine()) {
    for (const QString& entry : settings.value(QLatin1String(kModelsKey)).toStringList()) {
      const QStringList parts = entry.split(QLatin1Char('\t'));
      if (!parts.isEmpty() && !parts.first().isEmpty()) {
        m_models.append(Model{parts.first(), parts.size() > 1 ? parts.at(1) : parts.first()});
      }
    }
  }
}

AcpAgent::~AcpAgent() {
  if (m_probe != nullptr) {
    m_probe->disconnect(this);
    m_probe->kill();
    m_probe->waitForFinished(1000);
  }
  if (m_process != nullptr) {
    m_process->disconnect(this);
    m_process->kill();
    m_process->waitForFinished(1000);
  }
}

AcpAgent::Command AcpAgent::command() const {
  Command found;
  const QString given = customAgentCommand();
  if (given.isEmpty()) {
    found.reason = tr("No command is set for the custom agent: set it in Preferences.");
    return found;
  }
  const QJsonObject split = chatRequest({{QStringLiteral("op"), QStringLiteral("acp_command")},
                                         {QStringLiteral("command"), given},
                                         {QStringLiteral("arguments"), customAgentArguments()}});
  if (split.contains(QStringLiteral("error"))) {
    found.reason = tr("The custom command cannot be used: %1.").arg(split.value(QStringLiteral("error")).toString());
    return found;
  }
  const QString written = split.value(QStringLiteral("program")).toString();
  QString program;
  if (written.contains(QLatin1Char('/')) || written.contains(QLatin1Char('\\'))) {
    // A path: used as it is.
    const QFileInfo file(QDir::cleanPath(QDir::fromNativeSeparators(written)));
    if (file.isFile() && file.isExecutable()) {
      program = file.absoluteFilePath();
    }
  } else {
    // A name: looked for as an assistant's program is (PATH and the usual
    // folders of a user's installations).
    program = mcp::assistantExecutable(written);
  }
  if (program.isEmpty()) {
    found.reason = tr("The custom command %1 was not found.").arg(QDir::toNativeSeparators(written));
    return found;
  }
#ifdef Q_OS_WIN
  // A launcher script's arguments would go through cmd.exe's own quoting;
  // the command must be the agent's program itself.
  const QString suffix = QFileInfo(program).suffix().toLower();
  if (suffix == QStringLiteral("cmd") || suffix == QStringLiteral("bat") || suffix == QStringLiteral("ps1")) {
    found.reason = tr("The custom command %1 is a launcher script; set the agent's program itself.")
                       .arg(QDir::toNativeSeparators(program));
    return found;
  }
#endif
  found.program = program;
  for (const QJsonValue& argument : split.value(QStringLiteral("arguments")).toArray()) {
    found.arguments << argument.toString();
  }
  return found;
}

QString AcpAgent::signature(const Command& found) {
  const QFileInfo file(found.program);
  return QStringList{found.program, found.arguments.join(QLatin1Char('\n')),
                     file.lastModified().toString(Qt::ISODateWithMs), QString::number(file.size())}
      .join(QLatin1Char('\n'));
}

bool AcpAgent::installedNow() const {
  if (!customAgentEnabled()) {
    return false;
  }
  const Command found = command();
  return !found.program.isEmpty() && signature(found) != m_failed;
}

void AcpAgent::setReadiness(Readiness readiness, const QString& detail) {
  if (readiness == m_readiness && detail == m_detail) {
    return;
  }
  m_readiness = readiness;
  m_detail = detail;
  emit readinessChanged();
}

void AcpAgent::check() {
  ++m_generation;
  if (m_probe != nullptr) {
    QProcess* previous = m_probe;
    m_probe = nullptr;
    previous->disconnect(this);
    discard(previous);
  }
  const Command found = command();
  if (found.program.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Chat: the custom command cannot start: %1").arg(found.reason);
    setReadiness(Readiness::Missing, found.reason);
    return;
  }
  const QString key = signature(found);
  if (key == m_passed && m_readiness == Readiness::Ready) {
    return;
  }
  setReadiness(Readiness::Checking);
  probe(found, key);
}

void AcpAgent::probe(const Command& found, const QString& key) {
  auto* process = new QProcess(this);
  m_probe = process;
  process->setProgram(found.program);
  process->setArguments(found.arguments);
  process->setProcessChannelMode(QProcess::SeparateChannels);
  const QString folder = agentWorkDirectory();
  if (!folder.isEmpty() && QDir().mkpath(folder)) {
    process->setWorkingDirectory(folder);
  }
  const QString program = found.program;
  // The state line names the command as it was set.
  const QString shown = customAgentCommand();
  const auto output = std::make_shared<QByteArray>();
  const auto errors = std::make_shared<QByteArray>();
  connect(process, &QProcess::readyReadStandardOutput, this, [this, process, key, program, shown, output] {
    *output += process->readAllStandardOutput();
    if (output->size() > kMaxHandshake) {
      finishProbe(process, key, program, shown, false, tr("it wrote more than %1 bytes without answering").arg(kMaxHandshake));
      return;
    }
    const QJsonObject answer = chatRequest({{QStringLiteral("op"), QStringLiteral("acp_handshake_answer")},
                                            {QStringLiteral("output"), QString::fromUtf8(*output)}});
    const QString state = answer.value(QStringLiteral("state")).toString();
    if (state == QStringLiteral("ok")) {
      finishProbe(process, key, program, shown, true, answer.value(QStringLiteral("agent")).toString());
    } else if (state == QStringLiteral("failed")) {
      finishProbe(process, key, program, shown, false, answer.value(QStringLiteral("reason")).toString());
    }
  });
  connect(process, &QProcess::readyReadStandardError, this, [process, errors] {
    *errors += process->readAllStandardError();
    if (errors->size() > kErrorTail) {
      *errors = errors->right(kErrorTail);
    }
  });
  connect(process, &QProcess::finished, this, [this, process, key, program, shown, errors](int code, QProcess::ExitStatus) {
    const QString tail = lastLines(*errors);
    finishProbe(process, key, program, shown, false,
                tail.isEmpty() ? tr("it ended (exit status %1)").arg(code)
                               : tr("it ended (exit status %1): %2").arg(code).arg(tail));
  });
  connect(process, &QProcess::errorOccurred, this, [this, process, key, program, shown](QProcess::ProcessError failure) {
    if (failure == QProcess::FailedToStart) {
      finishProbe(process, key, program, shown, false, process->errorString());
    }
  });
  QTimer::singleShot(kHandshakeMs, process, [this, process, key, program, shown] {
    finishProbe(process, key, program, shown, false, tr("no answer in %1 s").arg(kHandshakeMs / 1000));
  });
  qInfo().noquote() << QStringLiteral("Chat: the custom command %1 asked for the Agent Client Protocol handshake")
                           .arg(QDir::toNativeSeparators(program));
  process->start();
  const QByteArray line =
      chatRequest({{QStringLiteral("op"), QStringLiteral("acp_handshake")}}).value(QStringLiteral("line")).toString().toUtf8();
  process->write(line);
}

void AcpAgent::finishProbe(QProcess* process, const QString& key, const QString& program, const QString& shown, bool ok,
                           const QString& text) {
  if (process != m_probe) {
    return;
  }
  m_probe = nullptr;
  process->disconnect(this);
  discard(process);
  if (ok) {
    m_passed = key;
    m_failed.clear();
    qInfo().noquote() << QStringLiteral("Chat: the custom command %1 answered the Agent Client Protocol handshake (%2)")
                             .arg(QDir::toNativeSeparators(program), text);
    setReadiness(Readiness::Ready, text);
    return;
  }
  m_failed = key;
  m_passed.clear();
  qInfo().noquote() << QStringLiteral("Chat: the custom command %1 did not answer the Agent Client Protocol handshake: %2")
                           .arg(QDir::toNativeSeparators(program), text);
  setReadiness(Readiness::Missing, tr("%1 did not answer the Agent Client Protocol handshake: %2.").arg(shown, text));
}

QString AcpAgent::privacyNotice() const {
  const QString given = customAgentCommand();
  return tr("What you write here, the selection, the open comments and pictures of the view go to the custom command "
            "%1, and to wherever it sends them. The agent changes this design through MitCad's tools.")
      .arg(given.isEmpty() ? name() : given);
}

QString AcpAgent::accountHelp() const {
  return tr("Sign in again with the custom command's own tools, in a terminal, using an account allowed to use it; "
            "MitCad does not sign in for it. Ask your administrator about access, or check billing and wait for a credit "
            "or rate limit to reset. Then choose Check again. The check sends one short request without your design or "
            "conversation; account usage may apply. New Conversation (+) starts a fresh process.");
}

bool AcpAgent::running() const { return m_process != nullptr && m_process->state() != QProcess::NotRunning; }

bool AcpAgent::control(const QJsonObject& request, QString* error) {
  if (!m_stream) {
    if (error != nullptr) {
      *error = tr("The custom command is not running.");
    }
    return false;
  }
  const QByteArray text = QJsonDocument(request).toJson(QJsonDocument::Compact);
  const std::string answer((*m_stream)->control(rust::Str(text.constData(), static_cast<std::size_t>(text.size()))));
  const QJsonArray events = eventsOf(answer);
  if (events.size() == 1 && events.at(0).toObject().value(QStringLiteral("kind")).toString() == QStringLiteral("refused")) {
    if (error != nullptr) {
      *error = events.at(0).toObject().value(QStringLiteral("text")).toString();
    }
    return false;
  }
  handle(events);
  return true;
}

bool AcpAgent::start(const Start& start, QString* error) {
  if (running()) {
    return true;
  }
  const Command found = command();
  if (found.program.isEmpty()) {
    if (error != nullptr) {
      *error = found.reason;
    }
    return false;
  }
  const QString folder = agentWorkDirectory();
  if (folder.isEmpty() || !QDir().mkpath(folder)) {
    if (error != nullptr) {
      *error = tr("The chat's working folder could not be made.");
    }
    return false;
  }
  try {
    m_stream.emplace(new_chat_stream("acp"));
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
  m_process->setProgram(found.program);
  m_process->setArguments(found.arguments);
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
  qInfo().noquote() << (start.resume ? QStringLiteral("Chat: Custom command started (resuming session %1)").arg(start.session)
                                     : QStringLiteral("Chat: Custom command started (new session)"));
  m_process->start();
  // The handshake and the session: written once the process reads.
  const bool started = control({{QStringLiteral("op"), QStringLiteral("start")},
                                {QStringLiteral("session"), start.session},
                                {QStringLiteral("resume"), start.resume},
                                {QStringLiteral("bridge"), start.bridge},
                                {QStringLiteral("instance"), start.instance},
                                {QStringLiteral("chat"), start.chat},
                                {QStringLiteral("model"), start.model},
                                {QStringLiteral("cwd"), QDir::toNativeSeparators(folder)}},
                               error);
  if (!started) {
    QProcess* failed = m_process;
    m_process = nullptr;
    m_stream.reset();
    failed->disconnect(this);
    discard(failed);
    return false;
  }
  return true;
}

bool AcpAgent::send(const Message& message, QString* error) {
  if (!running()) {
    if (error != nullptr) {
      *error = tr("The custom command is not running.");
    }
    return false;
  }
  QJsonArray pictures;
  for (const Picture& picture : message.pictures) {
    pictures.append(QJsonObject{{QStringLiteral("media_type"), picture.mediaType},
                                {QStringLiteral("data"), QString::fromLatin1(picture.base64)}});
  }
  return control({{QStringLiteral("op"), QStringLiteral("message")},
                  {QStringLiteral("text"), message.text},
                  {QStringLiteral("context"), message.context},
                  {QStringLiteral("images"), pictures}},
                 error);
}

void AcpAgent::setModels(const QJsonArray& listed) {
  QList<Model> found;
  QStringList kept;
  for (const QJsonValue& value : listed) {
    const QJsonObject model = value.toObject();
    const QString identifier = model.value(QStringLiteral("id")).toString();
    const QString label = model.value(QStringLiteral("name")).toString(identifier);
    if (identifier.isEmpty()) {
      continue;
    }
    found.append(Model{identifier, label});
    kept << identifier + QLatin1Char('\t') + label;
  }
  QSettings settings;
  settings.setValue(QLatin1String(kModelsKey), kept);
  settings.setValue(QLatin1String(kModelsCommandKey), commandLine());
  qInfo().noquote() << QStringLiteral("Chat: the custom command offers %1 model(s)").arg(found.size());
  m_models = found;
  emit modelsChanged();
}

void AcpAgent::handle(const QJsonArray& events) {
  QProcess* const reading = m_process;
  for (const QJsonValue& value : events) {
    if (m_process != reading || m_process == nullptr) {
      return; // the conversation was ended meanwhile
    }
    const QJsonObject item = value.toObject();
    const QString kind = item.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("write")) {
      const QByteArray line = item.value(QStringLiteral("line")).toString().toUtf8();
      if (m_process->write(line) != line.size()) {
        qWarning().noquote() << QStringLiteral("Chat: a line could not be given to the custom command");
      }
      continue;
    }
    if (kind == QStringLiteral("end")) {
      // The conversation cannot go on: the process ends with this reason.
      const QString text = item.value(QStringLiteral("text")).toString();
      qInfo().noquote() << QStringLiteral("Chat: the custom command's conversation cannot go on: %1").arg(text);
      m_errors = text.toUtf8();
      m_process->closeWriteChannel();
      m_process->kill();
      continue;
    }
    if (kind == QStringLiteral("models")) {
      setModels(item.value(QStringLiteral("models")).toArray());
      continue;
    }
    if (kind == QStringLiteral("turn_end")) {
      m_stopTimer->stop();
      m_stopping = false;
    }
    emit panelEvent(item);
  }
}

void AcpAgent::readOutput() {
  QProcess* const reading = m_process;
  m_pending += reading->readAllStandardOutput();
  qsizetype from = 0;
  for (qsizetype newline = m_pending.indexOf('\n'); newline >= 0; newline = m_pending.indexOf('\n', from)) {
    const qsizetype length = newline - from;
    if (length <= kMaxLine) {
      const QByteArray line = QString::fromUtf8(m_pending.constData() + from, length).toUtf8();
      const std::string events((*m_stream)->feed_line(rust::Str(line.constData(), static_cast<std::size_t>(line.size()))));
      handle(eventsOf(events));
      if (m_process != reading) {
        return; // the conversation was ended meanwhile
      }
    } else {
      qWarning().noquote() << QStringLiteral("Chat: an output line of the custom command of %1 bytes was skipped").arg(length);
    }
    from = newline + 1;
  }
  m_pending.remove(0, from);
  if (m_pending.size() > kMaxLine) {
    qWarning().noquote() << QStringLiteral("Chat: an output line of the custom command over %1 bytes was skipped").arg(kMaxLine);
    m_pending.clear();
  }
}

void AcpAgent::processEnded(int code) {
  QProcess* process = m_process;
  if (process == nullptr) {
    return;
  }
  m_stopTimer->stop();
  if (process->bytesAvailable() > 0) {
    readOutput();
    if (m_process != process) {
      return;
    }
  }
  const bool stopped = m_stopping;
  const QString errors = lastLines(m_errors);
  m_process = nullptr;
  m_stream.reset();
  m_stopping = false;
  process->deleteLater();
  qInfo().noquote() << QStringLiteral("Chat: Custom command ended (exit %1%2)%3")
                           .arg(code)
                           .arg(stopped ? QStringLiteral(", stopped") : QString())
                           .arg(errors.isEmpty() ? QString() : QStringLiteral(": ") + errors);
  emit exited(code, errors, stopped);
}

void AcpAgent::stop() {
  if (!running()) {
    return;
  }
  m_stopping = true;
  qInfo().noquote() << QStringLiteral("Chat: stopping the custom command (session/cancel)");
  m_stopTimer->start(kStopMs);
  QString error;
  if (!control({{QStringLiteral("op"), QStringLiteral("interrupt")}}, &error)) {
    qInfo().noquote() << QStringLiteral("Chat: the custom command's prompt could not be cancelled (%1); ending it").arg(error);
    m_process->kill();
  }
}

void AcpAgent::shutdown() {
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
  // The end of its input ends the agent; one that does not end soon is
  // ended.
  process->closeWriteChannel();
  connect(process, &QProcess::finished, process, &QObject::deleteLater);
  QTimer::singleShot(3000, process, [process] {
    if (process->state() != QProcess::NotRunning) {
      process->kill();
    }
  });
  qInfo().noquote() << QStringLiteral("Chat: the custom command's conversation closed");
}

} // namespace mitcad::agent
