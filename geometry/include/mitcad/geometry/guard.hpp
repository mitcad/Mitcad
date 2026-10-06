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
// as an access violation inside OCCT would.

namespace mitcad::geometry {

// Installs OCCT's handlers for the process (OSD::SetSignal without
// floating point traps; Ctrl+C keeps its default); later calls do nothing.
// On Windows the handler is per thread: call it from the thread that runs
// the operations. On Linux it is the process's: a crash on another thread
// then becomes an exception there, where it is not always caught.
void catch_occt_crashes();

} // namespace mitcad::geometry
