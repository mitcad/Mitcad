// SPDX-License-Identifier: MIT
// End-to-end check of the C++ -> Rust model -> C++/OCCT chain without the
// UI: JSON commands in, shapes and names out.

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <set>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepGProp.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_PointsToBSplineSurface.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array2.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Vec.hxx>

#include "bridge/analysis.hpp"
#include "bridge/memory.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad_bridge/kernel/exception_probe.h"
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
                 R"("kind":"model","name":"d4","owner":"F3","table_dependencies":[],"text":"3 mm","unit":"mm","value":3.0})"));

  // A failing fillet keeps the unfilleted body: the ball is too large to
  // touch either face inside the 80 x 40 block, even rolling on a far edge.
  const std::string failed = run(*document, set("d4", 150));
  CHECK(contains(failed, R"("error":"Fillet1: )") && contains(failed, "fillet"));
  CHECK(near(body_volume(*document), 80.0 * 40.0 * 20.0));
  // Undo brings the working fillet back from the cache.
  CHECK(contains(run(*document, R"({"cmd": "undo"})"), R"("recomputed":0)"));
  CHECK(near(body_volume(*document), 80.0 * 40.0 * 20.0 - removed));

  // Wider than the 40 mm face (mitcad#121): the ball touches the front and
  // rolls on the far edge at (80, 40), the 40 mm face vanishes. Its centre
  // lies 50 behind the front, L = sqrt(2400) left of x = 80; the section
  // taken away is the integral of 50 - sqrt(2500 - u^2) over [0, L].
  const std::string wide = run(*document, set("d4", 50));
  CHECK(contains(wide, R"("error":null)"));
  const double l = std::sqrt(2400.0);
  const double cut = 50.0 * l - 0.5 * (10.0 * l + 2500.0 * std::asin(l / 50.0));
  CHECK(near(body_volume(*document), 80.0 * 40.0 * 20.0 - cut * 20.0));
  CHECK(!has_sharp_vertical_edge_at(*document, 80, 0));
  CHECK(has_sharp_vertical_edge_at(*document, 80, 40));
  CHECK(has_sharp_vertical_edge_at(*document, 0, 0));
  CHECK(has_sharp_vertical_edge_at(*document, 80.0 - l, 0)); // where the rounding leaves the front
  const auto rounded = document->body_shape("F2.b0");
  int faces = 0;
  for (TopExp_Explorer it(rounded->occt(), TopAbs_FACE); it.More(); it.Next()) {
    ++faces;
  }
  CHECK(faces == 6); // the rounding in place of the 40 mm face
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

// The version 1 design file example of V0: a block with a boss joined
// onto it, a hole cut through it, and a fillet on the block's corner 1 and
// the boss's corner 1.
const char* const kDesignFile = R"({
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

void test_design_file_load_and_recompute() {
  auto document = mitcad::load_document(kDesignFile);
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

void test_design_file_errors() {
  CHECK(throws_with([] { mitcad::load_document("{"); }, "not valid JSON"));
  CHECK(throws_with([] { mitcad::load_document(R"({"format": "step"})"); },
                    "not a Mitcad design file"));
  CHECK(throws_with([] { mitcad::load_document(R"({"format": "mitcad", "version": 4})"); },
                    "newer than this Mitcad"));

  const auto edited = [](const std::string& from, const std::string& to) {
    std::string json(kDesignFile);
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
  // What is left keeps the body's names, so picks on the clipped view refer
  // to the body; the cap has none (mitcad#233).
  const auto unnamed = [](const mitcad::geometry::Shape& shape) {
    int count = 0;
    for (int i = 0; i < shape.face_count(); ++i) {
      count += shape.face_names(i).empty() ? 1 : 0;
    }
    return count;
  };
  CHECK(clipped && clipped->find_faces(top).size() == 1 && unnamed(*clipped) == 1);
  // On the top face's plane nothing is cut: every face keeps its name.
  const auto touching = document->analysis_shape(
      R"({"shape": "clip", "plane": {"origin": [0, 0, 20], "normal": [0, 0, 1]}, "body": "F2.b0"})");
  CHECK(touching && touching->find_faces(top).size() == 1 && unnamed(*touching) == 0);
  const auto above = document->analysis_shape(R"({"shape": "section", "plane": "F3", "body": "F2.b0"})");
  CHECK(!above);
  CHECK(throws_with([&] { document->analysis_shape(R"({"shape": "datum", "uid": "F2"})"); },
                    "Extrude1 (F2) has no datum at the timeline marker"));
  const std::string text(document->analysis(R"({"query": "properties"})", false));
  CHECK(contains(text, "Body1 (F2.b0): Steel, density 7.85 g/cm3"));
  CHECK(contains(text, "mass 0.376800 kg"));
}

// A world section must clip each local body before its usual placement.
// Rotated nested occurrences catch both missing and double transforms.
void test_section_occurrence_coordinates() {
  auto document = mitcad::new_document();
  run(*document, R"({"cmd":"create_component","name":"Assembly","transform":{"translation":[0,0,100]}})");
  run(*document, R"({"cmd":"create_component","name":"Block","transform":{
      "rotation":{"axis":[0,1,0],"angle":1.5707963267948966}}})");
  run(*document, R"({"cmd":"sketch.create"})");
  run(*document, R"({"cmd":"sketch.add_rectangle","sketch":"F1","corner":[0,0],"width":20,"height":20})");
  run(*document, extrude("F1", kRectangle, 10, "new_body"));
  run(*document, R"({"cmd":"activate_component","component":"C0"})");
  run(*document, R"({"cmd":"copy_occurrence","occurrence":"Assembly:1","transform":{"translation":[30,0,100]}})");
  for (const char* path : {"O1/O2", "O3/O2"}) {
    for (int sign : {1, -1}) {
      const std::string request = std::string(R"({"plane":{"origin":[0,0,90],"normal":[0,0,)") +
          std::to_string(sign) + R"(]},"body":"F2.b0","occurrence":")" + path + "\"";
      const auto clipped = document->analysis_shape("{\"shape\":\"clip\"," + request.substr(1) + "}");
      CHECK(clipped && near(mitcad::geometry::volume(*clipped), 2000.0));
      if (clipped) {
        // World z = 100 - local x: the kept half changes sides, in place.
        const auto box = mitcad::geometry::bounding_box(*clipped);
        CHECK(!box.empty && near(box.min.X(), sign == 1 ? 10.0 : 0.0));
        CHECK(!box.empty && near(box.max.X(), sign == 1 ? 20.0 : 10.0));
        CHECK(!box.empty && near(box.min.Z(), 0.0) && near(box.max.Z(), 10.0));
      }
      const auto cap = document->analysis_shape("{\"shape\":\"section\"," + request.substr(1) + "}");
      CHECK(cap && near(mitcad::geometry::mass_properties(*cap).area, 200.0));
    }
  }
  // A plane outside the body legitimately retains all or none of it.
  const auto all = document->analysis_shape(R"({"shape":"clip","plane":{
      "origin":[0,0,110],"normal":[0,0,1]},"body":"F2.b0","occurrence":"O1/O2"})");
  const auto none = document->analysis_shape(R"({"shape":"clip","plane":{
      "origin":[0,0,110],"normal":[0,0,-1]},"body":"F2.b0","occurrence":"O1/O2"})");
  CHECK(all && near(mitcad::geometry::volume(*all), 4000.0));
  CHECK(none && near(mitcad::geometry::volume(*none), 0.0));
  CHECK(near(body_volume(*document), 4000.0));
  // Existing callers without occurrence still get component-local planes.
  const auto legacy = document->analysis_shape(R"({"shape":"clip","plane":{
      "origin":[10,0,0],"normal":[1,0,0]},"body":"F2.b0"})");
  CHECK(legacy && near(mitcad::geometry::volume(*legacy), 2000.0));
  CHECK(throws_with([&] { document->analysis_shape(R"({"shape":"clip","plane":"xy",
      "body":"F2.b0","occurrence":"O1"})"); }, "does not own body"));
  const std::string query = R"({"query":"section","world":true,"plane":{"origin":[0,0,90],"normal":[0,0,1]}})";
  const auto area = [&] {
    const std::string result(document->query(query));
    // JSON object keys are sorted, so the first area is the total.
    const auto at = result.find("\"area\":");
    return at == std::string::npos ? -1.0 : std::strtod(result.c_str() + at + 7, nullptr);
  };
  CHECK(near(area(), 400.0));
  run(*document, R"({"cmd":"set_occurrence_visible","occurrence":"Assembly:2","visible":false})");
  CHECK(near(area(), 200.0));
  run(*document, R"({"cmd":"undo"})");
  CHECK(near(area(), 400.0));
}

// Section Analysis clips every body shown at once, on all cores
// (mitcad#438): a body behind the plane is its own shape (the view keeps it
// drawn as it is), one in front has no faces, one across is clipped as on
// its own, and a body that cannot be clipped has its error without
// stopping the others. The world section measures them all at once.
void test_clips_together() {
  auto document = mitcad::new_document();
  // Blocks of 20 x 20 x 10 at x = 0, 30 and 60 (F2.b0, F4.b0, F6.b0).
  for (int i = 0; i < 3; ++i) {
    const std::string sketch = "F" + std::to_string(2 * i + 1);
    run(*document, R"({"cmd":"sketch.create"})");
    run(*document, R"({"cmd":"sketch.add_rectangle","sketch":")" + sketch + R"(","corner":[)" +
                       std::to_string(30 * i) + R"(,0],"width":20,"height":20})");
    run(*document, extrude(sketch, kRectangle, 10, "new_body"));
  }
  const auto unnamed = [](const mitcad::geometry::Shape& shape) {
    int count = 0;
    for (int i = 0; i < shape.face_count(); ++i) {
      count += shape.face_names(i).empty() ? 1 : 0;
    }
    return count;
  };
  const auto clipOne = [&](const std::string& plane, const char* body) {
    return document->analysis_shape(R"({"shape":"clip","plane":)" + plane + R"(,"body":")" + body + "\"}");
  };
  const auto clipAll = [&](const std::string& plane, const std::string& bodies) {
    return document->analysis_clips(R"({"plane":)" + plane + R"(,"bodies":[)" + bodies + "]}");
  };

  // At x = 40: F2.b0 behind, F4.b0 across, F6.b0 in front; F9.b0 is none.
  const std::string at40 = R"({"origin":[40,0,0],"normal":[1,0,0]})";
  const auto clips = clipAll(at40, R"({"body":"F2.b0"},{"body":"F4.b0"},{"body":"F9.b0"},{"body":"F6.b0"})");
  CHECK(clips->count() == 4);
  CHECK(clips->shape(0) && clips->shape(0) == document->body_shape("F2.b0"));
  CHECK(clipOne(at40, "F2.b0") == document->body_shape("F2.b0"));
  CHECK(std::string(clips->error(0)).empty());
  const auto across = clips->shape(1);
  const auto alone = clipOne(at40, "F4.b0");
  CHECK(across && alone && near(mitcad::geometry::volume(*across), 2000.0) &&
        near(mitcad::geometry::volume(*alone), 2000.0));
  CHECK(across && alone && across->face_count() == alone->face_count() && unnamed(*across) == 1 &&
        unnamed(*alone) == 1);
  CHECK(!clips->shape(2) && contains(std::string(clips->error(2)), "F9.b0"));
  const auto front = clips->shape(3);
  CHECK(front && !TopExp_Explorer(front->occt(), TopAbs_FACE).More());
  const auto frontAlone = clipOne(at40, "F6.b0");
  CHECK(frontAlone && !TopExp_Explorer(frontAlone->occt(), TopAbs_FACE).More());

  // At y = 5 the plane crosses all three: each clipped together as alone.
  const std::string at5 = R"({"origin":[0,5,0],"normal":[0,1,0]})";
  const auto all = clipAll(at5, R"({"body":"F2.b0"},{"body":"F4.b0"},{"body":"F6.b0"})");
  CHECK(all->count() == 3);
  const char* uids[] = {"F2.b0", "F4.b0", "F6.b0"};
  for (std::size_t i = 0; i < 3 && i < all->count(); ++i) {
    const auto together = all->shape(i);
    const auto single = clipOne(at5, uids[i]);
    CHECK(together && single && near(mitcad::geometry::volume(*together), 1000.0) &&
          near(mitcad::geometry::volume(*single), 1000.0));
    CHECK(together && single && together->face_count() == single->face_count() &&
          unnamed(*together) == unnamed(*single));
    if (together) {
      const auto box = mitcad::geometry::bounding_box(*together);
      CHECK(!box.empty && near(box.min.X(), 30.0 * static_cast<double>(i)) && near(box.max.Y(), 5.0));
    }
  }
  CHECK(throws_with([&] { clipAll(at5, R"({"uid":"F2.b0"})"); }, "uid"));

  // The world section at x = 40 meets F4.b0 only: 20 x 10 mm. (JSON
  // object keys are sorted, so the first area is the total.)
  const std::string world(document->query(R"({"query":"section","world":true,"plane":)" + at40 + "}"));
  const auto at = world.find("\"area\":");
  CHECK(at != std::string::npos && near(std::strtod(world.c_str() + at + 7, nullptr), 200.0));
}

