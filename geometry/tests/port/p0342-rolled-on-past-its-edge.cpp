// SPDX-License-Identifier: MIT
// Port patch 0342: a rounding whose ball rolls on a straight edge Er of the
// face beyond a narrow top, Er ending on the faces W at the ends of the
// spine, rolls on along Er's line past its ends and is cut by W
// (RolledOnProlongedEdge, the corners by ChFi3d_RolledPastEdgeVertex).
// Without the patch the ball turned about Er's ends and rolled on along W's
// edges with the top, and the corners failed or the result was invalid.
//
// A second geometry to geometry.fillet_corners' ear on a tube: a
// rectangular ear 3 wide (x from 17 to 20) standing 5 out of a flat wall
// (the plane y = 0, the ear on y < 0), z from 5 to 20, its outer top edge
// chamfered at 45 degrees so that its top is a strip w = 1 wide along the
// wall. The top's edge with the wall (concave) rounded with r: the ball
// touches the wall (its centre r from it) and rolls on the top's edge with
// the chamfer (y = -w, z = 20), its centre at z = 20 + sqrt(r^2 - (r - w)^2).
// The blend is that cylinder from the wall's contact to Er, cut by the
// ear's sides x = 17 and x = 20, which grow up to it: built, valid, the top
// consumed (as many faces as the body), the material added the section
// between the wall, the top and the ball times the ear's width, exactly
// (r = 2 and r = 1.5). The boundary: r = 0.8, which the strip holds, as
// stock: the top stays, a quarter corner's fill times the width.

#include <cmath>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>

#include "port_test.hpp"

namespace {

constexpr double kStrip = 1.0; // the top's width
constexpr double kEarX0 = 17.0, kEarX1 = 20.0;

// The wall (x from 10 to 27, y from 0 to 4, z from 0 to 30) with the ear.
TopoDS_Shape wall_with_chamfered_ear() {
  const TopoDS_Shape         wall = BRepPrimAPI_MakeBox(gp_Pnt(10, 0, 0), gp_Pnt(27, 4, 30)).Shape();
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(kEarX0, 0, 5));
  profile.Add(gp_Pnt(kEarX0, -5, 5));
  profile.Add(gp_Pnt(kEarX0, -5, 20 - (5 - kStrip)));
  profile.Add(gp_Pnt(kEarX0, -kStrip, 20));
  profile.Add(gp_Pnt(kEarX0, 0, 20));
  profile.Close();
  const TopoDS_Shape ear =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(profile.Wire()).Face(), gp_Vec(kEarX1 - kEarX0, 0, 0)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(wall, ear).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The top's edge with the wall (y = 0, z = 20) rounded with r.
TopoDS_Shape top_rounded(const TopoDS_Shape& body, double r) {
  const std::vector<TopoDS_Edge> edges = port::edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.Y()) < 1e-9 && std::abs(m.Z() - 20.0) < 1e-9 && m.X() > kEarX0 && m.X() < kEarX1;
  });
  CHECK(edges.size() == 1);
  if (edges.size() != 1) {
    return TopoDS_Shape();
  }
  return port::rounded(body, edges, r);
}

// The section a ball of radius r rolling on the strip's far edge fills:
// between the wall's contact, the corner, the edge and the ball's arc
// (centre r from the wall, h above the top).
double rolled_section(double r, double w) {
  const double h = std::sqrt(r * r - (r - w) * (r - w));
  return 0.5 * (r + w) * h - 0.5 * r * r * std::acos((r - w) / r);
}

void check_rolled_and_cut(double r) {
  const TopoDS_Shape body    = wall_with_chamfered_ear();
  const TopoDS_Shape rounded = top_rounded(body, r);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    std::fprintf(stderr, "radius %g: no valid rounding\n", r);
    return;
  }
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE));
  CHECK_NEAR_TOL(port::volume_of(rounded) - port::volume_of(body),
                 (kEarX1 - kEarX0) * rolled_section(r, kStrip), 1e-5);
}

// r = 2: the ball's centre 30 degrees off the top's normal towards the
// chamfer (whose normal is 45 degrees off).
void test_rounding_rolled_on_past_its_edge_cut_by_the_ears_sides() {
  check_rolled_and_cut(2.0);
}

// r = 1.5: nearer to the strip's width, 19.5 degrees off.
void test_smaller_rounding_rolled_on_past_its_edge() {
  check_rolled_and_cut(1.5);
}

// The boundary: r = 0.8 touches the top within the strip; an analytic
// rounding ending on the ear's sides as stock, the top kept.
void test_rounding_the_strip_holds_is_as_stock() {
  const TopoDS_Shape body    = wall_with_chamfered_ear();
  const TopoDS_Shape rounded = top_rounded(body, 0.8);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 1);
  CHECK_NEAR_TOL(port::volume_of(rounded) - port::volume_of(body), (kEarX1 - kEarX0) * test::fillet_loss(0.8),
                 1e-5);
}

} // namespace

MITCAD_PORT_TEST("0342", test_rounding_rolled_on_past_its_edge_cut_by_the_ears_sides);
MITCAD_PORT_TEST("0342", test_smaller_rounding_rolled_on_past_its_edge);
MITCAD_PORT_TEST("0342", test_rounding_the_strip_holds_is_as_stock);
