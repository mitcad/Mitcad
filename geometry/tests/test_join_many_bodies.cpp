// SPDX-License-Identifier: MIT
// Unions of many bodies (mitcad#1273): bodies whose boxes meet, directly or
// through others, are united group by group and the groups put together
// without a boolean operation; a union that lost material is tried with a
// fuzzy value and fails when that does not help, instead of coming out
// empty.
//
// The reported design of GitHub #133 (MITCAD_MITCAD_REPORTED, a folder of
// reported MitCad designs, `github-133-…`): its Combine joined 257 bodies
// of a machine and came out empty. The fault is in one pair: a nut that
// lies inside a slide carriage (body 158 inside body 157 of the design's
// base feature), its bore and chamfers coaxial with the carriage's threaded
// hole. The nut's bore seam passes 2.5 to 8 micrometres from vertices of
// the thread's faces, and a vertex of its chamfer circle lies 2.1
// micrometres from a flank of the thread. OCCT's exact union of the two
// splits a face of the carriage with edges left over (unused edges, tiny
// edges, self-intersections it acquired), loses the carriage's outer shell
// and gives no solid, a result its checker finds valid. With a fuzzy value
// of 1e-5 mm it gives the carriage with the nut in it. The same nut in the
// carriage's other holes, 0.111 mm lower, joins exactly. A second fault of
// the design: two copies of a back plate whose side walls lie 1e-4 mm
// apart joined exactly to one plate, edges of their faces left unused
// (fuzzy 1e-4 mm joins them), and a mount whose stud stands in the
// carriage's third threaded hole, which the exact union kept apart from the
// carriage. No synthetic layout reproduced OCCT's failures; skipped without
// the variable.
//
// mitcad#1278: those pairs are found before the union of all from their
// near misses (a vertex of one micrometres from a face of the other) and
// united first, so that the design's join is one union of all, not two; it
// logs its time. Cut and Intersect of the two plates, which OCCT got wrong
// exactly (one cut whole, the other nothing, the intersection empty), are
// made again with a fuzzy value where OCCT left pieces unused, as the join
// is. Boxes 1e-4 mm aside, parallel or turned, cut and intersect exactly
// (no synthetic pair reproduces it either).

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <memory>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#include <zlib.h>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <NCollection_List.hxx>
#include <Bnd_Box.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/pattern.hpp"
#include "mitcad/geometry/query.hpp"

