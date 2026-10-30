// SPDX-License-Identifier: MIT
#pragma once

// Section Analysis off the UI thread (mitcad#438). The clips of the bodies
// shown, their meshes for the view and the panel's area and length took
// seconds on a large design while the window waited; they now run as
// rounds on a thread of their own, without the document: what a round
// needs is taken from the document while it is idle (Document::prepare_clips
// and prepare_query), and the round's result is shown on the UI thread
// when it is ready. The window stays responsive meanwhile, and the view
// keeps drawing what it showed.
//
// One round runs at a time, and a newer one supersedes what is left: a
// View round (the clips for the view) stops the running View or Measure
// round and replaces the View and Measure rounds waiting; a Measure round
// (the panel's numbers) stops a running Measure round, replaces a waiting
// one, and waits for the View round before it. A Preview round (the clips
// of a feature preview's bodies, mitcad#943) stops only a running Preview
// round and replaces a waiting one; the other kinds neither stop nor
// replace it, a View round waits for one that runs, and a Measure round
// waits for both. A stopped or replaced round's result is never shown, and
// requests never pile up: at most one round runs and one of each kind
// waits. A round stops through the geometry's cancel (geometry/cancel.hpp):
// its thread's operations, OCCT's thread pool's tasks too, are told to stop
// while it runs.
//
// The rounds never read a sub-shape of a body the UI thread draws: what
// they work on is detached from the bodies when they are prepared, on the
// UI thread while the document is idle (copies that share no face, edge or
// vertex with the bodies the plane may reach, and of the others only the
// box they keep), so the view's meshes and a model job's, which write into
// the bodies' sub-shapes, may be made beside a round. A model job still
// tells the View and Measure rounds to stop (cancel()), as what they would
// show may be stale after it; a Preview round goes on: what it clips is the
// preview that asked for it, which a job does not change.
//
// Each round logs "Computing section <label> started" when it starts and
// "... done in <n> ms" or "... cancelled after <n> ms" when it ends, as
// the model's jobs do, so that the UI tests' idle wait (ui_wait_idle)
// waits for it; "Section round <label> superseded" when a newer one stops
// it, and "Section round <label> replaced" when one waiting is dropped.

#include <condition_variable>
#include <cstdint>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>

#include <QElapsedTimer>
#include <QObject>
#include <QString>

#include "WorkerThreadStart.hpp"

class QThread;

namespace mitcad {

namespace geometry {
class CancelFlag;
class CancelSource;
} // namespace geometry

// A QObject (without signals) for the queued calls that end each round on
// the UI thread.
class SectionWorker : public QObject {
public:
  enum class Kind { View, Measure, Preview };
  // Runs on the worker thread, its operations stopping when `stop` says so
  // (a CancelScope of it is entered); what it computes goes where `Done`
  // finds it. It must not use the document or anything the UI thread
  // changes.
  using Work = std::function<void(const geometry::CancelSource& stop)>;
  // Runs on the UI thread after the work, unless the round was stopped or
  // replaced; `error` is what the work threw, empty when it did not.
  using Done = std::function<void(const QString& error)>;

  // What stop() stopped or dropped.
  struct Stopped {
    bool view = false;
    bool measure = false;
    bool preview = false;
  };

  explicit SectionWorker(QObject* parent = nullptr, WorkerThreadStarter starter = {});
  // Stops the running round, waits for it and ends the thread.
  ~SectionWorker() override;
  SectionWorker(const SectionWorker&) = delete;
  SectionWorker& operator=(const SectionWorker&) = delete;

  // Starts a round as the header says: at once when none runs, else after
  // the running one, which a View round tells to stop.
  void start(Kind kind, const QString& label, Work work, Done done);
  // Stops the running round and drops those waiting, then waits until the
  // worker thread has left the running round (the destructor's). None of
  // their Done runs.
  Stopped stop();
  // stop() without the wait, of the View and Measure rounds only: the
  // running one is told to stop and ends as cancelled when its thread
  // leaves it (a plane that needs no round, a model job). Preview rounds
  // run on.
  Stopped cancel();
  // Whether a round of the kind runs (and was not told to stop) or waits.
  bool busyWith(Kind kind) const;
  bool idle() const { return !m_current && !m_waitingView && !m_waitingMeasure && !m_waitingPreview; }

private:
  struct Round {
    std::uint64_t id = 0;
    Kind kind = Kind::View;
    QString label;
    Work work;
    Done done;
    std::shared_ptr<geometry::CancelFlag> stop;
  };
  // The active round, including a startup error waiting for UI delivery.
  struct Current {
    std::uint64_t id = 0;
    Kind kind = Kind::View;
    QString label;
    Done done;
    std::shared_ptr<geometry::CancelFlag> stop;
    QElapsedTimer clock;
  };

  // The flag of a kind in what stop() or cancel() say.
  static bool& stoppedOf(Stopped& stopped, Kind kind);
  void postNext();
  void finished(std::uint64_t id, const QString& error);
  void ended(const Current& round, bool cancelled) const;
  void loop();

  // The UI thread's.
  std::uint64_t m_ids = 0;
  std::optional<Current> m_current;
  std::optional<Round> m_waitingView;
  std::optional<Round> m_waitingMeasure;
  std::optional<Round> m_waitingPreview;

  // Shared with the worker thread, under the mutex.
  std::mutex m_mutex;
  std::condition_variable m_changed;
  std::optional<Round> m_next; // handed over, not yet taken
  bool m_running = false;      // the thread runs a round's work
  bool m_quit = false;

  QThread* m_thread = nullptr;
  WorkerThreadStarter m_starter;
};

} // namespace mitcad