// The first "area" of a section answer: the total (JSON object keys are
// sorted); -1 without one.
double total_area(const std::string& answer) {
  const auto at = answer.find("\"area\":");
  return at == std::string::npos ? -1.0 : std::strtod(answer.c_str() + at + 7, nullptr);
}

// The TShapes of a shape's faces, edges and vertices.
std::set<const void*> sub_shapes(const TopoDS_Shape& shape) {
  std::set<const void*> found;
  for (const TopAbs_ShapeEnum type : {TopAbs_FACE, TopAbs_EDGE, TopAbs_VERTEX}) {
    for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
      found.insert(it.Current().TShape().get());
    }
  }
  return found;
}

// Section Analysis off the UI thread (mitcad#438): clips and the section
// query prepared from the document and worked out on another thread,
// without it, give what the document gave when they were prepared, also
// after it changed. What the other thread reads is detached when they are
// prepared: a body the plane crosses as a copy that shares no face, edge
// or vertex with it, with its names (the view may mesh the body meanwhile),
// one the plane passes as itself with its box kept; a body clipped comes
// back as what is left of its copy, one left whole as its own shape. A stop
// asked of the running thread's geometry makes the clips not started by
// then fail as cancelled.
void test_prepared_section_analysis() {
  auto document = mitcad::new_document();
  // Blocks of 20 x 20 x 10 at x = 0, 30 and 60 (F2.b0, F4.b0, F6.b0).
  for (int i = 0; i < 3; ++i) {
    const std::string sketch = "F" + std::to_string(2 * i + 1);
    run(*document, R"({"cmd":"sketch.create"})");
    run(*document, R"({"cmd":"sketch.add_rectangle","sketch":")" + sketch + R"(","corner":[)" +
                       std::to_string(30 * i) + R"(,0],"width":20,"height":20})");
    run(*document, extrude(sketch, kRectangle, 10, "new_body"));
  }
  const auto unnamed = [](const mitcad::geometry::Shape& shape) {
    int count = 0;
    for (int i = 0; i < shape.face_count(); ++i) {
      count += shape.face_names(i).empty() ? 1 : 0;
    }
    return count;
  };
  const std::string at5 = R"({"origin":[0,5,0],"normal":[0,1,0]})";
  const std::string at40 = R"({"origin":[40,0,0],"normal":[1,0,0]})";
  const std::string sectionAt5 = R"({"query":"section","world":true,"plane":)" + at5 + "}";
  const auto cuts = document->prepare_clips(R"({"plane":)" + at5 +
                                            R"(,"bodies":[{"body":"F2.b0"},{"body":"F9.b0"},{"body":"F6.b0"}]})");
  // F2.b0 lies behind x = 40: left whole.
  const auto whole = document->prepare_clips(R"({"plane":)" + at40 + R"(,"bodies":[{"body":"F2.b0"}]})");
  const auto section = document->prepare_query(sectionAt5);
  const double now = total_area(std::string(document->query(sectionAt5)));
  CHECK(near(now, 600.0));
  const auto single = document->analysis_shape(R"({"shape":"clip","plane":)" + at5 + R"(,"body":"F6.b0"})");
  const auto body = document->body_shape("F6.b0");

  // What the other thread reads is detached when it is prepared: the
  // bodies the plane crosses as copies that share no face, edge or vertex
  // with them; the one it passes as the body itself, whose kept box alone
  // its clip reads.
  const char* crossed[] = {"F2.b0", nullptr, "F6.b0"};
  for (std::size_t i = 0; i < 3; ++i) {
    const auto input = cuts->input(i);
    if (crossed[i] == nullptr) {
      CHECK(!input && !cuts->detached(i)); // F9.b0: an error
      continue;
    }
    const auto own = document->body_shape(crossed[i]);
    CHECK(input && own && cuts->detached(i) && input != own);
    if (input && own) {
      const std::set<const void*> parts = sub_shapes(own->occt());
      int shared = 0;
      for (const void* part : sub_shapes(input->occt())) {
        shared += parts.count(part) != 0 ? 1 : 0;
      }
      CHECK(shared == 0);
      CHECK(input->face_count() == own->face_count() && unnamed(*input) == unnamed(*own));
    }
  }
  const auto passed = whole->input(0);
  CHECK(passed && !whole->detached(0) && passed == document->body_shape("F2.b0"));
  CHECK(passed && passed->bounds().has_value());
  CHECK(throws_with([&] { document->prepare_query(R"({"query":"properties"})"); }, "prepared query"));

  // The last block is taken back: what was prepared still has it.
  run(*document, R"({"cmd": "undo"})");
  CHECK(!document->body_shape("F6.b0"));
  CHECK(near(total_area(std::string(document->query(sectionAt5))), 400.0));

  std::optional<rust::Box<mitcad::ClippedBodies>> clipped;
  std::optional<rust::Box<mitcad::ClippedBodies>> kept;
  std::string answer;
  std::thread worker([&] {
    clipped.emplace(cuts->run());
    kept.emplace(whole->run());
    try {
      answer = std::string(section->run());
    } catch (const rust::Error& error) {
      answer = error.what();
    }
  });
  worker.join();
  CHECK(near(total_area(answer), now));
  CHECK(clipped && (*clipped)->count() == 3);
  if (clipped && (*clipped)->count() == 3) {
    CHECK((*clipped)->shape(0) && near(mitcad::geometry::volume(*(*clipped)->shape(0)), 1000.0));
    CHECK(!(*clipped)->shape(1) && contains(std::string((*clipped)->error(1)), "F9.b0"));
    const auto last = (*clipped)->shape(2);
    CHECK(last && single && near(mitcad::geometry::volume(*last), 1000.0));
    CHECK(last && single && last->face_count() == single->face_count() && unnamed(*last) == unnamed(*single) &&
          unnamed(*last) == 1);
    if (last && body) {
      const std::set<const void*> own = sub_shapes(body->occt());
      int shared = 0;
      for (const void* part : sub_shapes(last->occt())) {
        shared += own.count(part) != 0 ? 1 : 0;
      }
      CHECK(shared == 0);
      // The single clip keeps the faces the cut leaves as they are.
      int keeps = 0;
      for (const void* part : sub_shapes(single->occt())) {
        keeps += own.count(part) != 0 ? 1 : 0;
      }
      CHECK(keeps > 0);
    }
  }
  CHECK(kept && (*kept)->count() == 1 && (*kept)->shape(0) == document->body_shape("F2.b0"));

  // Stopped before they start: every clip fails as cancelled.
  mitcad::geometry::CancelFlag stop;
  stop.cancel();
  std::optional<rust::Box<mitcad::ClippedBodies>> stopped;
  std::thread stopping([&] {
    const mitcad::geometry::CancelScope scope(stop);
    stopped.emplace(cuts->run());
  });
  stopping.join();
  CHECK(stopped && (*stopped)->count() == 3);
  if (stopped && (*stopped)->count() == 3) {
    CHECK(!(*stopped)->shape(0) && contains(std::string((*stopped)->error(0)), "cancelled"));
    CHECK(!(*stopped)->shape(2) && contains(std::string((*stopped)->error(2)), "cancelled"));
  }
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
  auto document = mitcad::load_document(kDesignFile); // 6 features, none computed
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

// The boundary distances on a plate of many holes (mitcad#60): every
// edge's middle is on the boundary, points off the faces have the
// distance OCCT's distance query measures, and a point above the top face
// has its nearest point and normal there unless it is over a hole.
void test_boundary_distances_of_a_plate_with_holes() {
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(20.0, 20.0, 2.0).Shape();
  for (int i = 0; i < 4; ++i) {
    for (int j = 0; j < 4; ++j) {
      const gp_Ax2 axis(gp_Pnt(3.0 + 4.5 * i, 3.0 + 4.5 * j, -1.0), gp::DZ());
      plate = BRepAlgoAPI_Cut(plate, BRepPrimAPI_MakeCylinder(axis, 1.0, 4.0).Shape()).Shape();
    }
  }
  const mitcad::geometry::Shape body(plate);
  std::vector<double> points;
  const auto add = [&](const gp_Pnt& p) { points.insert(points.end(), {p.X(), p.Y(), p.Z()}); };
  for (int i = 0; i < body.edge_count(); ++i) {
    const BRepAdaptor_Curve curve(body.edge(i));
    add(curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())));
  }
  const std::size_t middles = points.size() / 3;
  // Off the faces: on a hole's axis, above the plate, beyond a corner,
  // inside the material, beside a hole's rim above it.
  const std::vector<gp_Pnt> off{gp_Pnt(3.0, 3.0, 1.0), gp_Pnt(10.0, 10.0, 3.5), gp_Pnt(-1.0, -2.0, 4.0),
                                gp_Pnt(5.25, 5.25, 1.0), gp_Pnt(3.5, 3.0, 2.3)};
  for (const gp_Pnt& p : off) {
    add(p);
  }
  const rust::Vec<double> d =
      mitcad::bridge::analysis_boundary_distances(body, rust::Slice<const double>(points.data(), points.size()));
  CHECK(d.size() == points.size() / 3);
  for (std::size_t k = 0; k < middles; ++k) {
    CHECK(d[k] <= 1e-6);
  }
  BRep_Builder builder;
  TopoDS_Compound faces;
  builder.MakeCompound(faces);
  for (int i = 0; i < body.face_count(); ++i) {
    builder.Add(faces, body.face(i));
  }
  for (std::size_t k = 0; k < off.size(); ++k) {
    BRepExtrema_DistShapeShape occt(BRepBuilderAPI_MakeVertex(off[k]).Vertex(), faces);
    CHECK(occt.IsDone() && std::abs(d[middles + k] - occt.Value()) < 1e-7);
  }
  // The top face: the plane z = 2 facing up.
  int top = -1;
  for (int i = 0; i < body.face_count(); ++i) {
    Bnd_Box box;
    BRepBndLib::Add(body.face(i), box);
    double x0 = 0.0;
    double y0 = 0.0;
    double z0 = 0.0;
    double x1 = 0.0;
    double y1 = 0.0;
    double z1 = 0.0;
    box.Get(x0, y0, z0, x1, y1, z1);
    if (std::abs(z0 - 2.0) < 1e-3 && std::abs(z1 - 2.0) < 1e-3) {
      top = i;
    }
  }
  CHECK(top >= 0);
  std::array<double, 3> point{};
  std::array<double, 3> normal{};
  // Beside a hole's rim, and exactly over it (on the boundary).
  CHECK(mitcad::bridge::analysis_face_point(body, {top}, {4.5, 3.0, 2.7}, point, normal));
  CHECK(std::abs(point[0] - 4.5) < 1e-9 && std::abs(point[2] - 2.0) < 1e-9 && normal[2] > 1.0 - 1e-9);
  CHECK(mitcad::bridge::analysis_face_point(body, {top}, {4.0, 3.0, 2.0}, point, normal));
  CHECK(std::abs(point[0] - 4.0) < 1e-9 && normal[2] > 1.0 - 1e-9);
  // Over the hole: OCCT's distance query finds the rim.
  CHECK(!mitcad::bridge::analysis_face_point(body, {top}, {3.2, 3.0, 2.5}, point, normal));
}

