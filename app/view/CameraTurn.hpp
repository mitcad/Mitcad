// SPDX-License-Identifier: MIT
#pragma once

// The arrow keys' turn of the view (mitcad#1211): OCCT's camera turned by
// a step about a centre (the mouse orbit's, OcctViewer::turnView), as the
// orientation cube's arrows turn it, by the step instead of a quarter turn.
// Right turns the camera round to the model's right (the face on the right
// comes into sight), Up round to the face above. The camera's distance to
// the centre, its scale (the zoom) and its projection are kept: a rotation
// about the centre moves the eye and the target together. The unit tests
// (tests/camera_turn.cpp) check the axis, the angle, the centre and the
// zoom on both projections.

#include <cmath>

#include <Graphic3d_Camera.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

namespace mitcad {

// Turns `camera` about `centre`: `right` degrees about `vertical` (the
// view's up, or Z with a constrained orbit; positive: the camera goes round
// to the right), then `up` degrees about the view's right (positive: the
// camera goes round upwards). False (the camera unchanged) for no turn or
// an angle that is no number.
inline bool turnCamera(Graphic3d_Camera& camera, const gp_Pnt& centre, const gp_Dir& vertical, double right,
                       double up) {
  if ((right == 0.0 && up == 0.0) || !std::isfinite(right) || !std::isfinite(up)) {
    return false;
  }
  constexpr double kRadians = 3.14159265358979323846 / 180.0;
  const auto apply = [&camera](const gp_Trsf& turn) {
    const gp_Pnt eye = camera.Eye().Transformed(turn);
    const gp_Pnt target = camera.Center().Transformed(turn);
    const gp_Dir upward = camera.Up().Transformed(turn);
    camera.SetEyeAndCenter(eye, target);
    camera.SetUp(upward);
    camera.OrthogonalizeUp();
  };
  if (right != 0.0) {
    // About the vertical through the centre: the camera goes round to the
    // right when it turns counterclockwise seen from above (a positive
    // angle about the vertical).
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(centre, vertical), right * kRadians);
    apply(turn);
  }
  if (up != 0.0) {
    // About the view's right through the centre (after the turn above): the
    // camera goes up when the eye turns towards the view's up.
    const gp_Vec across = gp_Vec(camera.Direction()).Crossed(gp_Vec(camera.Up()));
    if (across.Magnitude() < 1e-12) {
      return right != 0.0;
    }
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(centre, gp_Dir(across)), -up * kRadians);
    apply(turn);
  }
  return true;
}

} // namespace mitcad
