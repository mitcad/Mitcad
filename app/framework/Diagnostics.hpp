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
//
// And for background computation (P7, MainWindow::runJob):
//
// - MITCAD_TEST_RECOMPUTE_DELAY_MS=n[,<label>=m...]: for tests, every
//   feature a job evaluates takes n ms longer, so that opening a small
//   design shows the progress dialog and can be cancelled
//   (tools/ui-compute-test.sh); in a job named <label> m ms instead: a
//   model command's name ("undo", "set_parameter"), "preview", or the file
//   an open reads ("400,preview=1500,set_parameter=0"). The .f3d import's
//   worker takes "import_f3d" for the definitions it tries
//   (files/F3dImport.cpp; tools/ui-import-test.sh stops it so).
// - MITCAD_COMPUTE_INLINE=1: jobs run on the UI thread, with the same
//   progress and cancel in the model but without waiting for them (no
//   dialog, no drawing meanwhile): for a debugger, or to compare.
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
