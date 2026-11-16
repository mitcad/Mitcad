// SPDX-License-Identifier: MIT
#include "SessionLog.hpp"

#include <algorithm>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QRegularExpression>
#include <QSysInfo>
#include <QtLogging>

#include "mcp/LogRing.hpp"

namespace mitcad {
namespace {

// What the ring kept from before the file opened goes into it.
constexpr int kEarlierLines = 5000;

// Set while this thread holds the log's lock: a message logged meanwhile
// (by Qt, inside a file operation) does not wait for the lock it holds.
thread_local bool t_locked = false;

class Locked {
public:
  explicit Locked(std::mutex& mutex) : m_lock(mutex) { t_locked = true; }
  ~Locked() { t_locked = false; }
  Locked(const Locked&) = delete;
  Locked& operator=(const Locked&) = delete;

private:
  std::lock_guard<std::mutex> m_lock;
};

QString native(const QString& path) { return QDir::toNativeSeparators(path); }

const QRegularExpression& sessionPattern() {
  static const QRegularExpression pattern(QStringLiteral("^(session-(\\d{8}-\\d{6})-(\\d+))(?:-(\\d+))?\\.log$"));
  return pattern;
}

bool samePath(const QString& a, const QString& b) {
  return !a.isEmpty() && !b.isEmpty() && QFileInfo(a).absoluteFilePath() == QFileInfo(b).absoluteFilePath();
}

struct Entry {
  DebugLogFile file;
  SessionLogName name;
};

// The oldest first: by the session's start, its process, then its part.
bool olderEntry(const Entry& a, const Entry& b) {
  if (a.name.time != b.name.time) {
    return a.name.time < b.name.time;
  }
  if (a.name.pid != b.name.pid) {
    return a.name.pid < b.name.pid;
  }
  return a.name.part < b.name.part;
}

std::vector<Entry> entriesOf(const QList<DebugLogFile>& files) {
  std::vector<Entry> entries;
  for (const DebugLogFile& file : files) {
    const SessionLogName name = parseSessionLogName(QFileInfo(file.path).fileName());
    if (name.valid) {
      entries.push_back({file, name});
    }
  }
  std::sort(entries.begin(), entries.end(), olderEntry);
  return entries;
}

} // namespace

SessionLogName parseSessionLogName(const QString& fileName) {
  SessionLogName name;
  const QRegularExpressionMatch match = sessionPattern().match(fileName);
  if (!match.hasMatch()) {
    return name;
  }
  name.valid = true;
  name.session = match.captured(1);
  name.time = match.captured(2);
  name.pid = match.captured(3).toLongLong();
  name.part = match.captured(4).isEmpty() ? 1 : match.captured(4).toInt();
  return name;
}

QList<DebugLogFile> sessionLogFiles(const QString& folder) {
  QList<DebugLogFile> files;
  for (const QFileInfo& info : QDir(folder).entryInfoList({QStringLiteral("session-*.log")}, QDir::Files)) {
    if (parseSessionLogName(info.fileName()).valid) {
      files.append({info.absoluteFilePath(), info.size(), info.lastModified()});
    }
  }
  return files;
}

QStringList sessionLogFilesToRemove(QList<DebugLogFile> files, const QStringList& running, const QString& current,
                                    const SessionLogLimits& limits) {
  const std::vector<Entry> entries = entriesOf(files);
  const QString currentSession = parseSessionLogName(QFileInfo(current).fileName()).session;
  QStringList sessions; // the oldest first
  QHash<QString, int> lastPart;
  qint64 total = 0;
  for (const Entry& entry : entries) {
    if (!sessions.contains(entry.name.session)) {
      sessions << entry.name.session;
    }
    lastPart[entry.name.session] = std::max(lastPart.value(entry.name.session), entry.name.part);
    total += entry.file.size;
  }
  const auto kept = [&](const QString& session) {
    return running.contains(session) || (!currentSession.isEmpty() && session == currentSession);
  };
  QStringList removed;
  // Whole sessions over the count.
  qsizetype count = sessions.size();
  for (const QString& session : std::as_const(sessions)) {
    if (count <= limits.sessions) {
      break;
    }
    if (kept(session)) {
      continue;
    }
    for (const Entry& entry : entries) {
      if (entry.name.session == session) {
        removed << entry.file.path;
        total -= entry.file.size;
      }
    }
    --count;
  }
  // The bytes: files of the sessions not kept, the oldest first; then the
  // parts of the kept ones before their last.
  for (int pass = 0; pass < 2 && total > limits.folderBytes; ++pass) {
    for (const Entry& entry : entries) {
      if (total <= limits.folderBytes) {
        break;
      }
      if (removed.contains(entry.file.path) || samePath(entry.file.path, current)) {
        continue;
      }
      const bool isKept = kept(entry.name.session);
      if (pass == 0 ? isKept : (!isKept || entry.name.part >= lastPart.value(entry.name.session))) {
        continue;
      }
      removed << entry.file.path;
      total -= entry.file.size;
    }
  }
  return removed;
}

// ---------------------------------------------------------------------------

SessionLog& SessionLog::instance() {
  // Never deleted: the message handler may write until the process ends.
  static SessionLog* log = new SessionLog;
  return *log;
}

QString SessionLog::fileName(const QDateTime& when, qint64 pid) {
  return QStringLiteral("session-%1-%2.log").arg(when.toString(QStringLiteral("yyyyMMdd-HHmmss"))).arg(pid);
}

bool SessionLog::wanted() { return qEnvironmentVariable("MITCAD_SESSION_LOG").trimmed() != QStringLiteral("0"); }

bool SessionLog::start(const QString& folder, QString* error) {
  if (m_started) {
    return running();
  }
  m_started = true;
  const QString absolute = QDir::cleanPath(QDir(folder).absolutePath());
  if (!QDir().mkpath(absolute)) {
    if (error != nullptr) {
      *error = QStringLiteral("the folder %1 cannot be made").arg(native(absolute));
    }
    return false;
  }
  const QDir dir(absolute);
  // The sessions that ended without their clean end: a lock whose process
  // is gone. QLockFile takes such a lock over (only by the process's id:
  // no stale time, a session can run for days) and its unlock removes it.
  EndedSession ended;
  QStringList endedNames;
  for (const QFileInfo& info : dir.entryInfoList({QStringLiteral("session-*.lock")}, QDir::Files, QDir::Name)) {
    QLockFile lock(info.absoluteFilePath());
    lock.setStaleLockTime(0);
    if (!lock.tryLock(0)) {
      continue; // its session still runs (or the lock cannot be read)
    }
    lock.unlock();
    const QString session = info.completeBaseName();
    endedNames << session;
    // The newest such session (names sort by their start).
    if (ended.session.isEmpty() || session > ended.session) {
      ended.session = session;
    }
  }
  if (!ended.session.isEmpty()) {
    const SessionLogName name = parseSessionLogName(ended.session + QStringLiteral(".log"));
    ended.pid = name.pid;
    int part = 0;
    for (const DebugLogFile& file : sessionLogFiles(absolute)) {
      const SessionLogName partName = parseSessionLogName(QFileInfo(file.path).fileName());
      if (partName.session == ended.session && partName.part > part) {
        part = partName.part;
        ended.log = file.path;
      }
    }
    const QFileInfoList dumps =
        dir.entryInfoList({QStringLiteral("crash-*-%1.dmp").arg(name.pid)}, QDir::Files, QDir::Time);
    if (!dumps.isEmpty()) {
      ended.dump = dumps.first().absoluteFilePath();
    }
  }
  // Lines logged from now on come to the sink, which waits for the file
  // (a line the snapshot below takes is not written twice).
  mcp::setSessionLogSink(&SessionLog::sink);
  const QString path =
      dir.filePath(fileName(QDateTime::currentDateTime(), QCoreApplication::applicationPid()));
  {
    const Locked lock(m_mutex);
    m_ended = ended;
    m_folder = absolute;
    m_session = QFileInfo(path).completeBaseName();
    m_lock = std::make_unique<QLockFile>(dir.filePath(m_session + QStringLiteral(".lock")));
    m_lock->setStaleLockTime(0);
    if (!m_lock->tryLock(0)) {
      // Not fatal: the log is written without its lock, and the next start
      // cannot tell how this one ended.
      m_lock.reset();
    }
    m_file.setFileName(path);
    if (!m_file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
      if (error != nullptr) {
        *error = QStringLiteral("%1 cannot be written: %2").arg(native(path), m_file.errorString());
      }
      if (m_lock) {
        m_lock->unlock();
        m_lock.reset();
      }
      return false;
    }
    m_part = 1;
    m_written = 0;
    writeHeader(QString());
    // What the ring kept from before: the start-up.
    quint64 next = 1;
    const std::vector<mcp::LogLine> earlier = mcp::logLines(0, kEarlierLines, &next);
    m_snapshotUpTo = next - 1;
    for (const mcp::LogLine& line : earlier) {
      writeText(mcp::formatLogLine(line));
    }
    m_file.flush();
    prune();
  }
  qInfo().noquote() << QStringLiteral("Session log: %1").arg(native(path));
  for (const QString& session : std::as_const(endedNames)) {
    qInfo().noquote() << QStringLiteral("Previous session ended unexpectedly: %1").arg(session);
  }
  if (!ended.dump.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Its crash dump: %1").arg(native(ended.dump));
  }
  return true;
}

void SessionLog::finish() {
  const Locked lock(m_mutex);
  if (!m_file.isOpen()) {
    return;
  }
  writeText(QStringLiteral("--- The session ended: %1")
                .arg(QDateTime::currentDateTime().toString(QStringLiteral("yyyy-MM-dd HH:mm:ss.zzz"))));
  m_file.flush();
  m_file.close();
  if (m_lock) {
    m_lock->unlock();
    m_lock.reset();
  }
}

bool SessionLog::running() const {
  const Locked lock(m_mutex);
  return m_file.isOpen();
}

QString SessionLog::currentFile() const {
  const Locked lock(m_mutex);
  return m_file.isOpen() ? m_file.fileName() : QString();
}

QString SessionLog::folder() const {
  const Locked lock(m_mutex);
  return m_folder;
}

QStringList SessionLog::recentFiles(int earlier) const {
  QString folderPath;
  QString current;
  {
    const Locked lock(m_mutex);
    folderPath = m_folder;
    current = m_session;
  }
  if (folderPath.isEmpty()) {
    return {};
  }
  const std::vector<Entry> entries = entriesOf(sessionLogFiles(folderPath));
  // The sessions, the newest first: this one, then those before it.
  QStringList sessions;
  for (auto it = entries.rbegin(); it != entries.rend(); ++it) {
    if (!sessions.contains(it->name.session)) {
      sessions << it->name.session;
    }
  }
  if (!sessions.contains(current)) {
    return {}; // this session has no file
  }
  QStringList chosen{current};
  for (const QString& other : std::as_const(sessions)) {
    if (chosen.size() > earlier) {
      break;
    }
    // Before this one: a session started later is another MitCad's.
    if (other != current && (current.isEmpty() || other < current)) {
      chosen << other;
    }
  }
  QStringList files;
  for (const QString& name : std::as_const(chosen)) {
    for (const Entry& entry : entries) {
      if (entry.name.session == name) {
        files << entry.file.path;
      }
    }
  }
  return files;
}

EndedSession SessionLog::endedUnexpectedly() const {
  const Locked lock(m_mutex);
  return m_ended;
}

void SessionLog::setLimits(const SessionLogLimits& limits) {
  const Locked lock(m_mutex);
  m_limits = limits;
}

void SessionLog::sink(QtMsgType, const mcp::LogLine& line) { instance().write(line); }

void SessionLog::write(const mcp::LogLine& line) {
  if (t_locked) {
    return;
  }
  const Locked lock(m_mutex);
  if (!m_file.isOpen() || line.sequence <= m_snapshotUpTo) {
    return;
  }
  writeText(mcp::formatLogLine(line));
  // At once: a hang or a kill keeps every line before it.
  m_file.flush();
  if (m_written >= m_limits.fileBytes) {
    rollOver();
  }
}

void SessionLog::writeText(const QString& text) {
  QByteArray bytes = text.toUtf8();
  bytes.append('\n');
  const qint64 written = m_file.write(bytes);
  if (written > 0) {
    m_written += written;
  }
}

void SessionLog::writeHeader(const QString& continuedFrom) {
#ifdef NDEBUG
  const QString kind = QStringLiteral("release");
#else
  const QString kind = QStringLiteral("debug");
#endif
  writeText(QStringLiteral("MitCad session log"));
  writeText(QStringLiteral("Version: MitCad %1").arg(QCoreApplication::applicationVersion()));
  writeText(QStringLiteral("Build: %1, %2, Qt %3 (running %4)")
                .arg(kind, QSysInfo::buildAbi(), QString::fromLatin1(QT_VERSION_STR), QString::fromLatin1(qVersion())));
  writeText(QStringLiteral("System: %1 (%2 %3, %4)")
                .arg(QSysInfo::prettyProductName(), QSysInfo::kernelType(), QSysInfo::kernelVersion(),
                     QSysInfo::currentCpuArchitecture()));
  writeText(QStringLiteral("Started: %1, process %2, file %3%4")
                .arg(QDateTime::currentDateTime().toString(QStringLiteral("yyyy-MM-dd HH:mm:ss.zzz")))
                .arg(QCoreApplication::applicationPid())
                .arg(m_part)
                .arg(continuedFrom.isEmpty() ? QString() : QStringLiteral(", continued from ") + continuedFrom));
}

void SessionLog::rollOver() {
  const QString previous = QFileInfo(m_file.fileName()).fileName();
  const QString next =
      QDir(m_folder).filePath(QStringLiteral("%1-%2.log").arg(m_session).arg(m_part + 1));
  writeText(QStringLiteral("--- Continued in %1").arg(QFileInfo(next).fileName()));
  m_file.flush();
  m_file.close();
  m_file.setFileName(next);
  if (!m_file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
    return; // the rest of the session is not written
  }
  ++m_part;
  m_written = 0;
  writeHeader(previous);
  m_file.flush();
  prune();
}

QStringList SessionLog::runningSessions() const {
  // The sessions whose lock is there: they run (an ended one's lock was
  // taken over and removed at this start).
  QStringList sessions;
  for (const QFileInfo& info : QDir(m_folder).entryInfoList({QStringLiteral("session-*.lock")}, QDir::Files)) {
    sessions << info.completeBaseName();
  }
  return sessions;
}

void SessionLog::prune() {
  const QStringList removed =
      sessionLogFilesToRemove(sessionLogFiles(m_folder), runningSessions(), m_file.fileName(), m_limits);
  for (const QString& path : removed) {
    QFile::remove(path);
  }
  // The newest crash dumps.
  const QFileInfoList dumps =
      QDir(m_folder).entryInfoList({QStringLiteral("crash-*.dmp")}, QDir::Files, QDir::Time); // newest first
  for (qsizetype i = m_limits.dumps; i < dumps.size(); ++i) {
    QFile::remove(dumps.at(i).absoluteFilePath());
  }
}

} // namespace mitcad