namespace test {
namespace {

using mitcad::geometry::BooleanOp;
using mitcad::geometry::BooleanResult;
using mitcad::geometry::Shape;
using mitcad::geometry::ShapePtr;

std::string env(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, name) == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? std::string(value) : std::string();
#endif
}

int solid_count(const Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

double volume_of(const Shape& shape) { return mitcad::geometry::volume(shape); }

// A box from (x, y, z) of the given size, its faces named F<feature>:import(k).
ShapePtr box(const std::string& feature, double x, double y, double z, double dx, double dy, double dz) {
  return mitcad::geometry::import_body(BRepPrimAPI_MakeBox(gp_Pnt(x, y, z), dx, dy, dz).Shape(), feature, 0);
}

void test_groups_of_meeting_boxes() {
  // Three groups: a row of three overlapping blocks (the first and the
  // third apart, linked through the second), two blocks sharing a face, and
  // one block alone, given in mixed order.
  const ShapePtr a1 = box("F1", 0, 0, 0, 10, 10, 10);
  const ShapePtr b1 = box("F2", 100, 0, 0, 10, 10, 10);
  const ShapePtr a2 = box("F3", 8, 0, 0, 10, 10, 10);
  const ShapePtr c1 = box("F4", 0, 100, 0, 5, 5, 5);
  const ShapePtr b2 = box("F5", 110, 0, 0, 10, 10, 10);
  const ShapePtr a3 = box("F6", 16, 0, 0, 10, 10, 10);
  const ShapePtr joined =
      mitcad::geometry::unite({a1.get(), b1.get(), a2.get(), c1.get(), b2.get(), a3.get()});
  CHECK(solid_count(*joined) == 3);
  CHECK_NEAR_TOL(volume_of(*joined), 26.0 * 100 + 20.0 * 100 + 125.0, 1e-6);
  CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
  // Every input keeps the names of its faces left in the union: the row's
  // ends, the pair's outer sides (their shared faces are inside), the
  // single block's six faces.
  for (const char* feature : {"F1", "F2", "F4", "F5", "F6"}) {
    bool named = false;
    for (int i = 0; i < joined->face_count() && !named; ++i) {
      for (const std::string& name : joined->face_names(i)) {
        named = named || name.rfind(std::string(feature) + ":import(", 0) == 0;
      }
    }
    CHECK(named);
  }
  CHECK(joined->face_count() == 6 + 6 + 6);
  // As one union of all, the groups give the same result.
  const ShapePtr row = mitcad::geometry::unite({a1.get(), a2.get(), a3.get()});
  CHECK(solid_count(*row) == 1);
  CHECK_NEAR_TOL(volume_of(*row), 26.0 * 100, 1e-6);
}

void test_near_miss_in_a_row() {
  // A row of eight overlapping blocks and a ninth standing in the fourth,
  // its side walls 1e-4 mm beside the fourth's (mitcad#1278): the union of
  // the nine unites the near pair first (a vertex of one lies 1e-4 mm from
  // a face of the other), then all, exactly, and every block keeps the
  // names of its faces left in the union.
  std::vector<ShapePtr> blocks;
  for (int k = 0; k < 8; ++k) {
    blocks.push_back(box("F" + std::to_string(k + 1), 9.0 * k, 0, 0, 10, 10, 10));
  }
  blocks.push_back(box("F9", 27.0 + 1e-4, 0, 9.5, 10, 10, 5));
  std::vector<const Shape*> inputs;
  for (const ShapePtr& block : blocks) {
    inputs.push_back(block.get());
  }
  const ShapePtr joined = mitcad::geometry::unite(inputs);
  CHECK(solid_count(*joined) == 1);
  CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
  // The row, 73 mm long, and the ninth block's 4.5 mm above it.
  CHECK_NEAR_TOL(volume_of(*joined), 73.0 * 100.0 + 450.0, 1e-9);
  for (const char* feature : {"F1", "F4", "F8", "F9"}) {
    bool named = false;
    for (int i = 0; i < joined->face_count() && !named; ++i) {
      for (const std::string& name : joined->face_names(i)) {
        named = named || name.rfind(std::string(feature) + ":import(", 0) == 0;
      }
    }
    CHECK(named);
  }
}

// Body `index` of the base feature of the reported design of GitHub #133
// (the index-th B-rep data in the file: its 257 bodies are the file's only
// ones), its faces named as the base feature names them; null without it.
// The body whose B-rep data runs from `at` to `end` of the design's text.
ShapePtr decoded_body(const std::string& text, std::size_t at, std::size_t end, int index) {
  // Base64, then zlib.
  std::string packed;
  unsigned bits = 0;
  int count = 0;
  for (std::size_t i = at; i < end; ++i) {
    const char c = text[i];
    int value = -1;
    if (c >= 'A' && c <= 'Z') value = c - 'A';
    else if (c >= 'a' && c <= 'z') value = c - 'a' + 26;
    else if (c >= '0' && c <= '9') value = c - '0' + 52;
    else if (c == '+') value = 62;
    else if (c == '/') value = 63;
    if (value < 0) {
      continue;
    }
    bits = (bits << 6) | static_cast<unsigned>(value);
    count += 6;
    if (count >= 8) {
      count -= 8;
      packed.push_back(static_cast<char>((bits >> count) & 0xFFu));
    }
  }
  std::string data;
  z_stream stream{};
  if (inflateInit(&stream) != Z_OK) {
    return nullptr;
  }
  stream.next_in = reinterpret_cast<Bytef*>(packed.data());
  stream.avail_in = static_cast<uInt>(packed.size());
  char buffer[65536];
  int status = Z_OK;
  while (status == Z_OK) {
    stream.next_out = reinterpret_cast<Bytef*>(buffer);
    stream.avail_out = sizeof buffer;
    status = inflate(&stream, Z_NO_FLUSH);
    data.append(buffer, sizeof buffer - stream.avail_out);
  }
  inflateEnd(&stream);
  if (status != Z_STREAM_END) {
    return nullptr;
  }
  return mitcad::geometry::import_body(mitcad::geometry::read_brep_data(data.data(), data.size()), "F1",
                                       index * 1000);
}

ShapePtr reported_body(const std::string& text, int index) {
  std::size_t at = 0;
  for (int k = 0; k <= index; ++k) {
    at = text.find("\"data\": \"", at);
    if (at == std::string::npos) {
      return nullptr;
    }
    at += 9;
  }
  const std::size_t end = text.find('"', at);
  if (end == std::string::npos) {
    return nullptr;
  }
  return decoded_body(text, at, end, index);
}

// All bodies of the design's base feature, in order.
std::vector<ShapePtr> reported_bodies(const std::string& text) {
  std::vector<ShapePtr> bodies;
  std::size_t at = 0;
  while ((at = text.find("\"data\": \"", at)) != std::string::npos) {
    at += 9;
    const std::size_t end = text.find('"', at);
    if (end == std::string::npos) {
      break;
    }
    bodies.push_back(decoded_body(text, at, end, static_cast<int>(bodies.size())));
    at = end;
  }
  return bodies;
}

// Points of the boxes (a fixed sequence of `count` in each) that the union
// puts wrongly: inside an input but outside the union, or inside the union
// but outside every input (points on a boundary, within 1e-3 mm, count as
// either). None when the union holds exactly what its inputs hold.
int misplaced_points(const Shape& joined, const std::vector<const Shape*>& inputs,
                     const std::vector<Bnd_Box>& boxes, int count) {
  BRepClass3d_SolidClassifier in_union(joined.occt());
  // Each input's classifier made when a point first falls in its box.
  std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> in_inputs(inputs.size());
  std::vector<Bnd_Box> input_boxes(inputs.size());
  for (std::size_t i = 0; i < inputs.size(); ++i) {
    BRepBndLib::Add(inputs[i]->occt(), input_boxes[i]);
    input_boxes[i].Enlarge(1e-2);
  }
  unsigned state = 12345u;
  const auto next = [&state] {
    state = state * 1664525u + 1013904223u;
    return static_cast<double>(state >> 8) / 16777216.0;
  };
  int wrong = 0;
  for (int k = 0; k < count * static_cast<int>(boxes.size()); ++k) {
    double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
    boxes[static_cast<std::size_t>(k / count)].Get(xmin, ymin, zmin, xmax, ymax, zmax);
    const double px = xmin + (xmax - xmin) * next();
    const double py = ymin + (ymax - ymin) * next();
    const double pz = zmin + (zmax - zmin) * next();
    const gp_Pnt point(px, py, pz);
    in_union.Perform(point, 1e-3);
    const TopAbs_State united = in_union.State();
    if (united == TopAbs_ON) {
      continue;
    }
    bool inside = false;
    bool on = false;
    for (std::size_t i = 0; i < inputs.size(); ++i) {
      if (input_boxes[i].IsOut(point)) {
        continue;
      }
      if (!in_inputs[i]) {
        in_inputs[i] = std::make_unique<BRepClass3d_SolidClassifier>(inputs[i]->occt());
      }
      in_inputs[i]->Perform(point, 1e-3);
      inside = inside || in_inputs[i]->State() == TopAbs_IN;
      on = on || in_inputs[i]->State() == TopAbs_ON;
    }
    if (on) {
      continue;
    }
    if (inside != (united == TopAbs_IN)) {
      ++wrong;
    }
  }
  return wrong;
}

int misplaced_points(const Shape& joined, const std::vector<const Shape*>& inputs, const Bnd_Box& box,
                     int count) {
  return misplaced_points(joined, inputs, std::vector<Bnd_Box>{box}, count);
}

Bnd_Box box_of(const Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape.occt(), box);
  return box;
}

std::string reported_design() {
  const std::string folder = env("MITCAD_MITCAD_REPORTED");
  if (folder.empty() || !std::filesystem::is_directory(folder)) {
    return {};
  }
  for (const auto& entry : std::filesystem::directory_iterator(folder)) {
    const std::string name = entry.path().filename().string();
    if (name.rfind("github-133-", 0) == 0 && entry.path().extension() == ".mitcad") {
      std::ifstream in(entry.path(), std::ios::binary);
      std::ostringstream text;
      text << in.rdbuf();
      return text.str();
    }
  }
  return {};
}

void test_reported_nut_in_a_threaded_carriage() {
  const std::string design = reported_design();
  if (design.empty()) {
    std::printf("test_reported_nut_in_a_threaded_carriage: skipped (MITCAD_MITCAD_REPORTED has no "
                "github-133 design)\n");
    return;
  }
  const ShapePtr carriage = reported_body(design, 157);
  const ShapePtr nut = reported_body(design, 158);
  const ShapePtr lower = reported_body(design, 166);
  const ShapePtr third = reported_body(design, 174);
  CHECK(carriage && nut && lower && third);
  if (!carriage || !nut || !lower || !third) {
    return;
  }
  // The test's premise: OCCT's exact union of the carriage and the nut
  // has no solid, and the nut lies inside the carriage (their common
  // volume is the nut's).
  const int exact = [&] {
    BRepAlgoAPI_Fuse fuse;
    NCollection_List<TopoDS_Shape> arguments;
    NCollection_List<TopoDS_Shape> tools;
    arguments.Append(carriage->occt());
    tools.Append(nut->occt());
    fuse.SetArguments(arguments);
    fuse.SetTools(tools);
    fuse.SetNonDestructive(true);
    fuse.Build();
    int n = 0;
    for (TopExp_Explorer it(fuse.Shape(), TopAbs_SOLID); it.More(); it.Next()) {
      ++n;
    }
    return n;
  }();
  CHECK(exact == 0);
  const BooleanResult common = mitcad::geometry::boolean(BooleanOp::Intersect, {carriage.get()}, *nut);
  CHECK(common.pieces.size() == 1);
  const double inside = common.pieces.empty() ? 0.0 : volume_of(*common.pieces[0].shape);
  CHECK_NEAR_TOL(inside, volume_of(*nut), 1e-6);
  // The union is the carriage with the nut: one valid solid of the volume
  // of the two less their common volume.
  const double expected = volume_of(*carriage) + volume_of(*nut) - inside;
  const ShapePtr joined = mitcad::geometry::unite({carriage.get(), nut.get()});
  CHECK(solid_count(*joined) == 1);
  CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*joined), expected, 1e-6);
  std::printf("  carriage %.6f, nut %.6f, common %.6f, union %.6f\n", volume_of(*carriage), volume_of(*nut),
              inside, volume_of(*joined));
  // So is a join of the nut into the carriage (an Extrude's or a pattern's
  // join), which had no piece.
  const BooleanResult join = mitcad::geometry::boolean(BooleanOp::Join, {carriage.get()}, *nut);
  CHECK(join.pieces.size() == 1);
  if (join.pieces.size() == 1) {
    CHECK((join.pieces[0].sources == std::vector<std::size_t>{0}));
    CHECK(BRepCheck_Analyzer(join.pieces[0].shape->occt()).IsValid());
    CHECK_NEAR_TOL(volume_of(*join.pieces[0].shape), expected, 1e-6);
  }
  // It holds what the two hold, and nothing else, around the nut.
  Bnd_Box around = box_of(*nut);
  around.Enlarge(2.0);
  CHECK(misplaced_points(*joined, {carriage.get(), nut.get()}, around, 1000) == 0);
  // With the nuts of the carriage's other two holes, which join exactly.
  const ShapePtr all = mitcad::geometry::unite({carriage.get(), nut.get(), lower.get(), third.get()});
  CHECK(solid_count(*all) == 1);
  CHECK(BRepCheck_Analyzer(all->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*all), expected, 1e-6);
  // A mount whose stud stands in the third hole (body 179), coaxial with
  // the thread: OCCT's exact union of the carriage, that hole's nut and
  // the mount gave three solids, warning only that sub-shapes of the
  // carriage and the nut had come to meet; with a fuzzy value of 1e-5 mm
  // the carriage and the mount gave one solid that missed most of their
  // common volume, without a warning.
  const ShapePtr mount = reported_body(design, 179);
  CHECK(mount != nullptr);
  if (mount) {
    const ShapePtr stud = mitcad::geometry::unite({carriage.get(), third.get(), mount.get()});
    CHECK(solid_count(*stud) == 1);
    CHECK(BRepCheck_Analyzer(stud->occt()).IsValid());
    Bnd_Box hole = box_of(*third);
    hole.Add(box_of(*mount));
    CHECK(misplaced_points(*stud, {carriage.get(), third.get(), mount.get()}, hole, 1000) == 0);
    std::printf("  carriage, nut and mount %.6f\n", volume_of(*stud));
  }
}

