// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/memory.rs: the process's memory against the
// limits it runs under (mitcad#80).

#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/memory.h.
struct MemoryUse;

// The process's memory measured against the tightest of its limits: an
// address-space or data limit (Linux), a job's memory limit or the commit
// the system has left (Windows), or the memory the system has left
// (Linux: available and free swap; macOS: the physical memory).
MemoryUse memory_use();

} // namespace mitcad::bridge
