// SPDX-License-Identifier: MIT
// The session log (mitcad#1896): its files' names, the folder's limits (the
// last sessions, the bytes, never a running session nor the file being
// written), the rotation of a long session, the header, the lines from
// before the start, the lock while it runs and the clean end, the crash
// dumps kept.
#include "UnitTests.hpp"
#include "framework/SessionLog.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDate>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLockFile>
#include <QRegularExpression>
#include <QStringList>
#include <QTemporaryDir>
#include <QTime>
#include <QtLogging>

#include "mcp/LogRing.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "session_log.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QStringList linesOf(const QString& path) {
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    return {};
  }
  return QString::fromUtf8(file.readAll()).split(QLatin1Char('\n'), Qt::SkipEmptyParts);
}

bool hasLine(const QStringList& lines, const QString& pattern) {
  const QRegularExpression re(pattern);
  for (const QString& line : lines) {
    if (re.match(line).hasMatch()) {
      return true;
    }
  }
  return false;
}

void writeFile(const QString& path, qint64 size) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QFile file(path);
  if (file.open(QIODevice::WriteOnly)) {
    file.write(QByteArray(static_cast<int>(size), 'x'));
  }
}

void nameTests() {
  using mitcad::parseSessionLogName;
  const mitcad::SessionLogName first = parseSessionLogName(QStringLiteral("session-20260101-093012-4711.log"));
  CHECK(first.valid);
  CHECK(first.session == QStringLiteral("session-20260101-093012-4711"));
  CHECK(first.time == QStringLiteral("20260101-093012"));
  CHECK(first.pid == 4711);
  CHECK(first.part == 1);
  const mitcad::SessionLogName third = parseSessionLogName(QStringLiteral("session-20260101-093012-4711-3.log"));
  CHECK(third.valid);
  CHECK(third.session == QStringLiteral("session-20260101-093012-4711"));
  CHECK(third.pid == 4711);
  CHECK(third.part == 3);
  // The debug log's files and others are not session logs.
  CHECK(!parseSessionLogName(QStringLiteral("mitcad-20260101-093012-4711.log")).valid);
  CHECK(!parseSessionLogName(QStringLiteral("session-20260101-093012-4711.lock")).valid);
  CHECK(!parseSessionLogName(QStringLiteral("session-2026-4711.log")).valid);
  CHECK(mitcad::SessionLog::fileName(QDateTime(QDate(2026, 11, 15), QTime(13, 47, 21)), 99) ==
        QStringLiteral("session-20261115-134721-99.log"));
}

