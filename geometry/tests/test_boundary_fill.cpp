// SPDX-License-Identifier: MIT
// Boundary Fill (mitcad#457): the cells that tool bodies enclose, picked by
// points, with the tools' face names; with surface bodies and planes as
// tools (mitcad#2149), the closed regions they bound. The tools are not
// changed.

#include <cmath>
#include <cstddef>
#include <cstdio>
#include <string>
#include <vector>

#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/boundary_fill.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/surfaces.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/primitive.hpp"
#include "mitcad/geometry/query.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using mitcad::geometry::BoundaryCell;
using mitcad::geometry::Shape;

// A box with its lowest corner at `corner` and sides along the axes.
ShapePtr box(const std::string& feature, const gp_Pnt& corner, double length, double width,
             double height) {
  mitcad::geometry::PrimitiveSpec spec;
  spec.feature = feature;
  spec.frame.origin = corner;
  spec.kind = mitcad::geometry::PrimitiveKind::Box;
  spec.a = length;
  spec.b = width;
  spec.c = height;
  return mitcad::geometry::primitive(spec);
}

int solid_count(const Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

bool has_face(const Shape& shape, const std::string& name) { return !shape.find_faces(name).empty(); }

bool all_faces_named(const Shape& shape) {
  for (int i = 0; i < shape.face_count(); ++i) {
    if (shape.face_names(i).empty()) {
      return false;
    }
  }
  return shape.face_count() > 0;
}

void check_box(const Shape& shape, const gp_Pnt& min, const gp_Pnt& max) {
  const mitcad::geometry::BoundingBox b = mitcad::geometry::bounding_box(shape);
  CHECK(!b.empty);
  CHECK_NEAR_TOL(b.min.X(), min.X(), 1e-6);
  CHECK_NEAR_TOL(b.min.Y(), min.Y(), 1e-6);
  CHECK_NEAR_TOL(b.min.Z(), min.Z(), 1e-6);
  CHECK_NEAR_TOL(b.max.X(), max.X(), 1e-6);
  CHECK_NEAR_TOL(b.max.Y(), max.Y(), 1e-6);
  CHECK_NEAR_TOL(b.max.Z(), max.Z(), 1e-6);
}

// Two 20 mm cubes, A at the origin and B at (10, 10, 10): the cube from
// 10 to 20 is in both, an L-shaped block of 7000 mm3 in each alone.
void test_cells_of_two_overlapping_boxes() {
  const ShapePtr a = box("F1", gp_Pnt(0, 0, 0), 20, 20, 20);
  const ShapePtr b = box("F2", gp_Pnt(10, 10, 10), 20, 20, 20);
  const std::vector<BoundaryCell> cells = mitcad::geometry::boundary_cells(
      {a.get(), b.get()},
      {gp_Pnt(15, 15, 15), gp_Pnt(5, 5, 5), gp_Pnt(25, 25, 25), gp_Pnt(12, 18, 11)});
  CHECK(cells.size() == 4);
  if (cells.size() != 4) {
    return;
  }

  const Shape& common = *cells[0].shape;
  CHECK((cells[0].holders == std::vector<std::size_t>{0, 1}));
  CHECK_NEAR(mitcad::geometry::volume(common), 1000.0);
  check_box(common, gp_Pnt(10, 10, 10), gp_Pnt(20, 20, 20));
  CHECK(solid_count(common) == 1);
  CHECK(common.face_count() == 6);
  // Bounded by A's top and far sides and B's bottom and near sides.
  CHECK(has_face(common, "F1:top"));
  CHECK(has_face(common, "F1:side1"));
  CHECK(has_face(common, "F2:bottom"));
  CHECK(has_face(common, "F2:side3"));
  CHECK(!has_face(common, "F1:bottom"));
  CHECK(!has_face(common, "F2:top"));
  CHECK(all_faces_named(common));

  const Shape& only_a = *cells[1].shape;
  CHECK((cells[1].holders == std::vector<std::size_t>{0}));
  CHECK_NEAR(mitcad::geometry::volume(only_a), 7000.0);
  check_box(only_a, gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 20));
  CHECK(solid_count(only_a) == 1);
  // A's six faces and the three faces of B inside A.
  CHECK(only_a.face_count() == 9);
  CHECK(has_face(only_a, "F1:bottom"));
  CHECK(has_face(only_a, "F2:bottom"));
  CHECK(all_faces_named(only_a));

  const Shape& only_b = *cells[2].shape;
  CHECK((cells[2].holders == std::vector<std::size_t>{1}));
  CHECK_NEAR(mitcad::geometry::volume(only_b), 7000.0);
  check_box(only_b, gp_Pnt(10, 10, 10), gp_Pnt(30, 30, 30));
  CHECK(has_face(only_b, "F2:top"));
  CHECK(has_face(only_b, "F1:top"));
  CHECK(all_faces_named(only_b));

  // A second point in the common cell picks the same cell.
  CHECK(cells[3].holders == cells[0].holders);
  CHECK(cells[3].shape == cells[0].shape);

  // The tools are as they were.
  CHECK_NEAR(mitcad::geometry::volume(*a), 8000.0);
  CHECK_NEAR(mitcad::geometry::volume(*b), 8000.0);
}

