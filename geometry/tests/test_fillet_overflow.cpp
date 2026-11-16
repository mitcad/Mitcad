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
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <ShapeCustom.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

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

void test_rounding_rolls_on_a_bevels_far_edge() {
  // A block bevelled 1 mm along its top front edge; the edge between the
  // top and the bevel rounded with 5 mm: the rolling ball cannot touch the
  // bevel (1.4 mm wide). The bevel's far edge is sharp and convex, so the
  // rounding stays tangent to the top and rolls on that edge (a ball
  // touching the front instead would round the block's whole corner); the
  // bevel vanishes in the rounding. The ball's centre lies 5 below the top
  // and 3 behind the front (through the edge 1 below the top); the section
  // taken away is the triangle between the top's contact, the bevel's two
  // edges (area 1) less the circular segment over the chord from the edge
  // to the contact (sqrt 10 long, angle 2 asin(1 / sqrt 10), sine 0.6).
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  ChamferSpec bevel;
  bevel.distance = 1;
  const ShapePtr bevelled = chamfer("F3", *block, {top_front}, bevel);
  const std::string bevel_face = "F3:chamfer(" + top_front + ")";
  const ShapePtr rounded = fillet("F4", *bevelled, {edge(end_cap("F2", 1), bevel_face)}, 5);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
  const double removed = 1.0 - 12.5 * (2.0 * std::asin(1.0 / std::sqrt(10.0)) - 0.6);
  CHECK_NEAR(volume(*rounded), 60.0 * 40.0 * 20.0 - 0.5 * 60.0 - removed * 60.0);
  // (The rounding takes the name of the bevel it consumed.)
  CHECK(faces_named(*rounded, bevel_face) == 1);
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
  // The front lives on in the rounding, which meets the bottom along the
  // front's bottom edge: the rounding takes the front's name.
  CHECK(faces_named(*rounded, side("F2", 1, 0)) == 1);
  CHECK(faces_named(*rounded, "F4:fillet(" + top_front + ")") == 1);
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
  // (The rounding takes the name of the top it consumed.)
  CHECK(faces_named(*rounded, end_cap("F3", 5)) == 1);
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
  // (Each rounding takes the name of the side it consumed.)
  CHECK(faces_named(*filled, side("F3", 5, 0)) == 1);
  const ShapePtr rounded = fillet("F4", *body, {edge(end_cap("F3", 5), side("F3", 5, 0))}, 2);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
  CHECK_NEAR(volume(*rounded), volume(*body) - step_fill() * 40.0);
  CHECK(faces_named(*rounded, side("F3", 5, 0)) == 1);
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

void test_consumed_side_keeps_its_edges_names() {
  // A step 3 high: its foot rounded with 3, the side vanishes and the
  // rounding meets the step's top along the side's top edge. The rounding
  // takes the side's name, so that edge keeps its name and is rounded next
  // with 1 (as the import rounds one tangent chain at a time). The second
  // ball, of radius 1, touches the top and the first rounding (its centre
  // 1 below the top, 3 + 1 from the first one's axis, 3 before the side).
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 40, 20)}, 0, 5);
  const ShapePtr step = extrude("F3", {rectangle(5, 0, 10, 40, 10)}, 5, 8);
  const ShapePtr body = joined(plate, step);
  const ShapePtr filled = fillet("F4", *body, {edge(end_cap("F2", 1), side("F3", 5, 0))}, 3);
  CHECK(detail::is_valid(filled->occt()));
  CHECK_NEAR(volume(*filled), volume(*body) + fillet_loss(3) * 40.0);
  CHECK(faces_named(*filled, side("F3", 5, 0)) == 1);
  const std::string top_edge = edge(side("F3", 5, 0), end_cap("F3", 5));
  CHECK(filled->find_edges(top_edge).size() == 1);
  const ShapePtr rounded = fillet("F5", *filled, {top_edge}, 1);
  CHECK(detail::is_valid(rounded->occt()));
  // The section between the first rounding (centre (7, 8)), the top
  // (z = 8) and the second ball (centre (7 + sqrt 15, 7)), from z = 7.25
  // (where the balls touch) to the top.
  const double c = 7.0 + std::sqrt(15.0);
  double area = 0.0;
  const int n = 20000;
  for (int i = 0; i < n; ++i) {
    const double z = 7.25 + 0.75 * (i + 0.5) / n;
    area += ((c - std::sqrt(1.0 - (z - 7.0) * (z - 7.0))) - (7.0 + std::sqrt(9.0 - (z - 8.0) * (z - 8.0)))) * (0.75 / n);
  }
  CHECK_NEAR_TOL(volume(*rounded), volume(*filled) - area * 40.0, 1e-6);
}

void test_two_distance_chamfer_on_a_face_as_high_as_it_ends_on_slanted_faces() {
  // A trapezoid boss 3 high (front 20 long at y = 5, back 12 long at
  // y = 15) on a block; its top front edge chamfered with 2 on the top and
  // 3 on the front: the front vanishes, the chamfer meets the block's top
  // along the front's foot and ends on the slanted sides, where its
  // contact on the top ends before the foot does. The wedge removed:
  // width 20 - 0.8 t times height 3 - 1.5 t over t in [0, 2] from the front.
  const std::vector<std::pair<double, double>> footprint = {{10, 5}, {30, 5}, {26, 15}, {14, 15}};
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& [x, y] : footprint) {
    polygon.Add(gp_Pnt(x, y, 10));
  }
  polygon.Close();
  const TopoDS_Shape boss =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(polygon.Wire()).Face(), gp_Vec(0, 0, 3)).Shape();
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 40, 30, 10).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(block, boss).Shape(), true, true, true);
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  TopoDS_Face top;
  for (TopExp_Explorer e(body, TopAbs_FACE); e.More(); e.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(e.Current(), props);
    if (std::abs(props.CentreOfMass().Z() - 13.0) < 1e-9) {
      top = TopoDS::Face(e.Current());
    }
  }
  CHECK(!top.IsNull());
  BRepFilletAPI_MakeChamfer maker(body);
  maker.Add(2.0, 3.0, edge_near(body, gp_Pnt(20, 5, 13)), top);
  maker.Build();
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(maker.Shape()).IsValid());
  CHECK_NEAR_TOL(volume_of(maker.Shape()), volume_of(body) - (120.0 - 64.8 + 3.2), 1e-7);
  CHECK(faces_of(maker.Shape()) == faces_of(body));
}

void test_corner_where_the_end_face_is_split_in_one_plane() {
  // A tab 10 long, 3 high standing behind a block's top on a support of
  // its own below (fused, not unified: the tab's sides and the support's
  // are faces of one plane). The concave edge at the tab's front foot,
  // chamfered with 2 or 3 or rounded with 2, ends at the tab's sides,
  // whose planes cut it: a wedge of d^2 / 2 per unit length, or the
  // fillet's corner. Then the same with the block's top back edge rounded
  // with 0.5 first, the tab on the rounding's start: the top runs on
  // tangentially into the rounding beside the tab.
  for (const bool rounded_edge : {false, true}) {
    const double y = rounded_edge ? 19.5 : 20.0;
    TopoDS_Shape base = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 40, 20, 10).Shape();
    if (rounded_edge) {
      base = occt_fillet(base, gp_Pnt(20, 20, 10), 0.5);
    }
    const TopoDS_Shape support = BRepPrimAPI_MakeBox(gp_Pnt(15, y, 0), 10, 23 - y, 10).Shape();
    const TopoDS_Shape tab = BRepPrimAPI_MakeBox(gp_Pnt(15, y, 10), 10, 23 - y, 3).Shape();
    const TopoDS_Shape body = BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(base, support).Shape(), tab).Shape();
    const TopoDS_Edge foot = edge_near(body, gp_Pnt(20, y, 10));
    for (const double d : {2.0, 3.0}) {
      BRepFilletAPI_MakeChamfer chamfer(body);
      chamfer.Add(d, foot);
      chamfer.Build();
      CHECK(chamfer.IsDone());
      if (chamfer.IsDone()) {
        CHECK(BRepCheck_Analyzer(chamfer.Shape()).IsValid());
        CHECK_NEAR_TOL(volume_of(chamfer.Shape()), volume_of(body) + 0.5 * d * d * 10.0, 1e-7);
      }
    }
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(2.0, foot);
    fillet.Build();
    CHECK(fillet.IsDone());
    if (fillet.IsDone()) {
      CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
      CHECK_NEAR_TOL(volume_of(fillet.Shape()), volume_of(body) + fillet_loss(2.0) * 10.0, 1e-7);
    }
  }
}

void test_rounding_runs_into_the_faces_at_its_ends() {
  // A slot with a round end (radius 7) through a plate 1.5 thick, below a
  // pocket as wide as the slot whose floor is the plate's top; the slot's
  // convex top edge along the half circle rounded with 0.5 (a quarter
  // torus). Near the ends of the half circle the rounding's contact on the
  // floor runs into the pocket's side walls, while the slot's wall runs on
  // tangentially into them: the walls cut the torus. The section taken
  // away (the corner outside the quarter circle) within the pocket's walls,
  // around the half circle.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-5, 0, 0), 30, 10, 30).Shape();
  const TopoDS_Shape pocket = BRepPrimAPI_MakeBox(gp_Pnt(3, 1.5, 3), 14, 10, 30).Shape();
  const TopoDS_Shape slot_top = BRepPrimAPI_MakeBox(gp_Pnt(3, -1, 12), 14, 3, 20).Shape();
  const TopoDS_Shape slot_end =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, -1, 12), gp_Dir(0, 1, 0)), 7, 3).Shape();
  const TopoDS_Shape cut = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(block, pocket).Shape(),
                                           BRepAlgoAPI_Fuse(slot_top, slot_end).Shape())
                               .Shape();
  ShapeUpgrade_UnifySameDomain unify(cut, true, true, true);
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  const TopoDS_Shape rounded = occt_fillet(body, gp_Pnt(10, 1.5, 5), 0.5);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  // Per angle phi around the slot's axis, the corner between radius 7
  // and 7.5 (radius rho) and depth 0.5 - sqrt(0.25 - (rho - 7.5)^2), up to
  // where the walls (x = 3, 17) cut it: Gauss-Legendre over rho.
  const double gx[5] = {-0.9061798459386640, -0.5384693101056831, 0.0, 0.5384693101056831,
                        0.9061798459386640};
  const double gw[5] = {0.2369268850561891, 0.4786286704993665, 0.5688888888888889,
                        0.4786286704993665, 0.2369268850561891};
  double removed = 0.0;
  const int n = 20000;
  for (int i = 0; i < n; ++i) {
    const double c = std::abs(std::cos(kPi + kPi * (i + 0.5) / n));
    const double hi = c > 1e-12 ? std::min(7.5, 7.0 / c) : 7.5;
    for (int k = 0; k < 50 && hi > 7.0; ++k) {
      const double a = 7.0 + (hi - 7.0) * k / 50.0, b = 7.0 + (hi - 7.0) * (k + 1) / 50.0;
      for (int g = 0; g < 5; ++g) {
        const double rho = 0.5 * (a + b) + 0.5 * (b - a) * gx[g];
        const double depth = 0.5 - std::sqrt(0.25 - (rho - 7.5) * (rho - 7.5));
        removed += gw[g] * 0.5 * (b - a) * depth * rho * (kPi / n);
      }
    }
  }
  CHECK_NEAR_TOL(volume_of(rounded), volume_of(body) - removed, 1e-8);
  // One torus face: the floor's contact runs on up to the walls.
  CHECK(faces_of(rounded) == faces_of(body) + 1);
}

