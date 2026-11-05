// SPDX-License-Identifier: MIT
// mitcad-cli import, import-f3d, import-fcstd, import-ipt, import-iam and
// import-ifc: files of other formats imported into a new or opened
// document, and the helpers mitcad-cli's other commands share with them
// (import.hpp).
#include "import.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <locale>
#include <sstream>
#include <thread>

#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

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
  rust::Box<mitcad::Document> document = mitcad::load_design(path);
  if (!store.dir.empty()) {
    document->set_result_store(store.dir, mitcad::geometry::kernel_build_id(),
                               std::filesystem::path(path).filename().string());
  }
  // Linked components follow their files, relative to the design's.
  std::string base = std::filesystem::path(path).parent_path().string();
  if (base.empty()) {
    base = ".";
  }
  document->command(R"({"cmd": "update_links", "base": )" + json_string(base) + "}");
  const auto start = std::chrono::steady_clock::now();
  const std::string recomputed(document->command(R"({"cmd": "recompute"})"));
  const std::chrono::duration<double, std::milli> recompute_ms = std::chrono::steady_clock::now() - start;
  // The drawings of an older design file whose views now keep the bodies
  // they showed (mitcad#1293), one line each.
  if (log != nullptr) {
    const QJsonArray notices =
        QJsonDocument::fromJson(QByteArray::fromStdString(recomputed)).object().value(QStringLiteral("notices")).toArray();
    for (const QJsonValue& notice : notices) {
      *log << "Drawing pinned: " << notice.toObject().value(QStringLiteral("message")).toString().toStdString() << "\n";
    }
  }
  // Slot connectors and their nuts and screws placed by the first BUILD
  // rule are placed by the current one, as the application opens them
  // (mitcad#1115). A refused upgrade leaves them as they are and the
  // design opens as loaded (mitcad#1151).
  try {
    document->command(R"({"cmd": "build_upgrade"})");
  } catch (const std::exception& e) {
    std::cerr << "warning: connectors of the first rule were not placed again: " << e.what() << "\n";
  }
  if (!store.dir.empty()) {
    const std::string persisted(document->persist_results(store.min_ms));
    if (log != nullptr) {
      *log << "Result store: restored from store: " << std::max(0LL, json_count(recomputed, "restored"))
           << ", evaluated: " << json_count(recomputed, "recomputed")
           << "; stored: " << json_count(persisted, "results") << " ("
           << json_count(persisted, "bytes") << " bytes, " << json_count(persisted, "quick")
           << " quick); recompute " << std::lround(recompute_ms.count())
           << " ms\n";
    }
  }
  return document;
}

void warn_dropped_fields(const mitcad::Document& document, const std::string& path) {
  // A design a newer MitCad wrote (mitcad#1132): a save over the file it
  // was read from drops the fields this MitCad did not read. The command
  // line asks nothing (a script must not wait for an answer): it saves and
  // says so on stderr.
  const std::string answer(
      document.query(R"({"query": "unknown_fields", "path": )" + json_string(path) + "}"));
  const QString message = QJsonDocument::fromJson(QByteArray::fromStdString(answer))
                              .object()
                              .value(QStringLiteral("message"))
                              .toString();
  if (!message.isEmpty()) {
    std::cerr << "warning: " << path << ": " << message.toStdString() << "\n";
  }
}

void save_document(const mitcad::Document& document, const std::string& path) {
  warn_dropped_fields(document, path);
  document.save_design(path, "auto");
}

