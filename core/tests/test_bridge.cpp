// SPDX-License-Identifier: MIT
// End-to-end check of the C++ -> Rust model -> C++/OCCT chain without the
// UI: JSON commands in, shapes and names out.

#include <array>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_PointsToBSplineSurface.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array2.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Vec.hxx>

#include "bridge/analysis.hpp"
#include "bridge/memory.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "mitcad_bridge/lib.h"
#include "mitcad_bridge/memory.h"
#include "rust/cxx.h"

// The Rust panic hook's sink and a test panic (core/ffi/src/panic_note.rs).
extern "C" void mitcad_set_panic_sink(void (*sink)(const char* message, std::size_t length));
extern "C" void mitcad_test_panic_note();

namespace {

constexpr double kPi = 3.14159265358979323846;
int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_bridge.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool near(double a, double b) { return std::abs(a - b) <= 1e-6 * std::max(1.0, std::abs(b)); }

// Removed by a fillet of radius r along one straight edge, per unit length.
double fillet_loss(double r) { return r * r - kPi * r * r / 4.0; }

bool contains(const std::string& text, const std::string& part) {
  return text.find(part) != std::string::npos;
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

// Runs a command; returns its JSON result.
std::string run(mitcad::Document& document, const std::string& json) {
  try {
    return std::string(document.command(json));
  } catch (const rust::Error& error) {
    std::fprintf(stderr, "command failed: %s\n  %s\n", json.c_str(), error.what());
    ++failures;
    return {};
  }
}

double body_volume(const mitcad::Document& document, const char* uid = "F2.b0") {
  const auto body = document.body_shape(uid);
  return body ? mitcad::geometry::volume(*body) : -1.0;
}

int body_count(const mitcad::Document& document) {
  const std::string bodies(document.query(R"({"query": "bodies"})"));
  int count = 0;
  for (std::size_t at = bodies.find("\"uid\""); at != std::string::npos;
       at = bodies.find("\"uid\"", at + 1)) {
    ++count;
  }
  return count;
}

// The straight vertical edge standing at (x, y), or a null edge.
TopoDS_Edge vertical_edge_at(const TopoDS_Shape& shape, double x, double y) {
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.X() - x) < 1e-6 && std::abs(a.Y() - y) < 1e-6 && std::abs(b.X() - x) < 1e-6 &&
        std::abs(b.Y() - y) < 1e-6 && std::abs(a.Z() - b.Z()) > 1) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

bool has_sharp_vertical_edge_at(const mitcad::Document& document, double x, double y,
                                const char* uid = "F2.b0") {
  const auto body = document.body_shape(uid);
  return body && !vertical_edge_at(body->occt(), x, y).IsNull();
}

const std::string kRectangle = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
// The vertical edge at corner 1 (x + width, y) of a rectangle extruded by `feature`.
std::string corner1(const std::string& feature) {
  return "E{" + feature + ":side(c1[c4,c2])|" + feature + ":side(c2[c1,c3])}";
}

std::string extrude(const std::string& sketch, const std::string& region, double distance,
                    const std::string& operation, const std::string& participants = "[]") {
  return R"({"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": ")" + sketch +
         R"(", "region": ")" + region + R"("}], "extent": {"type": "distance", "distance": )" +
         std::to_string(distance) + R"(}, "operation": ")" + operation +
         R"(", "participants": )" + participants + "}}";
}

std::string fillet(const std::string& edges, double radius, const std::string& body = "F2.b0") {
  return R"({"cmd": "add_feature", "def": {"type": "fillet", "body": ")" + body +
         R"(", "edges": [)" + edges + R"(], "radius": )" + std::to_string(radius) + "}}";
}

std::string quoted(const std::string& text) { return "\"" + text + "\""; }

std::string set(const std::string& name, double value) {
  return R"({"cmd": "set_parameter", "name": ")" + name + R"(", "value": )" +
         std::to_string(value) + "}";
}

std::string sketch_rectangle(const std::string& sketch, double x, double y, double w, double h) {
  return R"({"cmd": "sketch.add_rectangle", "sketch": ")" + sketch + R"(", "corner": [)" +
         std::to_string(x) + "," + std::to_string(y) + R"(], "width": )" + std::to_string(w) +
         R"(, "height": )" + std::to_string(h) + "}";
}

std::string sketch_circle(const std::string& sketch, double x, double y, double d) {
  return R"({"cmd": "sketch.add_circle", "sketch": ")" + sketch + R"(", "center": [)" +
         std::to_string(x) + "," + std::to_string(y) + R"(], "diameter": )" + std::to_string(d) +
         "}";
}

