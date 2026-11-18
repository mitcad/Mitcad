// SPDX-License-Identifier: MIT
// Port patch 1523: a ball that stays put between tubes of its radius within
// a tolerance is walked as one between exact tubes. 1520 takes a ball for
// staying put only where the centre seen from a contact moves at less than
// a thousandth of the contact's speed; 1523 also takes a support that is a
// tube of the ball's radius within StillTolerance, min(3.2e-3, R / 200):
// the centres of the ball seen from its two contacts may lie that far apart.
//
// The case, synthetic: a box's two top edges at a corner rounded first, one
// with R = 2, the other with R + d; then the vertical edge below them and
// the mitre where the two roundings meet, one tangent chain, rounded with R.
// Over the mitre the ball stays put; seen from its contact on the tube of
// radius R + d its centre lies up to d from the one seen from the exact
// tube (and moves at d / (R + d) of the contact's speed, 1.5e-3 for
// d = 3e-3, past 1520's thousandth). Inside: d = 3.0e-3, the rounding
// builds, valid, taking the material of the exact mitre's within 0.05 mm3
// (without the patch it fails, as before 1520). Outside: d = 3.4e-3, past
// 3.2e-3: the ball is not taken for staying put and the rounding fails.

#include <cmath>
#include <vector>

#include <BRepPrimAPI_MakeBox.hxx>

#include "port_test.hpp"

namespace {

constexpr double kPi     = 3.14159265358979323846;
constexpr double kLength = 30.0;
constexpr double kWidth  = 30.0;
constexpr double kHeight = 20.0;
constexpr double kRadius = 2.0;

// The box with its top edge along x at y = 0 rounded with kRadius and its
// top edge along y at x = 0 with kRadius + d; null if that fails.
TopoDS_Shape mitred(double d) {
  const TopoDS_Shape       box = BRepPrimAPI_MakeBox(kLength, kWidth, kHeight).Shape();
  BRepFilletAPI_MakeFillet fillet(box);
  const std::vector<TopoDS_Edge> front = port::edges_where(
    box, [](const gp_Pnt& m) { return std::abs(m.Y()) < 1e-9 && std::abs(m.Z() - kHeight) < 1e-9; });
  const std::vector<TopoDS_Edge> left = port::edges_where(
    box, [](const gp_Pnt& m) { return std::abs(m.X()) < 1e-9 && std::abs(m.Z() - kHeight) < 1e-9; });
  CHECK(front.size() == 1 && left.size() == 1);
  if (front.size() != 1 || left.size() != 1) {
    return TopoDS_Shape();
  }
  fillet.Add(kRadius, front[0]);
  fillet.Add(kRadius + d, left[0]);
  return port::built(fillet, "mitre");
}

// The vertical edge at x = 0, y = 0 and the mitre above it rounded with
// kRadius (the chain follows from the vertical edge); null if that fails.
TopoDS_Shape rounded_through_the_mitre(const TopoDS_Shape& body) {
  const std::vector<TopoDS_Edge> vertical = port::edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.X()) < 1e-9 && std::abs(m.Y()) < 1e-9 && m.Z() < kHeight - 3.0;
  });
  CHECK(vertical.size() == 1);
  if (vertical.size() != 1) {
    return TopoDS_Shape();
  }
  return port::rounded(body, vertical, kRadius);
}

// The material the rounding takes at an exact mitre (both earlier roundings
// of radius R): the vertical edge's quarter round below the corner, and at
// the corner the mitre's (2/3 R^3) less the ball's eighth (pi/6 R^3).
double exact_mitre_removal() {
  const double r3 = kRadius * kRadius * kRadius;
  return (kHeight - kRadius) * kRadius * kRadius * (1.0 - kPi / 4.0) + r3 * (2.0 / 3.0 - kPi / 6.0);
}

// Inside: one support a tube of radius R + 3.0e-3, the centres seen from the
// two contacts up to 3.0e-3 apart, within StillTolerance (3.2e-3).
void test_ball_staying_put_on_a_tube_within_the_tolerance() {
  const TopoDS_Shape body = mitred(3.0e-3);
  CHECK(port::valid(body));
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape result = rounded_through_the_mitre(body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  const double removed = port::volume_of(body) - port::volume_of(result);
  if (std::abs(removed - exact_mitre_removal()) > 0.05) {
    std::fprintf(stderr, "still ball within the tolerance: removed %.6f, exact mitre %.6f\n", removed,
                 exact_mitre_removal());
  }
  CHECK(std::abs(removed - exact_mitre_removal()) <= 0.05);
}

// Outside: the tube of radius R + 3.4e-3, the centres up to 3.4e-3 apart,
// past StillTolerance: the ball is not taken for staying put there and the
// walk over the mitre fails, as on a tube of another radius.
void test_ball_on_a_tube_beyond_the_tolerance_is_not_still() {
  const TopoDS_Shape body = mitred(3.4e-3);
  CHECK(port::valid(body));
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape result = rounded_through_the_mitre(body);
  if (!result.IsNull()) {
    std::fprintf(stderr, "still ball beyond the tolerance: built (valid %d)\n", port::valid(result) ? 1 : 0);
  }
  CHECK(result.IsNull() || !port::valid(result));
}

} // namespace

MITCAD_PORT_TEST("1523", test_ball_staying_put_on_a_tube_within_the_tolerance);
MITCAD_PORT_TEST("1523", test_ball_on_a_tube_beyond_the_tolerance_is_not_still);
