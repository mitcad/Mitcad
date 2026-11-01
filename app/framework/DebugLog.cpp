// SPDX-License-Identifier: MIT
#include "DebugLog.hpp"

#include <algorithm>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QProcessEnvironment>
#include <QRegularExpression>
#include <QSettings>
#include <QStandardPaths>
#include <QSysInfo>
#include <QTimer>
#include <QtLogging>

#include "mcp/LogRing.hpp"

namespace mitcad {
namespace {

const QString kSetting = QStringLiteral("diagnostics/debugLog");
// What the ring kept from before the start goes into the first file.
constexpr int kEarlierLines = 5000;
constexpr qint64 kFlushEveryMs = 1000;
// The room of a header's graphics line written before they were known.
constexpr int kGraphicsWidth = 240;

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

// mitcad-YYYYMMDD-HHMMSS-<pid>[-n].log: its time and its part n.
const QRegularExpression& defaultName() {
  static const QRegularExpression pattern(QStringLiteral("^mitcad-(\\d{8}-\\d{6})-\\d+(?:-(\\d+))?\\.log$"));
  return pattern;
}

// The part n of <base>-n.<suffix>, 1 for <base>.<suffix>.
qint64 partOf(const QString& fileName) {
  static const QRegularExpression part(QStringLiteral("-(\\d+)$"));
  const QRegularExpressionMatch match = part.match(QFileInfo(fileName).completeBaseName());
  return match.hasMatch() ? match.captured(1).toLongLong() : 1;
}

// The oldest first. The files' times can be as coarse as the system clock
// (about 15 ms on Windows), so parts written in a row share one: the log
// folder's files go by the time in their names, then by their part
// (x.log, x-2.log, ..., x-10.log); the files of a file asked for by their
// modification time, then by their part.
bool older(const DebugLogFile& a, const DebugLogFile& b) {
  const QString nameA = QFileInfo(a.path).fileName();
  const QString nameB = QFileInfo(b.path).fileName();
  const QRegularExpressionMatch defaultA = defaultName().match(nameA);
  const QRegularExpressionMatch defaultB = defaultName().match(nameB);
  if (defaultA.hasMatch() && defaultB.hasMatch()) {
    if (defaultA.captured(1) != defaultB.captured(1)) {
      return defaultA.captured(1) < defaultB.captured(1);
    }
  } else if (a.modified != b.modified) {
    return a.modified < b.modified;
  }
  const auto part = [](const QRegularExpressionMatch& asDefault, const QString& name) -> qint64 {
    if (asDefault.hasMatch()) {
      return asDefault.captured(2).isEmpty() ? 1 : asDefault.captured(2).toLongLong();
    }
    return partOf(name);
  };
  const qint64 partA = part(defaultA, nameA);
  const qint64 partB = part(defaultB, nameB);
  if (partA != partB) {
    return partA < partB;
  }
  if (a.modified != b.modified) {
    return a.modified < b.modified;
  }
  return a.path < b.path;
}

bool samePath(const QString& a, const QString& b) {
  return !a.isEmpty() && !b.isEmpty() && QFileInfo(a).absoluteFilePath() == QFileInfo(b).absoluteFilePath();
}

// path, or path with -2, -3, ... before its suffix when that is taken.
QString unusedPath(const QString& path) {
  if (!QFileInfo::exists(path)) {
    return path;
  }
  const QFileInfo info(path);
  const QString base = info.dir().filePath(info.completeBaseName());
  const QString suffix = info.suffix().isEmpty() ? QString() : QLatin1Char('.') + info.suffix();
  for (int n = 2;; ++n) {
    const QString candidate = QStringLiteral("%1-%2%3").arg(base, QString::number(n), suffix);
    if (!QFileInfo::exists(candidate)) {
      return candidate;
    }
  }
}

QString quoted(const QString& argument) {
  return argument.contains(QLatin1Char(' ')) ? QLatin1Char('"') + argument + QLatin1Char('"') : argument;
}

} // namespace

QStringList debugLogFilesToRemove(QList<DebugLogFile> files, const QString& keep, const DebugLogLimits& limits) {
  std::sort(files.begin(), files.end(), older);
  qint64 total = 0;
  for (const DebugLogFile& file : files) {
    total += file.size;
  }
  qsizetype count = files.size();
  QStringList removed;
  for (const DebugLogFile& file : files) {
    if (count <= limits.files && total <= limits.folderBytes) {
      break;
    }
    if (samePath(file.path, keep)) {
      continue;
    }
    removed << file.path;
    --count;
    total -= file.size;
  }
  return removed;
}

QList<DebugLogFile> debugLogFiles(const QString& folder, const QString& asked) {
  QRegularExpression pattern;
  if (asked.isEmpty()) {
    pattern.setPattern(QStringLiteral("^mitcad-\\d{8}-\\d{6}-\\d+(-\\d+)?\\.log$"));
  } else {
    const QFileInfo info(asked);
    pattern.setPattern(QStringLiteral("^%1(-\\d+)?%2$")
                           .arg(QRegularExpression::escape(info.completeBaseName()),
                                info.suffix().isEmpty()
                                    ? QString()
                                    : QStringLiteral("\\.") + QRegularExpression::escape(info.suffix())));
  }
  QList<DebugLogFile> files;
  for (const QFileInfo& info : QDir(folder).entryInfoList(QDir::Files)) {
    if (pattern.match(info.fileName()).hasMatch()) {
      files.append({info.absoluteFilePath(), info.size(), info.lastModified()});
    }
  }
  return files;
}

DebugLogRequest debugLogRequest(QStringList& arguments, const QString& environment) {
  DebugLogRequest request;
  const QString option = QStringLiteral("--debug-log");
  for (qsizetype i = 1; i < arguments.size(); ++i) {
    const QString argument = arguments.at(i);
    if (argument == QStringLiteral("--")) {
      break;
    }
    if (argument.startsWith(option + QLatin1Char('='))) {
      request.on = true;
      request.file = argument.mid(option.size() + 1);
      request.source = option;
      arguments[i] = option;
    } else if (argument == option) {
      request.on = true;
      request.source = option;
      if (i + 1 < arguments.size()) {
        const QString next = arguments.at(i + 1);
        if (!next.startsWith(QLatin1Char('-')) && next.endsWith(QStringLiteral(".log"), Qt::CaseInsensitive)) {
          request.file = next;
          arguments.removeAt(i + 1);
        }
      }
    }
  }
  const QString value = environment.trimmed();
  if (!request.on && !value.isEmpty() && value != QStringLiteral("0")) {
    request.on = true;
    request.source = QStringLiteral("MITCAD_DEBUG_LOG");
    if (value != QStringLiteral("1")) {
      request.file = value;
    }
  }
  return request;
}

// ---------------------------------------------------------------------------

DebugLog& DebugLog::instance() {
  // Never deleted: the message handler may write until the process ends.
  static DebugLog* log = new DebugLog;
  return *log;
}

DebugLog::DebugLog() {
  m_timer = new QTimer(this);
  m_timer->setInterval(static_cast<int>(kFlushEveryMs));
  connect(m_timer, &QTimer::timeout, this, [this] {
    QString problem;
    bool open = false;
    {
      const Locked lock(m_mutex);
      problem.swap(m_problem);
      if (m_file.isOpen() && m_dirty) {
        m_file.flush();
        m_dirty = false;
        m_lastFlush = QDateTime::currentMSecsSinceEpoch();
      }
      open = m_file.isOpen() || m_pending;
    }
    if (!problem.isEmpty()) {
      qWarning().noquote() << problem;
    }
    if (!open) {
      m_timer->stop();
      emit runningChanged(false);
    }
  });
  // The fallback of a header that waits for the graphics: a crash before
  // the 3D view starts still leaves a file.
  m_graphicsTimer = new QTimer(this);
  m_graphicsTimer->setSingleShot(true);
  connect(m_graphicsTimer, &QTimer::timeout, this, [this] {
    const Locked lock(m_mutex);
    openPending();
  });
  mcp::setLogSink(&DebugLog::sink);
}

QString DebugLog::defaultFolder() {
  const QString moved = qEnvironmentVariable("MITCAD_DEBUG_LOG_DIR");
  if (!moved.isEmpty()) {
    return QDir::cleanPath(QDir(moved).absolutePath());
  }
  return QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation) + QStringLiteral("/logs");
}