// The boundary distances of points off free-form faces (mitcad#442): a
// wavy spline of many knot spans (a projection let go after each call, as
// a stored thread's flanks) on a prism whose sides are extruded curves.
// Each point is measured as OCCT's distance query measures it, from the
// faces' grids kept with the shape: the query itself (a search over the
// whole surface and a classifier set up anew, for every point) is not run.
void test_boundary_distances_off_free_form_faces() {
  constexpr int kU = 160;
  constexpr int kV = 12;
  NCollection_Array2<gp_Pnt> heights(1, kU, 1, kV);
  for (int i = 1; i <= kU; ++i) {
    for (int j = 1; j <= kV; ++j) {
      heights(i, j) = gp_Pnt(0.5 * i, 2.0 * j, 1.5 * std::sin(0.15 * i) * std::cos(0.4 * j));
    }
  }
  GeomAPI_PointsToBSplineSurface interpolation;
  interpolation.Interpolate(heights);
  const Handle(Geom_BSplineSurface) wave = interpolation.Surface();
  CHECK(static_cast<double>(wave->NbUKnots() * wave->UDegree()) * 44.0 > 20000.0);
  const TopoDS_Face top = BRepBuilderAPI_MakeFace(wave, 1e-7);
  const mitcad::geometry::Shape body(BRepPrimAPI_MakePrism(top, gp_Vec(0.0, 0.0, -5.0)).Shape());
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  wave->Bounds(u0, u1, v0, v1);
  std::vector<gp_Pnt> off;
  // Above the spline and inside the body under it, near and farther; beside
  // the sides and beyond the corners; far above the waves (as near to
  // several crests).
  for (const double s : {0.07, 0.31, 0.55, 0.78, 0.96}) {
    for (const double t : {0.15, 0.5, 0.9}) {
      gp_Pnt p;
      gp_Vec du;
      gp_Vec dv;
      wave->D1(u0 + s * (u1 - u0), v0 + t * (v1 - v0), p, du, dv);
      const gp_Vec normal = du.Crossed(dv).Normalized();
      for (const double h : {1e-3, 0.05, 0.4, -0.05, 2.5}) {
        off.push_back(p.Translated(normal * h));
      }
    }
  }
  for (const gp_Pnt& p : {gp_Pnt(-0.3, 10.0, -2.0), gp_Pnt(40.0, -0.4, -1.0), gp_Pnt(81.0, 25.0, 0.5),
                          gp_Pnt(-1.0, -1.0, 3.0), gp_Pnt(40.0, 12.0, 30.0)}) {
    off.push_back(p);
  }
  std::vector<double> points;
  for (const gp_Pnt& p : off) {
    points.insert(points.end(), {p.X(), p.Y(), p.Z()});
  }
  BRep_Builder builder;
  TopoDS_Compound faces;
  builder.MakeCompound(faces);
  for (int i = 0; i < body.face_count(); ++i) {
    builder.Add(faces, body.face(i));
  }
  const std::size_t searches = mitcad::bridge::analysis_whole_face_searches();
  // Half the points in each call: the spline's projection is let go between.
  const std::size_t half = 3 * (off.size() / 2);
  const rust::Vec<double> first =
      mitcad::bridge::analysis_boundary_distances(body, rust::Slice<const double>(points.data(), half));
  const rust::Vec<double> second = mitcad::bridge::analysis_boundary_distances(
      body, rust::Slice<const double>(points.data() + half, points.size() - half));
  CHECK(first.size() + second.size() == off.size());
  CHECK(mitcad::bridge::analysis_whole_face_searches() == searches);
  for (std::size_t k = 0; k < off.size(); ++k) {
    const double d = k < first.size() ? first[k] : second[k - first.size()];
    BRepExtrema_DistShapeShape occt(BRepBuilderAPI_MakeVertex(off[k]).Vertex(), faces);
    CHECK(occt.IsDone() && std::abs(d - occt.Value()) < 1e-6);
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

// A file reader the application runs in its own process (mitcad#922): a
// panic of the DXF reader in the dxf_info query (forced by the test hook
// MITCAD_TEST_PARSER_PANIC, which names the file) is the query's error,
// with the panic's message, not an abort at the bridge; this process lives
// on, and the same query answers once the reader is well again.
void test_reader_panic_is_an_error() {
  const std::string path = "test_bridge_reader_panic.dxf"; // (in the test's working folder)
  {
    std::ofstream file(path, std::ios::binary);
    file << "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n0\nENDSEC\n0\nEOF\n";
  }
  const std::string query = R"({"query": "dxf_info", "path": ")" + path + "\"}";
  auto document = mitcad::new_document();
  g_panic_note.clear();
#ifdef _WIN32
  _putenv_s("MITCAD_TEST_PARSER_PANIC", path.c_str());
#else
  setenv("MITCAD_TEST_PARSER_PANIC", path.c_str(), 1);
#endif
  CHECK(throws_with([&] { (void)document->query(query); },
                    "test_bridge_reader_panic.dxf: the file's reader failed (an internal error): a test panic in "
                    "the reader of test_bridge_reader_panic.dxf (MITCAD_TEST_PARSER_PANIC)"));
  // The panic hook still ran: a crash report would have its message.
  CHECK(contains(g_panic_note, "MITCAD_TEST_PARSER_PANIC"));
#ifdef _WIN32
  _putenv_s("MITCAD_TEST_PARSER_PANIC", "");
#else
  unsetenv("MITCAD_TEST_PARSER_PANIC");
#endif
  try {
    const std::string answer(document->query(query));
    CHECK(contains(answer, "\"entities\""));
  } catch (const rust::Error& error) {
    std::fprintf(stderr, "dxf_info after the panic failed: %s\n", error.what());
    ++failures;
  }
  std::remove(path.c_str());
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

// Live updates (mitcad#89): the hub's commands and events through the
// bridge, a connection's thread reporting a broker that cannot be reached,
// TLS set up with the system's root certificates, and a read of the events
// on another thread ended by close.
void test_live_hub() {
  const rust::Box<mitcad::LiveHub> hub = mitcad::new_live_hub();
  CHECK(contains(std::string(hub->command(R"({"cmd": "status"})")), R"("connections":[])"));
  CHECK(contains(std::string(hub->events(0)), R"("events":[])"));
  CHECK(throws_with(
      [&] {
        hub->command(R"({"cmd": "connect", "broker": "mqtt://127.0.0.1:9", "user": "u",
                         "password": "secret", "session": "01234567-89ab-cdef-0123-456789abcdef"})");
      },
      "only over TLS"));
  CHECK(throws_with([&] { hub->command(R"({"cmd": "dance"})"); }, "unknown live command"));
  // Nothing listens on port 9 here: the connection goes offline.
  const std::string subscribed(hub->command(
      R"({"cmd": "subscribe", "broker": "mqtt://127.0.0.1:9", "session": "01234567-89ab-cdef-0123-456789abcdef",
          "project": "0123456789abcdef0123456789abcdef01234567"})"));
  CHECK(contains(subscribed, R"("connection":"c1")"));
  bool offline = false;
  const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
  while (!offline && std::chrono::steady_clock::now() < deadline) {
    const std::string events(hub->events(500));
    offline = contains(events, R"("state":"offline")");
  }
  CHECK(offline);
  // TLS: the system's root certificates are read (Windows' and macOS'
  // certificate stores through their libraries) before connecting.
  const std::string test(hub->command(R"({"cmd": "test", "broker": "mqtts://127.0.0.1:9", "timeout_ms": 2000})"));
  CHECK(contains(test, R"("ok":false)") && contains(test, R"("tls":true)"));
  CHECK(contains(test, R"("step":"connect")"));
  // A blocking read on another thread ends when the hub closes. Events of
  // the connection's retries may come first (on Windows a refused connect
  // takes a while), so the reader reads until it sees the hub closed.
  std::string closed;
  std::thread reader([&] {
    for (int reads = 0; reads < 1000 && !contains(closed, R"("closed":true)"); ++reads) {
      closed = std::string(hub->events(60000));
    }
  });
  std::this_thread::sleep_for(std::chrono::milliseconds(100));
  const auto closing = std::chrono::steady_clock::now();
  hub->command(R"({"cmd": "close"})");
  reader.join();
  CHECK(contains(closed, R"("closed":true)"));
  CHECK(std::chrono::steady_clock::now() - closing < std::chrono::seconds(10));
}

// The block of the demo document after a circular pattern x6 (about the
// block's middle) of four join or cut extrudes of 2 x 2 squares through
// it, 3, 8, 13 and 18 mm from the middle: its volume, its faces (names
// and areas) and its edge names.
struct PatternedBlock {
  double volume = -1.0;
  std::vector<std::pair<std::string, double>> faces;
  std::vector<std::string> edges;
};

PatternedBlock patterned_block(const std::string& operation, const std::string& compute) {
  auto document = mitcad::new_demo_document(); // F1, F2
  std::string features;
  for (int i = 0; i < 4; ++i) {
    const std::string sketch = "F" + std::to_string(3 + 2 * i);
    run(*document, R"({"cmd": "sketch.create"})");
    run(*document, sketch_rectangle(sketch, 33.0 + 5.0 * i, 19.0, 2.0, 2.0));
    run(*document, extrude(sketch, kRectangle, 30, operation, R"(["F2.b0"])"));
    features += (i > 0 ? "," : "") + quoted("F" + std::to_string(4 + 2 * i));
  }
  const std::string pattern =
      run(*document, R"({"cmd": "add_feature", "def": {"type": "circular_pattern", "objects": )"
                     R"({"type": "features", "features": [)" +
                         features +
                         R"(]}, "axis": {"origin": [30, 20, 0], "direction": [0, 0, 1]}, )"
                         R"("quantity": 6, "angle": 6.283185307179586, "compute": ")" +
                         compute + R"("}})");
  CHECK(contains(pattern, R"("uid":"F11")"));
  CHECK(!contains(std::string(document->query(R"({"query": "timeline"})")), R"("status":"error")"));
  PatternedBlock block;
  const auto body = document->body_shape("F2.b0");
  if (!body) {
    std::fprintf(stderr, "test_bridge.cpp: no body F2.b0 after the pattern (%s, %s)\n", operation.c_str(),
                 compute.c_str());
    ++failures;
    return block;
  }
  block.volume = mitcad::geometry::volume(*body);
  for (int i = 0; i < body->face_count(); ++i) {
    std::string names;
    for (const std::string& name : body->face_names(i)) {
      names += name + ";";
    }
    GProp_GProps props;
    BRepGProp::SurfaceProperties(body->face(i), props);
    block.faces.emplace_back(names, props.Mass());
  }
  std::sort(block.faces.begin(), block.faces.end());
  for (int i = 0; i < body->edge_count(); ++i) {
    block.edges.push_back(body->edge_name(i));
  }
  std::sort(block.edges.begin(), block.edges.end());
  return block;
}