// A bar crossed by a wall: the part of the bar outside the wall falls apart
// into two blocks, and one point picks both.
void test_a_cell_of_two_solids() {
  const ShapePtr bar = box("F3", gp_Pnt(0, -10, 0), 10, 40, 10);
  const ShapePtr wall = box("F4", gp_Pnt(-5, 8, -5), 20, 4, 20);
  const std::vector<BoundaryCell> cells =
      mitcad::geometry::boundary_cells({bar.get(), wall.get()}, {gp_Pnt(5, 0, 5)});
  CHECK(cells.size() == 1);
  if (cells.size() != 1) {
    return;
  }
  CHECK((cells[0].holders == std::vector<std::size_t>{0}));
  CHECK(solid_count(*cells[0].shape) == 2);
  CHECK_NEAR(mitcad::geometry::volume(*cells[0].shape), 3600.0);
  check_box(*cells[0].shape, gp_Pnt(0, -10, 0), gp_Pnt(10, 30, 10));
  CHECK(has_face(*cells[0].shape, "F4:side0"));
  CHECK(has_face(*cells[0].shape, "F4:side2"));
  CHECK(all_faces_named(*cells[0].shape));
}

// One tool: its cell is all of it.
void test_one_tool() {
  const ShapePtr a = box("F1", gp_Pnt(0, 0, 0), 20, 10, 5);
  const std::vector<BoundaryCell> cells = mitcad::geometry::boundary_cells({a.get()}, {gp_Pnt(1, 1, 1)});
  CHECK(cells.size() == 1);
  if (cells.size() == 1) {
    CHECK_NEAR(mitcad::geometry::volume(*cells[0].shape), 1000.0);
    CHECK(cells[0].shape->face_count() == 6);
    CHECK(has_face(*cells[0].shape, "F1:top"));
  }
}

void test_points_outside_or_on_a_boundary_fail() {
  const ShapePtr a = box("F1", gp_Pnt(0, 0, 0), 20, 20, 20);
  const ShapePtr b = box("F2", gp_Pnt(10, 10, 10), 20, 20, 20);
  CHECK(throws_with([&] { mitcad::geometry::boundary_cells({a.get(), b.get()}, {gp_Pnt(50, 50, 50)}); },
                    "is inside no tool body"));
  CHECK(throws_with([&] { mitcad::geometry::boundary_cells({a.get(), b.get()}, {gp_Pnt(20, 5, 5)}); },
                    "lies on the boundary of tool 1"));
}

// --- Surface bodies and planes as tools (mitcad#2149)

using mitcad::geometry::BoundaryPointPlace;
using mitcad::geometry::BoundaryRegions;

// A 40 x 40 mm surface body (a patch) with its corner at `corner`, along
// `x` and `y`: "<feature>:patch(<name>)".
ShapePtr sheet(const std::string& feature, const std::string& name, const gp_Pnt& corner, const gp_Dir& x,
               const gp_Dir& y) {
  mitcad::geometry::Frame frame;
  frame.origin = corner;
  frame.x_axis = x;
  frame.y_axis = y;
  mitcad::geometry::Region region = mitcad::geometry::rectangle_region(0, 0, 40, 40);
  region.name = name;
  const std::vector<ShapePtr> patches = mitcad::geometry::patch_regions(feature, frame, {region});
  CHECK(patches.size() == 1);
  return patches.empty() ? nullptr : patches.front();
}

