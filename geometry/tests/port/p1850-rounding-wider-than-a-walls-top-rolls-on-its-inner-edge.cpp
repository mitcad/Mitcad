// SPDX-License-Identifier: MIT
// Port patch 1850: a rounding of a wall's outer top edge with a radius
// wider than the wall's top takes the top away, its ball rolling on the
// inner wall's top edge (StartSolOverflow), and where the top is closed
// at the edge's ends by sloping faces (the ends' top edges chamfered) the
// blend rolls on past the spine's ends until the slopes cut it
// (RolledOnProlongedEdgeAtEnd, the caps in PerformOneCorner at vertices
// whose edges are all convex).
//
// The body: a plate 30 long (x), 10 wide (y), its top (the floor) at
// z = zf, and along its outer side a wall 2 wide (y from 0 to 2) standing
// to z = 10, the inner wall's height h = 10 - zf above the floor. The
// wall's top ends at x = 3 and x = 27 in slopes of 45 degrees down to
// z = 7 at the ends x = 0 and x = 30 (the top's end edges chamfered with
// 3). The outer top edge (y = 0, z = 10, x from 3 to 27) rounded with
// r = 2.5.
//
// The ball touching the outer wall (y = 0) and the top (z = 10) touches
// the top at y = 2.5, past its inner edge (y = 2): StartSolOverflow takes
// that side over the inner top edge onto the inner wall (y = 2), which
// the ball touches at no point (parallel to the outer wall, 2 apart, the
// ball 5 across), and then over the inner wall's foot onto the floor, the
// crossing h from the inner top edge, while h <= 2r = 5. Before the
// patch, with h = 4, the ball touching the outer wall and the floor was
// taken: the whole wall above the floor went, a valid body that was
// wrong (the points in the wall below the arc are then outside). With
// the patch that side rolls on the inner top edge (the inner wall at least
// r high, h = 4 >= 2.5, across an edge where the solid turns away by a
// right angle): the ball's centre at y = r = 2.5, z = 10 - h' with
// h' = sqrt(r^2 - (r - 2)^2) = sqrt(6), the blend from the outer wall's
// contact at z = 10 - h' = 7.55 to the inner top edge. Its section takes
// away (2 + r) / 2 * h' - r^2 / 2 * acos((r - 2) / r) = 1.2320 of the wall
// across. At the ends the outer contact (z = 7.55) lies inside the outer
// wall, on no edge, off the top's and the slope's planes: the blend rolls
// on past the spine's ends along the inner top edge's line and is cut by
// the slopes, at x = 0.55 and 29.45 on the outer wall, inside the slopes
// (z from 7 to 10); before the patch such an end was not rolled on.
//
// Checked: valid; in the middle (x from 14 to 16, far from the ends' caps)
// the wall below the arc kept and above it gone (points either side of
// the arc), and the volume there the slab's less the section above,
// exactly. The boundary of the start's condition: h = 6 > 2r, where the
// side over the inner wall rolled on its top edge before the patch too
// (the floor beyond the ball's reach); the same rounding.

#include <cmath>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>

#include "port_test.hpp"

namespace {

constexpr double kLength = 30.0; // the plate and wall along x
constexpr double kWidth  = 10.0; // the plate across (y)
constexpr double kTop    = 2.0;  // the wall's top across (y)
constexpr double kHeight = 10.0; // the wall's top (z)
constexpr double kSlope  = 3.0;  // the top's end edges chamfered with 3
constexpr double kRadius = 2.5;

// The plate with its top at z = floor_z and the wall along y from 0 to 2,
// its top sloping down at the ends.
TopoDS_Shape walled_plate(double floor_z) {
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(0, 0, 0));
  profile.Add(gp_Pnt(kLength, 0, 0));
  profile.Add(gp_Pnt(kLength, 0, kHeight - kSlope));
  profile.Add(gp_Pnt(kLength - kSlope, 0, kHeight));
  profile.Add(gp_Pnt(kSlope, 0, kHeight));
  profile.Add(gp_Pnt(0, 0, kHeight - kSlope));
  profile.Close();
  const TopoDS_Shape wall =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(profile.Wire()).Face(), gp_Vec(0, kTop, 0)).Shape();
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(kLength, kWidth, floor_z)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(wall, plate).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The outer top edge (y = 0, z = 10) rounded with r.
TopoDS_Shape outer_top_rounded(const TopoDS_Shape& body, double r) {
  const std::vector<TopoDS_Edge> edges = port::edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.Y()) < 1e-9 && std::abs(m.Z() - kHeight) < 1e-9;
  });
  CHECK(edges.size() == 1);
  if (edges.size() != 1) {
    return TopoDS_Shape();
  }
  return port::rounded(body, edges, r);
}

