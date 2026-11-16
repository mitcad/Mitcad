// SPDX-License-Identifier: MIT
#pragma once

// The session log (mitcad#1896): every run's log kept on disk, so that a
// hang or a crash can be traced after the user ended MitCad. Unlike the
// debug log (DebugLog.hpp), which the user turns on, it is always written.
//
// - Every line the log ring sees (mcp/LogRing.hpp: what Live MCP's ui_log
//   shows, its warnings and errors) goes into the file, in the same
//   format as the debug log's (mcp::formatLogLine), flushed to the system
//   at once, so that a hang or a kill keeps everything up to it.
// - The files: in the log folder (DebugLog::defaultFolder():
//   AppLocalDataLocation/logs, which MITCAD_DEBUG_LOG_DIR moves), one per
//   session, named by its start: session-YYYYMMDD-HHMMSS-<pid>.log. A
//   session's file over 20 MB continues in <name>-2.log, -3, ...
// - The folder keeps the last 10 sessions and at most 50 MB of session
//   logs, the oldest going first; never a session that still runs.
// - Each file begins with a short header (the version, the build, the
//   system, the start), then the ring's lines from before the file was
//   opened; a clean end writes "--- The session ended" as its last line.
// - A session holds a lock file (session-...-<pid>.lock, QLockFile) while
//   it runs; a clean end removes it. The next start finds a lock whose
//   process is gone: that session ended unexpectedly (a crash, a hang the
//   user ended, a kill), and the window says so (ReportCenter).
// - Crash dumps (report/CrashHandler.hpp, Windows minidumps
//   crash-<time>-<pid>.dmp) go into the same folder; the newest 5 are kept.
// - Logs stay on the computer: Send Feedback offers this session's and the
//   previous one's files, attached only by the user's choice. A log may
//   contain file paths, design names and entity names, never passwords or
//   tokens (no log line carries them).
//
// MITCAD_SESSION_LOG=0 turns it off for a run; --screenshot runs write
// none.

#include <memory>
#include <mutex>

#include <QDateTime>
#include <QFile>
#include <QList>
#include <QLockFile>
#include <QString>
#include <QStringList>
#include <QtGlobal>

#include "framework/DebugLog.hpp"

namespace mitcad {

namespace mcp {
struct LogLine;
}

struct SessionLogLimits {
  qint64 fileBytes = 20LL * 1024 * 1024;   // a session's file continues in -2 at this size
  int sessions = 10;                        // the folder keeps the last this many sessions
  qint64 folderBytes = 50LL * 1024 * 1024; // and at most this much of session logs
  int dumps = 5;                            // and the newest this many crash dumps
};

// A session log's file name, session-YYYYMMDD-HHMMSS-<pid>[-n].log.
struct SessionLogName {
  bool valid = false;
  QString session; // session-YYYYMMDD-HHMMSS-<pid>
  QString time;    // YYYYMMDD-HHMMSS
  qint64 pid = 0;
  int part = 1;
};
SessionLogName parseSessionLogName(const QString& fileName);

// The session logs of a folder.
QList<DebugLogFile> sessionLogFiles(const QString& folder);

// The files to remove so that the folder keeps the limits: whole sessions,
// the oldest first, until at most `limits.sessions` are left; then files,
// the oldest first, until the session logs take at most
// `limits.folderBytes`. Never a file of the sessions in `running` (their
// names, session-...-<pid>) nor of `current`'s, except, over the bytes, the
// parts before their last; never `current` (the file being written).
QStringList sessionLogFilesToRemove(QList<DebugLogFile> files, const QStringList& running, const QString& current,
                                    const SessionLogLimits& limits);

// A session found ended unexpectedly at the start: its log (the last
// part), the crash dump of its process when there is one ("" none).
struct EndedSession {
  QString session; // session-YYYYMMDD-HHMMSS-<pid>
  QString log;
  QString dump;
  qint64 pid = 0;
};

class SessionLog {
public:
  // Made by the first call; it lives until the process ends.
  static SessionLog& instance();

  // session-YYYYMMDD-HHMMSS-<pid>.log
  static QString fileName(const QDateTime& when, qint64 pid);
  // Whether this run writes one: not with MITCAD_SESSION_LOG=0.
  static bool wanted();

  // Starts this session's log in `folder`: notes the sessions that ended
  // unexpectedly (their lock left by a process that is gone), locks this
  // one, writes the file's header and the ring's lines so far, removes
  // what is over the limits. On the GUI thread, once.
  bool start(const QString& folder, QString* error = nullptr);
  // A clean end: the last line, the file closed, the lock removed.
  void finish();
  bool running() const;
  QString currentFile() const;
  QString folder() const;
  // This session's files and those of up to `earlier` sessions before it,
  // the newest session first, each session's parts in order.
  QStringList recentFiles(int earlier = 1) const;
  // The newest session found ended unexpectedly at the start (its session
  // empty for none).
  EndedSession endedUnexpectedly() const;

  // For tests: smaller limits.
  void setLimits(const SessionLogLimits& limits);

private:
  SessionLog() = default;
  static void sink(QtMsgType type, const mcp::LogLine& line);
  void write(const mcp::LogLine& line);
  // Under m_mutex:
  void writeText(const QString& text);
  void writeHeader(const QString& continuedFrom);
  void rollOver();
  void prune();
  QStringList runningSessions() const;

  mutable std::mutex m_mutex;
  QFile m_file;
  QString m_folder;
  QString m_session; // session-YYYYMMDD-HHMMSS-<pid>
  int m_part = 0;
  qint64 m_written = 0;
  quint64 m_snapshotUpTo = 0; // the ring's lines up to this one went in with the header
  SessionLogLimits m_limits;
  std::unique_ptr<QLockFile> m_lock;
  EndedSession m_ended;
  bool m_started = false;
};

} // namespace mitcad
