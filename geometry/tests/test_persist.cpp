// SPDX-License-Identifier: MIT
// Shapes with their names as bytes (P7d): a shape read back has the same
// faces, face and edge names, notes, volume and what it had measured, and
// later operations find their references on it; damaged bytes are refused.

#include <cstddef>
#include <cstdint>
#include <string>

#include "check.hpp"
#include "mitcad/geometry/persist.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// A block with a boss joined onto it and one edge filleted.
ShapePtr part() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const ShapePtr boss = extrude("F4", {circle(5, 30, 20, 8)}, 20, 35);
  const BooleanResult joined = boolean(BooleanOp::Join, {block.get()}, *boss);
  const ShapePtr body = joined.pieces.at(0).shape;
  return fillet("F5", *body, std::vector<std::string>{edge(side("F2", 1, 0), side("F2", 1, 1))}, 3.0);
}

void check_same(const Shape& a, const Shape& b) {
  CHECK(a.face_count() == b.face_count());
  CHECK(a.edge_count() == b.edge_count());
  CHECK(a.vertex_count() == b.vertex_count());
  if (a.face_count() != b.face_count() || a.edge_count() != b.edge_count()) {
    return;
  }
  for (int i = 0; i < a.face_count(); ++i) {
    CHECK(a.face_names(i) == b.face_names(i));
  }
  for (int i = 0; i < a.edge_count(); ++i) {
    CHECK(a.edge_name(i) == b.edge_name(i));
  }
  for (int i = 0; i < a.vertex_count(); ++i) {
    CHECK(a.vertex_name(i) == b.vertex_name(i));
  }
}

void test_round_trip_keeps_names_and_measures() {
  const ShapePtr original = part();
  CHECK(edge_names_unique(*original));
  // Not measured yet: the copy is not either, and measures the same.
  const ShapePtr fresh = deserialize_shape(serialize_shape(*original));
  check_same(*original, *fresh);
  CHECK(!fresh->measured() && !fresh->bounds());
  const MassProperties props = mass_properties(*original);
  CHECK_NEAR(mass_properties(*fresh).volume, props.volume);
  // Measured: the copy has the same values without measuring.
  bounding_box(*original);
  const ShapePtr copy = deserialize_shape(serialize_shape(*original));
  CHECK(copy->measured() == original->measured());
  CHECK(copy->bounds() == original->bounds());
  check_same(*original, *copy);
  CHECK(faces_named(*copy, "F5:fillet(" + edge(side("F2", 1, 0), side("F2", 1, 1)) + ")") == 1);
  CHECK(edge_names_unique(*copy));
  // A later operation finds its edge on the copy and builds the same.
  const std::string top = edge(side("F2", 1, 2), end_cap("F2", 1));
  const ShapePtr again = fillet("F6", *copy, std::vector<std::string>{top}, 2.0);
  const ShapePtr direct = fillet("F6", *original, std::vector<std::string>{top}, 2.0);
  CHECK_NEAR(mass_properties(*again).volume, mass_properties(*direct).volume);
  check_same(*direct, *again);
}

void test_notes_and_unnamed_faces() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 10, 10)}, 0, 5);
  // Faces without names stay without; notes come back.
  Shape plain(block->occt());
  plain.add_note("built 0.1 % smaller");
  const ShapePtr copy = deserialize_shape(serialize_shape(plain));
  CHECK(copy->notes() == plain.notes());
  for (int i = 0; i < copy->face_count(); ++i) {
    CHECK(copy->face_names(i).empty());
  }
}

void test_damaged_bytes_are_refused() {
  const std::string bytes = serialize_shape(*part());
  CHECK(throws_with([&] { deserialize_shape(bytes.substr(0, bytes.size() / 2)); }, "stored shape"));
  CHECK(throws_with([&] { deserialize_shape(bytes + "x"); }, "data after the shape"));
  std::string magic = bytes;
  magic[0] = 'X';
  CHECK(throws_with([&] { deserialize_shape(magic); }, "not a shape"));
  std::string format = bytes;
  format[4] = 9;
  CHECK(throws_with([&] { deserialize_shape(format); }, "format 9"));
  CHECK(throws_with([&] { serialize_shape(Shape(TopoDS_Shape())); }, "empty"));
  // The counts after the B-rep data: one face more than the B-rep has.
  std::string faces = bytes;
  std::uint64_t brep = 0;
  for (std::size_t i = 0; i < 8; ++i) {
    brep |= static_cast<std::uint64_t>(static_cast<unsigned char>(faces[8 + i])) << (8 * i);
  }
  const std::size_t count = 16 + static_cast<std::size_t>(brep);
  faces[count] = static_cast<char>(faces[count] + 1);
  CHECK(throws_with([&] { deserialize_shape(faces); }, "do not match their names"));
}

void test_build_id() {
  const std::string id = kernel_build_id();
  CHECK(id.find("occt ") != std::string::npos);
  CHECK(id.find("program unknown") == std::string::npos);
  CHECK(id == kernel_build_id());
}

// What the caches' diagnostics read: more faces take more memory, and the
// process's memory is known on the platforms Mitcad runs on.
void test_memory() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 10, 10)}, 0, 5);
  const ShapePtr detailed = part();
  const std::size_t small = memory_estimate(*block);
  CHECK(small > 4000);
  CHECK(memory_estimate(*detailed) > small);
  CHECK(memory_estimate(*deserialize_shape(serialize_shape(*detailed))) == memory_estimate(*detailed));
  const ProcessMemory memory = process_memory();
  CHECK(memory.resident > 1000000);
  CHECK(memory.peak_resident >= memory.resident);
  CHECK(physical_memory() > memory.resident);
}

} // namespace

void persist_tests() {
  guarded("round trip", test_round_trip_keeps_names_and_measures);
  guarded("notes", test_notes_and_unnamed_faces);
  guarded("damaged", test_damaged_bytes_are_refused);
  guarded("build id", test_build_id);
  guarded("memory", test_memory);
}

} // namespace test
