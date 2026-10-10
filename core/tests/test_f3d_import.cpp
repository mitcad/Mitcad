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
//   --threads N                         threads an import uses at once:
//                                       an item's definitions, reading the
//                                       bodies, the history's bodies built
//                                       ahead (default 1, mitcad#95,
//                                       mitcad#103); the reports are the
//                                       same, only the times differ
//   test_f3d_import --models [dir]      the reference lofts and sweeps (route
//                                       A): the reference models under dir
//                                       (default MITCAD_F3D_MODELS, else
//                                       ~/f3d-models; <id>/<id>.f3d and its
//                                       dump <id>.json, kept outside the
//                                       repository) replayed and compared
//                                       with the volumes of the bodies
//                                       stored in the files, and imported
//                                       from the files' own streams
//                                       (mitcad#34); exits 77 (skipped)
//                                       when they are missing
//   --jobs N, --memory SIZE,            corpus and models: each file or
//   --file-timeout S                    model in a child process of its
//                                       own, N at a time
//                                       (parallel_runs.hpp; --jobs 1: all
//                                       in this process)
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
#include <stdexcept>
#include <string>
#include <vector>

#ifdef __linux__
#include <sys/resource.h>
#include <unistd.h>
#endif

#include "mitcad/io/body.hpp"
#include "mitcad_bridge/kernel/exchange.h"
#include "mitcad_bridge/lib.h"
#include "parallel_runs.hpp"
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
  // The try given up still runs (it never returns): a program ending now
  // ends without its static destructors (mitcad#82).
  CHECK(mitcad::abandoned_imports() >= 1);
  // A document with features is imported into directly (no watchdog).
  auto used = mitcad::new_demo_document();
  const std::string direct(used->import_f3d_timeline(file.string(), R"({"hang_limit": 0.5})"));
  CHECK(!contains(direct, "did not return"));
  CHECK(contains(std::string(used->query(R"({"query": "document"})")), "\"undo\":\"Import bodies.f3d\""));
}

// A build of a stored body longer than the hang limit that returns
// (mitcad#82: healing one body of a large design took 74 s in one call of
// OCCT's, over a limit of 60 s). MITCAD_IMPORT_STALL=slow makes every build
// of the file's first body take 1.5 s without progress: the first try is
// given up while it builds it, comparing the final bodies, and the next
// one waits for that build and reads the body from it instead of building
// it again (which would take as long and hang too), so the import
// finishes.
void test_watchdog_slow_build(const fs::path& dir) {
  const fs::path file = dir / "bodies.f3d";
  set_env("MITCAD_IMPORT_STALL", "slow");
  auto document = mitcad::new_document();
  std::string report;
  std::string error;
  try {
    report = std::string(document->import_f3d_timeline(file.string(), R"({"hang_limit": 1.0})"));
  } catch (const std::exception& e) {
    error = e.what();
  }
  set_env("MITCAD_IMPORT_STALL", "");
  if (!error.empty()) {
    std::fprintf(stderr, "test_f3d_import: import with a slow build: %s\n", error.c_str());
  }
  CHECK(error.empty());
  CHECK(contains(report, "the geometry kernel did not return for 1 s on comparing the final bodies"));
  CHECK(contains(std::string(document->query(R"({"query": "document"})")), "\"bodies\":2"));
  std::vector<double> volumes = numbers_of(report, "file_volume");
  std::sort(volumes.begin(), volumes.end());
  CHECK(volumes.size() == 2);
  if (volumes.size() == 2) {
    CHECK(std::abs(volumes[0] - 1000.0) < 1e-6);
    CHECK(std::abs(volumes[1] - kPi * 100.0 * 20.0) < 1e-3);
  }
}

