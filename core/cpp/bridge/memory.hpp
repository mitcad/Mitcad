// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/memory.rs: the process's memory against the
// limits it runs under (mitcad#80).

#include <cstdint>
#include <string>

#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/memory.h.
struct MemoryUse;

// The process's memory measured against the tightest of its limits: an
// address-space or data limit (Linux), a job's memory limit or the commit
// the system has left (Windows), or the memory the system has left
// (Linux: available and free swap; macOS: the physical memory), or a
// cgroup v2 memory limit (Linux, as cgroup_memory counts it).
MemoryUse memory_use();

// Gives the memory the C library's allocator holds free back to the system
// (glibc: the free pages of every arena; elsewhere nothing), mitcad#885.
void release_free_memory();

// A cgroup's memory limit as the memory guard counts it (mitcad#885,
// mitcad#1339): `used` against `limit` in bytes (limit 0: none), and whose
// memory it is (`kind`, for the report).
struct CgroupMemory {
  std::uint64_t used = 0;
  std::uint64_t limit = 0;
  std::string kind;
};

// The decision for one cgroup with a limit (`limit`), what it uses less its
// page cache (`used`) and the process's resident memory (`resident`);
// `own_cgroup` when it is the process's own cgroup rather than one above it.
// While the process holds at least half of the usage, the usage counts
// against the limit (its own memory); otherwise others' memory counts only
// up to as much as the process's own: its resident memory against half the
// limit (mitcad#1339: a slice full of other jobs made imports give items
// up). Pure, on every platform.
CgroupMemory cgroup_pressure(std::uint64_t used, std::uint64_t limit, std::uint64_t resident, bool own_cgroup);

// The tightest cgroup limit (the largest share, cgroup_pressure) of the
// process's cgroup v2 and those above it: `proc_cgroup` is the text of
// /proc/self/cgroup (its `0::<path>` line), `root` where the hierarchy is
// mounted (/sys/fs/cgroup), each cgroup's memory.max, memory.current and
// memory.stat read under it. Reads only the files it names, so a test can
// give it a tree of its own on any platform; memory_use calls it on Linux.
CgroupMemory cgroup_memory(const std::string& root, const std::string& proc_cgroup, std::uint64_t resident);

} // namespace mitcad::bridge