void test_rounding_rolls_on_a_bevel_edge_round_the_corners() {
  // A plate 120 x 60 x 2 with corners rounded 2, its bottom edges bevelled
  // 0.5; the edges between its sides and the bevel rounded with 2. The ball
  // touches the side (centre 2 inside) and rolls on the bevel's bottom edge
  // (centre sqrt(1.75) above the bottom); at the corners its centre stays
  // on the corner's axis and the side's contact passes onto the corner
  // while the ball rolls on along the bevel's edge. The section removed
  // along the straight sides, and revolved about the corners' axes.
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -7), 120, 60, 2).Shape();
  BRepFilletAPI_MakeFillet corners(plate);
  for (const gp_Pnt& p : {gp_Pnt(0, 0, -6), gp_Pnt(120, 0, -6), gp_Pnt(120, 60, -6), gp_Pnt(0, 60, -6)}) {
    corners.Add(2.0, edge_near(plate, p));
  }
  corners.Build();
  CHECK(corners.IsDone());
  if (!corners.IsDone()) {
    return;
  }
  plate = corners.Shape();
  BRepFilletAPI_MakeChamfer bevel(plate);
  for (TopExp_Explorer e(plate, TopAbs_EDGE); e.More(); e.Next()) {
    BRepAdaptor_Curve c(TopoDS::Edge(e.Current()));
    if (std::abs(c.Value(c.FirstParameter()).Z() + 7) < 1e-9 && std::abs(c.Value(c.LastParameter()).Z() + 7) < 1e-9) {
      bevel.Add(0.5, TopoDS::Edge(e.Current()));
    }
  }
  bevel.Build();
  CHECK(bevel.IsDone());
  if (!bevel.IsDone()) {
    return;
  }
  plate = bevel.Shape();
  std::vector<gp_Pnt> rims;
  for (TopExp_Explorer e(plate, TopAbs_EDGE); e.More(); e.Next()) {
    BRepAdaptor_Curve c(TopoDS::Edge(e.Current()));
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    bool seen = false;
    for (const gp_Pnt& q : rims) {
      seen = seen || q.Distance(m) < 1e-9;
    }
    if (!seen && std::abs(c.Value(c.FirstParameter()).Z() + 6.5) < 1e-9
        && std::abs(c.Value(c.LastParameter()).Z() + 6.5) < 1e-9) {
      rims.push_back(m);
    }
  }
  CHECK(rims.size() == 8);
  const TopoDS_Shape rounded = occt_fillet(plate, rims, 2.0);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  // For z from the bottom to the centre's height: from the side (x = 0)
  // or the bevel (x = -6.5 - z) to the arc; the moment of (2 - x) gives
  // the corners (a quarter turn each about an axis 2 inside).
  const double zc = -7.0 + std::sqrt(1.75);
  double area = 0.0, moment = 0.0;
  const int n = 200000;
  for (int i = 0; i < n; ++i) {
    const double z = -7.0 + (zc + 7.0) * (i + 0.5) / n;
    const double lo = z < -6.5 ? -6.5 - z : 0.0;
    const double hi = 2.0 - std::sqrt(4.0 - (z - zc) * (z - zc));
    if (hi > lo) {
      area += (hi - lo) * (zc + 7.0) / n;
      moment += ((2.0 * hi - 0.5 * hi * hi) - (2.0 * lo - 0.5 * lo * lo)) * (zc + 7.0) / n;
    }
  }
  const double removed = area * (2 * (120 - 4) + 2 * (60 - 4)) + 2.0 * kPi * moment;
  // (The blend rolling on an edge is approximated within the builder's
  // tolerance, 6e-4 of the volume removed.)
  CHECK_NEAR_TOL(volume_of(rounded), volume_of(plate) - removed, 2e-6);
}

// A plate 2 thick whose outline has a notch: a slanted side (2.3 long)
// narrower than the rounding meets a straight side that runs on
// tangentially into a corner of radius 1. The concave edge between them
// rounded with 5: the ball touches the corner and rolls on the slanted
// side's far edge (P1); the slanted side vanishes. The fill is the region
// between the straight side, the corner, the ball (centre C: 6 from the
// corner's centre Q, 5 from P1) and the slanted side, 2 high. With a tab
// under the straight side and the corner (down to z = -4) their faces run
// on below the plate, and the rounding ends at the plate's bottom inside
// them.
void check_rounding_beside_a_round_corner(bool tab) {
  const gp_Pnt2d P0(13.2408, -1.6283), P1(7.35145, -5.16699), A(5.5, -2.68314), B(5.5, -4.5), D(4.5, -5.5),
      E(-4.5, -5.5), F(-4.5, 10), G(13.2408, 10), Q(4.5, -4.5);
  BRepBuilderAPI_MakeWire wire;
  const auto segment = [&](const gp_Pnt2d& a, const gp_Pnt2d& b) {
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(a.X(), a.Y(), 0), gp_Pnt(b.X(), b.Y(), 0)).Edge());
  };
  segment(P0, P1);
  segment(P1, A);
  segment(A, B);
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(Q.X(), Q.Y(), 0), gp_Dir(0, 0, -1)), 1.0),
                                   gp_Pnt(B.X(), B.Y(), 0), gp_Pnt(D.X(), D.Y(), 0))
               .Edge());
  segment(D, E);
  segment(E, F);
  segment(F, G);
  segment(G, P0);
  TopoDS_Shape plate =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(0, 0, 2)).Shape();
  if (tab) {
    BRepBuilderAPI_MakeWire tab_wire;
    const auto low = [](const gp_Pnt2d& p) { return gp_Pnt(p.X(), p.Y(), -4); };
    tab_wire.Add(BRepBuilderAPI_MakeEdge(low(gp_Pnt2d(5.5, 5)), low(B)).Edge());
    tab_wire.Add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(low(Q), gp_Dir(0, 0, -1)), 1.0), low(B), low(D)).Edge());
    tab_wire.Add(BRepBuilderAPI_MakeEdge(low(D), low(E)).Edge());
    tab_wire.Add(BRepBuilderAPI_MakeEdge(low(E), low(gp_Pnt2d(-4.5, 5))).Edge());
    tab_wire.Add(BRepBuilderAPI_MakeEdge(low(gp_Pnt2d(-4.5, 5)), low(gp_Pnt2d(5.5, 5))).Edge());
    const TopoDS_Shape tab_solid =
        BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(tab_wire.Wire(), true).Face(), gp_Vec(0, 0, 4)).Shape();
    ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, tab_solid).Shape(), true, true, true);
    unify.Build();
    plate = unify.Shape();
  }
  const TopoDS_Shape rounded = occt_fillet(plate, gp_Pnt(5.5, -2.68314, 1.0), 5.0);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  // The ball's centre, below.
  const double d = Q.Distance(P1);
  const double a = (36.0 - 25.0 + d * d) / (2.0 * d);
  const double h = std::sqrt(36.0 - a * a);
  const gp_Vec2d n = gp_Vec2d(Q, P1).Normalized();
  const gp_Pnt2d M(Q.X() + a * n.X(), Q.Y() + a * n.Y());
  gp_Pnt2d C(M.X() + h * n.Y(), M.Y() - h * n.X());
  if (M.Y() + h * n.X() < C.Y()) {
    C = gp_Pnt2d(M.X() - h * n.Y(), M.Y() + h * n.X());
  }
  const gp_Pnt2d T(Q.X() + (C.X() - Q.X()) / 6.0, Q.Y() + (C.Y() - Q.Y()) / 6.0);
  // The region A, B, (the corner) T, (the ball) P1 by Green's theorem; an
  // arc of radius r from angle t0 to t1 contributes exactly.
  const auto chord = [](const gp_Pnt2d& p, const gp_Pnt2d& q) { return p.X() * q.Y() - q.X() * p.Y(); };
  const auto arc = [](const gp_Pnt2d& c, double r, double t0, double t1) {
    return r * r * (t1 - t0) + c.X() * r * (std::sin(t1) - std::sin(t0)) - c.Y() * r * (std::cos(t1) - std::cos(t0));
  };
  double t0 = std::atan2(T.Y() - C.Y(), T.X() - C.X()), t1 = std::atan2(P1.Y() - C.Y(), P1.X() - C.X());
  if (t1 - t0 > kPi) {
    t1 -= 2 * kPi;
  } else if (t0 - t1 > kPi) {
    t1 += 2 * kPi;
  }
  const double twice = chord(A, B) + arc(Q, 1.0, 0.0, std::atan2(T.Y() - Q.Y(), T.X() - Q.X())) +
                       arc(C, 5.0, t0, t1) + chord(P1, A);
  CHECK_NEAR_TOL(volume_of(rounded), volume_of(plate) + std::abs(twice), 1e-8);
  // The slanted side gone, the rounding added; without the tab the straight
  // side's piece in the plate is gone too.
  CHECK(faces_of(rounded) == faces_of(plate) - (tab ? 0 : 1));
}

void test_rounding_rolls_beside_a_round_corner() {
  check_rounding_beside_a_round_corner(false);
}

void test_rounding_beside_a_round_corner_ends_inside_its_faces() {
  check_rounding_beside_a_round_corner(true);
}

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, true);
  unify.Build();
  return unify.Shape();
}

