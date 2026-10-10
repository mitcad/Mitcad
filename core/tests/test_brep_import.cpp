// SPDX-License-Identifier: MIT
// Builds the bodies of .f3d files with OCCT: Rust reads and converts them
// (mitcad-f3d), C++ builds them (mitcad/geometry/brep_import.hpp).
//
//   test_brep_import                  self test with Mitcad's own test bodies
//   test_brep_import --corpus [dir]   every .f3d/.f3z under dir (default
//                                     MITCAD_F3D_CORPUS, else ~/f3d-corpus);
//                                     exits 77 (skipped) when it is missing
//   test_brep_import [-v] <file>...   report on the given files
//   --every N                         build only every Nth body (quick runs)
//   --min-valid P                     fail when fewer than P % of the solids
//                                     are valid
//   --only TEXT                       build only bodies whose name (as
//                                     printed, e.g. BREP.d2bbd301#1) ends
//                                     with TEXT
//   --save DIR                        write invalid bodies (and those chosen
//                                     with --only) as OCCT .brep files to
//                                     DIR: NAME.brep as built and
//                                     NAME.raw.brep without healing
//   --history                         also build the top-level bodies of
//                                     the .smbh blobs rolled back through
//                                     their ASM history (NAME@1, @2, ...:
//                                     states rolled back)
//   --meshes P                        instead of the per-body report,
//                                     compare the display meshes saved with
//                                     the documents with the built bodies of
//                                     the same face count and bounding box;
//                                     fail when a mesh has no such body or
//                                     its volume differs by more than P %
//   --jobs N, --memory SIZE,          each file in a child process of its
//   --file-timeout S                  own, N at a time (parallel_runs.hpp;
//                                     --jobs 1: all in this process)
//
// Corpus and file runs print one line per body and a summary. Files are
// named by their position (f01, f02, ...) in sorted path order, so that
// reports need not name other people's models; -v adds the file names and
// the messages of each body.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <filesystem>
#include <map>
#include <string>
#include <vector>

#include <BRepTools.hxx>

#include "mitcad/geometry/brep_import.hpp"
#include "mitcad_bridge/brep_import.h"
#include "parallel_runs.hpp"
#include "rust/cxx.h"

