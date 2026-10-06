// SPDX-License-Identifier: MIT
// The .f3d import with the timeline (T1) through the document API with OCCT.
//
//   test_f3d_import <dir>               self test; writes its files to <dir>:
//                                       a small .f3d without a design (its
//                                       bodies come in as they are), the
//                                       import_f3d command and the report
//   test_f3d_import --corpus [dir]      imports every design of every
//                                       .f3d/.f3z under dir (default
//                                       MITCAD_F3D_CORPUS, else ~/f3d-corpus)
//                                       with its timeline; exits 77 (skipped)
//                                       when it is missing
//   --every N                           only every Nth file (quick runs)
//   --max-items N                       leave out designs with more timeline
//                                       items (slow in debug builds)
//   --reports DIR                       keep each JSON report there
//   --time-limit S                      per design, then the file's bodies
//   --hang-limit S                      a design whose kernel calls do not
//                                       return for S seconds is imported
//                                       again without the item (default
//                                       600), so no design hangs the run
//   test_f3d_import --models [dir]      the reference lofts (route A): the
//                                       reference models under dir (default
//                                       MITCAD_F3D_MODELS, else
//                                       ~/f3d-models; <id>/<id>.f3d and its
//                                       dump <id>.json, kept outside the
//                                       repository) replayed and compared
//                                       with the volumes of the bodies
//                                       stored in the files; exits 77
//                                       (skipped) when they are missing
//
// The corpus run requires every import to finish without an error and
// prints the coverage: per feature type how the items came in, and how the
// final bodies agree with the bodies stored in the file. Files are named by
// position (f01, ...), as in core/f3d/CORPUS_REPORT.md.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

#include "mitcad/io/body.hpp"
#include "mitcad_bridge/kernel/exchange.h"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace {

namespace fs = std::filesystem;