// Four walls of the 20 mm cube at the origin, untrimmed: each runs 10 mm
// past the cube on every side, so that they overlap one another.
std::vector<ShapePtr> four_walls() {
  const gp_Dir x(1, 0, 0), y(0, 1, 0), z(0, 0, 1);
  return {sheet("F1", "x0", gp_Pnt(0, -10, -10), y, z), sheet("F2", "x20", gp_Pnt(20, -10, -10), y, z),
          sheet("F3", "y0", gp_Pnt(-10, 0, -10), x, z), sheet("F4", "y20", gp_Pnt(-10, 20, -10), x, z)};
}

std::vector<const Shape*> pointers(const std::vector<ShapePtr>& shapes) {
  std::vector<const Shape*> result;
  for (const ShapePtr& shape : shapes) {
    result.push_back(shape.get());
  }
  return result;
}

gp_Pln plane_z(double z) { return gp_Pln(gp_Pnt(0, 0, z), gp_Dir(0, 0, 1)); }

// Four untrimmed, overlapping surfaces and two planes close off one box: the
// one closed region, filled as a new body, its faces named after the
// surfaces and the planes.
void test_surfaces_and_planes_close_off_one_box() {
  const std::vector<ShapePtr> walls = four_walls();
  const BoundaryRegions found = mitcad::geometry::boundary_regions(
      pointers(walls), {plane_z(0), plane_z(20)}, {"F9:plane1", "F9:plane2"}, {gp_Pnt(10, 10, 10)});
  CHECK(found.regions.size() == 1);
  CHECK(found.places.size() == 1);
  if (found.regions.size() != 1 || found.places.size() != 1) {
    return;
  }
  CHECK(found.places[0].kind == BoundaryPointPlace::Kind::Region);
  CHECK(found.places[0].region == 0);
  const Shape& region = *found.regions[0].shape;
  CHECK(solid_count(region) == 1);
  CHECK_NEAR(mitcad::geometry::volume(region), 8000.0);
  check_box(region, gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 20));
  CHECK(region.face_count() == 6);
  for (const char* name : {"F9:plane1", "F9:plane2", "F1:patch(x0)", "F2:patch(x20)", "F3:patch(y0)",
                           "F4:patch(y20)"}) {
    CHECK(has_face(region, name));
  }
  CHECK(all_faces_named(region));
  // The region's own point lies inside it.
  const gp_Pnt& inside = found.regions[0].point;
  CHECK(inside.X() > 0 && inside.X() < 20 && inside.Y() > 0 && inside.Y() < 20 && inside.Z() > 0 &&
        inside.Z() < 20);
  // Without points: the same region, nothing placed.
  const BoundaryRegions listed =
      mitcad::geometry::boundary_regions(pointers(walls), {plane_z(0), plane_z(20)}, {"a", "b"}, {});
  CHECK(listed.regions.size() == 1);
  CHECK(listed.places.empty());
}

// A surface across a solid divides it into two regions; the chosen one is
// cut out of the solid.
void test_a_surface_across_a_solid_cuts_the_chosen_cell() {
  const ShapePtr block = box("F1", gp_Pnt(0, 0, 0), 20, 20, 20);
  const ShapePtr across = sheet("F2", "mid", gp_Pnt(-10, -10, 10), gp_Dir(1, 0, 0), gp_Dir(0, 1, 0));
  const BoundaryRegions found =
      mitcad::geometry::boundary_regions({block.get(), across.get()}, {}, {}, {gp_Pnt(5, 5, 15)});
  CHECK(found.regions.size() == 2);
  CHECK(found.places.size() == 1);
  if (found.regions.size() != 2 || found.places.size() != 1) {
    return;
  }
  // In the order of their points: the lower half first.
  CHECK(found.places[0].kind == BoundaryPointPlace::Kind::Region);
  CHECK(found.places[0].region == 1);
  const Shape& upper = *found.regions[1].shape;
  CHECK_NEAR(mitcad::geometry::volume(upper), 4000.0);
  check_box(upper, gp_Pnt(0, 0, 10), gp_Pnt(20, 20, 20));
  CHECK(has_face(upper, "F1:top"));
  CHECK(has_face(upper, "F2:patch(mid)"));
  CHECK(all_faces_named(upper));
  // The cut leaves the lower half of the block.
  const mitcad::geometry::BooleanResult cut =
      mitcad::geometry::boolean(mitcad::geometry::BooleanOp::Cut, {block.get()}, upper);
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() == 1) {
    CHECK_NEAR(mitcad::geometry::volume(*cut.pieces[0].shape), 4000.0);
    check_box(*cut.pieces[0].shape, gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 10));
  }
  CHECK_NEAR(mitcad::geometry::volume(*block), 8000.0);
}

