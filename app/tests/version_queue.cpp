// SPDX-License-Identifier: MIT
// Version work off the UI thread (mitcad#1895): a project folder's queue
// runs its work one at a time in order, other folders' beside it, and a
// withdrawn work leaves its place; a command run aside runs on a thread of
// its own while this thread's timers go on; a project's sync state in words.
#include "UnitTests.hpp"
#include "files/SyncState.hpp"
#include "files/VersionQueue.hpp"

#include <chrono>
#include <cstdio>
#include <stdexcept>
#include <thread>

#include <QCoreApplication>
#include <QDateTime>
#include <QStringList>
#include <QThread>
#include <QTimer>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "version_queue.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// The work that waited begins through the event loop.
void settle() {
  for (int i = 0; i < 3; ++i) {
    QCoreApplication::processEvents();
  }
}

void keyTests() {
  CHECK(VersionQueue::keyOf(QString()).isEmpty());
  CHECK(VersionQueue::keyOf(QStringLiteral("/work/Shop/")) == VersionQueue::keyOf(QStringLiteral("/work/Shop")));
  CHECK(VersionQueue::keyOf(QStringLiteral("/work/a/../Shop")) == VersionQueue::keyOf(QStringLiteral("/work/Shop")));
#ifdef _WIN32
  // Windows does not tell the case of letters apart, nor the slashes.
  CHECK(VersionQueue::keyOf(QStringLiteral("C:\\Work\\Shop")) == VersionQueue::keyOf(QStringLiteral("c:/work/shop")));
#endif
}

void orderTests() {
  VersionQueue queue;
  QStringList began;
  const QString shop = QStringLiteral("/work/Shop");
  const QString yard = QStringLiteral("/work/Yard");
  const auto note = [&began](const char* name) { return [&began, name] { began << QString::fromLatin1(name); }; };
  // Nothing runs: the first begins at once.
  const quint64 push = queue.submit(shop, QStringLiteral("push"), note("push"));
  CHECK(push != 0);
  CHECK(began == QStringList{QStringLiteral("push")});
  CHECK(queue.running(shop) == QStringLiteral("push"));
  CHECK(!queue.isWaiting(push));
  // The same project's next ones wait, in order.
  const quint64 poll = queue.submit(shop, QStringLiteral("lock_poll"), note("lock_poll"));
  const quint64 fetch = queue.submit(shop + QLatin1Char('/'), QStringLiteral("fetch"), note("fetch"));
  CHECK(queue.isWaiting(poll) && queue.isWaiting(fetch));
  CHECK(queue.waitingCount(shop) == 2);
  CHECK(began.size() == 1);
  // Another project's work does not wait for it.
  const quint64 other = queue.submit(yard, QStringLiteral("push"), note("yard push"));
  CHECK(began.size() == 2 && began.last() == QStringLiteral("yard push"));
  // A waiting one withdrawn leaves its place; the next begins when the
  // running one ends.
  queue.finished(poll);
  CHECK(!queue.isWaiting(poll));
  CHECK(queue.waitingCount(shop) == 1);
  queue.finished(push);
  // Not in the middle of the call that ended the work before it.
  CHECK(began.last() != QStringLiteral("fetch"));
  CHECK(queue.running(shop) == QStringLiteral("fetch"));
  settle();
  CHECK(began.last() == QStringLiteral("fetch"));
  CHECK(!began.contains(QStringLiteral("lock_poll")));
  CHECK(queue.running(shop) == QStringLiteral("fetch"));
  // Reported twice, or unknown: nothing happens.
  queue.finished(push);
  queue.finished(0);
  CHECK(queue.running(shop) == QStringLiteral("fetch"));
  queue.finished(fetch);
  queue.finished(other);
  CHECK(queue.running(shop).isEmpty() && queue.running(yard).isEmpty());
  CHECK(queue.waitingCount(shop) == 0);
  // Idle again: the next one begins at once.
  const quint64 sync = queue.submit(shop, QStringLiteral("sync"), note("sync"));
  CHECK(began.last() == QStringLiteral("sync"));
  // Work submitted while its own begins waits for it.
  quint64 inner = 0;
  const quint64 outer = queue.submit(yard, QStringLiteral("release"), [&] {
    inner = queue.submit(yard, QStringLiteral("lock_take"), note("lock_take"));
  });
  CHECK(inner != 0 && queue.isWaiting(inner));
  queue.finished(outer);
  settle();
  CHECK(began.last() == QStringLiteral("lock_take"));
  queue.finished(inner);
  queue.finished(sync);
}

