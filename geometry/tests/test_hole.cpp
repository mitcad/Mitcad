// SPDX-License-Identifier: MIT
// Holes: types, drill points, extents, several positions and names, and the
// face and edge queries features place them with. Volumes follow the T0
// models (hole_*).

#include <cmath>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kDegree = kPi / 180.0;

// The 60 x 40 x 20 block of the T0 hole models.
ShapePtr block() { return extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20); }

// A hole at (30, 20) on the block's top, pointing down.
HoleSpec at_top(double diameter) {
  HoleSpec spec;
  spec.feature = "F3";
  spec.positions = {gp_Ax1(gp_Pnt(30, 20, 20), gp_Dir(0, 0, -1))};
  spec.diameter = diameter;
  return spec;
}

double cut_volume(const ShapePtr& body, const HoleSpec& spec) {
  const BooleanResult cut = boolean(BooleanOp::Cut, {body.get()}, *hole_tool(spec));
  return cut.pieces.size() == 1 ? volume(*cut.pieces.front().shape) : -1.0;
}

void test_blind_holes() {
  // hole_simple_distance: 10 mm, 10 deep to the shoulder, 118 degree point.
  HoleSpec spec = at_top(10);
  spec.depth = 10;
  const ShapePtr tool = hole_tool(spec);
  const double point = 5 / std::tan(59 * kDegree);
  CHECK(near(volume(*tool), kPi * 25 * 10 + kPi * 25 * point / 3));
  CHECK(faces_named(*tool, "F3:hole0.wall") == 1 && faces_named(*tool, "F3:hole0.tip") == 1);
  CHECK(faces_named(*tool, "F3:hole0.top") == 1 && tool->face_count() == 3);
  const ShapePtr body = block();
  const BooleanResult cut = boolean(BooleanOp::Cut, {body.get()}, *tool);
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() == 1) {
    const Shape& drilled = *cut.pieces.front().shape;
    CHECK(near(volume(drilled), 47135.95, 1e-6));
    CHECK(faces_named(drilled, "F3:hole0.wall") == 1 && faces_named(drilled, "F3:hole0.tip") == 1);
    CHECK(drilled.find_faces("F3:hole0.top").empty());
    CHECK(drilled.find_edges(edge("F3:hole0.tip", "F3:hole0.wall")).size() == 1);
    CHECK(edge_names_unique(drilled));
  }
  // hole_tip_90 and a flat bottom.
  spec.tip_angle = 90 * kDegree;
  CHECK(near(cut_volume(body, spec), 48000 - kPi * 25 * 10 - kPi * 25 * 5 / 3));
  spec.flat = true;
  CHECK(near(cut_volume(body, spec), 48000 - kPi * 25 * 10));
}

void test_through_and_types() {
  const ShapePtr body = block();
  // hole_through_simple: 6.6 mm through all.
  HoleSpec spec = at_top(6.6);
  spec.extent = HoleExtent::ThroughAll;
  spec.bodies = {body};
  CHECK(near(cut_volume(body, spec), 48000 - kPi * 3.3 * 3.3 * 20));
  // hole_counterbore: 11 mm, 6.4 deep.
  spec.type = HoleType::Counterbore;
  spec.counterbore_diameter = 11;
  spec.counterbore_depth = 6.4;
  CHECK(near(cut_volume(body, spec), 48000 - kPi * (3.3 * 3.3 * (20 - 6.4) + 5.5 * 5.5 * 6.4)));
  CHECK(faces_named(*hole_tool(spec), "F3:hole0.cbore_floor") == 1);
  spec.counterbore_diameter = 6;
  CHECK(throws_with([&] { hole_tool(spec); }, "wider than the hole"));
  // hole_countersink: 12 mm at 90 degrees, a cone 2.7 deep.
  spec.type = HoleType::Countersink;
  spec.countersink_diameter = 12;
  spec.countersink_angle = 90 * kDegree;
  CHECK(near(cut_volume(body, spec),
             48000 - kPi * 3.3 * 3.3 * (20 - 2.7) - kPi * 2.7 / 3 * (36 + 6 * 3.3 + 3.3 * 3.3)));
  CHECK(faces_named(*hole_tool(spec), "F3:hole0.csink") == 1);
  // Nothing to drill through.
  spec.bodies.clear();
  CHECK(throws_with([&] { hole_tool(spec); }, "no body to drill through"));
}

