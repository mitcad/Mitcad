// SPDX-License-Identifier: MIT
#pragma once

// The hang marker (mitcad#1896): a thread of its own notices when the UI
// thread has not run its event loop for a while and writes
// "UI not responding for N s (last action: ...; last line: ...)" to the
// log (a warning, so the session log has it: SessionLog.hpp), again every
// 30 s while it lasts; when the UI thread runs its loop again it writes
// "UI responding again after N.N s". A modal dialog's or a progress
// dialog's own event loop counts as running.
//
// The threshold: MITCAD_HANG_WATCHDOG_MS (milliseconds; 0 turns it off),
// else 5000 ms; off by default under the UI tests (MITCAD_TEST_INPUT,
// MITCAD_TEST_SYNC=1 or MITCAD_TEST_LOG_URLS=1), whose slow steps on a
// software renderer would otherwise log it, unless a test sets the
// variable.

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

  // Stops watching (the end of the run: what follows the event loop is
  // no hang).
  void stop();

  // What the UI thread is doing, for the hang's line: a command's or a
  // model command's id (ReportCenter::noteAction), a test step. Any
  // thread; never design content.
  static void noteActivity(const QString& activity);

private:
  void beat();
  void watch();

  int m_threshold = 0;
  QTimer* m_timer = nullptr;
  QThread* m_thread = nullptr;
  std::mutex m_mutex;
  std::condition_variable m_wake;
  qint64 m_lastBeat = 0;     // steady ms of the event loop's last turn; 0 before the first
  bool m_reported = false;   // the hang since m_lastBeat was logged
  qint64 m_nextReport = 0;   // its next "still" line, in ms of the hang
  bool m_stopping = false;
};

} // namespace mitcad