// The oldest sessions go first, whole; then files for the bytes; never a
// running session's nor the file being written.
void limitTests() {
  using mitcad::DebugLogFile;
  using mitcad::SessionLogLimits;
  using mitcad::sessionLogFilesToRemove;
  const QDateTime t0(QDate(2026, 1, 1), QTime(0, 0));
  const QString current = QStringLiteral("/logs/session-20260101-000005-9.log");
  const QList<DebugLogFile> files = {
      {QStringLiteral("/logs/session-20260101-000003-7.log"), 100, t0},
      {QStringLiteral("/logs/session-20260101-000001-7.log"), 100, t0},
      {QStringLiteral("/logs/session-20260101-000002-7-2.log"), 100, t0},
      {QStringLiteral("/logs/session-20260101-000002-7.log"), 100, t0},
      {QStringLiteral("/logs/session-20260101-000004-8.log"), 100, t0},
      {current, 100, t0},
  };
  SessionLogLimits limits;
  CHECK(limits.fileBytes == 20LL * 1024 * 1024);
  CHECK(limits.sessions == 10);
  CHECK(limits.folderBytes == 50LL * 1024 * 1024);
  CHECK(limits.dumps == 5);
  CHECK(sessionLogFilesToRemove(files, {}, current, limits).isEmpty());
  // Three sessions: the two oldest go whole (the second's both parts).
  limits.sessions = 3;
  CHECK(sessionLogFilesToRemove(files, {}, current, limits) ==
        QStringList({QStringLiteral("/logs/session-20260101-000001-7.log"),
                     QStringLiteral("/logs/session-20260101-000002-7.log"),
                     QStringLiteral("/logs/session-20260101-000002-7-2.log")}));
  // A session that still runs stays; the next oldest goes in its place.
  CHECK(sessionLogFilesToRemove(files, {QStringLiteral("session-20260101-000001-7")}, current, limits) ==
        QStringList({QStringLiteral("/logs/session-20260101-000002-7.log"),
                     QStringLiteral("/logs/session-20260101-000002-7-2.log"),
                     QStringLiteral("/logs/session-20260101-000003-7.log")}));
  // The bytes: the oldest files first, part by part.
  limits.sessions = 10;
  limits.folderBytes = 350;
  CHECK(sessionLogFilesToRemove(files, {}, current, limits) ==
        QStringList({QStringLiteral("/logs/session-20260101-000001-7.log"),
                     QStringLiteral("/logs/session-20260101-000002-7.log"),
                     QStringLiteral("/logs/session-20260101-000002-7-2.log")}));
  // Alone over the limits, the file being written stays; a running
  // session's older part may go, its last one not.
  limits.sessions = 1;
  limits.folderBytes = 0;
  const QStringList removed = sessionLogFilesToRemove(files, {QStringLiteral("session-20260101-000002-7")}, current,
                                                      limits);
  CHECK(!removed.contains(current));
  CHECK(removed.contains(QStringLiteral("/logs/session-20260101-000002-7.log")));
  CHECK(!removed.contains(QStringLiteral("/logs/session-20260101-000002-7-2.log")));
  CHECK(removed.size() == 4);
  // Other files of the folder are not counted.
  CHECK(sessionLogFilesToRemove({{QStringLiteral("/logs/mitcad-20200101-000000-1.log"), 999, t0}, {current, 10, t0}},
                                {}, current, limits)
            .isEmpty());
}

void fileTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString folder = directory.filePath(QStringLiteral("logs"));
  // Twelve sessions before this one, the oldest of them still running (its
  // lock held here); a debug log and seven crash dumps beside them.
  for (int i = 0; i < 12; ++i) {
    writeFile(QDir(folder).filePath(QStringLiteral("session-202001%1-000000-%2.log")
                                        .arg(i + 10)
                                        .arg(100 + i)),
              100);
  }
  QLockFile running(QDir(folder).filePath(QStringLiteral("session-20200110-000000-100.lock")));
  running.setStaleLockTime(0);
  CHECK(running.tryLock(0));
  writeFile(QDir(folder).filePath(QStringLiteral("mitcad-20200101-000000-1.log")), 100);
  for (int i = 0; i < 7; ++i) {
    writeFile(QDir(folder).filePath(QStringLiteral("crash-%1-%2.dmp").arg(1600000000 + i).arg(i)), 10);
  }

  mitcad::mcp::installLogRing();
  qInfo().noquote() << QStringLiteral("before the session log started");
  mitcad::SessionLog& log = mitcad::SessionLog::instance();
  QString error;
  CHECK(log.start(folder, &error));
  CHECK(error.isEmpty());
  CHECK(log.running());
  const QString first = log.currentFile();
  CHECK(mitcad::parseSessionLogName(QFileInfo(first).fileName()).pid == QCoreApplication::applicationPid());
  CHECK(QFileInfo(log.folder()) == QFileInfo(folder));
  // Its lock while it runs; no session ended unexpectedly (the one whose
  // lock is held runs).
  CHECK(QFileInfo::exists(QDir(folder).filePath(QFileInfo(first).completeBaseName() + QStringLiteral(".lock"))));
  CHECK(log.endedUnexpectedly().session.isEmpty());
  QStringList lines = linesOf(first);
  CHECK(lines.value(0) == QStringLiteral("MitCad session log"));
  CHECK(hasLine(lines, QStringLiteral("^Version: MitCad ")));
  CHECK(hasLine(lines, QStringLiteral("^Build: (release|debug), ")));
  CHECK(hasLine(lines, QStringLiteral("^System: ")));
  CHECK(hasLine(lines, QStringLiteral("^Started: .*, file 1$")));
  CHECK(hasLine(lines, QStringLiteral("info +before the session log started$")));
  CHECK(hasLine(lines, QStringLiteral("info +Session log: .*session-")));
  // Flushed at once: the line is in the file without a flush or a wait.
  qWarning().noquote() << QStringLiteral("a warning for the session log");
  const QString warningLine =
      QStringLiteral("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}\\.\\d{3} warning +a warning for the session log$");
  CHECK(hasLine(linesOf(first), warningLine));

  // The folder's limits at the start: ten sessions with this one (the
  // running one kept, the three oldest others gone), the newest five dumps,
  // the debug log untouched.
  const QList<mitcad::DebugLogFile> sessions = mitcad::sessionLogFiles(folder);
  CHECK(sessions.size() == 10);
  CHECK(QFileInfo::exists(QDir(folder).filePath(QStringLiteral("session-20200110-000000-100.log"))));
  CHECK(!QFileInfo::exists(QDir(folder).filePath(QStringLiteral("session-20200111-000000-101.log"))));
  CHECK(!QFileInfo::exists(QDir(folder).filePath(QStringLiteral("session-20200113-000000-103.log"))));
  CHECK(QFileInfo::exists(QDir(folder).filePath(QStringLiteral("session-20200114-000000-104.log"))));
  CHECK(QFileInfo::exists(QDir(folder).filePath(QStringLiteral("mitcad-20200101-000000-1.log"))));
  CHECK(QDir(folder).entryList({QStringLiteral("crash-*.dmp")}, QDir::Files).size() == 5);

  // A long session continues in -2, -3, ... (the file already holds the
  // earlier tests' lines the ring kept).
  mitcad::SessionLogLimits limits;
  limits.fileBytes = 4000;
  log.setLimits(limits);
  const QString filler(200, QLatin1Char('y'));
  for (int i = 0; i < 40; ++i) {
    qInfo().noquote() << QStringLiteral("line %1 %2").arg(i).arg(filler);
  }
  const QString later = log.currentFile();
  CHECK(later != first);
  CHECK(mitcad::parseSessionLogName(QFileInfo(later).fileName()).session ==
        mitcad::parseSessionLogName(QFileInfo(first).fileName()).session);
  CHECK(mitcad::parseSessionLogName(QFileInfo(later).fileName()).part >= 2);
  CHECK(hasLine(linesOf(first), QStringLiteral("^--- Continued in session-.*-2\\.log$")));
  CHECK(hasLine(linesOf(later), QStringLiteral("^Started: .*, continued from session-")));
  // Send Feedback's files: this session's parts first, then the previous
  // session's (the newest of the others).
  const QStringList recent = log.recentFiles(1);
  CHECK(recent.size() >= 3);
  CHECK(QFileInfo(recent.value(0)) == QFileInfo(first));
  CHECK(QFileInfo(recent.last()).fileName() == QStringLiteral("session-20200121-000000-111.log"));

  // The clean end: its last line, the lock gone.
  log.finish();
  CHECK(!log.running());
  lines = linesOf(later);
  CHECK(lines.value(lines.size() - 1).startsWith(QStringLiteral("--- The session ended: ")));
  CHECK(!QFileInfo::exists(QDir(folder).filePath(QFileInfo(first).completeBaseName() + QStringLiteral(".lock"))));
  qInfo().noquote() << QStringLiteral("after the end");
  CHECK(!hasLine(linesOf(later), QStringLiteral("after the end")));
  running.unlock();
  mitcad::mcp::removeLogRing();
}

} // namespace

int sessionLogTests() {
  nameTests();
  limitTests();
  fileTests();
  return failures;
}
MITCAD_UNIT_TEST(sessionLogTests);