void test_targets_and_positions() {
  const ShapePtr body = block();
  // hole_to_object: 8 mm down to a plane 5 mm above the bottom; the point
  // goes past it.
  HoleSpec spec = at_top(8);
  spec.extent = HoleExtent::Target;
  Target plane;
  plane.plane = gp_Pln(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1));
  spec.target = plane;
  const double point = 4 / std::tan(59 * kDegree);
  CHECK(near(cut_volume(body, spec), 48000 - kPi * 16 * 15 - kPi * 16 * point / 3));
  // To the body: its far face from the top, here the bottom.
  Target to_body;
  to_body.kind = Target::Kind::Body;
  to_body.body = body;
  spec.target = to_body;
  spec.flat = true;
  CHECK(near(cut_volume(body, spec), 48000 - kPi * 16 * 20));
  // A plane above the start is not reached.
  spec.target = plane;
  spec.target->plane = gp_Pln(gp_Pnt(0, 0, 30), gp_Dir(0, 0, 1));
  CHECK(throws_with([&] { hole_tool(spec); }, "behind the start"));

  // hole_sketch_points: four 5 mm holes through a 10 mm plate.
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 10);
  HoleSpec four = at_top(5);
  four.positions.clear();
  const double corners[4][2] = {{10, 10}, {50, 10}, {50, 30}, {10, 30}};
  for (const auto& corner : corners) {
    four.positions.emplace_back(gp_Pnt(corner[0], corner[1], 10), gp_Dir(0, 0, -1));
  }
  four.extent = HoleExtent::ThroughAll;
  four.bodies = {plate};
  CHECK(near(cut_volume(plate, four), 60 * 40 * 10 - 4 * kPi * 2.5 * 2.5 * 10));
  const ShapePtr tools = hole_tool(four);
  CHECK(faces_named(*tools, "F3:hole3.wall") == 1 && solids(*tools).size() == 4);
}

void test_queries() {
  const ShapePtr body = block();
  const gp_Pln top = face_plane(*body, end_cap("F2", 1));
  CHECK(top.Axis().Direction().IsEqual(gp_Dir(0, 0, 1), 1e-9) && near(top.Location().Z(), 20));
  CHECK(near(top.Location().X(), 30) && near(top.Location().Y(), 20));
  CHECK(face_plane(*body, start_cap("F2", 1)).Axis().Direction().IsEqual(gp_Dir(0, 0, -1), 1e-9));
  CHECK(throws_with([&] { face_plane(*body, "F2:side(c9)"); }, "no face"));

  // A rod and a bore: the bore's wall is internal.
  const ShapePtr rod = extrude("F4", {circle(5, 0, 0, 3)}, 0, 6);
  const CylinderFace outside = face_cylinder(*rod, "F4:side(c5)");
  CHECK(near(outside.radius, 3) && near(outside.length, 6) && !outside.internal);
  CHECK(throws_with([&] { face_cylinder(*rod, "F4:end(r{c5})"); }, "not cylindrical"));
  HoleSpec bore = at_top(8);
  bore.extent = HoleExtent::ThroughAll;
  bore.bodies = {body};
  const BooleanResult drilled = boolean(BooleanOp::Cut, {body.get()}, *hole_tool(bore));
  const CylinderFace inside = face_cylinder(*drilled.pieces.at(0).shape, "F3:hole0.wall");
  CHECK(near(inside.radius, 4) && near(inside.length, 20) && inside.internal);
  // The wall's axis points along the hole, so its start is the low end
  // (the model's threads query relies on this).
  CHECK(inside.axis.Direction().IsEqual(gp_Dir(0, 0, -1), 1e-9) && near(inside.axis.Location().Z(), 20));
}

} // namespace

void hole_tests() {
  guarded("test_blind_holes", test_blind_holes);
  guarded("test_through_and_types", test_through_and_types);
  guarded("test_targets_and_positions", test_targets_and_positions);
  guarded("test_queries", test_queries);
}

} // namespace test
