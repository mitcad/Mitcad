// SPDX-License-Identifier: MIT
#pragma once

#include <QElapsedTimer>
#include <QHash>
#include <QString>
#include <QtGlobal>
#include <QtLogging>

namespace mitcad {

// Logs for tests and measurements, switched on by environment variables
// (as MITCAD_LOG_PICKS is):
//
// - MITCAD_LOG_VOLUMES=1: the bodies' volumes before and after each model
//   change, "Body Body1 (F2.b0): volume a -> b mm3" (app/COMMANDS.md). The
//   UI tests set it; it is off otherwise, because the first change after
//   opening a large design would measure every body (seconds).
// - MITCAD_LOG_TIMING=1: how long the application's steps take,
//   "Timing <step>: <ms> ms[, <detail>]", for tools/perf-measure.sh.
// - MITCAD_LOG_EDGE_PLACES=1: in sketch mode, once the camera rests, where
//   a click hits each body edge in sight and behind faces, "Sketch pick
//   edge F1.b0/<edge> visible at x,y" (mitcad#217,
//   tools/ui-sketch-select-test.sh); every edge is tried in the view.
//
// And for background computation (P7, MainWindow::runJob):
//
// - MITCAD_TEST_RECOMPUTE_DELAY_MS=n[,<label>=m...]: for tests, every
//   feature a job evaluates or restores from the result store takes n ms
//   longer, so that opening a small
//   design shows the progress dialog and can be cancelled
//   (tools/ui-compute-test.sh); in a job named <label> m ms instead: a
//   model command's name ("undo", "set_parameter"), "preview", or the file
//   an open reads ("400,preview=1500,set_parameter=0"). The .f3d import's
//   worker takes "import_f3d" for the definitions it tries
//   (files/F3dImport.cpp; tools/ui-import-test.sh stops it so).
// - MITCAD_TEST_JOB_SLEEP_MS=<label>=m[,<label>=m...]: for tests, the job
//   named <label> (a model command's name: "redo") takes m ms longer on
//   the worker after its work, whether it evaluates features or not, so
//   that a test can watch what the window does while it waits for a job
//   that only changes a drawing (tools/ui-drawings-test.sh: the hover's
//   question while a redo runs, mitcad#753). Also a job in the background
//   ("bed plan": Send to Slicer's, tools/ui-print-bed-test.sh, mitcad#1121).
// - MITCAD_COMPUTE_INLINE=1: jobs run on the UI thread, with the same
//   progress and cancel in the model but without waiting for them (no
//   dialog, no drawing meanwhile): for a debugger, or to compare.
// - MITCAD_TEST_WAIT_CURSOR=1: the wait cursor's test commands, Nested Wait
//   (Test) and Leave Wait Cursor On (Test) (framework/WaitCursor.hpp,
//   mitcad#268; tools/ui-compute-test.sh).
//
// And for autosave (P8, app/files/Autosave.hpp):
//
// - MITCAD_TEST_AUTOSAVE_SECONDS=n: for tests, autosave every n seconds
//   instead of the minutes in Preferences (tools/ui-autosave-test.sh).
//   MITCAD_AUTOSAVE_DIR (AppSettings.hpp) moves the recovery folder.
//
// And for the theme (mitcad#14):
//
// - MITCAD_TEST_COLOR_SCHEME=dark|light: once the window shows, the
//   application asks the platform for that colour scheme, as when the
//   desktop's setting changes while Mitcad runs (QStyleHints::
//   setColorScheme; Qt's GTK platform theme follows it, the generic one
//   does not; tools/ui-theme-test.sh).
// - MITCAD_TEST_WINDOWS10=1: Windows 10's dark app mode path (Qt's own
//   style with a dark palette while the app mode is dark, mitcad#1282) on
//   Windows 11 too, the build VM's (platform/WindowsTheme.hpp;
//   tools/ui-windows-windows10-dark-test.ps1).
inline QString testColorScheme() { return qEnvironmentVariable("MITCAD_TEST_COLOR_SCHEME"); }
inline bool volumesLogged() {
  static const bool logged = qEnvironmentVariableIntValue("MITCAD_LOG_VOLUMES") != 0;
  return logged;
}

inline bool timingLogged() {
  static const bool logged = qEnvironmentVariableIntValue("MITCAD_LOG_TIMING") != 0;
  return logged;
}

inline int testRecomputeDelay(const QString& label) {
  static const QHash<QString, int> delays = [] {
    QHash<QString, int> parsed;
    const QString spec = qEnvironmentVariable("MITCAD_TEST_RECOMPUTE_DELAY_MS");
    for (const QString& part : spec.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
      const qsizetype equals = part.lastIndexOf(QLatin1Char('='));
      parsed.insert(equals < 0 ? QString() : part.left(equals).trimmed(), part.mid(equals + 1).trimmed().toInt());
    }
    return parsed;
  }();
  return delays.value(label, delays.value(QString()));
}

inline int testJobSleep(const QString& label) {
  static const QHash<QString, int> sleeps = [] {
    QHash<QString, int> parsed;
    const QString spec = qEnvironmentVariable("MITCAD_TEST_JOB_SLEEP_MS");
    for (const QString& part : spec.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
      const qsizetype equals = part.lastIndexOf(QLatin1Char('='));
      if (equals > 0) {
        parsed.insert(part.left(equals).trimmed(), part.mid(equals + 1).trimmed().toInt());
      }
    }
    return parsed;
  }();
  return sleeps.value(label, 0);
}

inline bool computeInline() {
  static const bool inlined = qEnvironmentVariableIntValue("MITCAD_COMPUTE_INLINE") != 0;
  return inlined;
}

inline int testAutosaveSeconds() {
  static const int seconds = qEnvironmentVariableIntValue("MITCAD_TEST_AUTOSAVE_SECONDS");
  return seconds;
}

// Logs the time from its construction to its destruction (or finish()).
class ScopedTiming {
public:
  explicit ScopedTiming(const char* step) : m_step(step) {
    if (timingLogged()) {
      m_timer.start();
    }
  }
  ~ScopedTiming() { finish(); }
  ScopedTiming(const ScopedTiming&) = delete;
  ScopedTiming& operator=(const ScopedTiming&) = delete;

  void setDetail(const QString& detail) { m_detail = detail; }

  void finish() {
    if (!m_timer.isValid()) {
      return;
    }
    const double milliseconds = static_cast<double>(m_timer.nsecsElapsed()) / 1.0e6;
    m_timer.invalidate();
    qDebug().noquote() << QStringLiteral("Timing %1: %2 ms%3")
                              .arg(QString::fromLatin1(m_step))
                              .arg(milliseconds, 0, 'f', 2)
                              .arg(m_detail.isEmpty() ? QString() : QStringLiteral(", ") + m_detail);
  }

private:
  const char* m_step;
  QString m_detail;
  QElapsedTimer m_timer;
};

} // namespace mitcad
