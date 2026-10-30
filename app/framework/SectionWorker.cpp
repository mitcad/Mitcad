// SPDX-License-Identifier: MIT
#include "SectionWorker.hpp"

#include <exception>
#include <utility>

#include <QDebug>
#include <QMetaObject>
#include <QThread>
#include <QtLogging>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/guard.hpp"
#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

// As the model's worker: OCCT's algorithms recurse deeply on large models
// (OCCT's thread pool runs some of a round's tasks on this thread too).
constexpr unsigned kStackSize = 256u << 20;

// The worker's thread: runs the worker's loop.
class SectionThread final : public QThread {
public:
  explicit SectionThread(std::function<void()> body) : m_body(std::move(body)) {}

protected:
  void run() override { m_body(); }

private:
  std::function<void()> m_body;
};

} // namespace

SectionWorker::SectionWorker(QObject* parent, WorkerThreadStarter starter)
    : QObject(parent), m_starter(std::move(starter)) {
  m_thread = new SectionThread([this] { loop(); });
  m_thread->setObjectName(QStringLiteral("section worker"));
  m_thread->setStackSize(kStackSize);
  QString error;
  if (!startWorkerThread(*m_thread, error, m_starter)) {
    qWarning().noquote() << error;
  }
}

SectionWorker::~SectionWorker() {
  stop();
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_quit = true;
  }
  m_changed.notify_all();
  m_thread->wait();
  delete m_thread;
}

void SectionWorker::start(Kind kind, const QString& label, Work work, Done done) {
  Round round;
  round.id = ++m_ids;
  round.kind = kind;
  round.label = label;
  round.work = std::move(work);
  round.done = std::move(done);
  round.stop = std::make_shared<geometry::CancelFlag>();
  const auto replace = [](std::optional<Round>& waiting) {
    if (waiting) {
      qDebug().noquote() << QStringLiteral("Section round %1 replaced").arg(waiting->label);
      waiting.reset();
    }
  };
  if (kind == Kind::View) {
    replace(m_waitingView);
    replace(m_waitingMeasure);
    m_waitingView = std::move(round);
  } else if (kind == Kind::Measure) {
    replace(m_waitingMeasure);
    m_waitingMeasure = std::move(round);
  } else {
    replace(m_waitingPreview);
    m_waitingPreview = std::move(round);
  }
  // A View round stops a View or Measure round that runs; a Measure round
  // only the measurement before it, and waits for the view; a Preview
  // round only the preview's before it (mitcad#943).
  const bool stops = m_current && (kind == Kind::Preview
                                       ? m_current->kind == Kind::Preview
                                       : m_current->kind != Kind::Preview &&
                                             (kind == Kind::View || m_current->kind == Kind::Measure));
  if (stops && !m_current->stop->cancel_requested()) {
    m_current->stop->cancel();
    qDebug().noquote() << QStringLiteral("Section round %1 superseded").arg(m_current->label);
  }
  postNext();
}

SectionWorker::Stopped SectionWorker::stop() {
  Stopped stopped;
  if (m_waitingView) {
    stopped.view = true;
    m_waitingView.reset();
  }
  if (m_waitingMeasure) {
    stopped.measure = true;
    m_waitingMeasure.reset();
  }
  if (m_waitingPreview) {
    stopped.preview = true;
    m_waitingPreview.reset();
  }
  if (m_current) {
    stoppedOf(stopped, m_current->kind) = true;
    m_current->stop->cancel();
    {
      // Not taken yet: it never runs. Taken: until the thread leaves it.
      std::unique_lock<std::mutex> lock(m_mutex);
      m_next.reset();
      m_changed.wait(lock, [this] { return !m_running; });
    }
    ended(*m_current, true);
    m_current.reset(); // its queued end finds none
  }
  return stopped;
}

SectionWorker::Stopped SectionWorker::cancel() {
  Stopped stopped;
  if (m_waitingView) {
    stopped.view = true;
    m_waitingView.reset();
  }
  if (m_waitingMeasure) {
    stopped.measure = true;
    m_waitingMeasure.reset();
  }
  if (m_current && m_current->kind != Kind::Preview && !m_current->stop->cancel_requested()) {
    stoppedOf(stopped, m_current->kind) = true;
    m_current->stop->cancel();
    qDebug().noquote() << QStringLiteral("Section round %1 superseded").arg(m_current->label);
  }
  return stopped;
}