void test_contact_runs_along_a_part_of_the_walls_top_edge() {
  // A channel 10 wide in a block; its side wall is 3 high, as high as the
  // rounding at its foot, then flares out (a concave quarter cylinder of
  // radius 7). The floor rounds down (3) to the block's ends. The foot
  // rounded with 3: along the straight part its contact runs along the
  // wall's top edge, round the floor's end roundings it lies inside the
  // wall, which keeps two pieces at the ends.
  TopoDS_Shape body = BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(0, 20, 0), gp_Pnt(20, 43, 30)).Shape(),
                                      BRepPrimAPI_MakeBox(gp_Pnt(5, 33, -1), gp_Pnt(15, 50, 31)).Shape())
                          .Shape();
  for (int side = 0; side < 2; ++side) {
    const double x0 = side == 0 ? 15.0 : -2.0;
    const TopoDS_Shape corner = BRepPrimAPI_MakeBox(gp_Pnt(x0, 36, -1), gp_Pnt(x0 + 7, 43.5, 31)).Shape();
    const TopoDS_Shape disc =
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(side == 0 ? 22.0 : -2.0, 36, -1), gp_Dir(0, 0, 1)), 7, 32).Shape();
    body = BRepAlgoAPI_Cut(body, BRepAlgoAPI_Cut(corner, disc).Shape()).Shape();
  }
  body = unified(body);
  BRepFilletAPI_MakeFillet ends(body);
  ends.Add(3, edge_near(body, gp_Pnt(10, 33, 0)));
  ends.Add(3, edge_near(body, gp_Pnt(10, 33, 30)));
  ends.Build();
  CHECK(ends.IsDone());
  if (!ends.IsDone()) {
    return;
  }
  const TopoDS_Shape channel = ends.Shape();
  const TopoDS_Shape rounded = occt_fillet(channel, gp_Pnt(15, 33, 15), 3.0);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  // The section filled: the corner outside the quarter circle, 9 - 9 pi / 4;
  // along the floor (24 long) a prism. Round each end rounding (axis 3
  // inside the floor and the end) it turns about the axis; a point d from
  // the axis turns by asin(3 / d) until it reaches the block's end.
  const double section = 9.0 - 9.0 * kPi / 4.0;
  double turned = 0.0;
  const int n = 200000;
  for (int i = 0; i < n; ++i) {
    const double d = 3.0 + 3.0 * (i + 0.5) / n;
    turned += (3.0 - std::sqrt(9.0 - (d - 6.0) * (d - 6.0))) * d * std::asin(3.0 / d) * 3.0 / n;
  }
  // (The end cuts of the tori are approximated within the builder's
  // tolerance.)
  CHECK_NEAR_TOL(volume_of(rounded), volume_of(channel) + section * 24.0 + 2.0 * turned, 1e-8);
  const TopoDS_Shape middle = BRepPrimAPI_MakeBox(gp_Pnt(10, 30, 3), gp_Pnt(20, 40, 27)).Shape();
  CHECK_NEAR_TOL(volume_of(BRepAlgoAPI_Common(rounded, middle).Shape()),
                 volume_of(BRepAlgoAPI_Common(channel, middle).Shape()) + section * 24.0, 1e-7);
  // The rounding (a cylinder and two tori) added, the wall's middle gone.
  CHECK(faces_of(rounded) == faces_of(channel) + 4);
}

void test_rounding_over_a_hole_is_cut_by_it() {
  // A wall 2 thick on a plate, a hole of radius 1.5 through the plate in
  // front of it, 5 from the wall. The wall's foot rounded with 4: the
  // contact on the plate (4 from the wall) crosses the hole. The rounding
  // runs on over it and the hole's wall, carried up, cuts it.
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(40, 20, 4)).Shape();
  const TopoDS_Shape wall = BRepPrimAPI_MakeBox(gp_Pnt(0, 12, 4), gp_Pnt(40, 14, 12)).Shape();
  // (The hole's seam away from the wall.)
  const TopoDS_Shape hole =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(20, 7, -1), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)), 1.5, 6).Shape();
  const TopoDS_Shape body = unified(BRepAlgoAPI_Cut(BRepAlgoAPI_Fuse(plate, wall).Shape(), hole).Shape());
  const TopoDS_Shape rounded = occt_fillet(body, gp_Pnt(10, 12, 4), 4.0);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  const TopoDS_Shape corner = BRepPrimAPI_MakeBox(gp_Pnt(0, 8, 4), gp_Pnt(40, 12, 8)).Shape();
  const TopoDS_Shape circle = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1, 8, 8), gp_Dir(1, 0, 0)), 4, 42).Shape();
  const TopoDS_Shape fill = BRepAlgoAPI_Cut(corner, circle).Shape();
  const TopoDS_Shape up = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(20, 7, -1), gp_Dir(0, 0, 1)), 1.5, 20).Shape();
  CHECK_NEAR_TOL(volume_of(rounded),
                 volume_of(body) + volume_of(fill) - volume_of(BRepAlgoAPI_Common(fill, up).Shape()), 1e-9);
  // The rounding one face, the hole cut out of it (its pieces beside
  // the hole and the one behind it are one surface).
  CHECK(faces_of(rounded) == faces_of(body) + 1);
}

void test_rim_rounding_past_a_side_is_cut_by_it() {
  // A hole of radius 5 through a block, 0.5 from its side; the hole's top
  // rim rounded with 1. Near the side the contact on the top runs past the
  // side's top edge: the rounding runs on and the side cuts it. What goes
  // is the block's part of the section revolved about the hole's axis.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(60, 40, 20)).Shape();
  const TopoDS_Shape drill = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5.5, 20, -1), gp_Dir(0, 0, 1)), 5, 22).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(block, drill).Shape();
  const TopoDS_Shape rounded = occt_fillet(body, gp_Pnt(0.5, 20, 20), 1.0);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  const gp_Ax2 axis(gp_Pnt(5.5, 20, 19), gp_Dir(0, 0, 1));
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(
      BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(axis, 6, 1).Shape(),
                      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5.5, 20, 18), gp_Dir(0, 0, 1)), 5, 3).Shape())
          .Shape(),
      BRepPrimAPI_MakeTorus(axis, 6, 1).Shape())
                                .Shape();
  // (Integrated finely: the default leaves 2e-4 on the cut torus.)
  const auto fine_volume = [](const TopoDS_Shape& shape) {
    GProp_GProps props;
    BRepGProp::VolumeProperties(shape, props, 1e-10);
    return props.Mass();
  };
  CHECK_NEAR_TOL(fine_volume(rounded), fine_volume(body) - fine_volume(BRepAlgoAPI_Common(body, ring).Shape()),
                 1e-9);
  // The rounding one face round the rim, the side's cut in it.
  CHECK(faces_of(rounded) == faces_of(body) + 1);
}

// A pipe (radius 25, axis z) with a rod (radius 5) standing beside it, 28
// from its axis; one end of both cut by a plane inclined across the
// junction, rising by 0.5 per unit of y (from z0 at y = 5), the other end
// flat. The pipe's side is split in two faces at pipe_split, the rod's at
// rod_split. The concave junction at y > 0 rounded with 3: the ball touches
// both cylinders, the fill's section is the same at every height (between
// the rod, the ball's arc and the pipe), cut by the end planes. Near the
// inclined end the contact on the pipe leaves it first and the walk goes
// on beyond the pipe; past the rod's split a last piece follows whose
// contact on the rod reaches the inclined end, and the plane cuts both
// pieces. With the inclined end at the bottom, the last piece is not
// prolonged: next to the rod's contact the ball's section dips under the
// plane (the cylinders' normals are more than a right angle apart), so the
// plane's cut leaves the piece through its end section.
void check_rounding_between_rod_and_pipe(bool inclined_bottom) {
  const double R = 25.0, r = 5.0, d = 28.0, ball = 3.0, z0 = inclined_bottom ? 5.0 : 15.0, flat = 20.0;
  const double slope = inclined_bottom ? 0.5 : -0.5;
  const double pipe_split = inclined_bottom ? 13.0 : 7.0, rod_split = inclined_bottom ? 5.5 : 14.5;
  const auto stacked = [](const gp_Pnt& base, double radius, double split) {
    const gp_Dir up(0, 0, 1);
    const gp_Ax2 upper(base.Translated(gp_Vec(0, 0, split)), up);
    return BRepAlgoAPI_Fuse(BRepPrimAPI_MakeCylinder(gp_Ax2(base, up), radius, split).Shape(),
                            BRepPrimAPI_MakeCylinder(upper, radius, 40 - split).Shape())
        .Shape();
  };
  const TopoDS_Shape both =
      BRepAlgoAPI_Fuse(stacked(gp_Pnt(0, 0, 0), R, pipe_split), stacked(gp_Pnt(-d, 0, 0), r, rod_split)).Shape();
  // The half-space beyond the inclined plane: a big box turned about x.
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 5, 0), gp_Dir(1, 0, 0)), std::atan(slope));
  gp_Trsf lift;
  lift.SetTranslation(gp_Vec(0, 0, z0));
  const TopoDS_Shape beyond =
      BRepBuilderAPI_Transform(
          BRepPrimAPI_MakeBox(gp_Pnt(-100, -100, inclined_bottom ? -100 : 0), 200, 200, 100).Shape(), lift * turn)
          .Shape();
  TopoDS_Shape body = BRepAlgoAPI_Cut(both, beyond).Shape();
  if (inclined_bottom) {
    body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-100, -100, flat), 200, 200, 100).Shape()).Shape();
  }
  // The junction J, the ball's centre C, its contacts P1 (rod), P2 (pipe).
  const double jx = -(d * d + R * R - r * r) / (2 * d), jy = std::sqrt(R * R - jx * jx);
  const double cx = -(d * d + (R + ball) * (R + ball) - (r + ball) * (r + ball)) / (2 * d);
  const double cy = std::sqrt((R + ball) * (R + ball) - cx * cx);
  const double p1x = -d + r * (cx + d) / (r + ball), p1y = r * cy / (r + ball);
  const double p2x = R * cx / (R + ball), p2y = R * cy / (R + ball);
  const TopoDS_Shape rounded =
      occt_fillet(body, gp_Pnt(jx, jy, inclined_bottom ? 16.0 : 3.0), ball);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  // The section's area and first moment in y by Green's theorem, round its
  // three arcs (the short way): J to P1 on the rod, P1 to P2 on the ball,
  // P2 to J on the pipe.
  double area = 0.0, moment = 0.0;
  const auto arc = [&](double a, double b, double rho, double x0, double y0, double x1, double y1) {
    const double t0 = std::atan2(y0 - b, x0 - a);
    double t1 = std::atan2(y1 - b, x1 - a);
    if (t1 - t0 > kPi) {
      t1 -= 2 * kPi;
    } else if (t0 - t1 > kPi) {
      t1 += 2 * kPi;
    }
    area += 0.5 * (rho * rho * (t1 - t0) +
                   rho * (a * (std::sin(t1) - std::sin(t0)) - b * (std::cos(t1) - std::cos(t0))));
    const auto m = [&](double t) {
      const double c = std::cos(t);
      return b * b * -c + 2 * b * rho * (t / 2 - std::sin(2 * t) / 4) + rho * rho * (-c + c * c * c / 3);
    };
    moment += 0.5 * rho * (m(t1) - m(t0));
  };
  arc(-d, 0, r, jx, jy, p1x, p1y);
  arc(cx, cy, ball, p1x, p1y, p2x, p2y);
  arc(0, 0, R, p2x, p2y, jx, jy);
  if (area < 0) {
    area = -area;
    moment = -moment;
  }
  // The fill between the planes: the section times the height over it,
  // the plane's height at a point being z0 + slope (y - 5).
  const double fill = inclined_bottom ? (flat - z0 + 5 * slope) * area - slope * moment
                                      : (z0 - 5 * slope) * area + slope * moment;
  CHECK_NEAR_TOL(volume_of(rounded), volume_of(body) + fill, 1e-9);
  // The rounding in three pieces (its two walked pieces near the inclined
  // end and the one below the pipe's split).
  CHECK(faces_of(rounded) == faces_of(body) + 3);
}

