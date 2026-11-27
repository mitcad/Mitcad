// SPDX-License-Identifier: MIT
#include "HangWatchdog.hpp"

#include <algorithm>
#include <chrono>
#include <vector>

#include <QByteArray>
#include <QDebug>
#include <QStringList>
#include <QThread>
#include <QTimer>
#include <QtLogging>

#include "AccessibilityClients.hpp"
#include "mcp/LogRing.hpp"

namespace mitcad {
namespace {

// A hang still going on is logged again every this many ms.
constexpr qint64 kRepeatMs = 30000;
// The longest the hang's line quotes of the last action and log line.
constexpr qsizetype kQuoted = 200;

std::mutex g_activityMutex;
QString g_activity;
// The graphics call the UI thread is in (noteGraphicsStep), "" none; under
// g_activityMutex.
QString g_graphicsStep;

qint64 steadyMs() {
  return std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now().time_since_epoch())
      .count();
}

QString quoted(const QString& text) {
  QString line = text.simplified();
  return line.size() > kQuoted ? line.left(kQuoted - 1) + QStringLiteral("…") : line;
}

// The UI thread's newest log line (of the lines the ring keeps), "" none.
QString lastUiLine() {
  const std::vector<mcp::LogLine> lines = mcp::logLines(0, 50);
  for (auto it = lines.rbegin(); it != lines.rend(); ++it) {
    if (it->thread.isEmpty()) {
      return it->text;
    }
  }
  return QString();
}

} // namespace

int HangWatchdog::thresholdFromEnvironment() {
  const QByteArray set = qgetenv("MITCAD_HANG_WATCHDOG_MS").trimmed();
  if (!set.isEmpty()) {
    bool ok = false;
    const int value = set.toInt(&ok);
    return ok && value > 0 ? value : 0;
  }
  // The UI tests' slow steps (a software renderer, a debug build) are no
  // hangs: off unless a test asks for it.
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") || qEnvironmentVariableIntValue("MITCAD_TEST_SYNC") == 1 ||
      qEnvironmentVariableIntValue("MITCAD_TEST_LOG_URLS") == 1) {
    return 0;
  }
  return 5000;
}

HangWatchdog::HangWatchdog(int thresholdMs, QObject* parent) : QObject(parent), m_threshold(std::max(thresholdMs, 0)) {
  if (m_threshold == 0) {
    return;
  }
  const int interval = std::clamp(m_threshold / 4, 50, 250);
  // A turn of the event loop: a timer's tick.
  m_timer = new QTimer(this);
  m_timer->setInterval(interval);
  connect(m_timer, &QTimer::timeout, this, &HangWatchdog::beat);
  m_timer->start();
  m_thread = QThread::create([this] { watch(); });
  m_thread->setObjectName(QStringLiteral("hang watchdog"));
  m_thread->start();
  qInfo().noquote() << QStringLiteral("Hang watchdog: %1 ms").arg(m_threshold);
}

HangWatchdog::~HangWatchdog() { stop(); }

void HangWatchdog::stop() {
  if (m_timer != nullptr) {
    m_timer->stop();
  }
  if (m_thread == nullptr) {
    return;
  }
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_stopping = true;
  }
  m_wake.notify_all();
  m_thread->wait();
  delete m_thread;
  m_thread = nullptr;
}

void HangWatchdog::watchShutdown() {
  if (m_thread == nullptr) {
    return;
  }
  // The loop's timer ticks no more; its last beat must not read as a hang.
  if (m_timer != nullptr) {
    m_timer->stop();
  }
  const std::lock_guard<std::mutex> lock(m_mutex);
  if (!m_shutdown) {
    m_shutdown = true;
    m_reported = false;
  }
}

void HangWatchdog::timeStep(const QString& step, int limitMs) {
  if (m_thread == nullptr) {
    return;
  }
  // The first step: the event loop has ended.
  watchShutdown();
  QString finished;
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    const qint64 now = steadyMs();
    if (m_stepReported) {
      finished = QStringLiteral("Shutdown step finished after %1 s: %2")
                     .arg((now - m_stepSince) / 1000.0, 0, 'f', 1)
                     .arg(quoted(m_step));
    }
    m_step = step;
    m_stepSince = now;
    m_stepLimit = limitMs > 0 ? limitMs : m_threshold;
    m_stepReported = false;
    m_stepNextReport = 0;
  }
  if (!finished.isEmpty()) {
    qInfo().noquote() << finished;
  }
}

