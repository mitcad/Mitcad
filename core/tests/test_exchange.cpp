// SPDX-License-Identifier: MIT
// Base features, import and export through the document API with OCCT.
//
//   test_exchange <dir>                 self test; writes its files to <dir>:
//                                       round trips through STEP, IGES, BRep,
//                                       STL and OBJ, base features in project
//                                       files, base features from shapes,
//                                       the bodies of a small .f3d file and
//                                       3MF meshes for slicers
//   test_exchange --write-f3d <file>    writes that .f3d file (CLI tests)
//   test_exchange --write-ipt <file>    writes a small .ipt part file
//                                       (mitcad_ipt::testdata; CLI and UI
//                                       tests)
//   test_exchange --write-ipt-design <file>
//                                       writes a small .ipt part file with
//                                       a design (a parameter, a sketch and
//                                       an extrusion; CLI tests)
//   test_exchange --corpus [dir]        imports every .f3d/.f3z under dir
//                                       (default MITCAD_F3D_CORPUS, else
//                                       ~/f3d-corpus) bodies only; exits 77
//                                       (skipped) when it is missing
//   --every N                           only every Nth file (quick runs)
//   --min-valid P                       fail when fewer than P % of the
//                                       imported solids are valid
//   --jobs N, --memory SIZE,            each file in a child process of its
//   --file-timeout S                    own, N at a time (parallel_runs.hpp;
//                                       --jobs 1: all in this process)
//
// The corpus run checks per file that every body A4's reader finds is
// imported or reported as not built, that each imported body has the
// volume of its build, and that the saved project opens again to the same
// bodies. Files are named by position (f01, ...), as in
// core/f3d/CORPUS_REPORT.md.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <filesystem>
#include <fstream>
#include <map>
#include <string>
#include <vector>

#include "bridge/exchange.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/io/body.hpp"
#include "mitcad_bridge/brep_import.h"
#include "mitcad_bridge/kernel/exchange.h"
#include "mitcad_bridge/lib.h"
#include "parallel_runs.hpp"
#include "rust/cxx.h"

namespace {

namespace fs = std::filesystem;

constexpr double kPi = 3.14159265358979323846;
constexpr int kSkipped = 77;
// The block of block(): 60 x 40 x 20 with a 5 mm fillet on one vertical edge.
const double kBlockVolume = 48000.0 - 20.0 * (25.0 - kPi * 25.0 / 4.0);
const char* const kRegion = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_exchange.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool near(double a, double b, double relative = 1e-9) {
  return std::abs(a - b) <= relative * std::max(1.0, std::abs(b));
}

bool contains(const std::string& text, const std::string& part) {
  return text.find(part) != std::string::npos;
}

std::string json_path(const fs::path& path) {
  std::string text = path.generic_string();
  std::string out;
  for (const char c : text) {
    if (c == '"' || c == '\\') {
      out += '\\';
    }
    out += c;
  }
  return "\"" + out + "\"";
}

std::string run(mitcad::Document& document, const std::string& json) {
  try {
    return std::string(document.command(json));
  } catch (const rust::Error& error) {
    std::fprintf(stderr, "command failed: %s\n  %s\n", json.c_str(), error.what());
    ++failures;
    return {};
  }
}

template <class F>
bool throws_with(F&& f, const char* text) {
  try {
    f();
  } catch (const rust::Error& error) {
    if (contains(error.what(), text)) {
      return true;
    }
    std::fprintf(stderr, "unexpected error: %s\n", error.what());
    return false;
  }
  std::fprintf(stderr, "no error, expected: %s\n", text);
  return false;
}

double volume(const mitcad::Document& document, const std::string& uid) {
  const auto body = document.body_shape(uid);
  return body ? mitcad::geometry::volume(*body) : -1.0;
}

// The block (Sketch1 F1, Extrude1 F2, Fillet1 F3) named Bracket.
rust::Box<mitcad::Document> block() {
  auto document = mitcad::new_document();
  run(*document, R"({"cmd": "sketch.create"})");
  run(*document, R"({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40})");
  run(*document, std::string(R"({"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": ")") +
                     kRegion + R"("}], "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}})");
  run(*document, R"({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
                      "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 5}})");
  run(*document, R"({"cmd": "rename_body", "uid": "F2.b0", "name": "Bracket"})");
  return document;
}

