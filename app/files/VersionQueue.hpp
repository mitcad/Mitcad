// SPDX-License-Identifier: MIT
#pragma once

// The version work of projects off the UI thread (mitcad#1895).
//
// - **One queue per project folder.** Every background task that reaches a
//   project's server or moves its branch (a fetch, a push, a sync, the edit
//   locks' commands, a connection: files/RemoteTask.hpp) waits here until
//   the work of that project before it has ended, so two of them never run
//   at once on one repository. Different projects' work runs side by side.
//   The queue only orders: the tasks run on threads of their own, and a task
//   withdrawn before it began (cancelled, deleted) leaves its place to the
//   next one. Logged: "Version queue: <name> waits for <running> in
//   <folder>", "Version queue: <name> started in <folder>".
// - **Local version commands aside** (runAside): a commit, a new folder or
//   design, a move or a delete runs on a thread of its own while the UI
//   thread waits for it with its timers and painting going on, but without
//   taking the user's input (no second command starts in the middle). They
//   do not wait for the queue: they touch no server.
// - **A stall watch for tests** (watchStalls): with MITCAD_TEST_UI_STALL_MS
//   set, a gap of the UI thread's timers longer than that is logged ("UI
//   stall: <ms> ms"), so a UI test can tell that the window kept
//   responding while the server was slow.
//
// Plain Qt Core: unit-tested in app.unit (tests/version_queue.cpp).

#include <functional>
#include <map>
#include <vector>

#include <QObject>
#include <QString>

namespace mitcad {

class VersionQueue : public QObject {
  Q_OBJECT

public:
  // The application's one queue (the UI thread's).
  static VersionQueue& instance();

  // The queue a folder's work goes to: the folder cleaned (case ignored on
  // Windows); "" for an empty folder.
  static QString keyOf(const QString& folder);

  // Asks to run `name` in `folder`'s queue: `begin` is called on this
  // thread when nothing of that folder runs: at once when nothing does,
  // else through the event loop after the work before it ended.
  // Returns the work's id (never 0); finished(id) when it has ended.
  quint64 submit(const QString& folder, const QString& name, std::function<void()> begin);
  // The work `id` ended, or is withdrawn before it began: the next one of
  // its folder begins. An id not known (or reported already) is ignored.
  void finished(quint64 id);
  // Whether `id` waits for its turn.
  bool isWaiting(quint64 id) const;
  // The running work's name in `folder`'s queue ("" none), and how many
  // wait behind it.
  QString running(const QString& folder) const;
  int waitingCount(const QString& folder) const;

signals:
  // A folder's queue changed (`key`: keyOf of its folder).
  void changed(const QString& key);

private:
  struct Work {
    quint64 id = 0;
    QString name;
    std::function<void()> begin;
  };
  struct Queue {
    quint64 running = 0;
    QString runningName;
    std::vector<Work> waiting;
  };
  void beginNext(const QString& key);

  std::map<QString, Queue> m_queues;
  quint64 m_next = 0;
};

// Runs `work` on a thread of its own while this thread waits for it, its
// timers and painting going on but no user input taken (mitcad#1895). Takes
// about as long as `work`; `name` goes into the log when it took longer
// than 100 ms.
void runAside(const QString& name, const std::function<void()>& work);

// How long closing MitCad waits for a running push or sync before it
// stops it, leaving the versions to the next start: 10 s
// (MITCAD_TEST_REMOTE_CLOSE_WAIT_SECONDS for tests).
int closeWaitSeconds();

// With MITCAD_TEST_UI_STALL_MS set (milliseconds, > 0): logs every gap of
// the UI thread's timers longer than that ("UI stall: <ms> ms"). Once per
// process; nothing without the variable.
void watchStalls();

} // namespace mitcad
