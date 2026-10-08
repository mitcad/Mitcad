// SPDX-License-Identifier: MIT
#pragma once

// The render settings' lights in the 3D view (mitcad#54, docs/rendering.md
// "Lights"): where a light is in the design (also one relative to the
// camera), the point and normal of a body's face under the mouse (a light
// aimed at it), and the glyphs Render Environment draws over the view.

#include <optional>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QWidget>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

namespace mitcad {

class OcctViewer;
struct CameraState;

namespace render {

// A light's place in the design: where it is and where it shines.
struct LightPose {
  gp_Pnt position;
  gp_Dir direction = gp_Dir(0, 0, -1);
};

// The design's place of a light of the render settings (its `position`,
// `direction` and `space`), seen from `camera` when it is relative to it
// (x to the right, y up, z toward the viewer, from the eye).
LightPose worldPose(const QJsonObject& light, const CameraState& camera);
// A place in the design as the `position` and `direction` of a light in
// `space` ("world" or "camera").
QJsonObject poseFields(const LightPose& pose, const QString& space, const CameraState& camera);

// The point of a shown body's face under a widget position (logical
// pixels) and the face's normal there, toward the viewer; none when no
// body is there. From the bodies' display triangles.
std::optional<std::pair<gp_Pnt, gp_Dir>> surfaceAt(OcctViewer& viewer, const QPointF& position);

// Small glyphs of the lights over the 3D view while Render Environment is
// open: a point light a dot with rays, a spot a cone, an area light its
// outline, all with a line where they shine; a sun an arrow toward the
// view's middle. The chosen light is orange. A child widget of the view
// that takes no input; follows the camera. Logs "Render light glyph <id>
// at x,y" (window coordinates) when glyphs move.
class RenderLightsOverlay : public QWidget {
public:
  explicit RenderLightsOverlay(OcctViewer& viewer);
  void setLights(const QJsonArray& lights, const QString& chosen);

protected:
  void paintEvent(QPaintEvent* event) override;

private:
  void follow();

  OcctViewer& m_viewer;
  QJsonArray m_lights;
  QString m_chosen;
  QString m_logged;
};

} // namespace render
} // namespace mitcad
