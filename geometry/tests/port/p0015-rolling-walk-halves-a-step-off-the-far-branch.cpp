// SPDX-License-Identifier: MIT
// Port patch 0015: BRepBlend_SurfRstLineBuilder::InternalPerform, the walk
// of a blend between a face and an edge it rolls on (surface/restriction),
// ended at once (Blend_SamePoints) at a section whose contacts lay outside
// their domains and could not be reframed onto an arc. A step that is too
// large lands such sections on the other branch of the solution, the ball
// on the far side of the edge; the step is now halved there while it is
// larger than twice the guide's tolerance.
//
// The body: a round step 0.5 high on a plate, its foot rounded with 0.6
// (higher than the step): the ball touches the plate and rolls on the
// step's top rim all round (the overflow of patch 0011), its centre 0.6
// above the plate and 0.6 from the rim, c = sqrt(0.35) = 0.5916 outside
// the step. The other branch, the ball's centre c inside the rim, puts its
// plate contact under the step, off the plate's top face.
//
// Why a step lands there: the guide is the foot circle, its maximal step
// 5 % of its length (18 degrees). The deflection test (the builder's 1e-3
// of the guide's length against the arc's sagitta, the same angles on any
// radius) takes steps up to 12.8 degrees as they are and grows those under
// 6.4 by half. The first step (18, then 9, the walk's first point) is
// halved by the walk itself wherever it lands outside; the steps after it
// run at 4.5 to 9 degrees. The solver starts each step from the previous
// section's solution; carried into the new section's plane, the ball's
// centre lands at cos(step) of its distance from the axis. With the step's
// radius 100 the centre lies at 100.59 and the far branch's at 99.41: after
// a first step of 4.5 degrees (too small, grown) the second, 6.75, starts
// at 99.89, nearer the far branch, and the solver goes there. Its plate
// contact cannot be reframed onto the plate's edge (a ball touching the
// plate at the foot circle lies 0.1 from the rim, not 0.6), nor its rim
// contact (on the closed rim all along): the walk ended there and the
// closed rounding was not built. Halved (3.4 degrees: 100.41) the start
// lies near the right centre. The check: built, valid, the exact volume
// (the section under the ball's arc revolved) and the step's side gone (as
// many faces as before).
//
// The boundary: the step's radius 8.5 (the body of test_fillet_overflow
// .cpp's test_closed_roundings_roll_on_a_rim): the centre at 9.09, the far
// branch at 7.91; a 6.75 degree step starts at 9.03 and a 9 degree one at
// 8.98, both by the right one. The walk never comes to the branch and
// builds with and without the patch.

#include <cmath>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kStep  = 0.5; // the step's height
constexpr double kRound = 0.6; // the rounding's radius

// The material the rounding adds round a step of radius `radius`: the
// section between the plate, the step's side and the ball's arc (from the
// plate contact c outside the side up to the rim) revolved.
double fill(double radius) {
  const double c   = std::sqrt(kRound * kRound - (kRound - kStep) * (kRound - kStep));
  double       sum = 0.0;
  const int    n   = 20000;
  for (int i = 0; i < n; ++i) {
    const double u = c * (i + 0.5) / n;
    sum += 2.0 * test::kPi * (radius + c - u) * (kRound - std::sqrt(kRound * kRound - u * u)) * (c / n);
  }
  return sum;
}

// A plate 5 thick with a round step of `radius` on it, its foot rounded:
// built, valid, the exact volume, the step's side gone.
void check_rounded_foot(double radius) {
  const double       half    = radius + 10.0;
  const TopoDS_Shape plate   = BRepPrimAPI_MakeBox(gp_Pnt(-half, -half, 0), 2 * half, 2 * half, 5).Shape();
  const TopoDS_Shape disc    = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5), gp::DZ()), radius, kStep).Shape();
  const TopoDS_Shape stepped = BRepAlgoAPI_Fuse(plate, disc).Shape();
  const std::vector<TopoDS_Edge> foot = port::edges_where(stepped, [radius](const gp_Pnt& m) {
    return std::abs(m.Z() - 5.0) < 1e-6 && std::abs(std::hypot(m.X(), m.Y()) - radius) < 1e-6;
  });
  CHECK(foot.size() == 1);
  if (foot.size() != 1) {
    return;
  }
  const TopoDS_Shape filled = port::rounded(stepped, foot, kRound);
  CHECK(port::valid(filled));
  if (!port::valid(filled)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(filled), port::volume_of(stepped) + fill(radius), 1e-7);
  CHECK(port::count_of(filled, TopAbs_FACE) == port::count_of(stepped, TopAbs_FACE));
}

// The step's radius 100: the walk's second step lands on the far branch.
void test_rounding_rolls_round_a_wide_steps_rim() { check_rounded_foot(100.0); }

// The boundary: the step's radius 8.5, whose steps keep to the right branch.
void test_rounding_rolls_round_a_narrow_steps_rim() { check_rounded_foot(8.5); }

} // namespace

MITCAD_PORT_TEST("0015", test_rounding_rolls_round_a_wide_steps_rim);
MITCAD_PORT_TEST("0015", test_rounding_rolls_round_a_narrow_steps_rim);