void test_reported_plates_a_tenth_of_a_micrometre_aside() {
  // Two copies of the design's back plate (bodies 23 and 44), the second
  // 0.523 mm above the first and 9.87e-5 mm aside: their side walls lie
  // 1e-4 mm apart. OCCT's exact union left edges of eight faces of each
  // unused and gave the lower plate alone, a valid solid of its volume;
  // with a fuzzy value of 1e-4 mm it gives the two. With a block standing
  // on them (body 37), the exact union gave no solid.
  const std::string design = reported_design();
  if (design.empty()) {
    std::printf("test_reported_plates_a_tenth_of_a_micrometre_aside: skipped\n");
    return;
  }
  const ShapePtr lower = reported_body(design, 23);
  const ShapePtr upper = reported_body(design, 44);
  const ShapePtr block = reported_body(design, 37);
  CHECK(lower && upper && block);
  if (!lower || !upper || !block) {
    return;
  }
  const ShapePtr joined = mitcad::geometry::unite({lower.get(), upper.get()});
  CHECK(solid_count(*joined) == 1);
  CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
  // More than either plate: the upper plate's top 0.523 mm.
  CHECK(volume_of(*joined) > volume_of(*lower) + 1000.0);
  CHECK(misplaced_points(*joined, {lower.get(), upper.get()}, box_of(*upper), 3000) == 0);
  const ShapePtr three = mitcad::geometry::unite({lower.get(), block.get(), upper.get()});
  CHECK(solid_count(*three) == 1);
  CHECK(BRepCheck_Analyzer(three->occt()).IsValid());
  Bnd_Box all = box_of(*upper);
  all.Add(box_of(*block));
  CHECK(misplaced_points(*three, {lower.get(), upper.get(), block.get()}, all, 3000) == 0);
  std::printf("  plates %.6f and %.6f, union %.6f, with the block %.6f\n", volume_of(*lower), volume_of(*upper),
              volume_of(*joined), volume_of(*three));
}

