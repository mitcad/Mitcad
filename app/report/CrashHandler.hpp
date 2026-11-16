// SPDX-License-Identifier: MIT
#pragma once

// Crash capture for error reports (mitcad#62), in the application and in
// its worker processes (the import worker, mitcad --import-worker, and the
// render worker mitcad-render). Plain C++ without Qt, so that the handlers
// do nothing a crashed process cannot do: on Linux and macOS signal
// handlers (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT) that write with
// write() and backtrace_symbols_fd(); on Windows an unhandled exception
// filter that walks the stack from the fault (StackWalk64, on a thread of
// its own) and a SIGABRT handler with CaptureStackBackTrace, both naming
// the frames' functions from the PDB file next to the executable where
// there is one (dbghelp's SymFromAddr; without it the executable's frames
// keep their offset in it, never the name of a nearby exported function).
// An uncaught C++ exception (std::terminate; on Windows the unhandled
// exception 0xE06D7363, CXX_EXCEPTION, which is read before anything
// unwinds) and a Rust panic (noteMessage, before the abort that follows
// it) leave their message in the report: an exception's type and its
// what() (mitcad#773).
//
// A report is a text file `crash-<time>-<pid>.crash` in the crash folder
// (setDirectory; a worker takes it from MITCAD_CRASH_DIR, which the
// application sets for its children): a header of `key: value` lines, the
// stack (`stack:`), the recent actions (`actions:`). The application reads
// it with report::parseCrashReport (ReportText.hpp) and offers it on its
// next start, or at once for a worker of its own (ReportCenter).
//
// The process still crashes as it would have: the handler writes the
// report, puts the default action back and raises the signal again.
//
// OCCT's own handlers (geometry::catch_occt_crashes) come later and keep
// what is crashes inside OCCT's operations; the rest comes here.

#include <cstddef>

namespace mitcad::crash {

// Installs the handlers for this process: `process` names it in the report
// ("app", "import-worker", "render-worker"), `version` is Mitcad's. The
// folder comes from MITCAD_CRASH_DIR when set; the parent process's id
// from MITCAD_CRASH_PARENT (a worker's application).
void install(const char* process, const char* version);

// The folder reports go to (UTF-8); empty: none are written (the crash is
// only noted on standard error).
void setDirectory(const char* directory);

// Windows: the folder minidumps go to (UTF-8), crash-<time>-<pid>.dmp
// beside the session logs (framework/SessionLog.hpp, mitcad#1896); empty:
// none. A worker takes it from MITCAD_CRASH_DUMP_DIR, which the
// application sets for its children. Elsewhere nothing: the text report
// holds the stack, and the system keeps its own crash reports (macOS's
// DiagnosticReports, Linux's core dumps where they are set up).
void setDumpDirectory(const char* directory);

// A recent action for the report: a command's or a menu entry's id, never
// design content. The last 32 are kept.
void noteAction(const char* action);

// A message for the report of a crash that follows: a Rust panic's, an
// uncaught exception's. Several are joined.
void noteMessage(const char* message, std::size_t length);

// For tests (MITCAD_TEST_CRASH, ReportCenter.hpp): whether the variable
// names `where` ("app", "app-exception", "model-worker", "import-worker",
// "render-worker").
bool testCrashRequested(const char* where);

// For tests: crashes as an access violation would.
[[noreturn]] void crashNow();

// For tests: throws a std::range_error that nothing catches, as an OCCT
// failure escaping a Qt slot did (mitcad#773).
[[noreturn]] void throwNow();

} // namespace mitcad::crash
