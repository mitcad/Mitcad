// SPDX-License-Identifier: MIT
// Holes: types, drill points, extents, several positions and names, and the
// face and edge queries features place them with. Volumes follow the T0
// models (hole_*).

#include <cmath>
#include <optional>
#include <string>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

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

// Twice the distance from the axis of the hole at (30, 20) to the body it
// drilled: the diameter its flats or crush ribs' tips touch (mitcad#1517).
// The axis is taken from 1 to 5 mm below the top, farther from the hole's
// rim and from a 10 mm deep hole's bottom than from its sides.
double inner_diameter(const ShapePtr& body, const HoleSpec& spec) {
  const BooleanResult cut = boolean(BooleanOp::Cut, {body.get()}, *hole_tool(spec));
  if (cut.pieces.size() != 1) {
    return -1.0;
  }
  const TopoDS_Edge axis = BRepBuilderAPI_MakeEdge(gp_Pnt(30, 20, 19), gp_Pnt(30, 20, 15)).Edge();
  BRepExtrema_DistShapeShape distance(axis, cut.pieces.front().shape->occt());
  return distance.IsDone() ? 2.0 * distance.Value() : -1.0;
}

void test_press_fits() {
  const ShapePtr body = block();
  // A 5 mm pin in a hex hole whose flats lie 0.2 mm under it, through the
  // block.
  HoleSpec spec = at_top(5);
  spec.extent = HoleExtent::ThroughAll;
  spec.bodies = {body};
  spec.press_fit = PressFit::Polygon;
  spec.press_fit_count = 6;
  spec.press_fit_inner = 4.8;
  const ShapePtr hex = hole_tool(spec);
  for (int k = 0; k < 6; ++k) {
    CHECK(faces_named(*hex, "F3:hole0.flat" + std::to_string(k)) == 1);
  }
  CHECK(faces_named(*hex, "F3:hole0.top") == 1 && faces_named(*hex, "F3:hole0.tip") == 1);
  CHECK(hex->face_count() == 8);
  CHECK(near(cut_volume(body, spec), 48000 - std::sqrt(3.0) / 2.0 * 4.8 * 4.8 * 20));
  CHECK(near(inner_diameter(body, spec), 4.8));
  // A square turned 45 degrees, 10 deep with a flat bottom (no drill
  // point).
  spec.press_fit_count = 4;
  spec.press_fit_angle = 45 * kDegree;
  spec.extent = HoleExtent::Distance;
  spec.depth = 10;
  CHECK(faces_named(*hole_tool(spec), "F3:hole0.flat3") == 1);
  CHECK(near(cut_volume(body, spec), 48000 - 4.8 * 4.8 * 10));
  CHECK(near(inner_diameter(body, spec), 4.8));

  // An 8 mm pin in three crush ribs: tips 0.2 mm under it, 0.3 mm high, so
  // the bore is 8.4 mm.
  HoleSpec ribs = at_top(8);
  ribs.extent = HoleExtent::ThroughAll;
  ribs.bodies = {body};
  ribs.press_fit = PressFit::CrushRibs;
  ribs.press_fit_count = 3;
  ribs.press_fit_inner = 7.8;
  ribs.press_fit_bore = 8.4;
  const ShapePtr ribbed = hole_tool(ribs);
  for (int k = 0; k < 3; ++k) {
    const std::string rib = "F3:hole0.rib" + std::to_string(k);
    CHECK(faces_named(*ribbed, rib + ".side0") == 1 && faces_named(*ribbed, rib + ".side1") == 1);
    CHECK(faces_named(*ribbed, "F3:hole0.wall" + std::to_string(k)) == 1);
  }
  CHECK(ribbed->face_count() == 11);
  // Each rib is the bore's sector over its base less the two triangles
  // from the axis to its flanks.
  const double r = 4.2;
  const double half = std::asin(0.3 / r);
  const double rib_area = r * r * half - 3.9 * r * std::sin(half);
  CHECK(near(cut_volume(body, ribs), 48000 - (kPi * r * r - 3 * rib_area) * 20));
  CHECK(near(inner_diameter(body, ribs), 7.8));
  // Ribs that do not fit, and a bore no wider than the tips.
  ribs.press_fit_count = 50;
  CHECK(throws_with([&] { hole_tool(ribs); }, "do not fit round the bore"));
  ribs.press_fit_count = 3;
  ribs.press_fit_bore = 7.8;
  CHECK(throws_with([&] { hole_tool(ribs); }, "bore must be wider"));
}

// A 6 mm hole lying on the bed (mitcad#1515): into the block's side at
// (30, 0, 10) along +y, its roof up +z.
HoleSpec lying(double depth) {
  HoleSpec spec;
  spec.feature = "F3";
  spec.positions = {gp_Ax1(gp_Pnt(30, 0, 10), gp_Dir(0, 1, 0))};
  spec.diameter = 6;
  spec.depth = depth;
  spec.roofs = {gp_Dir(0, 0, 1)};
  return spec;
}

// A teardrop's section of the radius r: the circle's lower three quarters
// and the square of side r between the axis, the sides' feet and the point.
double teardrop_area(double r) { return 0.75 * kPi * r * r + r * r; }