QString DebugLog::fileName(const QDateTime& when, qint64 pid) {
  return QStringLiteral("mitcad-%1-%2.log").arg(when.toString(QStringLiteral("yyyyMMdd-HHmmss"))).arg(pid);
}

bool DebugLog::settingOn() { return QSettings().value(kSetting, false).toBool(); }

void DebugLog::setSettingOn(bool on) {
  QSettings settings;
  if (on) {
    settings.setValue(kSetting, true);
  } else {
    settings.remove(kSetting);
  }
}

bool DebugLog::start(const QString& file, const QString& source, QString* error) {
  if (running()) {
    stop();
  }
  QString path;
  if (file.isEmpty()) {
    const QString folder = defaultFolder();
    if (!QDir().mkpath(folder)) {
      if (error != nullptr) {
        *error = tr("The folder %1 cannot be made.").arg(native(folder));
      }
      return false;
    }
    path = unusedPath(QDir(folder).filePath(fileName(QDateTime::currentDateTime(), QCoreApplication::applicationPid())));
  } else {
    path = QFileInfo(file).absoluteFilePath();
    QDir().mkpath(QFileInfo(path).absolutePath());
  }
  bool waits = false;
  int waitMs = 0;
  {
    const Locked lock(m_mutex);
    // Made (and found writable) now, also when its header waits.
    m_file.setFileName(path);
    if (!m_file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
      if (error != nullptr) {
        *error = tr("%1 cannot be written: %2").arg(native(path), m_file.errorString());
      }
      return false;
    }
    m_asked = file.isEmpty() ? QString() : path;
    m_source = source;
    // Started before the 3D view (--debug-log, MITCAD_DEBUG_LOG, the
    // setting at start-up): the header waits for its GPU and renderer; the
    // ring keeps the lines meanwhile.
    waits = fact(QStringLiteral("Graphics")).isEmpty();
    waitMs = m_limits.graphicsWaitMs;
    if (waits) {
      m_file.close();
      m_pending = true;
    } else {
      beginFile();
    }
  }
  m_timer->start();
  if (waits) {
    m_graphicsTimer->start(waitMs);
  }
  qInfo().noquote() << QStringLiteral("Debug log on: %1 (%2)").arg(native(path), source);
  emit runningChanged(true);
  return true;
}

