// SPDX-License-Identifier: MIT
// mitcad-cli import, import-f3d, import-fcstd and import-ipt: files of
// other formats imported into a new or opened document, and the helpers
// mitcad-cli's other commands share with them (import.hpp).
#include "import.hpp"

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <iostream>
#include <sstream>
#include <thread>

#include "mitcad/geometry/persist.hpp"

namespace mitcad::cli {

std::string json_string(const std::string& text) {
  std::string out = "\"";
  for (const char c : text) {
    switch (c) {
    case '"':
      out += "\\\"";
      break;
    case '\\':
      out += "\\\\";
      break;
    case '\n':
      out += "\\n";
      break;
    case '\r':
      out += "\\r";
      break;
    case '\t':
      out += "\\t";
      break;
    default:
      if (static_cast<unsigned char>(c) < 0x20) {
        char escaped[8];
        std::snprintf(escaped, sizeof escaped, "\\u%04x", c);
        out += escaped;
      } else {
        out += c;
      }
    }
  }
  return out + "\"";
}

bool is_number(const std::string& text) {
  if (text.empty()) {
    return false;
  }
  std::istringstream in(text);
  double value = 0.0;
  in >> value;
  return !in.fail() && in.eof();
}

long long json_count(const std::string& json, const std::string& key) {
  const std::string field = "\"" + key + "\":";
  const std::size_t at = json.find(field);
  if (at == std::string::npos) {
    return -1;
  }
  return std::strtoll(json.c_str() + at + field.size(), nullptr, 10);
}

bool parse_arguments(const std::vector<std::string>& args, const std::vector<std::string>& with_value,
                     const std::vector<std::string>& flags, Arguments& out, std::string& problem) {
  const auto listed = [](const std::vector<std::string>& list, const std::string& name) {
    for (const std::string& item : list) {
      if (item == name) {
        return true;
      }
    }
    return false;
  };
  for (std::size_t i = 1; i < args.size(); ++i) {
    const std::string& arg = args[i];
    if (arg.size() > 1 && arg[0] == '-') {
      if (listed(flags, arg)) {
        out.options.emplace_back(arg, "");
      } else if (listed(with_value, arg) && i + 1 < args.size()) {
        out.options.emplace_back(arg, args[++i]);
      } else {
        problem = "unexpected argument '" + arg + "'";
        return false;
      }
    } else {
      out.positional.push_back(arg);
    }
  }
  return true;
}

rust::Box<mitcad::Document> open_document(const std::string& path, const StoreOptions& store, std::ostream* log) {
  // A version 3 file's B-rep data comes from its project's store (P12a).
  rust::Box<mitcad::Document> document = mitcad::load_project(path);
  if (!store.dir.empty()) {
    document->set_result_store(store.dir, mitcad::geometry::kernel_build_id(),
                               std::filesystem::path(path).filename().string());
  }
  // Linked components follow their files, relative to the project's.
  std::string base = std::filesystem::path(path).parent_path().string();
  if (base.empty()) {
    base = ".";
  }
  document->command(R"({"cmd": "update_links", "base": )" + json_string(base) + "}");
  const std::string recomputed(document->command(R"({"cmd": "recompute"})"));
  if (!store.dir.empty()) {
    const std::string persisted(document->persist_results(store.min_ms));
    if (log != nullptr) {
      *log << "Result store: restored from store: " << std::max(0LL, json_count(recomputed, "restored"))
           << ", evaluated: " << json_count(recomputed, "recomputed")
           << "; stored: " << json_count(persisted, "results") << " ("
           << json_count(persisted, "bytes") << " bytes)\n";
    }
  }
  return document;
}

void save_document(const mitcad::Document& document, const std::string& path) {
  document.save_project(path, "auto");
}

int import(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--unit-mm", "--open", "--save"}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no file to import" : "more than one file to import");
  }
  std::string command = R"({"cmd": "import_file", "path": )" + json_string(a.positional[0]);
  if (const std::string* unit = a.option("--unit-mm")) {
    if (!is_number(*unit)) {
      return usage("--unit-mm needs a number");
    }
    command += R"(, "unit_mm": )" + *unit;
  }
  command += "}";
  rust::Box<mitcad::Document> document =
      a.flag("--open") ? open_document(*a.option("--open")) : mitcad::new_document();
  document->command(command);
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  std::cout << std::string(document->report(a.flag("--json")));
  return 0;
}

