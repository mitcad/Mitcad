// SPDX-License-Identifier: MIT
#pragma once

// Long operations stopped on request (P7e). A thread that computes for
// someone who may cancel names what tells it to stop (CancelScope); its
// operations then stop OCCT's algorithms inside (booleans, fillets and
// chamfers, sweeps, lofts, offsets, splits, sewing: a progress range whose
// user break asks the source) and between their own steps, and throw
// Cancelled. What a cancelled operation would have made is gone. Without
// a scope nothing stops early, and nothing changes.

#include <atomic>
#include <stdexcept>

namespace mitcad::geometry {

// What tells a thread's operations to stop. It is asked often, also from
// the worker threads of OCCT's parallel algorithms, and once it has said
// yes it must keep saying so.
class CancelSource {
public:
  virtual ~CancelSource() = default;
  virtual bool cancel_requested() const noexcept = 0;
  // The progress of an OCCT algorithm stopped by this source advanced
  // (each ten-thousandth of it; from its worker threads too): the
  // operation is not stuck, for a caller watching for hangs (the .f3d
  // import's watchdog, mitcad#82). Nothing by default.
  virtual void progressed() const noexcept {}

protected:
  CancelSource() = default;
  CancelSource(const CancelSource&) = default;
  CancelSource& operator=(const CancelSource&) = default;
};

// A cancel request of its own: a flag another thread sets.
class CancelFlag final : public CancelSource {
public:
  void cancel() noexcept { m_set.store(true); }
  bool cancel_requested() const noexcept override { return m_set.load(); }

private:
  std::atomic<bool> m_set{false};
};

// While it lives, the operations of this thread stop when `source` asks;
// the source before it applies again afterwards. The source must outlive
// it.
class CancelScope {
public:
  explicit CancelScope(const CancelSource& source) noexcept;
  ~CancelScope();
  CancelScope(const CancelScope&) = delete;
  CancelScope& operator=(const CancelScope&) = delete;

private:
  const CancelSource* m_previous;
};

// An operation stopped on request.
class Cancelled : public std::runtime_error {
public:
  Cancelled() : std::runtime_error("the operation was cancelled") {}
};

// Whether this thread's operations were asked to stop.
bool cancel_requested() noexcept;

// Throws Cancelled when this thread's operations were asked to stop.
void throw_if_cancelled();

} // namespace mitcad::geometry