int import(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--unit-mm", "--open", "--save", "--structure"}, {"--json"}, a, problem)) {
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
  // A STEP assembly as components (the default) or one base feature
  // (mitcad#1274).
  if (const std::string* structure = a.option("--structure")) {
    if (*structure != "components" && *structure != "base") {
      return usage("--structure is components or base");
    }
    command += R"(, "structure": )" + json_string(*structure);
  }
  command += "}";
  rust::Box<mitcad::Document> document =
      a.flag("--open") ? open_document(*a.option("--open")) : mitcad::new_document();
  const std::string reply(document->command(command));
  // What the import left out (empty bodies).
  const QJsonObject answer = QJsonDocument::fromJson(QByteArray::fromStdString(reply)).object();
  for (const QJsonValue warning : answer.value(QStringLiteral("warnings")).toArray()) {
    std::cerr << "warning: " << warning.toString().toStdString() << "\n";
  }
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
  // A factor on the time an item is tried from its own start (mitcad#847):
  // a debug build's checks give their slowdown.
  if (const std::string* factor = a.option("--own-start-time-scale")) {
    char* end = nullptr;
    const double scale = std::strtod(factor->c_str(), &end);
    if (end == factor->c_str() || *end != '\0' || !(scale > 0.0) || !std::isfinite(scale)) {
      return usage("--own-start-time-scale needs a factor greater than 0");
    }
    std::ostringstream text;
    text.imbue(std::locale::classic());
    text.precision(17);
    text << scale;
    add(R"("own_start_time_scale": )" + text.str());
  }
  // How far the replay's geometry may be from the file's (mitcad#224).
  if (const std::string* mm = a.option("--geometric-tolerance")) {
    char* end = nullptr;
    const double tolerance = std::strtod(mm->c_str(), &end);
    if (end == mm->c_str() || *end != '\0' || !(tolerance > 0.0) || !std::isfinite(tolerance)) {
      return usage("--geometric-tolerance needs a distance in millimetres greater than 0");
    }
    std::ostringstream text;
    text.imbue(std::locale::classic());
    text.precision(17);
    text << tolerance;
    add(R"("geometric_tolerance": )" + text.str());
  }
  // What a retry on the file's exact state does with the imported bodies
  // that match it (mitcad#283): keep (the default) or replace.
  if (const std::string* bodies = a.option("--retry-bodies")) {
    if (*bodies != "keep" && *bodies != "replace") {
      return usage("--retry-bodies needs keep or replace");
    }
    add(R"("retry_bodies": )" + json_string(*bodies));
  }
  // Which items whose result deviates from the file's stay parametric
  // with it (mitcad#379): all, none (the default) or timeline indices.
  if (const std::string* accept = a.option("--accept-deviations")) {
    bool valid = !accept->empty();
    if (*accept != "all" && *accept != "none") {
      for (std::size_t i = 0; i < accept->size(); ++i) {
        const char c = (*accept)[i];
        const bool digit = c >= '0' && c <= '9';
        const bool comma = c == ',' && i > 0 && i + 1 < accept->size() && (*accept)[i - 1] != ',';
        valid = valid && (digit || comma);
      }
    }
    if (!valid) {
      return usage("--accept-deviations needs all, none or timeline indices separated by commas");
    }
    add(R"("accept_deviations": )" + json_string(*accept));
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
  // Folders of the documents the design refers to (mitcad#196), beside
  // the one next to it; the command line never asks for missing ones.
  std::string folders;
  for (const auto& [key, value] : a.options) {
    if (key == "--references") {
      folders += (folders.empty() ? "" : ", ") + json_string(value);
    }
  }
  if (!folders.empty()) {
    add(R"("references": [)" + folders + "]");
  }
  if (a.flag("--no-references")) {
    add(R"("no_references": true)");
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
                        "--learn", "--references", "--geometric-tolerance", "--retry-bodies",
                        "--accept-deviations", "--own-start-time-scale"},
                       {"--bodies-only", "--history", "--owners", "--json", "--no-verify", "--no-fallback",
                        "--no-compare", "--no-references"},
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

// An IFC building model (mitcad#273): its spatial structure as components,
// its elements' shapes as features, bodies and occurrences.
int import_ifc(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--memory-limit", "--write-test"},
                       {"--json", "--no-openings", "--no-occurrences"}, a, problem)) {
    return usage(problem);
  }
  // The reader's test building, for the tests.
  if (const std::string* path = a.option("--write-test")) {
    std::ofstream out(*path, std::ios::binary);
    out << std::string(mitcad::ifc_test_building());
    return out.good() ? 0 : 1;
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .ifc file" : "more than one .ifc file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"text": )") + (json ? "false" : "true");
  if (a.flag("--no-openings")) {
    options += R"(, "openings": false)";
  }
  if (a.flag("--no-occurrences")) {
    options += R"(, "occurrences": false)";
  }
  if (const std::string* limit = a.option("--memory-limit")) {
    if (!is_number(*limit)) {
      return usage("--memory-limit needs a number");
    }
    options += R"(, "memory_limit": )" + *limit;
  }
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_ifc(a.positional[0], options));
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

