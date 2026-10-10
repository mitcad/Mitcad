// SPDX-License-Identifier: MIT
// Roundings wider than a neighbouring face (mitcad#121): OCCT's rolling
// ball fillet with the edge overflow of the port's patch
// (third_party/vcpkg-ports/opencascade) either runs onto the face beyond
// the narrow one or rolls on the narrow face's far edge; the narrow face
// vanishes. The cross-sections are exact, so are the volumes.

#include <cmath>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec.hxx>

#include "../src/util.hpp"
#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/query.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

ShapePtr joined(const ShapePtr& base, const ShapePtr& tool) {
  return boolean(BooleanOp::Join, {base.get()}, *tool).pieces.at(0).shape;
}

// The area under the arc of a rounding of radius 2 that rolls on the top
// edge of a step 0.5 high (centre 2 above the floor, sqrt(1.75) from the
// step): the integral of 2 - sqrt(4 - u^2) for u in [0, sqrt(1.75)].
double step_fill() {
  const double u = std::sqrt(1.75);
  return 2.0 * u - (0.5 * u * std::sqrt(4.0 - u * u) + 2.0 * std::asin(u / 2.0));
}

void test_rounding_runs_over_a_bevel() {
  // A block bevelled 1 mm along its top front edge; the edge between the
  // top and the bevel rounded with 5 mm: the rolling ball cannot touch the
  // bevel (1.4 mm wide), it touches the top and the front, and the bevel
  // vanishes in the rounding.
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  ChamferSpec bevel;
  bevel.distance = 1;
  const ShapePtr bevelled = chamfer("F3", *block, {top_front}, bevel);
  const std::string bevel_face = "F3:chamfer(" + top_front + ")";
  const ShapePtr rounded = fillet("F4", *bevelled, {edge(end_cap("F2", 1), bevel_face)}, 5);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
  CHECK_NEAR(volume(*rounded), 60.0 * 40.0 * 20.0 - fillet_loss(5) * 60.0);
  CHECK(faces_named(*rounded, bevel_face) == 0);
  CHECK(rounded->face_count() == 7);
}

void test_rounding_wider_than_a_side() {
  // The top front edge of a block 20 mm high rounded with 30 mm: the ball
  // touches the top and rolls on the bottom front edge, the front vanishes.
  // Its centre lies 30 below the top and 30 from that edge, c = sqrt(800)
  // behind the front; the section kept under the arc from the edge (0, 0)
  // to the top (c, 20) is the integral of sqrt(900 - u^2) - 10 for u in
  // [-c, 0].
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  const ShapePtr rounded = fillet("F4", *block, {top_front}, 30);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
  const double c = std::sqrt(800.0);
  const double kept = -10.0 * c + 0.5 * c * 10.0 + 450.0 * std::asin(c / 30.0);
  CHECK_NEAR(volume(*rounded), 60.0 * 40.0 * 20.0 - (c * 20.0 - kept) * 60.0);
  CHECK(faces_named(*rounded, side("F2", 1, 0)) == 0);
  CHECK(rounded->face_count() == 6);
}

void test_rounding_rolls_on_the_far_edge_of_a_rib() {
  // A rib 1 mm thick, 10 mm high on a plate; one top edge rounded with
  // 2 mm: the ball touches the rib's side and rolls on the other top edge,
  // the rib's top face vanishes.
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 40, 20)}, 0, 5);
  const ShapePtr rib = extrude("F3", {rectangle(5, 0, 9.5, 40, 1)}, 5, 15);
  const ShapePtr body = joined(plate, rib);
  const ShapePtr rounded = fillet("F4", *body, {edge(end_cap("F3", 5), side("F3", 5, 0))}, 2);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
  // The section taken away: the corner triangle of the rib's top (1 by
  // sqrt 3) less the circular segment of 60 degrees of the rounding.
  const double h = std::sqrt(3.0);
  const double removed = 0.5 * h - 2.0 * (kPi / 3.0 - std::sin(kPi / 3.0));
  CHECK_NEAR(volume(*rounded), volume(*body) - removed * 40.0);
  CHECK(faces_named(*rounded, end_cap("F3", 5)) == 0);
  CHECK(rounded->face_count() == body->face_count());
}

