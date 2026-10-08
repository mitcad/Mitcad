// SPDX-License-Identifier: MIT
// Booleans on perforated bodies (mitcad#72): the holes away from the tool
// sit out the operation (far_features.hpp), a tool of many apart solids is
// cut region by region, and the result is the one the whole body and one
// cut give: the same faces, edges, names and volume, and valid.

#include <algorithm>
#include <chrono>
#include <cstdio>
#include <functional>
#include <string>
#include <vector>

#include <BRepCheck_Analyzer.hxx>

#include "../src/far_features.hpp"
#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

BooleanResult run(BooleanOp op, const ShapePtr& target, const ShapePtr& tool) {
  return boolean(op, {target.get()}, *tool);
}

// The union of the shapes (a pattern's copies).
ShapePtr united(const std::vector<ShapePtr>& shapes) {
  std::vector<const Shape*> raw;
  for (const ShapePtr& shape : shapes) {
    raw.push_back(shape.get());
  }
  return unite(raw);
}

// n x n holes of 4 mm, 10 mm apart, through z = 0..5 (copies of F3's
// cylinder, named as a pattern's).
ShapePtr holes(int n) {
  std::vector<ShapePtr> copies;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      copies.push_back(extrude("F3:inst" + std::to_string(i * n + j) + "(F3)",
                               {circle(5, 5.0 + 10.0 * i, 5.0 + 10.0 * j, 2.0)}, -1, 6));
    }
  }
  return united(copies);
}

// A plate 10 * n mm square and 5 mm thick (F2) with n x n holes cut at once.
ShapePtr perforated(int n) {
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 10.0 * n, 10.0 * n)}, 0, 5);
  return run(BooleanOp::Cut, plate, holes(n)).pieces.at(0).shape;
}

// Everything a later feature can see of a result: per piece its volume,
// counts, face, edge and vertex names, sorted; and whether OCCT's checker
// finds every piece valid.
struct Summary {
  std::vector<std::string> lines;
  bool valid = true;
};

Summary summary(const BooleanResult& result) {
  Summary out;
  std::string touched;
  for (bool t : result.touched) {
    touched += t ? '1' : '0';
  }
  out.lines.push_back("touched " + touched + " pieces " + std::to_string(result.pieces.size()));
  for (const BooleanPiece& piece : result.pieces) {
    const Shape& shape = *piece.shape;
    char buffer[128];
    std::snprintf(buffer, sizeof buffer, "piece %.6f faces %d edges %d vertices %d", volume(shape),
                  shape.face_count(), shape.edge_count(), shape.vertex_count());
    out.lines.emplace_back(buffer);
    std::vector<std::string> names;
    for (int i = 0; i < shape.face_count(); ++i) {
      std::string joined;
      for (const std::string& name : shape.face_names(i)) {
        joined += name + ";";
      }
      names.push_back("face " + joined);
    }
    for (int i = 0; i < shape.edge_count(); ++i) {
      names.push_back("edge " + shape.edge_name(i));
    }
    for (int i = 0; i < shape.vertex_count(); ++i) {
      names.push_back("vertex " + shape.vertex_name(i));
    }
    std::sort(names.begin(), names.end());
    out.lines.insert(out.lines.end(), names.begin(), names.end());
    out.valid = out.valid && BRepCheck_Analyzer(shape.occt()).IsValid();
  }
  return out;
}

// The operation with far features and tiles and without: the same result,
// valid. Prints both times.
void same_both_ways(const char* what, const std::function<BooleanResult()>& operation) {
  detail::enable_far_features(false);
  const auto t0 = std::chrono::steady_clock::now();
  const Summary whole = summary(operation());
  const auto t1 = std::chrono::steady_clock::now();
  detail::enable_far_features(true);
  const Summary reduced = summary(operation());
  const auto t2 = std::chrono::steady_clock::now();
  CHECK(whole.valid);
  CHECK(reduced.valid);
  CHECK(whole.lines == reduced.lines);
  if (whole.lines != reduced.lines) {
    std::fprintf(stderr, "%s: results differ\n", what);
    int shown = 0;
    for (std::size_t i = 0; i < std::max(whole.lines.size(), reduced.lines.size()) && shown < 6; ++i) {
      const std::string a = i < whole.lines.size() ? whole.lines[i] : "-";
      const std::string b = i < reduced.lines.size() ? reduced.lines[i] : "-";
      if (a != b) {
        std::fprintf(stderr, "  whole:   %s\n  reduced: %s\n", a.c_str(), b.c_str());
        ++shown;
      }
    }
  }
  std::printf("far features, %s: %.0f ms whole, %.0f ms reduced\n", what,
              std::chrono::duration<double, std::milli>(t1 - t0).count(),
              std::chrono::duration<double, std::milli>(t2 - t1).count());
}

void same_both_ways(const char* what, BooleanOp op, const ShapePtr& target, const ShapePtr& tool) {
  same_both_ways(what, [&] { return run(op, target, tool); });
}

