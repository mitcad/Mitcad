// SPDX-License-Identifier: MIT
// Port patch 0034: on Windows, OCCT's handler of SIGSEGV, SIGFPE and SIGILL
// (SIGWntHandler, installed by OSD::SetSignal) locked its signal mutex and
// then, to raise OCCT's exception, called CallHandler, which locked it
// again on the same thread. The mutex was a std::mutex, and MSVC's runtime
// threw std::system_error ("resource deadlock would occur") from that
// second lock instead of OCCT's exception, which a caller that catches
// Standard_Failure missed. The mutex is now recursive: the nested lock
// goes through, and setting the handlers and handling a signal on another
// thread still exclude each other.
//
// Signals raised with std::raise inside an operation (run_operation,
// guard.hpp, whose catch_occt_crashes installs OCCT's handlers on the
// thread and calls them from its own, guard.cpp):
// - SIGSEGV and SIGILL: SIGWntHandler hands them to CallHandler
//   (EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION), the nested
//   lock. Without the patch the operation ends with std::system_error;
//   with it, with the operation's error "probe: <OCCT's message>".
// - SIGFPE, the boundary: the C runtime's raise passes the sub-code
//   _FPE_EXPLICITGEN, none of those SIGWntHandler hands to CallHandler, so
//   the handler throws Standard_NumericError itself under its one lock:
//   the operation's error with and without the patch.
// - SIGSEGV on a new thread after one was handled on this one: the
//   handler's exception released both locks as it unwound, so the new
//   thread's catch_occt_crashes (OSD::SetSignal, which takes the mutex)
//   goes through and its signal is its operation's error, within 30 s (a
//   mutex left locked would hang it).
// The patched code is the Windows part of OSD_signal.cxx; OCCT's handlers
// on Linux and macOS take no such lock. There the tests check the same
// outcome through those handlers and pass with and without the patch;
// they show the patch on Windows.

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <memory>
#include <stdexcept>
#include <string>
#include <system_error>
#include <thread>

#ifndef _WIN32
#include <signal.h>
#endif

#include <OSD.hxx>

#include "mitcad/geometry/guard.hpp"
#include "port_test.hpp"

namespace {

// On Linux and macOS OCCT's handler jumps out of a signal with it still
// blocked on the thread (and a new thread inherits the mask), so that a
// second one raised would never arrive. Unblocks it (nothing to do on
// Windows).
void unblock(int number) {
#ifndef _WIN32
  sigset_t set;
  sigemptyset(&set);
  sigaddset(&set, number);
  pthread_sigmask(SIG_UNBLOCK, &set, nullptr);
#else
  (void)number;
#endif
}

// The error of an operation "probe" that raises `number` on this thread.
std::string raised_in_operation(int number) {
  mitcad::geometry::catch_occt_crashes();
  unblock(number);
  std::string error;
  try {
    mitcad::geometry::run_operation("probe", [number] { std::raise(number); });
    error = "no error";
  } catch (const std::system_error& failure) {
    error = std::string("std::system_error: ") + failure.what();
  } catch (const std::runtime_error& failure) {
    error = failure.what();
  }
  if (number == SIGFPE) {
    // OCCT's handler on Linux turns the floating point traps on before it
    // jumps; catch_occt_crashes runs without them.
    OSD::SetFloatingSignal(false);
  }
  return error;
}

// The operation's error: "probe: " and OCCT's message.
bool operations_error(const std::string& error) {
  return error.rfind("probe: ", 0) == 0 && error.size() > 7;
}

// SIGSEGV raised inside an operation: the operation's error.
void test_raised_segv_is_the_operations_error() {
  const std::string error = raised_in_operation(SIGSEGV);
  std::printf("raised SIGSEGV: %s\n", error.c_str());
  CHECK(operations_error(error));
}

// SIGILL raised inside an operation: the operation's error.
void test_raised_sigill_is_the_operations_error() {
  const std::string error = raised_in_operation(SIGILL);
  std::printf("raised SIGILL: %s\n", error.c_str());
  CHECK(operations_error(error));
}

// Boundary: SIGFPE, which SIGWntHandler handles under its one lock.
void test_raised_sigfpe_is_the_operations_error() {
  const std::string error = raised_in_operation(SIGFPE);
  std::printf("raised SIGFPE: %s\n", error.c_str());
  CHECK(operations_error(error));
}

// SIGSEGV on a new thread after one was handled on this thread: the
// mutex is free again.
void test_raised_segv_on_another_thread_after_one_was_handled() {
  const std::string first = raised_in_operation(SIGSEGV);
  struct Shared {
    std::atomic<bool> done{false};
    std::string       error;
  };
  const std::shared_ptr<Shared> shared = std::make_shared<Shared>();
  std::thread                   worker([shared] {
    shared->error = raised_in_operation(SIGSEGV);
    shared->done  = true;
  });
  const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
  while (!shared->done.load() && std::chrono::steady_clock::now() < deadline) {
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  if (!shared->done.load()) {
    // Left waiting for the mutex; it holds only `shared`.
    worker.detach();
    std::printf("raised SIGSEGV on another thread: no result within 30 s\n");
    CHECK(shared->done.load());
    return;
  }
  worker.join();
  std::printf("raised SIGSEGV: %s; then on another thread: %s\n", first.c_str(), shared->error.c_str());
  CHECK(operations_error(first));
  CHECK(operations_error(shared->error));
}

} // namespace

MITCAD_PORT_TEST("0034", test_raised_segv_is_the_operations_error);
MITCAD_PORT_TEST("0034", test_raised_sigill_is_the_operations_error);
MITCAD_PORT_TEST("0034", test_raised_sigfpe_is_the_operations_error);
MITCAD_PORT_TEST("0034", test_raised_segv_on_another_thread_after_one_was_handled);
