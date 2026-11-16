// SPDX-License-Identifier: MIT
#pragma once

// One remote operation (P12 remote): a command of a project's version
// history (core/model-api/src/commands.md, "Remote repositories": fetch,
// push, sync, connect, ...) or a project opened from a remote (a clone),
// run on a thread of its own. Network work takes seconds to minutes, so it
// is neither on the UI thread nor on the model's worker. A project is used
// by one thread, so the task opens its own; its SyncControl carries git's
// progress, which the task reports here, and the cancel request, which
// ends git.
//
// A task of a project (command(), or one given its folder with
// setProject()) waits in that project's version queue (files/
// VersionQueue.hpp, mitcad#1895) until the project's earlier tasks have
// ended: two of them never run on one repository at once. Cancelled while
// it waits, it ends at once, its answer of the class "cancelled".

#include <functional>
#include <mutex>

#include <QJsonObject>
#include <QObject>
#include <QString>

#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

class QThread;
class QTimer;

namespace mitcad {

class RemoteTask : public QObject {
  Q_OBJECT

public:
  // The command `command` (JSON) of the project that `path` (a file or the
  // project's folder) is in; name() is the command's name.
  static RemoteTask* command(const QString& path, const QJsonObject& command, QObject* parent);
  // The project at `url` opened into `folder` (git clone); name() "clone".
  static RemoteTask* clone(const QString& url, const QString& folder, QObject* parent);
  // A library command (mitcad#64, commands.md "Component libraries":
  // library_fetch, ...); name() is the command's name.
  static RemoteTask* library(const QJsonObject& command, QObject* parent);
  // A projects command without a project (mitcad#89, commands.md
  // "Projects": check_remote, create_project, clone_project, init_bare,
  // host_keys, ...); name() is the command's name.
  static RemoteTask* projects(const QJsonObject& command, QObject* parent);
  // Work of several commands in a row (`run` on the task's thread, its
  // answer in the form of a command's), named `name`.
  using Run = std::function<QJsonObject(const SyncControl& control)>;
  static RemoteTask* custom(const QString& name, Run run, QObject* parent);
  // A task still running is cancelled and waited for.
  ~RemoteTask() override;

  // The project folder whose version queue the task waits in (before
  // start(); command() sets its path). "" none: it starts at once.
  void setProject(const QString& folder);
  const QString& project() const { return m_project; }

  void start();
  // Ends git; the answer's error is then of the class "cancelled".
  void cancel();
  // Started and not finished: waiting in its queue, or running.
  bool isRunning() const;
  // Started and waiting for the project's earlier tasks.
  bool isWaiting() const;
  bool isCancelled() const;
  const QString& name() const { return m_name; }
  // git's phase ("Receiving objects", empty before git reports one) and its
  // percentage (-1: none).
  const QString& progressText() const { return m_text; }
  int progressPercent() const { return m_percent; }
  // The answer once finished: the command's JSON (a failure of the remote
  // or of git is its "error"); a command that failed as a command gives
  // "error" {"class": "other", "message"}.
  QJsonObject answer() const;

signals:
  void progressed(const QString& text, int percent);
  void finished();

private:
  RemoteTask(QString name, Run run, QObject* parent);
  // Its turn in the queue: the thread starts.
  void begin();
  void poll();
  // The queue learns that the task ended (once).
  void leaveQueue();

  QString m_name;
  Run m_run;
  rust::Box<SyncControl> m_control;
  QThread* m_thread = nullptr;
  QTimer* m_poll = nullptr;
  mutable std::mutex m_mutex; // the answer, written by the thread
  QJsonObject m_answer;
  QString m_text;
  int m_percent = -1;
  QString m_project;
  quint64 m_queueId = 0; // its place in the version queue, 0 none
  bool m_started = false;
  bool m_finished = false;
};

// A command of the project in `root` that needs no network (commit,
// create_folder, new_design, move, delete, ...) run aside (VersionQueue's
// runAside: on a thread of its own, the window repainting meanwhile);
// throws std::runtime_error with the core's message when it fails.
QJsonObject projectCommandAside(const QString& root, const QJsonObject& command);

} // namespace mitcad
