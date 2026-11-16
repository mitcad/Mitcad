// SPDX-License-Identifier: MIT
#include "RemoteTask.hpp"

#include <exception>
#include <stdexcept>
#include <utility>

#include <QPointer>
#include <QThread>
#include <QTimer>
#include <QtLogging>

#include "../framework/Json.hpp"
#include "VersionQueue.hpp"

namespace mitcad {
namespace {

// How often git's progress is read while a task runs.
constexpr int kPollMs = 100;

QString qstr(const rust::String& text) { return QString::fromUtf8(text.data(), static_cast<qsizetype>(text.size())); }

// A command that failed as a command, in the form of a remote failure.
QJsonObject failed(const QString& message) {
  return {{QStringLiteral("error"),
           QJsonObject{{QStringLiteral("class"), QStringLiteral("other")},
                       {QStringLiteral("message"), message},
                       {QStringLiteral("detail"), QString()}}}};
}

// A task cancelled before its turn came.
QJsonObject cancelledBeforeStart() {
  return {{QStringLiteral("error"),
           QJsonObject{{QStringLiteral("class"), QStringLiteral("cancelled")},
                       {QStringLiteral("message"), QStringLiteral("cancelled before it started")},
                       {QStringLiteral("detail"), QString()}}}};
}

} // namespace

RemoteTask* RemoteTask::command(const QString& path, const QJsonObject& command, QObject* parent) {
  const QByteArray where = path.toUtf8();
  const QByteArray json = compactJson(command);
  auto* task = new RemoteTask(
      command.value(QStringLiteral("cmd")).toString(),
      [where, json](const SyncControl& control) {
        // A project of the thread's own.
        const rust::Box<Project> project = open_project(rustStr(where));
        return parseObject(project->command_with(rustStr(json), control));
      },
      parent);
  task->setProject(path);
  return task;
}

RemoteTask* RemoteTask::clone(const QString& url, const QString& folder, QObject* parent) {
  const QByteArray from = url.toUtf8();
  const QByteArray into = folder.toUtf8();
  return new RemoteTask(
      QStringLiteral("clone"),
      [from, into](const SyncControl& control) {
        return parseObject(clone_project(rustStr(from), rustStr(into), control, false));
      },
      parent);
}

RemoteTask* RemoteTask::library(const QJsonObject& command, QObject* parent) {
  const QByteArray json = compactJson(command);
  return new RemoteTask(
      command.value(QStringLiteral("cmd")).toString(),
      [json](const SyncControl& control) { return parseObject(library_command(rustStr(json), control)); }, parent);
}

RemoteTask* RemoteTask::projects(const QJsonObject& command, QObject* parent) {
  const QByteArray json = compactJson(command);
  return new RemoteTask(
      command.value(QStringLiteral("cmd")).toString(),
      [json](const SyncControl& control) { return parseObject(projects_command(rustStr(json), control)); }, parent);
}

RemoteTask* RemoteTask::custom(const QString& name, Run run, QObject* parent) {
  return new RemoteTask(name, std::move(run), parent);
}

RemoteTask::RemoteTask(QString name, Run run, QObject* parent)
    : QObject(parent), m_name(std::move(name)), m_run(std::move(run)), m_control(new_sync_control()) {
  m_poll = new QTimer(this);
  m_poll->setInterval(kPollMs);
  connect(m_poll, &QTimer::timeout, this, &RemoteTask::poll);
}

RemoteTask::~RemoteTask() {
  if (m_thread != nullptr) {
    m_control->cancel();
    m_thread->wait();
    delete m_thread;
  }
  // Waiting, or ended without its finished signal handled: its place goes.
  leaveQueue();
}

void RemoteTask::setProject(const QString& folder) {
  if (!m_started) {
    m_project = folder;
  }
}

void RemoteTask::leaveQueue() {
  const quint64 id = std::exchange(m_queueId, 0);
  if (id != 0) {
    VersionQueue::instance().finished(id);
  }
}

void RemoteTask::start() {
  if (m_started) {
    return;
  }
  m_started = true;
  if (m_project.isEmpty()) {
    begin();
    return;
  }
  const QPointer<RemoteTask> self(this);
  const quint64 id = VersionQueue::instance().submit(m_project, m_name, [self] {
    if (self != nullptr) {
      self->begin();
    }
  });
  // begin() may have run already (nothing else of the project ran); the
  // thread's finished signal comes later, through this thread's events.
  m_queueId = id;
}

void RemoteTask::begin() {
  if (m_thread != nullptr || m_finished) {
    return;
  }
  if (m_control->is_cancelled()) {
    // Cancelled before its turn: ends without running.
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_answer = cancelledBeforeStart();
    }
    m_finished = true;
    QTimer::singleShot(0, this, [this] {
      leaveQueue();
      emit finished();
    });
    return;
  }
  const SyncControl& control = *m_control;
  m_thread = QThread::create([this, &control] {
    QJsonObject answer;
    try {
      answer = m_run(control);
    } catch (const std::exception& e) {
      answer = failed(errorText(e));
    }
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_answer = answer;
  });
  connect(m_thread, &QThread::finished, this, [this] {
    m_poll->stop();
    poll();
    m_finished = true;
    // The project's next task may start before this one's answer is used.
    leaveQueue();
    emit finished();
  });
  m_poll->start();
  m_thread->start();
  // Waiting is over: what shows its state learns it.
  emit progressed(m_text, m_percent);
}

void RemoteTask::cancel() {
  if (!m_control->is_cancelled()) {
    qInfo().noquote() << QStringLiteral("Remote task %1: cancel").arg(m_name);
  }
  m_control->cancel();
  if (isWaiting()) {
    // Not begun: it ends now, and its place in the queue goes.
    leaveQueue();
    begin();
  }
}

bool RemoteTask::isRunning() const { return m_started && !m_finished; }

bool RemoteTask::isWaiting() const { return m_started && m_thread == nullptr && !m_finished; }

bool RemoteTask::isCancelled() const { return m_control->is_cancelled(); }

QJsonObject RemoteTask::answer() const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  return m_answer;
}

void RemoteTask::poll() {
  const SyncProgress progress = m_control->progress();
  const QString text = qstr(progress.text);
  const int percent = static_cast<int>(progress.percent);
  if (text != m_text || percent != m_percent) {
    m_text = text;
    m_percent = percent;
    emit progressed(m_text, m_percent);
  }
}

QJsonObject projectCommandAside(const QString& root, const QJsonObject& command) {
  const QByteArray where = root.toUtf8();
  const QByteArray json = compactJson(command);
  QJsonObject answer;
  QString error;
  bool failed = false;
  runAside(command.value(QStringLiteral("cmd")).toString(), [&] {
    try {
      // A project of the thread's own.
      const rust::Box<Project> project = open_project(rustStr(where));
      answer = parseObject(project->command(rustStr(json)));
    } catch (const std::exception& e) {
      failed = true;
      error = QString::fromUtf8(e.what());
    }
  });
  if (failed) {
    throw std::runtime_error(error.toStdString());
  }
  return answer;
}

} // namespace mitcad