QString HangWatchdog::checkStep() {
  if (m_step.isEmpty()) {
    return QString();
  }
  const qint64 running = steadyMs() - m_stepSince;
  if (running < m_stepLimit) {
    return QString();
  }
  const QString seconds = QString::number(running / 1000.0, 'f', 1);
  if (!m_stepReported) {
    m_stepReported = true;
    m_stepNextReport = running + kRepeatMs;
    return QStringLiteral("Shutdown step not finished after %1 s: %2").arg(seconds, quoted(m_step));
  }
  if (running >= m_stepNextReport) {
    m_stepNextReport += kRepeatMs;
    return QStringLiteral("Shutdown step still not finished after %1 s: %2").arg(seconds, quoted(m_step));
  }
  return QString();
}

void HangWatchdog::noteActivity(const QString& activity) {
  const std::lock_guard<std::mutex> lock(g_activityMutex);
  g_activity = activity;
}

void HangWatchdog::noteGraphicsStep(const QString& step) {
  const std::lock_guard<std::mutex> lock(g_activityMutex);
  g_graphicsStep = step;
}

void HangWatchdog::beat() {
  const qint64 now = steadyMs();
  qint64 before = 0;
  bool reported = false;
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    before = m_lastBeat;
    reported = m_reported;
    m_lastBeat = now;
    m_reported = false;
  }
  if (reported && before > 0) {
    qInfo().noquote() << QStringLiteral("UI responding again after %1 s").arg((now - before) / 1000.0, 0, 'f', 1);
  }
}

void HangWatchdog::watch() {
  const auto interval = std::chrono::milliseconds(std::clamp(m_threshold / 4, 50, 250));
  std::unique_lock<std::mutex> lock(m_mutex);
  while (!m_stopping) {
    m_wake.wait_for(lock, interval);
    if (m_stopping) {
      continue;
    }
    if (m_shutdown) {
      const QString warning = checkStep();
      if (!warning.isEmpty()) {
        // Under the lock: the UI thread's "finished" line comes after it.
        qWarning().noquote() << warning;
      }
      continue;
    }
    if (m_lastBeat == 0) {
      continue;
    }
    const qint64 hung = steadyMs() - m_lastBeat;
    if (hung < m_threshold) {
      continue;
    }
    QString text;
    if (!m_reported) {
      m_reported = true;
      m_nextReport = hung + kRepeatMs;
      QString activity;
      QString graphicsStep;
      {
        const std::lock_guard<std::mutex> activityLock(g_activityMutex);
        activity = g_activity;
        graphicsStep = g_graphicsStep;
      }
      QStringList known;
      known << QStringLiteral("last action: %1").arg(activity.isEmpty() ? QStringLiteral("none") : quoted(activity));
      // Inside a graphics call that has not returned (mitcad#2406).
      if (!graphicsStep.isEmpty()) {
        known << QStringLiteral("in graphics step: %1").arg(quoted(graphicsStep));
      }
      const QString line = lastUiLine();
      if (!line.isEmpty()) {
        known << QStringLiteral("last line: %1").arg(quoted(line));
      }
      // A client reading the window through UI Automation (mitcad#2227).
      // COM does not tell a caller in the multithreaded apartment: the
      // processes that can be reading then (this thread, not the UI's).
      const QString client = currentAccessibilityClient();
      if (!client.isEmpty()) {
        known << QStringLiteral("accessibility client: %1").arg(client);
        if (client.endsWith(QLatin1String("(pid unknown)"))) {
          known << QStringLiteral("UI Automation loaded in %1").arg(accessibilityClientCandidates());
        }
      }
      text = QStringLiteral("UI not responding for %1 s (%2)").arg(hung / 1000).arg(known.join(QStringLiteral("; ")));
    } else if (hung >= m_nextReport) {
      m_nextReport += kRepeatMs;
      text = QStringLiteral("UI not responding for %1 s").arg(hung / 1000);
    } else {
      continue;
    }
    // Under the lock: the UI thread's "responding again" comes after it.
    qWarning().noquote() << text;
  }
}

} // namespace mitcad
