// SPDX-License-Identifier: MIT
// Extrusion: face names from the profile, derived edge names, extents and
// several profiles.

#include <BRep_Tool.hxx>
#include <TopExp.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// True when the edge is a straight vertical segment standing at (x, y).
bool vertical_at(const TopoDS_Edge& e, double x, double y) {
  const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
  const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
  return near(a.X(), x) && near(a.Y(), y) && near(b.X(), x) && near(b.Y(), y) && !near(a.Z(), b.Z());
}

// The single edge a reference resolves to must stand vertically at (x, y).
bool named_vertical_edge_at(const Shape& shape, const std::string& name, double x, double y) {
  const std::vector<int> found = shape.find_edges(name);
  return found.size() == 1 && vertical_at(shape.edge(found[0]), x, y);
}

void test_block_faces_and_edges_are_named() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  CHECK(near(volume(*block), 60.0 * 40.0 * 20.0));
  CHECK(block->face_count() == 6 && block->edge_count() == 12 && block->vertex_count() == 8);
  for (int i = 0; i < 4; ++i) {
    CHECK(faces_named(*block, side("F2", 1, i)) == 1);
  }
  CHECK(faces_named(*block, start_cap("F2", 1)) == 1);
  CHECK(faces_named(*block, end_cap("F2", 1)) == 1);
  for (int i = 0; i < block->face_count(); ++i) {
    CHECK(block->face_names(i).size() == 1);
  }
  CHECK(edge_names_unique(*block));
  for (int i = 0; i < block->vertex_count(); ++i) {
    CHECK(block->find_vertices(block->vertex_name(i)) == std::vector<int>{i});
  }

  // The start cap lies on the sketch plane, the end cap at the far end.
  const std::vector<int> start = block->find_faces(start_cap("F2", 1));
  const std::vector<int> end = block->find_faces(end_cap("F2", 1));
  CHECK(start.size() == 1 && end.size() == 1);
  if (start.size() == 1 && end.size() == 1) {
    CHECK(near(bounding_box(Shape(block->face(start[0]))).max.Z(), 0.0));
    CHECK(near(bounding_box(Shape(block->face(end[0]))).min.Z(), 20.0));
  }

  // Line 0 runs along y = 0 and line 1 along x = 60: the vertical edge
  // between their side faces stands at corner (60, 0).
  const std::string corner = edge(side("F2", 1, 0), side("F2", 1, 1));
  CHECK(named_vertical_edge_at(*block, corner, 60, 0));
  CHECK(block->name_of_edge(block->edge(block->find_edges(corner).at(0))) == corner);
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  CHECK(block->find_edges(top_front).size() == 1);
  CHECK(block->find_edges(edge(side("F2", 1, 0), side("F2", 1, 2))).empty());
  CHECK(block->find_edges("not an edge").empty());
}

void test_names_survive_a_dimension_change() {
  const std::string corner = edge(side("F2", 1, 0), side("F2", 1, 1));
  const std::string back_left = edge(side("F2", 1, 2), side("F2", 1, 3));
  const ShapePtr narrow = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const ShapePtr wide = extrude("F2", {rectangle(1, 0, 0, 80, 50)}, 0, 35);
  CHECK(named_vertical_edge_at(*narrow, corner, 60, 0));
  CHECK(named_vertical_edge_at(*wide, corner, 80, 0));
  CHECK(named_vertical_edge_at(*narrow, back_left, 0, 40));
  CHECK(named_vertical_edge_at(*wide, back_left, 0, 50));
  // The feature is part of the name.
  CHECK(wide->find_edges(edge(side("F3", 1, 0), side("F3", 1, 1))).empty());
}

