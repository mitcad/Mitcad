// SPDX-License-Identifier: MIT
#pragma once

namespace mitcad {

// The end of a process that may have given up threads still inside the
// geometry kernel: an import call the hang watchdog gave up
// (abandoned_imports, mitcad#82) or a face of a display meshing that was
// cancelled and did not end (io::abandoned_faces, mitcad#846). While any
// runs, the process ends here with `code`, without its static destructors
// (std::_Exit, standard output and error flushed): they would tear down
// OCCT's and the C++ runtime's state under those threads and crash the
// process after it ended normally, with a crash report offered at the
// next start (mitcad#62) and the session taken as one that did not end
// normally (mitcad#936). Otherwise returns `code`, for main to return.
// Called by main once the application's window, document session and
// autosave are gone (QApplication's destructor has run its post routines:
// a staged restart or update was started).
int endProcess(int code);

} // namespace mitcad