// Exports the block to each B-rep format and imports it back: the same
// solid, faces named by index, and a fillet on an imported edge works.
void test_brep_round_trips(const fs::path& dir) {
  auto document = block();
  CHECK(near(volume(*document, "F2.b0"), kBlockVolume));
  int feature = 4;
  for (const char* extension : {"step", "igs", "brep"}) {
    const fs::path file = dir / (std::string("block.") + extension);
    const std::string exported = run(*document, R"({"cmd": "export", "path": )" + json_path(file) +
                                                    R"(, "bodies": ["F2.b0"]})");
    CHECK(contains(exported, R"("bodies":["F2.b0"])"));
    const std::string imported = run(*document, R"({"cmd": "import_file", "path": )" + json_path(file) + "}");
    const std::string uid = "F" + std::to_string(feature++) + ".b0";
    CHECK(contains(imported, "\"uid\":\"" + uid + "\""));
    CHECK(contains(imported, R"("kind":"solid")"));
    CHECK(near(volume(*document, uid), kBlockVolume, 1e-9));
    const auto body = document->body_shape(uid);
    CHECK(body && body->face_count() == 7);
    if (body) {
      const std::string first = uid.substr(0, uid.find('.')) + ":import(0)";
      CHECK(body->find_faces(first).size() == 1);
      CHECK(body->find_faces(uid.substr(0, uid.find('.')) + ":import(6)").size() == 1);
    }
  }
  // STEP keeps the name; the BRep body is named after the file.
  const std::string bodies(document->query(R"({"query": "bodies"})"));
  CHECK(contains(bodies, R"j({"name":"Bracket (2)","uid":"F4.b0"})j"));
  CHECK(contains(bodies, R"({"name":"block","uid":"F6.b0"})"));

  // A fillet on an edge of the imported STEP body.
  const auto imported = document->body_shape("F4.b0");
  std::string edge;
  for (int i = 0; imported && i < imported->edge_count() && edge.empty(); ++i) {
    // A straight edge of length 20 between two planes: a vertical corner.
    const std::string name = imported->edge_name(i);
    if (!name.empty() && imported->find_edges(name).size() == 1) {
      edge = name;
    }
  }
  CHECK(!edge.empty());
  run(*document, R"({"cmd": "add_feature", "def": {"type": "fillet", "body": "F4.b0", "edges": [")" + edge +
                     R"("], "radius": 1}})");
  CHECK(contains(std::string(document->query(R"({"query": "feature", "uid": "F7"})")), R"("status":"ok")"));
  CHECK(volume(*document, "F4.b0") < kBlockVolume);

  // The project file keeps the imported bodies: it opens to the same shapes.
  const std::string saved(document->to_json());
  CHECK(contains(saved, R"("type": "base")"));
  CHECK(contains(saved, R"("compression": "zlib")"));
  CHECK(contains(saved, R"("source": "block.step")"));
  auto reloaded = mitcad::load_document(saved);
  run(*reloaded, R"({"cmd": "recompute"})");
  for (const char* uid : {"F2.b0", "F4.b0", "F5.b0", "F6.b0"}) {
    CHECK(near(volume(*reloaded, uid), volume(*document, uid), 1e-12));
  }
  CHECK(std::string(reloaded->to_json()) == saved);
  CHECK(std::string(reloaded->report(false)) == std::string(document->report(false)));

  // Errors.
  CHECK(throws_with([&] { document->command(R"({"cmd": "import_file", "path": "missing.step"})"); },
                    "missing.step"));
  CHECK(throws_with([&] { document->command(R"({"cmd": "import_file", "path": "model.dwg"})"); },
                    "unknown file type"));
  CHECK(throws_with([&] { document->command(R"({"cmd": "export", "path": "out.dwg"})"); },
                    "unknown file type"));
}

