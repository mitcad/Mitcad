// SPDX-License-Identifier: MIT
// The 3D view's side of a 3D mouse (mitcad#951): OcctViewer's camera moved
// at each frame by SpaceMouseController's rates (moveSpaceMouseCamera,
// SpaceMouseCamera.hpp, through the pure spaceMouseMove).
#include "../OcctViewer.hpp"

#include <algorithm>
#include <cmath>

#include <QDebug>

#include <Graphic3d_Camera.hxx>
#include <Standard_Failure.hxx>
#include <gp_Pnt.hxx>

#include "SpaceMouseCamera.hpp"

namespace mitcad {

void OcctViewer::setSpaceMouseRates(const SpaceMouseRates& rates) {
  // A hidden view (another workspace in front of it) keeps still.
  const SpaceMouseRates next = isVisible() ? rates : SpaceMouseRates();
  const bool wasMoving = m_spaceMouse.moving();
  m_spaceMouse = next;
  if (!next.moving()) {
    if (wasMoving) {
      endSpaceMouse();
    }
    return;
  }
  if (!wasMoving) {
    m_spaceMouseClock.invalidate();
    m_spaceMousePivot.reset();
    m_spaceMouseFrames = 0;
    m_spaceMouseTurned = 0.0;
    m_spaceMousePanned = 0.0;
    m_spaceMouseStartScale = 0.0;
  }
  requestFrame();
}

void OcctViewer::endSpaceMouse() {
  m_spaceMouseClock.invalidate();
  m_spaceMousePivot.reset();
  if (m_spaceMouseFrames > 0) {
    // For UI tests and support (app/COMMANDS.md, "Logs that UI tests read"):
    // the zoom is the camera's own, its scale before over its scale now.
    const double scale = m_view->Camera()->Scale();
    const double zoom = scale > 0.0 && m_spaceMouseStartScale > 0.0 ? m_spaceMouseStartScale / scale : 1.0;
    qDebug().noquote() << QStringLiteral("3D mouse motion: %1 frame(s), turned %2 degrees, panned %3 view "
                                         "heights, zoom %4x")
                              .arg(m_spaceMouseFrames)
                              .arg(m_spaceMouseTurned, 0, 'f', 1)
                              .arg(m_spaceMousePanned, 0, 'f', 2)
                              .arg(zoom, 0, 'f', 2);
  }
  m_spaceMouseFrames = 0;
}

void OcctViewer::stepSpaceMouse() {
  if (!m_spaceMouse.moving()) {
    return;
  }
  // The time since the motion's last frame: its first frame takes one of
  // 60 Hz, a frame after a stall at most 50 ms, so that the view never
  // jumps.
  double seconds = 1.0 / 60.0;
  if (m_spaceMouseClock.isValid()) {
    seconds = std::clamp(static_cast<double>(m_spaceMouseClock.nsecsElapsed()) / 1.0e9, 0.0, 0.05);
  }
  m_spaceMouseClock.start();
  try {
    const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
    if (m_spaceMouseFrames == 0) {
      AbortViewAnimation(); // the device takes over from an animated move
      m_spaceMouseStartScale = camera->Scale();
    }
    if (m_spaceMouse.turning() && !m_spaceMousePivot) {
      // The orbit's centre, taken as the motion begins, as a mouse orbit
      // takes it when its drag begins.
      m_spaceMousePivot = GravityPoint(m_context, m_view);
    }
    moveSpaceMouseCamera(*camera, m_spaceMousePivot.value_or(camera->Center()), m_spaceMouse, seconds,
                         m_navigation.constrainedOrbit);
  } catch (const Standard_Failure& failure) {
    qWarning().noquote() << QStringLiteral("3D mouse motion failed: %1").arg(QString::fromUtf8(failure.what()));
    m_spaceMouse = SpaceMouseRates();
    endSpaceMouse();
    return;
  }
  const SpaceMouseRates& rates = m_spaceMouse;
  const double turn = m_navigation.constrainedOrbit ? std::hypot(rates.pitch, rates.yaw)
                                                    : std::sqrt(rates.pitch * rates.pitch + rates.yaw * rates.yaw +
                                                                rates.roll * rates.roll);
  ++m_spaceMouseFrames;
  m_spaceMouseTurned += turn * seconds * 180.0 / 3.14159265358979323846;
  m_spaceMousePanned += std::hypot(rates.panX, rates.panY) * seconds;
  m_view->Invalidate();
  cameraMoved();
  // Input without a release: a navigation until the camera keeps still
  // (ViewNavigation's quiet time, docs/rendering.md).
  navigationStep();
  requestFrame(); // the next frame goes on with the motion
}

} // namespace mitcad