void asideTests() {
  // On a thread of its own; this thread's timers go on meanwhile.
  QThread* uiThread = QThread::currentThread();
  QThread* ran = nullptr;
  int ticks = 0;
  QTimer timer;
  timer.setInterval(10);
  QObject::connect(&timer, &QTimer::timeout, [&ticks] { ++ticks; });
  timer.start();
  runAside(QStringLiteral("test"), [&ran] {
    ran = QThread::currentThread();
    std::this_thread::sleep_for(std::chrono::milliseconds(300));
  });
  timer.stop();
  CHECK(ran != nullptr && ran != uiThread);
  CHECK(ticks >= 5);
  // Its exception comes to the caller.
  bool thrown = false;
  try {
    runAside(QStringLiteral("failing"), [] { throw std::runtime_error("refused"); });
  } catch (const std::runtime_error& e) {
    thrown = QString::fromUtf8(e.what()) == QStringLiteral("refused");
  }
  CHECK(thrown);
}

void wordTests() {
  const QDate today(2026, 11, 15);
  const QDateTime noon(today, QTime(12, 5));
  CHECK(noon.isValid());
  SyncState state;
  // Not known yet.
  CHECK(syncWords(state, today).logged == QStringLiteral("not synced yet"));
  // Sending.
  state.busy = QStringLiteral("push");
  SyncWords words = syncWords(state, today);
  CHECK(words.text == QStringLiteral("Sending to the project's server…"));
  CHECK(words.logged == QStringLiteral("sending") && !words.problem);
  state.queued = true;
  CHECK(syncWords(state, today).logged == QStringLiteral("waiting"));
  state.queued = false;
  state.busy = QStringLiteral("sync");
  CHECK(syncWords(state, today).logged == QStringLiteral("syncing"));
  state.busy = QStringLiteral("fetch");
  CHECK(syncWords(state, today).logged == QStringLiteral("checking"));
  // Up to date, with the time of the last sync.
  state = SyncState();
  state.ahead = 0;
  state.behind = 0;
  state.lastDone = noon.toUTC();
  words = syncWords(state, today);
  CHECK(words.text.startsWith(QStringLiteral("Up to date, last sync ")));
  CHECK(words.logged.startsWith(QStringLiteral("up to date, last sync ")));
  CHECK(!words.problem);
  // Another day: with the date.
  CHECK(syncWords(state, today.addDays(3)).logged.contains(QStringLiteral("2026-11-15")));
  // Offline: kept and sent later, with the versions waiting.
  state.problem = QStringLiteral("network");
  state.ahead = 2;
  words = syncWords(state, today);
  CHECK(words.text.startsWith(QStringLiteral("Offline: changes are kept and sent later (2 version")));
  CHECK(words.logged.startsWith(QStringLiteral("offline")) && words.problem);
  state.problem = QStringLiteral("auth_failed");
  CHECK(syncWords(state, today).logged.startsWith(QStringLiteral("sign-in needed")));
  state.problem = QStringLiteral("other");
  CHECK(syncWords(state, today).problem);
  // A refused push is no failure of its own: newer versions are there.
  state.problem = QStringLiteral("rejected");
  state.behind = 1;
  words = syncWords(state, today);
  CHECK(!words.problem && words.logged.startsWith(QStringLiteral("ahead 2, behind 1")));
  // A conflict.
  state.conflict = true;
  CHECK(syncWords(state, today).logged.startsWith(QStringLiteral("conflict")));
  // No names, ids or coordinates in the words: none of the state's own.
  state = SyncState();
  state.ahead = 3;
  CHECK(syncWords(state, today).logged == QStringLiteral("ahead 3"));
}

} // namespace

int versionQueueTests() {
  failures = 0;
  keyTests();
  orderTests();
  asideTests();
  wordTests();
  return failures;
}
MITCAD_UNIT_TEST(versionQueueTests);