void test_rounding_between_rod_and_pipe_ends_at_an_inclined_top() {
  check_rounding_between_rod_and_pipe(false);
}

void test_rounding_between_rod_and_pipe_ends_at_an_inclined_bottom() {
  check_rounding_between_rod_and_pipe(true);
}

// A cylinder of radius 17 and height 25 whose top is crossed by six
// vertical cylindrical cuts of radius a, centred d from the axis at 30 +
// 60 k degrees (their seams turned towards the axis, away from the
// rounding); the top rim rounded with 1.5. The torus's contact on the top
// (radius 15.5) runs over each cut, and the cut's wall, carried up, cuts
// the torus. What goes is the corner outside the quarter circle at radius
// rho in [15.5, 17], depth h(rho), less the parts within the cuts.
void rim_rounding_over_cuts(const TopoDS_Shape& body, double d, double a) {
  // (The rim's middle lies opposite its seam.)
  const TopoDS_Shape rounded = occt_fillet(body, gp_Pnt(-17, 0, 25), 1.5);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  const auto h = [](double rho) { return 1.5 - std::sqrt(std::max(0.0, 2.25 - (rho - 15.5) * (rho - 15.5))); };
  double removed = 0.0;
  const int n = 400000;
  for (int i = 0; i < n; ++i) {
    const double rho = 15.5 + 1.5 * (i + 0.5) / n;
    // (The angle each cut takes at radius rho.)
    const double c = (rho * rho + d * d - a * a) / (2 * rho * d);
    const double taken = c < 1.0 ? 6 * 2 * std::acos(std::max(-1.0, c)) : 0.0;
    removed += (2 * kPi - taken) * rho * h(rho) * 1.5 / n;
  }
  // (Integrated finely: the rounding's faces span the cuts.)
  GProp_GProps before, after;
  BRepGProp::VolumeProperties(body, before, 1e-10);
  BRepGProp::VolumeProperties(rounded, after, 1e-10);
  CHECK_NEAR_TOL(before.Mass() - after.Mass(), removed, 1e-5);
}

TopoDS_Shape cylinder_with_cuts(double bore, double d, double a, double depth) {
  TopoDS_Shape body = BRepPrimAPI_MakeCylinder(17.0, 25.0).Shape();
  if (bore > 0) {
    body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), bore, 27).Shape())
               .Shape();
  }
  for (int k = 0; k < 6; ++k) {
    const double t = (30.0 + 60.0 * k) * kPi / 180.0;
    const gp_Ax2 axis(gp_Pnt(d * std::cos(t), d * std::sin(t), 25.0 - depth), gp_Dir(0, 0, 1),
                      gp_Dir(-std::cos(t), -std::sin(t), 0));
    body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeCylinder(axis, a, depth + 1).Shape()).Shape();
  }
  return body;
}

void test_rim_rounding_over_blind_holes_is_cut_by_them() {
  // Holes of radius 1, 15 from the axis, 5 deep: the vertices at their
  // bottoms lie far below the rounding.
  const TopoDS_Shape body = cylinder_with_cuts(0.0, 15.0, 1.0, 5.0);
  rim_rounding_over_cuts(body, 15.0, 1.0);
}

void test_rim_rounding_over_notches_is_cut_by_them() {
  // A bore of radius 13.5 through the cylinder, notches of radius 1.5
  // 14.5 from the axis open to it: each notch is a part of a cylinder,
  // whose intersection with the torus is one closed line.
  const TopoDS_Shape body = cylinder_with_cuts(13.5, 14.5, 1.5, 30.0);
  rim_rounding_over_cuts(body, 14.5, 1.5);
}

} // namespace

void test_concave_rounding_rolls_on_a_strips_far_edge() {
  // An L-shaped section: a floor and a wall whose foot is a vertical strip
  // 1 high, above it the wall leans back by 30 degrees (a sharp convex edge
  // at the strip's top). The concave edge between the floor and the strip
  // rounded with 3: a ball touching the floor and the leaning wall exists,
  // but the rounding rolls on the strip's top edge. Its centre lies 3 above
  // the floor and sqrt 5 in front of the strip; the section filled lies
  // under the arc from the floor to the edge: 2 sqrt 5 - 4.5 asin(sqrt 5 / 3).
  const double lean = 9.0 * std::tan(kPi / 6.0);
  const TopoDS_Shape body = profile_prism({{20, 0}, {0, 0}, {0, 1}, {-lean, 10}, {-15, 10}, {-15, -5}, {20, -5}});
  const TopoDS_Shape filled = occt_fillet(body, gp_Pnt(20, 0, 0), 3);
  CHECK(!filled.IsNull());
  if (!filled.IsNull()) {
    const double fill = 2.0 * std::sqrt(5.0) - 4.5 * std::asin(std::sqrt(5.0) / 3.0);
    CHECK_NEAR_TOL(volume_of(filled), volume_of(body) + fill * 40.0, 1e-7);
    // The strip gone, the rounding added.
    CHECK(faces_of(filled) == faces_of(body));
  }
}

// A groove 40 long in the top (z = 0) of a block, its section a flat
// bottom d deep and 4 wide, concave arcs of radius 5 and convex arcs of
// radius 3 tangent to the top. The chain of edges at the foot of its end
// wall (x = 0) rounded with 1.5: towards the top the wall is lower than the
// rounding, which runs on past the wall's top edge, and the top cuts it. It
// ends in a point where its contact on the convex arc reaches the top (the
// spine's ends are the wall's corners on the top: no corner is made there).
// The result has the body's faces and added_faces more.
void check_rounding_at_a_groove_end(double d, int added_faces) {
  const double a = 3.0, b = 5.0, yb = 2.0, r = 1.5;
  const double dz = d - a - b;
  const double y1 = yb + std::sqrt((a + b) * (a + b) - dz * dz);
  const gp_Pnt2d C1(-y1, -a), C2(-yb, -d + b);
  const gp_Pnt2d T(C1.X() + (C2.X() - C1.X()) * a / (a + b), C1.Y() + (C2.Y() - C1.Y()) * a / (a + b));
  BRepBuilderAPI_MakeWire wire;
  const auto p3 = [](double y, double z) { return gp_Pnt(0, y, z); };
  const auto seg = [&](double ya, double za, double yz, double zz) {
    wire.Add(BRepBuilderAPI_MakeEdge(p3(ya, za), p3(yz, zz)).Edge());
  };
  // (An arc from p to q round c, counter-clockwise in (y, z) or not.)
  const auto arc = [&](const gp_Pnt2d& c, double radius, const gp_Pnt2d& p, const gp_Pnt2d& q, bool ccw) {
    const gp_Circ circle(gp_Ax2(p3(c.X(), c.Y()), gp_Dir(ccw ? 1 : -1, 0, 0)), radius);
    wire.Add(BRepBuilderAPI_MakeEdge(circle, p3(p.X(), p.Y()), p3(q.X(), q.Y())).Edge());
  };
  const auto mirror = [](const gp_Pnt2d& p) { return gp_Pnt2d(-p.X(), p.Y()); };
  seg(-y1, 1, -y1, 0);
  arc(C1, a, gp_Pnt2d(-y1, 0), T, false);
  arc(C2, b, T, gp_Pnt2d(-yb, -d), true);
  seg(-yb, -d, yb, -d);
  arc(mirror(C2), b, gp_Pnt2d(yb, -d), mirror(T), true);
  arc(mirror(C1), a, mirror(T), gp_Pnt2d(y1, 0), false);
  seg(y1, 0, y1, 1);
  seg(y1, 1, -y1, 1);
  const TopoDS_Shape groove =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(40, 0, 0)).Shape();
  const TopoDS_Shape body = unified(
      BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(-10, -20, -20), gp_Pnt(50, 20, 0)).Shape(), groove).Shape());
  const TopoDS_Shape filled = occt_fillet(body, gp_Pnt(0, 0, -d), r);
  CHECK(!filled.IsNull());
  if (filled.IsNull()) {
    return;
  }
  // The material added: along the section's curve (arclength s, point p,
  // normal n into the groove, the offset's curvature k), between the wall
  // and the curve outside the ball, its distance t from the curve up to
  // r (1 - cos al) where the wall distance is r (1 - sin al), and below the
  // top; the offset's area element is 1 - k t.
  const int ns = 4000, na = 1000;
  double fill = 0.0;
  const auto piece = [&](double len, auto&& at) {
    for (int i = 0; i < ns; ++i) {
      gp_Pnt2d p;
      gp_Vec2d n;
      double k = 0.0;
      at(len * (i + 0.5) / ns, p, n, k);
      for (int j = 0; j < na; ++j) {
        const double al = (kPi / 2.0) * (j + 0.5) / na;
        double t = r * (1.0 - std::cos(al));
        if (n.Y() > 0.0) {
          t = std::min(t, -p.Y() / n.Y());
        }
        if (t > 0.0) {
          fill += (t - 0.5 * k * t * t) * r * std::cos(al) * (kPi / 2.0 / na) * (len / ns);
        }
      }
    }
  };
  const double th1 = std::atan2(T.Y() - C1.Y(), T.X() - C1.X());
  const double th2 = std::atan2(T.Y() - C2.Y(), T.X() - C2.X());
  for (const double side : {-1.0, 1.0}) {
    piece(a * (kPi / 2.0 - th1), [&](double s, gp_Pnt2d& p, gp_Vec2d& n, double& k) {
      const double th = kPi / 2.0 - s / a;
      n = gp_Vec2d(side * std::cos(th), std::sin(th));
      p = gp_Pnt2d(side * C1.X() + a * n.X(), C1.Y() + a * n.Y());
      k = -1.0 / a;
    });
    const double l2 = b * std::abs(th2 + kPi / 2.0);
    piece(l2, [&](double s, gp_Pnt2d& p, gp_Vec2d& n, double& k) {
      const double th = th2 + (-kPi / 2.0 - th2) * s / l2;
      n = gp_Vec2d(-side * std::cos(th), -std::sin(th));
      p = gp_Pnt2d(side * C2.X() - b * n.X(), C2.Y() - b * n.Y());
      k = 1.0 / b;
    });
  }
  piece(2.0 * yb, [&](double s, gp_Pnt2d& p, gp_Vec2d& n, double& k) {
    p = gp_Pnt2d(-yb + s, -d);
    n = gp_Vec2d(0, 1);
    k = 0.0;
  });
  CHECK_NEAR_TOL(volume_of(filled) - volume_of(body), fill, 1e-5);
  CHECK(faces_of(filled) == faces_of(body) + added_faces);
}

