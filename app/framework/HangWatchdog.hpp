// SPDX-License-Identifier: MIT
#pragma once

// The hang marker (mitcad#1896): a thread of its own notices when the UI
// thread has not run its event loop for a while and writes
// "UI not responding for N s (last action: ...; last line: ...)" to the
// log (a warning, so the session log has it: SessionLog.hpp), again every
// 30 s while it lasts; when the UI thread runs its loop again it writes
// "UI responding again after N.N s". A modal dialog's or a progress
// dialog's own event loop counts as running.
// While a UI Automation client reads the window (AccessibilityClients.hpp,
// mitcad#2227), the first line also names it ("accessibility client:
// <exe name> (pid N)"), and for a client COM does not tell ("unknown (pid
// unknown)") the processes that have UI Automation loaded ("UI Automation
// loaded in <exe> (pid N), ...", found by this thread).
//
// The threshold: MITCAD_HANG_WATCHDOG_MS (milliseconds; 0 turns it off),
// else 5000 ms; off by default under the UI tests (MITCAD_TEST_INPUT,
// MITCAD_TEST_SYNC=1 or MITCAD_TEST_LOG_URLS=1), whose slow steps on a
// software renderer would otherwise log it, unless a test sets the
// variable.
//
// The teardown (mitcad#2275, ShutdownSteps.hpp): once the event loop has
// ended it no longer turns, and watchShutdown() switches the thread from
// the loop to the shutdown's steps: timeStep() names each step as it
// begins, and a step that runs longer than its limit (the threshold unless
// the step has its own) is logged as the warning "Shutdown step not
// finished after N.N s: <step>", again every 30 s while it lasts ("Shutdown
// step still not finished after ..."), and when the next step begins
// "Shutdown step finished after N.N s: <step>". A long teardown whose
// every step is under its limit logs nothing. The object outlives the
// rest of the run (main's LogScope) and stops just before the session
// log's clean end.

#include <condition_variable>
#include <mutex>

#include <QObject>
#include <QString>
#include <QtGlobal>

class QThread;
class QTimer;

namespace mitcad {

class HangWatchdog : public QObject {
  Q_OBJECT

public:
  // The threshold from the environment (see above), 0 for off.
  static int thresholdFromEnvironment();

  // Watches from the first turn of the event loop on; `thresholdMs` 0 does
  // nothing. On the GUI thread.
  explicit HangWatchdog(int thresholdMs, QObject* parent = nullptr);
  ~HangWatchdog() override;
  HangWatchdog(const HangWatchdog&) = delete;
  HangWatchdog& operator=(const HangWatchdog&) = delete;

  // Stops watching (the end of the run, just before the session log's
  // clean end).
  void stop();

  // The event loop has ended: from now on the shutdown's steps are timed
  // (timeStep), not the loop. Again: nothing. On the GUI thread.
  void watchShutdown();
  // A shutdown step begins (the first one switches to the teardown,
  // watchShutdown): it is timed against `limitMs` (0: the threshold) until
  // the next. On the GUI thread.
  void timeStep(const QString& step, int limitMs = 0);

  // What the UI thread is doing, for the hang's line: a command's or a
  // model command's id (ReportCenter::noteAction), a test step. Any
  // thread; never design content.
  static void noteActivity(const QString& activity);
  // The graphics call the UI thread is about to make, "" when it returned
  // (mitcad#2406: a view paint layer's texture upload and draw): a hang
  // inside it adds "in graphics step: <step>" to the hang's first line, so
  // that the call that stalled is named. Any thread.
  static void noteGraphicsStep(const QString& step);

private:
  void beat();
  void watch();
  // Under m_mutex, in the teardown: the step's warning when it is over its
  // limit ("" none).
  QString checkStep();

  int m_threshold = 0;
  QTimer* m_timer = nullptr;
  QThread* m_thread = nullptr;
  std::mutex m_mutex;
  std::condition_variable m_wake;
  qint64 m_lastBeat = 0;     // steady ms of the event loop's last turn; 0 before the first
  bool m_reported = false;   // the hang since m_lastBeat was logged
  qint64 m_nextReport = 0;   // its next "still" line, in ms of the hang
  bool m_stopping = false;
  // The teardown (watchShutdown, timeStep):
  bool m_shutdown = false;
  QString m_step;            // the step timed, "" before the first
  qint64 m_stepSince = 0;    // steady ms of its start
  qint64 m_stepLimit = 0;    // its limit in ms
  bool m_stepReported = false;
  qint64 m_stepNextReport = 0; // its next "still" line, in ms of the step
};

} // namespace mitcad