void test_block_fillet_and_dimension_change() {
  auto document = mitcad::new_demo_document(); // Sketch1 (F1), Extrude1 (F2), Body1
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0));

  // The UI names a picked edge through the body shape.
  const auto body = document->body_shape("F2.b0");
  const auto name = body->name_of_edge(vertical_edge_at(body->occt(), 60, 0));
  CHECK(name && *name == corner1("F2"));

  const double r = 3;
  const double removed = fillet_loss(r) * 20.0;
  CHECK(contains(run(*document, fillet(quoted(corner1("F2")), r)), R"("uid":"F3")"));
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 - removed));

  // Widen the sketch: the fillet moves with corner 1 to x = 80.
  CHECK(contains(run(*document, set("d1", 80)), R"("recomputed":3)"));
  CHECK(near(body_volume(*document), 80.0 * 40.0 * 20.0 - removed));
  CHECK(!has_sharp_vertical_edge_at(*document, 80, 0));
  CHECK(has_sharp_vertical_edge_at(*document, 0, 0));
  CHECK(has_sharp_vertical_edge_at(*document, 80, 40));
  CHECK(contains(std::string(document->query(R"({"query": "parameters"})")),
                 R"({"comment":"Fillet1 radius","dependencies":[],"expression":"3 mm","favorite":false,)"
                 R"("kind":"model","name":"d4","owner":"F3","text":"3 mm","unit":"mm","value":3.0})"));

  // A failing fillet (wider than the 40 mm face) keeps the unfilleted body.
  const std::string failed = run(*document, set("d4", 50));
  CHECK(contains(failed, R"("error":"Fillet1: )") && contains(failed, "fillet"));
  CHECK(near(body_volume(*document), 80.0 * 40.0 * 20.0));
  // Undo brings the working fillet back from the cache.
  CHECK(contains(run(*document, R"({"cmd": "undo"})"), R"("recomputed":0)"));
  CHECK(near(body_volume(*document), 80.0 * 40.0 * 20.0 - removed));
}

// A classic: a block with a boss joined onto it, a fillet on a boss
// edge, then the boss changes size and the fillet follows.
void test_join_boss_fillet_follows_the_boss() {
  auto document = mitcad::new_demo_document(); // d1, d2, d3
  run(*document, R"({"cmd": "sketch.create"})"); // F3
  run(*document, sketch_rectangle("F3", 10, 10, 20, 10)); // d4, d5
  // From the sketch plane through the block: 10 mm stand above its top.
  run(*document, extrude("F3", kRectangle, 30, "join", R"(["F2.b0"])")); // F4, d6
  CHECK(body_count(*document) == 1);
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 + 20.0 * 10.0 * 10.0));

  // Pick the boss's vertical edge at corner 1 (30, 10), as the UI does.
  const auto body = document->body_shape("F2.b0");
  const auto name = body->name_of_edge(vertical_edge_at(body->occt(), 30, 10));
  CHECK(name && *name == corner1("F4"));
  // The edges where the boss meets the block's top have names too.
  CHECK(body->find_edges("E{F2:end(" + kRectangle + ")|F4:side(c2[c1,c3])}").size() == 1);

  const double r = 2;
  run(*document, fillet(quoted(corner1("F4")), r)); // F5, d7
  CHECK(near(body_volume(*document),
             60.0 * 40.0 * 20.0 + 20.0 * 10.0 * 10.0 - fillet_loss(r) * 10.0));
  CHECK(!has_sharp_vertical_edge_at(*document, 30, 10));

  // Widen the boss: the fillet moves with corner 1 to x = 40.
  CHECK(contains(run(*document, set("d4", 30)), R"("recomputed":3)")); // F3, F4, F5
  CHECK(near(body_volume(*document),
             60.0 * 40.0 * 20.0 + 30.0 * 10.0 * 10.0 - fillet_loss(r) * 10.0));
  CHECK(!has_sharp_vertical_edge_at(*document, 40, 10));
  CHECK(has_sharp_vertical_edge_at(*document, 10, 10));
  CHECK(has_sharp_vertical_edge_at(*document, 40, 20));

  // Raise the boss: the fillet runs along the taller edge.
  CHECK(contains(run(*document, set("d6", 35)), R"("recomputed":2)"));
  CHECK(near(body_volume(*document),
             60.0 * 40.0 * 20.0 + 30.0 * 10.0 * 15.0 - fillet_loss(r) * 15.0));

  // A second fillet on the same body rounds the boss's back top edge.
  const std::string top = "E{F4:end(" + kRectangle + ")|F4:side(c3[c2,c4])}";
  run(*document, fillet(quoted(top), 1)); // F6
  const std::string timeline(document->query(R"({"query": "timeline"})"));
  CHECK(!contains(timeline, R"("status":"error")"));
  CHECK(body_volume(*document) < 60.0 * 40.0 * 20.0 + 30.0 * 10.0 * 15.0 - fillet_loss(r) * 15.0);

  // Undo removes the second fillet, then the change of d6.
  run(*document, R"({"cmd": "undo"})");
  run(*document, R"({"cmd": "undo"})");
  CHECK(near(body_volume(*document),
             60.0 * 40.0 * 20.0 + 30.0 * 10.0 * 10.0 - fillet_loss(r) * 10.0));
}

