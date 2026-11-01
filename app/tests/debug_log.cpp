// SPDX-License-Identifier: MIT
// The debug log's files (mitcad#269): the folder's limits, the rotation,
// the header, the lines from before the start, the flushes, the thread's
// name, and what the command line and MITCAD_DEBUG_LOG ask for.
#include "framework/DebugLog.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDate>
#include <QDateTime>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>
#include <QStringList>
#include <QTemporaryDir>
#include <QThread>
#include <QTime>
#include <QTimer>
#include <QtLogging>

#include "mcp/LogRing.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "debug_log.cpp:%d: check failed: %s\n", line, expression);
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

// A file of `size` bytes last modified at `modified`.
void writeFile(const QString& path, qint64 size, const QDateTime& modified) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QFile file(path);
  if (file.open(QIODevice::WriteOnly)) {
    file.write(QByteArray(static_cast<int>(size), 'x'));
    file.setFileTime(modified, QFileDevice::FileModificationTime);
    file.close();
  }
  QFile touched(path);
  if (touched.open(QIODevice::ReadWrite)) {
    touched.setFileTime(modified, QFileDevice::FileModificationTime);
  }
}

// On a failure: the folder's log files (size, time, last line) and the
// file meant.
void printFiles(const QString& folder, const QString& meant) {
  std::fprintf(stderr, "debug_log.cpp: meant %s\n", qPrintable(meant));
  for (const mitcad::DebugLogFile& file : mitcad::debugLogFiles(folder)) {
    const QStringList lines = linesOf(file.path);
    std::fprintf(stderr, "  %s %lld %s, last line: %s\n", qPrintable(file.path), static_cast<long long>(file.size),
                 qPrintable(file.modified.toString(QStringLiteral("HH:mm:ss.zzz"))),
                 qPrintable(lines.value(lines.size() - 1).left(160)));
  }
}

void waitMs(int ms) {
  QEventLoop loop;
  QTimer::singleShot(ms, &loop, &QEventLoop::quit);
  loop.exec();
}

// The oldest go first; x.log before x-2.log; never the file written.
void limitTests() {
  using mitcad::DebugLogFile;
  using mitcad::DebugLogLimits;
  using mitcad::debugLogFilesToRemove;
  const QDateTime t0(QDate(2026, 1, 1), QTime(0, 0));
  const QString current = QStringLiteral("/logs/mitcad-20260101-000004-7.log");
  const QList<DebugLogFile> files = {
      {QStringLiteral("/logs/mitcad-20260101-000003-7.log"), 100, t0.addSecs(3)},
      {QStringLiteral("/logs/mitcad-20260101-000001-7.log"), 100, t0.addSecs(1)},
      {QStringLiteral("/logs/mitcad-20260101-000002-7-2.log"), 100, t0.addSecs(2)},
      {QStringLiteral("/logs/mitcad-20260101-000002-7.log"), 100, t0.addSecs(2)},
      {current, 100, t0.addSecs(4)},
  };
  DebugLogLimits limits;
  CHECK(limits.fileBytes == 20LL * 1024 * 1024);
  CHECK(limits.files == 10);
  CHECK(limits.folderBytes == 200LL * 1024 * 1024);
  CHECK(debugLogFilesToRemove(files, current, limits).isEmpty());
  limits.files = 3;
  CHECK(debugLogFilesToRemove(files, current, limits) ==
        QStringList({QStringLiteral("/logs/mitcad-20260101-000001-7.log"),
                     QStringLiteral("/logs/mitcad-20260101-000002-7.log")}));
  limits.files = 10;
  limits.folderBytes = 250;
  CHECK(debugLogFilesToRemove(files, current, limits) ==
        QStringList({QStringLiteral("/logs/mitcad-20260101-000001-7.log"),
                     QStringLiteral("/logs/mitcad-20260101-000002-7.log"),
                     QStringLiteral("/logs/mitcad-20260101-000002-7-2.log")}));
  // The file written stays, even alone over the limits.
  limits.files = 1;
  limits.folderBytes = 10;
  const QStringList removed = debugLogFilesToRemove(files, current, limits);
  CHECK(removed.size() == 4);
  CHECK(!removed.contains(current));
  CHECK(mitcad::DebugLog::fileName(QDateTime(QDate(2026, 10, 14), QTime(9, 30, 12)), 4242) ==
        QStringLiteral("mitcad-20261014-093012-4242.log"));
}

