// SPDX-License-Identifier: MIT
#pragma once

// Background computation (P7, docs/architecture.md): the model computes on
// one long-lived worker thread, one job at a time, while the UI thread waits
// for it without freezing (MainWindow::runJob).

#include <chrono>
#include <condition_variable>
#include <exception>
#include <functional>
#include <mutex>
#include <stdexcept>

#include <QString>
#include <QThread>

#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad {

// A document access while a job computes the model: the UI thread may not
// use the document then.
class ModelBusy : public std::runtime_error {
public:
  ModelBusy() : std::runtime_error("the model is busy computing") {}
};

// A job that was cancelled (MainWindow::runJob): what it ran was rejected
// and changed nothing. The model's own words for it.
class ComputationCancelled : public std::runtime_error {
public:
  ComputationCancelled() : std::runtime_error("the computation was cancelled") {}
};

// What a job has: the control its documents report their progress to and
// find a cancel request in, and the stage it is in ("Reading <file>") for
// the progress dialog. Both threads use it at once.
class ModelJob {
public:
  ModelJob() : m_control(new_job_control()) {}
  ModelJob(const ModelJob&) = delete;
  ModelJob& operator=(const ModelJob&) = delete;

  const JobControl& control() const { return *m_control; }
  void setStage(const QString& stage) {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_stage = stage;
  }
  QString stage() const {
    const std::lock_guard<std::mutex> lock(m_mutex);
    return m_stage;
  }

private:
  rust::Box<JobControl> m_control;
  mutable std::mutex m_mutex;
  QString m_stage;
};

// A document reports to a job while this lives (attach_job, detach_job).
class AttachedJob {
public:
  AttachedJob(Document& document, const ModelJob& job) : m_document(document) {
    m_document.attach_job(job.control());
  }
  ~AttachedJob() { m_document.detach_job(); }
  AttachedJob(const AttachedJob&) = delete;
  AttachedJob& operator=(const AttachedJob&) = delete;

private:
  Document& m_document;
};

// The thread the model computes on: a large stack (OCCT recurses deeply) and
// OCCT's crash handlers (catch_occt_crashes), one job at a time. The job
// takes the document with it, and the UI thread does not use the document
// until the job is done; the job and what it leaves pass through the
// worker's mutex, which orders what one thread did before what the other
// does next.
class ModelWorker : public QThread {
  Q_OBJECT

public:
  explicit ModelWorker(QObject* parent = nullptr);
  // Waits for a running job (cancel it first), then ends the thread.
  ~ModelWorker() override;

  // Starts a job; the one before must have been taken.
  void post(std::function<void()> job);
  // Waits up to `timeout` for the job; true when it is done.
  bool waitFor(std::chrono::milliseconds timeout);
  // The exception the job ended with, or null; the job must be done. The
  // worker is then free for the next.
  std::exception_ptr take();

signals:
  // A job is done; sent from the worker thread.
  void jobDone();

protected:
  void run() override;

private:
  std::mutex m_mutex;
  std::condition_variable m_changed;
  std::function<void()> m_job; // posted, not yet started
  std::exception_ptr m_error;
  bool m_busy = false; // posted and not taken
  bool m_done = false;
  bool m_stop = false;
};

} // namespace mitcad
