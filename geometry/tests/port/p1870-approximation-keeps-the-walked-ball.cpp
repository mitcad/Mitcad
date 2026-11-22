// SPDX-License-Identifier: MIT
// Port patch 1870: BRepBlend_AppFuncRoot::SearchPoint, the sections the
// approximation of a walked surface/restriction blend solves between the
// walk's points, kept on the walked ball (mitcad#2161).
//
// The body: a plate with a round step 0.2 high on it, the step's foot
// rounded with 0.4 (higher than the step): the ball touches the plate and
// rolls on the step's top rim all round (patch 0011's overflow, walked as
// a blend between the plate and the rim), its centre 0.4 above the plate
// and c = sqrt(0.4^2 - 0.2^2) = 0.346 outside the side. Each section has a
// second solution, the ball on the rim's inner side, its plate contact 2c
// nearer the axis.
//
// Why it failed: the walk keeps to its branch with steps of 9.6 degrees
// (its deflection allowance is a thousandth of the guide's length, the
// same angles at any radius). The approximation solves its sections
// between the walk's points from the linear interpolation of the two
// next to them; on the plate that start lies on the chord, inside the
// contact circle by its sagitta, (R + c) theta^2 / 8 = 0.35 at the step's
// radius 100: past the middle between the two balls. The solver went over
// to the inner ball, its sections were taken into the line, and the
// approximation joined the two branches only to 0.33 mm. Its normals all
// lay below that tolerance, so the blend's orientation was not found and
// the fillet failed. The step's radius 50 (sagitta 0.17) built. With the
// patch such a section is solved again from the contact the walk's
// tangents give (a cubic Hermite interpolation, off by the fourth power of
// the step), which lies on the walked ball. The check: built, valid, the
// blend within the approximation's tolerance (1e-4), the exact volume of
// the rolling ball (the section under its arc revolved) and the step's
// side gone.

#include <cmath>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

// The material a rounding of radius r adds round the foot of a round step
// of radius `radius` and height h (r > h): the section between the plate,
// the step's side and the ball's arc, from the plate contact c outside the
// side up to the rim, revolved.
double rolled_fill(double radius, double h, double r) {
  const double c   = std::sqrt(r * r - (r - h) * (r - h));
  double       sum = 0.0;
  const int    n   = 20000;
  for (int i = 0; i < n; ++i) {
    const double u = c * (i + 0.5) / n;
    sum += 2.0 * test::kPi * (radius + c - u) * (r - std::sqrt(r * r - u * u)) * (c / n);
  }
  return sum;
}

// A plate 5 thick with a round step of `radius` and height h on it, its
// foot rounded with r: one valid solid within the approximation's
// tolerance, the rolling ball's volume, the step's side gone.
void check_rolled_foot(double radius, double h, double r) {
  const double       half    = radius + 10.0;
  const TopoDS_Shape plate   = BRepPrimAPI_MakeBox(gp_Pnt(-half, -half, 0), 2 * half, 2 * half, 5).Shape();
  const TopoDS_Shape disc    = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5), gp::DZ()), radius, h).Shape();
  const TopoDS_Shape stepped = BRepAlgoAPI_Fuse(plate, disc).Shape();
  const std::vector<TopoDS_Edge> foot = port::edges_where(stepped, [radius](const gp_Pnt& m) {
    return std::abs(m.Z() - 5.0) < 1e-6 && std::abs(std::hypot(m.X(), m.Y()) - radius) < 1e-6;
  });
  CHECK(foot.size() == 1);
  if (foot.size() != 1) {
    return;
  }
  const TopoDS_Shape filled = port::rounded(stepped, foot, r);
  CHECK(port::valid(filled));
  if (!port::valid(filled)) {
    return;
  }
  CHECK(port::count_of(filled, TopAbs_SOLID) == 1);
  CHECK(port::max_tolerance(filled) <= 1e-4);
  CHECK_NEAR_TOL(port::volume_of(filled) - port::volume_of(stepped), rolled_fill(radius, h, r), 1e-5);
  CHECK(port::count_of(filled, TopAbs_FACE) == port::count_of(stepped, TopAbs_FACE));
}

// The step's radius 100: the linear start lies past the middle between the
// two balls.
void test_foot_of_a_wide_low_step_rolled_on_its_rim() { check_rolled_foot(100.0, 0.2, 0.4); }

// The step's radius 200, and with a rounding of 1 (c = 0.6, sagitta 0.70).
void test_foot_of_a_wider_low_step_rolled_on_its_rim() {
  check_rolled_foot(200.0, 0.2, 0.4);
  check_rolled_foot(200.0, 0.2, 1.0);
}

// The boundary: the step's radius 50, whose linear starts stay by the
// walked ball (sagitta 0.17 < c); built with and without the patch.
void test_foot_of_a_narrower_low_step_as_before() { check_rolled_foot(50.0, 0.2, 0.4); }

} // namespace

MITCAD_PORT_TEST("1870", test_foot_of_a_wide_low_step_rolled_on_its_rim);
MITCAD_PORT_TEST("1870", test_foot_of_a_wider_low_step_rolled_on_its_rim);
MITCAD_PORT_TEST("1870", test_foot_of_a_narrower_low_step_as_before);