// STL and OBJ: mesh bodies, refinement and units.
void test_mesh_round_trips(const fs::path& dir) {
  auto document = block();
  const fs::path coarse = dir / "coarse.stl";
  const fs::path fine = dir / "fine.stl";
  const fs::path obj = dir / "block.obj";
  run(*document, R"({"cmd": "export", "path": )" + json_path(coarse) + R"(, "refinement": "low"})");
  run(*document, R"({"cmd": "export", "path": )" + json_path(fine) +
                     R"(, "refinement": {"deviation": 0.002, "angle": 0.05}, "ascii": true})");
  run(*document, R"({"cmd": "export", "path": )" + json_path(obj) + "}");
  CHECK(fs::file_size(fine) > fs::file_size(coarse));

  const std::string imported = run(*document, R"({"cmd": "import_file", "path": )" + json_path(fine) + "}");
  CHECK(contains(imported, R"("kind":"mesh")"));
  CHECK(near(volume(*document, "F4.b0"), kBlockVolume, 1e-5));
  run(*document, R"({"cmd": "import_file", "path": )" + json_path(obj) + R"(, "unit_mm": 10})");
  CHECK(near(volume(*document, "F5.b0"), kBlockVolume * 1000.0, 1e-4));
  const std::string bodies(document->query(R"({"query": "bodies", "properties": true})"));
  CHECK(contains(bodies, R"("kind":"mesh")"));
  CHECK(contains(bodies, R"("kind":"solid")"));

  // Mesh bodies stay meshes through the project file and go to STL only.
  auto reloaded = mitcad::load_document(std::string(document->to_json()));
  run(*reloaded, R"({"cmd": "recompute"})");
  CHECK(near(volume(*reloaded, "F4.b0"), volume(*document, "F4.b0"), 1e-12));
  CHECK(throws_with(
      [&] { document->command(R"({"cmd": "export", "path": "mesh.step", "bodies": ["F4.b0"]})"); },
      "is a mesh body"));
  run(*document, R"({"cmd": "export", "path": )" + json_path(dir / "mesh.stl") + R"(, "bodies": ["F4.b0"]})");
  CHECK(fs::exists(dir / "mesh.stl"));
}