// Whether p lies inside a solid of the shape.
bool inside(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer s(shape, TopAbs_SOLID); s.More(); s.Next()) {
    BRepClass3d_SolidClassifier classifier(s.Current(), p, 1e-7);
    if (classifier.State() == TopAbs_IN) {
      return true;
    }
  }
  return false;
}

// The section a ball of radius r, touching the outer wall and rolling on
// the inner edge of a top w wide, takes away: between the wall's contact,
// the corner, the inner edge and the ball's arc.
double rolled_section(double r, double w) {
  const double h = std::sqrt(r * r - (r - w) * (r - w));
  return 0.5 * (r + w) * h - 0.5 * r * r * std::acos((r - w) / r);
}

// The rounding rolled on the inner top edge, on a plate whose floor lies
// at z = floor_z.
void check_rolled_on_the_inner_edge(double floor_z) {
  const TopoDS_Shape body    = walled_plate(floor_z);
  const TopoDS_Shape rounded = outer_top_rounded(body, kRadius);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    std::fprintf(stderr, "floor at %g: no valid rounding\n", floor_z);
    return;
  }
  // The arc at y: z = 10 - h' + sqrt(r^2 - (y - r)^2), h' = sqrt(6):
  // 9.5505 at y = 1, 9.9774 at y = 1.9, 8.7379 at y = 0.3.
  const double x = 0.5 * kLength;
  CHECK(inside(rounded, gp_Pnt(x, 1.0, 7.0)));  // the wall above the floor, kept
  CHECK(inside(rounded, gp_Pnt(x, 1.0, 9.3)));  // under the arc
  CHECK(inside(rounded, gp_Pnt(x, 1.9, 9.8)));  // under the arc by the inner edge
  CHECK(!inside(rounded, gp_Pnt(x, 0.3, 9.0))); // above the arc by the outer wall
  CHECK(!inside(rounded, gp_Pnt(x, 1.0, 9.8))); // above the arc
  // The slab x from 14 to 16: the profile (the wall 2 by 10 and the plate
  // beyond it 8 by floor_z) less the section, times 2.
  const TopoDS_Shape slab =
    BRepPrimAPI_MakeBox(gp_Pnt(x - 1.0, -1.0, -1.0), gp_Pnt(x + 1.0, kWidth + 1.0, kHeight + 1.0)).Shape();
  const double expect = 2.0 * (kTop * kHeight + (kWidth - kTop) * floor_z - rolled_section(kRadius, kTop));
  CHECK_NEAR_TOL(port::volume_of(BRepAlgoAPI_Common(rounded, slab).Shape()), expect, 1e-6);
}

// The inner wall 4 high (between r and 2r): before the patch the side
// over it went on onto the floor and the wall was taken away.
void test_rounding_wider_than_the_top_rolls_on_its_inner_edge() {
  check_rolled_on_the_inner_edge(kHeight - 4.0);
}

// The boundary: the inner wall 6 high, past the ball's diameter, where the
// side over it rolled on the inner top edge before the patch too.
void test_rounding_wider_than_the_top_beside_a_high_inner_wall() {
  check_rolled_on_the_inner_edge(kHeight - 6.0);
}

} // namespace

MITCAD_PORT_TEST("1850", test_rounding_wider_than_the_top_rolls_on_its_inner_edge);
MITCAD_PORT_TEST("1850", test_rounding_wider_than_the_top_beside_a_high_inner_wall);
