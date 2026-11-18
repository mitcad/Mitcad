// SPDX-License-Identifier: MIT
// Port patch 0173: a stripe whose walk failed after a walk rolling on an
// edge (whose end, where the surface's contact reaches an arc, 0170 solves
// with the rolling ball's equations) is computed again with OCCT's
// equations in that inverse function, and kept where that succeeds. Where
// the walk on two edges started from such an end took no step, the
// rounding failed.
//
// A bar 40 long whose top (z = 10) narrows from 6.36 at its ends to 1 in
// the middle (its back a vertical cylinder of radius 40) and whose front
// (y = 0) narrows from 6.86 to 1.5 (its bottom a cylinder of radius 40
// across it); the front-top edge, a box's edge, rounded with 2. The
// contacts lie 2 from the edge: the ball rolls first on the top's back
// edge (the top narrower than 2 over 17.8 of the middle), then on both
// that edge and the front's bottom edge (the front narrower than 2 over
// 12.6 of the middle), and back. Both edges are arcs whose direction
// differs from the guide's, so the rolling ball's end section and OCCT's
// differ. Built at the scales 1 and 10: built, valid, the same rounding
// (the removed volume in proportion). The boundary: the front whole (10
// high), so that the ball rolls on the top's edge only and no walk on two
// edges follows: built, valid at both scales, as before.

#include <cmath>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Trsf.hxx>

#include "port_test.hpp"

namespace {

// The bar at the scale: its top narrowed to `top` in the middle by a
// vertical cylinder of radius 40 behind it, its front narrowed to `front`
// by a cylinder of radius 40 along y below it (0: the front whole). The
// cylinders' seams lie outside the bar.
TopoDS_Shape narrowing_bar(double top, double front, double scale) {
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(40, 10, 10)).Shape();
  const TopoDS_Shape back =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(20, top + 40, -1), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), 40, 12).Shape();
  body = BRepAlgoAPI_Cut(body, back).Shape();
  if (front > 0.0) {
    const TopoDS_Shape bottom =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(20, -1, 10 - front - 40), gp_Dir(0, 1, 0), gp_Dir(0, 0, -1)), 40, 12)
        .Shape();
    body = BRepAlgoAPI_Cut(body, bottom).Shape();
  }
  gp_Trsf scaling;
  scaling.SetScale(gp_Pnt(0, 0, 0), scale);
  return BRepBuilderAPI_Transform(body, scaling, true).Shape();
}

// The bar's front-top edge (y = 0, z = 10 at the scale) rounded with 2 at
// the scale; a null shape when that fails.
TopoDS_Shape front_top_rounded(const TopoDS_Shape& bar, double scale) {
  const std::vector<TopoDS_Edge> edge = port::edges_where(bar, [scale](const gp_Pnt& m) {
    return std::abs(m.Y()) < 1e-7 * scale && std::abs(m.Z() - 10.0 * scale) < 1e-7 * scale;
  });
  CHECK(edge.size() == 1);
  return edge.size() == 1 ? port::rounded(bar, edge, 2.0 * scale) : TopoDS_Shape();
}

// The volume the rounding removes from the bar at the scale, divided by the
// scale's cube; -1 where the rounding fails or is invalid.
double removed_at_scale(double top, double front, double scale, int& faces) {
  const TopoDS_Shape bar = narrowing_bar(top, front, scale);
  CHECK(port::valid(bar));
  const TopoDS_Shape result = front_top_rounded(bar, scale);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return -1.0;
  }
  faces = port::count_of(result, TopAbs_FACE);
  return (port::volume_of(bar) - port::volume_of(result)) / (scale * scale * scale);
}

// The walk on two edges from the end of the walk rolling on the top's edge,
// at both scales: built, valid, the same rounding.
void test_box_edge_rolled_on_two_edges_at_two_scales() {
  int          faces1 = 0, faces10 = 0;
  const double removed1  = removed_at_scale(1.0, 1.5, 1.0, faces1);
  const double removed10 = removed_at_scale(1.0, 1.5, 10.0, faces10);
  CHECK(removed1 > 0.0 && removed10 > 0.0);
  if (removed1 <= 0.0 || removed10 <= 0.0) {
    return;
  }
  // Less than a rounding of 2 along the whole 40 takes where both faces
  // are wide.
  CHECK(removed1 < test::fillet_loss(2.0) * 40.0);
  CHECK_NEAR_TOL(removed10, removed1, 1e-4);
  CHECK(faces10 == faces1);
}

// The boundary: the front whole, the ball rolls on the top's edge only.
void test_box_edge_rolled_on_one_edge_at_two_scales() {
  int          faces1 = 0, faces10 = 0;
  const double removed1  = removed_at_scale(1.0, 0.0, 1.0, faces1);
  const double removed10 = removed_at_scale(1.0, 0.0, 10.0, faces10);
  CHECK(removed1 > 0.0 && removed10 > 0.0);
  if (removed1 <= 0.0 || removed10 <= 0.0) {
    return;
  }
  CHECK(removed1 < test::fillet_loss(2.0) * 40.0);
  CHECK_NEAR_TOL(removed10, removed1, 1e-4);
  CHECK(faces10 == faces1);
}

} // namespace

MITCAD_PORT_TEST("0173", test_box_edge_rolled_on_two_edges_at_two_scales);
MITCAD_PORT_TEST("0173", test_box_edge_rolled_on_one_edge_at_two_scales);