namespace {

namespace fs = std::filesystem;
using mitcad::f3d::BrepBodyData;
using mitcad::f3d::DisplayMeshData;

constexpr double kPi = 3.14159265358979323846;
constexpr int kSkipped = 77;

mitcad::brep::Body to_body(const BrepBodyData& d) {
  mitcad::brep::Body b;
  b.ints.assign(d.ints.begin(), d.ints.end());
  b.reals.assign(d.reals.begin(), d.reals.end());
  for (const auto& g : d.curves) {
    b.curves.push_back({g.kind, g.int_offset, g.int_count, g.real_offset, g.real_count});
  }
  for (const auto& g : d.surfaces) {
    b.surfaces.push_back({g.kind, g.int_offset, g.int_count, g.real_offset, g.real_count});
  }
  b.vertices.assign(d.vertices.begin(), d.vertices.end());
  for (const auto& e : d.edges) {
    b.edges.push_back({e.curve, e.v0, e.v1, e.t0, e.t1, e.tolerance});
  }
  for (const auto& f : d.faces) {
    b.faces.push_back(
        {f.surface, f.reversed, f.double_sided, f.first_loop, f.loop_count, f.first_point_loop, f.point_loop_count});
  }
  for (const auto& l : d.loops) {
    b.loops.push_back({l.first_coedge, l.coedge_count});
  }
  for (const auto& c : d.coedges) {
    b.coedges.push_back({c.edge, c.forward});
  }
  for (const auto& s : d.shells) {
    b.shells.push_back({s.lump, s.first_face, s.face_count, s.closed});
  }
  b.shell_faces.assign(d.shell_faces.begin(), d.shell_faces.end());
  b.point_loops.assign(d.point_loops.begin(), d.point_loops.end());
  b.lump_count = d.lump_count;
  b.transform.assign(d.transform.begin(), d.transform.end());
  return b;
}

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_brep_import.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool near(double a, double b, double rel) { return std::abs(a - b) <= rel * std::abs(b); }

void print_messages(const mitcad::brep::BuildReport& r) {
  for (const auto& m : r.messages) {
    std::printf("    %s\n", m.c_str());
  }
  if (!r.error.empty()) {
    std::printf("    error: %s\n", r.error.c_str());
  }
}

int self_test() {
  const rust::Vec<BrepBodyData> bodies = mitcad::f3d::f3d_test_bodies();
  CHECK(bodies.size() == 3);
  if (bodies.size() != 3) {
    return 1;
  }
  const auto cube = mitcad::brep::build_body(to_body(bodies[0]));
  print_messages(cube.report);
  CHECK(cube.report.built);
  CHECK(cube.report.solid);
  CHECK(cube.report.valid);
  CHECK(near(cube.report.volume, 1000.0, 1e-9));
  CHECK(near(cube.report.raw_volume, 1000.0, 1e-9));
  CHECK(near(cube.report.area, 600.0, 1e-9));
  CHECK(cube.report.faces == 6);
  CHECK(cube.report.edges == 12);
  CHECK(cube.report.vertices == 8);
  CHECK(near(cube.report.bbox[3], 10.0, 1e-6));

  // The ASM side face has no seam; the builder adds one.
  const auto cylinder = mitcad::brep::build_body(to_body(bodies[1]));
  print_messages(cylinder.report);
  CHECK(cylinder.report.built);
  CHECK(cylinder.report.solid);
  CHECK(cylinder.report.valid);
  CHECK(near(cylinder.report.volume, kPi * 100.0 * 20.0, 1e-6));
  CHECK(near(cylinder.report.raw_volume, kPi * 100.0 * 20.0, 1e-6));
  CHECK(near(cylinder.report.area, 2.0 * kPi * 100.0 + 2.0 * kPi * 10.0 * 20.0, 1e-6));
  CHECK(cylinder.report.faces == 3);

  // The apex is a loop without edges in ASM; the builder closes the cone
  // face there with a degenerated edge.
  const auto cone = mitcad::brep::build_body(to_body(bodies[2]));
  print_messages(cone.report);
  CHECK(cone.report.built);
  CHECK(cone.report.solid);
  CHECK(cone.report.valid);
  CHECK(near(cone.report.volume, kPi * 100.0 * 20.0 / 3.0, 1e-6));
  CHECK(near(cone.report.raw_volume, kPi * 100.0 * 20.0 / 3.0, 1e-6));
  CHECK(near(cone.report.area, kPi * 100.0 + kPi * 10.0 * std::sqrt(500.0), 1e-6));
  CHECK(cone.report.faces == 2);
  CHECK(near(cone.report.bbox[5], 20.0, 1e-6));

  if (failures == 0) {
    std::printf("test_brep_import: all checks passed\n");
  }
  return failures == 0 ? 0 : 1;
}

// Results of a group of bodies.
struct Counts {
  int bodies = 0;
  int solids = 0;
  int solids_valid = 0;
  int sheets = 0;
  int sheets_valid = 0;
  int not_built = 0;
  int negative_raw_volume = 0;
  int with_issues = 0;
  int with_failed_geometry = 0;
};

// Display mesh comparison (--meshes).
struct MeshTotals {
  int bodies = 0;
  int hidden = 0;
  int closed = 0;
  int matched = 0;
  int from_history_blobs = 0;
  int within = 0;
  double worst = 0.0;
};

struct Totals {
  int files = 0;
  int files_failed = 0;
  // --meshes: the allowed volume difference (%), negative when not comparing.
  double mesh_error = -1.0;
  MeshTotals meshes;
  // The saved bodies, and the .smbh bodies rolled back (--history).
  Counts saved;
  Counts history;
  // Sampling: build every `every`-th body only.
  int every = 1;
  long seen = 0;
  // Body filter (--only), output directory (--save), history (--history).
  std::string only;
  std::string save;
  bool with_history = false;
};

std::string short_name(const std::string& entry) {
  const auto slash = entry.rfind('/');
  return slash == std::string::npos ? entry : entry.substr(slash + 1);
}

// Builds one body and prints its line; returns whether it is a valid solid
// (in `solid`, whether it is a solid).
bool report_body(const std::string& id, const BrepBodyData& data, bool verbose, const Totals& t, Counts& c,
                 bool& solid) {
  solid = false;
  const std::string document(data.document);
  const std::string doc = document.empty() ? "" : short_name(document).substr(0, 8) + "/";
  std::string name = doc + short_name(std::string(data.blob)).substr(0, 13) + (data.history ? "h" : "") + "#" +
                     std::to_string(data.record);
  if (data.history_step > 0) {
    name += "@" + std::to_string(data.history_step);
  }
  if (!t.only.empty() &&
      (name.size() < t.only.size() || name.compare(name.size() - t.only.size(), t.only.size(), t.only) != 0)) {
    return false;
  }
  ++c.bodies;
  const auto start = std::chrono::steady_clock::now();
  const mitcad::brep::Body body = to_body(data);
  const auto result = mitcad::brep::build_body(body);
  const auto& r = result.report;
  const double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
  const bool issues = !data.issues.empty() || data.skipped_faces > 0;
  const bool failed_geometry = r.curves_failed + r.surfaces_failed + r.edges_failed + r.faces_failed > 0;
  c.with_issues += issues ? 1 : 0;
  c.with_failed_geometry += failed_geometry ? 1 : 0;
  if (!r.built) {
    ++c.not_built;
  } else if (r.solid) {
    solid = true;
    ++c.solids;
    c.solids_valid += r.valid ? 1 : 0;
    c.negative_raw_volume += r.raw_volume < 0.0 ? 1 : 0;
  } else {
    ++c.sheets;
    c.sheets_valid += r.valid ? 1 : 0;
  }
  std::printf(
      "%s %s%s %s %s V=%.6g raw=%.6g A=%.6g F=%d E=%d bbox=[%.4g %.4g %.4g, %.4g %.4g %.4g] %.0fms%s%s%s\n",
      id.c_str(), name.c_str(), data.top_level ? "" : "o", !r.built ? "FAILED" : (r.solid ? "solid" : "sheet"),
      r.valid ? "valid" : "INVALID", r.volume, r.raw_volume, r.area, r.faces, r.edges, r.bbox[0], r.bbox[1],
      r.bbox[2], r.bbox[3], r.bbox[4], r.bbox[5], ms, data.transform.empty() ? "" : " transformed",
      issues ? " issues" : "", failed_geometry ? " failed-geometry" : "");
  if (verbose) {
    for (const auto& i : data.issues) {
      std::printf("    issue: %s\n", std::string(i).c_str());
    }
    print_messages(r);
  }
  if (!t.save.empty() && r.built && (!r.valid || !t.only.empty())) {
    std::string file = id + "_" + name;
    for (char& ch : file) {
      if (ch == '/' || ch == '#' || ch == '@') {
        ch = '_';
      }
    }
    const fs::path dir(t.save);
    BRepTools::Write(result.shape, (dir / (file + ".brep")).string().c_str());
    mitcad::brep::BuildOptions raw;
    raw.fix = false;
    const auto unfixed = mitcad::brep::build_body(body, raw);
    if (unfixed.report.built) {
      BRepTools::Write(unfixed.shape, (dir / (file + ".raw.brep")).string().c_str());
    }
  }
  return solid && r.valid;
}

std::string body_name(const BrepBodyData& data) {
  const std::string document(data.document);
  const std::string doc = document.empty() ? "" : short_name(document).substr(0, 8) + "/";
  return doc + short_name(std::string(data.blob)).substr(0, 13) + (data.history ? "h" : "") + "#" +
         std::to_string(data.record);
}

// Compares every display mesh of the file with the built solids of the same
// document that have as many faces: the one with the closest bounding box
// (within 1 % of the diagonal) is taken, and the volumes are compared.
void compare_meshes(const std::string& id, const std::string& path, Totals& t) {
  rust::Vec<DisplayMeshData> meshes;
  rust::Vec<BrepBodyData> bodies;
  try {
    meshes = mitcad::f3d::f3d_read_display_meshes(path);
    if (meshes.empty()) {
      return;
    }
    bodies = mitcad::f3d::f3d_read_bodies(path);
  } catch (const std::exception& e) {
    ++t.files_failed;
    std::printf("%s: read failed: %s\n", id.c_str(), e.what());
    return;
  }
  // Built solids by body index, built once.
  std::map<std::size_t, mitcad::brep::BuildReport> built;
  auto build = [&](std::size_t i) -> const mitcad::brep::BuildReport& {
    auto it = built.find(i);
    if (it == built.end()) {
      it = built.emplace(i, mitcad::brep::build_body(to_body(bodies[i])).report).first;
    }
    return it->second;
  };
  for (const DisplayMeshData& m : meshes) {
    ++t.meshes.bodies;
    const std::string document(m.document);
    const std::string label =
        id + " mesh " + (document.empty() ? "" : short_name(document).substr(0, 8) + "/") + std::to_string(m.index);
    if (m.triangles == 0) {
      ++t.meshes.hidden;
      std::printf("%s F=%u hidden (no mesh)\n", label.c_str(), m.faces);
      continue;
    }
    t.meshes.closed += m.closed ? 1 : 0;
    const double diagonal =
        std::sqrt(std::pow(m.bbox[3] - m.bbox[0], 2) + std::pow(m.bbox[4] - m.bbox[1], 2) +
                  std::pow(m.bbox[5] - m.bbox[2], 2));
    std::size_t best = bodies.size();
    double best_box = 0.0;
    double best_error = 0.0;
    for (std::size_t i = 0; i < bodies.size(); ++i) {
      if (std::string(bodies[i].document) != document || bodies[i].faces.size() != m.faces) {
        continue;
      }
      const auto& r = build(i);
      if (!r.built || !r.solid) {
        continue;
      }
      double box = 0.0;
      for (int k = 0; k < 6; ++k) {
        box = std::max(box, std::abs(r.bbox[k] - m.bbox[k]));
      }
      const double error = 100.0 * (m.volume - r.volume) / r.volume;
      if (box <= 0.01 * diagonal &&
          (best == bodies.size() || box < best_box - 1e-9 ||
           (box <= best_box + 1e-9 && std::abs(error) < std::abs(best_error)))) {
        best = i;
        best_box = box;
        best_error = error;
      }
    }
    if (best == bodies.size()) {
      std::printf("%s F=%u T=%u %s V=%.6g NO MATCH\n", label.c_str(), m.faces, m.triangles,
                  m.closed ? "closed" : "open", m.volume);
      continue;
    }
    const auto& r = built.at(best);
    ++t.meshes.matched;
    t.meshes.from_history_blobs += bodies[best].history ? 1 : 0;
    t.meshes.within += std::abs(best_error) <= t.mesh_error ? 1 : 0;
    t.meshes.worst = std::max(t.meshes.worst, std::abs(best_error));
    std::printf("%s F=%u T=%u %s V=%.6g A=%.6g ~ %s%s V=%.6g A=%.6g dV=%+.3f%% dA=%+.3f%% bbox %.3g mm\n",
                label.c_str(), m.faces, m.triangles, m.closed ? "closed" : "open", m.volume, m.area,
                body_name(bodies[best]).c_str(), r.valid ? "" : " (invalid)", r.volume, r.area, best_error,
                100.0 * (m.area - r.area) / r.area, best_box);
  }
}

void report_file(const std::string& id, const std::string& path, bool verbose, Totals& t) {
  ++t.files;
  if (t.mesh_error >= 0.0) {
    compare_meshes(id, path, t);
    return;
  }
  rust::Vec<BrepBodyData> bodies;
  rust::Vec<BrepBodyData> history;
  try {
    bodies = mitcad::f3d::f3d_read_bodies(path);
    if (t.with_history) {
      history = mitcad::f3d::f3d_read_history_bodies(path);
    }
  } catch (const std::exception& e) {
    ++t.files_failed;
    std::printf("%s: read failed: %s\n", id.c_str(), e.what());
    return;
  }
  if (verbose) {
    std::printf("%s: %s\n", id.c_str(), path.c_str());
  }
  int file_solids = 0;
  int file_valid = 0;
  for (const BrepBodyData& data : bodies) {
    if (t.every > 1 && t.seen++ % t.every != 0) {
      continue;
    }
    bool solid = false;
    file_valid += report_body(id, data, verbose, t, t.saved, solid) ? 1 : 0;
    file_solids += solid ? 1 : 0;
  }
  for (const BrepBodyData& data : history) {
    bool solid = false;
    report_body(id, data, verbose, t, t.history, solid);
  }
  std::printf("%s: %zu bodies, %d solids, %d valid\n", id.c_str(), bodies.size(), file_solids, file_valid);
}

void print_counts(const char* what, const Counts& c) {
  std::printf("%s %d, not built %d\n", what, c.bodies, c.not_built);
  std::printf("solids %d (valid %d), sheets %d (valid %d), negative raw volume %d\n", c.solids, c.solids_valid,
              c.sheets, c.sheets_valid, c.negative_raw_volume);
  std::printf("bodies with converter issues %d, with geometry the builder rejected %d\n", c.with_issues,
              c.with_failed_geometry);
}

void print_totals(const Totals& t) {
  std::printf("files %d (read failed %d), ", t.files, t.files_failed);
  if (t.mesh_error >= 0.0) {
    const MeshTotals& m = t.meshes;
    std::printf("display bodies %d (hidden %d, closed meshes %d), matched %d (%d of them .smbh bodies), "
                "volume within %.3g %%: %d, largest difference %.3f %%\n",
                m.bodies, m.hidden, m.closed, m.matched, m.from_history_blobs, t.mesh_error, m.within, m.worst);
    return;
  }
  print_counts("bodies", t.saved);
  if (t.with_history) {
    print_counts("rolled back .smbh bodies (--history)", t.history);
  }
}

std::vector<std::string> corpus_files(const fs::path& dir) {
  std::vector<std::string> out;
  for (const auto& entry : fs::recursive_directory_iterator(dir)) {
    if (!entry.is_regular_file()) {
      continue;
    }
    const auto ext = entry.path().extension().string();
    if (ext == ".f3d" || ext == ".f3z") {
      out.push_back(entry.path().string());
    }
  }
  std::sort(out.begin(), out.end());
  return out;
}

std::string file_id(std::size_t i) {
  char buf[32];
  std::snprintf(buf, sizeof buf, "f%02zu", i + 1);
  return buf;
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

// The totals as totals lines (parallel_runs.hpp) and back.
void counts_to(const char* prefix, const Counts& c, std::map<std::string, double>& out) {
  const std::string p(prefix);
  out[p + "bodies"] = c.bodies;
  out[p + "solids"] = c.solids;
  out[p + "solids_valid"] = c.solids_valid;
  out[p + "sheets"] = c.sheets;
  out[p + "sheets_valid"] = c.sheets_valid;
  out[p + "not_built"] = c.not_built;
  out[p + "negative_raw_volume"] = c.negative_raw_volume;
  out[p + "with_issues"] = c.with_issues;
  out[p + "with_failed_geometry"] = c.with_failed_geometry;
}

void add_counts(const char* prefix, const std::map<std::string, double>& in, Counts& c) {
  const std::string p(prefix);
  const auto get = [&](const char* key) {
    const auto at = in.find(p + key);
    return at == in.end() ? 0 : static_cast<int>(at->second);
  };
  c.bodies += get("bodies");
  c.solids += get("solids");
  c.solids_valid += get("solids_valid");
  c.sheets += get("sheets");
  c.sheets_valid += get("sheets_valid");
  c.not_built += get("not_built");
  c.negative_raw_volume += get("negative_raw_volume");
  c.with_issues += get("with_issues");
  c.with_failed_geometry += get("with_failed_geometry");
}

std::map<std::string, double> totals_of(const Totals& t) {
  std::map<std::string, double> out = {
      {"files", t.files},
      {"files_failed", t.files_failed},
      {"meshes/bodies", t.meshes.bodies},
      {"meshes/hidden", t.meshes.hidden},
      {"meshes/closed", t.meshes.closed},
      {"meshes/matched", t.meshes.matched},
      {"meshes/from_history_blobs", t.meshes.from_history_blobs},
      {"meshes/within", t.meshes.within},
      {"max:meshes/worst", t.meshes.worst},
  };
  counts_to("saved/", t.saved, out);
  counts_to("history/", t.history, out);
  return out;
}

void add_totals(const std::map<std::string, double>& in, Totals& t) {
  const auto get = [&](const char* key) {
    const auto at = in.find(key);
    return at == in.end() ? 0.0 : at->second;
  };
  t.files += static_cast<int>(get("files"));
  t.files_failed += static_cast<int>(get("files_failed"));
  t.meshes.bodies += static_cast<int>(get("meshes/bodies"));
  t.meshes.hidden += static_cast<int>(get("meshes/hidden"));
  t.meshes.closed += static_cast<int>(get("meshes/closed"));
  t.meshes.matched += static_cast<int>(get("meshes/matched"));
  t.meshes.from_history_blobs += static_cast<int>(get("meshes/from_history_blobs"));
  t.meshes.within += static_cast<int>(get("meshes/within"));
  t.meshes.worst = std::max(t.meshes.worst, get("max:meshes/worst"));
  add_counts("saved/", in, t.saved);
  add_counts("history/", in, t.history);
}

// Reports every file, each in a child process of its own when `parallel`
// says so (mitcad#70): the child gets the file, its id, the options and,
// when only every Nth body is built, how many bodies the files before it
// have (counted first, also in children).
void report_files(const std::vector<std::string>& files, const std::vector<std::string>& options,
                  const mitcad::runs::Settings& parallel, const std::string& self, bool verbose, Totals& t) {
  if (parallel.jobs <= 1) {
    for (std::size_t i = 0; i < files.size(); ++i) {
      report_file(file_id(i), files[i], verbose, t);
    }
    return;
  }
  std::vector<double> weights;
  for (const std::string& file : files) {
    weights.push_back(mitcad::runs::file_weight(file));
  }
  std::vector<long> seen(files.size(), 0);
  if (t.every > 1 && t.mesh_error < 0.0) {
    std::vector<mitcad::runs::Command> counts;
    for (const std::string& file : files) {
      counts.push_back({self, "--count-bodies", file});
    }
    long total = 0;
    mitcad::runs::run_all(counts, parallel, [&](std::size_t i, const mitcad::runs::Outcome& outcome) {
      std::string text;
      std::map<std::string, double> totals;
      mitcad::runs::split_totals(outcome.output, text, totals);
      seen[i] = total;
      total += static_cast<long>(totals["bodies"]);
    }, weights);
  }
  std::vector<mitcad::runs::Command> commands;
  for (std::size_t i = 0; i < files.size(); ++i) {
    mitcad::runs::Command command = {self, "--corpus-file", files[i], "--id", file_id(i), "--seen",
                                     std::to_string(seen[i])};
    command.insert(command.end(), options.begin(), options.end());
    commands.push_back(command);
  }
  mitcad::runs::run_all(commands, parallel, [&](std::size_t i, const mitcad::runs::Outcome& outcome) {
    std::string text;
    std::map<std::string, double> totals;
    const bool finished = mitcad::runs::split_totals(outcome.output, text, totals);
    std::fputs(text.c_str(), stdout);
    std::fputs(mitcad::runs::describe_failure(file_id(i), outcome, finished).c_str(), stdout);
    if (finished) {
      add_totals(totals, t);
    } else {
      ++t.files;
      ++t.files_failed;
    }
    std::fflush(stdout);
  }, weights);
}

} // namespace

int main(int argc, char** argv) {
  mitcad::runs::no_core_dumps();
  bool verbose = false;
  bool corpus = false;
  int every = 1;
  double min_valid = 0.0;
  std::string only;
  std::string save;
  bool history = false;
  double mesh_error = -1.0;
  std::vector<std::string> paths;
  std::vector<std::string> args(argv + 1, argv + argc);
  std::string jobs;
  std::string memory;
  std::string timeout;
  // A child of a parallel run (mitcad#70): one file, then its totals.
  std::string child_file;
  std::string child_id;
  long child_seen = 0;
  // The options a child gets.
  std::vector<std::string> options;
  try {
    mitcad::runs::take_options(args, jobs, memory, timeout);
    if (args.size() == 2 && args[0] == "--count-bodies") {
      std::size_t bodies = 0;
      try {
        bodies = mitcad::f3d::f3d_read_bodies(args[1]).size();
      } catch (const std::exception&) {
        // Read again, and reported, by the file's run.
      }
      mitcad::runs::print_totals({{"bodies", static_cast<double>(bodies)}});
      return 0;
    }
    for (std::size_t i = 0; i < args.size(); ++i) {
      const std::string& a = args[i];
      const bool more = i + 1 < args.size();
      if (a == "-v") {
        verbose = true;
        options.push_back(a);
      } else if (a == "--corpus") {
        corpus = true;
      } else if (a == "--every" && more) {
        every = std::max(1, std::atoi(args[++i].c_str()));
        options.insert(options.end(), {a, args[i]});
      } else if (a == "--min-valid" && more) {
        min_valid = std::atof(args[++i].c_str());
      } else if (a == "--only" && more) {
        only = args[++i];
        options.insert(options.end(), {a, args[i]});
      } else if (a == "--save" && more) {
        save = args[++i];
        options.insert(options.end(), {a, args[i]});
      } else if (a == "--history") {
        history = true;
        options.push_back(a);
      } else if (a == "--meshes" && more) {
        mesh_error = std::max(0.0, std::atof(args[++i].c_str()));
        options.insert(options.end(), {a, args[i]});
      } else if (a == "--corpus-file" && more) {
        child_file = args[++i];
      } else if (a == "--id" && more) {
        child_id = args[++i];
      } else if (a == "--seen" && more) {
        child_seen = std::atol(args[++i].c_str());
      } else {
        paths.push_back(a);
      }
    }
    Totals totals;
    totals.every = every;
    totals.only = only;
    totals.save = save;
    totals.with_history = history;
    totals.mesh_error = mesh_error;
    if (!child_file.empty()) {
      totals.seen = child_seen;
      report_file(child_id, child_file, verbose, totals);
      mitcad::runs::print_totals(totals_of(totals));
      return 0;
    }
    if (!corpus && paths.empty()) {
      return self_test();
    }
    std::vector<std::string> files;
    if (corpus) {
      const fs::path dir = !paths.empty() ? fs::path(paths[0]) : default_corpus();
      std::error_code ec;
      if (dir.empty() || !fs::is_directory(dir, ec)) {
        std::printf("corpus not found; skipped\n");
        return kSkipped;
      }
      files = corpus_files(dir);
    } else {
      files = paths;
    }
    report_files(files, options, mitcad::runs::settings(jobs, memory, timeout),
                 mitcad::runs::executable_path(argv[0]), verbose, totals);
    print_totals(totals);
    if (mesh_error >= 0.0) {
      const MeshTotals& m = totals.meshes;
      const bool ok = m.matched == m.bodies - m.hidden && m.within == m.matched;
      return totals.files_failed == 0 && ok ? 0 : 1;
    }
    const Counts& c = totals.saved;
    const double valid_percent = c.solids > 0 ? 100.0 * c.solids_valid / c.solids : 100.0;
    if (valid_percent < min_valid) {
      std::printf("only %.1f %% of the solids are valid (required %.1f %%)\n", valid_percent, min_valid);
      return 1;
    }
    return totals.files_failed == 0 ? 0 : 1;
  } catch (const std::exception& e) {
    std::fprintf(stderr, "test_brep_import: %s\n", e.what());
    return 2;
  }
}
