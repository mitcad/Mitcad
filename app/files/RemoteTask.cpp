// SPDX-License-Identifier: MIT
#include "RemoteTask.hpp"

#include <exception>
#include <utility>

#include <QThread>
#include <QTimer>
#include <QtLogging>

#include "../framework/Json.hpp"

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

} // namespace

RemoteTask* RemoteTask::command(const QString& path, const QJsonObject& command, QObject* parent) {
  const QByteArray where = path.toUtf8();
  const QByteArray json = compactJson(command);
  return new RemoteTask(
      command.value(QStringLiteral("cmd")).toString(),
      [where, json](const SyncControl& control) {
        // A project of the thread's own.
        const rust::Box<Project> project = open_project(rustStr(where));
        return parseObject(project->command_with(rustStr(json), control));
      },
      parent);
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
}

void RemoteTask::start() {
  if (m_thread != nullptr) {
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
    emit finished();
  });
  m_poll->start();
  m_thread->start();
}

void RemoteTask::cancel() {
  if (!m_control->is_cancelled()) {
    qInfo().noquote() << QStringLiteral("Remote task %1: cancel").arg(m_name);
  }
  m_control->cancel();
}

bool RemoteTask::isRunning() const { return m_thread != nullptr && !m_thread->isFinished(); }

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

} // namespace mitcad