void test_roundings_on_a_low_step() {
  // A step 0.5 mm high: the rounding at its foot (concave, 2 mm) fills up
  // to the step's top edge, the rounding of its top edge (convex, 2 mm)
  // runs down to its foot; the step's side vanishes both times.
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 40, 20)}, 0, 5);
  const ShapePtr step = extrude("F3", {rectangle(5, 0, 10, 40, 10)}, 5, 5.5);
  const ShapePtr body = joined(plate, step);
  const ShapePtr filled = fillet("F4", *body, {edge(end_cap("F2", 1), side("F3", 5, 0))}, 2);
  CHECK(detail::is_valid(filled->occt()));
  CHECK(filled->notes().empty());
  CHECK_NEAR(volume(*filled), volume(*body) + step_fill() * 40.0);
  CHECK(faces_named(*filled, side("F3", 5, 0)) == 0);
  const ShapePtr rounded = fillet("F4", *body, {edge(end_cap("F3", 5), side("F3", 5, 0))}, 2);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
  CHECK_NEAR(volume(*rounded), volume(*body) - step_fill() * 40.0);
  CHECK(faces_named(*rounded, side("F3", 5, 0)) == 0);
}

// OCCT's own fillet, without the names and the checks of the dressups.

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

int faces_of(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    ++n;
  }
  return n;
}

// The edge of a shape whose middle lies nearest to p.
TopoDS_Edge edge_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    BRepAdaptor_Curve curve(edge);
    const double d = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2).Distance(p);
    if (d < nearest) {
      nearest = d;
      best = edge;
    }
  }
  return best;
}

// OCCT's fillet of one edge; the result, valid, or a null shape.
TopoDS_Shape occt_fillet(const TopoDS_Shape& shape, const gp_Pnt& near_edge, double radius) {
  BRepFilletAPI_MakeFillet maker(shape);
  maker.Add(radius, edge_near(shape, near_edge));
  maker.Build();
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Shape()).IsValid()) {
    return TopoDS_Shape();
  }
  return maker.Shape();
}

void test_closed_roundings_roll_on_a_rim() {
  // A round step 0.5 high and 8.5 in radius on a plate: the rounding at its
  // foot (radius 0.6) fills up to its top rim all round, its side vanishes.
  // The ball's centre lies 0.6 above the plate, c = sqrt(0.35) outside the
  // step; the fill is the section under the arc revolved.
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, 0), 40, 40, 5).Shape();
  const TopoDS_Shape disc =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1)), 8.5, 0.5).Shape();
  const TopoDS_Shape stepped = BRepAlgoAPI_Fuse(plate, disc).Shape();
  const TopoDS_Shape filled = occt_fillet(stepped, gp_Pnt(-8.5, 0, 5), 0.6);
  CHECK(!filled.IsNull());
  if (!filled.IsNull()) {
    const double c = std::sqrt(0.35);
    double fill = 0.0;
    const int n = 20000;
    for (int i = 0; i < n; ++i) {
      const double u = c * (i + 0.5) / n;
      fill += 2.0 * kPi * (8.5 + c - u) * (0.6 - std::sqrt(0.36 - u * u)) * (c / n);
    }
    CHECK_NEAR_TOL(volume_of(filled), volume_of(stepped) + fill, 1e-7);
    CHECK(faces_of(filled) == faces_of(stepped));
  }
  // A boss of radius 5 on a ring 0.5 wide and high: the rounding at the
  // boss's foot (radius 2) cannot roll on the ring's outer top edge (that
  // ball would cut into the plate); it touches the boss and the plate, and
  // the ring vanishes in it. The fill is the corner region between the
  // boss, the plate and the arc (centre 7, 7 in the axis's plane) revolved,
  // less the ring that was there: 2 pi (24 - pi (7 - 8 / (3 pi))) - the
  // ring's volume.
  const TopoDS_Shape ring =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1)), 5.5, 0.5).Shape();
  const TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5.5), gp_Dir(0, 0, 1)), 5, 10).Shape();
  const TopoDS_Shape bossed = BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(plate, ring).Shape(), boss).Shape();
  const TopoDS_Shape rounded = occt_fillet(bossed, gp_Pnt(-5, 0, 5.5), 2);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    const double fill = 2.0 * kPi * (24.0 - kPi * (7.0 - 8.0 / (3.0 * kPi))) - kPi * (5.5 * 5.5 - 25.0) * 0.5;
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(bossed) + fill, 1e-7);
    // The ring's top and side gone, the plate's top and the rounding.
    CHECK(faces_of(rounded) == faces_of(bossed) - 1);
  }
}