void test_rounding_at_a_groove_end_is_cut_by_the_top() {
  // The bottom 2 deep: the wall is higher than the rounding at the bottom.
  // The rounding's faces: one on each convex arc's edge (cut by the top all
  // along), two on each concave arc's (within the wall and cut by the top),
  // one on the bottom's; the wall is kept between them.
  check_rounding_at_a_groove_end(2.0, 7);
}

void test_rounding_at_a_groove_end_consumes_the_wall() {
  // The bottom as deep as the rounding: along the bottom the rounding's
  // contact runs along the wall's top edge, elsewhere past it; the wall
  // vanishes. The rounding's faces: one on each edge's (five), the wall
  // gone.
  check_rounding_at_a_groove_end(1.5, 4);
}

// A block 20 x 20 x 20 with a slot 4 deep in its side x = 20: a stadium 3
// wide (y from 8.5 to 11.5) whose straight sides run from z = 13 to 18,
// its ends half cylinders of radius 1.5 (axes along x). The top edge of
// that side rounded with 3 (axis at x = z = 17): the contact on the side
// (z = 17) crosses the slot between its straight sides, whose planes cut
// the rounding along its sections; the slot's top cuts it in between.
// What goes is the corner outside the rounding's cylinder, less its part
// in the slot.
void test_rounding_over_a_slot_is_cut_by_its_faces() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 20)).Shape();
  const auto end = [](double z) {
    return BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(16, 10, z), gp_Dir(1, 0, 0), gp_Dir(0, 0, z > 15 ? -1 : 1)), 1.5,
                                    5)
        .Shape();
  };
  const TopoDS_Shape slot =
      BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(BRepPrimAPI_MakeBox(gp_Pnt(16, 8.5, 13), gp_Pnt(21, 11.5, 18)).Shape(),
                                        end(13))
                           .Shape(),
                       end(18))
          .Shape();
  const TopoDS_Shape body = unified(BRepAlgoAPI_Cut(block, slot).Shape());
  const TopoDS_Shape rounded = occt_fillet(body, gp_Pnt(20, 10, 20), 3.0);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  const TopoDS_Shape corner = BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(17, -1, 17), gp_Pnt(20, 21, 20)).Shape(),
                                              BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(17, -2, 17), gp_Dir(0, 1, 0)), 3, 24)
                                                  .Shape())
                                  .Shape();
  // (Integrated finely: the default leaves 2e-5 on the cut cylinders.)
  const auto fine_volume = [](const TopoDS_Shape& shape) {
    GProp_GProps props;
    BRepGProp::VolumeProperties(shape, props, 1e-10);
    return props.Mass();
  };
  CHECK_NEAR_TOL(fine_volume(rounded),
                 fine_volume(body) - fine_volume(BRepAlgoAPI_Common(corner, block).Shape())
                     + fine_volume(BRepAlgoAPI_Common(corner, slot).Shape()),
                 1e-9);
  // The rounding one face (its pieces beside the slot and over it are one
  // surface), the slot's sides and top cut back.
  CHECK(faces_of(rounded) == faces_of(body) + 1);
}

// A block 80 x 40 x 92 with a ring 4 high (radii 22.5 and 26.5, axis along
// y) standing on its face y = 0, 3 from its side x = 0. The ring's foot
// chamfered by 4: the chamfer's contact on the ring runs along its top
// edge (the ring's side goes), on the block it lies 30.5 from the axis and
// passes the side x = 0, which cuts the chamfer's cone there. What comes is
// the cone's frustum outside the ring, on the block's side of x = 0.
void test_chamfer_round_a_ring_past_the_side_is_cut_by_it() {
  const gp_Pnt centre(29.5, 0, 46);
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(80, 40, 92)).Shape();
  const gp_Ax2 down(gp_Pnt(centre.X(), -4, centre.Z()), gp_Dir(0, 1, 0));
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(down, 26.5, 4).Shape(),
                                            BRepPrimAPI_MakeCylinder(down, 22.5, 4).Shape())
                                .Shape();
  const TopoDS_Shape body = unified(BRepAlgoAPI_Fuse(block, ring).Shape());
  BRepFilletAPI_MakeChamfer maker(body);
  // (The foot: the circle of radius 26.5 on the block's face.)
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    BRepAdaptor_Curve c(TopoDS::Edge(e.Current()));
    if (c.GetType() == GeomAbs_Circle && std::abs(c.Circle().Radius() - 26.5) < 1e-9 &&
        std::abs(c.Circle().Location().Y()) < 1e-9 && maker.NbContours() == 0) {
      maker.Add(4.0, TopoDS::Edge(e.Current()));
    }
  }
  maker.Build();
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape chamfered = maker.Shape();
  CHECK(BRepCheck_Analyzer(chamfered).IsValid());
  const TopoDS_Shape frustum = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCone(down, 26.5, 30.5, 4).Shape(),
                                               BRepPrimAPI_MakeCylinder(down, 26.5, 4).Shape())
                                   .Shape();
  const TopoDS_Shape side = BRepPrimAPI_MakeBox(gp_Pnt(0, -10, -10), gp_Pnt(100, 10, 110)).Shape();
  CHECK_NEAR_TOL(volume_of(chamfered), volume_of(body) + volume_of(BRepAlgoAPI_Common(frustum, side).Shape()),
                 1e-9);
  // The ring's side gone, the chamfer one face: its pieces beside the
  // block's side and the one it cuts are one cone, joined across the
  // closed spine's start (patch 0146; three faces before).
  CHECK(faces_of(chamfered) == faces_of(body));
}

// An inside corner between a wall N (the side y = 0 of a plate 2 thick under
// the top z = 0, the plate on y < 0) and a wall W (the side x = 0 of a block
// 10 deep on x < 0) whose top edge is rounded with 2; the corner's edge, the
// arc where N meets W's rounding, rounded with 3. Near the top the contact
// on N leaves N across its top edge before the arc ends: the rounding runs
// on, the top cuts it, and it ends in a point on the edge between W's
// rounding and the top, 3 from N (0074 for the walked blend, 0059 for the
// analytic one). The blend is the torus about the axis of W's rounding (its
// centre circle of radius 5 lies 3 from N). The fill, between N, W's
// rounding and the torus, revolved about that axis from the plate's bottom
// (the end section of the arc) up to the top: at distance rho from the axis
// the section's height is y(rho) and the angle up to the top asin(2 / rho);
// with rho = 5 - 3 cos(psi), y = 3 - 3 sin(psi) the integrand is smooth.
// With W's rounding made a surface of revolution the blend is walked.
TopoDS_Shape corner_beside_a_rounded_top_edge(bool walked) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-20, -5, -10), gp_Pnt(0, 20, 0)).Shape();
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-5, -20, -2), gp_Pnt(20, 0, 0)).Shape();
  const TopoDS_Shape body = unified(BRepAlgoAPI_Fuse(block, plate).Shape());
  BRepFilletAPI_MakeFillet rounding(body);
  rounding.Add(2.0, edge_near(body, gp_Pnt(0, 10, 0)));
  rounding.Build();
  CHECK(rounding.IsDone());
  if (!rounding.IsDone()) {
    return TopoDS_Shape();
  }
  return walked ? ShapeCustom::ConvertToRevolution(rounding.Shape()) : rounding.Shape();
}

void test_corner_rounding_beside_a_rounded_top_edge_is_cut_by_the_top() {
  double fill = 0.0;
  const int n = 20000;
  for (int i = 0; i < n; ++i) {
    const double psi = 0.5 * kPi * (i + 0.5) / n;
    const double rho = 5.0 - 3.0 * std::cos(psi);
    fill += rho * std::asin(2.0 / rho) * (3.0 - 3.0 * std::sin(psi)) * 3.0 * std::sin(psi) * (0.5 * kPi / n);
  }
  for (const bool walked : {false, true}) {
    const TopoDS_Shape body = corner_beside_a_rounded_top_edge(walked);
    if (body.IsNull()) {
      continue;
    }
    // (The arc where N meets W's rounding, about (-2, 0, -2).)
    const TopoDS_Shape rounded = occt_fillet(body, gp_Pnt(-2 + std::sqrt(2.0), 0, -2 + std::sqrt(2.0)), 3);
    CHECK(!rounded.IsNull());
    if (rounded.IsNull()) {
      continue;
    }
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(body) + fill, 1e-8);
    bool end_point = false;
    for (TopExp_Explorer v(rounded, TopAbs_VERTEX); v.More(); v.Next()) {
      end_point = end_point || BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(gp_Pnt(-2, 3, 0)) < 1e-6;
    }
    CHECK(end_point);
    // The analytic blend is one face; the walked one is two (the part cut
    // by the top and the rest), and its end at the plate's bottom is a face
    // of its own.
    CHECK(faces_of(rounded) == faces_of(body) + (walked ? 3 : 1));
  }
}

// A hole near a rounded edge whose rim is rounded more sharply than the
// rounding: the contact on the face of the hole crosses the hole's mouth,
// where the ball cannot touch the rim (it bends towards the ball more
// sharply than the ball). The walked blend runs on over the mouth on the
// face's own surface and the rim cuts it (mitcad#238). Exact check: the
// rounding removes from the body what the ball's corner region holds.
// The edge of a shape that passes nearest to p.
TopoDS_Edge edge_through(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), edge);
    if (distance.IsDone() && distance.Value() < nearest) {
      nearest = distance.Value();
      best = edge;
    }
  }
  return best;
}