constexpr int kSkipped = 77;
constexpr double kPi = 3.14159265358979323846;

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_f3d_import.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(condition) check((condition), #condition, __LINE__)

bool contains(const std::string& text, const std::string& part) { return text.find(part) != std::string::npos; }

std::string json_path(const fs::path& path) {
  std::string out = "\"";
  for (const char c : path.generic_string()) {
    if (c == '"' || c == '\\') {
      out += '\\';
    }
    out += c;
  }
  return out + "\"";
}

// The string values after each `"key": "` of a JSON text, in order.
std::vector<std::string> strings_of(const std::string& json, const std::string& key) {
  std::vector<std::string> out;
  const std::string pattern = "\"" + key + "\": \"";
  for (std::size_t at = json.find(pattern); at != std::string::npos; at = json.find(pattern, at + 1)) {
    const std::size_t start = at + pattern.size();
    out.push_back(json.substr(start, json.find('"', start) - start));
  }
  return out;
}

// The numbers after each `"key": ` of a JSON text, in order.
std::vector<double> numbers_of(const std::string& json, const std::string& key) {
  std::vector<double> out;
  const std::string pattern = "\"" + key + "\": ";
  for (std::size_t at = json.find(pattern); at != std::string::npos; at = json.find(pattern, at + 1)) {
    out.push_back(std::strtod(json.c_str() + at + pattern.size(), nullptr));
  }
  return out;
}

// A small .f3d without a design segment: its two bodies (a 10 mm cube and
// a cylinder of radius 10 mm and height 20 mm) come in as one base feature.
void test_file_without_design(const fs::path& dir) {
  const fs::path file = dir / "bodies.f3d";
  mitcad::bridge::f3d_write_test_file(file.string());
  auto document = mitcad::new_document();
  const std::string report(document->import_f3d_timeline(file.string(), "{}"));
  CHECK(contains(report, "\"items\": []"));
  const std::string bodies(document->query(R"({"query": "bodies", "properties": true})"));
  std::vector<double> volumes;
  const std::string pattern = "\"volume\":";
  for (std::size_t at = bodies.find(pattern); at != std::string::npos; at = bodies.find(pattern, at + 1)) {
    volumes.push_back(std::strtod(bodies.c_str() + at + pattern.size(), nullptr));
  }
  std::sort(volumes.begin(), volumes.end());
  CHECK(volumes.size() == 2);
  if (volumes.size() == 2) {
    CHECK(std::abs(volumes[0] - 1000.0) < 1e-6);
    CHECK(std::abs(volumes[1] - kPi * 100.0 * 20.0) < 1e-3);
  }
  // The file's bodies are compared with the replay's.
  const std::vector<double> differences = numbers_of(report, "volume_difference");
  CHECK(differences.size() == 2);
  for (const double d : differences) {
    CHECK(std::abs(d) < 1e-9);
  }
  // The import is one undo step.
  CHECK(contains(std::string(document->query(R"({"query": "document"})")), "\"undo\":\"Import bodies.f3d\""));
  document->command(R"({"cmd": "undo"})");
  CHECK(contains(std::string(document->query(R"({"query": "document"})")), "\"bodies\":0"));

  // The same as a document command, for the application.
  auto again = mitcad::new_document();
  const std::string result(again->command(R"({"cmd": "import_f3d", "path": )" + json_path(file) + "}"));
  CHECK(contains(result, "\"file\":\"bodies.f3d\""));
  CHECK(contains(result, "\"items\":0"));
  CHECK(contains(std::string(again->query(R"({"query": "document"})")), "\"bodies\":2"));
  // The text report.
  auto text_doc = mitcad::new_document();
  const std::string text(text_doc->import_f3d_timeline(file.string(), R"({"text": true})"));
  CHECK(contains(text, "0 timeline items"));
}

void set_env(const char* name, const char* value) {
#ifdef _WIN32
  _putenv_s(name, value);
#else
  setenv(name, value, 1);
#endif
}

// The watchdog: an import whose kernel call does not return (simulated
// while comparing the final bodies) runs again on a new thread, comparing
// volumes only, and its result replaces the document's.
void test_watchdog(const fs::path& dir) {
  const fs::path file = dir / "bodies.f3d";
  set_env("MITCAD_IMPORT_STALL", "compare");
  auto document = mitcad::new_document();
  const std::string report(document->import_f3d_timeline(file.string(), R"({"hang_limit": 0.5})"));
  set_env("MITCAD_IMPORT_STALL", "");
  CHECK(contains(report, "the geometry kernel did not return for 0.5 s on comparing the final bodies"));
  CHECK(contains(std::string(document->query(R"({"query": "document"})")), "\"bodies\":2"));
  CHECK(contains(std::string(document->query(R"({"query": "document"})")), "\"undo\":\"Import bodies.f3d\""));
  // A document with features is imported into directly (no watchdog).
  auto used = mitcad::new_demo_document();
  const std::string direct(used->import_f3d_timeline(file.string(), R"({"hang_limit": 0.5})"));
  CHECK(!contains(direct, "did not return"));
  CHECK(contains(std::string(used->query(R"({"query": "document"})")), "\"undo\":\"Import bodies.f3d\""));
}

void test_bad_input(const fs::path& dir) {
  auto document = mitcad::new_document();
  bool failed = false;
  try {
    document->import_f3d_timeline((dir / "missing.f3d").string(), "{}");
  } catch (const std::exception& e) {
    failed = true;
    CHECK(contains(e.what(), "missing.f3d"));
  }
  CHECK(failed);
  failed = false;
  try {
    document->import_f3d_timeline((dir / "bodies.f3d").string(), R"({"unknown": 1})");
  } catch (const std::exception& e) {
    failed = true;
    CHECK(contains(e.what(), "invalid import options"));
  }
  CHECK(failed);
}

// Corpus.

struct Coverage {
  int files = 0;
  int designs = 0;
  int failed = 0;
  int skipped = 0;
  // type -> outcome -> count
  std::map<std::string, std::map<std::string, int>> items;
  // Final bodies by relative volume difference.
  int exact = 0;       // |dV| < 1e-6
  int close = 0;       // < 1e-3
  int near = 0;        // < 1e-2
  int off = 0;         // larger
  int unmatched = 0;   // no replayed body
  // Components and occurrences (F6).
  int components = 0;
  int occurrences = 0;
  int external = 0;
  int placed = 0;  // bodies placed by occurrences (not the root's own)
  int hung = 0;    // designs whose import the watchdog ran again
  // Light bulbs (mitcad#6): sketches and construction features imported,
  // those whose light bulb the file does not tell, and those kept.
  int sketch_bulbs = 0;
  int sketch_bulbs_unknown = 0;
  int construction_bulbs = 0;
  int construction_bulbs_unknown = 0;
  int bulbs_set = 0;
  double seconds = 0.0;
};

// How many times `part` occurs in `text`.
int count_of(const std::string& text, const std::string& part) {
  int n = 0;
  for (std::size_t at = text.find(part); at != std::string::npos; at = text.find(part, at + 1)) {
    ++n;
  }
  return n;
}

std::string env(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, name) != 0 || value == nullptr) {
    return {};
  }
  std::string out(value);
  std::free(value);
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? value : "";
#endif
}

