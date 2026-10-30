// SPDX-License-Identifier: MIT
#include "CodexAgent.hpp"

#include <cstddef>
#include <string>
#include <utility>

#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
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
constexpr int kProbeMs = 20000;
constexpr int kStopMs = 5000;
// The settings key of the models Codex offered last ("id\tname" each).
const char* const kModelsKey = "chat/models/codex";

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
  // A refused control: {"error": ...} as one status event.
  QJsonArray events;
  events.append(QJsonObject{{QStringLiteral("kind"), QStringLiteral("refused")},
                            {QStringLiteral("text"), document.object().value(QStringLiteral("error")).toString()}});
  return events;
}

} // namespace

CodexAgent::CodexAgent(QObject* parent) : ChatAgent(parent) {
  m_stopTimer = new QTimer(this);
  m_stopTimer->setSingleShot(true);
  connect(m_stopTimer, &QTimer::timeout, this, [this] {
    if (running()) {
      qInfo().noquote() << QStringLiteral("Chat: Codex did not stop its turn in %1 ms; ending it").arg(kStopMs);
      m_process->kill();
    }
  });
  // The models it offered last, before it runs again.
  for (const QString& entry : QSettings().value(QLatin1String(kModelsKey)).toStringList()) {
    const QStringList parts = entry.split(QLatin1Char('\t'));
    if (!parts.isEmpty() && !parts.first().isEmpty()) {
      m_models.append(Model{parts.first(), parts.size() > 1 ? parts.at(1) : parts.first()});
    }
  }
}

CodexAgent::~CodexAgent() {
  if (m_process != nullptr) {
    m_process->disconnect(this);
    m_process->kill();
    m_process->waitForFinished(1000);
  }
  if (m_signIn != nullptr) {
    m_signIn->disconnect(this);
  }
}

QString CodexAgent::findProgram(QString* reason) const {
  const QString program = mcp::assistantExecutable(QStringLiteral("codex"));
  if (program.isEmpty()) {
    return QString();
  }
#ifdef Q_OS_WIN
  // An npm installation's launcher script: its arguments would go through
  // cmd.exe's own quoting. The native program its package carries is
  // started instead, when it is there.
  const QFileInfo found(program);
  const QString suffix = found.suffix().toLower();
  if (suffix == QStringLiteral("cmd") || suffix == QStringLiteral("bat") || suffix == QStringLiteral("ps1")) {
    const QDir package(found.dir().filePath(QStringLiteral("node_modules/@openai/codex/vendor")));
    for (const QString& target : {QStringLiteral("x86_64-pc-windows-msvc"), QStringLiteral("aarch64-pc-windows-msvc")}) {
      for (const QString& name : {QStringLiteral("codex/codex.exe"), QStringLiteral("codex.exe")}) {
        const QFileInfo native(package.filePath(target + QLatin1Char('/') + name));
        if (native.isFile() && native.isExecutable()) {
          return native.canonicalFilePath();
        }
      }
    }
    if (reason != nullptr) {
      *reason = tr("Only an npm launcher of Codex (%1) was found, without its native program.").arg(found.fileName());
    }
    return QString();
  }
#else
  Q_UNUSED(reason);
#endif
  return program;
}

bool CodexAgent::installedNow() const { return !findProgram(nullptr).isEmpty(); }

void CodexAgent::setReadiness(Readiness readiness, const QString& detail) {
  if (readiness == m_readiness && detail == m_detail) {
    return;
  }
  m_readiness = readiness;
  m_detail = detail;
  emit readinessChanged();
}

void CodexAgent::probe(const QStringList& arguments, std::function<void(int code, const QByteArray& output)> done) {
  auto* process = new QProcess(this);
  process->setProgram(m_program);
  process->setArguments(arguments);
  // `codex login status` may say it on either channel.
  process->setProcessChannelMode(QProcess::MergedChannels);
  process->setStandardInputFile(QProcess::nullDevice());
  const QString folder = agentWorkDirectory();
  if (!folder.isEmpty() && QDir().mkpath(folder)) {
    process->setWorkingDirectory(folder);
  }
  const int generation = m_generation;
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
  connect(process, &QProcess::errorOccurred, this, [finish](QProcess::ProcessError failure) {
    if (failure == QProcess::FailedToStart) {
      finish(-1);
    }
  });
  QTimer::singleShot(kProbeMs, process, [process] { process->kill(); });
  process->start();
}