namespace {

// The timeline import (T1): the design replayed as Mitcad features.
int import_f3d_timeline(const Arguments& a) {
  std::string options = "{";
  const auto add = [&options](const std::string& field) {
    options += (options.size() > 1 ? ", " : "") + field;
  };
  if (const std::string* design = a.option("--design")) {
    add(R"("design": )" + json_string(*design));
  }
  if (const std::string* dump = a.option("--dump")) {
    add(R"("dump": )" + json_string(*dump));
  }
  if (a.flag("--no-verify")) {
    add(R"("no_verify": true)");
  }
  if (a.flag("--no-fallback")) {
    add(R"("no_fallback": true)");
  }
  if (a.flag("--no-compare")) {
    add(R"("no_compare": true)");
  }
  if (const std::string* seconds = a.option("--time-limit")) {
    add(R"("time_limit": )" + std::to_string(std::atof(seconds->c_str())));
  }
  if (const std::string* seconds = a.option("--hang-limit")) {
    add(R"("hang_limit": )" + std::to_string(std::atof(seconds->c_str())));
  }
  if (const std::string* path = a.option("--report")) {
    add(R"("report_path": )" + json_string(*path));
  }
  // An item's definitions evaluated in parallel (mitcad#95): as the
  // application, the logical cores up to 8.
  int threads = static_cast<int>(std::min(std::thread::hardware_concurrency(), 8u));
  if (const std::string* count = a.option("--threads")) {
    threads = std::atoi(count->c_str());
    if (threads < 1) {
      return usage("--threads needs a number of at least 1");
    }
  }
  add(R"("threads": )" + std::to_string(std::max(threads, 1)));
  if (const std::string* dir = a.option("--learn")) {
    add(R"("learn": )" + json_string(*dir));
  }
  const bool json = a.flag("--json");
  if (!json) {
    add(R"("text": true)");
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_f3d_timeline(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  return 0;
}

} // namespace

int import_f3d(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args,
                       {"--save", "--report", "--dump", "--design", "--time-limit", "--hang-limit", "--threads",
                        "--learn"},
                       {"--bodies-only", "--history", "--owners", "--json", "--no-verify", "--no-fallback",
                        "--no-compare"},
                       a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .f3d file" : "more than one .f3d file");
  }
  if (!a.flag("--bodies-only")) {
    if (a.flag("--history") || a.flag("--owners")) {
      return usage("--history and --owners go with --bodies-only");
    }
    return import_f3d_timeline(a);
  }
  const bool json = a.flag("--json");
  const auto yes_no = [&a](const char* flag) { return a.flag(flag) ? "true" : "false"; };
  const std::string options = std::string(R"({"history": )") + yes_no("--history") + R"(, "owners": )" +
                              yes_no("--owners") + R"(, "text": )" + (json ? "false" : "true") + "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string imported(document->import_f3d(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << imported << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << imported << std::string(document->report(false));
  }
  return 0;
}

