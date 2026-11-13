// SPDX-License-Identifier: MIT
#pragma once

// A view's visible height and OCCT's camera scale (mitcad#959). The camera
// the app saves and sets (CameraState: named views, Home, ui_camera, a
// reveal) carries the height of the model the view shows at the target,
// as Graphic3d_Camera::ViewDimensions().Y() reads it. OCCT's Scale() is
// that height only in a view wider than tall; in a view taller than wide
// it is the visible width, the height times the aspect. Setting a stored
// height as the scale showed a tall view zoomed out by 1/aspect. Here the
// height goes through the camera's aspect, so a camera shows the stored
// height at any aspect, and a height stored before keeps its meaning.
// OcctViewer::setCamera sets it; the unit tests (tests/camera_height.cpp)
// check it on OCCT's cameras of both aspects and projections. The focal
// length (mitcad#1717) changes the field of view with the same height.

#include <cmath>

#include <Graphic3d_Camera.hxx>

namespace mitcad {

// OCCT's scale for a view `aspect` wide for its height that shows `height`
// at the target: the height itself in a view wider than tall, else the
// width, the height times the aspect (as Graphic3d_Camera::ViewDimensions
// divides the scale by an aspect of 1 or less). An aspect that is no
// number or not positive leaves the height as it is.
inline double cameraScaleForHeight(double height, double aspect) {
  if (!std::isfinite(aspect) || !(aspect > 0.0) || aspect > 1.0) {
    return height;
  }
  return height * aspect;
}

// Sets `camera`'s scale so that it shows `height` at the target in its own
// aspect (a perspective camera moves its eye, as SetScale does).
inline void setVisibleHeight(Graphic3d_Camera& camera, double height) {
  camera.SetScale(cameraScaleForHeight(height, camera.Aspect()));
}

// The perspective camera's focal length (mitcad#1717), in mm as on a
// 35 mm frame, 36 x 24 mm: OCCT's field of view (FOVy, degrees) spans
// the frame's 24 mm, the view's height in a view wider than tall, so it
// is 2 atan(12 mm / f). OCCT's default of 45 degrees is about 29 mm.
inline constexpr double kFrameHalfHeight = 12.0; // mm

inline double fieldOfViewForFocalLength(double focalLength) {
  return 2.0 * std::atan(kFrameHalfHeight / focalLength) * 180.0 / 3.14159265358979323846;
}

inline double focalLengthForFieldOfView(double degrees) {
  return kFrameHalfHeight / std::tan(degrees * 3.14159265358979323846 / 360.0);
}

// Sets `camera`'s field of view to `degrees` and keeps the height it shows
// at the target (mitcad#1717): the eye moves along the view direction, as
// far as the new field of view takes to show the same height. In an
// orthographic camera the picture does not change; its eye moves the same
// way, so that the perspective shows the same height when it comes back.
// A field of view that is no number or not between 0 and 180 degrees
// leaves the camera as it is.
inline void setFieldOfViewKeepingHeight(Graphic3d_Camera& camera, double degrees) {
  if (!std::isfinite(degrees) || !(degrees > 0.0) || !(degrees < 180.0) || degrees == camera.FOVy()) {
    return;
  }
  const double before = std::tan(camera.FOVy() * 3.14159265358979323846 / 360.0);
  const double after = std::tan(degrees * 3.14159265358979323846 / 360.0);
  const double distance = camera.Distance() * before / after;
  if (std::isfinite(distance) && distance > 0.0) {
    camera.SetDistance(distance);
  }
  camera.SetFOVy(degrees);
}

} // namespace mitcad
