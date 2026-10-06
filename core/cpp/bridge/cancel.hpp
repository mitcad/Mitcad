// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/cancel.rs: the geometry's long operations
// stop when a recompute's monitor is cancelled (P7e).

#include <memory>

#include "rust/cxx.h"

namespace mitcad::bridge {

// Rust's; defined in the generated mitcad_bridge/kernel/cancel.h.
struct CancelCheck;

// While it lives, the geometry's operations on the thread that made it stop
// when the check asks (geometry::CancelScope).
class CancelScope {
public:
  explicit CancelScope(rust::Box<CancelCheck> check);
  ~CancelScope();
  CancelScope(const CancelScope&) = delete;
  CancelScope& operator=(const CancelScope&) = delete;

private:
  class State;
  std::unique_ptr<State> m_state;
};

std::unique_ptr<CancelScope> enter_cancel_scope(rust::Box<CancelCheck> check);

} // namespace mitcad::bridge
