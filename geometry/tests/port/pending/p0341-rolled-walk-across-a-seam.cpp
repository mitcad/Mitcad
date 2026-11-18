// SPDX-License-Identifier: MIT
// Port patch 0341: a rounding rolling on an edge Er of its face B from one
// OnSame end of the spine to the other, whose walk past an end stopped on
// the seam of its other face N before the ball came tangent to B, is walked
// again from its middle with N's surface unbounded. Without the patch the
// stripe ends at the spine's end on the seam (0011) and 0340's corner, cut
// by B from the point of tangency, has no point to start from: the fillet
// fails.
//
// A rod of radius 4.5 along x with a ridge on its top from x = 15 on (a
// cylinder of radius 2.5 about the rod's top line), whose end face, a lens
// 2.5 high, is rounded with 5 at its edge with the rod (0340's case,
// geometry.fillet_corners has the seam at 45 degrees on one side). The ball
// touches the rod and rolls on the lens's edge with the ridge's side; at
// the spine's ends (the lens's tips, 32.3 degrees from the top) its contact
// on the rod lies at the tips, and past them it rolls on until it comes
// tangent to the ridge's side, its contact on the rod 50.6 degrees from the
// top. The rod's seam at 40 and 45 degrees on one side and at 45 degrees on
// the other (past the spine's other end) lies between the two and is
// crossed: built, valid, as many faces as the body (the lens consumed), the
// material added the same as with the seam underneath. The boundary: the
// seam underneath and at 90 degrees, beyond the point of tangency, is not
// crossed: built as 0340 alone builds it.

#include <cmath>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

// The rod with the ridge, the rod's seam along `seam` (a direction across
// the axis, at `degrees` from the top towards +y).
TopoDS_Shape rod_with_ridge(double degrees) {
  const double       a   = degrees * test::kPi / 180.0;
  const gp_Dir       seam(0.0, std::sin(a), std::cos(a));
  const TopoDS_Shape rod = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), seam), 4.5, 30).Shape();
  const TopoDS_Shape ridge =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(15, 0, 4.5), gp_Dir(1, 0, 0), gp_Dir(0, 0, -1)), 2.5, 15).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(rod, ridge).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The lens's edge with the rod (a circle of the rod at x = 15) rounded
// with 5; a null shape when it fails.
TopoDS_Shape lens_rounded(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> edges;
  for (const TopoDS_Edge& edge : port::edges_where(body, [](const gp_Pnt& m) {
         return std::abs(m.X() - 15.0) < 1e-9 && std::hypot(m.Y(), m.Z()) < 4.5 + 1e-6;
       })) {
    if (BRepAdaptor_Curve(edge).GetType() == GeomAbs_Circle) {
      edges.push_back(edge);
    }
  }
  CHECK(edges.size() == 1);
  if (edges.size() != 1) {
    return TopoDS_Shape();
  }
  return port::rounded(body, edges, 5.0);
}

// The material the rounding adds to the body with the seam at `degrees`;
// a negative value when the rounding fails or is invalid.
double added_with_seam_at(double degrees) {
  const TopoDS_Shape body    = rod_with_ridge(degrees);
  const TopoDS_Shape rounded = lens_rounded(body);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    std::fprintf(stderr, "seam at %g degrees: no valid rounding\n", degrees);
    return -1.0;
  }
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE));
  return port::volume_of(BRepAlgoAPI_Cut(rounded, body).Shape());
}

// The seam underneath: never crossed (0340 alone).
double added_as_0340() {
  return added_with_seam_at(180.0);
}

// The seam at 45 degrees on the +y side, past one end of the spine.
void test_walk_across_a_seam_past_one_end() {
  const double plain = added_as_0340();
  const double added = added_with_seam_at(45.0);
  CHECK(plain > 1.0 && added > 1.0);
  CHECK_NEAR_TOL(added, plain, 1e-5);
}

// The seam at 45 degrees on the -y side: crossed past the spine's other
// end, the walk across it the other way round the rod.
void test_walk_across_a_seam_past_the_other_end() {
  const double plain = added_as_0340();
  const double added = added_with_seam_at(-45.0);
  CHECK(plain > 1.0 && added > 1.0);
  CHECK_NEAR_TOL(added, plain, 1e-5);
}

// The seam at 40 degrees, nearer to the lens's tip (32.3 degrees).
void test_walk_across_a_seam_near_the_tip() {
  const double plain = added_as_0340();
  const double added = added_with_seam_at(40.0);
  CHECK(plain > 1.0 && added > 1.0);
  CHECK_NEAR_TOL(added, plain, 1e-5);
}

// The boundary: the seam at 90 degrees, beyond the point where the ball
// comes tangent to the ridge's side (50.6 degrees): the walk never reaches
// it, built as with the seam underneath.
void test_seam_beyond_the_tangency_is_not_crossed() {
  const double plain = added_as_0340();
  const double added = added_with_seam_at(90.0);
  CHECK(plain > 1.0 && added > 1.0);
  CHECK_NEAR_TOL(added, plain, 1e-5);
}

} // namespace

MITCAD_PORT_TEST("0341", test_walk_across_a_seam_past_one_end);
MITCAD_PORT_TEST("0341", test_walk_across_a_seam_past_the_other_end);
MITCAD_PORT_TEST("0341", test_walk_across_a_seam_near_the_tip);
MITCAD_PORT_TEST("0341", test_seam_beyond_the_tangency_is_not_crossed);