// An .rvt project's elements, one component per level, or an .rfa
// family's metadata (mitcad#274).
int import_rvt(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--reference-ifc", "--link-folder", "--elements"},
                       {"--json", "--report-only", "--mesh-only", "--no-reference-ifc"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .rvt or .rfa file" : "more than one .rvt or .rfa file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"text": )") + (json ? "false" : "true");
  if (a.flag("--report-only")) {
    options += R"(, "report_only": true)";
  }
  if (a.flag("--mesh-only")) {
    options += R"(, "mesh_only": true)";
  }
  if (a.flag("--no-reference-ifc")) {
    options += R"(, "no_reference_ifc": true)";
  }
  if (const std::string* path = a.option("--reference-ifc")) {
    options += R"(, "reference_ifc": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--link-folder")) {
    options += R"(, "link_folder": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  // Only these elements (ids separated by commas, mitcad#385).
  if (const std::string* ids = a.option("--elements")) {
    std::string list;
    std::string id;
    for (const char c : *ids + ",") {
      if (c != ',') {
        if (c < '0' || c > '9') {
          return usage("--elements takes element ids separated by commas");
        }
        id += c;
      } else if (!id.empty()) {
        list += (list.empty() ? "" : ", ") + id;
        id.clear();
      }
    }
    options += R"(, "elements": [)" + list + "]";
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_rvt(a.positional[0], options));
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

// A SolidCraft design with its history (mitcad#450).
int import_solidcraft(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--expected", "--set"}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .solidcraft file" : "more than one .solidcraft file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"text": )") + (json ? "false" : "true");
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--expected")) {
    options += R"(, "expected": )" + json_string(*path);
  }
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
  const std::string report(document->import_solidcraft(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  const bool failed = report.find("check: FAILED") != std::string::npos ||
                      report.find("\"passed\": false") != std::string::npos;
  return failed ? 1 : 0;
}

// A .dwg drawing's 3D content: its solids and meshes as bodies, its blocks
// as components (mitcad#453).
int import_dwg(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--write-test", "--version", "--case", "--expect"},
                       {"--json", "--check"}, a, problem)) {
    return usage(problem);
  }
  // The reader's test drawing, for the tests, with a case added.
  if (const std::string* path = a.option("--write-test")) {
    const std::string* version = a.option("--version");
    const std::string* test_case = a.option("--case");
    mitcad::dwg_write_test_file(*path, version != nullptr ? *version : std::string("R2018"),
                                test_case != nullptr ? *test_case : std::string());
    return 0;
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .dwg file" : "more than one .dwg file");
  }
  // The report as JSON either way: --check reads whether its checks pass.
  const bool json = a.flag("--json");
  std::string options = R"({"text": false)";
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  // Data sheet values: name=value pairs separated by commas.
  if (const std::string* expect = a.option("--expect")) {
    std::string fields;
    std::string pair;
    for (const char c : *expect + ",") {
      if (c != ',') {
        pair += c;
        continue;
      }
      if (pair.empty()) {
        continue;
      }
      const std::size_t eq = pair.find('=');
      if (eq == std::string::npos || !is_number(pair.substr(eq + 1))) {
        return usage("--expect takes name=number pairs separated by commas");
      }
      fields += (fields.empty() ? "" : ", ") + json_string(pair.substr(0, eq)) + ": " + pair.substr(eq + 1);
      pair.clear();
    }
    options += R"(, "expect": {)" + fields + "}";
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string json_report(document->import_dwg(a.positional[0], options));
  const bool pass = mitcad::dwg_report_passes(json_report);
  // The text form of the same report, without importing again.
  const std::string report = json ? json_report : std::string(mitcad::dwg_report_text(json_report));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  // --check: the exit status says whether the report's checks pass.
  return a.flag("--check") && !pass ? 1 : 0;
}

} // namespace mitcad::cli
