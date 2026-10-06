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

} // namespace

int main(int argc, char** argv) {
  bool verbose = false;
  bool corpus = false;
  int every = 1;
  double min_valid = 0.0;
  std::string only;
  std::string save;
  bool history = false;
  double mesh_error = -1.0;
  std::vector<std::string> paths;
  for (int i = 1; i < argc; ++i) {
    const std::string a = argv[i];
    if (a == "-v") {
      verbose = true;
    } else if (a == "--corpus") {
      corpus = true;
    } else if (a == "--every" && i + 1 < argc) {
      every = std::max(1, std::atoi(argv[++i]));
    } else if (a == "--min-valid" && i + 1 < argc) {
      min_valid = std::atof(argv[++i]);
    } else if (a == "--only" && i + 1 < argc) {
      only = argv[++i];
    } else if (a == "--save" && i + 1 < argc) {
      save = argv[++i];
    } else if (a == "--history") {
      history = true;
    } else if (a == "--meshes" && i + 1 < argc) {
      mesh_error = std::max(0.0, std::atof(argv[++i]));
    } else {
      paths.push_back(a);
    }
  }
  try {
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
    Totals totals;
    totals.every = every;
    totals.only = only;
    totals.save = save;
    totals.with_history = history;
    totals.mesh_error = mesh_error;
    for (std::size_t i = 0; i < files.size(); ++i) {
      report_file(file_id(i), files[i], verbose, totals);
    }
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