void test_cut_hole_lost_edge_and_split_body() {
  auto document = mitcad::new_demo_document();
  run(*document, R"({"cmd": "sketch.create"})"); // F3
  run(*document, sketch_circle("F3", 30, 20, 10)); // d4
  run(*document, extrude("F3", "r{c1}", 30, "cut", R"(["F2.b0"])")); // F4, d5
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 - kPi * 25.0 * 20.0));
  // The rim of the hole can be rounded: its edge has a name.
  run(*document, fillet(quoted("E{F2:end(" + kRectangle + ")|F4:side(c1)}"), 1)); // F5
  CHECK(!contains(std::string(document->query(R"({"query": "timeline"})")), R"("status":"error")"));
  for (int i = 0; i < 4; ++i) {
    run(*document, R"({"cmd": "undo"})");
  }
  CHECK(std::string(document->query(R"({"query": "document"})")).find(R"("features":2)") !=
        std::string::npos);

  // A notch into the front face, away from corner 1 at (60, 0).
  run(*document, R"({"cmd": "sketch.create"})"); // F3
  run(*document, sketch_rectangle("F3", 45, -5, 5, 10)); // d4, d5
  run(*document, extrude("F3", kRectangle, 30, "cut", R"(["F2.b0"])")); // F4, d6
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 - 5.0 * 5.0 * 20.0));
  run(*document, fillet(quoted(corner1("F2")), 2)); // F5, d7
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 - 5.0 * 5.0 * 20.0 - fillet_loss(2) * 20));

  // Widening the notch past the corner removes the filleted edge.
  const std::string lost = run(*document, set("d4", 20));
  CHECK(contains(lost, "Fillet1: edge " + corner1("F2") + " no longer exists after Extrude2"));
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 - 15.0 * 5.0 * 20.0));

  // A notch through the whole depth splits the body in two:
  // the left piece keeps the body, the right one is a new body of the cut.
  run(*document, set("d4", 5));
  run(*document, set("d5", 60));
  CHECK(body_count(*document) == 2);
  CHECK(near(body_volume(*document, "F2.b0"), 45.0 * 40.0 * 20.0));
  CHECK(near(body_volume(*document, "F4.b0"), 10.0 * 40.0 * 20.0));
  CHECK(contains(std::string(document->query(R"({"query": "bodies"})")),
                 R"({"name":"Body2","uid":"F4.b0"})"));
  // The fillet's corner is on the new body now, so the fillet fails.
  CHECK(contains(std::string(document->query(R"({"query": "timeline"})")), "no longer exists"));
}

void test_chamfer_intersect_and_several_profiles() {
  auto document = mitcad::new_demo_document();
  const std::string top_front = "E{F2:end(" + kRectangle + ")|F2:side(c1[c4,c2])}";
  run(*document, R"({"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0", "edges": [")" +
                     top_front + R"("], "size": {"type": "equal_distance", "distance": 2}}})");
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 - 2.0 * 2.0 / 2.0 * 60.0));
  run(*document, R"({"cmd": "undo"})");

  // Two profiles of one sketch extruded at once make two bodies.
  run(*document, R"({"cmd": "sketch.create"})"); // F3
  run(*document, sketch_circle("F3", 100, 0, 10)); // c1, centre p2
  run(*document, sketch_circle("F3", 130, 0, 10)); // c3, centre p4
  run(*document, R"({"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}, {"sketch": "F3", "region": "r{c3}"}], "extent": {"type": "symmetric", "distance": 5}, "operation": "new_body"}})");
  CHECK(body_count(*document) == 3);
  CHECK(near(body_volume(*document, "F4.b1"), kPi * 25.0 * 10.0));

  // Intersect keeps the part of the block inside a cylinder.
  run(*document, R"({"cmd": "sketch.create"})"); // F5
  run(*document, sketch_circle("F5", 30, 20, 10));
  run(*document, extrude("F5", "r{c1}", 10, "intersect", R"(["F2.b0"])"));
  CHECK(near(body_volume(*document), kPi * 25.0 * 10.0));
}

void test_preview_and_profile_handles() {
  auto document = mitcad::new_demo_document();
  run(*document, R"({"cmd": "sketch.create"})"); // F3
  run(*document, sketch_circle("F3", 30, 20, 10));
  const auto profile = document->profile_shape("F3", "r{c1}");
  CHECK(profile && near(mitcad::geometry::mass_properties(*profile).area, kPi * 25.0));
  CHECK(!document->profile_shape("F3", "r{c9}"));

  const std::string cut = extrude("F3", "r{c1}", 30, "cut", R"(["F2.b0"])");
  const std::string report(document->preview(cut));
  CHECK(contains(report, R"("status":"ok")"));
  const auto previewed = document->preview_body_shape("F2.b0");
  CHECK(previewed && near(mitcad::geometry::volume(*previewed), 48000.0 - kPi * 25.0 * 20.0));
  CHECK(document->preview_tool_shape() != nullptr);
  CHECK(near(body_volume(*document), 48000.0));
  // Committing the previewed command takes its result from the cache.
  CHECK(contains(run(*document, cut), R"("recomputed":0)"));
  CHECK(near(body_volume(*document), 48000.0 - kPi * 25.0 * 20.0));
  CHECK(!document->preview_body_shape("F2.b0"));
}

void test_rejected_commands() {
  auto document = mitcad::new_demo_document();
  CHECK(throws_with([&] { document->command(sketch_rectangle("F1", 0, 0, 0, 5)); },
                    "length must be greater than zero"));
  CHECK(throws_with([&] { document->command(extrude("F1", kRectangle, 0, "new_body")); },
                    "extrude distance must not be zero"));
  CHECK(throws_with([&] { document->command(fillet("", 1)); }, "no edges selected"));
  CHECK(throws_with([&] { document->command(set("missing", 1)); }, "unknown parameter"));
  CHECK(throws_with([&] { document->command(extrude("F2", kRectangle, 5, "join")); },
                    "Extrude1 (F2) is not a sketch"));
  CHECK(throws_with([&] { document->command(extrude("F1", kRectangle, 5, "cut", R"(["F7.b0"])")); },
                    "feature F7 does not exist"));
  CHECK(throws_with([&] { document->command(fillet(quoted("E{F2:side(c1)}"), 1)); },
                    "invalid name"));
  CHECK(throws_with([&] { document->query(R"({"query": "nothing"})"); }, "invalid query"));
  CHECK(std::string(document->query(R"({"query": "document"})")).find(R"("features":2)") !=
        std::string::npos);
}