void CodexAgent::check() {
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
    const QJsonObject version = chatRequest(
        {{QStringLiteral("op"), QStringLiteral("codex_version")}, {QStringLiteral("output"), QString::fromUtf8(output)}});
    if (code != 0 || version.value(QStringLiteral("version")).isNull()) {
      qInfo().noquote() << QStringLiteral("Chat: Codex at %1 did not tell its version (exit %2)")
                               .arg(QDir::toNativeSeparators(m_program))
                               .arg(code);
      setReadiness(Readiness::Missing, tr("Codex at %1 did not start.").arg(QDir::toNativeSeparators(m_program)));
      return;
    }
    const QString found = version.value(QStringLiteral("version")).toString();
    if (!version.value(QStringLiteral("ok")).toBool()) {
      qInfo().noquote() << QStringLiteral("Chat: Codex %1 is older than %2")
                               .arg(found, version.value(QStringLiteral("minimum")).toString());
      setReadiness(Readiness::Outdated, tr("Codex %1 is older than %2, which the chat needs.")
                                            .arg(found, version.value(QStringLiteral("minimum")).toString()));
      return;
    }
    probe({QStringLiteral("login"), QStringLiteral("status")}, [this, found](int status, const QByteArray& answer) {
      const QJsonObject auth = chatRequest({{QStringLiteral("op"), QStringLiteral("codex_auth")},
                                            {QStringLiteral("exit_code"), status},
                                            {QStringLiteral("output"), QString::fromUtf8(answer)}});
      const bool signedIn = auth.value(QStringLiteral("signed_in")).toBool();
      qInfo().noquote() << QStringLiteral("Chat: Codex %1 found, %2")
                               .arg(found, signedIn ? QStringLiteral("signed in") : QStringLiteral("not signed in"));
      if (signedIn) {
        setReadiness(Readiness::Ready, found);
      } else if (m_signIn == nullptr) {
        setReadiness(Readiness::SignedOut);
      }
    });
  });
}

