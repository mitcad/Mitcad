// SPDX-License-Identifier: MIT
#pragma once

// A 3D mouse's step applied to OCCT's camera (mitcad#951): the camera read
// into a SpaceMousePose, moved by spaceMouseMove (SpaceMouseNavigation.hpp)
// and written back. The scale goes out as it came in (Scale and SetScale),
// the visible height only sizes the pan: a view taller than wide has a
// visible height larger than its scale, and setting that height as the
// scale grew the view at every frame. OcctViewer calls it at each frame of
// a motion (view/SpaceMouseView.cpp); the unit tests call it on cameras of
// both aspects and projections.

#include <Graphic3d_Camera.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_XYZ.hxx>

#include "SpaceMouseNavigation.hpp"

namespace mitcad {

// Moves `camera` by `seconds` of `rates` about `pivot`; false (the camera
// unchanged) at rest, or for a camera or a step that makes no view.
inline bool moveSpaceMouseCamera(Graphic3d_Camera& camera, const gp_Pnt& pivot, const SpaceMouseRates& rates,
                                 double seconds, bool zUp) {
  if (!rates.moving()) {
    return false;
  }
  const auto vector = [](const gp_XYZ& value) { return SpaceMouseVec{{value.X(), value.Y(), value.Z()}}; };
  SpaceMousePose pose;
  pose.eye = vector(camera.Eye().XYZ());
  pose.target = vector(camera.Center().XYZ());
  pose.up = vector(camera.Up().XYZ());
  pose.height = camera.Scale();
  pose.panHeight = camera.ViewDimensions().Y();
  const SpaceMousePose moved = spaceMouseMove(pose, vector(pivot.XYZ()), rates, seconds, zUp);
  if (moved.eye == pose.eye && moved.target == pose.target && moved.up == pose.up && moved.height == pose.height) {
    return false;
  }
  camera.SetEyeAndCenter(gp_Pnt(moved.eye[0], moved.eye[1], moved.eye[2]),
                         gp_Pnt(moved.target[0], moved.target[1], moved.target[2]));
  camera.SetUp(gp_Dir(moved.up[0], moved.up[1], moved.up[2]));
  camera.OrthogonalizeUp();
  camera.SetScale(moved.height);
  return true;
}

} // namespace mitcad
