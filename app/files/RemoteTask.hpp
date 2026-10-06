// SPDX-License-Identifier: MIT
#pragma once

// One remote operation (P12 remote): a command of a project's version
// history (core/model/src/api/commands.md, "Remote repositories": fetch,
// push, sync, connect, ...) or a project opened from a remote (a clone),
// run on a thread of its own. Network work takes seconds to minutes, so it
// is neither on the UI thread nor on the model's worker. A project is used
// by one thread, so the task opens its own; its SyncControl carries git's
// progress, which the task reports here, and the cancel request, which
// ends git.

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
  // A task still running is cancelled and waited for.
  ~RemoteTask() override;

  void start();
  // Ends git; the answer's error is then of the class "cancelled".
  void cancel();
  bool isRunning() const;
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
  using Run = std::function<QJsonObject(const SyncControl& control)>;
  RemoteTask(QString name, Run run, QObject* parent);
  void poll();

  QString m_name;
  Run m_run;
  rust::Box<SyncControl> m_control;
  QThread* m_thread = nullptr;
  QTimer* m_poll = nullptr;
  mutable std::mutex m_mutex; // the answer, written by the thread
  QJsonObject m_answer;
  QString m_text;
  int m_percent = -1;
};

} // namespace mitcad
