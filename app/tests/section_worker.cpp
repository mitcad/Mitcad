// SPDX-License-Identifier: MIT
// The section worker's rounds (mitcad#438): work runs off the UI thread with
// the round's stop as its thread's geometry cancel; a newer View round
// stops the running one and replaces those waiting, and only its result is
// delivered; a Measure round waits for the View round before it and is
// superseded by a newer Measure round; a Preview round (mitcad#943) is
// superseded only by a newer Preview round, not by cancel(), and a View
// round waits for it; stop() waits for the round that runs
// and delivers nothing; cancel() does not wait; the log lines of each
// round pair up ("Computing section <label> started", then "done in" or
// "cancelled after").
#include "framework/SectionWorker.hpp"

#include <atomic>
#include <chrono>
#include <cstdio>
#include <functional>
#include <stdexcept>
#include <thread>

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QThread>
#include <QtLogging>

#include "mitcad/geometry/cancel.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "section_worker.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QStringList g_lines;
QtMessageHandler g_previous = nullptr;

void collect(QtMsgType type, const QMessageLogContext& context, const QString& message) {
  g_lines << message;
  if (g_previous != nullptr) {
    g_previous(type, context, message);
  }
}

// Handles the UI thread's events until `done` or 5 s.
bool waitUntil(const std::function<bool()>& done) {
  QElapsedTimer clock;
  clock.start();
  while (!done() && clock.elapsed() < 5000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    std::this_thread::sleep_for(std::chrono::milliseconds(2));
  }
  return done();
}

// How many lines match.
int count(const QString& pattern) {
  const QRegularExpression expression(pattern);
  int found = 0;
  for (const QString& line : g_lines) {
    found += expression.match(line).hasMatch() ? 1 : 0;
  }
  return found;
}

// Every round started has ended once.
bool balanced() {
  return count(QStringLiteral("^Computing section .* started$")) ==
         count(QStringLiteral("^Computing section .* (done in|cancelled after) [0-9]+ ms$"));
}

// A round's work that runs until its stop or until `release`; records that
// it ran, on which thread, and whether its thread's geometry was told to
// stop.
struct Blocking {
  std::atomic<bool> started{false};
  std::atomic<bool> left{false};
  std::atomic<bool> release{false};
  std::atomic<bool> offUi{false};
  std::atomic<bool> geometryStopped{false};
  bool delivered = false;

  mitcad::SectionWorker::Work work(QThread* ui) {
    return [this, ui](const mitcad::geometry::CancelSource& stop) {
      started = true;
      offUi = QThread::currentThread() != ui;
      QElapsedTimer clock;
      clock.start();
      while (!stop.cancel_requested() && !release && clock.elapsed() < 5000) {
        std::this_thread::sleep_for(std::chrono::milliseconds(2));
      }
      geometryStopped = mitcad::geometry::cancel_requested();
      left = true;
    };
  }
  mitcad::SectionWorker::Done done() {
    return [this](const QString&) { delivered = true; };
  }
};

} // namespace

