// SPDX-License-Identifier: MIT
#pragma once

// Crashes inside OCCT's algorithms (an access violation in a fillet on
// unusual geometry, say) as errors instead: OCCT's signal handlers turn
// them into Standard_Failure exceptions, which the operations report like
// any other failure. The .f3d importer tries many definitions on
// imported geometry and installs this before it starts (T1); the
// application's model worker thread installs it when it starts (P7), and
// mitcad-cli in main. For tests, the environment variable
// MITCAD_TEST_OCCT_CRASH=<operation> ("fillet") makes that operation crash
// as an access violation inside OCCT would. The application's UI thread
// installs it at start too: Section Analysis clips the bodies shown and
// measures sections there (mitcad#392).

#include <cstddef>
#include <functional>

namespace mitcad::geometry {

// Installs OCCT's handlers for the process (OSD::SetSignal without
// floating point traps; Ctrl+C keeps its default); later calls do nothing.
// On Windows the handler is per thread: call it from the thread that runs
// the operations; a fault becomes an exception only in an operation (and
// on OCCT's own threads while one runs), any other goes to the unhandled
// exception filter installed before the first call (the application's
// crash report, with the stack from the fault, mitcad#76); where that
// exception cannot unwind to the operation (a fault in a destructor, which
// passes on no exception), the thread goes back to the operation without
// unwinding, as on Linux and macOS (mitcad#1429). On Linux and
// macOS the handlers are the process's: a crash outside an operation that
// catches it (OCC_CATCH_SIGNALS, on any thread) goes to the handler
// installed before the first call (the application's crash report,
// mitcad#62), or crashes as without OCCT's.
void catch_occt_crashes();

// Runs `work`, which calls OCCT's algorithms from outside this library (the
// analysis bridge's section, clip and measurements, mitcad#392), as one of
// the library's operations named `operation`: with the handlers installed
// on the thread (catch_occt_crashes), a crash inside OCCT is the error
// "<operation>: <OCCT's message>" (std::runtime_error) instead of a crash
// report, and so is an exception of OCCT's (Standard_Failure); an
// allocation that fails is "<operation>: out of memory"; other exceptions
// pass as they are. The input check compares the Shapes `work` reads
// (input_check.hpp), and MITCAD_TEST_OCCT_CRASH=<operation> crashes it.
void run_operation(const char* operation, const std::function<void()>& work);

// How many allocations inside OCCT have failed in the process so far, on
// any thread (mitcad#132; OCCT's Standard_OutOfMemory, counted by patches
// of the port): the import's memory guard takes a moved count as low
// memory.
std::size_t failed_allocations();

// How many of them failed under this thread's calls: on this thread and in
// the jobs of OCCT's thread pool it waited for (the checker and the
// booleans run on the pool). OCCT's checker and healing catch these
// failures inside and go on, so a caller compares the count before and
// after a call to know whether memory ran out under it, not under another
// thread's call at the same time: the operations (util.hpp's run), the
// checks and the build of a body from B-rep data (brep_import.hpp) do.
std::size_t failed_allocations_in_thread();

} // namespace mitcad::geometry
