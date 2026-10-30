// SPDX-License-Identifier: MIT
#include "LogRing.hpp"

#include <algorithm>
#include <atomic>
#include <cstddef>
#include <deque>
#include <mutex>

#include <QCoreApplication>
#include <QDateTime>
#include <QThread>
#include <QtLogging>

namespace mitcad::mcp {
namespace {

constexpr std::size_t kKept = 5000;
// mitcad#935: the ring's bytes too, and each line's text cut so that its
// formatted line stays under kMaxLogLineBytes.
constexpr std::size_t kKeptBytes = 8 * 1024 * 1024;
constexpr qsizetype kTextBytes = kMaxLogLineBytes - 512;

struct Ring {
  std::mutex mutex;
  std::deque<LogLine> lines;
  std::size_t bytes = 0;
  quint64 next = 1;
  QtMessageHandler previous = nullptr;
  bool installed = false;
  std::atomic<LogSink> sink{nullptr};
};

Ring& ring() {
  static Ring instance;
  return instance;
}

// What a line takes in the ring, about: its strings' characters and the
// line itself.
std::size_t lineBytes(const LogLine& line) {
  return sizeof(LogLine) +
         static_cast<std::size_t>(line.text.size() + line.level.size() + line.thread.size()) * sizeof(QChar);
}

// The text as the ring keeps it: whole when it takes at most kTextBytes in
// UTF-8, a line break counted with the formatted line's indent; else as
// much as fits and a marker of how many characters were cut. Only the kept
// part is walked, however long the text.
QString keptText(const QString& text) {
  qsizetype bytes = 0;
  qsizetype index = 0;
  const qsizetype size = text.size();
  while (index < size) {
    const char16_t unit = text.at(index).unicode();
    qsizetype cost = 3;
    qsizetype units = 1;
    if (unit == u'\n') {
      cost = 5;
    } else if (unit < 0x80) {
      cost = 1;
    } else if (unit < 0x800) {
      cost = 2;
    } else if (QChar::isHighSurrogate(unit) && index + 1 < size && text.at(index + 1).isLowSurrogate()) {
      cost = 4;
      units = 2;
    }
    if (bytes + cost > kTextBytes) {
      break;
    }
    bytes += cost;
    index += units;
  }
  if (index >= size) {
    return text;
  }
  return text.left(index) + QStringLiteral(" … [%1 more characters cut]").arg(size - index);
}

// A message logged while this thread is in the handler (by the sink, or
// by the handler before it): passed on to the previous handler only.
thread_local bool t_handling = false;

QString levelName(QtMsgType type) {
  switch (type) {
  case QtDebugMsg:
    return QStringLiteral("debug");
  case QtInfoMsg:
    return QStringLiteral("info");
  case QtWarningMsg:
    return QStringLiteral("warning");
  case QtCriticalMsg:
    return QStringLiteral("critical");
  case QtFatalMsg:
    return QStringLiteral("fatal");
  }
  return QStringLiteral("debug");
}

// The logging thread's name, empty for the GUI thread.
QString threadName() {
  const QCoreApplication* app = QCoreApplication::instance();
  QThread* current = QThread::currentThread();
  if (app == nullptr || current == nullptr || current == app->thread()) {
    return QString();
  }
  const QString name = current->objectName();
  return name.isEmpty()
             ? QStringLiteral("thread %1")
                   .arg(static_cast<qulonglong>(reinterpret_cast<quintptr>(QThread::currentThreadId())), 0, 16)
             : name;
}

void handle(QtMsgType type, const QMessageLogContext& context, const QString& message) {
  Ring& r = ring();
  if (t_handling) {
    QtMessageHandler previous = nullptr;
    {
      const std::lock_guard<std::mutex> lock(r.mutex);
      previous = r.previous;
    }
    if (previous != nullptr) {
      previous(type, context, message);
    }
    return;
  }
  t_handling = true;
  LogLine line;
  line.level = levelName(type);
  line.text = keptText(message);
  line.time = QDateTime::currentMSecsSinceEpoch();
  line.thread = threadName();
  QtMessageHandler previous = nullptr;
  {
    // Any thread logs (the model's worker too).
    const std::lock_guard<std::mutex> lock(r.mutex);
    line.sequence = r.next++;
    r.lines.push_back(line);
    r.bytes += lineBytes(line);
    while (r.lines.size() > kKept || (r.bytes > kKeptBytes && r.lines.size() > 1)) {
      r.bytes -= lineBytes(r.lines.front());
      r.lines.pop_front();
    }
    previous = r.previous;
  }
  // The file first: for a fatal message the default handler ends the
  // process.
  if (const LogSink sink = r.sink.load()) {
    sink(type, line);
  }
  if (previous != nullptr) {
    previous(type, context, message);
  }
  t_handling = false;
}

} // namespace

void installLogRing() {
  Ring& r = ring();
  if (r.installed) {
    return;
  }
  // Messages of other threads wait until the previous handler is known.
  const std::lock_guard<std::mutex> lock(r.mutex);
  r.previous = qInstallMessageHandler(handle);
  r.installed = true;
}

void removeLogRing() {
  Ring& r = ring();
  if (!r.installed) {
    return;
  }
  QtMessageHandler previous = nullptr;
  {
    const std::lock_guard<std::mutex> lock(r.mutex);
    previous = r.previous;
    r.installed = false;
  }
  qInstallMessageHandler(previous);
}

std::vector<LogLine> logLines(quint64 since, int limit, quint64* next, bool* dropped) {
  Ring& r = ring();
  const std::lock_guard<std::mutex> lock(r.mutex);
  if (next != nullptr) {
    *next = r.next;
  }
  if (dropped != nullptr) {
    *dropped = !r.lines.empty() && since + 1 < r.lines.front().sequence;
  }
  std::vector<LogLine> found;
  for (const LogLine& line : r.lines) {
    if (line.sequence > since) {
      found.push_back(line);
    }
  }
  const std::size_t most = static_cast<std::size_t>(std::max(limit, 0));
  if (found.size() > most) {
    found.erase(found.begin(), found.end() - static_cast<std::ptrdiff_t>(most));
  }
  return found;
}

QString formatLogLine(const LogLine& line) {
  QString text = QDateTime::fromMSecsSinceEpoch(line.time).toString(QStringLiteral("yyyy-MM-dd HH:mm:ss.zzz"));
  text += QLatin1Char(' ');
  text += line.level.leftJustified(8, QLatin1Char(' '));
  if (!line.thread.isEmpty()) {
    text += QStringLiteral(" [%1]").arg(line.thread);
  }
  text += QLatin1Char(' ');
  QString message = line.text;
  message.replace(QLatin1Char('\n'), QStringLiteral("\n    "));
  text += message;
  return text;
}

void setLogSink(LogSink sink) { ring().sink.store(sink); }

std::size_t logRingBytes() {
  Ring& r = ring();
  const std::lock_guard<std::mutex> lock(r.mutex);
  return r.bytes;
}

} // namespace mitcad::mcp