// The version 1 project file example of V0: a block with a boss joined
// onto it, a hole cut through it, and a fillet on the block's corner 1 and
// the boss's corner 1.
const char* const kProjectFile = R"({
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0, "comment": "Sketch1 width" },
    { "name": "d2", "value": 40.0, "comment": "Sketch1 height" },
    { "name": "d3", "value": 20.0, "comment": "Extrude1 distance" },
    { "name": "d4", "value": 20.0, "comment": "Sketch2 width" },
    { "name": "d5", "value": 10.0, "comment": "Sketch2 height" },
    { "name": "d6", "value": 10.0, "comment": "Sketch2 diameter" },
    { "name": "d7", "value": 30.0, "comment": "Extrude2 distance" },
    { "name": "d8", "value": 30.0, "comment": "Extrude3 distance" },
    { "name": "d9", "value": 2.0, "comment": "Fillet1 radius" }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" }
    ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" },
    { "type": "sketch", "name": "Sketch2", "shapes": [
      { "type": "rectangle", "corner": [10.0, 10.0], "width": "d4", "height": "d5" },
      { "type": "circle", "center": [45.0, 20.0], "diameter": "d6" }
    ] },
    { "type": "extrude", "name": "Extrude2", "sketch": "Sketch2", "profile": 0,
      "distance": "d7", "operation": "join", "body": "Extrude1" },
    { "type": "extrude", "name": "Extrude3", "sketch": "Sketch2", "profile": 1,
      "distance": "d8", "operation": "cut", "body": "Extrude1" },
    { "type": "fillet", "name": "Fillet1", "body": "Extrude1", "edges": [
      { "extrude": "Extrude1", "role": "side", "index": 1 },
      { "extrude": "Extrude2", "role": "side", "index": 1 }
    ], "radius": "d9" }
  ]
})";

// Block, boss of width `boss` and 10 mm above the block, the 10 mm hole and
// the two fillets: 20 mm along the block's corner, 10 mm along the boss's.
double example_volume(double boss) {
  return 60.0 * 40.0 * 20.0 + boss * 10.0 * 10.0 - kPi * 5.0 * 5.0 * 20.0 -
         fillet_loss(2.0) * (20.0 + 10.0);
}

void test_project_file_load_and_recompute() {
  auto document = mitcad::load_document(kProjectFile);
  CHECK(body_count(*document) == 0); // nothing computed yet
  const std::string first = run(*document, R"({"cmd": "recompute"})");
  CHECK(contains(first, R"("recomputed":6)") && contains(first, R"("error":null)"));
  CHECK(body_count(*document) == 1);
  CHECK(near(body_volume(*document), example_volume(20.0)));
  CHECK(!has_sharp_vertical_edge_at(*document, 60, 0));
  CHECK(!has_sharp_vertical_edge_at(*document, 30, 10));
  CHECK(has_sharp_vertical_edge_at(*document, 0, 0));
  CHECK(has_sharp_vertical_edge_at(*document, 10, 10));

  // The loaded model stays parametric: the boss fillet follows corner 1.
  CHECK(contains(run(*document, set("d4", 25)), R"("recomputed":4)"));
  CHECK(near(body_volume(*document), example_volume(25.0)));
  CHECK(!has_sharp_vertical_edge_at(*document, 35, 10));
  CHECK(has_sharp_vertical_edge_at(*document, 35, 20));

  // Saved as version 2; loading it again gives the same file and geometry.
  const std::string json(document->to_json());
  CHECK(contains(json, R"("version": 2)"));
  auto reloaded = mitcad::load_document(json);
  CHECK(std::string(reloaded->to_json()) == json);
  run(*reloaded, R"({"cmd": "recompute"})");
  CHECK(near(body_volume(*reloaded), example_volume(25.0)));

  // Rolled back for an edit, the file can still be written with the marker
  // the edit restores (autosave): the marker at the end is left out.
  run(*reloaded, R"({"cmd": "set_marker", "position": 2})");
  CHECK(contains(std::string(reloaded->to_json()), R"("marker": 2)"));
  CHECK(std::string(reloaded->to_json_with_marker(6)) == json);
}

void test_report() {
  auto document = mitcad::new_demo_document();
  const std::string text(document->report(false));
  CHECK(contains(text, "F2 Extrude1 (extrude): ok"));
  CHECK(contains(text, "Body1 (F2.b0): volume 48000.000 mm3, area 8800.000 mm2, 6 faces, 12 edges"));
  CHECK(contains(text, "bounding box [0.000, 0.000, 0.000] - [60.000, 40.000, 20.000]"));
  const std::string json(document->report(true));
  CHECK(contains(json, R"("volume": 48000.0)"));
}

void test_project_file_errors() {
  CHECK(throws_with([] { mitcad::load_document("{"); }, "not valid JSON"));
  CHECK(throws_with([] { mitcad::load_document(R"({"format": "step"})"); },
                    "not a Mitcad project file"));
  CHECK(throws_with([] { mitcad::load_document(R"({"format": "mitcad", "version": 4})"); },
                    "newer than this Mitcad"));

  const auto edited = [](const std::string& from, const std::string& to) {
    std::string json(kProjectFile);
    json.replace(json.find(from), from.size(), to);
    return json;
  };
  CHECK(throws_with([&] { mitcad::load_document(edited(R"("d9" })", R"("r1" })")); },
                    "features[5] (Fillet1): parameter 'r1' does not exist"));
  // A body is its new body extrude, not the join into it.
  const std::string cutIntoJoin =
      edited(R"("cut", "body": "Extrude1")", R"("cut", "body": "Extrude2")");
  CHECK(throws_with([&] { mitcad::load_document(cutIntoJoin); },
                    "features[4] (Extrude3): 'Extrude2' is not a new_body extrude"));
}