void requestTests() {
  using mitcad::debugLogRequest;
  QStringList arguments{QStringLiteral("mitcad"), QStringLiteral("--debug-log"), QStringLiteral("run.log"),
                        QStringLiteral("--demo")};
  mitcad::DebugLogRequest request = debugLogRequest(arguments, QString());
  CHECK(request.on && request.file == QStringLiteral("run.log") && request.source == QStringLiteral("--debug-log"));
  CHECK(arguments ==
        QStringList({QStringLiteral("mitcad"), QStringLiteral("--debug-log"), QStringLiteral("--demo")}));
  // A design to open is no log file.
  arguments = {QStringLiteral("mitcad"), QStringLiteral("--debug-log"), QStringLiteral("bracket.mitcad")};
  request = debugLogRequest(arguments, QString());
  CHECK(request.on && request.file.isEmpty());
  CHECK(arguments.size() == 3);
  arguments = {QStringLiteral("mitcad"), QStringLiteral("--debug-log=/tmp/x/trace.txt")};
  request = debugLogRequest(arguments, QString());
  CHECK(request.on && request.file == QStringLiteral("/tmp/x/trace.txt"));
  CHECK(arguments == QStringList({QStringLiteral("mitcad"), QStringLiteral("--debug-log")}));
  arguments = {QStringLiteral("mitcad")};
  request = debugLogRequest(arguments, QStringLiteral("1"));
  CHECK(request.on && request.file.isEmpty() && request.source == QStringLiteral("MITCAD_DEBUG_LOG"));
  CHECK(!debugLogRequest(arguments, QStringLiteral("0")).on);
  CHECK(!debugLogRequest(arguments, QString()).on);
  request = debugLogRequest(arguments, QStringLiteral("/tmp/x/env.log"));
  CHECK(request.on && request.file == QStringLiteral("/tmp/x/env.log"));
  // The command line before the variable.
  arguments = {QStringLiteral("mitcad"), QStringLiteral("--debug-log")};
  request = debugLogRequest(arguments, QStringLiteral("/tmp/x/env.log"));
  CHECK(request.on && request.file.isEmpty() && request.source == QStringLiteral("--debug-log"));
  arguments = {QStringLiteral("mitcad"), QStringLiteral("--"), QStringLiteral("--debug-log")};
  CHECK(!debugLogRequest(arguments, QString()).on);
}

// "<date> <time>.<ms> <level> [<thread>] <text>"
QString linePattern(const QString& level, const QString& text, const QString& thread = QString()) {
  return QStringLiteral("^\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}\\.\\d{3} %1 +%2%3$")
      .arg(level, thread.isEmpty() ? QString() : QStringLiteral("\\[%1\\] ").arg(QRegularExpression::escape(thread)),
           QRegularExpression::escape(text));
}