// A prism 40 long in x of a closed polygon in the (y, z) plane.
TopoDS_Shape profile_prism(const std::vector<std::pair<double, double>>& yz) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& [y, z] : yz) {
    polygon.Add(gp_Pnt(0, y, z));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(40, 0, 0)).Shape();
}

// The area of a polygon in the (y, z) plane.
double polygon_area(const std::vector<std::pair<double, double>>& yz) {
  double twice = 0.0;
  for (std::size_t i = 0; i < yz.size(); ++i) {
    const auto& [y1, z1] = yz[i];
    const auto& [y2, z2] = yz[(i + 1) % yz.size()];
    twice += y1 * z2 - y2 * z1;
  }
  return 0.5 * std::abs(twice);
}

// OCCT's fillet of the edges nearest to the points, one radius; the
// result, valid, or a null shape.
TopoDS_Shape occt_fillet(const TopoDS_Shape& shape, const std::vector<gp_Pnt>& near_edges, double radius) {
  BRepFilletAPI_MakeFillet maker(shape);
  for (const gp_Pnt& p : near_edges) {
    maker.Add(radius, edge_near(shape, p));
  }
  maker.Build();
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Shape()).IsValid()) {
    return TopoDS_Shape();
  }
  return maker.Shape();
}

void test_roundings_over_several_narrow_faces() {
  // The top front corner of a block 20 x 10 (40 long) cut off by two strips
  // narrower than the rounding: the rounding (5) of the top edge of the
  // first runs over both onto the front; it is the block's corner rounded,
  // less what the strips cut off already. The second strip is 0.75 wide,
  // then 0.001 (a sliver on which no ball touches).
  for (const double z : {9.0, 9.2}) {
    const std::vector<std::pair<double, double>> corner = {
        {0, 10}, {1, 10}, z == 9.0 ? std::pair<double, double>{0.3, 9.6} : std::pair<double, double>{0.001, 9.2},
        {0, z}};
    const TopoDS_Shape block = profile_prism({{0, 0}, {20, 0}, {20, 10}, corner[1], corner[2], corner[3]});
    const TopoDS_Shape rounded = occt_fillet(block, {gp_Pnt(20, 1, 10)}, 5);
    CHECK(!rounded.IsNull());
    if (!rounded.IsNull()) {
      CHECK_NEAR_TOL(volume_of(rounded), 20.0 * 10.0 * 40.0 - fillet_loss(5) * 40.0, 1e-7);
      CHECK(faces_of(rounded) == faces_of(block) - 1);
    }
  }
  // The edge between the two strips rounded: the ball leaves both, touches
  // the top and the front.
  const std::vector<std::pair<double, double>> corner = {{0, 10}, {1, 10}, {0.4, 9.85}, {0, 9}};
  const TopoDS_Shape block = profile_prism({{0, 0}, {20, 0}, {20, 10}, corner[1], corner[2], corner[3]});
  const TopoDS_Shape rounded = occt_fillet(block, {gp_Pnt(20, 0.4, 9.85)}, 5);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(volume_of(rounded), 20.0 * 10.0 * 40.0 - fillet_loss(5) * 40.0, 1e-7);
    CHECK(faces_of(rounded) == faces_of(block) - 1);
  }
  // A wall 1 thick on a plate, its top a flat strip and a slanted one: the
  // rounding (2) of the left top edge rolls on the far edge of the slanted
  // strip (10.5, 14.9), both strips vanish. Its centre lies 2 right of the
  // wall's left side, sqrt 3 below that edge; the section taken away is the
  // corner region of the two strips less the circular segment of 60
  // degrees of the rounding.
  const double zc = 14.9 - std::sqrt(3.0);
  const TopoDS_Shape wall =
      profile_prism({{0, 0}, {20, 0}, {20, 5}, {10.5, 5}, {10.5, 14.9}, {10.2, 15}, {9.5, 15}, {9.5, 5}, {0, 5}});
  const TopoDS_Shape rolled = occt_fillet(wall, {gp_Pnt(20, 9.5, 15)}, 2);
  CHECK(!rolled.IsNull());
  if (!rolled.IsNull()) {
    const double removed =
        polygon_area({{9.5, zc}, {9.5, 15}, {10.2, 15}, {10.5, 14.9}}) - 2.0 * (kPi / 3.0 - std::sin(kPi / 3.0));
    CHECK_NEAR_TOL(volume_of(rolled), volume_of(wall) - removed * 40.0, 1e-7);
    CHECK(faces_of(rolled) == faces_of(wall) - 1);
  }
}