// A FreeCAD document's stored bodies in its structure.
int import_fcstd(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--reference", "--set"}, {"--bodies-only", "--json"}, a,
                       problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .FCStd file" : "more than one .FCStd file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"bodies_only": )") + (a.flag("--bodies-only") ? "true" : "false") +
                        R"(, "text": )" + (json ? "false" : "true");
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--reference")) {
    options += R"(, "reference": )" + json_string(*path);
  }
  // Parameters changed after the import: --set name=expression, repeated.
  std::string changes;
  for (const auto& [key, value] : a.options) {
    if (key != "--set") {
      continue;
    }
    const std::size_t equals = value.find('=');
    if (equals == std::string::npos || equals == 0) {
      return usage("--set needs name=expression");
    }
    changes += (changes.empty() ? "" : ", ") + json_string(value.substr(0, equals)) + ": " +
               json_string(value.substr(equals + 1));
  }
  if (!changes.empty()) {
    options += R"(, "set_parameters": {)" + changes + "}";
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_fcstd(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  const bool differs = report.find("\"pass\": false") != std::string::npos ||
                       report.find("reference check failed") != std::string::npos;
  return differs ? 1 : 0;
}

// The bodies stored in an .ipt part file (mitcad#60).
int import_ipt(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args,
                       {"--save", "--report", "--reference", "--max-relative", "--dump", "--design", "--time-limit",
                        "--hang-limit"},
                       {"--deviation", "--json", "--bodies-only", "--no-verify", "--no-fallback", "--no-compare"}, a,
                       problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .ipt file" : "more than one .ipt file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"text": )") + (json ? "false" : "true");
  for (const auto& [flag, key] : {std::pair<const char*, const char*>{"--bodies-only", "bodies_only"},
                                  {"--no-verify", "no_verify"},
                                  {"--no-fallback", "no_fallback"},
                                  {"--no-compare", "no_compare"}}) {
    if (a.flag(flag)) {
      options += std::string(R"(, ")") + key + R"(": true)";
    }
  }
  if (const std::string* path = a.option("--dump")) {
    options += R"(, "dump_path": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--design")) {
    options += R"(, "design_path": )" + json_string(*path);
  }
  if (const std::string* limit = a.option("--time-limit")) {
    if (!is_number(*limit)) {
      return usage("--time-limit needs a number");
    }
    options += R"(, "time_limit": )" + *limit;
  }
  if (const std::string* limit = a.option("--hang-limit")) {
    if (!is_number(*limit)) {
      return usage("--hang-limit needs a number");
    }
    options += R"(, "hang_limit": )" + *limit;
  }
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--reference")) {
    options += R"(, "reference": )" + json_string(*path);
  }
  if (const std::string* limit = a.option("--max-relative")) {
    if (!is_number(*limit)) {
      return usage("--max-relative needs a number");
    }
    options += R"(, "max_relative": )" + *limit;
  }
  if (a.flag("--deviation")) {
    options += R"(, "deviation": true)";
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_ipt(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  const bool differs = report.find("\"pass\": false") != std::string::npos ||
                       report.find(": FAILED\n") != std::string::npos;
  return differs ? 1 : 0;
}

// An .iam assembly: its occurrences as components and occurrences, each
// part with the .ipt import (mitcad#60, stage 4).
int import_iam(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--search", "--time-limit"},
                       {"--json", "--history", "--no-verify", "--no-fallback", "--no-compare"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .iam file" : "more than one .iam file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"text": )") + (json ? "false" : "true");
  for (const auto& [flag, key] : {std::pair<const char*, const char*>{"--history", "history"},
                                  {"--no-verify", "no_verify"},
                                  {"--no-fallback", "no_fallback"},
                                  {"--no-compare", "no_compare"}}) {
    if (a.flag(flag)) {
      options += std::string(R"(, ")") + key + R"(": true)";
    }
  }
  if (const std::string* limit = a.option("--time-limit")) {
    if (!is_number(*limit)) {
      return usage("--time-limit needs a number");
    }
    options += R"(, "time_limit": )" + *limit;
  }
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  std::string search;
  for (const auto& [key, value] : a.options) {
    if (key == "--search") {
      search += (search.empty() ? "" : ", ") + json_string(value);
    }
  }
  if (!search.empty()) {
    options += R"(, "search": [)" + search + "]";
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_iam(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report;
  }
  // A placement that disagrees with the file, or an occurrence not placed.
  const bool differs = report.find("check failed") != std::string::npos ||
                       report.find("\"pass\": false") != std::string::npos;
  return differs ? 1 : 0;
}

} // namespace mitcad::cli
