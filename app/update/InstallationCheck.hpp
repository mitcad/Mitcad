// SPDX-License-Identifier: MIT
#pragma once

// The start-up check for a mixed installation (mitcad#1454): mitcad.exe
// and mitcad-cli.exe compare the OCCT DLLs they loaded with the ones they
// were built with (update/BuildStamp.hpp). Before any geometry work: with
// DLLs of another build the first fillet can corrupt the heap.

#include <string>

namespace mitcad::installation {

// Empty when the loaded OCCT DLLs are this build's (always off Windows,
// and when MITCAD_SKIP_INSTALLATION_CHECK=1 is set); else what to tell the
// user (UTF-8), naming the DLLs and the programs that still run from the
// installation. `version` is this program's.
std::string mixedInstallationProblem(const std::string& version);

} // namespace mitcad::installation
