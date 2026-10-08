// SPDX-License-Identifier: MIT
#include "mitcad/geometry/guard.hpp"

#include <csignal>
#include <cstdlib>
#include <mutex>
#include <new>
#include <string>
#ifndef _WIN32
#include <signal.h>
#endif

#include <OSD.hxx>
#include <Standard_ErrorHandler.hxx>

#include "util.hpp"

namespace mitcad::geometry {

#ifndef _WIN32
namespace {

// The signals OSD::SetSignal takes over (SIGINT goes back to its default).
constexpr int kOcctSignals[] = {SIGFPE, SIGHUP, SIGQUIT, SIGILL, SIGBUS, SIGSYS, SIGSEGV};
constexpr int kSignalCount = static_cast<int>(sizeof kOcctSignals / sizeof kOcctSignals[0]);
// Per signal: the handler before OCCT's (the application's crash report,
// or the default) and OCCT's.
struct sigaction g_before[kSignalCount];
struct sigaction g_occt[kSignalCount];

} // namespace

// Named (not in an anonymous namespace), so that crash reports' stacks
// show them and leave them out of a crash's duplicate key (mitcad#62).
namespace detail {

void call_signal_handler(const struct sigaction& action, int number, siginfo_t* info, void* context) {
  if ((action.sa_flags & SA_SIGINFO) != 0) {
    action.sa_sigaction(number, info, context);
  } else if (action.sa_handler == SIG_IGN) {
    return;
  } else if (action.sa_handler == SIG_DFL) {
    // What the signal does without a handler: blocked while this runs, it
    // comes again when it returns (a fault also by running the instruction
    // again).
    std::signal(number, SIG_DFL);
    std::raise(number);
  } else {
    action.sa_handler(number);
  }
}

// OCCT's handlers turn a crash into an exception only where an operation
// can catch it (OCC_CATCH_SIGNALS); elsewhere they would print "no catch
// was found" and exit. Such a crash goes to the handler that was there
// before OCCT's instead: the application's crash report, or the default.
void dispatch_occt_signal(int number, siginfo_t* info, void* context) {
  for (int i = 0; i < kSignalCount; ++i) {
    if (kOcctSignals[i] == number) {
      call_signal_handler(Standard_ErrorHandler::IsInTryBlock() ? g_occt[i] : g_before[i], number, info, context);
      return;
    }
  }
}

} // namespace detail
#endif

void catch_occt_crashes() {
#ifdef _WIN32
  // The structured exception translator is per thread.
  thread_local bool installed = false;
  if (!installed) {
    OSD::SetSignal(false);
    installed = true;
  }
#else
  static std::once_flag once;
  std::call_once(once, [] {
    for (int i = 0; i < kSignalCount; ++i) {
      sigaction(kOcctSignals[i], nullptr, &g_before[i]);
    }
    OSD::SetSignal(false);
    // OCCT's handler of Ctrl+C only arms a flag that nothing here reads
    // (OSD::ControlBreak): Ctrl+C ends the program as before.
    std::signal(SIGINT, SIG_DFL);
    for (int i = 0; i < kSignalCount; ++i) {
      sigaction(kOcctSignals[i], nullptr, &g_occt[i]);
      struct sigaction action = g_occt[i];
      action.sa_flags |= SA_SIGINFO;
      action.sa_sigaction = detail::dispatch_occt_signal;
      sigaction(kOcctSignals[i], &action, nullptr);
    }
  });
#endif
}

namespace detail {
namespace {

// The operation an environment variable names; empty without it.
std::string named_operation(const char* variable) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, variable) != 0 || value == nullptr) {
    return {};
  }
  std::string operation(value);
  std::free(value);
  return operation;
#else
  const char* value = std::getenv(variable);
  return value != nullptr ? value : "";
#endif
}

} // namespace

void fail_allocation_if_asked(const char* operation) {
  // Read at each call: a test asks for it around one import.
  if (named_operation("MITCAD_TEST_OCCT_OUT_OF_MEMORY") == operation) {
    throw std::bad_alloc();
  }
}

void crash_if_asked(const char* operation) {
  static const std::string asked = named_operation("MITCAD_TEST_OCCT_CRASH");
  if (asked.empty() || asked != operation) {
    return;
  }
#ifdef _WIN32
  // An access violation, which reaches OCCT's handlers as a real one does
  // (here through the C runtime's SIGSEGV: this code is not built with
  // /EHa, so the structured exception translator is not called).
  volatile int* volatile nowhere = nullptr;
  *nowhere = 0;
#else
  // What a segmentation fault sends: OCCT's handler makes it an exception.
  std::raise(SIGSEGV);
#endif
}

} // namespace detail
} // namespace mitcad::geometry