fs::path default_corpus() {
  if (const std::string dir = env("MITCAD_F3D_CORPUS"); !dir.empty()) {
    return dir;
  }
#ifdef _WIN32
  const std::string home = env("USERPROFILE");
#else
  const std::string home = env("HOME");
#endif
  return home.empty() ? fs::path() : fs::path(home) / "f3d-corpus";
}

// Imports one design of a corpus file (`design` empty: the default one)
// and adds its items and body agreement to the coverage.
void import_corpus_design(const std::string& id, const std::string& path, const std::string& design,
                          const fs::path& reports, double time_limit, double hang_limit, Coverage& c) {
  ++c.designs;
  const auto start = std::chrono::steady_clock::now();
  std::string options = "{" + std::string(R"("time_limit": )") + std::to_string(time_limit) +
                        R"(, "hang_limit": )" + std::to_string(hang_limit);
  if (!design.empty()) {
    options += R"(, "design": )" + json_path(design);
  }
  options += "}";
  auto document = mitcad::new_document();
  std::string report;
  try {
    report = std::string(document->import_f3d_timeline(path, options));
  } catch (const std::exception& e) {
    ++c.failed;
    std::printf("%s %s: import failed: %s\n", id.c_str(), design.c_str(), e.what());
    return;
  }
  const double seconds =
      std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  c.seconds += seconds;
  if (contains(report, "the geometry kernel did not return")) {
    ++c.hung;
    std::printf("%s %s: the geometry kernel hung; imported again without the item\n", id.c_str(),
                design.c_str());
  }
  if (!reports.empty()) {
    std::ofstream(reports / (id + (design.empty() ? "" : "-" + design.substr(design.find('!') + 1)) + ".json"))
        << report;
  }
  const std::vector<std::string> types = strings_of(report, "type");
  const std::vector<std::string> outcomes = strings_of(report, "outcome");
  std::map<std::string, int> counts;
  for (std::size_t i = 0; i < outcomes.size() && i < types.size(); ++i) {
    ++c.items[types[i]][outcomes[i]];
    ++counts[outcomes[i]];
  }
  const std::vector<double> volumes = numbers_of(report, "file_volume");
  const std::vector<double> differences = numbers_of(report, "volume_difference");
  for (const double d : differences) {
    const double a = std::abs(d);
    (a < 1e-6 ? c.exact : a < 1e-3 ? c.close : a < 1e-2 ? c.near : c.off) += 1;
  }
  c.unmatched += static_cast<int>(volumes.size() - std::min(volumes.size(), differences.size()));
  for (const double n : numbers_of(report, "components")) {
    c.components += static_cast<int>(n);
  }
  for (const double n : numbers_of(report, "occurrences")) {
    c.occurrences += static_cast<int>(n);
  }
  for (const double n : numbers_of(report, "external")) {
    c.external += static_cast<int>(n);
  }
  const auto sum = [&report](const char* key) {
    int total = 0;
    for (const double n : numbers_of(report, key)) {
      total += static_cast<int>(n);
    }
    return total;
  };
  c.sketch_bulbs += sum("sketch_bulbs");
  c.sketch_bulbs_unknown += sum("sketch_bulbs_unknown");
  c.construction_bulbs += sum("construction_bulbs");
  c.construction_bulbs_unknown += sum("construction_bulbs_unknown");
  c.bulbs_set += sum("bulbs_set");
  const std::string instances(document->query(R"({"query": "instances", "hidden": true})"));
  c.placed += count_of(instances, "\"occurrence\":\"") - count_of(instances, "\"occurrence\":\"\"");
  std::printf("%s%s: %zu items: %d parametric, %d partial, %d fallback, %d skipped; %zu bodies, %zu "
              "compared; %.1f s\n",
              id.c_str(), design.empty() ? "" : (" " + design).c_str(), outcomes.size(), counts["parametric"],
              counts["partial"], counts["fallback"], counts["skipped"], volumes.size(), differences.size(),
              seconds);
}