bool SectionWorker::busyWith(Kind kind) const {
  if (m_current && m_current->kind == kind && !m_current->stop->cancel_requested()) {
    return true; // a round told to stop no longer counts: what comes next waits for it anyway
  }
  switch (kind) {
  case Kind::View:
    return m_waitingView.has_value();
  case Kind::Measure:
    return m_waitingMeasure.has_value();
  case Kind::Preview:
    break;
  }
  return m_waitingPreview.has_value();
}

bool& SectionWorker::stoppedOf(Stopped& stopped, Kind kind) {
  switch (kind) {
  case Kind::View:
    return stopped.view;
  case Kind::Measure:
    return stopped.measure;
  case Kind::Preview:
    break;
  }
  return stopped.preview;
}

void SectionWorker::postNext() {
  if (m_current) {
    return;
  }
  // The view first, then a preview's clips, then the measurement.
  std::optional<Round>& next = m_waitingView      ? m_waitingView
                               : m_waitingPreview ? m_waitingPreview
                                                  : m_waitingMeasure;
  if (!next) {
    return;
  }
  Round round = std::move(*next);
  next.reset();
  Current current;
  current.id = round.id;
  current.kind = round.kind;
  current.label = round.label;
  current.done = std::move(round.done);
  current.stop = round.stop;
  current.clock.start();
  qDebug().noquote() << QStringLiteral("Computing section %1 started").arg(current.label);
  m_current = std::move(current);
  QString startupError;
  if (!startWorkerThread(*m_thread, startupError, m_starter)) {
    qWarning().noquote() << startupError;
    // A startup failure is still this round's result. Normal completion
    // checks suppress it if a model job cancels, stop() discards it, or a
    // newer round replaces it before the UI handles this queued call.
    const std::uint64_t id = m_current->id;
    QMetaObject::invokeMethod(this, [this, id, startupError] { finished(id, startupError); }, Qt::QueuedConnection);
    return;
  }
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_next = std::move(round);
  }
  m_changed.notify_all();
}

void SectionWorker::finished(std::uint64_t id, const QString& error) {
  if (!m_current || m_current->id != id) {
    return; // stopped already (stop() said so)
  }
  const Current round = std::move(*m_current);
  m_current.reset();
  // Superseded while it ran, or after: its result is not shown.
  const bool cancelled = round.stop->cancel_requested();
  ended(round, cancelled);
  if (!cancelled && round.done) {
    round.done(error);
  }
  postNext();
}

void SectionWorker::ended(const Current& round, bool cancelled) const {
  qDebug().noquote() << QStringLiteral("Computing section %1 %2 %3 ms")
                            .arg(round.label, cancelled ? QStringLiteral("cancelled after")
                                                        : QStringLiteral("done in"))
                            .arg(round.clock.elapsed());
}

void SectionWorker::loop() {
  mac::setThreadQuality(mac::ThreadQuality::UserInitiated);
  // On Windows OCCT's handlers are per thread; a crash inside OCCT is then
  // the round's error (mitcad#392), as on the model's worker.
  geometry::catch_occt_crashes();
  std::unique_lock<std::mutex> lock(m_mutex);
  for (;;) {
    m_changed.wait(lock, [this] { return m_quit || m_next.has_value(); });
    if (m_quit) {
      return;
    }
    Round round = std::move(*m_next);
    m_next.reset();
    m_running = true;
    lock.unlock();
    QString error;
    try {
      const mac::ProcessActivity activity("Computing sections");
      const geometry::CancelScope scope(*round.stop);
      round.work(*round.stop);
    } catch (const std::exception& failure) {
      error = QString::fromUtf8(failure.what());
    } catch (...) {
      error = QStringLiteral("the section could not be worked out");
    }
    const std::uint64_t id = round.id;
    round = Round(); // what the work holds goes here, before the UI thread goes on
    lock.lock();
    m_running = false;
    m_changed.notify_all();
    lock.unlock();
    QMetaObject::invokeMethod(this, [this, id, error] { finished(id, error); }, Qt::QueuedConnection);
    lock.lock();
  }
}

} // namespace mitcad