// mitcad#855: a pattern of join features (and of cut features) whose
// copies stay apart is applied in one boolean, which gives the body the
// features give one after another (MITCAD_NO_PATTERN_RUNS=1): the same
// volume, faces with the same names and areas, and edge names; and leaves
// its input shapes as they were (the input check).
void test_pattern_of_features_in_one_boolean() {
  mitcad::geometry::clear_input_changes();
  mitcad::geometry::enable_input_checks(true);
  for (const char* operation : {"join", "cut"}) {
    // Each square 2 x 2, 10 mm above the block or 20 mm through it.
    const double volume = 60.0 * 40.0 * 20.0 + (std::string(operation) == "join" ? 1.0 : -2.0) * 24 * 40.0;
    for (const char* compute : {"adjust", "identical"}) {
      set_env("MITCAD_NO_PATTERN_RUNS", "");
      const PatternedBlock together = patterned_block(operation, compute);
      set_env("MITCAD_NO_PATTERN_RUNS", "1");
      const PatternedBlock one_by_one = patterned_block(operation, compute);
      set_env("MITCAD_NO_PATTERN_RUNS", "");
      CHECK(near(together.volume, volume));
      CHECK(near(one_by_one.volume, volume));
      CHECK(together.faces.size() == one_by_one.faces.size());
      bool same = together.faces.size() == one_by_one.faces.size();
      for (std::size_t i = 0; same && i < together.faces.size(); ++i) {
        same = together.faces[i].first == one_by_one.faces[i].first &&
               near(together.faces[i].second, one_by_one.faces[i].second);
        if (!same) {
          std::fprintf(stderr, "%s %s: face %s (%f) against %s (%f)\n", operation, compute,
                       together.faces[i].first.c_str(), together.faces[i].second,
                       one_by_one.faces[i].first.c_str(), one_by_one.faces[i].second);
        }
      }
      CHECK(same);
      CHECK(together.edges == one_by_one.edges);
    }
  }
  const std::vector<std::string> changes = mitcad::geometry::input_changes();
  for (const std::string& change : changes) {
    std::fprintf(stderr, "input changed: %s\n", change.c_str());
  }
  CHECK(changes.empty());
  mitcad::geometry::enable_input_checks(false);
  mitcad::geometry::clear_input_changes();
}