int corpus(const fs::path& dir, int every, int max_items, double time_limit, double hang_limit,
           const fs::path& reports) {
  std::error_code ec;
  if (dir.empty() || !fs::is_directory(dir, ec)) {
    std::printf("corpus not found; skipped\n");
    return kSkipped;
  }
  if (!reports.empty()) {
    fs::create_directories(reports);
  }
  std::vector<std::string> files;
  for (const auto& entry : fs::recursive_directory_iterator(dir)) {
    const auto extension = entry.path().extension().string();
    if (entry.is_regular_file() && (extension == ".f3d" || extension == ".f3z")) {
      files.push_back(entry.path().string());
    }
  }
  std::sort(files.begin(), files.end());
  Coverage c;
  for (std::size_t i = 0; i < files.size(); i += static_cast<std::size_t>(every)) {
    const std::string number = std::to_string(i + 1);
    const std::string id = (number.size() < 2 ? "f0" : "f") + number;
    ++c.files;
    // The timeline length decides whether the design is too slow here.
    auto probe = mitcad::new_document();
    std::string designs;
    try {
      designs = std::string(probe->import_f3d_timeline(files[i], R"({"no_verify": true, "no_fallback": true,
                                                                      "no_compare": true, "list": true})"));
    } catch (const std::exception& e) {
      ++c.failed;
      std::printf("%s: listing failed: %s\n", id.c_str(), e.what());
      continue;
    }
    const std::vector<std::string> labels = strings_of(designs, "label");
    const std::vector<double> lengths = numbers_of(designs, "items");
    for (std::size_t k = 0; k < labels.size(); ++k) {
      if (k < lengths.size() && lengths[k] > max_items) {
        ++c.skipped;
        std::printf("%s %s: %g items, left out (--max-items %d)\n", id.c_str(), labels[k].c_str(), lengths[k],
                    max_items);
        continue;
      }
      import_corpus_design(id, files[i], labels.size() > 1 ? labels[k] : "", reports, time_limit, hang_limit,
                           c);
    }
  }
  std::printf("\nfiles %d, designs %d (failed %d, left out %d), %.0f s\n", c.files, c.designs, c.failed, c.skipped,
              c.seconds);
  std::printf("%-28s %10s %8s %8s %8s\n", "type", "parametric", "partial", "fallback", "skipped");
  std::map<std::string, int> total;
  for (const auto& [type, outcomes] : c.items) {
    std::printf("%-28s %10d %8d %8d %8d\n", type.c_str(), outcomes.count("parametric") ? outcomes.at("parametric") : 0,
                outcomes.count("partial") ? outcomes.at("partial") : 0,
                outcomes.count("fallback") ? outcomes.at("fallback") : 0,
                outcomes.count("skipped") ? outcomes.at("skipped") : 0);
    for (const auto& [outcome, n] : outcomes) {
      total[outcome] += n;
    }
  }
  std::printf("%-28s %10d %8d %8d %8d\n", "all", total["parametric"], total["partial"], total["fallback"],
              total["skipped"]);
  std::printf("final bodies: %d exact (|dV| < 1e-6), %d within 0.1 %%, %d within 1 %%, %d off, %d not replayed\n",
              c.exact, c.close, c.near, c.off, c.unmatched);
  std::printf("components: %d made, %d occurrences placed, %d of other documents left out; %d bodies placed by "
              "occurrences\n",
              c.components, c.occurrences, c.external, c.placed);
  std::printf("light bulbs: %d sketches (%d unknown), %d construction features (%d unknown), %d kept where "
              "they differ from Mitcad's default\n",
              c.sketch_bulbs, c.sketch_bulbs_unknown, c.construction_bulbs, c.construction_bulbs_unknown,
              c.bulbs_set);
  std::printf("designs the geometry kernel hung on (imported again without the item): %d\n", c.hung);
  const int code = c.failed == 0 ? 0 : 1;
  if (c.hung > 0) {
    // Threads the kernel hung in still run: end without static destructors.
    std::fflush(nullptr);
    std::_Exit(code);
  }
  return code;
}