int sectionWorkerTests() {
  using mitcad::SectionWorker;
  g_lines.clear();
  g_previous = qInstallMessageHandler(collect);
  QThread* ui = QThread::currentThread();
  {
    bool reject = true;
    int attempts = 0;
    SectionWorker worker(nullptr, [&](QThread& thread) {
      ++attempts;
      if (!reject) { thread.start(); }
    });
    bool ran = false;
    bool delivered = false;
    worker.start(SectionWorker::Kind::View, QStringLiteral("unavailable"),
        [&](const mitcad::geometry::CancelSource&) { ran = true; },
        [&](const QString& error) {
          CHECK(error.contains(QStringLiteral("could not start")));
          delivered = true;
        });
    CHECK(!worker.idle());
    CHECK(worker.busyWith(SectionWorker::Kind::View));
    CHECK(waitUntil([&] { return delivered; }));
    CHECK(!ran);
    CHECK(attempts > 1);
    worker.stop();
    // A later retry can recover when native thread creation works again.
    reject = false;
    delivered = false;
    worker.start(SectionWorker::Kind::View, QStringLiteral("available"),
        [&](const mitcad::geometry::CancelSource&) { ran = true; },
        [&](const QString& error) { CHECK(error.isEmpty()); delivered = true; });
    CHECK(waitUntil([&] { return delivered; }));
    CHECK(ran && worker.idle());
  }
  for (int action = 0; action < 3; ++action) {
    SectionWorker worker(nullptr, [](QThread&) {});
    bool staleError = false;
    bool replacementError = false;
    worker.start(SectionWorker::Kind::View, QStringLiteral("failed-start"),
        [](const mitcad::geometry::CancelSource&) { CHECK(false); },
        [&](const QString&) { staleError = true; });
    CHECK(worker.busyWith(SectionWorker::Kind::View));
    if (action == 0) {
      // A document job cancels sections before accessing the document;
      // an already queued startup failure must not call its View callback.
      const auto stopped = worker.cancel();
      CHECK(stopped.view && !stopped.measure);
      CHECK(!worker.busyWith(SectionWorker::Kind::View));
    } else if (action == 1) {
      const auto stopped = worker.stop();
      CHECK(stopped.view && !stopped.measure);
      CHECK(worker.idle());
    } else {
      worker.start(SectionWorker::Kind::View, QStringLiteral("replacement-start"),
          [](const mitcad::geometry::CancelSource&) { CHECK(false); },
          [&](const QString& error) {
            CHECK(error.contains(QStringLiteral("could not start")));
            replacementError = true;
          });
      CHECK(waitUntil([&] { return replacementError; }));
    }
    QCoreApplication::processEvents(QEventLoop::AllEvents);
    CHECK(!staleError);
    CHECK(worker.idle());
  }
  {
    SectionWorker worker;
    CHECK(worker.idle());

    // A view round runs off the UI thread; a newer one stops it and only
    // the newer one's result arrives.
    Blocking first;
    Blocking second;
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), first.work(ui), first.done());
    CHECK(worker.busyWith(SectionWorker::Kind::View));
    CHECK(waitUntil([&] { return first.started.load(); }));
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), second.work(ui), second.done());
    CHECK(waitUntil([&] { return first.left.load(); }));
    CHECK(first.geometryStopped);
    CHECK(waitUntil([&] { return second.started.load(); }));
    second.release = true;
    CHECK(waitUntil([&] { return second.delivered; }));
    CHECK(!first.delivered);
    CHECK(first.offUi && second.offUi);
    CHECK(!second.geometryStopped);
    CHECK(count(QStringLiteral("^Section round view superseded$")) == 1);
    CHECK(count(QStringLiteral("^Computing section view cancelled after [0-9]+ ms$")) == 1);
    CHECK(count(QStringLiteral("^Computing section view done in [0-9]+ ms$")) == 1);
    CHECK(waitUntil([&] { return worker.idle(); }));

    // A measurement waits for the view before it, then runs; a view round
    // waiting is replaced by a newer one before it starts.
    Blocking view;
    Blocking replaced;
    Blocking newest;
    Blocking measure;
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), view.work(ui), view.done());
    CHECK(waitUntil([&] { return view.started.load(); }));
    worker.start(SectionWorker::Kind::Measure, QStringLiteral("measure"), measure.work(ui), measure.done());
    CHECK(worker.busyWith(SectionWorker::Kind::Measure));
    std::this_thread::sleep_for(std::chrono::milliseconds(30));
    QCoreApplication::processEvents();
    CHECK(!measure.started); // the view runs
    view.release = true;
    CHECK(waitUntil([&] { return view.delivered && measure.started.load(); }));
    // While the measurement runs: a view round stops it; of two view rounds
    // asked for at once, the newer one runs.
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), replaced.work(ui), replaced.done());
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), newest.work(ui), newest.done());
    CHECK(count(QStringLiteral("^Section round view replaced$")) == 1);
    CHECK(waitUntil([&] { return measure.left.load() && newest.started.load(); }));
    newest.release = true;
    CHECK(waitUntil([&] { return newest.delivered; }));
    CHECK(!replaced.started && !replaced.delivered && !measure.delivered);

    // A newer measurement supersedes the one running.
    Blocking older;
    Blocking later;
    worker.start(SectionWorker::Kind::Measure, QStringLiteral("measure"), older.work(ui), older.done());
    CHECK(waitUntil([&] { return older.started.load(); }));
    worker.start(SectionWorker::Kind::Measure, QStringLiteral("measure"), later.work(ui), later.done());
    CHECK(waitUntil([&] { return later.started.load(); }));
    later.release = true;
    CHECK(waitUntil([&] { return later.delivered; }));
    CHECK(older.left && !older.delivered);

    // A feature preview's clips (mitcad#943): a newer preview round stops
    // the running one and only its result arrives; a model job's cancel()
    // leaves it running, and a view round waits for it.
    Blocking firstPreview;
    Blocking newerPreview;
    Blocking viewAfter;
    worker.start(SectionWorker::Kind::Preview, QStringLiteral("preview"), firstPreview.work(ui), firstPreview.done());
    CHECK(worker.busyWith(SectionWorker::Kind::Preview));
    CHECK(!worker.busyWith(SectionWorker::Kind::View));
    CHECK(waitUntil([&] { return firstPreview.started.load(); }));
    worker.start(SectionWorker::Kind::Preview, QStringLiteral("preview"), newerPreview.work(ui), newerPreview.done());
    CHECK(waitUntil([&] { return firstPreview.left.load() && newerPreview.started.load(); }));
    CHECK(firstPreview.geometryStopped);
    const SectionWorker::Stopped untouched = worker.cancel();
    CHECK(!untouched.view && !untouched.measure && !untouched.preview);
    CHECK(worker.busyWith(SectionWorker::Kind::Preview));
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), viewAfter.work(ui), viewAfter.done());
    std::this_thread::sleep_for(std::chrono::milliseconds(30));
    QCoreApplication::processEvents();
    CHECK(!newerPreview.left && !viewAfter.started);
    newerPreview.release = true;
    CHECK(waitUntil([&] { return newerPreview.delivered && viewAfter.started.load(); }));
    viewAfter.release = true;
    CHECK(waitUntil([&] { return viewAfter.delivered; }));
    CHECK(!firstPreview.delivered && !newerPreview.geometryStopped);
    CHECK(count(QStringLiteral("^Section round preview superseded$")) == 1);
    CHECK(waitUntil([&] { return worker.idle(); }));
    // stop() drops a preview round waiting behind a view round.
    Blocking viewFirst;
    Blocking dropped;
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), viewFirst.work(ui), viewFirst.done());
    CHECK(waitUntil([&] { return viewFirst.started.load(); }));
    worker.start(SectionWorker::Kind::Preview, QStringLiteral("preview"), dropped.work(ui), dropped.done());
    const SectionWorker::Stopped both = worker.stop();
    CHECK(both.view && both.preview && !both.measure);
    CHECK(worker.idle() && !dropped.started);

    // stop(): the running round has ended when it returns, nothing is
    // delivered, and what was dropped is said.
    Blocking running;
    Blocking waiting;
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), running.work(ui), running.done());
    CHECK(waitUntil([&] { return running.started.load(); }));
    worker.start(SectionWorker::Kind::Measure, QStringLiteral("measure"), waiting.work(ui), waiting.done());
    const SectionWorker::Stopped stopped = worker.stop();
    CHECK(stopped.view && stopped.measure);
    CHECK(running.left);
    CHECK(worker.idle());
    for (int i = 0; i < 20; ++i) {
      QCoreApplication::processEvents(QEventLoop::AllEvents, 5);
    }
    CHECK(!running.delivered && !waiting.started && !waiting.delivered);
    const SectionWorker::Stopped none = worker.stop();
    CHECK(!none.view && !none.measure);

    // cancel(): told to stop, not waited for; it ends as cancelled, and a
    // measurement asked for meanwhile runs after it.
    Blocking told;
    Blocking after;
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), told.work(ui), told.done());
    CHECK(waitUntil([&] { return told.started.load(); }));
    const SectionWorker::Stopped cancelled = worker.cancel();
    CHECK(cancelled.view && !cancelled.measure);
    CHECK(!worker.busyWith(SectionWorker::Kind::View));
    worker.start(SectionWorker::Kind::Measure, QStringLiteral("measure"), after.work(ui), after.done());
    CHECK(waitUntil([&] { return after.started.load(); }));
    after.release = true;
    CHECK(waitUntil([&] { return after.delivered; }));
    CHECK(told.left && !told.delivered);
    CHECK(waitUntil([&] { return worker.idle(); }));

    // A round whose work throws: its error is delivered.
    QString error;
    bool failed = false;
    worker.start(
        SectionWorker::Kind::Measure, QStringLiteral("measure"),
        [](const mitcad::geometry::CancelSource&) { throw std::runtime_error("no section"); },
        [&](const QString& e) {
          failed = true;
          error = e;
        });
    CHECK(waitUntil([&] { return failed; }));
    CHECK(error == QStringLiteral("no section"));

    // The destructor stops a running round and waits for it.
    Blocking last;
    worker.start(SectionWorker::Kind::View, QStringLiteral("view"), last.work(ui), last.done());
    CHECK(waitUntil([&] { return last.started.load(); }));
  }
  CHECK(balanced());
  qInstallMessageHandler(g_previous);
  g_previous = nullptr;
  return failures;
}