// OCCT's fillet of the edge through a point; the result, valid, or a null
// shape.
TopoDS_Shape occt_fillet_through(const TopoDS_Shape& shape, const gp_Pnt& on_edge, double radius) {
  BRepFilletAPI_MakeFillet maker(shape);
  maker.Add(radius, edge_through(shape, on_edge));
  maker.Build();
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Shape()).IsValid()) {
    return TopoDS_Shape();
  }
  return maker.Shape();
}

TopoDS_Shape rim_rounded_hole(const TopoDS_Shape& body, const gp_Ax2& axis, double bore, double rim, double depth) {
  gp_Ax2 tool = axis;
  tool.Translate(gp_Vec(axis.Direction()) * -1.0);
  const TopoDS_Shape hole = BRepPrimAPI_MakeCylinder(tool, bore, depth + 2.0).Shape();
  const TopoDS_Shape holed = BRepAlgoAPI_Cut(body, hole).Shape();
  // (The rim: the hole's edge on the face its axis starts on.)
  BRepFilletAPI_MakeFillet rounding(holed);
  rounding.Add(rim, edge_through(holed, axis.Location().Translated(gp_Vec(axis.XDirection()) * bore)));
  rounding.Build();
  CHECK(rounding.IsDone());
  return rounding.IsDone() ? rounding.Shape() : TopoDS_Shape();
}

void test_rounding_over_a_hole_with_a_rounded_rim_is_cut_by_the_rim() {
  // A block's top front edge rounded with 3; a hole of radius 1.5 down
  // from the top 4.6 behind the front, its rim rounded with 0.5: the mouth
  // reaches 2.6 from the front, over the contact on the top (3), the bore
  // (from 3.1) stays behind it. The analytic blend is split at the hole;
  // the gap over it is walked. The rim's seam in front (the rim cuts the
  // blend across it; its end under the blend goes) or behind the hole.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(40, 20, 10)).Shape();
  const TopoDS_Shape corner = BRepPrimAPI_MakeBox(gp_Pnt(-1, 0, 7), gp_Pnt(41, 3, 10)).Shape();
  const TopoDS_Shape ball = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-2, 3, 7), gp_Dir(1, 0, 0)), 3, 44).Shape();
  const TopoDS_Shape region = BRepAlgoAPI_Cut(corner, ball).Shape();
  for (const double seam : {-1.0, 1.0}) {
    const TopoDS_Shape body =
        rim_rounded_hole(block, gp_Ax2(gp_Pnt(20, 4.6, 10), gp_Dir(0, 0, -1), gp_Dir(0, seam, 0)), 1.5, 0.5, 10);
    if (body.IsNull()) {
      continue;
    }
    const TopoDS_Shape rounded = occt_fillet_through(body, gp_Pnt(5, 0, 10), 3.0);
    CHECK(!rounded.IsNull());
    if (rounded.IsNull()) {
      continue;
    }
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(BRepAlgoAPI_Cut(body, region).Shape()), 1e-6);
  }
}

void test_closed_walked_rounding_over_a_hole_with_a_rounded_rim_is_cut_by_the_rim() {
  // A disc of radius 17, 3 thick, its bottom edge rounded with 1 (its
  // cylinder made a surface of revolution: the blend is walked round the
  // closed edge). A hole of radius 0.9 through it, 15 from the axis, its
  // bottom rim rounded with 0.5: the mouth reaches 16.4 from the axis, over
  // the contact on the bottom (16), the bore (to 15.9) stays inside it.
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)), 17, 3).Shape();
  const TopoDS_Shape holed =
      rim_rounded_hole(disc, gp_Ax2(gp_Pnt(-15, 0, 0), gp_Dir(0, 1, 0), gp_Dir(-1, 0, 0)), 0.9, 0.5, 3);
  if (holed.IsNull()) {
    return;
  }
  const TopoDS_Shape body = ShapeCustom::ConvertToRevolution(holed);
  const TopoDS_Shape rounded = occt_fillet_through(body, gp_Pnt(17, 0, 0), 1.0);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -1, 0), gp_Dir(0, 1, 0)), 18, 2).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -2, 0), gp_Dir(0, 1, 0)), 16, 4).Shape();
  const TopoDS_Shape tube = BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0, 1, 0), gp_Dir(0, 1, 0)), 16, 1).Shape();
  const TopoDS_Shape band = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)), 18, 1).Shape();
  const TopoDS_Shape region =
      BRepAlgoAPI_Cut(BRepAlgoAPI_Common(BRepAlgoAPI_Cut(outer, inner).Shape(), band).Shape(), tube).Shape();
  CHECK_NEAR_TOL(volume_of(rounded), volume_of(BRepAlgoAPI_Cut(body, region).Shape()), 1e-5);
}

// The direction at angle a (degrees) from +z about +y.
gp_Dir around_y(double a) {
  const double r = a * kPi / 180.0;
  return gp_Dir(std::sin(r), 0, std::cos(r));
}

// A disc of radius 17, 1.5 thick about +y, its bottom edge rounded with 1,
// turned down to radius 16.9 (the turned cylinder's seam at +z, the disc's,
// which the rounding's torus takes, at disc_seam degrees about +y): the
// earlier rounding meets the cylinder at a sharp angle 0.564 above the
// bottom. A hole of radius 1 through it whose centre lies 14.6955 from the
// axis at hole degrees, its bottom rim rounded with 0.5 (the rim's seam at
// rim_seam degrees from outwards): the rim reaches 16.1955 from the axis,
// into the earlier rounding. (The cylinder made first, its face comes first
// in the body, as in the files' bodies.)
TopoDS_Shape turned_disc(bool hole, double hole_at, double disc_seam, double rim_seam) {
  const gp_Dir up(0, 1, 0);
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), up, around_y(disc_seam)), 17, 1.5).Shape();
  BRepFilletAPI_MakeFillet first(disc);
  first.Add(1.0, edge_through(disc, gp_Pnt(17, 0, 0)));
  first.Build();
  CHECK(first.IsDone());
  if (!first.IsDone()) {
    return TopoDS_Shape();
  }
  TopoDS_Shape body =
      BRepAlgoAPI_Common(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -1, 0), up, gp_Dir(0, 0, 1)), 16.9, 4).Shape(),
                         first.Shape())
          .Shape();
  if (hole) {
    const gp_Pnt centre = gp_Pnt(0, 0, 0).Translated(gp_Vec(around_y(hole_at)) * 14.6955);
    const gp_Dir out = around_y(hole_at + rim_seam);
    const gp_Ax2 axis(centre.Translated(gp_Vec(0, -1, 0)), up, out);
    // (The rim: the ring under its torus, then the bore.)
    const TopoDS_Shape flare =
        BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(axis, 1.5, 1.5).Shape(),
                        BRepPrimAPI_MakeTorus(gp_Ax2(centre.Translated(gp_Vec(0, 0.5, 0)), up, out), 1.5, 0.5).Shape())
            .Shape();
    const TopoDS_Shape bore = BRepPrimAPI_MakeCylinder(axis, 1.0, 4).Shape();
    body = BRepAlgoAPI_Cut(body, BRepAlgoAPI_Fuse(flare, bore).Shape()).Shape();
  }
  return body;
}

void test_closed_rounding_consumes_an_earlier_rounding_over_a_rounded_rim() {
  // The edge between the turned cylinder and the earlier rounding rounded
  // with 1: the ball touches the cylinder and the bottom (centre 15.9 from
  // the axis, 1 above the bottom), so the start overflows the earlier
  // rounding, which lies wholly between the contacts all round the closed
  // edge and vanishes. Over the hole the contact on the bottom crosses the
  // rim, which the ball cannot touch (0.5 under a ball of 1): the blend is
  // walked over the mouth and the rim cuts it (mitcad#261, port patch
  // 0190). The hole beside the closed edge's start; on the turned
  // cylinder's seam, where the contact on the cylinder crosses that seam
  // inside the walk over the mouth (the disc's seam out of the way); and
  // with no hole (the earlier rounding vanished before the patch already).
  // Exact check: the rounding removes from the body what the ball's corner
  // region round the bottom edge holds (within the walked blend's
  // approximation).
  struct Case {
    bool hole;
    double hole_at, disc_seam, rim_seam;
  };
  for (const Case& c : {Case{true, 3.45, 0.0, 0.0}, Case{true, 0.0, -30.0, 10.0}, Case{false, 0.0, 0.0, 0.0}}) {
    const TopoDS_Shape body = turned_disc(c.hole, c.hole_at, c.disc_seam, c.rim_seam);
    if (body.IsNull()) {
      continue;
    }
    const double y = 1.0 - std::sqrt(1.0 - 0.9 * 0.9);
    // (Through the edge away from the seams.)
    const TopoDS_Shape rounded = occt_fillet_through(body, gp_Pnt(16.9, y, 0), 1.0);
    CHECK(!rounded.IsNull());
    if (rounded.IsNull()) {
      continue;
    }
    const gp_Dir up(0, 1, 0);
    const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), up), 17.9, 1).Shape();
    const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -1, 0), up), 15.9, 3).Shape();
    const TopoDS_Shape tube = BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0, 1, 0), up), 15.9, 1).Shape();
    const TopoDS_Shape region = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(outer, inner).Shape(), tube).Shape();
    GProp_GProps got, expected;
    BRepGProp::VolumeProperties(rounded, got, 1e-9);
    BRepGProp::VolumeProperties(BRepAlgoAPI_Cut(body, region).Shape(), expected, 1e-9);
    CHECK_NEAR_TOL(got.Mass(), expected.Mass(), 1e-4);
    // No face left on the earlier rounding's torus (major radius 16).
    for (TopExp_Explorer f(rounded, TopAbs_FACE); f.More(); f.Next()) {
      const BRepAdaptor_Surface s(TopoDS::Face(f.Current()));
      CHECK(s.GetType() != GeomAbs_Torus || std::abs(s.Torus().MajorRadius() - 16.0) > 1e-6);
    }
  }
}

// The area a rounding of radius r takes from the section of an edge whose
// faces meet at an angle phi inside the solid: r^2 (cot(phi / 2) - (pi - phi) / 2).
double rounding_area(double r, double phi) { return r * r * (1.0 / std::tan(phi / 2.0) - (kPi - phi) / 2.0); }