void test_cuts_and_joins_match_the_whole_body() {
  const int n = 10;
  const ShapePtr plate = perforated(n);
  CHECK(plate->face_count() == 6 + n * n);
  // A notch at a corner, over one hole.
  same_both_ways("corner notch", BooleanOp::Cut, plate, extrude("F4", {rectangle(9, -5, -5, 12, 12)}, -1, 6));
  // A slot across a row of holes in the middle.
  same_both_ways("slot", BooleanOp::Cut, plate, extrude("F4", {rectangle(9, 22, 42, 50, 6)}, -1, 6));
  // A pocket from the top, 2 mm deep, around four holes: the bottom's
  // holes are taken out of it too.
  same_both_ways("pocket", BooleanOp::Cut, plate, extrude("F4", {rectangle(9, 31, 31, 18, 18)}, 3, 6));
  // A cut through a column of holes that splits the plate in two.
  same_both_ways("split", BooleanOp::Cut, plate, extrude("F4", {rectangle(9, 54, -5, 2, 110)}, -1, 6));
  // A flush extension: the top and bottom faces merge with its own.
  same_both_ways("flush join", BooleanOp::Join, plate, extrude("F4", {rectangle(9, 100, 0, 20, 100)}, 0, 5));
  // A boss on the top over two holes.
  same_both_ways("boss", BooleanOp::Join, plate, extrude("F4", {rectangle(9, 2, 2, 16, 6)}, 5, 9));
  // A tool far from every hole touches only the plate's side.
  same_both_ways("side tab", BooleanOp::Join, plate, extrude("F4", {rectangle(9, -10, 40, 10, 20)}, 0, 5));
  // A tool that misses the plate, and one through all of it.
  same_both_ways("miss", BooleanOp::Cut, plate, extrude("F4", {rectangle(9, 200, 0, 10, 10)}, -1, 6));
  same_both_ways("all", BooleanOp::Cut, plate, extrude("F4", {rectangle(9, -5, -5, 110, 110)}, -1, 6));
}

void test_holes_whose_merge_could_change_stay() {
  // Plugs in four of the holes, flush with the top and bottom: their caps
  // merge with the plate's faces.
  const ShapePtr plate = perforated(10);
  std::vector<ShapePtr> plugs;
  for (int k = 0; k < 4; ++k) {
    plugs.push_back(extrude("F6:inst" + std::to_string(k) + "(F6)", {circle(7, 55.0 + 10.0 * k, 55.0, 2.0)}, 0, 5));
  }
  same_both_ways("plugs", BooleanOp::Join, plate, united(plugs));
  // Counterbores half the plate deep around a column of holes, then a
  // notch far from them: features of three faces each.
  std::vector<ShapePtr> collars;
  for (int k = 0; k < 4; ++k) {
    collars.push_back(extrude("F6:inst" + std::to_string(k) + "(F6)", {circle(7, 75.0, 15.0 + 10.0 * k, 3.0)}, 2.5, 6));
  }
  const ShapePtr counterbored = run(BooleanOp::Cut, plate, united(collars)).pieces.at(0).shape;
  same_both_ways("counterbores", BooleanOp::Cut, counterbored, extrude("F4", {rectangle(9, -5, 80, 12, 30)}, -1, 6));
}

void test_tiled_pattern_cut_matches_one_cut() {
  // 26 x 26 holes: more than twice a tile's solids, cut region by region.
  const int n = 26;
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 10.0 * n, 10.0 * n)}, 0, 5);
  const ShapePtr tool = holes(n);
  same_both_ways("26 x 26 holes", BooleanOp::Cut, plate, tool);
  // Copies that cross the plate's edge split its sides into pieces: one cut.
  const ShapePtr shifted = extrude("F2", {rectangle(1, 3, 3, 10.0 * n, 10.0 * n)}, 0, 5);
  same_both_ways("holes over the edge", BooleanOp::Cut, shifted, tool);
}

void test_holes_in_a_curved_wall() {
  // A tube along x (radius 50, wall 3) with 12 x 6 holes through its top,
  // then a notch at one end and a boss on the top: the holes' edges are
  // intersection curves on cylinders.
  const ShapePtr outer = extrude("F2", {circle(1, 0, 0, 50)}, -100, 100);
  const ShapePtr inner = extrude("F3", {circle(2, 0, 0, 47)}, -101, 101);
  const ShapePtr along_z = run(BooleanOp::Cut, outer, inner).pieces.at(0).shape;
  Affine turn;
  turn.linear = {{{0, 0, 1}, {0, 1, 0}, {-1, 0, 0}}};
  const ShapePtr tube = transform_shape(*along_z, turn);
  std::vector<ShapePtr> copies;
  for (int i = 0; i < 12; ++i) {
    for (int j = 0; j < 6; ++j) {
      copies.push_back(extrude("F4:inst" + std::to_string(i * 6 + j) + "(F4)",
                               {circle(5, -77.0 + 14.0 * i, -25.0 + 10.0 * j, 3.0)}, 0, 60));
    }
  }
  const ShapePtr perforated_tube = run(BooleanOp::Cut, tube, united(copies)).pieces.at(0).shape;
  CHECK(perforated_tube->face_count() == 4 + 72);
  same_both_ways("tube notch", BooleanOp::Cut, perforated_tube, extrude("F6", {rectangle(9, 90, -20, 20, 40)}, 30, 60));
  same_both_ways("tube boss", BooleanOp::Join, perforated_tube, extrude("F6", {circle(9, 80, 0, 6)}, 40, 60));
}

} // namespace

void far_features_tests() {
  guarded("test_cuts_and_joins_match_the_whole_body", test_cuts_and_joins_match_the_whole_body);
  guarded("test_holes_whose_merge_could_change_stay", test_holes_whose_merge_could_change_stay);
  guarded("test_tiled_pattern_cut_matches_one_cut", test_tiled_pattern_cut_matches_one_cut);
  guarded("test_holes_in_a_curved_wall", test_holes_in_a_curved_wall);
}

} // namespace test
