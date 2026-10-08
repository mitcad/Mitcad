// SPDX-License-Identifier: MIT
#pragma once

// mitcad-cli library and mitcad-cli parts (library.cpp; mitcad#64,
// mitcad#63): component libraries in git repositories and the parts list
// of a design.

#include <functional>
#include <string>
#include <vector>

#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad::cli {

// The lines of mitcad-cli's usage for these commands.
extern const char* const kLibraryUsage;

// `mitcad-cli library <what> ...` (args[0] is "library"); the exit status.
int library(const std::vector<std::string>& args);

// Opens a project file, following its links (main.cpp).
using OpenDocument = std::function<rust::Box<mitcad::Document>(const std::string&)>;

// `mitcad-cli parts <file.mitcad> [--json]`: the parts list.
int parts(const std::vector<std::string>& args, const OpenDocument& open);

} // namespace mitcad::cli