// The reference models (route A).

// A reference model and how Mitcad's replay compares with the body stored
// in its file: the volume within `tolerance` (relative) of the stored one,
// and, where `parametric`, the import (checked against the file's
// history) keeping its loft as a feature. The others differ by the fit of
// the stored surfaces, which Mitcad's own loft rules do not reproduce
// (commands.md, loft): their tolerances are the differences known, so
// that a change in Mitcad's lofts shows.
struct ModelCheck {
  const char* id;
  double tolerance;
  bool parametric;
};

const ModelCheck kLoftModels[] = {
    {"loft_two_squares", 1e-4, true},
    {"loft_three_circles", 1e-3, true},
    {"loft_centerline", 1e-3, true},
    {"loft_direction_0", 7e-3, false},     // +0.65 %: the stored lofts of circles are not round
    {"loft_direction_20", 1.3e-2, false},  // +1.18 %
    {"loft_direction_w05", 8e-3, false},   // +0.74 %
    {"loft_direction_w2", 5e-3, false},    // +0.48 % (+0.52 % to the file's own body)
    {"loft_point_tangent", 1e-4, true},
    {"loft_rail_circle_one", 1e-3, true},  // its bent faces integrate to about 1e-4
    {"loft_rails_circle_two", 1e-3, true},
    {"loft_rails_squares", 1e-4, true},
    {"loft_tangent_face", 1e-6, true},
    {"loft_smooth_face", 1e-6, true},
    {"loft_rail_tangent", 6e-2, false},    // +5.6 %: the stored fit of a rail with a condition
    {"loft_direction_w15", 7e-3, false},   // +0.56 %
    {"loft_direction_w3", 5e-3, true},     // +0.34 %
    {"loft_direction_squares_w2", 1e-6, true},
};

fs::path default_models() {
  if (const std::string dir = env("MITCAD_F3D_MODELS"); !dir.empty()) {
    return dir;
  }
#ifdef _WIN32
  const std::string home = env("USERPROFILE");
#else
  const std::string home = env("HOME");
#endif
  return home.empty() ? fs::path() : fs::path(home) / "f3d-models";
}

std::string read_text(const fs::path& path) {
  std::ifstream in(path, std::ios::binary);
  std::stringstream text;
  text << in.rdbuf();
  return text.str();
}

// The total volume of the document's bodies (mm3).
double body_volume(const mitcad::Document& document) {
  const std::string bodies(document.query(R"({"query": "bodies", "properties": true})"));
  double total = 0.0;
  const std::string pattern = "\"volume\":";
  for (std::size_t at = bodies.find(pattern); at != std::string::npos; at = bodies.find(pattern, at + 1)) {
    total += std::strtod(bodies.c_str() + at + pattern.size(), nullptr);
  }
  return total;
}

