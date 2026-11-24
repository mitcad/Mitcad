// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QImage>
#include <QObject>
#include <QSize>
#include <QString>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <V3d_View.hxx>
#include <gp_XYZ.hxx>

class QPainter;
class QWidget;

namespace mitcad {

// A drawing over the whole 3D view painted with QPainter in view pixels
// (sketch mode's overlay, the dimensions shown outside sketch mode;
// mitcad#2296) that the view draws itself: painted into a picture of the
// view's size while the view draws a frame, just before OCCT draws it,
// and drawn by OCCT in its overlay layer over the scene, pixel for pixel.
//
// Not a widget over the view (mitcad#768): such a widget is painted again
// after every frame the view draws, and Qt then composes the window's area
// under the view again, which held sketch mode to about 30 frames a second
// on a fast screen while the processor and the graphics card idled.
//
// Painted again in a frame when update() asked for it, or when the camera,
// the view's size or its pixel ratio changed since it was last painted, so
// that it follows the camera in the same frame as the scene; painting asks
// for no frame of its own. Not pickable and not part of what a fit takes
// in: its owner answers what lies under a place (hit tests), and the view's
// mouse stays the view's. Made by OcctViewer::addPaintLayer, hidden at
// first.
class ViewPaintLayer : public QObject {
public:
  // Paints the layer: the painter's device is the picture, in view pixels
  // at the screen's ratio, cleared, with antialiasing and the view's font
  // and text colour.
  using Paint = std::function<void(QPainter&)>;

  // `frame` asks the view for a frame.
  ViewPaintLayer(QWidget& view, occ::handle<AIS_InteractiveContext> context, QString name, Paint painter,
                 std::function<void()> frame, QObject* owner);
  ~ViewPaintLayer() override;
  ViewPaintLayer(const ViewPaintLayer&) = delete;
  ViewPaintLayer& operator=(const ViewPaintLayer&) = delete;

  // Shown, it is painted in the next frame (which it asks for); hidden,
  // the view no longer draws it.
  void setShown(bool shown);
  bool isShown() const { return m_shown; }
  // What it draws changed: painted again in the next frame, which it asks
  // for. Nothing while hidden.
  void update();
  // Its name in the view's logs ("sketch overlay").
  const QString& name() const { return m_name; }

  // For OcctViewer::handleViewRedraw. Before OCCT draws a frame: paints the
  // picture again if update() asked for it or the camera, the size or the
  // ratio changed.
  void beforeFrame(const occ::handle<V3d_View>& view);
  // After OCCT drew it: whether a camera animation moved the camera as the
  // frame was drawn, after the picture was painted (the next frame paints
  // it again).
  bool behindCamera(const occ::handle<V3d_View>& view) const;
  // How often it was painted since the last call (the view's navigation
  // line counts the paints of a navigation).
  int takePaints();
  // For a snapshot of the view at another size (OcctViewer::snapshot),
  // which a widget over the view never was in: takes the picture out of
  // what OCCT draws (true when it was drawn), and puts it back.
  bool takeOutPicture();
  void putBackPicture();

private:
  // What the picture depends on of the camera: where it looks from and
  // how, not its depth range (which OCCT fits again at every frame).
  struct Pose {
    gp_XYZ eye;
    gp_XYZ center;
    gp_XYZ up;
    gp_XYZ axial;
    double scale = 0.0;
    double fov = 0.0;
    bool orthographic = true;
    bool operator==(const Pose& other) const;
  };
  static Pose poseOf(const occ::handle<V3d_View>& view);
  void paint(const QSize& pixels);

  QWidget& m_view;
  occ::handle<AIS_InteractiveContext> m_context;
  QString m_name;
  Paint m_paint;
  std::function<void()> m_frame;
  occ::handle<AIS_InteractiveObject> m_picture;
  bool m_shown = false;
  bool m_dirty = true;
  int m_paints = 0;
  // What it was painted for.
  QSize m_pixels;
  qreal m_ratio = 0.0;
  Pose m_pose;
};

} // namespace mitcad