void test_print_shapes() {
  const ShapePtr body = block();
  // A teardrop through the block: 40 mm of its section; the lower arc keeps
  // the diameter, which the tangent sides keep too.
  HoleSpec spec = lying(0);
  spec.extent = HoleExtent::ThroughAll;
  spec.bodies = {body};
  const ShapePtr tool = hole_tool(spec);
  CHECK(faces_named(*tool, "F3:hole0.roof0") == 1 && faces_named(*tool, "F3:hole0.roof1") == 1);
  CHECK(faces_named(*tool, "F3:hole0.wall") == 1 && faces_named(*tool, "F3:hole0.top") == 1);
  CHECK(faces_named(*tool, "F3:hole0.roof") == 0);
  CHECK_NEAR(cut_volume(body, spec), 48000 - teardrop_area(3) * 40);
  const BooleanResult cut = boolean(BooleanOp::Cut, {body.get()}, *tool);
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() == 1) {
    const Shape& drilled = *cut.pieces.front().shape;
    CHECK(faces_named(drilled, "F3:hole0.roof0") == 1 && faces_named(drilled, "F3:hole0.roof1") == 1);
    CHECK(faces_named(drilled, "F3:hole0.wall") == 1);
    CHECK(edge_names_unique(drilled));
    const TopoDS_Edge axis = BRepBuilderAPI_MakeEdge(gp_Pnt(30, 5, 10), gp_Pnt(30, 35, 10)).Edge();
    BRepExtrema_DistShapeShape distance(axis, drilled.occt());
    CHECK(distance.IsDone() && near(distance.Value(), 3.0));
  }

  // A flat roof 0.4 mm above the circle cuts the point's top off: a
  // triangle as high as the point stands above the roof, twice as wide.
  spec.roof_sag = 0.4;
  const ShapePtr flat = hole_tool(spec);
  CHECK(faces_named(*flat, "F3:hole0.roof") == 1 && faces_named(*flat, "F3:hole0.roof0") == 1);
  const double above = 3 * std::sqrt(2.0) - 3.4;
  CHECK_NEAR(cut_volume(body, spec), 48000 - (teardrop_area(3) - above * above) * 40);
  // A roof above the point leaves the point.
  spec.roof_sag = 2;
  CHECK(faces_named(*hole_tool(spec), "F3:hole0.roof") == 0);
  CHECK_NEAR(cut_volume(body, spec), 48000 - teardrop_area(3) * 40);

  // Blind, 15 deep: the drill point beyond the roof's end; flat, the end
  // is the bottom too.
  HoleSpec blind = lying(15);
  const double point = 3 / std::tan(59 * kDegree);
  const ShapePtr pointed = hole_tool(blind);
  CHECK(faces_named(*pointed, "F3:hole0.tip") == 1 && faces_named(*pointed, "F3:hole0.roof_end") == 1);
  CHECK_NEAR(volume(*pointed), teardrop_area(3) * 15 + kPi * 9 * point / 3);
  blind.flat = true;
  const ShapePtr bottomed = hole_tool(blind);
  CHECK(faces_named(*bottomed, "F3:hole0.tip") == 1 && faces_named(*bottomed, "F3:hole0.roof_end") == 1);
  CHECK(bottomed->find_faces("F3:hole0.tip") == bottomed->find_faces("F3:hole0.roof_end"));
  CHECK_NEAR(volume(*bottomed), teardrop_area(3) * 15);

  // A counterbore 11 wide, 6.4 deep gets its own roof; the hole's lies
  // inside it there.
  blind.type = HoleType::Counterbore;
  blind.counterbore_diameter = 11;
  blind.counterbore_depth = 6.4;
  const ShapePtr bored = hole_tool(blind);
  CHECK(faces_named(*bored, "F3:hole0.cbore_roof0") == 1 && faces_named(*bored, "F3:hole0.cbore_roof1") == 1);
  CHECK(faces_named(*bored, "F3:hole0.cbore_wall") == 1 && faces_named(*bored, "F3:hole0.cbore_floor") == 1);
  CHECK_NEAR(volume(*bored), teardrop_area(5.5) * 6.4 + teardrop_area(3) * (15 - 6.4));
  CHECK(cut_volume(body, blind) > 0);

  // A hole with no roof stays round.
  HoleSpec plain = lying(15);
  plain.flat = true;
  plain.roofs = {std::nullopt};
  CHECK_NEAR(volume(*hole_tool(plain)), kPi * 9 * 15);
  CHECK(faces_named(*hole_tool(plain), "F3:hole0.roof0") == 0);

  // A roof along the axis, a roof for only some holes, a taper.
  HoleSpec wrong = lying(15);
  wrong.roofs = {gp_Dir(0, 1, 0)};
  CHECK(throws_with([&] { hole_tool(wrong); }, "right angles to the hole's axis"));
  wrong.roofs = {gp_Dir(0, 0, 1), gp_Dir(0, 0, 1)};
  CHECK(throws_with([&] { hole_tool(wrong); }, "every hole"));
  wrong.roofs = {gp_Dir(0, 0, 1)};
  wrong.taper = 5 * kDegree;
  CHECK(throws_with([&] { hole_tool(wrong); }, "no print shape"));
}

} // namespace

void hole_tests() {
  guarded("test_blind_holes", test_blind_holes);
  guarded("test_through_and_types", test_through_and_types);
  guarded("test_targets_and_positions", test_targets_and_positions);
  guarded("test_queries", test_queries);
  guarded("test_press_fits", test_press_fits);
  guarded("test_print_shapes", test_print_shapes);
}

} // namespace test
