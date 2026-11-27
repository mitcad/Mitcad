// SPDX-License-Identifier: MIT
#pragma once

// Perspective with Orthographic Faces (mitcad#2473): a camera projection
// that is perspective, except while the camera looks exactly along one of
// the six face directions (Top, Bottom, Front, Back, Left, Right); then it
// is orthographic. OcctViewer applies it to the view's camera before each
// frame, to a camera animation's end (and turns its start perspective when
// the animation turns the view), and to the cameras it reports; the unit
// tests (tests/face_projection.cpp) check the rule and the height kept on
// OCCT's cameras.
//
// A switch keeps the height the view shows at the target (view/
// CameraHeight.hpp). The orthographic camera keeps its eye where a
// perspective camera of the same field of view shows that height: an
// orbit begun there (OCCT's orbit turns the eye it had as the drag began)
// and the perspective that comes back show the same height, without a
// jump.

#include <algorithm>
#include <cmath>

#include <Graphic3d_Camera.hxx>
#include <gp_Dir.hxx>

#include "CameraHeight.hpp"

namespace mitcad {

// How far the view direction may be from a face direction and still count
// as looking along it, in degrees: the standard views, the cube's faces
// and Look At end on the axis to rounding; a degree off is an orbit.
inline constexpr double kFaceDirectionTolerance = 0.05;

// True when `direction` is within `toleranceDegrees` of +X, -X, +Y, -Y,
// +Z or -Z.
inline bool looksAlongFace(const gp_Dir& direction, double toleranceDegrees = kFaceDirectionTolerance) {
  const double largest = std::max({std::abs(direction.X()), std::abs(direction.Y()), std::abs(direction.Z())});
  return largest >= std::cos(toleranceDegrees * 3.14159265358979323846 / 180.0);
}

// True when `a` and `b` look along the same face direction (within the
// tolerance): a move between them does not turn the view off it.
inline bool alongSameFace(const gp_Dir& a, const gp_Dir& b, double toleranceDegrees = kFaceDirectionTolerance) {
  return looksAlongFace(a, toleranceDegrees) && looksAlongFace(b, toleranceDegrees) &&
         a.Angle(b) <= 2.0 * toleranceDegrees * 3.14159265358979323846 / 180.0;
}

// Makes a mono `camera` orthographic or perspective, keeping the height it
// shows at the target; an orthographic one has its eye moved (the target
// kept) to where the perspective shows that height. A stereo camera, or a
// height or distance that makes no view, is left as it is. True when the
// projection changed.
inline bool setProjectionKeepingHeight(Graphic3d_Camera& camera, bool orthographic) {
  const Graphic3d_Camera::Projection type = camera.ProjectionType();
  if (type != Graphic3d_Camera::Projection_Orthographic && type != Graphic3d_Camera::Projection_Perspective) {
    return false;
  }
  const double height = camera.ViewDimensions().Y();
  if (!std::isfinite(height) || !(height > 0.0)) {
    return false;
  }
  const Graphic3d_Camera::Projection wanted =
      orthographic ? Graphic3d_Camera::Projection_Orthographic : Graphic3d_Camera::Projection_Perspective;
  const bool changed = type != wanted;
  if (changed) {
    camera.SetProjectionType(wanted);
  }
  const double scale = cameraScaleForHeight(height, camera.Aspect());
  if (orthographic) {
    // The eye where OCCT's own perspective puts it for this scale, moved
    // only when it is off by more than a millionth (a camera changed in
    // every frame would make each frame a full one). OCCT's field of view
    // tangent uses a rounded degree (Graphic3d_Camera's DTR_HALF): a
    // distance worked out here with the exact one moved the eye by about
    // 1e-9 of it at each switch, so OCCT moves it itself.
    const double occtHalfDegree = 0.5 * 0.0174532925;
    const double estimate = scale * 0.5 / std::tan(occtHalfDegree * camera.FOVy());
    if (std::isfinite(estimate) && estimate > 0.0 &&
        std::abs(camera.Distance() - estimate) > 1e-6 * std::max(estimate, camera.Distance())) {
      const double zNear = camera.ZNear();
      const double zFar = camera.ZFar();
      camera.SetProjectionType(Graphic3d_Camera::Projection_Perspective);
      camera.SetScale(scale);
      camera.SetProjectionType(Graphic3d_Camera::Projection_Orthographic);
      if (zFar > zNear) {
        camera.SetZRange(zNear, zFar); // an orthographic near plane may be behind the eye
      }
    }
  }
  // OCCT's orthographic scale or the perspective's eye, the same height.
  if (std::abs(camera.Scale() - scale) > 1e-12 * scale) {
    camera.SetScale(scale);
  }
  return changed;
}

// Perspective with Orthographic Faces on a mono `camera`: orthographic
// while it looks along a face direction, perspective otherwise, the height
// at the target kept. True when the projection changed.
inline bool followFaceProjection(Graphic3d_Camera& camera) {
  return setProjectionKeepingHeight(camera, looksAlongFace(camera.Direction()));
}

} // namespace mitcad