// Construction geometry and analysis shapes for display (F5).
void test_datums_and_analysis_shapes() {
  rust::Box<mitcad::Document> document = mitcad::new_demo_document();
  const std::string top = "F2:end(" + kRectangle + ")";
  run(*document, R"({"cmd": "add_feature", "def": {"type": "construction_plane", "definition":
      {"type": "offset", "plane": {"body": "F2.b0", "face": ")" +
                     top + R"("}, "distance": 5}}})");
  CHECK(contains(std::string(document->query(R"({"query": "datum", "uid": "F3"})")),
                 R"("origin":[0.0,0.0,25.0])"));
  const auto plane = document->analysis_shape(R"({"shape": "datum", "uid": "F3", "size": 10})");
  CHECK(plane && plane->occt().ShapeType() == TopAbs_FACE);
  CHECK(plane && near(mitcad::geometry::mass_properties(*plane).area, 100.0));
  const auto axis = document->analysis_shape(R"({"shape": "datum", "uid": "z"})");
  CHECK(axis && axis->occt().ShapeType() == TopAbs_EDGE);
  const auto section = document->analysis_shape(
      R"({"shape": "section", "plane": {"origin": [0, 0, 10], "normal": [0, 0, 1]}, "body": "F2.b0"})");
  CHECK(section && near(mitcad::geometry::mass_properties(*section).area, 2400.0));
  // The clip keeps the side against the normal: y < 20.
  const auto clipped = document->analysis_shape(
      R"({"shape": "clip", "plane": {"origin": [0, 20, 0], "normal": [0, 1, 0]}, "body": "F2.b0"})");
  CHECK(clipped && near(mitcad::geometry::volume(*clipped), 24000.0));
  const auto above = document->analysis_shape(R"({"shape": "section", "plane": "F3", "body": "F2.b0"})");
  CHECK(!above);
  CHECK(throws_with([&] { document->analysis_shape(R"({"shape": "datum", "uid": "F2"})"); },
                    "Extrude1 (F2) has no datum at the timeline marker"));
  const std::string text(document->analysis(R"({"query": "properties"})", false));
  CHECK(contains(text, "Body1 (F2.b0): Steel, density 7.85 g/cm3"));
  CHECK(contains(text, "mass 0.376800 kg"));
}

// Background computation (P7): a document computed on another thread.

// A thread that runs one job at a time and hands the document back, as the
// application's worker does (app/framework/ModelWorker.hpp).
class TestWorker {
public:
  TestWorker() : m_thread([this] { loop(); }) {}
  ~TestWorker() {
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_stop = true;
    }
    m_wake.notify_all();
    m_thread.join();
  }
  TestWorker(const TestWorker&) = delete;
  TestWorker& operator=(const TestWorker&) = delete;

  void start(std::function<void()> job) {
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_job = std::move(job);
      m_done = false;
    }
    m_wake.notify_all();
  }

  void wait() {
    std::unique_lock<std::mutex> lock(m_mutex);
    m_wake.wait(lock, [this] { return m_done; });
  }

  std::thread::id id() const { return m_thread.get_id(); }

private:
  void loop() {
    std::unique_lock<std::mutex> lock(m_mutex);
    for (;;) {
      m_wake.wait(lock, [this] { return m_stop || m_job; });
      if (m_stop) {
        return;
      }
      std::function<void()> job = std::move(m_job);
      m_job = nullptr;
      lock.unlock();
      job();
      lock.lock();
      m_done = true;
      m_wake.notify_all();
    }
  }

  std::mutex m_mutex;
  std::condition_variable m_wake;
  std::function<void()> m_job;
  bool m_done = true;
  bool m_stop = false;
  std::thread m_thread; // last: it starts when the rest is ready
};

// A recompute on another thread reports its progress here; a cancel from
// here stops it with the model's error, and the document stays as it was,
// ready for the next computation.
void test_job_cancelled_from_another_thread() {
  auto document = mitcad::load_document(kProjectFile); // 6 features, none computed
  const rust::Box<mitcad::JobControl> control = mitcad::new_job_control();
  control->set_test_delay(200); // each evaluation takes 200 ms longer
  document->attach_job(*control);
  std::string error;
  std::thread worker([&] {
    try {
      document->command(R"({"cmd": "recompute"})");
    } catch (const rust::Error& e) {
      error = e.what();
    }
  });
  // Cancel while the second feature is evaluated: the first is kept.
  const auto start = std::chrono::steady_clock::now();
  while (control->progress().evaluated < 1 &&
         std::chrono::steady_clock::now() - start < std::chrono::seconds(60)) {
    std::this_thread::sleep_for(std::chrono::milliseconds(2));
  }
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
  const mitcad::JobProgress progress = control->progress();
  CHECK(progress.position == 1 && progress.total == 6 && progress.evaluated == 1);
  CHECK(std::string(progress.feature) == "Extrude1" && !progress.cancelled);
  control->cancel();
  worker.join();
  CHECK(error == "the computation was cancelled");
  CHECK(control->is_cancelled() && control->progress().cancelled);
  CHECK(control->progress().evaluated == 1);
  document->detach_job();
  CHECK(body_count(*document) == 0); // nothing taken
  // Without the job: the feature evaluated before the cancel is cached.
  CHECK(contains(run(*document, R"({"cmd": "recompute"})"), R"("recomputed":5)"));
  CHECK(near(body_volume(*document), example_volume(20.0)));
}

