// SPDX-License-Identifier: MIT
#include "ModelWorker.hpp"

#include <utility>

#include "mitcad/geometry/guard.hpp"

namespace mitcad {
namespace {

// OCCT's algorithms recurse deeply on large models; the .f3d import's
// threads have as much (core/ffi/src/f3d_import.rs).
constexpr unsigned kStackSize = 256u << 20;

} // namespace

ModelWorker::ModelWorker(QObject* parent) : QThread(parent) {
  setObjectName(QStringLiteral("model worker"));
  setStackSize(kStackSize);
}

ModelWorker::~ModelWorker() {
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_stop = true;
  }
  m_changed.notify_all();
  wait();
}

void ModelWorker::post(std::function<void()> job) {
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    Q_ASSERT(!m_busy);
    m_job = std::move(job);
    m_error = nullptr;
    m_busy = true;
    m_done = false;
  }
  m_changed.notify_all();
}

bool ModelWorker::waitFor(std::chrono::milliseconds timeout) {
  std::unique_lock<std::mutex> lock(m_mutex);
  return m_changed.wait_for(lock, timeout, [this] { return m_done; });
}

std::exception_ptr ModelWorker::take() {
  std::unique_lock<std::mutex> lock(m_mutex);
  m_changed.wait(lock, [this] { return m_done; });
  m_busy = false;
  m_done = false;
  return std::exchange(m_error, nullptr);
}

void ModelWorker::run() {
  // On Windows OCCT's handlers are per thread; on Linux they are the
  // process's, so a crash in OCCT on the UI thread becomes an exception too.
  geometry::catch_occt_crashes();
  std::unique_lock<std::mutex> lock(m_mutex);
  for (;;) {
    m_changed.wait(lock, [this] { return m_stop || m_job; });
    if (!m_job) {
      return; // stopped
    }
    std::function<void()> job = std::exchange(m_job, nullptr);
    lock.unlock();
    std::exception_ptr error;
    try {
      job();
    } catch (...) {
      error = std::current_exception();
    }
    job = nullptr; // what it holds goes here, before the UI thread goes on
    lock.lock();
    m_error = error;
    m_done = true;
    m_changed.notify_all();
    lock.unlock();
    emit jobDone();
    lock.lock();
  }
}

} // namespace mitcad