void test_roundings_that_overlap_on_a_face_are_one() {
  // A block 40 x 20 x 10 bevelled along its top front edge, 1 and 2 wide;
  // both edges of the bevel rounded with 5. With 1 each rounding runs over
  // the bevel (onto the front, onto the top); with 2 each fits on the bevel
  // alone (2.07 from its edge) but they overlap on it. Either way the
  // rounding of both is the block's edge rounded, tangent to the top and
  // the front, and the bevel vanishes.
  for (const double d : {1.0, 2.0}) {
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(40, 20, 10).Shape();
    BRepFilletAPI_MakeChamfer bevel(box);
    bevel.Add(d, edge_near(box, gp_Pnt(20, 0, 10)));
    bevel.Build();
    CHECK(bevel.IsDone());
    if (!bevel.IsDone()) {
      continue;
    }
    const TopoDS_Shape bevelled = bevel.Shape();
    const TopoDS_Shape rounded = occt_fillet(bevelled, {gp_Pnt(20, d, 10), gp_Pnt(20, 0, 10 - d)}, 5);
    CHECK(!rounded.IsNull());
    if (!rounded.IsNull()) {
      CHECK_NEAR_TOL(volume_of(rounded), 40.0 * 20.0 * 10.0 - fillet_loss(5) * 40.0, 1e-7);
      CHECK(faces_of(rounded) == 7);
    }
  }
  // The same through the dressup, with the names: the faces of both edges
  // are the one rounding's.
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  ChamferSpec spec;
  spec.distance = 2;
  const ShapePtr bevelled = chamfer("F3", *block, {top_front}, spec);
  const std::string bevel_face = "F3:chamfer(" + top_front + ")";
  const ShapePtr rounded =
      fillet("F4", *bevelled, {edge(end_cap("F2", 1), bevel_face), edge(bevel_face, side("F2", 1, 0))}, 5);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK_NEAR(volume(*rounded), 60.0 * 40.0 * 20.0 - fillet_loss(5) * 60.0);
  CHECK(faces_named(*rounded, bevel_face) == 0);
  CHECK(rounded->face_count() == 7);
}

// OCCT's chamfer of one edge with one distance; the result, valid, or a
// null shape.
TopoDS_Shape occt_chamfer(const TopoDS_Shape& shape, const gp_Pnt& near_edge, double distance) {
  BRepFilletAPI_MakeChamfer maker(shape);
  maker.Add(distance, edge_near(shape, near_edge));
  maker.Build();
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Shape()).IsValid()) {
    return TopoDS_Shape();
  }
  return maker.Shape();
}