// A cancel stops a long kernel operation inside it (P7e): a modelled thread
// (OCCT's sweeps and boolean cut, seconds in a debug build) cancelled while
// it is evaluated fails the command within a second, and what it stopped
// short is not cached: the edit without the job evaluates and cuts it.
void test_kernel_operation_cancelled_inside() {
  auto document = mitcad::new_document();
  run(*document, R"({"cmd": "sketch.create"})");
  run(*document, sketch_circle("F1", 0, 0, 10));
  run(*document, extrude("F1", "r{c1}", 20, "new_body"));
  const std::string thread =
      R"j({"type": "thread", "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
          "thread": {"designation": "M10x1.5", "class": "6g"})j";
  run(*document, R"({"cmd": "add_feature", "def": )" + thread + "}}");
  const double rod = kPi * 25.0 * 20.0;
  CHECK(near(body_volume(*document), rod));
  const std::string modelled = R"({"cmd": "edit_feature", "uid": "F3", "def": )" + thread +
                               R"(, "modeled": true}})";

  const rust::Box<mitcad::JobControl> control = mitcad::new_job_control();
  document->attach_job(*control);
  std::string error;
  std::thread worker([&] {
    try {
      document->command(modelled);
    } catch (const rust::Error& e) {
      error = e.what();
    }
  });
  const auto start = std::chrono::steady_clock::now();
  while (std::string(control->progress().feature) != "Thread1" &&
         std::chrono::steady_clock::now() - start < std::chrono::seconds(60)) {
    std::this_thread::sleep_for(std::chrono::milliseconds(2));
  }
  std::this_thread::sleep_for(std::chrono::milliseconds(300));
  const auto requested = std::chrono::steady_clock::now();
  control->cancel();
  worker.join();
  const double after =
      std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - requested).count();
  std::printf("test_bridge: the modelled thread stopped %.0f ms after the cancel\n", after);
  CHECK(error == "the computation was cancelled");
  CHECK(after < 1000);
  document->detach_job();
  CHECK(near(body_volume(*document), rod)); // still cosmetic

  const std::string result = run(*document, modelled);
  CHECK(contains(result, R"("error":null)"));
  CHECK(std::abs(body_volume(*document) - 1302.830611500115) < 1e-3 * 1302.830611500115);
}

// Jobs handed back and forth between two threads, as the application hands
// its document to its worker and back: every result is right, and this
// thread measures the shapes the jobs made. It holds the last result's
// shape while the next job runs, and measures it only when the job is back:
// the job's fillet shares its faces and edges, and OCCT's algorithms set
// flags on the sub-shapes of their inputs (ThreadSanitizer reports reads
// of them meanwhile).
void test_jobs_handed_between_threads() {
  auto document = mitcad::new_demo_document(); // d3: the extrusion's height
  run(*document, fillet(quoted(corner1("F2")), 2)); // d4: the fillet's radius
  TestWorker workers[2];
  CHECK(workers[0].id() != workers[1].id());
  double height = 20.0;
  double radius = 2.0;
  std::shared_ptr<mitcad::geometry::Shape> last = document->body_shape("F2.b0");
  double last_volume = 60.0 * 40.0 * height - fillet_loss(radius) * height;
  double last_height = height;
  for (int i = 0; i < 50; ++i) {
    // Even jobs change the height (all is computed again), odd ones the
    // radius (the fillet only, on the cached extrusion).
    if (i % 2 == 0) {
      height = 21.0 + (i / 2) % 7; // never the value it has
    } else {
      radius = 1.0 + 0.25 * ((i / 2) % 5);
    }
    const std::string command = i % 2 == 0 ? set("d3", height) : set("d4", radius);
    const rust::Box<mitcad::JobControl> control = mitcad::new_job_control();
    document->attach_job(*control);
    std::string result;
    std::string error;
    TestWorker& worker = workers[i % 2];
    worker.start([&] {
      try {
        result = std::string(document->command(command));
      } catch (const rust::Error& e) {
        error = e.what();
      }
    });
    worker.wait();
    document->detach_job();
    // The last result's shape, held here meanwhile, is as it was.
    const double measured = mitcad::geometry::volume(*last);
    const mitcad::geometry::BoundingBox box = mitcad::geometry::bounding_box(*last);
    CHECK(near(measured, last_volume));
    CHECK(!box.empty && near(box.max.Z() - box.min.Z(), last_height));
    CHECK(error.empty() && contains(result, R"("error":null)"));
    CHECK(control->progress().position == 3 && control->progress().total == 3);
    last = document->body_shape("F2.b0");
    last_volume = 60.0 * 40.0 * height - fillet_loss(radius) * height;
    last_height = height;
    CHECK(last && near(mitcad::geometry::volume(*last), last_volume));
  }
  // The document is this thread's again.
  CHECK(contains(run(*document, R"({"cmd": "undo"})"), R"("error":null)"));
}