int models(const fs::path& dir) {
  std::error_code ec;
  if (dir.empty() || !fs::is_directory(dir, ec)) {
    std::printf("reference models not found; skipped\n");
    return kSkipped;
  }
  int failed = 0;
  std::printf("%-24s %14s %14s %9s  %s\n", "model", "stored mm3", "Mitcad mm3", "diff %", "import");
  for (const ModelCheck& model : kLoftModels) {
    const fs::path f3d = dir / model.id / (std::string(model.id) + ".f3d");
    const fs::path dump = dir / model.id / (std::string(model.id) + ".json");
    if (!fs::is_regular_file(f3d, ec) || !fs::is_regular_file(dump, ec)) {
      std::printf("%-24s missing\n", model.id);
      ++failed;
      continue;
    }
    // The stored volume: the dump's final bodies (cm3).
    const std::string text = read_text(dump);
    const std::size_t final_at = text.find("\"final\"");
    const std::vector<double> stored =
        final_at == std::string::npos ? std::vector<double>{} : numbers_of(text.substr(final_at), "volume");
    if (stored.empty()) {
      std::printf("%-24s no final volume in its dump\n", model.id);
      ++failed;
      continue;
    }
    const double expected = stored.front() * 1000.0;
    try {
      // Mitcad's own replay, then the import that checks it.
      auto replay = mitcad::new_document();
      replay->import_f3d_timeline(f3d.string(),
                                  R"({"no_verify": true, "no_fallback": true, "dump": )" + json_path(dump) + "}");
      const double volume = body_volume(*replay);
      auto checked = mitcad::new_document();
      const std::string report(checked->import_f3d_timeline(f3d.string(), R"({"dump": )" + json_path(dump) + "}"));
      const std::vector<std::string> types = strings_of(report, "type");
      const std::vector<std::string> outcomes = strings_of(report, "outcome");
      std::string outcome = "none";
      for (std::size_t i = 0; i < types.size() && i < outcomes.size(); ++i) {
        if (types[i] == "LoftFeature") {
          outcome = outcomes[i];
        }
      }
      const double difference = volume / expected - 1.0;
      const bool ok = std::abs(difference) <= model.tolerance && (!model.parametric || outcome == "parametric");
      std::printf("%-24s %14.4f %14.4f %+9.4f  %s%s\n", model.id, expected, volume, 100 * difference,
                  outcome.c_str(), ok ? "" : "  FAILED");
      failed += ok ? 0 : 1;
    } catch (const std::exception& e) {
      std::printf("%-24s import failed: %s\n", model.id, e.what());
      ++failed;
    }
  }
  return failed == 0 ? 0 : 1;
}

} // namespace

int main(int argc, char** argv) {
  mitcad::io::silence_occt_messages();
  std::vector<std::string> args(argv + 1, argv + argc);
  try {
    if (!args.empty() && args[0] == "--corpus") {
      fs::path dir;
      fs::path reports;
      int every = 1;
      int max_items = 1 << 30;
      double time_limit = 0;
      double hang_limit = 600;
      for (std::size_t i = 1; i < args.size(); ++i) {
        if (args[i] == "--every" && i + 1 < args.size()) {
          every = std::max(1, std::atoi(args[++i].c_str()));
        } else if (args[i] == "--max-items" && i + 1 < args.size()) {
          max_items = std::atoi(args[++i].c_str());
        } else if (args[i] == "--time-limit" && i + 1 < args.size()) {
          time_limit = std::atof(args[++i].c_str());
        } else if (args[i] == "--hang-limit" && i + 1 < args.size()) {
          hang_limit = std::atof(args[++i].c_str());
        } else if (args[i] == "--reports" && i + 1 < args.size()) {
          reports = args[++i];
        } else {
          dir = args[i];
        }
      }
      return corpus(dir.empty() ? default_corpus() : dir, every, max_items, time_limit, hang_limit, reports);
    }
    if (!args.empty() && args[0] == "--models") {
      return models(args.size() > 1 ? fs::path(args[1]) : default_models());
    }
    if (args.size() != 1) {
      std::fprintf(stderr, "usage: test_f3d_import <dir> | --corpus [dir] [--every N] [--max-items N] "
                           "[--time-limit S] [--hang-limit S] [--reports DIR] | --models [dir]\n");
      return 2;
    }
    const fs::path dir = args[0];
    fs::create_directories(dir);
    test_file_without_design(dir);
    test_watchdog(dir);
    test_bad_input(dir);
  } catch (const std::exception& e) {
    std::fprintf(stderr, "test_f3d_import: %s\n", e.what());
    return 2;
  }
  if (failures == 0) {
    std::printf("test_f3d_import: all checks passed\n");
  }
  return failures == 0 ? 0 : 1;
}