void fileTests() {
  using mitcad::DebugLog;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString folder = directory.filePath(QStringLiteral("logs"));
  qputenv("MITCAD_DEBUG_LOG_DIR", QDir::toNativeSeparators(folder).toLocal8Bit());
  CHECK(QDir(DebugLog::defaultFolder()) == QDir(folder));
  // Four files of earlier runs, two days old, and files that are not logs.
  const QDateTime old = QDateTime::currentDateTime().addDays(-2);
  QStringList earlier;
  for (int i = 0; i < 4; ++i) {
    earlier << QDir(folder).filePath(QStringLiteral("mitcad-20200101-00000%1-1.log").arg(i));
    writeFile(earlier.last(), 500, old.addSecs(i));
  }
  const QString notes = QDir(folder).filePath(QStringLiteral("notes.txt"));
  const QString other = QDir(folder).filePath(QStringLiteral("mitcad-notes.log"));
  writeFile(notes, 100, old);
  writeFile(other, 100, old);

  mitcad::mcp::installLogRing();
  qDebug().noquote() << "line from before the debug log";
  DebugLog& log = DebugLog::instance();
  mitcad::DebugLogLimits limits;
  limits.fileBytes = 4000;
  limits.files = 3;
  limits.folderBytes = 12000;
  log.setLimits(limits);
  log.setFact(QStringLiteral("Design"), QStringLiteral("/designs/bracket.mitcad"));
  // Started before the 3D view: the file is made, its header waits for the
  // graphics.
  log.setFact(QStringLiteral("Graphics"), QString());
  QString error;
  CHECK(log.start(QString(), QStringLiteral("unit test"), &error));
  CHECK(error.isEmpty());
  CHECK(log.running());
  const QString first = log.currentFile();
  CHECK(QFileInfo(first).fileName().startsWith(QStringLiteral("mitcad-")));
  CHECK(QFileInfo(first).fileName().endsWith(QStringLiteral("-%1.log").arg(QCoreApplication::applicationPid())));
  CHECK(QDir(QFileInfo(first).absolutePath()) == QDir(folder));
  CHECK(QDir(log.folder()) == QDir(folder));
  CHECK(QFileInfo::exists(first) && linesOf(first).isEmpty());
  qDebug().noquote() << "a line while the header waits";
  CHECK(linesOf(first).isEmpty());
  // The view knows its GPU: the file begins with them.
  log.setFact(QStringLiteral("Graphics"), QStringLiteral("Test GPU, OpenGL 4.5"));
  // The folder's limits at the start: the two oldest of the earlier files
  // went; what is not a log stays.
  CHECK(!QFileInfo::exists(earlier[0]) && !QFileInfo::exists(earlier[1]));
  CHECK(QFileInfo::exists(earlier[2]) && QFileInfo::exists(earlier[3]));
  CHECK(QFileInfo::exists(notes) && QFileInfo::exists(other));

  // The header and the lines from before.
  QStringList lines = linesOf(first);
  CHECK(lines.value(0) == QStringLiteral("MitCad debug log"));
  for (const char* start : {"^Version: MitCad", "^Build: (release|debug), ", "^System: ",
                            "^Command line: ", "^Started: .*, file 1$"}) {
    if (!hasLine(lines, QString::fromLatin1(start))) {
      std::fprintf(stderr, "debug_log.cpp: no header line %s\n", start);
      ++failures;
    }
  }
  CHECK(lines.contains(QStringLiteral("Graphics: Test GPU, OpenGL 4.5")));
  CHECK(hasLine(lines, linePattern(QStringLiteral("debug"), QStringLiteral("a line while the header waits"))));
  CHECK(lines.contains(QStringLiteral("Design: /designs/bracket.mitcad")));
  CHECK(hasLine(lines, QStringLiteral("^Switches: debug log \\(unit test\\)")));
  CHECK(hasLine(lines, QStringLiteral("^--- The newest \\d+ lines from before the debug log started:$")));
  CHECK(hasLine(lines, linePattern(QStringLiteral("debug"), QStringLiteral("line from before the debug log"))));
  CHECK(lines.contains(QStringLiteral("--- The debug log starts")));

  // A warning is written at once, with what came before it.
  qDebug().noquote() << "a debug line";
  qWarning().noquote() << "a warning line";
  lines = linesOf(first);
  CHECK(hasLine(lines, linePattern(QStringLiteral("info"),
                                   QStringLiteral("Debug log on: %1 (unit test)").arg(QDir::toNativeSeparators(first)))));
  CHECK(hasLine(lines, linePattern(QStringLiteral("debug"), QStringLiteral("a debug line"))));
  CHECK(hasLine(lines, linePattern(QStringLiteral("warning"), QStringLiteral("a warning line"))));
  // Any line within a second.
  qDebug().noquote() << "a later debug line";
  waitMs(1300);
  CHECK(hasLine(linesOf(first), linePattern(QStringLiteral("debug"), QStringLiteral("a later debug line"))));
  // Another thread's line has its name.
  QThread* worker = QThread::create([] { qInfo().noquote() << "a line of a worker"; });
  worker->setObjectName(QStringLiteral("unit worker"));
  worker->start();
  worker->wait();
  delete worker;
  log.flush();
  CHECK(hasLine(linesOf(first), linePattern(QStringLiteral("info"), QStringLiteral("a line of a worker"),
                                            QStringLiteral("unit worker"))));

  // The rotation: 4000 bytes a file, at most three files and 12000 bytes.
  const QString filler(300, QLatin1Char('r'));
  for (int i = 0; i < 60; ++i) {
    qDebug().noquote() << QStringLiteral("rotation line %1 %2").arg(i).arg(filler);
  }
  log.flush();
  const QString current = log.currentFile();
  CHECK(current != first);
  const QList<mitcad::DebugLogFile> files = mitcad::debugLogFiles(folder);
  CHECK(files.size() <= 3);
  CHECK(!QFileInfo::exists(earlier[2]) && !QFileInfo::exists(earlier[3]));
  CHECK(QFileInfo::exists(notes) && QFileInfo::exists(other));
  qint64 total = 0;
  bool continued = false;
  for (const mitcad::DebugLogFile& file : files) {
    total += file.size;
    const QStringList content = linesOf(file.path);
    CHECK(content.value(0) == QStringLiteral("MitCad debug log"));
    if (file.path != current) {
      CHECK(file.size >= limits.fileBytes);
      continued = continued || hasLine(content, QStringLiteral("^--- Continued in mitcad-.*\\.log$"));
    }
  }
  CHECK(continued);
  CHECK(total <= limits.folderBytes + limits.fileBytes);
  lines = linesOf(current);
  const QRegularExpression startedPattern(QStringLiteral("^Started: .*, file \\d+, continued from (mitcad-.*\\.log)$"));
  qsizetype started = 0;
  while (started < lines.size() && !startedPattern.match(lines.at(started)).hasMatch()) {
    ++started;
  }
  const QRegularExpressionMatch continuedFrom = startedPattern.match(lines.value(started));
  CHECK(continuedFrom.hasMatch());
  // The last rotation line is in the file being written; or, when it took
  // the file before to its size (the header's size follows the machine and
  // the environment: mitcad#476), that file ends with it and the one being
  // written has its header only.
  const QString lastRotation = linePattern(QStringLiteral("debug"), QStringLiteral("rotation line 59 %1").arg(filler));
  if (!hasLine(lines, lastRotation)) {
    const QStringList before = linesOf(QDir(folder).filePath(continuedFrom.captured(1)));
    CHECK(started == lines.size() - 1);
    CHECK(before.size() >= 2 && hasLine(QStringList{before.value(before.size() - 2)}, lastRotation));
    CHECK(before.value(before.size() - 1) ==
          QStringLiteral("--- Continued in %1").arg(QFileInfo(current).fileName()));
  }

  // Off: the file's last line says so; nothing after it. The line itself
  // never rolls the file over (mitcad#476).
  log.stop();
  CHECK(!log.running());
  CHECK(log.currentFile().isEmpty());
  qDebug().noquote() << "a line after the stop";
  lines = linesOf(current);
  const bool offLast = hasLine(QStringList{lines.value(lines.size() - 1)},
                               linePattern(QStringLiteral("info"), QStringLiteral("Debug log off: %1")
                                                                       .arg(QDir::toNativeSeparators(current))));
  CHECK(offLast);
  CHECK(!hasLine(lines, QStringLiteral("a line after the stop")));
  const bool latest = QFileInfo(log.latestFile()) == QFileInfo(current);
  CHECK(latest);
  if (!offLast || !latest) {
    std::fprintf(stderr, "debug_log.cpp: latest %s\n", qPrintable(log.latestFile()));
    printFiles(folder, current);
  }

  // A header that waits for the graphics (default limits: the ring's lines
  // fill no file). A warning writes the file at once, with room for the
  // graphics, which take it when they come.
  const QString pending = directory.filePath(QStringLiteral("pending"));
  mitcad::DebugLogLimits waiting;
  log.setLimits(waiting);
  log.setFact(QStringLiteral("Graphics"), QString());
  const QString warned = QDir(pending).filePath(QStringLiteral("warned.log"));
  CHECK(log.start(warned, QStringLiteral("unit test"), &error));
  CHECK(log.running() && QFileInfo(log.currentFile()) == QFileInfo(warned));
  qDebug().noquote() << "a quiet line while the header waits";
  CHECK(linesOf(warned).isEmpty());
  qWarning().noquote() << "an early warning";
  lines = linesOf(warned);
  CHECK(lines.value(0) == QStringLiteral("MitCad debug log"));
  CHECK(hasLine(lines, QStringLiteral("^Graphics: not known yet \\(.*\\) +$")));
  CHECK(hasLine(lines, linePattern(QStringLiteral("debug"), QStringLiteral("a quiet line while the header waits"))));
  CHECK(hasLine(lines, linePattern(QStringLiteral("warning"), QStringLiteral("an early warning"))));
  qDebug().noquote() << "a line after the early warning";
  log.setFact(QStringLiteral("Graphics"), QStringLiteral("Late GPU, OpenGL 3.3"));
  log.flush();
  lines = linesOf(warned);
  CHECK(lines.value(0) == QStringLiteral("MitCad debug log"));
  CHECK(hasLine(lines, QStringLiteral("^Graphics: Late GPU, OpenGL 3\\.3 +$")));
  CHECK(!hasLine(lines, QStringLiteral("not known yet")));
  CHECK(hasLine(lines, linePattern(QStringLiteral("debug"), QStringLiteral("a line after the early warning"))));
  log.stop();
  // Without the graphics for graphicsWaitMs: the file is written anyway.
  log.setFact(QStringLiteral("Graphics"), QString());
  waiting.graphicsWaitMs = 300;
  log.setLimits(waiting);
  const QString timedOut = QDir(pending).filePath(QStringLiteral("timeout.log"));
  CHECK(log.start(timedOut, QStringLiteral("unit test"), &error));
  CHECK(linesOf(timedOut).isEmpty());
  waitMs(800);
  lines = linesOf(timedOut);
  CHECK(lines.value(0) == QStringLiteral("MitCad debug log"));
  CHECK(hasLine(lines, QStringLiteral("^Graphics: not known yet")));
  log.stop();
  // Stopped while it waits: the file is written, its end too.
  waiting.graphicsWaitMs = 5000;
  log.setLimits(waiting);
  const QString stopped = QDir(pending).filePath(QStringLiteral("stopped.log"));
  CHECK(log.start(stopped, QStringLiteral("unit test"), &error));
  log.stop();
  CHECK(!log.running());
  lines = linesOf(stopped);
  CHECK(lines.value(0) == QStringLiteral("MitCad debug log"));
  CHECK(hasLine(lines, linePattern(QStringLiteral("info"),
                                   QStringLiteral("Debug log off: %1").arg(QDir::toNativeSeparators(stopped)))));
  // The graphics known (as when Preferences switches it on): at once.
  log.setFact(QStringLiteral("Graphics"), QStringLiteral("Test GPU, OpenGL 4.5"));
  // The off line takes the file over its size (mitcad#476): it stays the
  // file's last line, and no file continues it.
  log.setLimits(mitcad::DebugLogLimits());
  const QString full = directory.filePath(QStringLiteral("full/full.log"));
  CHECK(log.start(full, QStringLiteral("unit test"), &error));
  log.flush();
  mitcad::DebugLogLimits fullLimits;
  fullLimits.fileBytes = QFileInfo(full).size() + 1;
  log.setLimits(fullLimits);
  log.stop();
  lines = linesOf(full);
  CHECK(QFileInfo(full).size() > fullLimits.fileBytes);
  CHECK(hasLine(QStringList{lines.value(lines.size() - 1)},
                linePattern(QStringLiteral("info"),
                            QStringLiteral("Debug log off: %1").arg(QDir::toNativeSeparators(full)))));
  CHECK(!hasLine(lines, QStringLiteral("^--- Continued in ")));
  CHECK(mitcad::debugLogFiles(directory.filePath(QStringLiteral("full")), full).size() == 1);
  // 4000 bytes a file and three files, but room for the first one: it gets
  // the ring's lines from before (over 20 kB).
  mitcad::DebugLogLimits askedLimits = limits;
  askedLimits.folderBytes = 1024 * 1024;
  log.setLimits(askedLimits);

  // A file asked for continues in <base>-2.log, -3, ...; a file of another
  // name in its folder stays.
  const QString asked = directory.filePath(QStringLiteral("asked/run.log"));
  const QString runner = directory.filePath(QStringLiteral("asked/runner.log"));
  writeFile(runner, 100, old);
  CHECK(log.start(asked, QStringLiteral("--debug-log"), &error));
  // The lines kept from before are over 4000 bytes: the start's own line
  // already continued in run-2.log.
  lines = linesOf(asked);
  CHECK(lines.value(0) == QStringLiteral("MitCad debug log"));
  CHECK(hasLine(lines, QStringLiteral("^Switches: debug log \\(--debug-log\\)")));
  CHECK(lines.contains(QStringLiteral("--- Continued in run-2.log")));
  CHECK(QFileInfo(log.currentFile()) == QFileInfo(directory.filePath(QStringLiteral("asked/run-2.log"))));
  CHECK(QDir(log.folder()) == QDir(directory.filePath(QStringLiteral("asked"))));
  for (int i = 0; i < 60; ++i) {
    qDebug().noquote() << QStringLiteral("asked line %1 %2").arg(i).arg(filler);
  }
  log.flush();
  CHECK(QFileInfo(log.currentFile()).fileName().startsWith(QStringLiteral("run-")));
  CHECK(mitcad::debugLogFiles(directory.filePath(QStringLiteral("asked")), asked).size() <= 3);
  CHECK(QFileInfo::exists(runner));
  log.stop();

  mitcad::mcp::removeLogRing();
  log.setLimits(mitcad::DebugLogLimits());
  qunsetenv("MITCAD_DEBUG_LOG_DIR");
}

} // namespace

int debugLogTests() {
  limitTests();
  requestTests();
  fileTests();
  return failures;
}