// Remote repositories (P12 remote): the git program's version, a clone that
// is refused before git runs, and the control of a remote operation.
void test_remote_bridge() {
  const std::string info(mitcad::git_info(false));
  CHECK(contains(info, R"("minimum":"2.34")"));
  const rust::Box<mitcad::SyncControl> control = mitcad::new_sync_control();
  CHECK(!control->is_cancelled() && control->progress().percent == -1);
  const std::string refused(
      mitcad::clone_project("https://user:s3cret@example.invalid/x.git", "remote-bridge-clone", *control, false));
  CHECK(contains(refused, R"("class":"invalid_url")") && !contains(refused, "s3cret"));
  CHECK(throws_with([&] { mitcad::clone_project("ext::sh -c id", "remote-bridge-clone", *control, true); },
                    "remote helpers"));
  // A sync's answer as text, its failure too.
  const std::string stopped(mitcad::describe_remote(
      "sync", R"({"case": "replay", "branch": "main", "upstream": "origin/main", "ahead": 1, "behind": 1,
                 "error": {"class": "conflict", "message": "a file changed both here and on the remote"}})"));
  CHECK(contains(stopped, "Sync stopped: a file changed both here and on the remote\n"));
  CHECK(throws_with([&] { mitcad::describe_remote("sync", "not json"); }, "not a JSON answer"));
  control->cancel();
  CHECK(control->is_cancelled() && control->progress().cancelled);
  if (contains(info, R"("error":null)")) {
    // git is there: a cancelled control runs nothing.
    const std::string cancelled(
        mitcad::clone_project("https://example.invalid/x.git", "remote-bridge-clone", *control, false));
    CHECK(contains(cancelled, R"("class":"cancelled")"));
  }
}

// The positions of a sketch's points ("at": [x, y]) in the sketch query.
std::vector<std::array<double, 2>> sketch_points(const mitcad::Document& document, const std::string& uid) {
  const std::string json(document.query(R"({"query": "sketch", "uid": ")" + uid + "\"}"));
  std::vector<std::array<double, 2>> out;
  const std::string key = R"("at":[)";
  for (std::size_t at = json.find(key); at != std::string::npos; at = json.find(key, at + 1)) {
    char* end = nullptr;
    const double x = std::strtod(json.c_str() + at + key.size(), &end);
    const double y = std::strtod(end + 1, nullptr);
    out.push_back({x, y});
  }
  return out;
}

bool has_point(const std::vector<std::array<double, 2>>& points, double x, double y) {
  for (const auto& p : points) {
    if (std::abs(p[0] - x) < 1e-6 && std::abs(p[1] - y) < 1e-6) {
      return true;
    }
  }
  return false;
}

// Project with Projection Link, as the application sends it (mitcad#40): a
// vertical edge and the top face of the block into a sketch on XZ (sketch
// y is -Z). When the block's sketch dimension or its extrude distance
// changes, the projected entities follow, by their topological names.
void test_linked_projection_follows_its_source() {
  auto document = mitcad::new_demo_document(); // d1 = 60, d2 = 40, d3 = 20
  run(*document, R"({"cmd": "sketch.create", "plane": "xz"})"); // F3
  run(*document, R"({"cmd": "sketch.project", "sketch": "F3", "source": ")" + corner1("F2") +
                     R"(", "body": "F2.b0", "linked": true})");
  const std::string top = "F2:end(" + kRectangle + ")";
  run(*document, R"({"cmd": "sketch.project", "sketch": "F3", "source": ")" + top +
                     R"(", "body": "F2.b0", "linked": true})");
  auto points = sketch_points(*document, "F3");
  CHECK(has_point(points, 60, 0) && has_point(points, 60, -20) && has_point(points, 0, -20));
  CHECK(!has_point(points, 80, 0));
  CHECK(contains(run(*document, set("d1", 80)), R"("error":null)"));
  points = sketch_points(*document, "F3");
  CHECK(has_point(points, 80, 0) && has_point(points, 80, -20) && has_point(points, 0, -20));
  CHECK(!has_point(points, 60, 0) && !has_point(points, 60, -20));
  CHECK(contains(run(*document, set("d3", 30)), R"("error":null)"));
  points = sketch_points(*document, "F3");
  CHECK(has_point(points, 80, 0) && has_point(points, 80, -30) && has_point(points, 0, -30));
  CHECK(!has_point(points, 80, -20));
}

// Distances from a body's faces (the .f3d import's fillet edge guesses):
// zero on a spline face (projected from the nearest point of its grid) and
// on an extruded side face, at most 1e-5 for a point 2e-6 off a face, the
// distance for one farther off.
void test_boundary_distances_on_spline_faces() {
  NCollection_Array2<gp_Pnt> heights(1, 8, 1, 8);
  for (int i = 1; i <= 8; ++i) {
    for (int j = 1; j <= 8; ++j) {
      heights(i, j) = gp_Pnt(2.0 * i, 2.0 * j, 0.5 * std::sin(i) * std::cos(0.7 * j));
    }
  }
  const Handle(Geom_BSplineSurface) wave = GeomAPI_PointsToBSplineSurface(heights).Surface();
  const TopoDS_Face top = BRepBuilderAPI_MakeFace(wave, 1e-7);
  const mitcad::geometry::Shape body(BRepPrimAPI_MakePrism(top, gp_Vec(0.0, 0.0, -5.0)).Shape());
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  wave->Bounds(u0, u1, v0, v1);
  std::vector<double> points;
  const auto add = [&](const gp_Pnt& p) { points.insert(points.end(), {p.X(), p.Y(), p.Z()}); };
  // On the spline at a few places, 2e-6 and 0.3 above it, and on a side.
  const std::array<std::array<double, 2>, 3> at{{{0.3, 0.6}, {0.55, 0.2}, {0.8, 0.85}}};
  for (const auto& [s, t] : at) {
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    wave->D1(u0 + s * (u1 - u0), v0 + t * (v1 - v0), p, du, dv);
    const gp_Vec normal = du.Crossed(dv).Normalized();
    add(p);
    add(p.Translated(normal * 2e-6));
    add(p.Translated(normal * 0.3));
  }
  add(wave->Value(0.5 * (u0 + u1), v0).Translated(gp_Vec(0.0, 0.0, -2.0)));
  for (int pass = 0; pass < 2; ++pass) {
    // (The second pass reads the distances kept with the shape.)
    const rust::Vec<double> d = mitcad::bridge::analysis_boundary_distances(
        body, rust::Slice<const double>(points.data(), points.size()));
    CHECK(d.size() == 10);
    for (std::size_t k = 0; k < 3; ++k) {
      CHECK(d[3 * k] == 0.0);
      CHECK(d[3 * k + 1] <= 1e-5);
      CHECK(std::abs(d[3 * k + 2] - 0.3) < 0.05);
    }
    CHECK(d[9] <= 1e-6);
  }
}