// Base features from shapes the caller built (the .f3d import's way):
// new bodies with names and colours, and a cut into a body as a fallback
// replay would do it.
void test_base_features_from_shapes(const fs::path& dir) {
  const fs::path f3d = dir / "bodies.f3d";
  mitcad::bridge::f3d_write_test_file(f3d.string());
  auto shapes = mitcad::bridge::new_shape_list();
  const rust::Vec<mitcad::bridge::F3dBody> bodies = mitcad::bridge::f3d_bodies(f3d.string(), false, false, *shapes);
  CHECK(bodies.size() == 2 && shapes->size() == 2);
  if (bodies.size() != 2 || shapes->size() != 2) {
    return;
  }
  CHECK(bodies[0].built && bodies[0].solid && bodies[0].valid);
  CHECK(near(bodies[0].volume, 1000.0, 1e-9));
  CHECK(near(bodies[1].volume, kPi * 100.0 * 20.0, 1e-6));

  auto document = block();
  const std::string added(document->add_base_feature(
      *shapes, R"({"name": "Stored bodies", "source": "bodies.f3d",
                   "bodies": [{"name": "Cube", "color": [1, 0.5, 0]}, {"name": "Cylinder"}]})"));
  CHECK(contains(added, R"("name":"Stored bodies")"));
  CHECK(contains(added, R"({"kind":"solid","name":"Cube","uid":"F4.b0"})"));
  CHECK(contains(added, R"({"kind":"solid","name":"Cylinder","uid":"F4.b1"})"));
  CHECK(near(volume(*document, "F4.b0"), 1000.0, 1e-9));
  CHECK(near(volume(*document, "F4.b1"), kPi * 100.0 * 20.0, 1e-6));
  // Faces are numbered over both bodies: the cube has 6.
  const auto cylinder = document->body_shape("F4.b1");
  CHECK(cylinder && cylinder->find_faces("F4:import(6)").size() == 1);

  // The colour goes to STEP and comes back.
  const fs::path step = dir / "cube.step";
  run(*document, R"({"cmd": "export", "path": )" + json_path(step) + R"(, "bodies": ["Cube"]})");
  run(*document, R"({"cmd": "import_file", "path": )" + json_path(step) + "}");
  const std::string feature(document->query(R"({"query": "feature", "uid": "F5"})"));
  CHECK(contains(feature, R"("color":[1.0,0.5,0.0],"name":"Cube")"));
  CHECK(contains(std::string(document->query(R"({"query": "bodies"})")),
                 R"j({"name":"Cube (2)","uid":"F5.b0"})j"));

  // The cube cut out of the block's corner, as a fallback replay of a cut.
  auto cube = mitcad::bridge::new_shape_list();
  cube->push(shapes->at(0));
  document->add_base_feature(*cube, R"({"operation": "cut", "participants": ["F2.b0"]})");
  CHECK(near(volume(*document, "F2.b0"), kBlockVolume - 1000.0, 1e-9));
  CHECK(document->body_shape("F6.b0") == nullptr);
  CHECK(throws_with([&] { document->add_base_feature(*cube, R"({"operation": "join", "participants": ["F1.b0"]})"); },
                    "does not create bodies"));
  CHECK(throws_with([&] { document->add_base_feature(*cube, R"({"bodies": [{}, {}]})"); }, "2 body options for 1"));

  // The .f3d bodies without history, as mitcad-cli import-f3d does it.
  auto imported = mitcad::new_document();
  const std::string report(imported->import_f3d(f3d.string(), "{}"));
  CHECK(contains(report, R"("file":"bodies.f3d")"));
  CHECK(contains(report, R"("skipped":[])"));
  CHECK(contains(report, R"("source":"FusionAssetName[Active]/Breps.BlobParts/BREP.mitcad-test-cube.smb#)"));
  CHECK(near(volume(*imported, "F1.b0"), 1000.0, 1e-9));
  CHECK(near(volume(*imported, "F2.b0"), kPi * 100.0 * 20.0, 1e-6));
  const std::string text(imported->import_f3d(f3d.string(), R"({"text": true})"));
  CHECK(contains(text, "Imported 2 bodies of bodies.f3d without history (2 solids, 2 valid; 0 not built)"));
  CHECK(contains(text, "F3 Body3 (F3.b0): valid solid, volume 1000.000 mm3, 6 faces from "
                       "FusionAssetName[Active]/Breps.BlobParts/BREP.mitcad-test-cube.smb#"));
  CHECK(throws_with([&] { imported->import_f3d((dir / "none.f3d").string(), "{}"); }, "none.f3d"));
  CHECK(throws_with([&] { imported->import_f3d(f3d.string(), R"({"timeline": true})"); }, "unknown field"));
}

// The bodies stored in an .ipt part file (mitcad#60): Mitcad's own test
// part (a cube and a cylinder in B-rep records of their own, inches,
// aluminium), compared with a STEP file of its bodies, and files that are
// not part files.
void test_ipt_import(const fs::path& dir) {
  const fs::path ipt = dir / "part.ipt";
  mitcad::bridge::ipt_write_test_file(ipt.string());
  auto document = mitcad::new_document();
  const std::string text(document->import_ipt(ipt.string(), R"({"text": true})"));
  CHECK(contains(text, "Imported 2 bodies of part.ipt (2 solids, 2 valid; 0 not built)"));
  CHECK(contains(text, "part number: MITCAD-TEST-1; material: Aluminum (Mitcad's aluminum); units: in"));
  CHECK(contains(text, "F1 Body1 (F1.b0): valid solid, volume 1000.000 mm3, area 600.000 mm2, 6 faces from "
                       "PmBRepSegment#1/1"));
  CHECK(contains(text, "F2 Body2 (F2.b0): valid solid, volume 6283.185 mm3"));
  CHECK(near(volume(*document, "F1.b0"), 1000.0, 1e-9));
  CHECK(near(volume(*document, "F2.b0"), kPi * 100.0 * 20.0, 1e-6));
  const std::string state(document->query(R"({"query": "document"})"));
  CHECK(contains(state, R"("length":"in")"));
  CHECK(contains(state, R"("undo":"Import part.ipt")"));
  CHECK(contains(std::string(document->query(R"({"query": "bodies"})")), R"("material":"aluminum")"));
  // One undo step takes it all back: the bodies, the units.
  run(*document, R"({"cmd": "undo"})");
  CHECK(document->body_shape("F1.b0") == nullptr);
  CHECK(contains(std::string(document->query(R"({"query": "document"})")), R"("length":"mm")"));

  // The bodies written to STEP are the reference: they match.
  run(*document, R"({"cmd": "redo"})");
  const fs::path step = dir / "part.step";
  run(*document, R"({"cmd": "export", "path": )" + json_path(step) + "}");
  auto checked = mitcad::new_document();
  const std::string report(checked->import_ipt(ipt.string(), R"({"reference": )" + json_path(step) + "}"));
  CHECK(contains(report, R"("pass": true)"));
  CHECK(contains(report, R"("step_solids": 2)"));
  CHECK(contains(report, R"("format": "ipt")"));
  // Against the cube alone, the cylinder has no counterpart.
  const fs::path cube = dir / "part-cube.step";
  run(*document, R"({"cmd": "export", "path": )" + json_path(cube) + R"(, "bodies": ["F1.b0"]})");
  auto partial = mitcad::new_document();
  const std::string missing(
      partial->import_ipt(ipt.string(), R"({"text": true, "reference": )" + json_path(cube) + "}"));
  CHECK(contains(missing, "reference part-cube.step: 1 STEP solids"));
  CHECK(contains(missing, ": FAILED\n"));
  CHECK(contains(missing, "Body2 (F2.b0) has no STEP solid"));

  // As a document command (the application's import process).
  auto command = mitcad::new_document();
  const std::string result(command->command(R"({"cmd": "import_ipt", "path": )" + json_path(ipt) + "}"));
  CHECK(contains(result, R"("bodies":2)"));
  CHECK(contains(result, R"("design":"MITCAD-TEST-1")"));

  // Into a document with features: its units stay.
  auto existing = block();
  const std::string into(existing->import_ipt(ipt.string(), R"({"text": true})"));
  CHECK(contains(into, "warning: the part's length unit (in) is not applied to a document that has features"));
  CHECK(contains(std::string(existing->query(R"({"query": "document"})")), R"("length":"mm")"));
  CHECK(near(volume(*existing, "F4.b0"), 1000.0, 1e-9));

  // Not part files.
  const fs::path f3d = dir / "bodies.f3d";
  mitcad::bridge::f3d_write_test_file(f3d.string());
  CHECK(throws_with([&] { command->import_ipt(f3d.string(), "{}"); }, "bodies.f3d: not a compound file"));
  CHECK(throws_with([&] { command->import_ipt((dir / "none.ipt").string(), "{}"); }, "none.ipt"));
  CHECK(throws_with([&] { command->import_ipt(ipt.string(), R"({"timeline": true})"); }, "unknown field"));

  // A part with a design (mitcad#60, stages 2 and 3): its parameters, the
  // sketch and the extrusion come in parametric and make the stored cube.
  const fs::path designed = dir / "designed.ipt";
  mitcad::bridge::ipt_write_test_design_file(designed.string());
  auto history = mitcad::new_document();
  const std::string replayed(history->import_ipt(designed.string(), R"({"text": true})"));
  CHECK(contains(replayed, "2 timeline items: 2 parametric, 0 partial, 0 fallback, 0 skipped"));
  CHECK(contains(replayed, "expressions: 5 translated, 5 agree with the stored values"));
  CHECK(contains(replayed, "parameters: 5 imported, 0 as values"));
  CHECK(contains(replayed, "Imported 1 bodies of designed.ipt (1 solids, 1 valid; 0 not built)"));
  CHECK(near(volume(*history, "F2.b0"), 1000.0, 1e-9));
  CHECK(contains(std::string(history->query(R"({"query": "parameters"})")), R"("name":"Side")"));
  // The bodies only, as before.
  auto bodies = mitcad::new_document();
  const std::string plain(bodies->import_ipt(designed.string(), R"({"text": true, "bodies_only": true})"));
  CHECK(contains(plain, "F1 Body1 (F1.b0): valid solid, volume 1000.000 mm3"));
}

// The number after "<key>": in a JSON text from `from` on.
double json_number(const std::string& text, const std::string& key, std::size_t from = 0) {
  const std::size_t at = text.find("\"" + key + "\":", from);
  return at == std::string::npos ? -1.0 : std::atof(text.c_str() + at + key.size() + 3);
}

// 3MF for slicers (mitcad#13): the block and a cylinder as the parts of one
// object; each body's mesh closed by the kernel and within the
// refinement's deviation (times the area) of the body's volume.
void test_3mf_export(const fs::path& dir) {
  auto document = block();
  run(*document, R"({"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [100, 0],
                      "diameter": 20, "height": 30, "operation": "new_body"}})");
  const fs::path file = dir / "parts.3mf";
  for (const char* refinement : {R"("low")", R"("high")", R"({"deviation": 0.002, "angle": 0.05})"}) {
    const std::string result =
        run(*document, R"({"cmd": "export", "path": )" + json_path(file) + R"(, "refinement": )" + refinement + "}");
    CHECK(contains(result, R"("format":"3mf")"));
    CHECK(contains(result, R"("skipped":[])"));
    double deviation = 0.1;
    if (std::string(refinement) != R"("low")") {
      deviation = std::string(refinement) == R"("high")" ? 0.01 : 0.002;
    }
    std::size_t at = 0;
    for (const char* uid : {"F2.b0", "F4.b0"}) {
      at = result.find(std::string(R"({"body":")") + uid, at);
      CHECK(at != std::string::npos);
      const auto body = document->body_shape(uid);
      if (at == std::string::npos || !body) {
        continue;
      }
      const mitcad::geometry::MassProperties exact = mitcad::geometry::mass_properties(*body);
      const double meshed = json_number(result, "volume", at);
      std::fprintf(stderr, "3MF %s %s: %d triangles, %.4f mm3 of %.4f\n", refinement, uid,
                   static_cast<int>(json_number(result, "triangles", at)), meshed, exact.volume);
      CHECK(json_number(result, "triangles", at) >= 12);
      CHECK(std::abs(meshed - exact.volume) <= exact.area * deviation);
    }
  }
  // A zip package.
  std::ifstream written(file, std::ios::binary);
  char magic[2] = {0, 0};
  CHECK(written.read(magic, 2) && magic[0] == 'P' && magic[1] == 'K');
  // Explicit bodies, colours of the caller's; a mesh body goes as it is.
  const fs::path stl = dir / "bracket-3mf.stl";
  run(*document, R"({"cmd": "export", "path": )" + json_path(stl) + R"(, "bodies": ["Bracket"]})");
  run(*document, R"({"cmd": "import_file", "path": )" + json_path(stl) + "}");
  const std::string some = run(*document, R"({"cmd": "export", "path": )" + json_path(file) +
                                              R"(, "bodies": ["F5.b0", "Bracket"], "colors": {"Bracket": [1, 0, 0]}})");
  CHECK(contains(some, R"("bodies":["F5.b0","F2.b0"])"));
  CHECK(near(json_number(some, "volume"), volume(*document, "F5.b0"), 1e-9));
}

// Sketch geometry to DXF.
void test_sketch_export(const fs::path& dir) {
  auto document = block();
  const fs::path dxf = dir / "sketch.dxf";
  const std::string result =
      run(*document, R"({"cmd": "export_sketch", "sketch": "Sketch1", "path": )" + json_path(dxf) + "}");
  CHECK(contains(result, R"("entities":4)"));
  CHECK(fs::exists(dxf) && fs::file_size(dxf) > 0);
}

// Corpus: every file bodies only.

struct Totals {
  int files = 0;
  int failed = 0;
  int bodies = 0;
  int not_built = 0;
  int solids = 0;
  int valid_solids = 0;
  int sheets = 0;
};

// The numbers after each `"key":` in a JSON text, in order.
std::vector<double> numbers_of(const std::string& json, const std::string& key) {
  std::vector<double> out;
  const std::string pattern = "\"" + key + "\":";
  for (std::size_t at = json.find(pattern); at != std::string::npos; at = json.find(pattern, at + 1)) {
    out.push_back(std::strtod(json.c_str() + at + pattern.size(), nullptr));
  }
  return out;
}

std::size_t count_of(const std::string& json, const std::string& part) {
  std::size_t count = 0;
  for (std::size_t at = json.find(part); at != std::string::npos; at = json.find(part, at + 1)) {
    ++count;
  }
  return count;
}

void import_corpus_file(const std::string& id, const std::string& path, Totals& t) {
  ++t.files;
  const auto start = std::chrono::steady_clock::now();
  std::size_t expected = 0;
  try {
    for (const auto& body : mitcad::f3d::f3d_read_bodies(path)) {
      expected += body.top_level && !body.history ? 1 : 0;
    }
  } catch (const std::exception& e) {
    ++t.failed;
    std::printf("%s: read failed: %s\n", id.c_str(), e.what());
    return;
  }
  auto document = mitcad::new_document();
  std::string report;
  try {
    report = std::string(document->import_f3d(path, "{}"));
  } catch (const std::exception& e) {
    // A file without top-level bodies (an assembly of references) has
    // nothing to import.
    if (expected == 0 && contains(e.what(), "no bodies to import")) {
      std::printf("%s: no top-level bodies\n", id.c_str());
      return;
    }
    ++t.failed;
    std::printf("%s: import failed: %s\n", id.c_str(), e.what());
    return;
  }
  const std::size_t skipped_at = report.find(R"("skipped":)");
  const std::string imported = report.substr(0, skipped_at);
  const std::vector<double> volumes = numbers_of(imported, "volume");
  const std::vector<double> areas = numbers_of(imported, "area");
  const std::size_t not_built = count_of(report.substr(skipped_at), R"("error":)");
  bool ok = volumes.size() + not_built == expected;
  if (!ok) {
    std::printf("%s: %zu bodies read, %zu imported, %zu not built\n", id.c_str(), expected, volumes.size(),
                not_built);
  }
  // Each body has the volume of its build, also after saving and opening.
  auto reloaded = mitcad::load_document(std::string(document->to_json()));
  reloaded->command(R"({"cmd": "recompute"})");
  int valid = 0;
  int solid_count = 0;
  std::size_t at = 0;
  for (std::size_t i = 0; i < volumes.size(); ++i) {
    // Items are objects with sorted keys: ..."solid":<b>,"source":...,"valid":<b>,"volume":<v>}.
    at = imported.find(R"("solid":)", at + 1);
    const bool is_solid = imported.compare(at, 13, R"("solid":true,)") == 0;
    const std::size_t valid_at = imported.find(R"("valid":)", at);
    const bool is_valid = imported.compare(valid_at, 12, R"("valid":true)") == 0;
    solid_count += is_solid ? 1 : 0;
    valid += is_solid && is_valid ? 1 : 0;
    // Solids by volume, sheets by area (the build reports no volume).
    const std::string uid = "F" + std::to_string(i + 1) + ".b0";
    const double built = is_solid ? volumes[i] : areas[i];
    const double tolerance = 1e-9 * std::max(1.0, std::abs(built));
    for (const auto* doc : {&*document, &*reloaded}) {
      const auto body = doc->body_shape(uid);
      const double v = !body ? -1.0
                       : is_solid ? mitcad::geometry::volume(*body)
                                  : mitcad::geometry::mass_properties(*body).area;
      if (std::abs(v - built) > tolerance) {
        ok = false;
        std::printf("%s: %s %s %.9g, built %.9g\n", id.c_str(), uid.c_str(), is_solid ? "volume" : "area", v,
                    built);
        break;
      }
    }
  }
  t.bodies += static_cast<int>(volumes.size());
  t.not_built += static_cast<int>(not_built);
  t.solids += solid_count;
  t.valid_solids += valid;
  t.sheets += static_cast<int>(volumes.size()) - solid_count;
  t.failed += ok ? 0 : 1;
  const double ms =
      std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
  std::printf("%s: %zu bodies, %d solids (%d valid), %zu sheets, %zu not built, %.0f ms%s\n", id.c_str(),
              volumes.size(), solid_count, valid, volumes.size() - static_cast<std::size_t>(solid_count),
              not_built, ms, ok ? "" : " FAILED");
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
std::map<std::string, double> totals_of(const Totals& t) {
  return {{"files", t.files},   {"failed", t.failed},           {"bodies", t.bodies}, {"not_built", t.not_built},
          {"solids", t.solids}, {"valid_solids", t.valid_solids}, {"sheets", t.sheets}};
}

void add_totals(const std::map<std::string, double>& in, Totals& t) {
  const auto get = [&](const char* key) {
    const auto at = in.find(key);
    return at == in.end() ? 0 : static_cast<int>(at->second);
  };
  t.files += get("files");
  t.failed += get("failed");
  t.bodies += get("bodies");
  t.not_built += get("not_built");
  t.solids += get("solids");
  t.valid_solids += get("valid_solids");
  t.sheets += get("sheets");
}

int corpus(const fs::path& dir, int every, double min_valid, const mitcad::runs::Settings& parallel,
           const std::string& self) {
  std::error_code ec;
  if (dir.empty() || !fs::is_directory(dir, ec)) {
    std::printf("corpus not found; skipped\n");
    return kSkipped;
  }
  std::vector<std::string> files;
  for (const auto& entry : fs::recursive_directory_iterator(dir)) {
    const auto extension = entry.path().extension().string();
    if (entry.is_regular_file() && (extension == ".f3d" || extension == ".f3z")) {
      files.push_back(entry.path().string());
    }
  }
  std::sort(files.begin(), files.end());
  Totals totals;
  std::vector<std::string> ids;
  std::vector<mitcad::runs::Command> commands;
  std::vector<double> weights;
  for (std::size_t i = 0; i < files.size(); i += static_cast<std::size_t>(every)) {
    const std::string number = std::to_string(i + 1);
    const std::string id = (number.size() < 2 ? "f0" : "f") + number;
    if (parallel.jobs <= 1) {
      import_corpus_file(id, files[i], totals);
    } else {
      // Each file in a child process of its own (mitcad#70).
      ids.push_back(id);
      weights.push_back(mitcad::runs::file_weight(files[i]));
      commands.push_back({self, "--corpus-file", files[i], id});
    }
  }
  mitcad::runs::run_all(commands, parallel, [&](std::size_t i, const mitcad::runs::Outcome& outcome) {
    std::string text;
    std::map<std::string, double> child;
    const bool finished = mitcad::runs::split_totals(outcome.output, text, child);
    std::fputs(text.c_str(), stdout);
    std::fputs(mitcad::runs::describe_failure(ids[i], outcome, finished).c_str(), stdout);
    if (finished) {
      add_totals(child, totals);
    } else {
      ++totals.files;
      ++totals.failed;
    }
    std::fflush(stdout);
  }, weights);
  std::printf("files %d (failed %d), bodies imported %d (not built %d): solids %d (valid %d), sheets %d\n",
              totals.files, totals.failed, totals.bodies, totals.not_built, totals.solids, totals.valid_solids,
              totals.sheets);
  const double valid_percent = totals.solids > 0 ? 100.0 * totals.valid_solids / totals.solids : 100.0;
  if (valid_percent < min_valid) {
    std::printf("only %.1f %% of the solids are valid (required %.1f %%)\n", valid_percent, min_valid);
    return 1;
  }
  return totals.failed == 0 ? 0 : 1;
}

} // namespace

int main(int argc, char** argv) {
  mitcad::io::silence_occt_messages();
  std::vector<std::string> args(argv + 1, argv + argc);
  try {
    if (args.size() == 2 && args[0] == "--write-f3d") {
      mitcad::bridge::f3d_write_test_file(args[1]);
      return 0;
    }
    if (args.size() == 2 && args[0] == "--write-ipt") {
      mitcad::bridge::ipt_write_test_file(args[1]);
      return 0;
    }
    if (args.size() == 2 && args[0] == "--write-ipt-design") {
      mitcad::bridge::ipt_write_test_design_file(args[1]);
      return 0;
    }
    if (args.size() == 3 && args[0] == "--corpus-file") {
      // A child of a parallel run (mitcad#70): one file, then its totals.
      Totals totals;
      import_corpus_file(args[2], args[1], totals);
      mitcad::runs::print_totals(totals_of(totals));
      return 0;
    }
    std::string jobs;
    std::string memory;
    std::string timeout;
    mitcad::runs::take_options(args, jobs, memory, timeout);
    if (!args.empty() && args[0] == "--corpus") {
      fs::path dir;
      int every = 1;
      double min_valid = 0.0;
      for (std::size_t i = 1; i < args.size(); ++i) {
        if (args[i] == "--every" && i + 1 < args.size()) {
          every = std::max(1, std::atoi(args[++i].c_str()));
        } else if (args[i] == "--min-valid" && i + 1 < args.size()) {
          min_valid = std::atof(args[++i].c_str());
        } else {
          dir = args[i];
        }
      }
      return corpus(dir.empty() ? default_corpus() : dir, every, min_valid,
                    mitcad::runs::settings(jobs, memory, timeout), mitcad::runs::executable_path(argv[0]));
    }
    if (args.size() != 1) {
      std::fprintf(stderr, "usage: test_exchange <dir> | --write-f3d <file> | --write-ipt <file> | "
                           "--corpus [dir] [--every N] "
                           "[--min-valid P] [--jobs N] [--memory SIZE] [--file-timeout S]\n");
      return 2;
    }
    const fs::path dir = args[0];
    fs::create_directories(dir);
    test_brep_round_trips(dir);
    test_mesh_round_trips(dir);
    test_base_features_from_shapes(dir);
    test_sketch_export(dir);
    test_3mf_export(dir);
    test_ipt_import(dir);
  } catch (const std::exception& e) {
    std::fprintf(stderr, "test_exchange: %s\n", e.what());
    return 2;
  }
  if (failures == 0) {
    std::printf("test_exchange: all checks passed\n");
  }
  return failures == 0 ? 0 : 1;
}
