// SPDX-License-Identifier: MIT
#include "mitcad/geometry/guard.hpp"

#include <atomic>
#include <csignal>
#include <cstdlib>
#include <mutex>
#include <new>
#include <string>
#ifndef _WIN32
#include <signal.h>
#endif

#include <NCollection_Array1.hxx>
#include <OSD.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>

#include "util.hpp"

#ifdef _WIN32
#include <windows.h>
#endif

namespace mitcad::geometry {

#ifdef _WIN32
namespace {

// The signals OSD::SetSignal sets the C runtime's handlers of (for the
// calling thread). The runtime calls them from the exception filter of the
// threads it starts.
constexpr int kOcctSignals[] = {SIGSEGV, SIGILL, SIGFPE};
constexpr int kSignalCount = static_cast<int>(sizeof kOcctSignals / sizeof kOcctSignals[0]);
using SignalHandler = void (*)(int, int); // SIGFPE's with its subcode
SignalHandler g_occtSignal[kSignalCount];
// The unhandled exception filters: the one before OCCT's (the
// application's crash report, or the C runtime's) and OCCT's.
LPTOP_LEVEL_EXCEPTION_FILTER g_before = nullptr;
LPTOP_LEVEL_EXCEPTION_FILTER g_occt = nullptr;
// The operations running (run() in util.hpp): on this thread, in the
// process.
thread_local int t_operations = 0;
std::atomic<int> g_operations{0};
// This thread has OCCT's handlers (catch_occt_crashes).
thread_local bool t_installed = false;

} // namespace

namespace detail {

OperationScope::OperationScope() {
  ++t_operations;
  ++g_operations;
}

OperationScope::~OperationScope() {
  --t_operations;
  --g_operations;
}

void dispatch_occt_signal(int number, int code);

// OCCT's handlers turn a fault into an exception, thrown from the handler,
// that an operation catches as its failure: on a thread running one, and
// on OCCT's own threads (OSD_ThreadPool, which never install the handlers)
// while one runs. Any other fault goes to the handler that was there
// before OCCT's: an exception that nothing would catch ends the process
// without it (Windows does not filter an exception thrown from the
// unhandled exception filter again), and the crash report's stack starts
// at the fault (mitcad#76).
bool occt_converts() { return t_operations > 0 || (!t_installed && g_operations.load() > 0); }

LONG WINAPI dispatch_occt_exception(EXCEPTION_POINTERS* info) {
  if (g_occt != nullptr && occt_converts()) {
    g_occt(info); // throws for the faults it turns into exceptions
  }
  return g_before != nullptr ? g_before(info) : EXCEPTION_CONTINUE_SEARCH;
}

// Puts the dispatch's signal handler back after OCCT's has run (it puts
// itself back), also when it throws.
class SignalBack {
public:
  explicit SignalBack(int number) : m_number(number) {}
  ~SignalBack() { std::signal(m_number, reinterpret_cast<void (*)(int)>(dispatch_occt_signal)); }
  SignalBack(const SignalBack&) = delete;
  SignalBack& operator=(const SignalBack&) = delete;

private:
  int m_number;
};

// The C runtime's signal for a fault on a thread it started; it has put
// the default handler back for it. Outside an operation the fault comes
// again when this returns (the instruction runs again), with no handler:
// the unhandled exception filter, the crash report, gets it.
void dispatch_occt_signal(int number, int code) {
  if (!occt_converts()) {
    return;
  }
  for (int i = 0; i < kSignalCount; ++i) {
    if (kOcctSignals[i] == number && g_occtSignal[i] != nullptr) {
      const SignalBack back(number);
      g_occtSignal[i](number, code);
    }
  }
}

} // namespace detail
#else
namespace detail {

OperationScope::OperationScope() = default;
OperationScope::~OperationScope() = default;

} // namespace detail
#endif

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
  // The structured exception translator and the C runtime's signal
  // handlers are per thread; OCCT's unhandled exception filter takes over
  // the process's at each call.
  static std::once_flag once;
  std::call_once(once, [] {
    g_before = SetUnhandledExceptionFilter(nullptr);
    SetUnhandledExceptionFilter(g_before);
  });
  if (!t_installed) {
    OSD::SetSignal(false);
    const LPTOP_LEVEL_EXCEPTION_FILTER occt = SetUnhandledExceptionFilter(detail::dispatch_occt_exception);
    if (occt != detail::dispatch_occt_exception) {
      g_occt = occt;
    }
    for (int i = 0; i < kSignalCount; ++i) {
      const auto previous = reinterpret_cast<SignalHandler>(
          std::signal(kOcctSignals[i], reinterpret_cast<void (*)(int)>(detail::dispatch_occt_signal)));
      if (previous != detail::dispatch_occt_signal && previous != reinterpret_cast<SignalHandler>(SIG_DFL) &&
          previous != reinterpret_cast<SignalHandler>(SIG_ERR)) {
        g_occtSignal[i] = previous;
      }
    }
    t_installed = true;
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

std::size_t failed_allocations() { return Standard_OutOfMemory::NbRaised(); }

std::size_t failed_allocations_in_thread() { return Standard_OutOfMemory::NbRaisedInThread(); }

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

void fail_occt_allocation_if_asked(const char* operation) {
  if (named_operation("MITCAD_TEST_OCCT_ALLOCATION_FAILS") != operation) {
    return;
  }
  // An array larger than any address space, which OCCT's collections
  // allocate as the checker's polygon of a wire (CSLib_Class2d) does: the
  // allocation fails inside OCCT as one under a memory limit does. The
  // failure is caught here as the checker and the healing catch theirs and
  // go on: only the count of failed allocations tells.
  struct Megabyte {
    double values[1 << 17];
  };
  try {
    NCollection_Array1<Megabyte> huge;
    huge.Resize(1, 1 << 30, false);
    huge.ChangeFirst().values[0] = 1.0;
  } catch (const Standard_Failure&) {
  }
}

void throw_if_allocation_failed(std::size_t before) {
  if (failed_allocations_in_thread() != before) {
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