void DebugLog::stop() {
  QString path;
  {
    const Locked lock(m_mutex);
    if (!m_file.isOpen() && !m_pending) {
      return;
    }
    path = m_file.fileName();
    // The off line stays the file's last line also when it takes the file
    // over its size: a roll-over would leave it followed by "--- Continued
    // in" and a new file with a header only (mitcad#476).
    m_closing = true;
  }
  // The file's last line.
  qInfo().noquote() << QStringLiteral("Debug log off: %1").arg(native(path));
  {
    const Locked lock(m_mutex);
    // A log stopped before its header could have the graphics still leaves
    // its file.
    openPending();
    m_file.flush();
    m_file.close();
    m_asked.clear();
    m_dirty = false;
    m_closing = false;
  }
  m_graphicsTimer->stop();
  m_timer->stop();
  emit runningChanged(false);
}

bool DebugLog::running() const {
  const Locked lock(m_mutex);
  return m_file.isOpen() || m_pending;
}

QString DebugLog::currentFile() const {
  const Locked lock(m_mutex);
  return m_file.isOpen() || m_pending ? m_file.fileName() : QString();
}

QString DebugLog::folder() const {
  const QString current = currentFile();
  return current.isEmpty() ? defaultFolder() : QFileInfo(current).absolutePath();
}

QString DebugLog::latestFile() const {
  const QString current = currentFile();
  if (!current.isEmpty()) {
    return current;
  }
  const QList<DebugLogFile> files = debugLogFiles(defaultFolder());
  const auto newest = std::max_element(files.begin(), files.end(), older);
  return newest == files.end() ? QString() : newest->path;
}

void DebugLog::setFact(const QString& name, const QString& value) {
  const Locked lock(m_mutex);
  bool found = false;
  for (auto& fact : m_facts) {
    if (fact.first == name) {
      fact.second = value;
      found = true;
    }
  }
  if (!found) {
    m_facts.append({name, value});
  }
  // The GPU and renderer the waiting header needs, or the room a header
  // already written kept for them.
  if (name == QStringLiteral("Graphics") && !value.isEmpty()) {
    if (m_pending) {
      openPending();
    } else if (m_file.isOpen() && m_graphicsAt >= 0) {
      QByteArray bytes = QStringLiteral("Graphics: %1").arg(value).toUtf8().left(kGraphicsWidth);
      bytes.append(QByteArray(kGraphicsWidth - static_cast<int>(bytes.size()), ' '));
      m_file.flush();
      const qint64 end = m_file.pos();
      if (m_file.seek(m_graphicsAt)) {
        m_file.write(bytes);
        m_file.seek(end);
      }
      m_file.flush();
      m_graphicsAt = -1;
    }
  }
}