void test_contacts_along_the_far_edge_of_a_face() {
  // A step 3 high along a block: its foot rounded or chamfered with 3, and
  // its top edge rounded with 3. The contact on the step's side lies on its
  // other edge all along (the side is exactly as wide as the contact
  // distance); the side vanishes, the blend meets the step's top (or the
  // floor) at that edge.
  const std::vector<std::pair<double, double>> profile = {{0, 0}, {20, 0}, {20, 10}, {8, 10}, {8, 13}, {0, 13}};
  const TopoDS_Shape step = profile_prism(profile);
  const double corner = 9.0 * (1.0 - kPi / 4.0);
  const TopoDS_Shape filled = occt_fillet(step, gp_Pnt(20, 8, 10), 3);
  CHECK(!filled.IsNull());
  if (!filled.IsNull()) {
    CHECK_NEAR_TOL(volume_of(filled), volume_of(step) + corner * 40.0, 1e-7);
    CHECK(faces_of(filled) == faces_of(step));
  }
  const TopoDS_Shape bevelled = occt_chamfer(step, gp_Pnt(20, 8, 10), 3);
  CHECK(!bevelled.IsNull());
  if (!bevelled.IsNull()) {
    CHECK_NEAR_TOL(volume_of(bevelled), volume_of(step) + 4.5 * 40.0, 1e-7);
    CHECK(faces_of(bevelled) == faces_of(step));
  }
  const TopoDS_Shape rounded = occt_fillet(step, gp_Pnt(20, 8, 13), 3);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(step) - corner * 40.0, 1e-7);
    CHECK(faces_of(rounded) == faces_of(step));
  }
  // A boss 10 x 10, 3 high on a block: the edges at its front, rounded or
  // chamfered with 3, end at its corners, where the boss's sides cut them.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 20, 20, 10).Shape();
  const TopoDS_Shape boss = BRepPrimAPI_MakeBox(gp_Pnt(5, 5, 10), 10, 10, 3).Shape();
  const TopoDS_Shape bossed = BRepAlgoAPI_Fuse(block, boss).Shape();
  const TopoDS_Shape boss_filled = occt_fillet(bossed, gp_Pnt(10, 5, 10), 3);
  CHECK(!boss_filled.IsNull());
  if (!boss_filled.IsNull()) {
    CHECK_NEAR_TOL(volume_of(boss_filled), volume_of(bossed) + corner * 10.0, 1e-7);
    CHECK(faces_of(boss_filled) == faces_of(bossed));
  }
  const TopoDS_Shape boss_bevelled = occt_chamfer(bossed, gp_Pnt(10, 5, 10), 3);
  CHECK(!boss_bevelled.IsNull());
  if (!boss_bevelled.IsNull()) {
    CHECK_NEAR_TOL(volume_of(boss_bevelled), volume_of(bossed) + 4.5 * 10.0, 1e-7);
    CHECK(faces_of(boss_bevelled) == faces_of(bossed));
  }
  const TopoDS_Shape boss_rounded = occt_fillet(bossed, gp_Pnt(10, 5, 13), 3);
  CHECK(!boss_rounded.IsNull());
  if (!boss_rounded.IsNull()) {
    CHECK_NEAR_TOL(volume_of(boss_rounded), volume_of(bossed) - corner * 10.0, 1e-7);
    CHECK(faces_of(boss_rounded) == faces_of(bossed));
  }
}