// The volume of a boolean's pieces.
double pieces_volume(const BooleanResult& result) {
  double total = 0.0;
  for (const auto& piece : result.pieces) {
    total += volume_of(*piece.shape);
  }
  return total;
}

// Points of the box (a fixed sequence of `count`) that a cut of `a` by `b`
// (or their intersection) puts wrongly: inside the result but not inside
// `a` without `b` (inside both), or the other way round; points within
// 1e-3 mm of a boundary count as either.
int misplaced_by(const BooleanResult& result, const Shape& a, const Shape& b, bool cut, const Bnd_Box& box,
                 int count) {
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> pieces;
  for (const auto& piece : result.pieces) {
    pieces.push_back(std::make_unique<BRepClass3d_SolidClassifier>(piece.shape->occt()));
  }
  BRepClass3d_SolidClassifier in_a(a.occt());
  BRepClass3d_SolidClassifier in_b(b.occt());
  unsigned state = 4321u;
  const auto next = [&state] {
    state = state * 1664525u + 1013904223u;
    return static_cast<double>(state >> 8) / 16777216.0;
  };
  int wrong = 0;
  for (int k = 0; k < count; ++k) {
    const gp_Pnt point(xmin + (xmax - xmin) * next(), ymin + (ymax - ymin) * next(),
                       zmin + (zmax - zmin) * next());
    in_a.Perform(point, 1e-3);
    in_b.Perform(point, 1e-3);
    if (in_a.State() == TopAbs_ON || in_b.State() == TopAbs_ON) {
      continue;
    }
    const bool a_in = in_a.State() == TopAbs_IN;
    const bool b_in = in_b.State() == TopAbs_IN;
    const bool expected = cut ? a_in && !b_in : a_in && b_in;
    bool inside = false;
    bool on = false;
    for (const auto& classifier : pieces) {
      classifier->Perform(point, 1e-3);
      inside = inside || classifier->State() == TopAbs_IN;
      on = on || classifier->State() == TopAbs_ON;
    }
    if (!on && inside != expected) {
      ++wrong;
    }
  }
  return wrong;
}