// A Rust panic's message reaches the application's sink (mitcad#62),
// before the hook that prints it.
std::string g_panic_note;

void note_panic(const char* message, std::size_t length) { g_panic_note.assign(message, length); }

void test_panic_note() {
  mitcad_set_panic_sink(&note_panic);
  mitcad_test_panic_note();
  CHECK(contains(g_panic_note, "Rust panic: panicked at "));
  CHECK(contains(g_panic_note, "panic_note.rs:"));
  CHECK(contains(g_panic_note, "panic for a test of the note"));
}

// Where segments cross a body's faces (the .f3d import's thread angles,
// mitcad#68): a 10 mm block with a hole of radius 2 at (3, 7).
// Each crossing once (also on an edge two faces share), in order, entering
// the material or leaving it; none for a segment inside or outside.
void test_segment_crossings() {
  const TopoDS_Shape hole = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(3, 7, -1), gp::DZ()), 2.0, 12.0).Shape();
  const mitcad::geometry::Shape body(BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape(), hole).Shape());
  const auto crossings = [&](std::array<double, 6> segment) {
    const rust::Vec<double> c =
        mitcad::bridge::analysis_segment_crossings(body, rust::Slice<const double>(segment.data(), segment.size()));
    return std::vector<double>(c.begin(), c.end());
  };
  const auto same = [](const std::vector<double>& a, const std::vector<double>& b) {
    if (a.size() != b.size()) {
      return false;
    }
    for (std::size_t i = 0; i < a.size(); ++i) {
      if (std::abs(a[i] - b[i]) > 1e-7) {
        return false;
      }
    }
    return true;
  };
  // Across the hole: in at x 0, out at 1, in at 5, out at 10.
  CHECK(same(crossings({-1, 7, 5, 11, 7, 5}), {1.0 / 12, -1, 2.0 / 12, 1, 6.0 / 12, -1, 11.0 / 12, 1}));
  // The other way round.
  CHECK(same(crossings({11, 7, 5, -1, 7, 5}), {1.0 / 12, -1, 6.0 / 12, 1, 10.0 / 12, -1, 11.0 / 12, 1}));
  // Through two vertical edges of the block, past the hole.
  CHECK(same(crossings({-1, -1, 2, 11, 11, 2}), {1.0 / 12, -1, 11.0 / 12, 1}));
  // Within the material, and in the hole.
  CHECK(crossings({1, 1, 1, 1, 1, 9}).empty());
  CHECK(crossings({3, 7, 0, 3, 7, 10}).empty());
}

void set_env(const char* name, const char* value) {
#ifdef _WIN32
  _putenv_s(name, value);
#else
  setenv(name, value, 1);
#endif
}

// An allocation that fails inside a kernel operation (std::bad_alloc, as
// MITCAD_TEST_OCCT_OUT_OF_MEMORY makes one) is that operation's error, which
// says it ran out of memory (mitcad#80), and the document goes on; and the
// process's memory against its limits.
void test_out_of_memory_in_an_operation() {
  auto document = mitcad::new_demo_document();
  set_env("MITCAD_TEST_OCCT_OUT_OF_MEMORY", "fillet");
  std::string failed;
  try {
    failed = std::string(document->command(fillet(quoted(corner1("F2")), 3)));
  } catch (const rust::Error& error) {
    failed = error.what();
  }
  set_env("MITCAD_TEST_OCCT_OUT_OF_MEMORY", "");
  CHECK(contains(failed, "fillet: out of memory"));
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0));
  run(*document, R"({"cmd": "undo"})");
  CHECK(contains(run(*document, fillet(quoted(corner1("F2")), 3)), R"("uid":"F)"));
  CHECK(near(body_volume(*document), 60.0 * 40.0 * 20.0 - fillet_loss(3) * 20.0));

  const mitcad::bridge::MemoryUse memory = mitcad::bridge::memory_use();
  CHECK(memory.resident > 0);
  CHECK(memory.limit > 0);
  CHECK(memory.used > 0 && memory.used <= memory.limit);
  CHECK(!memory.limit_kind.empty());
}

} // namespace

int main() {
  test_block_fillet_and_dimension_change();
  test_join_boss_fillet_follows_the_boss();
  test_cut_hole_lost_edge_and_split_body();
  test_chamfer_intersect_and_several_profiles();
  test_preview_and_profile_handles();
  test_rejected_commands();
  test_project_file_load_and_recompute();
  test_report();
  test_project_file_errors();
  test_datums_and_analysis_shapes();
  test_job_cancelled_from_another_thread();
  test_jobs_handed_between_threads();
  test_kernel_operation_cancelled_inside();
  test_remote_bridge();
  test_linked_projection_follows_its_source();
  test_boundary_distances_on_spline_faces();
  test_panic_note();
  test_segment_crossings();
  test_out_of_memory_in_an_operation();

  if (failures == 0) {
    std::puts("test_bridge: all checks passed");
  }
  return failures == 0 ? 0 : 1;
}
