// SPDX-License-Identifier: MIT
#include "bridge/memory.hpp"

#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
// GetProcessMemoryInfo from kernel32 (K32GetProcessMemoryInfo), without
// linking psapi.
#ifndef PSAPI_VERSION
#define PSAPI_VERSION 2
#endif
#include <psapi.h>
#elif defined(__APPLE__)
#include <mach/mach.h>
#include <sys/sysctl.h>
#else
#include <sys/resource.h>
#include <unistd.h>

#if defined(__GLIBC__)
#include <malloc.h>
#endif
#endif

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <string>

#include "mitcad/geometry/guard.hpp"
#include "mitcad_bridge/memory.h"

namespace mitcad::bridge {

namespace {

// The tightest limit so far: the one the process has used the largest
// share of.
struct Tightest {
  MemoryUse out{};

  void consider(std::uint64_t used, std::uint64_t limit, const std::string& kind) {
    if (limit == 0) {
      return;
    }
    const long double share = static_cast<long double>(used) / static_cast<long double>(limit);
    const long double best =
        out.limit == 0 ? -1.0L : static_cast<long double>(out.used) / static_cast<long double>(out.limit);
    if (share > best) {
      out.used = used;
      out.limit = limit;
      out.limit_kind = rust::String(kind);
    }
  }
};

// A size in MiB, or in GiB from 1 GiB on (as memory.rs's `size`).
std::string size_text(std::uint64_t bytes) {
  const double mib = static_cast<double>(bytes) / static_cast<double>(1u << 20);
  char text[32];
  if (mib < 1024.0) {
    std::snprintf(text, sizeof text, "%.0f MiB", mib);
  } else {
    std::snprintf(text, sizeof text, "%.1f GiB", mib / 1024.0);
  }
  return text;
}

// The page cache of a cgroup (memory.stat's active_file and inactive_file).
std::uint64_t cgroup_page_cache(const std::string& dir) {
  std::ifstream stat(dir + "/memory.stat");
  std::string key;
  std::uint64_t value = 0;
  std::uint64_t cache = 0;
  while (stat >> key >> value) {
    if (key == "active_file" || key == "inactive_file") {
      cache += value;
    }
  }
  return cache;
}

#if !defined(_WIN32) && !defined(__APPLE__)
// A field of /proc/self/status or /proc/meminfo in bytes ("VmData:",
// "MemAvailable:", given in kB); 0 when it is missing.
std::uint64_t proc_field(const char* file, const std::string& name) {
  std::ifstream in(file);
  std::string key;
  std::string rest;
  while (in >> key) {
    if (key == name) {
      std::uint64_t value = 0;
      std::string unit;
      in >> value >> unit;
      return unit == "kB" ? value * 1024 : value;
    }
    std::getline(in, rest);
  }
  return 0;
}

// The soft limit of a resource in bytes; 0 without one.
std::uint64_t soft_limit(int resource) {
  rlimit limit{};
  if (getrlimit(resource, &limit) != 0 || limit.rlim_cur == RLIM_INFINITY) {
    return 0;
  }
  return static_cast<std::uint64_t>(limit.rlim_cur);
}

// The memory the C library's allocator holds free for the process to use
// again: freed memory it has not returned to the system (after a large
// operation most of it), which counts as mapped and often as resident.
// Walking the allocator's arenas takes their locks, so it is asked only
// when the process is near a limit.
std::uint64_t allocator_free() {
#if defined(__GLIBC__) && (__GLIBC__ > 2 || (__GLIBC__ == 2 && __GLIBC_MINOR__ >= 33))
  return static_cast<std::uint64_t>(mallinfo2().fordblks);
#else
  return 0;
#endif
}

// The share of a limit beyond which allocator_free is asked.
constexpr double kNearLimit = 0.6;

// `used` less the allocator's free memory when that is near the limit.
std::uint64_t in_use(std::uint64_t used, std::uint64_t limit, std::uint64_t& spare) {
  if (limit == 0 || static_cast<double>(used) < kNearLimit * static_cast<double>(limit)) {
    return used;
  }
  if (spare == UINT64_MAX) {
    spare = allocator_free();
  }
  return used > spare ? used - spare : 0;
}
#endif

} // namespace

// A cgroup v2 memory limit the process runs under (mitcad#885): beyond it
// the kernel kills a process of the cgroup rather than refuse memory (a
// job's or a service's scope, systemd's MemoryMax, a container's memory
// limit, did so to an import that measured only the memory the system had
// left). A limit above the process's own cgroup is shared with every
// process under it (mitcad#1339): in a slice of 36 GiB that other jobs
// filled, imports of a few GiB each gave items up at "33.5 GiB of 36.0 GiB"
// although they were not the ones growing. At the limit the kernel takes
// back page cache and then kills the process with the most memory, so the
// usage counts as the process's own only while it holds at least half of
// it; below that others' memory counts only up to as much as its own (its
// resident memory against half the limit), and an import that does grow
// past that share still gives items up.
CgroupMemory cgroup_pressure(std::uint64_t used, std::uint64_t limit, std::uint64_t resident, bool own_cgroup) {
  CgroupMemory out;
  if (limit == 0) {
    return out;
  }
  const std::string which = own_cgroup ? "its own cgroup's" : "a parent cgroup's";
  // (At least half: twice the resident memory is the usage or more.)
  if (resident >= used - used / 2) {
    out.used = used;
    out.limit = limit;
    out.kind = which + " memory limit, mostly the process's own memory";
    return out;
  }
  out.used = resident;
  out.limit = limit / 2;
  out.kind = "half of " + which + " memory limit of " + size_text(limit) + ", shared with other processes using " +
             size_text(used - resident);
  return out;
}

CgroupMemory cgroup_memory(const std::string& root, const std::string& proc_cgroup, std::uint64_t resident) {
  CgroupMemory out;
  std::istringstream groups(proc_cgroup);
  std::string line;
  while (std::getline(groups, line)) {
    if (line.rfind("0::", 0) != 0) {
      continue;
    }
    std::string group = line.substr(3);
    bool own = true;
    for (;;) {
      const std::string dir = root + group;
      std::string max;
      std::ifstream(dir + "/memory.max") >> max;
      if (!max.empty() && max != "max") {
        std::uint64_t current = 0;
        std::ifstream(dir + "/memory.current") >> current;
        const std::uint64_t limit = std::strtoull(max.c_str(), nullptr, 10);
        // What the cgroup uses less the page cache, which the kernel
        // reclaims under the limit (as the test runner counts it,
        // mitcad#872).
        const std::uint64_t cache = cgroup_page_cache(dir);
        const std::uint64_t used = current > cache ? current - cache : 0;
        const CgroupMemory here = cgroup_pressure(used, limit, resident, own);
        // The one with the largest share.
        const auto share = [](const CgroupMemory& m) {
          return m.limit == 0 ? -1.0L : static_cast<long double>(m.used) / static_cast<long double>(m.limit);
        };
        if (share(here) > share(out)) {
          out = here;
        }
      }
      own = false;
      const std::size_t slash = group.find_last_of('/');
      if (slash == std::string::npos || group.size() <= 1) {
        break;
      }
      group.resize(slash == 0 ? 1 : slash);
      if (group == "/") {
        break;
      }
    }
  }
  return out;
}

void release_free_memory() {
#if defined(__GLIBC__)
  // Returns the free pages of every arena, not only the top of the main
  // one as free() does by itself.
  malloc_trim(0);
#endif
}

MemoryUse memory_use() {
  Tightest tightest;
#if defined(_WIN32)
  PROCESS_MEMORY_COUNTERS_EX counters{};
  counters.cb = static_cast<DWORD>(sizeof counters);
  std::uint64_t committed = 0;
  if (GetProcessMemoryInfo(GetCurrentProcess(), reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&counters),
                           static_cast<DWORD>(sizeof counters)) != 0) {
    tightest.out.resident = static_cast<std::uint64_t>(counters.WorkingSetSize);
    committed = static_cast<std::uint64_t>(counters.PrivateUsage);
  }
  // A job's limit of the process's committed memory.
  JOBOBJECT_EXTENDED_LIMIT_INFORMATION job{};
  if (QueryInformationJobObject(nullptr, JobObjectExtendedLimitInformation, &job, static_cast<DWORD>(sizeof job),
                                nullptr) != 0 &&
      (job.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_PROCESS_MEMORY) != 0) {
    tightest.consider(committed, static_cast<std::uint64_t>(job.ProcessMemoryLimit), "job memory limit");
  }
  // The commit the system has left: allocations fail beyond it.
  MEMORYSTATUSEX status{};
  status.dwLength = static_cast<DWORD>(sizeof status);
  if (GlobalMemoryStatusEx(&status) != 0) {
    tightest.consider(committed, committed + static_cast<std::uint64_t>(status.ullAvailPageFile),
                      "memory the system can commit");
  }
#elif defined(__APPLE__)
  task_vm_info_data_t info{};
  mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
  std::uint64_t footprint = 0;
  if (task_info(mach_task_self(), TASK_VM_INFO, reinterpret_cast<task_info_t>(&info), &count) == KERN_SUCCESS) {
    tightest.out.resident = static_cast<std::uint64_t>(info.resident_size);
    footprint = static_cast<std::uint64_t>(info.phys_footprint);
  }
  std::uint64_t physical = 0;
  std::size_t size = sizeof physical;
  if (sysctlbyname("hw.memsize", &physical, &size, nullptr, 0) == 0) {
    tightest.consider(footprint, physical, "physical memory");
  }
#else
  const std::uint64_t page = static_cast<std::uint64_t>(sysconf(_SC_PAGESIZE));
  std::uint64_t pages = 0;
  std::uint64_t resident_pages = 0;
  std::ifstream("/proc/self/statm") >> pages >> resident_pages;
  const std::uint64_t resident = resident_pages * page;
  tightest.out.resident = resident;
  // Memory the allocator holds free is used again before any more is
  // mapped (asked for once, near a limit).
  std::uint64_t spare = UINT64_MAX;
  // ulimit -v: every mapping counts, reserved address space too (thread
  // stacks, the allocator's arenas).
  const std::uint64_t address_limit = soft_limit(RLIMIT_AS);
  tightest.consider(in_use(pages * page, address_limit, spare), address_limit, "address-space limit");
  const std::uint64_t data_limit = soft_limit(RLIMIT_DATA);
  tightest.consider(in_use(proc_field("/proc/self/status", "VmData:"), data_limit, spare), data_limit,
                    "data limit");
  // The memory the system has left: Linux overcommits, so beyond it a
  // process is killed rather than refused memory. Against it and a cgroup's
  // limit the resident memory counts as it is: the allocator's free memory
  // that is resident counts too (a thread's arena does not lend it to the
  // others, so the process grew past it, mitcad#885), and the memory guard
  // gives it back to the system (release_free_memory).
  const std::uint64_t left = proc_field("/proc/meminfo", "MemAvailable:") + proc_field("/proc/meminfo", "SwapFree:");
  if (left > 0) {
    tightest.consider(resident, resident + left, "memory the system has left");
  }
  std::ostringstream proc_cgroup;
  std::ifstream self_cgroup("/proc/self/cgroup");
  if (self_cgroup) {
    proc_cgroup << self_cgroup.rdbuf();
  }
  const CgroupMemory group = cgroup_memory("/sys/fs/cgroup", proc_cgroup.str(), resident);
  tightest.consider(group.used, group.limit, group.kind);
#endif
  tightest.out.failed_allocations = static_cast<std::uint64_t>(geometry::failed_allocations());
  return tightest.out;
}

} // namespace mitcad::bridge
