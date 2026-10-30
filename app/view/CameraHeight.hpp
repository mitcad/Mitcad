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
// check it on OCCT's cameras of both aspects and projections.

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

} // namespace mitcad
