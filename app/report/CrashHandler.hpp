// SPDX-License-Identifier: MIT
#pragma once

// Crash capture for error reports (mitcad#62), in the application and in
// its worker processes (the import worker, mitcad --import-worker, and the
// render worker mitcad-render). Plain C++ without Qt, so that the handlers
// do nothing a crashed process cannot do: on Linux and macOS signal
// handlers (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT) that write with
// write() and backtrace_symbols_fd(); on Windows an unhandled exception
// filter and a SIGABRT handler with CaptureStackBackTrace. An uncaught C++
// exception (std::terminate) and a Rust panic (noteMessage, before the
// abort that follows it) leave their message in the report.
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

// A recent action for the report: a command's or a menu entry's id, never
// design content. The last 32 are kept.
void noteAction(const char* action);

// A message for the report of a crash that follows: a Rust panic's, an
// uncaught exception's. Several are joined.
void noteMessage(const char* message, std::size_t length);

// For tests (MITCAD_TEST_CRASH, ReportCenter.hpp): whether the variable
// names `where` ("app", "model-worker", "import-worker", "render-worker").
bool testCrashRequested(const char* where);

// For tests: crashes as an access violation would.
[[noreturn]] void crashNow();

} // namespace mitcad::crash
