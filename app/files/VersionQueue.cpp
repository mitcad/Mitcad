// SPDX-License-Identifier: MIT
#include "VersionQueue.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QCoreApplication>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QThread>
#include <QTimer>
#include <QtLogging>

namespace mitcad {
namespace {

// A local version command that takes longer than this is logged.
constexpr qint64 kSlowAsideMs = 100;
// How often the stall watch's timer ticks.
constexpr int kStallTickMs = 50;
// How long closing waits for the versions to go.
constexpr int kCloseWaitSeconds = 10;

} // namespace

VersionQueue& VersionQueue::instance() {
  static VersionQueue* queue = new VersionQueue; // lives as long as the process
  return *queue;
}

QString VersionQueue::keyOf(const QString& folder) {
  if (folder.isEmpty()) {
    return {};
  }
  const QString clean = QDir::cleanPath(QDir::fromNativeSeparators(folder));
#ifdef _WIN32
  return clean.toLower();
#else
  return clean;
#endif
}

quint64 VersionQueue::submit(const QString& folder, const QString& name, std::function<void()> begin) {
  const quint64 id = ++m_next;
  const QString key = keyOf(folder);
  Queue& queue = m_queues[key];
  if (queue.running == 0 && queue.waiting.empty()) {
    queue.running = id;
    queue.runningName = name;
    qInfo().noquote() << QStringLiteral("Version queue: %1 started in %2").arg(name, folder);
    emit changed(key);
    if (begin) {
      begin();
    }
    return id;
  }
  qInfo().noquote() << QStringLiteral("Version queue: %1 waits for %2 in %3")
                           .arg(name, queue.runningName.isEmpty() ? QStringLiteral("earlier work") : queue.runningName,
                                folder);
  queue.waiting.push_back(Work{id, name, std::move(begin)});
  emit changed(key);
  return id;
}

void VersionQueue::finished(quint64 id) {
  if (id == 0) {
    return;
  }
  for (auto& [key, queue] : m_queues) {
    if (queue.running == id) {
      queue.running = 0;
      queue.runningName.clear();
      const QString folder = key;
      beginNext(folder);
      emit changed(folder);
      return;
    }
    const auto found = std::find_if(queue.waiting.begin(), queue.waiting.end(),
                                    [id](const Work& work) { return work.id == id; });
    if (found != queue.waiting.end()) {
      qInfo().noquote() << QStringLiteral("Version queue: %1 withdrawn in %2").arg(found->name, key);
      queue.waiting.erase(found);
      const QString folder = key;
      emit changed(folder);
      return;
    }
  }
}

void VersionQueue::beginNext(const QString& key) {
  const auto at = m_queues.find(key);
  if (at == m_queues.end()) {
    return;
  }
  Queue& queue = at->second;
  if (queue.waiting.empty()) {
    m_queues.erase(at);
    return;
  }
  Work next = std::move(queue.waiting.front());
  queue.waiting.erase(queue.waiting.begin());
  queue.running = next.id;
  queue.runningName = next.name;
  qInfo().noquote() << QStringLiteral("Version queue: %1 started in %2").arg(next.name, key);
  if (next.begin) {
    // Through the event loop: the work before it may end in a destructor
    // (a window closing), where nothing new should start running yet.
    QMetaObject::invokeMethod(this, std::move(next.begin), Qt::QueuedConnection);
  }
}

bool VersionQueue::isWaiting(quint64 id) const {
  for (const auto& [key, queue] : m_queues) {
    for (const Work& work : queue.waiting) {
      if (work.id == id) {
        return true;
      }
    }
  }
  return false;
}

QString VersionQueue::running(const QString& folder) const {
  const auto at = m_queues.find(keyOf(folder));
  return at == m_queues.end() ? QString() : at->second.runningName;
}

int VersionQueue::waitingCount(const QString& folder) const {
  const auto at = m_queues.find(keyOf(folder));
  return at == m_queues.end() ? 0 : static_cast<int>(at->second.waiting.size());
}

void runAside(const QString& name, const std::function<void()>& work) {
  QElapsedTimer clock;
  clock.start();
  std::exception_ptr failure;
  QThread* thread = QThread::create([&work, &failure] {
    try {
      work();
    } catch (...) {
      failure = std::current_exception();
    }
  });
  {
    // `finished` comes from the thread itself: queued to this loop, so it
    // cannot be missed between start() and exec().
    QEventLoop loop;
    QObject::connect(thread, &QThread::finished, &loop, &QEventLoop::quit);
    thread->start();
    loop.exec(QEventLoop::ExcludeUserInputEvents);
  }
  thread->wait();
  delete thread;
  const qint64 took = clock.elapsed();
  if (took > kSlowAsideMs) {
    qInfo().noquote() << QStringLiteral("Version command %1 took %2 ms (off the UI thread)").arg(name).arg(took);
  }
  if (failure) {
    std::rethrow_exception(failure);
  }
}

int closeWaitSeconds() {
  bool ok = false;
  const int seconds = qEnvironmentVariableIntValue("MITCAD_TEST_REMOTE_CLOSE_WAIT_SECONDS", &ok);
  return ok && seconds > 0 ? seconds : kCloseWaitSeconds;
}

void watchStalls() {
  static bool watching = false;
  bool ok = false;
  const int limit = qEnvironmentVariableIntValue("MITCAD_TEST_UI_STALL_MS", &ok);
  if (watching || !ok || limit <= 0 || QCoreApplication::instance() == nullptr) {
    return;
  }
  watching = true;
  auto* timer = new QTimer(QCoreApplication::instance());
  auto* clock = new QElapsedTimer;
  clock->start();
  timer->setInterval(kStallTickMs);
  QObject::connect(timer, &QTimer::timeout, timer, [clock, limit] {
    const qint64 gap = clock->restart();
    if (gap > limit) {
      qWarning().noquote() << QStringLiteral("UI stall: %1 ms").arg(gap);
    }
  });
  QObject::connect(timer, &QObject::destroyed, [clock] { delete clock; });
  qInfo().noquote() << QStringLiteral("UI stall watch: gaps over %1 ms").arg(limit);
  timer->start();
}

} // namespace mitcad