void CodexAgent::signIn() {
  if (m_program.isEmpty() || m_signIn != nullptr) {
    return;
  }
  // Codex's own sign-in: it opens the browser and keeps what it gets.
  m_signIn = new QProcess(this);
  m_signIn->setProgram(m_program);
  m_signIn->setArguments({QStringLiteral("login")});
  m_signIn->setProcessChannelMode(QProcess::MergedChannels);
  m_signIn->setStandardInputFile(QProcess::nullDevice());
  const QString folder = agentWorkDirectory();
  if (!folder.isEmpty() && QDir().mkpath(folder)) {
    m_signIn->setWorkingDirectory(folder);
  }
  connect(m_signIn, &QProcess::readyReadStandardOutput, this, [this] {
    // The address it asks the user to open, in case the browser did not
    // open: shown in the panel (not its local callback address).
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
    qInfo().noquote() << QStringLiteral("Chat: Codex's sign-in ended");
    setReadiness(Readiness::Checking);
    check();
  };
  connect(m_signIn, &QProcess::finished, this, ended);
  connect(m_signIn, &QProcess::errorOccurred, this, [ended](QProcess::ProcessError failure) {
    if (failure == QProcess::FailedToStart) {
      ended();
    }
  });
  qInfo().noquote() << QStringLiteral("Chat: Codex's sign-in started");
  setReadiness(Readiness::SigningIn);
  m_signIn->start();
}

bool CodexAgent::running() const { return m_process != nullptr && m_process->state() != QProcess::NotRunning; }

bool CodexAgent::control(const QJsonObject& request, QString* error) {
  if (!m_stream) {
    if (error != nullptr) {
      *error = tr("Codex is not running.");
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

bool CodexAgent::start(const Start& start, QString* error) {
  if (running()) {
    return true;
  }
  if (m_program.isEmpty()) {
    m_program = findProgram(error);
    if (m_program.isEmpty()) {
      return false;
    }
  }
  const QString folder = agentWorkDirectory();
  if (folder.isEmpty() || !QDir().mkpath(folder)) {
    if (error != nullptr) {
      *error = tr("The chat's working folder could not be made.");
    }
    return false;
  }
  const QJsonObject command = chatRequest({{QStringLiteral("op"), QStringLiteral("codex_arguments")}});
  QStringList arguments;
  for (const QJsonValue& argument : command.value(QStringLiteral("arguments")).toArray()) {
    arguments << argument.toString();
  }
  try {
    m_stream.emplace(new_chat_stream("codex"));
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
  qInfo().noquote() << (start.resume ? QStringLiteral("Chat: Codex started (resuming thread %1)").arg(start.session)
                                     : QStringLiteral("Chat: Codex started (new thread)"));
  m_process->start();
  // The handshake and the thread: written once the process reads.
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
    failed->kill();
    failed->deleteLater();
    return false;
  }
  return true;
}

bool CodexAgent::send(const Message& message, QString* error) {
  if (!running()) {
    if (error != nullptr) {
      *error = tr("Codex is not running.");
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

void CodexAgent::setModels(const QJsonArray& listed) {
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
  QSettings().setValue(QLatin1String(kModelsKey), kept);
  qInfo().noquote() << QStringLiteral("Chat: Codex offers %1 model(s)").arg(found.size());
  m_models = found;
  emit modelsChanged();
}

void CodexAgent::handle(const QJsonArray& events) {
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
        qWarning().noquote() << QStringLiteral("Chat: a line could not be given to Codex");
      }
      continue;
    }
    if (kind == QStringLiteral("end")) {
      // The conversation cannot go on: the process ends with this reason.
      const QString text = item.value(QStringLiteral("text")).toString();
      qInfo().noquote() << QStringLiteral("Chat: Codex's conversation cannot go on: %1").arg(text);
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

void CodexAgent::readOutput() {
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
      qWarning().noquote() << QStringLiteral("Chat: an output line of Codex of %1 bytes was skipped").arg(length);
    }
    from = newline + 1;
  }
  m_pending.remove(0, from);
  if (m_pending.size() > kMaxLine) {
    qWarning().noquote() << QStringLiteral("Chat: an output line of Codex over %1 bytes was skipped").arg(kMaxLine);
    m_pending.clear();
  }
}

void CodexAgent::processEnded(int code) {
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
  qInfo().noquote() << QStringLiteral("Chat: Codex ended (exit %1%2)%3")
                           .arg(code)
                           .arg(stopped ? QStringLiteral(", stopped") : QString())
                           .arg(errors.isEmpty() ? QString() : QStringLiteral(": ") + errors);
  emit exited(code, errors, stopped);
}

void CodexAgent::stop() {
  if (!running()) {
    return;
  }
  m_stopping = true;
  qInfo().noquote() << QStringLiteral("Chat: stopping Codex (turn/interrupt)");
  m_stopTimer->start(kStopMs);
  QString error;
  if (!control({{QStringLiteral("op"), QStringLiteral("interrupt")}}, &error)) {
    qInfo().noquote() << QStringLiteral("Chat: Codex's turn could not be interrupted (%1); ending it").arg(error);
    m_process->kill();
  }
}

void CodexAgent::shutdown() {
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
  // The end of its input ends the app server; one that does not end soon
  // is ended.
  process->closeWriteChannel();
  connect(process, &QProcess::finished, process, &QObject::deleteLater);
  QTimer::singleShot(3000, process, [process] {
    if (process->state() != QProcess::NotRunning) {
      process->kill();
    }
  });
  qInfo().noquote() << QStringLiteral("Chat: Codex's conversation closed");
}

} // namespace mitcad::agent
