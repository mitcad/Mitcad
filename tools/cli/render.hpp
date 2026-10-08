// SPDX-License-Identifier: MIT
#pragma once

// mitcad-cli render (render.cpp; only in builds with MITCAD_RENDER).

#include <functional>
#include <string>

#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad::cli {

// Opens a project file or runs a script on a new document (main.cpp).
using OpenPart = std::function<rust::Box<mitcad::Document>(const std::string&)>;

// `mitcad-cli render ...` with the whole command line; the exit status.
int render(int argc, char* argv[], const OpenPart& open);

} // namespace mitcad::cli
