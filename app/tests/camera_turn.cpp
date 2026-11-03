// SPDX-License-Identifier: MIT
// The arrow keys' turn of the view (mitcad#1211, view/CameraTurn.hpp): a
// step about the view's up (Left, Right) or right (Up, Down) through the
// mouse orbit's centre, on OCCT's cameras of both projections: the axis,
// the angle and the way it turns, the centre kept where it is, the zoom
// (scale, distance) unchanged, and back again with the opposite key.
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>
#include <limits>

#include <Graphic3d_Camera.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "view/CameraTurn.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "camera_turn.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)

constexpr double kDegree = 3.14159265358979323846 / 180.0;

bool nearly(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(1.0, std::abs(b)); }
bool same(const gp_Pnt& a, const gp_Pnt& b) { return a.Distance(b) <= 1e-9 * std::max(1.0, b.XYZ().Modulus()); }
bool same(const gp_Dir& a, const gp_Dir& b) { return a.Angle(b) <= 1e-9; }

// A camera as the 3D view has one: looking along +Y at the origin from 400
// away, +Z up, a view wider than tall.
occ::handle<Graphic3d_Camera> viewCamera(bool perspective) {
  occ::handle<Graphic3d_Camera> camera = new Graphic3d_Camera();
  camera->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                        : Graphic3d_Camera::Projection_Orthographic);
  camera->SetAspect(1.6);
  camera->SetEyeAndCenter(gp_Pnt(0.0, -400.0, 0.0), gp_Pnt(0.0, 0.0, 0.0));
  camera->SetUp(gp_Dir(0.0, 0.0, 1.0));
  camera->OrthogonalizeUp();
  camera->SetScale(120.0);
  return camera;
}

gp_Pnt turned(const gp_Pnt& point, const gp_Ax1& axis, double degrees) {
  gp_Trsf turn;
  turn.SetRotation(axis, degrees * kDegree);
  return point.Transformed(turn);
}

void turnTests() {
  const gp_Pnt centre(10.0, 20.0, 5.0); // the orbit's centre, off the target
  const gp_Dir z(0.0, 0.0, 1.0);
  for (const bool perspective : {false, true}) {
    // Right: about the view's up (Z here) through the centre, the camera
    // round to the right: it moves towards +X and looks a little to -X.
    {
      const occ::handle<Graphic3d_Camera> camera = viewCamera(perspective);
      const double scale = camera->Scale();
      const gp_Pnt eye = camera->Eye();
      const gp_Pnt target = camera->Center();
      const double distance = camera->Distance();
      CHECK(mitcad::turnCamera(*camera, centre, camera->Up(), 15.0, 0.0));
      const gp_Ax1 axis(centre, z);
      CHECK(same(camera->Eye(), turned(eye, axis, 15.0)));
      CHECK(same(camera->Center(), turned(target, axis, 15.0)));
      CHECK(same(camera->Direction(), gp_Dir(-std::sin(15.0 * kDegree), std::cos(15.0 * kDegree), 0.0)));
      CHECK(same(camera->Up(), z));
      CHECK(camera->Eye().X() > 0.0);
      // The centre keeps its distances: the turn is about it.
      CHECK(nearly(camera->Eye().Distance(centre), eye.Distance(centre)));
      CHECK(nearly(camera->Center().Distance(centre), target.Distance(centre)));
      // The zoom and the projection are kept.
      CHECK(nearly(camera->Scale(), scale));
      CHECK(nearly(camera->Distance(), distance));
      CHECK(camera->IsOrthographic() == !perspective);
      // Left by the same step turns it back.
      CHECK(mitcad::turnCamera(*camera, centre, camera->Up(), -15.0, 0.0));
      CHECK(same(camera->Eye(), eye));
      CHECK(same(camera->Center(), target));
      CHECK(same(camera->Direction(), gp_Dir(0.0, 1.0, 0.0)));
    }
    // Up: about the view's right (+X here) through the centre, the camera
    // round upwards: it rises and looks down by the step, its up leaning
    // back with it.
    {
      const occ::handle<Graphic3d_Camera> camera = viewCamera(perspective);
      const double scale = camera->Scale();
      const gp_Pnt eye = camera->Eye();
      CHECK(mitcad::turnCamera(*camera, centre, camera->Up(), 0.0, 15.0));
      const gp_Ax1 axis(centre, gp_Dir(1.0, 0.0, 0.0));
      CHECK(same(camera->Eye(), turned(eye, axis, -15.0)));
      CHECK(camera->Eye().Z() > eye.Z());
      CHECK(same(camera->Direction(), gp_Dir(0.0, std::cos(15.0 * kDegree), -std::sin(15.0 * kDegree))));
      CHECK(same(camera->Up(), gp_Dir(0.0, std::sin(15.0 * kDegree), std::cos(15.0 * kDegree))));
      CHECK(nearly(camera->Eye().Distance(centre), eye.Distance(centre)));
      CHECK(nearly(camera->Scale(), scale));
      // Down by the same step turns it back.
      CHECK(mitcad::turnCamera(*camera, centre, camera->Up(), 0.0, -15.0));
      CHECK(same(camera->Eye(), eye));
      CHECK(same(camera->Up(), z));
    }
    // 24 steps of 15 degrees go once round.
    {
      const occ::handle<Graphic3d_Camera> camera = viewCamera(perspective);
      const gp_Pnt eye = camera->Eye();
      for (int i = 0; i < 24; ++i) {
        mitcad::turnCamera(*camera, centre, camera->Up(), 15.0, 0.0);
      }
      CHECK(camera->Eye().Distance(eye) <= 1e-6);
    }
  }
}

void verticalTests() {
  // A constrained orbit turns about Z: a camera looking down at an angle
  // keeps its right level (Z stays up), as the view's up would tilt it.
  const occ::handle<Graphic3d_Camera> camera = viewCamera(false);
  camera->SetEyeAndCenter(gp_Pnt(0.0, -300.0, 300.0), gp_Pnt(0.0, 0.0, 0.0));
  camera->SetUp(gp_Dir(0.0, 0.0, 1.0));
  camera->OrthogonalizeUp();
  const gp_Dir z(0.0, 0.0, 1.0);
  CHECK(mitcad::turnCamera(*camera, gp_Pnt(), z, 30.0, 0.0));
  const gp_Vec right = gp_Vec(camera->Direction()).Crossed(gp_Vec(camera->Up()));
  CHECK(std::abs(right.Z()) <= 1e-9);
  CHECK(nearly(camera->Eye().Z(), 300.0));
}

void refusedTests() {
  const occ::handle<Graphic3d_Camera> camera = viewCamera(false);
  const gp_Pnt eye = camera->Eye();
  CHECK(!mitcad::turnCamera(*camera, gp_Pnt(), camera->Up(), 0.0, 0.0));
  CHECK(!mitcad::turnCamera(*camera, gp_Pnt(), camera->Up(), std::numeric_limits<double>::quiet_NaN(), 0.0));
  CHECK(!mitcad::turnCamera(*camera, gp_Pnt(), camera->Up(), 0.0, std::numeric_limits<double>::infinity()));
  CHECK(same(camera->Eye(), eye));
}

} // namespace

int cameraTurnTests() {
  turnTests();
  verticalTests();
  refusedTests();
  return failures;
}
