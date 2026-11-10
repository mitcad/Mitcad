// SPDX-License-Identifier: MIT
#pragma once

#include <QToolButton>

#include <AIS_ViewCube.hxx>
#include <gp_Dir.hxx>
#include <gp_XYZ.hxx>

class QEnterEvent;
class QEvent;
class QMouseEvent;
class QPaintEvent;

namespace mitcad {

// The orientation cube (mitcad#430): AIS_ViewCube's picking, hover, turns
// and logs, drawn in glass of its own. Over a dark background dark smoked
// glass with light names, over a light one frosted light glass with dark
// names; the faces a little see-through (only those in front are drawn, the
// names are in their textures), the edges and corners AIS_ViewCube's flat
// bevels, a shade apart, the axes in colour (X red, Y green, Z blue), the
// hover in the accent.
occ::handle<AIS_ViewCube> makeViewCube();

// The cube as large as AIS_ViewCube::SetSize(size) makes AIS_ViewCube's
// own cube, a tenth larger across, with wider bevels (mitcad#1620): its
// face 0.86 of the size and the bevels of its edges and corners 0.29 of it
// (AIS_ViewCube's 0.15), so that the edges and corners are about twice as
// easy to hit; the axes and their letters for the size (scaleViewCube).
// AIS_ViewCube::Size() is then the face's side; viewCubeSize gives `size`
// back. The caller sets the font height after it, and redisplays the cube.
void sizeViewCube(AIS_ViewCube& cube, double size);
// The size sizeViewCube gave the cube last (its Size() for another cube).
double viewCubeSize(const AIS_ViewCube& cube);
// The axes' thickness and letters for the cube's size (sizeViewCube).
void scaleViewCube(AIS_ViewCube& cube);

// Whether the view's background is dark: the cube's glass follows it. The
// caller redisplays the cube.
void setViewCubeBackdrop(AIS_ViewCube& cube, bool dark);

// The cube in a perspective of its own (mitcad#774), whatever the view's
// projection: seen from straight in front of its middle, from where a 45
// degree field of view (the view's default) shows twice its size, and as
// large as the orthographic cube where its front face is. Its faces, edges,
// corners, axes, hover and picking are made so on its geometry, for the
// direction setViewCubeEye gives; the caller draws it with an orthographic
// transform persistence and redisplays it.
void setViewCubePerspective(AIS_ViewCube& cube, bool perspective);
// The direction from the cube towards the eye (the camera's direction
// reversed). True when the cube's own perspective changed with it: the
// caller then redisplays the cube and computes its picking again.
bool setViewCubeEye(AIS_ViewCube& cube, const gp_Dir& toEye);
// Where the cube draws a point of its own coordinates: through its own
// perspective, or the point itself without one.
gp_XYZ viewCubePoint(const AIS_ViewCube& cube, const gp_XYZ& point);

// The Home button at the cube: a small capsule of the cube's glass, its
// house in the colour of the cube's names, the accent on hover.
class ViewCubeHomeButton : public QToolButton {
public:
  explicit ViewCubeHomeButton(QWidget* parent);
  // The glass over a dark background or a light one.
  void setDark(bool dark);

protected:
  void paintEvent(QPaintEvent* event) override;
  void enterEvent(QEnterEvent* event) override;
  void leaveEvent(QEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;

private:
  bool m_dark = false;
  bool m_hovered = false;
};

} // namespace mitcad