// The watchdog under an address-space limit (mitcad#58): the abandoned
// thread keeps its stack, and the limit leaves room for only one more
// import thread of the full stack size. The try after the hang still
// starts (on a smaller stack) and the import finishes. Linux only: the
// limit is RLIMIT_AS over the process's present size (/proc/self/statm).
void test_watchdog_memory_limit(const fs::path& dir) {
#ifdef __linux__
  long pages = 0;
  std::ifstream("/proc/self/statm") >> pages;
  rlimit old{};
  if (pages <= 0 || getrlimit(RLIMIT_AS, &old) != 0) {
    std::printf("test_f3d_import: no address-space limit here, watchdog memory test skipped\n");
    return;
  }
  const rlim_t size = static_cast<rlim_t>(pages) * static_cast<rlim_t>(sysconf(_SC_PAGESIZE));
  // One 256 MiB import stack, its allocator arena and some working memory.
  rlimit tight = old;
  tight.rlim_cur = size + (400ull << 20);
  if (old.rlim_cur != RLIM_INFINITY && old.rlim_cur < tight.rlim_cur) {
    tight.rlim_cur = old.rlim_cur;
  }
  const fs::path file = dir / "bodies.f3d";
  set_env("MITCAD_IMPORT_STALL", "compare");
  auto document = mitcad::new_document();
  std::string report;
  std::string error;
  setrlimit(RLIMIT_AS, &tight);
  try {
    report = std::string(document->import_f3d_timeline(file.string(), R"({"hang_limit": 0.5})"));
  } catch (const std::exception& e) {
    error = e.what();
  }
  setrlimit(RLIMIT_AS, &old);
  set_env("MITCAD_IMPORT_STALL", "");
  if (!error.empty()) {
    std::fprintf(stderr, "test_f3d_import: import under a memory limit: %s\n", error.c_str());
  }
  CHECK(error.empty());
  CHECK(contains(report, "the geometry kernel did not return for 0.5 s on comparing the final bodies"));
  CHECK(contains(report, "MiB stack instead of 256 MiB"));
  CHECK(contains(std::string(document->query(R"({"query": "document"})")), "\"bodies\":2"));
#else
  (void)dir;
#endif
}

// A small design as an external dump for bodies.f3d (cm in the IR): a
// 40 x 20 mm rectangle on XY extruded 10 mm into a new body.
const char kBlockDump[] = R"({
  "schema": "mitcad-f3d-dump", "schema_version": 2,
  "source": {"mode": "f3d_stream", "file": "bodies.f3d"},
  "parameters": {"model": [
    {"name": "d2", "expression": "10 mm", "value": 1.0, "unit": "mm"},
    {"name": "d3", "expression": "0.0 deg", "value": 0.0, "unit": "deg"}]},
  "timeline": {"items": [
    {"index": 0, "name": "Sketch1", "objectType": "Sketch", "detail": {
      "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
      "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                          [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
      "points": [{"id": "p0", "xyz": [0.0, 0.0, 0.0]}, {"id": "p1", "xyz": [4.0, 0.0, 0.0]},
                 {"id": "p2", "xyz": [4.0, 2.0, 0.0]}, {"id": "p3", "xyz": [0.0, 2.0, 0.0]}],
      "curves": [
        {"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1"},
        {"id": "c1", "type": "SketchLine", "startSketchPoint": "p1", "endSketchPoint": "p2"},
        {"id": "c2", "type": "SketchLine", "startSketchPoint": "p2", "endSketchPoint": "p3"},
        {"id": "c3", "type": "SketchLine", "startSketchPoint": "p3", "endSketchPoint": "p0"}]}},
    {"index": 1, "name": "Extrude1", "objectType": "ExtrudeFeature", "detail": {
      "operation": "NewBodyFeatureOperation",
      "profile": [{"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0}],
      "extentType": "OneSideFeatureExtentType",
      "extentOne": {"_type": "DistanceExtentDefinition", "distance":
        {"kind": "parameter", "name": "d2", "expression": "10 mm", "value": 1.0, "unit": "mm"}},
      "taperAngleOne":
        {"kind": "parameter", "name": "d3", "expression": "0.0 deg", "value": 0.0, "unit": "deg"}},
     "_f3d": {"extrude": {"operation_code": 4, "direction": 1.0}}}]}
})";

