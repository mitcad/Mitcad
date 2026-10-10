// SPDX-License-Identifier: MIT
#pragma once

// mitcad-cli import, import-f3d, import-fcstd and import-ipt (import.cpp),
// and what mitcad-cli's other commands share with them: the arguments, JSON
// text, and opening and saving project files. The import corpus tests run
// these commands; tools/check-all.sh runs the corpus again when this code
// changes, not when main.cpp does.

#include <ostream>
#include <string>
#include <utility>
#include <vector>

#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad::cli {

// The problem and mitcad-cli's usage on standard error; the exit status 2
// (main.cpp).
int usage(const std::string& problem);

// A JSON string literal.
std::string json_string(const std::string& text);

bool is_number(const std::string& text);

// The number after "key": in compact JSON, or -1.
long long json_count(const std::string& json, const std::string& key);

// Command line: positional arguments, `--name value` options and flags.
struct Arguments {
  std::vector<std::string> positional;
  std::vector<std::pair<std::string, std::string>> options;

  const std::string* option(const std::string& name) const {
    for (const auto& [key, value] : options) {
      if (key == name) {
        return &value;
      }
    }
    return nullptr;
  }
  bool flag(const std::string& name) const { return option(name) != nullptr; }
};

// Splits the arguments after the mode; options not in `with_value` or
// `flags` are an error.
bool parse_arguments(const std::vector<std::string>& args, const std::vector<std::string>& with_value,
                     const std::vector<std::string>& flags, Arguments& out, std::string& problem);

// The result store (P7d): with a folder, opening takes results from it
// and writes those that took at least min_ms to evaluate.
struct StoreOptions {
  std::string dir;
  double min_ms = 100.0;
};

// Opens a project file, follows its links and recomputes it; with a store,
// its line goes to `log`.
rust::Box<mitcad::Document> open_document(const std::string& path, const StoreOptions& store = {},
                                          std::ostream* log = nullptr);

// Writes the project file whole or not at all: version 3 in a project
// (P12a; the B-rep data in the project's store), else one file. Throws
// rust::Error when it cannot be written.
void save_document(const mitcad::Document& document, const std::string& path);

// `mitcad-cli import ...` (args[0] is the mode, as for the others); the exit
// status.
int import(const std::vector<std::string>& args);
int import_f3d(const std::vector<std::string>& args);
int import_fcstd(const std::vector<std::string>& args);
int import_ipt(const std::vector<std::string>& args);
int import_iam(const std::vector<std::string>& args);

} // namespace mitcad::cli