void test_rounding_round_a_chamfered_box_walks_on_along_its_rounded_corners() {
  // A box 60 x 40 x 20 whose vertical edges are rounded with 2, its bottom
  // front edge chamfered by 2 (the chamfer runs on round the box), then the
  // edge between the chamfer and the front rounded with 5 (mitcad#308). The
  // contour runs on round the box too, between the chamfer's cones and the
  // corners' cylinders, which bend towards the ball more sharply than the
  // ball along the contour: the walk goes on along those faces of its
  // contour, as before port patch 0160, which took them for the rounded rim
  // of a hole the ball cannot touch and failed (port patch 0250). The
  // straight runs (2 x 56 + 2 x 36 long) take the section of a rounding
  // between faces at 135 degrees; round each corner the same section turns
  // a quarter about the corner's axis, less than 2 from it.
  const double straight = rounding_area(5.0, 0.75 * kPi) * (2.0 * 56.0 + 2.0 * 36.0);
  const double corners = rounding_area(5.0, 0.75 * kPi) * 4.0 * (kPi / 2.0) * 2.0;
  {
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(60, 40, 20).Shape();
    const TopoDS_Shape rounded = occt_fillet(
        box, {gp_Pnt(0, 0, 10), gp_Pnt(60, 0, 10), gp_Pnt(60, 40, 10), gp_Pnt(0, 40, 10)}, 2.0);
    CHECK(!rounded.IsNull());
    const TopoDS_Shape chamfered = rounded.IsNull() ? rounded : occt_chamfer(rounded, gp_Pnt(30, 0, 0), 2.0);
    CHECK(!chamfered.IsNull());
    const TopoDS_Shape result = chamfered.IsNull() ? chamfered : occt_fillet(chamfered, gp_Pnt(30, 0, 2), 5.0);
    CHECK(!result.IsNull());
    if (!result.IsNull()) {
      const double removed = volume_of(chamfered) - volume_of(result);
      CHECK(removed > straight);
      CHECK(removed < straight + corners);
    }
  }
  // Mitcad's fillet of that edge by its name, after the extrusion, the
  // corners' fillet and the chamfer.
  const ShapePtr box = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  std::vector<std::string> corner_edges;
  for (int i = 0; i < 4; ++i) {
    corner_edges.push_back(edge(side("F2", 1, i), side("F2", 1, (i + 1) % 4)));
  }
  const ShapePtr rounded = fillet("F7", *box, corner_edges, 2.0);
  ChamferSpec spec;
  spec.distance = 2.0;
  const std::string bottom_front = edge(start_cap("F2", 1), side("F2", 1, 0));
  const ShapePtr chamfered = chamfer("F11", *rounded, {bottom_front}, spec);
  const std::string target = edge("F11:chamfer(" + bottom_front + ")", side("F2", 1, 0));
  const ShapePtr result = fillet("F12", *chamfered, {target}, 5.0);
  CHECK(detail::is_valid(result->occt()));
  CHECK(faces_named(*result, "F12:fillet(" + target + ")") >= 1);
  const double removed = volume_of(chamfered->occt()) - volume_of(result->occt());
  CHECK(removed > straight);
  CHECK(removed < straight + corners);
}

void test_top_rounding_larger_than_the_rounded_corners_walks_round_them() {
  // A box 60 x 40 x 20 whose vertical edges are rounded with 1, then its top
  // edges rounded with 1.5 (mitcad#297; a rim of tabs and notches with
  // corners rounded with 1 under a rounding of 1.5). The contour runs round
  // the box, between the top and the corners' cylinders, which bend towards
  // the ball (inside the solid) more sharply than the ball: the walk goes on
  // along those faces of its contour (port patch 0250; port patch 0160 took
  // them for a rounded rim the ball cannot touch, and the fillet failed).
  // The straight runs (2 x 58 + 2 x 38 long) take the section of a rounding
  // between faces at a right angle; round each corner the rest lies within
  // 1 of the corner's axis and turns a quarter about it.
  const double section = rounding_area(1.5, kPi / 2.0);
  const double straight = section * (2.0 * 58.0 + 2.0 * 38.0);
  const double corners = section * 4.0 * (kPi / 2.0) * 1.0;
  {
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(60, 40, 20).Shape();
    const TopoDS_Shape rounded = occt_fillet(
        box, {gp_Pnt(0, 0, 10), gp_Pnt(60, 0, 10), gp_Pnt(60, 40, 10), gp_Pnt(0, 40, 10)}, 1.0);
    CHECK(!rounded.IsNull());
    const TopoDS_Shape result = rounded.IsNull() ? rounded : occt_fillet(rounded, gp_Pnt(30, 0, 20), 1.5);
    CHECK(!result.IsNull());
    if (!result.IsNull()) {
      const double removed = volume_of(rounded) - volume_of(result);
      CHECK(removed > straight);
      CHECK(removed < straight + corners);
    }
  }
  // Mitcad's fillet of the top front edge by its name, after the extrusion
  // and the corners' fillet.
  const ShapePtr box = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  std::vector<std::string> corner_edges;
  for (int i = 0; i < 4; ++i) {
    corner_edges.push_back(edge(side("F2", 1, i), side("F2", 1, (i + 1) % 4)));
  }
  const ShapePtr rounded = fillet("F7", *box, corner_edges, 1.0);
  const std::string target = edge(end_cap("F2", 1), side("F2", 1, 0));
  const ShapePtr result = fillet("F8", *rounded, {target}, 1.5);
  CHECK(detail::is_valid(result->occt()));
  CHECK(faces_named(*result, "F8:fillet(" + target + ")") >= 1);
  const double removed = volume_of(rounded->occt()) - volume_of(result->occt());
  CHECK(removed > straight);
  CHECK(removed < straight + corners);
}

// A boss of radius 5 and height 3 on a plate, its foot rounded with 3, as
// high as the boss: the rounding's contact runs along the boss's top edge,
// the wall vanishes, and the top edge lies between the top and the foot's
// torus, at a right angle (patch 0350). The rounding marked that edge as
// tangent (a regularity G1, for the wall it was tangent to), and rounding the
// top edge then failed for any radius: the spine of the one closed edge was
// taken as ending at its vertex (not tangent there across the faces said to
// be tangent), its start was found on the guide's prolongation past the end
// and the walk stopped at once. The top's rounding removes the section
// between the top, the torus and the ball's arc, revolved: its volume by
// Green's theorem, 2 pi times the integral of rho^2 / 2 dz round it.
void test_boss_top_rounded_after_its_foot_rounded_as_high_as_it() {
  const double R = 5.0, h = 3.0;
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, -5), gp_Pnt(20, 20, 0)).Shape();
  BRepAlgoAPI_Fuse fuse(plate, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), R, h).Shape());
  fuse.SimplifyResult();
  const TopoDS_Shape footed = occt_fillet(fuse.Shape(), gp_Pnt(-R, 0, 0), h);
  CHECK(!footed.IsNull());
  if (footed.IsNull()) {
    return;
  }
  // The top edge, between the top and the torus, is not marked tangent.
  const TopoDS_Edge top = edge_near(footed, gp_Pnt(-R, 0, h));
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer f(footed, TopAbs_FACE); f.More(); f.Next()) {
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      if (e.Current().IsSame(top)) {
        faces.push_back(TopoDS::Face(f.Current()));
        break;
      }
    }
  }
  CHECK(faces.size() == 2u);
  if (faces.size() == 2u) {
    CHECK(BRep_Tool::Continuity(top, faces[0], faces[1]) == GeomAbs_C0);
  }
  for (const double r : {0.5, 1.0, 2.0}) {
    const TopoDS_Shape rounded = occt_fillet(footed, gp_Pnt(-R, 0, h), r);
    CHECK(!rounded.IsNull());
    if (rounded.IsNull()) {
      continue;
    }
    // The ball's centre: r below the top, h + r from the torus's centre
    // circle (radius R + h, height h).
    const double d = std::sqrt(h * h + 2.0 * h * r);
    const double rc = R + h - d;
    const int n = 4000;
    const auto simpson = [n](const auto& f, double a, double b) {
      double sum = 0.0;
      for (int i = 0; i <= n; ++i) {
        sum += (i == 0 || i == n ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0)) * f(a + (b - a) * i / n);
      }
      return sum * (b - a) / (3.0 * n);
    };
    // From the top edge down the torus to the ball's contact, then up the
    // ball's arc to its contact with the top (the top's part adds nothing).
    const double foot_arc = simpson(
        [&](double t) {
          const double rho = R + h + h * std::cos(t);
          return rho * rho / 2.0 * h * std::cos(t);
        },
        kPi, kPi + std::atan2(r, d));
    const double ball_arc = simpson(
        [&](double s) {
          const double rho = rc + r * std::cos(s);
          return rho * rho / 2.0 * r * std::cos(s);
        },
        std::atan2(r, d), kPi / 2.0);
    const double removed = 2.0 * kPi * std::abs(foot_arc + ball_arc);
    // (The blend is walked: an approximation within its tolerance.)
    CHECK_NEAR_TOL(volume_of(rounded) - volume_of(footed), -removed, 1e-5);
  }
}