void test_second_rounding_rolls_on_the_first_ones_contact_line() {
  // A disc 2.5 thick, both rims rounded with 1.5 (the bottom first): no ball
  // touches both flat faces, so the two roundings cannot be one. The first
  // is a quarter torus tangent to the side 1.5 above the bottom; the second
  // rolls on that contact line, its ball tangent to the top and centred
  // sqrt(2) inside the rim, and the side vanishes. The volume taken away:
  // the first rim's corner outside the quarter circle and the second's
  // outside the arc, revolved.
  const double R = 20, t = 2.5, r = 1.5;
  const TopoDS_Shape disc =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), R, t).Shape();
  BRepFilletAPI_MakeFillet maker(disc);
  maker.Add(r, edge_near(disc, gp_Pnt(-R, 0, 0)));
  maker.Add(r, edge_near(disc, gp_Pnt(-R, 0, t)));
  maker.Build();
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape rounded = maker.Shape();
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  const double d = std::sqrt(r * r - (t - 2 * r) * (t - 2 * r));
  double removed = 0.0;
  const int n = 20000;
  for (int i = 0; i < n; ++i) {
    const double z1 = r * (i + 0.5) / n;
    const double rho1 = (R - r) + std::sqrt(r * r - (r - z1) * (r - z1));
    const double z2 = r + (t - r) * (i + 0.5) / n;
    const double rho2 = (R - d) + std::sqrt(r * r - (z2 - (t - r)) * (z2 - (t - r)));
    removed += kPi * (R * R - rho1 * rho1) * (r / n) + kPi * (R * R - rho2 * rho2) * ((t - r) / n);
  }
  // (The second blend is approximated within the builder's tolerance.)
  CHECK_NEAR_TOL(volume_of(rounded), volume_of(disc) - removed, 1e-6);
  CHECK(faces_of(rounded) == 4);
  // The same along the straight front edges of a plate (40 long), ending at
  // its sides: the two roundings meet on the line there, the front and the
  // pieces of the side edges between the two edges vanish.
  const double L = 40;
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), L, 20, t).Shape();
  BRepFilletAPI_MakeFillet plate_maker(plate);
  plate_maker.Add(r, edge_near(plate, gp_Pnt(L / 2, 0, 0)));
  plate_maker.Add(r, edge_near(plate, gp_Pnt(L / 2, 0, t)));
  plate_maker.Build();
  CHECK(plate_maker.IsDone());
  if (!plate_maker.IsDone()) {
    return;
  }
  const TopoDS_Shape plate_rounded = plate_maker.Shape();
  CHECK(BRepCheck_Analyzer(plate_rounded).IsValid());
  double section = 0.0;
  for (int i = 0; i < n; ++i) {
    const double z1 = r * (i + 0.5) / n;
    const double z2 = r + (t - r) * (i + 0.5) / n;
    section += (r - std::sqrt(r * r - (r - z1) * (r - z1))) * (r / n) +
               (d - std::sqrt(r * r - (z2 - (t - r)) * (z2 - (t - r)))) * ((t - r) / n);
  }
  CHECK_NEAR_TOL(volume_of(plate_rounded), volume_of(plate) - section * L, 1e-7);
  CHECK(faces_of(plate_rounded) == 7);
}

} // namespace

void fillet_overflow_tests() {
  guarded("test_rounding_runs_over_a_bevel", test_rounding_runs_over_a_bevel);
  guarded("test_rounding_wider_than_a_side", test_rounding_wider_than_a_side);
  guarded("test_rounding_rolls_on_the_far_edge_of_a_rib", test_rounding_rolls_on_the_far_edge_of_a_rib);
  guarded("test_roundings_on_a_low_step", test_roundings_on_a_low_step);
  guarded("test_closed_roundings_roll_on_a_rim", test_closed_roundings_roll_on_a_rim);
  guarded("test_roundings_over_several_narrow_faces", test_roundings_over_several_narrow_faces);
  guarded("test_roundings_that_overlap_on_a_face_are_one", test_roundings_that_overlap_on_a_face_are_one);
  guarded("test_contacts_along_the_far_edge_of_a_face", test_contacts_along_the_far_edge_of_a_face);
  guarded("test_second_rounding_rolls_on_the_first_ones_contact_line",
          test_second_rounding_rolls_on_the_first_ones_contact_line);
}

} // namespace test