// C++ exceptions under a bridge call, thrown outside any geometry operation
// (mitcad#937): each reaches Rust as the kernel's error through the
// bridges' trycatch (core/cpp/bridge/trycatch.hpp), and the process goes
// on. An OCCT exception keeps its message, or names its type without one;
// an object that is no std::exception, which cxx's own trycatch would let
// end the process, is an error too.
void test_exceptions_across_the_bridge() {
  const auto probe = [](const char* kind, const char* message) {
    return std::string(mitcad::bridge::bridge_exception_probe(kind, message));
  };
  CHECK(probe("occt", "an OCCT failure outside an operation") == "Err: an OCCT failure outside an operation");
  CHECK(probe("occt-empty", "") == "Err: OCCT exception Standard_ConstructionError");
  CHECK(probe("std", "a standard exception") == "Err: a standard exception");
  CHECK(probe("other", "") == "Err: unknown C++ exception");
  CHECK(probe("none", "") == "Ok");
  // Still alive: a later bridge call works as before.
  CHECK(probe("occt", "again") == "Err: again");
}

} // namespace

int main() {
  test_block_fillet_and_dimension_change();
  test_join_boss_fillet_follows_the_boss();
  test_cut_hole_lost_edge_and_split_body();
  test_chamfer_intersect_and_several_profiles();
  test_preview_and_profile_handles();
  test_rejected_commands();
  test_design_file_load_and_recompute();
  test_report();
  test_design_file_errors();
  test_datums_and_analysis_shapes();
  test_section_occurrence_coordinates();
  test_clips_together();
  test_prepared_section_analysis();
  test_job_cancelled_from_another_thread();
  test_jobs_handed_between_threads();
  test_kernel_operation_cancelled_inside();
  test_remote_bridge();
  test_linked_projection_follows_its_source();
  test_boundary_distances_on_spline_faces();
  test_boundary_distances_of_a_plate_with_holes();
  test_boundary_distances_off_free_form_faces();
  test_panic_note();
  test_reader_panic_is_an_error();
  test_segment_crossings();
  test_out_of_memory_in_an_operation();
  test_live_hub();
  test_pattern_of_features_in_one_boolean();
  test_exceptions_across_the_bridge();

  if (failures == 0) {
    std::puts("test_bridge: all checks passed");
  }
  return failures == 0 ? 0 : 1;
}