void test_extents_and_directions() {
  const ShapePtr symmetric = extrude("F2", {rectangle(1, 0, 0, 10, 10)}, -5, 5);
  const BoundingBox box = bounding_box(*symmetric);
  CHECK(near(box.min.Z(), -5) && near(box.max.Z(), 5));
  const std::vector<int> start = symmetric->find_faces(start_cap("F2", 1));
  CHECK(start.size() == 1 && near(bounding_box(Shape(symmetric->face(start.at(0)))).max.Z(), -5));

  ExtrudeSpec down;
  down.feature = "F2";
  down.regions = {rectangle(1, 0, 0, 10, 10)};
  down.direction = gp_Dir(0, 0, -1);
  down.end = 7;
  const BoundingBox below = bounding_box(*mitcad::geometry::extrude(down));
  CHECK(near(below.min.Z(), -7) && near(below.max.Z(), 0));

  // Along a slanted direction the volume is the base times the height.
  ExtrudeSpec slanted = down;
  slanted.direction = gp_Dir(0, 1, 1);
  slanted.end = 10;
  CHECK(near(volume(*mitcad::geometry::extrude(slanted)), 100.0 * 10.0 * std::sqrt(0.5)));

  CHECK(throws_with([] { extrude("F2", {}, 0, 1); }, "no profiles"));
  CHECK(throws_with([] { extrude("F2", {rectangle(1, 0, 0, 1, 1)}, 3, 3); }, "no length"));
  ExtrudeSpec flat = down;
  flat.direction = gp_Dir(1, 0, 0);
  CHECK(throws_with([&] { mitcad::geometry::extrude(flat); }, "lies in the sketch plane"));
}

void test_cylinder_names() {
  const ShapePtr cylinder = extrude("F4", {circle(5, 0, 0, 5)}, 0, 10);
  CHECK(near(volume(*cylinder), kPi * 25.0 * 10.0));
  CHECK(cylinder->face_count() == 3);
  CHECK(faces_named(*cylinder, "F4:side(c5)") == 1);
  CHECK(faces_named(*cylinder, "F4:start(r{c5})") == 1);
  CHECK(faces_named(*cylinder, "F4:end(r{c5})") == 1);
  CHECK(edge_names_unique(*cylinder));
  // The seam lies between the side face and itself.
  CHECK(cylinder->find_edges("E{F4:side(c5)|F4:side(c5)}").size() == 1);
  CHECK(cylinder->find_edges("E{F4:end(r{c5})|F4:side(c5)}").size() == 1);
}

void test_several_profiles() {
  // Apart: two solids, each with its own names.
  const ShapePtr apart = extrude("F3", {rectangle(1, 0, 0, 10, 10), rectangle(5, 30, 0, 10, 10)}, 0, 5);
  const std::vector<ShapePtr> pieces = solids(*apart);
  CHECK(pieces.size() == 2);
  if (pieces.size() == 2) {
    CHECK(faces_named(*pieces[0], end_cap("F3", 1)) == 1 && faces_named(*pieces[0], end_cap("F3", 5)) == 0);
    CHECK(faces_named(*pieces[1], end_cap("F3", 5)) == 1);
    CHECK(near(volume(*pieces[0]) + volume(*pieces[1]), 1000.0));
  }

  // Touching: one solid. The shared side faces disappear; the coplanar caps
  // and front faces merge and carry the names of both profiles.
  const ShapePtr joined = extrude("F3", {rectangle(1, 0, 0, 10, 10), rectangle(5, 10, 0, 10, 10)}, 0, 5);
  CHECK(solids(*joined).size() == 1);
  CHECK(near(volume(*joined), 1000.0));
  CHECK(joined->face_count() == 6);
  const std::vector<int> top = joined->find_faces(end_cap("F3", 1));
  CHECK(top.size() == 1 && top == joined->find_faces(end_cap("F3", 5)));
  CHECK(joined->find_faces(side("F3", 1, 0)) == joined->find_faces(side("F3", 5, 0)));
  CHECK(joined->find_faces(side("F3", 1, 1)).empty()); // x = 10, inside
  CHECK(edge_names_unique(*joined));
}

} // namespace

void extrude_tests() {
  test_block_faces_and_edges_are_named();
  test_names_survive_a_dimension_change();
  test_extents_and_directions();
  test_cylinder_names();
  test_several_profiles();
}

} // namespace test