// Memory (mitcad#80), with the watchdog and without: an allocation that
// fails in an item's kernel operation (MITCAD_TEST_OCCT_OUT_OF_MEMORY) does
// not end the process; the item takes the file's bodies, saying why, the
// report warns and the import finishes. With a memory limit the process is
// over from the start, the import's memory guard gives every modelling
// item up and the stored bodies come in, compared by volume only.
void test_out_of_memory(const fs::path& dir) {
  const fs::path file = dir / "bodies.f3d";
  const fs::path dump = dir / "block-dump.json";
  std::ofstream(dump) << kBlockDump;
  for (const char* watched : {R"(, "hang_limit": 60)", ""}) {
    set_env("MITCAD_TEST_OCCT_OUT_OF_MEMORY", "extrude");
    auto document = mitcad::new_document();
    std::string report;
    std::string error;
    try {
      report = std::string(
          document->import_f3d_timeline(file.string(), R"({"dump": )" + json_path(dump) + watched + "}"));
    } catch (const std::exception& e) {
      error = e.what();
    }
    set_env("MITCAD_TEST_OCCT_OUT_OF_MEMORY", "");
    CHECK(error.empty());
    CHECK(contains(report, "the import ran low on memory at Extrude1 (a geometry kernel operation ran out "
                           "of memory"));
    CHECK(contains(report, "extrude: out of memory"));
    CHECK(contains(report, R"("note": "the import ran low on memory)"));
    CHECK(contains(report, R"("low_memory": {)"));
    CHECK(contains(std::string(document->query(R"({"query": "document"})")), "\"bodies\":2"));

    auto limited = mitcad::new_document();
    try {
      report = std::string(limited->import_f3d_timeline(
          file.string(), R"({"dump": )" + json_path(dump) + R"(, "memory_limit": 1)" + std::string(watched) + "}"));
    } catch (const std::exception& e) {
      error = e.what();
    }
    CHECK(error.empty());
    CHECK(contains(report, "the import ran low on memory before its first item ("));
    CHECK(contains(report, "(the import's memory limit)): definitions were cut short or not tried and 1 modelling "
                           "items took the file's bodies; the final bodies were compared by volume only"));
    CHECK(contains(std::string(limited->query(R"({"query": "document"})")), "\"bodies\":2"));
  }
}

// An allocation that fails inside OCCT (mitcad#132), caught there as its
// checker and healing catch their failures, so that only the count of
// failed allocations tells (MITCAD_TEST_OCCT_ALLOCATION_FAILS): in the
// healing of the file's bodies, and in an item's kernel operation. The
// process goes on (OCCT's allocator throws instead of returning null), and
// the import stops as low on memory.
void test_failed_occt_allocation(const fs::path& dir) {
  const fs::path file = dir / "bodies.f3d";
  const fs::path dump = dir / "block-dump.json";
  std::ofstream(dump) << kBlockDump;
  for (const char* operation : {"heal", "extrude"}) {
    set_env("MITCAD_TEST_OCCT_ALLOCATION_FAILS", operation);
    auto document = mitcad::new_document();
    std::string report;
    std::string error;
    try {
      report = std::string(
          document->import_f3d_timeline(file.string(), R"({"dump": )" + json_path(dump) + R"(, "hang_limit": 60})"));
    } catch (const std::exception& e) {
      error = e.what();
    }
    set_env("MITCAD_TEST_OCCT_ALLOCATION_FAILS", "");
    CHECK(error.empty());
    CHECK(contains(report, "the import ran low on memory"));
    CHECK(contains(report, R"("low_memory": {)"));
    if (std::string(operation) == "heal") {
      CHECK(contains(report, "building a body of the file: out of memory"));
    } else {
      CHECK(contains(report, "extrude: out of memory"));
    }
  }
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

// The threads of the corpus imports (--threads, mitcad#95).
int g_import_threads = 1;

// Imports one design of a corpus file (`design` empty: the default one)
// and adds its items and body agreement to the coverage.
void import_corpus_design(const std::string& id, const std::string& path, const std::string& design,
                          const fs::path& reports, double time_limit, double hang_limit, Coverage& c) {
  ++c.designs;
  const auto start = std::chrono::steady_clock::now();
  std::string options = "{" + std::string(R"("time_limit": )") + std::to_string(time_limit) +
                        R"(, "hang_limit": )" + std::to_string(hang_limit) + R"(, "threads": )" +
                        std::to_string(g_import_threads);
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

// Imports every design of one corpus file (the timeline length decides
// whether a design is too slow here).
void corpus_file(const std::string& id, const std::string& path, int max_items, double time_limit,
                 double hang_limit, const fs::path& reports, Coverage& c) {
  ++c.files;
  auto probe = mitcad::new_document();
  std::string designs;
  try {
    designs = std::string(probe->import_f3d_timeline(path, R"({"no_verify": true, "no_fallback": true,
                                                                "no_compare": true, "list": true})"));
  } catch (const std::exception& e) {
    ++c.failed;
    std::printf("%s: listing failed: %s\n", id.c_str(), e.what());
    return;
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
    import_corpus_design(id, path, labels.size() > 1 ? labels[k] : "", reports, time_limit, hang_limit, c);
  }
}

// The coverage as totals lines (parallel_runs.hpp) and back.
std::map<std::string, double> totals_of(const Coverage& c) {
  std::map<std::string, double> t = {
      {"files", c.files},
      {"designs", c.designs},
      {"failed", c.failed},
      {"skipped", c.skipped},
      {"exact", c.exact},
      {"close", c.close},
      {"near", c.near},
      {"off", c.off},
      {"unmatched", c.unmatched},
      {"components", c.components},
      {"occurrences", c.occurrences},
      {"external", c.external},
      {"placed", c.placed},
      {"hung", c.hung},
      {"sketch_bulbs", c.sketch_bulbs},
      {"sketch_bulbs_unknown", c.sketch_bulbs_unknown},
      {"construction_bulbs", c.construction_bulbs},
      {"construction_bulbs_unknown", c.construction_bulbs_unknown},
      {"bulbs_set", c.bulbs_set},
      {"seconds", c.seconds},
  };
  for (const auto& [type, outcomes] : c.items) {
    for (const auto& [outcome, n] : outcomes) {
      t["item/" + type + "/" + outcome] = n;
    }
  }
  return t;
}

void add_totals(const std::map<std::string, double>& t, Coverage& c) {
  const auto get = [&t](const char* key) {
    const auto at = t.find(key);
    return at == t.end() ? 0 : static_cast<int>(at->second);
  };
  c.files += get("files");
  c.designs += get("designs");
  c.failed += get("failed");
  c.skipped += get("skipped");
  c.exact += get("exact");
  c.close += get("close");
  c.near += get("near");
  c.off += get("off");
  c.unmatched += get("unmatched");
  c.components += get("components");
  c.occurrences += get("occurrences");
  c.external += get("external");
  c.placed += get("placed");
  c.hung += get("hung");
  c.sketch_bulbs += get("sketch_bulbs");
  c.sketch_bulbs_unknown += get("sketch_bulbs_unknown");
  c.construction_bulbs += get("construction_bulbs");
  c.construction_bulbs_unknown += get("construction_bulbs_unknown");
  c.bulbs_set += get("bulbs_set");
  const auto seconds = t.find("seconds");
  c.seconds += seconds == t.end() ? 0.0 : seconds->second;
  for (const auto& [key, n] : t) {
    if (key.rfind("item/", 0) == 0) {
      const std::size_t slash = key.rfind('/');
      c.items[key.substr(5, slash - 5)][key.substr(slash + 1)] += static_cast<int>(n);
    }
  }
}

// The options a child gets from the corpus run (--corpus-file).
struct CorpusOptions {
  int every = 1;
  int max_items = 1 << 30;
  double time_limit = 0;
  double hang_limit = 600;
  fs::path reports;
  mitcad::runs::Settings parallel;
  std::string self;
};

int corpus(const fs::path& dir, const CorpusOptions& o) {
  const int every = o.every;
  const int max_items = o.max_items;
  const double time_limit = o.time_limit;
  const double hang_limit = o.hang_limit;
  const fs::path& reports = o.reports;
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
  std::vector<std::string> ids;
  std::vector<mitcad::runs::Command> commands;
  std::vector<double> weights;
  for (std::size_t i = 0; i < files.size(); i += static_cast<std::size_t>(every)) {
    const std::string number = std::to_string(i + 1);
    const std::string id = (number.size() < 2 ? "f0" : "f") + number;
    if (o.parallel.jobs <= 1) {
      corpus_file(id, files[i], max_items, time_limit, hang_limit, reports, c);
      continue;
    }
    // Each file in a child process of its own (mitcad#70).
    ids.push_back(id);
    weights.push_back(mitcad::runs::file_weight(files[i]));
    commands.push_back({o.self, "--corpus-file", files[i], "--id", id, "--max-items", std::to_string(max_items),
                        "--time-limit", std::to_string(time_limit), "--hang-limit", std::to_string(hang_limit),
                        "--reports", reports.string(), "--threads", std::to_string(g_import_threads)});
  }
  mitcad::runs::run_all(commands, o.parallel, [&](std::size_t i, const mitcad::runs::Outcome& outcome) {
    std::string text;
    std::map<std::string, double> totals;
    const bool finished = mitcad::runs::split_totals(outcome.output, text, totals);
    std::fputs(text.c_str(), stdout);
    std::fputs(mitcad::runs::describe_failure(ids[i], outcome, finished).c_str(), stdout);
    if (finished) {
      add_totals(totals, c);
    } else {
      ++c.files;
      ++c.failed;
    }
    std::fflush(stdout);
  }, weights);
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
  std::printf("components: %d made, %d occurrences placed, %d of other documents placed empty; %d bodies placed by "
              "occurrences\n",
              c.components, c.occurrences, c.external, c.placed);
  std::printf("light bulbs: %d sketches (%d unknown), %d construction features (%d unknown), %d kept where "
              "they differ from Mitcad's default\n",
              c.sketch_bulbs, c.sketch_bulbs_unknown, c.construction_bulbs, c.construction_bulbs_unknown,
              c.bulbs_set);
  std::printf("designs the geometry kernel hung on (imported again without the item): %d\n", c.hung);
  const int code = c.failed == 0 ? 0 : 1;
  if (mitcad::abandoned_imports() > 0) {
    // Threads the kernel hung in still run (also after an import that
    // failed hung): end without static destructors (mitcad#82).
    std::fflush(nullptr);
    std::_Exit(code);
  }
  return code;
}

// The reference models (route A).

// A reference model and how Mitcad's replay compares with the body stored
// in its file: the volume within `tolerance` (relative) of the stored one,
// and, where `parametric`, the import (checked against the file's
// history) keeping its loft or sweep as a feature, from the dump and from
// the file's own streams alike. The others differ by the fit of the stored
// surfaces, which Mitcad's own loft rules do not reproduce (commands.md,
// loft): their tolerances are the differences known, so that a change in
// Mitcad's lofts shows.
struct ModelCheck {
  const char* id;
  double tolerance;
  bool parametric;
  const char* type;
};

const ModelCheck kLoftModels[] = {
    {"sweep_path", 1e-6, true, "SweepFeature"},
    {"sweep_twist", 1e-5, true, "SweepFeature"},
    {"sweep_guide_rail", 1e-5, true, "SweepFeature"},
    {"loft_two_squares", 1e-4, true, "LoftFeature"},
    {"loft_three_circles", 1e-3, true, "LoftFeature"},
    {"loft_centerline", 1e-3, true, "LoftFeature"},
    {"loft_direction_0", 7e-3, false, "LoftFeature"},     // +0.65 %: the stored lofts of circles are not round
    {"loft_direction_20", 1.3e-2, false, "LoftFeature"},  // +1.18 %
    {"loft_direction_w05", 8e-3, false, "LoftFeature"},   // +0.74 %
    {"loft_direction_w2", 5e-3, false, "LoftFeature"},    // +0.48 % (+0.52 % to the file's own body)
    {"loft_point_tangent", 1e-4, true, "LoftFeature"},
    {"loft_rail_circle_one", 1e-3, true, "LoftFeature"},  // its bent faces integrate to about 1e-4
    {"loft_rails_circle_two", 1e-3, true, "LoftFeature"},
    {"loft_rails_squares", 1e-4, true, "LoftFeature"},
    {"loft_tangent_face", 1e-6, true, "LoftFeature"},
    {"loft_smooth_face", 1e-6, true, "LoftFeature"},
    {"loft_rail_tangent", 6e-2, false, "LoftFeature"},    // +5.6 %: the stored fit of a rail with a condition
    {"loft_direction_w15", 7e-3, false, "LoftFeature"},   // +0.56 %
    {"loft_direction_w3", 5e-3, true, "LoftFeature"},     // +0.34 %
    {"loft_direction_squares_w2", 1e-6, true, "LoftFeature"},
    // M10x1.5 6g over a 10 mm rod: the stored flanks are fitted splines
    // (Mitcad's exact profile has 3e-5 more volume).
    {"thread_modeled", 1e-4, true, "ThreadFeature"},
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

// The outcome of the model's loft or sweep in an import report.
std::string outcome_of(const std::string& report, const char* type) {
  const std::vector<std::string> types = strings_of(report, "type");
  const std::vector<std::string> outcomes = strings_of(report, "outcome");
  std::string outcome = "none";
  for (std::size_t i = 0; i < types.size() && i < outcomes.size(); ++i) {
    if (types[i] == type) {
      outcome = outcomes[i];
    }
  }
  return outcome;
}

// Checks one reference model and prints its line; whether it passed.
bool check_model(const fs::path& dir, const ModelCheck& model) {
  std::error_code ec;
  const fs::path f3d = dir / model.id / (std::string(model.id) + ".f3d");
  const fs::path dump = dir / model.id / (std::string(model.id) + ".json");
  if (!fs::is_regular_file(f3d, ec) || !fs::is_regular_file(dump, ec)) {
    std::printf("%-24s missing\n", model.id);
    return false;
  }
  // The stored volume: the dump's final bodies (cm3).
  const std::string text = read_text(dump);
  const std::size_t final_at = text.find("\"final\"");
  const std::vector<double> stored =
      final_at == std::string::npos ? std::vector<double>{} : numbers_of(text.substr(final_at), "volume");
  if (stored.empty()) {
    std::printf("%-24s no final volume in its dump\n", model.id);
    return false;
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
    const std::string outcome = outcome_of(report, model.type);
    // The file's own streams, decoded (mitcad#34).
    auto streams = mitcad::new_document();
    const std::string own = outcome_of(std::string(streams->import_f3d_timeline(f3d.string(), "{}")), model.type);
    const double difference = volume / expected - 1.0;
    const bool ok = std::abs(difference) <= model.tolerance &&
                    (!model.parametric || (outcome == "parametric" && own == "parametric"));
    std::printf("%-24s %14.4f %14.4f %+9.4f  %-11s %s%s\n", model.id, expected, volume, 100 * difference,
                outcome.c_str(), own.c_str(), ok ? "" : "  FAILED");
    return ok;
  } catch (const std::exception& e) {
    std::printf("%-24s import failed: %s\n", model.id, e.what());
    return false;
  }
}

int models(const fs::path& dir, const mitcad::runs::Settings& parallel, const std::string& self) {
  std::error_code ec;
  if (dir.empty() || !fs::is_directory(dir, ec)) {
    std::printf("reference models not found; skipped\n");
    return kSkipped;
  }
  int failed = 0;
  std::printf("%-24s %14s %14s %9s  %-11s %s\n", "model", "stored mm3", "Mitcad mm3", "diff %", "import",
              "streams");
  std::vector<mitcad::runs::Command> commands;
  for (const ModelCheck& model : kLoftModels) {
    if (parallel.jobs <= 1) {
      failed += check_model(dir, model) ? 0 : 1;
    } else {
      // Each model in a child process of its own (mitcad#70).
      commands.push_back({self, "--model", dir.string(), model.id});
    }
  }
  std::fflush(stdout);
  mitcad::runs::run_all(commands, parallel, [&](std::size_t i, const mitcad::runs::Outcome& outcome) {
    std::string text;
    std::map<std::string, double> totals;
    const bool finished = mitcad::runs::split_totals(outcome.output, text, totals);
    std::fputs(text.c_str(), stdout);
    std::fputs(mitcad::runs::describe_failure(kLoftModels[i].id, outcome, finished).c_str(), stdout);
    failed += finished ? static_cast<int>(totals["failed"]) : 1;
    std::fflush(stdout);
  });
  return failed == 0 ? 0 : 1;
}

// A child of a parallel run (mitcad#70): one corpus file or one model,
// then its totals.
int child(const std::vector<std::string>& args) {
  if (args.size() == 3 && args[0] == "--model") {
    for (const ModelCheck& model : kLoftModels) {
      if (args[2] == model.id) {
        const bool ok = check_model(args[1], model);
        mitcad::runs::print_totals({{"failed", ok ? 0 : 1}});
        return 0;
      }
    }
    throw std::runtime_error("unknown model " + args[2]);
  }
  std::string path;
  std::string id;
  CorpusOptions o;
  for (std::size_t i = 0; i + 1 < args.size(); i += 2) {
    const std::string& value = args[i + 1];
    if (args[i] == "--corpus-file") {
      path = value;
    } else if (args[i] == "--id") {
      id = value;
    } else if (args[i] == "--max-items") {
      o.max_items = std::atoi(value.c_str());
    } else if (args[i] == "--time-limit") {
      o.time_limit = std::atof(value.c_str());
    } else if (args[i] == "--hang-limit") {
      o.hang_limit = std::atof(value.c_str());
    } else if (args[i] == "--reports") {
      o.reports = value;
    } else if (args[i] == "--threads") {
      g_import_threads = std::max(1, std::atoi(value.c_str()));
    }
  }
  Coverage c;
  corpus_file(id, path, o.max_items, o.time_limit, o.hang_limit, o.reports, c);
  mitcad::runs::print_totals(totals_of(c));
  if (mitcad::abandoned_imports() > 0) {
    // Threads the kernel hung in still run (also after an import that
    // failed hung): end without static destructors (mitcad#82).
    std::fflush(nullptr);
    std::_Exit(0);
  }
  return 0;
}

} // namespace

int main(int argc, char** argv) {
  mitcad::runs::no_core_dumps();
  mitcad::io::silence_occt_messages();
  std::vector<std::string> args(argv + 1, argv + argc);
  try {
    if (!args.empty() && (args[0] == "--corpus-file" || args[0] == "--model")) {
      return child(args);
    }
    std::string jobs;
    std::string memory;
    std::string timeout;
    mitcad::runs::take_options(args, jobs, memory, timeout);
    if (!args.empty() && args[0] == "--corpus") {
      fs::path dir;
      CorpusOptions o;
      for (std::size_t i = 1; i < args.size(); ++i) {
        if (args[i] == "--every" && i + 1 < args.size()) {
          o.every = std::max(1, std::atoi(args[++i].c_str()));
        } else if (args[i] == "--max-items" && i + 1 < args.size()) {
          o.max_items = std::atoi(args[++i].c_str());
        } else if (args[i] == "--time-limit" && i + 1 < args.size()) {
          o.time_limit = std::atof(args[++i].c_str());
        } else if (args[i] == "--hang-limit" && i + 1 < args.size()) {
          o.hang_limit = std::atof(args[++i].c_str());
        } else if (args[i] == "--reports" && i + 1 < args.size()) {
          o.reports = args[++i];
        } else if (args[i] == "--threads" && i + 1 < args.size()) {
          g_import_threads = std::max(1, std::atoi(args[++i].c_str()));
        } else {
          dir = args[i];
        }
      }
      o.parallel = mitcad::runs::settings(jobs, memory, timeout);
      o.self = mitcad::runs::executable_path(argv[0]);
      return corpus(dir.empty() ? default_corpus() : dir, o);
    }
    if (!args.empty() && args[0] == "--models") {
      return models(args.size() > 1 ? fs::path(args[1]) : default_models(),
                    mitcad::runs::settings(jobs, memory, timeout), mitcad::runs::executable_path(argv[0]));
    }
    if (args.size() != 1) {
      std::fprintf(stderr, "usage: test_f3d_import <dir> | --corpus [dir] [--every N] [--max-items N] "
                           "[--time-limit S] [--hang-limit S] [--reports DIR] [--threads N] | --models [dir]\n"
                           "  corpus and models: [--jobs N] [--memory SIZE] [--file-timeout S]\n");
      return 2;
    }
    const fs::path dir = args[0];
    fs::create_directories(dir);
    test_file_without_design(dir);
    test_watchdog_memory_limit(dir);
    test_watchdog(dir);
    test_watchdog_slow_build(dir);
    test_out_of_memory(dir);
    test_failed_occt_allocation(dir);
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