// A box 10 x 20 x 30 rounded along two of its long edges, as in OCCT's
// draft/angle A2: each rounding is tangent to both its faces, and its
// straight edges with them keep their regularity (G1). Patch 0350 checked
// the tangency with the faces oriented as in the fillet's data structure,
// where a rounding and one of its faces are opposite, and dropped the G1 on
// half of them (mitcad#1880); a draft of the faces beside the roundings,
// which carries the drafted face on to the rounding along its tangent
// edges, then failed ("Impossible edge recomputation").
void test_box_roundings_keep_their_tangent_edges_and_take_a_draft() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(10, 20, 30).Shape();
  BRepFilletAPI_MakeFillet maker(box);
  maker.Add(2, edge_near(box, gp_Pnt(5, 0, 30)));
  maker.Add(3, edge_near(box, gp_Pnt(5, 20, 0)));
  maker.Build();
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape rounded = maker.Shape();
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> around;
  TopExp::MapShapesAndAncestors(rounded, TopAbs_EDGE, TopAbs_FACE, around);
  int tangent = 0;
  int sharp = 0;
  for (int i = 1; i <= around.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& faces = around.FindFromIndex(i);
    const TopoDS_Face f1 = TopoDS::Face(faces.First());
    const TopoDS_Face f2 = TopoDS::Face(faces.Last());
    const TopoDS_Edge edge = TopoDS::Edge(around.FindKey(i));
    const bool rounding = BRepAdaptor_Surface(f1).GetType() != GeomAbs_Plane ||
                          BRepAdaptor_Surface(f2).GetType() != GeomAbs_Plane;
    if (rounding && BRepAdaptor_Curve(edge).GetType() == GeomAbs_Line) {
      ++(BRep_Tool::Continuity(edge, f1, f2) == GeomAbs_C0 ? sharp : tangent);
    }
  }
  CHECK(tangent == 4);
  CHECK(sharp == 0);
  // The faces facing -y and -z, each beside one rounding, drafted by 10 and
  // 5 degrees about the plane x = 0.
  const auto plane_facing = [&rounded](const gp_Dir& normal) {
    for (TopExp_Explorer f(rounded, TopAbs_FACE); f.More(); f.Next()) {
      const TopoDS_Face& face = TopoDS::Face(f.Current());
      BRepAdaptor_Surface surface(face);
      if (surface.GetType() != GeomAbs_Plane) {
        continue;
      }
      gp_Dir n = surface.Plane().Axis().Direction();
      if (face.Orientation() == TopAbs_REVERSED) {
        n.Reverse();
      }
      if (n.IsEqual(normal, 1e-9)) {
        return face;
      }
    }
    return TopoDS_Face();
  };
  const TopoDS_Face front = plane_facing(gp_Dir(0, -1, 0));
  const TopoDS_Face bottom = plane_facing(gp_Dir(0, 0, -1));
  CHECK(!front.IsNull());
  CHECK(!bottom.IsNull());
  if (front.IsNull() || bottom.IsNull()) {
    return;
  }
  const gp_Pln neutral(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0));
  BRepOffsetAPI_DraftAngle draft(rounded);
  draft.Add(front, gp_Dir(1, 0, 0), 10.0 * kPi / 180.0, neutral);
  draft.Add(bottom, gp_Dir(1, 0, 0), 5.0 * kPi / 180.0, neutral);
  CHECK(draft.AddDone());
  draft.Build();
  CHECK(draft.IsDone());
  if (draft.IsDone()) {
    CHECK(BRepCheck_Analyzer(draft.Shape()).IsValid());
  }
}

// The name of the edge of `shape` nearest to the point.
std::string edge_at(const Shape& shape, const gp_Pnt& point) {
  const TopoDS_Shape vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
  int best = -1;
  double nearest = 1e100;
  for (int i = 0; i < shape.edge_count(); ++i) {
    BRepExtrema_DistShapeShape distance(vertex, shape.edge(i));
    if (distance.IsDone() && distance.Value() < nearest) {
      nearest = distance.Value();
      best = i;
    }
  }
  return best < 0 ? std::string() : shape.edge_name(best);
}

void test_rounding_rolled_on_a_ribs_far_edge_is_a_cylinder() {
  // mitcad#328: the rounding of a rib's top edge that rolls on the other
  // top edge (a ball touching a plane and an edge line: a cylinder) is
  // walked by OCCT as a B-spline surface with B-spline edges. It is the
  // cylinder, its edges lines and arcs, as in the bodies the files store
  // (in a design, a rounding of a boss's end face after such roundings
  // failed in OCCT's walk on the B-splines and builds on the analytic
  // faces); the roundings after it build on them.
  const ShapePtr plate = extrude("F2", {rectangle(1, -5, 0, 50, 20)}, 0, 5);
  const ShapePtr rib = extrude("F3", {rectangle(5, 0, 9.5, 40, 1)}, 5, 15);
  const ShapePtr body = joined(plate, rib);
  const ShapePtr rounded = fillet("F4", *body, {edge(end_cap("F3", 5), side("F3", 5, 0))}, 2);
  CHECK(detail::is_valid(rounded->occt()));
  const std::vector<int> faces = rounded->find_faces(end_cap("F3", 5));
  CHECK(faces.size() == 1);
  if (faces.size() == 1) {
    const TopoDS_Face& face = rounded->face(faces[0]);
    CHECK(BRepAdaptor_Surface(face).GetType() == GeomAbs_Cylinder);
    for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
      const GeomAbs_CurveType type = BRepAdaptor_Curve(TopoDS::Edge(edges.Current())).GetType();
      CHECK(type == GeomAbs_Line || type == GeomAbs_Circle);
    }
  }
  const double h = std::sqrt(3.0);
  const double removed = 0.5 * h - 2.0 * (kPi / 3.0 - std::sin(kPi / 3.0));
  CHECK_NEAR(volume(*rounded), volume(*body) - removed * 40.0);
  // The rounding's far edge (on the rib's other side, at its top) rounded
  // with 0.3, then the edges of the rib's end face with 0.4 (one tangent
  // chain from the plate up and down again).
  const ShapePtr far = fillet("F5", *rounded, {edge_at(*rounded, gp_Pnt(20, 10.5, 15))}, 0.3);
  CHECK(detail::is_valid(far->occt()));
  const ShapePtr ends = fillet("F6", *far, {edge_at(*far, gp_Pnt(0, 9.5, 10))}, 0.4);
  CHECK(detail::is_valid(ends->occt()));
  CHECK(volume(*ends) < volume(*far));
  CHECK(volume(*far) - volume(*ends) < 10.0 * 0.4 * 0.4);
}

void fillet_overflow_tests() {
  guarded("test_rounding_rolls_on_a_bevels_far_edge", test_rounding_rolls_on_a_bevels_far_edge);
  guarded("test_concave_rounding_rolls_on_a_strips_far_edge", test_concave_rounding_rolls_on_a_strips_far_edge);
  guarded("test_rounding_at_a_groove_end_is_cut_by_the_top", test_rounding_at_a_groove_end_is_cut_by_the_top);
  guarded("test_rounding_at_a_groove_end_consumes_the_wall", test_rounding_at_a_groove_end_consumes_the_wall);
  guarded("test_rounding_wider_than_a_side", test_rounding_wider_than_a_side);
  guarded("test_rounding_rolls_on_the_far_edge_of_a_rib", test_rounding_rolls_on_the_far_edge_of_a_rib);
  guarded("test_roundings_on_a_low_step", test_roundings_on_a_low_step);
  guarded("test_closed_roundings_roll_on_a_rim", test_closed_roundings_roll_on_a_rim);
  guarded("test_roundings_over_several_narrow_faces", test_roundings_over_several_narrow_faces);
  guarded("test_roundings_that_overlap_on_a_face_are_one", test_roundings_that_overlap_on_a_face_are_one);
  guarded("test_contacts_along_the_far_edge_of_a_face", test_contacts_along_the_far_edge_of_a_face);
  guarded("test_second_rounding_rolls_on_the_first_ones_contact_line",
          test_second_rounding_rolls_on_the_first_ones_contact_line);
  guarded("test_consumed_side_keeps_its_edges_names", test_consumed_side_keeps_its_edges_names);
  guarded("test_two_distance_chamfer_on_a_face_as_high_as_it_ends_on_slanted_faces",
          test_two_distance_chamfer_on_a_face_as_high_as_it_ends_on_slanted_faces);
  guarded("test_corner_where_the_end_face_is_split_in_one_plane",
          test_corner_where_the_end_face_is_split_in_one_plane);
  guarded("test_rounding_runs_into_the_faces_at_its_ends", test_rounding_runs_into_the_faces_at_its_ends);
  guarded("test_rounding_rolls_on_a_bevel_edge_round_the_corners",
          test_rounding_rolls_on_a_bevel_edge_round_the_corners);
  guarded("test_rounding_rolls_beside_a_round_corner", test_rounding_rolls_beside_a_round_corner);
  guarded("test_rounding_beside_a_round_corner_ends_inside_its_faces",
          test_rounding_beside_a_round_corner_ends_inside_its_faces);
  guarded("test_contact_runs_along_a_part_of_the_walls_top_edge",
          test_contact_runs_along_a_part_of_the_walls_top_edge);
  guarded("test_rounding_over_a_hole_is_cut_by_it", test_rounding_over_a_hole_is_cut_by_it);
  guarded("test_rim_rounding_past_a_side_is_cut_by_it", test_rim_rounding_past_a_side_is_cut_by_it);
  guarded("test_rounding_between_rod_and_pipe_ends_at_an_inclined_top",
          test_rounding_between_rod_and_pipe_ends_at_an_inclined_top);
  guarded("test_rounding_between_rod_and_pipe_ends_at_an_inclined_bottom",
          test_rounding_between_rod_and_pipe_ends_at_an_inclined_bottom);
  guarded("test_rim_rounding_over_blind_holes_is_cut_by_them", test_rim_rounding_over_blind_holes_is_cut_by_them);
  guarded("test_rim_rounding_over_notches_is_cut_by_them", test_rim_rounding_over_notches_is_cut_by_them);
  guarded("test_rounding_over_a_slot_is_cut_by_its_faces", test_rounding_over_a_slot_is_cut_by_its_faces);
  guarded("test_chamfer_round_a_ring_past_the_side_is_cut_by_it",
          test_chamfer_round_a_ring_past_the_side_is_cut_by_it);
  guarded("test_corner_rounding_beside_a_rounded_top_edge_is_cut_by_the_top",
          test_corner_rounding_beside_a_rounded_top_edge_is_cut_by_the_top);
  guarded("test_rounding_over_a_hole_with_a_rounded_rim_is_cut_by_the_rim",
          test_rounding_over_a_hole_with_a_rounded_rim_is_cut_by_the_rim);
  guarded("test_closed_walked_rounding_over_a_hole_with_a_rounded_rim_is_cut_by_the_rim",
          test_closed_walked_rounding_over_a_hole_with_a_rounded_rim_is_cut_by_the_rim);
  guarded("test_closed_rounding_consumes_an_earlier_rounding_over_a_rounded_rim",
          test_closed_rounding_consumes_an_earlier_rounding_over_a_rounded_rim);
  guarded("test_rounding_round_a_chamfered_box_walks_on_along_its_rounded_corners",
          test_rounding_round_a_chamfered_box_walks_on_along_its_rounded_corners);
  guarded("test_top_rounding_larger_than_the_rounded_corners_walks_round_them",
          test_top_rounding_larger_than_the_rounded_corners_walks_round_them);
  guarded("test_boss_top_rounded_after_its_foot_rounded_as_high_as_it",
          test_boss_top_rounded_after_its_foot_rounded_as_high_as_it);
  guarded("test_box_roundings_keep_their_tangent_edges_and_take_a_draft",
          test_box_roundings_keep_their_tangent_edges_and_take_a_draft);
  guarded("test_rounding_rolled_on_a_ribs_far_edge_is_a_cylinder",
          test_rounding_rolled_on_a_ribs_far_edge_is_a_cylinder);
}

} // namespace test
