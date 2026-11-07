// SPDX-License-Identifier: MIT
// A crash in code of its own inside an operation (run_operation, guard.hpp)
// is the operation's error "<operation>: <OCCT's message>" and the process
// goes on (mitcad#1429, GitHub mitcad/Mitcad#173), also where the crash is
// in a destructor. 0.50.0's fillet faulted on Windows as it released a
// handle in a destructor at the end of its rounding; OCCT's handler turned
// the fault into an exception, but a destructor cannot pass one on (it is
// noexcept, also where it is inlined) and the process ended
// (std::terminate). On Linux and macOS OCCT's handler jumps back to the
// operation without unwinding the frames between.
//   test_guard_fault body        -> "error: probe: ..." (a fault in a function's body)
//   test_guard_fault destructor  -> "error: probe: ..." (a fault in a destructor)
//   test_guard_fault destructor-thread -> the same on a thread of its own

#include <cstdio>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

#include "mitcad/geometry/guard.hpp"

namespace {

int* volatile g_nowhere = nullptr;

// Writes through a null pointer with objects alive around the write: the
// fault is at that instruction, not in a call.
int fault_in_body(int count) {
  const std::string first("first");
  std::vector<std::string> names(static_cast<std::size_t>(count), first);
  int* target = g_nowhere;
  *target = static_cast<int>(names.size());
  const std::string second("second");
  return static_cast<int>(names.size() + second.size());
}

int g_released = 0;

void count_release(int count) { g_released += count; }

// What frees the handle's object, which may throw as far as the compiler
// knows.
void (*volatile g_release)(int) = count_release;

// Writes through a null pointer when it is destroyed, as a handle released
// on freed memory does (its count decremented, then its object freed when
// none is left).
struct ReleasesNowhere {
  int count;
  ~ReleasesNowhere() {
    int* target = g_nowhere;
    *target -= 1;
    if (*target == 0) {
      g_release(count);
    }
  }
};

// Releases a handle on destruction, which may throw as far as the compiler
// knows.
struct Releases {
  int count;
  ~Releases() { g_release(count); }
};

// An algorithm's object whose members are destroyed in turn (its
// destructor inlined into the function that made it): the handle released
// on freed memory first, then another.
struct Algorithm {
  Releases other;
  ReleasesNowhere released;
};

int fault_in_destructor(int count) {
  std::vector<int> values(static_cast<std::size_t>(count), count);
  {
    const Algorithm algorithm{{count}, {count}};
    values.push_back(algorithm.other.count);
  }
  return static_cast<int>(values.size());
}

// Called through a pointer the compiler cannot see through, so that the
// fault stays in a frame of its own.
int (*volatile g_body)(int) = fault_in_body;
int (*volatile g_destructor)(int) = fault_in_destructor;

// The crashing operation and one after it on the calling thread, which has
// installed OCCT's handlers as the application's threads do.
int crash_and_go_on(bool destructor) {
  mitcad::geometry::catch_occt_crashes();
  try {
    mitcad::geometry::run_operation("probe", [destructor] {
      const std::string outer("outer");
      try {
        std::printf("not crashed: %d\n", (destructor ? g_destructor : g_body)(3));
      } catch (const std::invalid_argument&) {
        throw;
      } catch (...) {
        // As the dressups' retries do (dressup.cpp's dress): the error
        // passed on.
        throw;
      }
    });
    std::printf("no error\n");
    return 1;
  } catch (const std::runtime_error& error) {
    std::printf("error: %s\n", error.what());
  }
  // The thread goes on with operations after it.
  int done = 0;
  mitcad::geometry::run_operation("next", [&done] { done = 1; });
  std::printf("next operation: %s\n", done == 1 ? "done" : "not done");
  return done == 1 ? 0 : 1;
}

} // namespace

int main(int argc, char* argv[]) {
  const std::string mode = argc == 2 ? argv[1] : "";
  if (mode != "body" && mode != "destructor" && mode != "destructor-thread") {
    std::fprintf(stderr, "usage: test_guard_fault body|destructor|destructor-thread\n");
    return 2;
  }
  if (mode == "destructor-thread") {
    // On a thread of its own, as the application's model worker runs the
    // previews (on Windows its crashes reach the unhandled exception filter,
    // the main thread's the C runtime's signals).
    int status = 1;
    std::thread worker([&status] { status = crash_and_go_on(true); });
    worker.join();
    return status;
  }
  return crash_and_go_on(mode == "destructor");
}