void test_reported_plates_cut_and_intersect() {
  // The two back plates (bodies 23 and 44) whose side walls lie 1e-4 mm
  // apart (mitcad#1278): their cut and their intersection hold what the
  // union says, the intersection the two plates less their union, each cut
  // the union less the other plate.
  const std::string design = reported_design();
  if (design.empty()) {
    std::printf("test_reported_plates_cut_and_intersect: skipped\n");
    return;
  }
  const ShapePtr lower = reported_body(design, 23);
  const ShapePtr upper = reported_body(design, 44);
  CHECK(lower && upper);
  if (!lower || !upper) {
    return;
  }
  const ShapePtr joined = mitcad::geometry::unite({lower.get(), upper.get()});
  const double both = volume_of(*joined);
  const double common = volume_of(*lower) + volume_of(*upper) - both;
  Bnd_Box box = box_of(*upper);
  box.Add(box_of(*lower));
  const BooleanResult intersection = mitcad::geometry::boolean(BooleanOp::Intersect, {lower.get()}, *upper);
  const BooleanResult lower_cut = mitcad::geometry::boolean(BooleanOp::Cut, {lower.get()}, *upper);
  const BooleanResult upper_cut = mitcad::geometry::boolean(BooleanOp::Cut, {upper.get()}, *lower);
  std::printf("  plates %.6f and %.6f, union %.6f: intersection %.6f (%zu), cuts %.6f (%zu) and %.6f (%zu)\n",
              volume_of(*lower), volume_of(*upper), both, pieces_volume(intersection), intersection.pieces.size(),
              pieces_volume(lower_cut), lower_cut.pieces.size(), pieces_volume(upper_cut), upper_cut.pieces.size());
  CHECK_NEAR_TOL(pieces_volume(intersection), common, 1e-6);
  CHECK_NEAR_TOL(pieces_volume(lower_cut), both - volume_of(*upper), 1e-6);
  CHECK_NEAR_TOL(pieces_volume(upper_cut), both - volume_of(*lower), 1e-6);
  for (const BooleanResult* result : {&intersection, &lower_cut, &upper_cut}) {
    for (const auto& piece : result->pieces) {
      CHECK(BRepCheck_Analyzer(piece.shape->occt()).IsValid());
    }
  }
  CHECK(misplaced_by(intersection, *lower, *upper, false, box, 3000) == 0);
  CHECK(misplaced_by(lower_cut, *lower, *upper, true, box, 3000) == 0);
  CHECK(misplaced_by(upper_cut, *upper, *lower, true, box, 3000) == 0);
}

