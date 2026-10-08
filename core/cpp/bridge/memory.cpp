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

#include <fstream>
#include <string>
#if defined(__GLIBC__)
#include <malloc.h>
#endif
#endif

#include <cstddef>
#include <cstdint>

#include "mitcad_bridge/memory.h"

namespace mitcad::bridge {

namespace {

// The tightest limit so far: the one the process has used the largest
// share of.
struct Tightest {
  MemoryUse out{};

  void consider(std::uint64_t used, std::uint64_t limit, const char* kind) {
    if (limit == 0) {
      return;
    }
    const long double share = static_cast<long double>(used) / static_cast<long double>(limit);
    const long double best =
        out.limit == 0 ? -1.0L : static_cast<long double>(out.used) / static_cast<long double>(out.limit);
    if (share > best) {
      out.used = used;
      out.limit = limit;
      out.limit_kind = kind;
    }
  }
};

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
  // process is killed rather than refused memory.
  const std::uint64_t left = proc_field("/proc/meminfo", "MemAvailable:") + proc_field("/proc/meminfo", "SwapFree:");
  if (left > 0) {
    tightest.consider(in_use(resident, resident + left, spare), resident + left, "memory the system has left");
  }
#endif
  return tightest.out;
}

} // namespace mitcad::bridge