// Without the top plane the region is open upwards: no closed region, the
// point placed in an open one with points along the walls' top edges,
// where the region leaks out.
void test_a_region_open_on_one_side() {
  const std::vector<ShapePtr> walls = four_walls();
  const BoundaryRegions found =
      mitcad::geometry::boundary_regions(pointers(walls), {plane_z(0)}, {"F9:plane1"}, {gp_Pnt(10, 10, 10)});
  CHECK(found.regions.empty());
  CHECK(found.places.size() == 1);
  if (found.places.size() != 1) {
    return;
  }
  const BoundaryPointPlace& place = found.places[0];
  CHECK(place.kind == BoundaryPointPlace::Kind::Open);
  CHECK(!place.gap.empty());
  bool top = false;
  for (const gp_Pnt& p : place.gap) {
    // Above the plane: never the walls' edges below it.
    CHECK(p.Z() > -1e-6);
    top = top || std::abs(p.Z() - 30.0) < 1e-6;
  }
  CHECK(top);
  // A point on a wall lies on a face.
  const BoundaryRegions on = mitcad::geometry::boundary_regions(pointers(walls), {plane_z(0), plane_z(20)},
                                                                {"F9:plane1", "F9:plane2"}, {gp_Pnt(0, 10, 10)});
  CHECK(on.places.size() == 1 && on.places[0].kind == BoundaryPointPlace::Kind::OnFace);
}

// Six planes alone close off a box.
void test_planes_alone() {
  const std::vector<gp_Pln> planes = {
      gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)),  gp_Pln(gp_Pnt(10, 0, 0), gp_Dir(1, 0, 0)),
      gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)),  gp_Pln(gp_Pnt(0, 10, 0), gp_Dir(0, 1, 0)),
      gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)),  gp_Pln(gp_Pnt(0, 0, 10), gp_Dir(0, 0, 1))};
  const BoundaryRegions found = mitcad::geometry::boundary_regions(
      {}, planes, {"F7:plane1", "F7:plane2", "F7:plane3", "F7:plane4", "F7:plane5", "F7:plane6"},
      {gp_Pnt(5, 5, 5)});
  CHECK(found.regions.size() == 1);
  CHECK(found.places.size() == 1 && found.places[0].kind == BoundaryPointPlace::Kind::Region);
  if (found.regions.size() == 1) {
    CHECK_NEAR(mitcad::geometry::volume(*found.regions[0].shape), 1000.0);
    CHECK(found.regions[0].shape->face_count() == 6);
    CHECK(all_faces_named(*found.regions[0].shape));
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(boundary_fill) {
  // The tools are read, never changed (T0e).
  mitcad::geometry::enable_input_checks(true);
  test::guarded("test_cells_of_two_overlapping_boxes", test::test_cells_of_two_overlapping_boxes);
  test::guarded("test_a_cell_of_two_solids", test::test_a_cell_of_two_solids);
  test::guarded("test_one_tool", test::test_one_tool);
  test::guarded("test_points_outside_or_on_a_boundary_fail", test::test_points_outside_or_on_a_boundary_fail);
  test::guarded("test_surfaces_and_planes_close_off_one_box", test::test_surfaces_and_planes_close_off_one_box);
  test::guarded("test_a_surface_across_a_solid_cuts_the_chosen_cell",
                test::test_a_surface_across_a_solid_cuts_the_chosen_cell);
  test::guarded("test_a_region_open_on_one_side", test::test_a_region_open_on_one_side);
  test::guarded("test_planes_alone", test::test_planes_alone);
  const std::vector<std::string> changes = mitcad::geometry::input_changes();
  for (const std::string& change : changes) {
    std::fprintf(stderr, "input changed: %s\n", change.c_str());
  }
  CHECK(changes.empty());
  if (test::failures == 0) {
    std::puts("test_boundary_fill: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