void test_reported_join_of_all_bodies() {
  // The design's Combine: one union of its 257 bodies (mitcad#1278).
  const std::string design = reported_design();
  if (design.empty()) {
    std::printf("test_reported_join_of_all_bodies: skipped\n");
    return;
  }
  const std::vector<ShapePtr> bodies = reported_bodies(design);
  CHECK(bodies.size() == 257);
  std::vector<const Shape*> inputs;
  std::vector<Bnd_Box> boxes;
  for (const ShapePtr& body : bodies) {
    CHECK(body != nullptr);
    if (!body) {
      return;
    }
    inputs.push_back(body.get());
    boxes.push_back(box_of(*body));
  }
  const auto start = std::chrono::steady_clock::now();
  const ShapePtr joined = mitcad::geometry::unite(inputs);
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  int shells = 0;
  for (TopExp_Explorer it(joined->occt(), TopAbs_SHELL); it.More(); it.Next()) {
    ++shells;
  }
  std::printf("  union of %zu bodies: %.1f s, %d solid(s) of %d shells, %d faces, volume %.3f\n", inputs.size(),
              seconds, solid_count(*joined), shells, joined->face_count(), volume_of(*joined));
  // Seven solids (the machine and the parts apart from it) of 76 shells
  // (the machine's closed cavities), the volume of #1273's union within
  // 1e-7 of it, and a point in each body's box placed rightly.
  CHECK(solid_count(*joined) == 7);
  CHECK(shells == 76);
  CHECK_NEAR_TOL(volume_of(*joined), 50311221.665, 1e-7);
  CHECK(misplaced_points(*joined, inputs, boxes, 1) == 0);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_groups_of_meeting_boxes", test::test_groups_of_meeting_boxes);
  test::guarded("test_near_miss_in_a_row", test::test_near_miss_in_a_row);
  test::guarded("test_reported_nut_in_a_threaded_carriage", test::test_reported_nut_in_a_threaded_carriage);
  test::guarded("test_reported_plates_a_tenth_of_a_micrometre_aside",
                test::test_reported_plates_a_tenth_of_a_micrometre_aside);
  test::guarded("test_reported_plates_cut_and_intersect", test::test_reported_plates_cut_and_intersect);
  test::guarded("test_reported_join_of_all_bodies", test::test_reported_join_of_all_bodies);
  if (test::failures != 0) {
    std::printf("%d check(s) failed\n", test::failures);
  }
  return test::failures == 0 ? 0 : 1;
}