void DebugLog::setLimits(const DebugLogLimits& limits) {
  const Locked lock(m_mutex);
  m_limits = limits;
}

void DebugLog::flush() {
  const Locked lock(m_mutex);
  if (m_file.isOpen() && m_dirty) {
    m_file.flush();
    m_dirty = false;
    m_lastFlush = QDateTime::currentMSecsSinceEpoch();
  }
}

void DebugLog::sink(QtMsgType type, const mcp::LogLine& line) { instance().write(type, line); }

void DebugLog::write(QtMsgType type, const mcp::LogLine& line) {
  if (t_locked) {
    return;
  }
  const bool serious = type == QtWarningMsg || type == QtCriticalMsg || type == QtFatalMsg;
  const Locked lock(m_mutex);
  if (m_pending && serious) {
    // Before the graphics are known: a warning opens the file at once (the
    // ring's lines, this one too, go in), as a crash may follow.
    openPending();
  }
  if (!m_file.isOpen() || line.sequence <= m_snapshotUpTo) {
    return;
  }
  writeText(mcp::formatLogLine(line));
  const qint64 now = QDateTime::currentMSecsSinceEpoch();
  if (serious || now - m_lastFlush >= kFlushEveryMs) {
    m_file.flush();
    m_dirty = false;
    m_lastFlush = now;
  }
  if (m_written >= m_limits.fileBytes && !m_closing) {
    rollOver();
  }
}

void DebugLog::writeText(const QString& text) {
  QByteArray bytes = text.toUtf8();
  bytes.append('\n');
  const qint64 written = m_file.write(bytes);
  if (written > 0) {
    m_written += written;
    m_dirty = true;
  }
}

void DebugLog::writeHeader(const QString& continuedFrom) {
#ifdef NDEBUG
  const QString kind = QStringLiteral("release");
#else
  const QString kind = QStringLiteral("debug");
#endif
  QStringList arguments;
  for (const QString& argument : QCoreApplication::arguments()) {
    arguments << quoted(argument);
  }
  QStringList switches{QStringLiteral("debug log (%1)").arg(m_source.isEmpty() ? QStringLiteral("?") : m_source)};
  const QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
  QStringList names = environment.keys();
  names.sort();
  for (const QString& name : std::as_const(names)) {
    if (name.startsWith(QStringLiteral("MITCAD_")) || name.startsWith(QStringLiteral("QT_"))) {
      switches << QStringLiteral("%1=%2").arg(name, environment.value(name));
    }
  }
  const QString graphics = fact(QStringLiteral("Graphics"));
  const QString design = fact(QStringLiteral("Design"));
  writeText(QStringLiteral("MitCad debug log"));
  writeText(QStringLiteral("Version: MitCad %1").arg(QCoreApplication::applicationVersion()));
  writeText(QStringLiteral("Build: %1, %2, Qt %3 (running %4)")
                .arg(kind, QSysInfo::buildAbi(), QString::fromLatin1(QT_VERSION_STR), QString::fromLatin1(qVersion())));
  writeText(QStringLiteral("System: %1 (%2 %3, %4)")
                .arg(QSysInfo::prettyProductName(), QSysInfo::kernelType(), QSysInfo::kernelVersion(),
                     QSysInfo::currentCpuArchitecture()));
  if (graphics.isEmpty()) {
    // A file opened before the view knew its graphics (a warning, the
    // time-out): the line keeps room, and setFact writes them over it.
    m_graphicsAt = m_written;
    writeText(QStringLiteral("Graphics: not known yet (the 3D view had not started when the file was opened; its "
                             "\"OpenGL renderer:\" line follows)")
                  .leftJustified(kGraphicsWidth, QLatin1Char(' ')));
  } else {
    m_graphicsAt = -1;
    writeText(QStringLiteral("Graphics: %1").arg(graphics));
  }
  writeText(QStringLiteral("Command line: %1").arg(arguments.join(QLatin1Char(' '))));
  writeText(QStringLiteral("Design: %1").arg(design.isEmpty() ? QStringLiteral("none") : design));
  writeText(QStringLiteral("Switches: %1").arg(switches.join(QStringLiteral(", "))));
  writeText(QStringLiteral("Started: %1, process %2, file %3%4")
                .arg(QDateTime::currentDateTime().toString(QStringLiteral("yyyy-MM-dd HH:mm:ss.zzz")))
                .arg(QCoreApplication::applicationPid())
                .arg(m_part)
                .arg(continuedFrom.isEmpty() ? QString() : QStringLiteral(", continued from ") + continuedFrom));
}

void DebugLog::beginFile() {
  m_pending = false;
  m_part = 1;
  m_written = 0;
  writeHeader(QString());
  // What the ring kept from before, so that a problem seen before the
  // switch was turned on (or before the header could be written) is in
  // the file too.
  quint64 next = 1;
  const std::vector<mcp::LogLine> earlier = mcp::logLines(0, kEarlierLines, &next);
  m_snapshotUpTo = next - 1;
  if (!earlier.empty()) {
    writeText(QStringLiteral("--- The newest %1 lines from before the debug log started:")
                  .arg(static_cast<qulonglong>(earlier.size())));
    for (const mcp::LogLine& line : earlier) {
      writeText(mcp::formatLogLine(line));
    }
    writeText(QStringLiteral("--- The debug log starts"));
  }
  m_file.flush();
  m_dirty = false;
  m_lastFlush = QDateTime::currentMSecsSinceEpoch();
  prune();
}

void DebugLog::openPending() {
  if (!m_pending) {
    return;
  }
  m_pending = false;
  if (!m_file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
    // Logged by the timer, outside the handler.
    m_problem =
        QStringLiteral("Debug log off: %1 cannot be written: %2").arg(native(m_file.fileName()), m_file.errorString());
    return;
  }
  beginFile();
}

void DebugLog::rollOver() {
  const QString previous = m_file.fileName();
  const QString next = nextPath();
  writeText(QStringLiteral("--- Continued in %1").arg(QFileInfo(next).fileName()));
  m_file.flush();
  m_file.close();
  m_file.setFileName(next);
  if (!m_file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
    // Logged by the timer, outside the handler.
    m_problem = QStringLiteral("Debug log off: %1 cannot be written: %2").arg(native(next), m_file.errorString());
    return;
  }
  ++m_part;
  m_written = 0;
  writeHeader(QFileInfo(previous).fileName());
  m_file.flush();
  m_dirty = false;
  m_lastFlush = QDateTime::currentMSecsSinceEpoch();
  prune();
}

void DebugLog::prune() {
  const QString folder = QFileInfo(m_file.fileName()).absolutePath();
  const QStringList removed = debugLogFilesToRemove(debugLogFiles(folder, m_asked), m_file.fileName(), m_limits);
  for (const QString& path : removed) {
    QFile::remove(path);
  }
}

QString DebugLog::nextPath() const {
  if (m_asked.isEmpty()) {
    // A new name for the time now; in the same second as the file being
    // written, a part after its own: a part the folder's limits removed is
    // never taken again (it would sort before the newer ones).
    const QFileInfo current(m_file.fileName());
    const QString name = fileName(QDateTime::currentDateTime(), QCoreApplication::applicationPid());
    const QString stem = name.chopped(4); // without ".log"
    int part = 1;
    const QRegularExpressionMatch match = defaultName().match(current.fileName());
    if (match.hasMatch() && current.fileName().startsWith(stem)) {
      part = (match.captured(2).isEmpty() ? 1 : match.captured(2).toInt()) + 1;
    }
    for (;; ++part) {
      const QString candidate = current.dir().filePath(
          part == 1 ? name : QStringLiteral("%1-%2.log").arg(stem, QString::number(part)));
      if (!QFileInfo::exists(candidate)) {
        return candidate;
      }
    }
  }
  const QFileInfo info(m_asked);
  const QString base = info.dir().filePath(info.completeBaseName());
  const QString suffix = info.suffix().isEmpty() ? QString() : QLatin1Char('.') + info.suffix();
  for (int n = m_part + 1;; ++n) {
    const QString candidate = QStringLiteral("%1-%2%3").arg(base, QString::number(n), suffix);
    if (!QFileInfo::exists(candidate)) {
      return candidate;
    }
  }
}

QString DebugLog::fact(const QString& name) const {
  for (const auto& fact : m_facts) {
    if (fact.first == name) {
      return fact.second;
    }
  }
  return QString();
}

} // namespace mitcad
