// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>

#include <QImage>
#include <QObject>
#include <QPointer>
#include <QSize>
#include <QString>
#include <QWidget>

#include <AIS_InteractiveContext.hxx>
#include <AIS_InteractiveObject.hxx>
#include <V3d_View.hxx>
#include <gp_XYZ.hxx>

#include "ReplacedPictures.hpp"

class QEvent;
class QPainter;

namespace mitcad {

struct PaintLayerProbe;

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
//
// The driver-safe path (mitcad#2406: sketch mode froze the window on old
// Intel graphics): a layer keeps one texture while it is shown and has OCCT
// update its pixels in place for each picture (glTexSubImage2D; a new size
// redefines the same texture), instead of a new texture per picture; the
// picture goes up as BGRA, or as RGBA where the driver does not take BGRA
// or a BGRA upload failed. Around the texture upload and the draw it checks
// the GL errors and times them (PaintLayerProbe, in OCCT's frame), names
// the step to the hang marker while it runs (HangWatchdog::
// noteGraphicsStep), and logs the driver path once ("View paint layer
// 'sketch overlay' drawn by the view: …"), a slow upload or draw, and every
// GL error with the step it followed.
//
// Where the view cannot draw it (a GL error after its texture upload, also
// as RGBA, or after its draw; the driver refusing its texture; a view
// larger than the driver's largest texture; an upload and draw of more than
// kTooLongMs after its first one), every paint layer of every view is drawn
// as a widget over its view for the rest of the session, as before 0.72
// (the same paint function, painted by Qt after the view's frames), and the
// log says why: "View paint layers drawn as widgets over the view for the
// rest of the session: <layer>: <why>". MITCAD_TEST_PAINT_LAYER_FAILURE
// forces that path for the UI tests: "gl-error" reports GL_OUT_OF_MEMORY
// after every texture upload, "slow-frame" counts every upload and draw as
// taking 5 s.
class ViewPaintLayer : public QObject {
public:
  // Paints the layer: the painter's device is the picture, in view pixels
  // at the screen's ratio, cleared, with antialiasing and the view's font
  // and text colour.
  using Paint = std::function<void(QPainter&)>;

  // An upload and draw that takes longer than this (after the layer's
  // first, which may compile OCCT's texture program) falls back to widgets.
  static constexpr double kTooLongMs = 1000.0;
  // An upload and draw that takes longer than this is logged.
  static constexpr double kSlowMs = 100.0;

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
  // Whether the view draws it (false: a widget over the view, after a
  // failure of the view's path in this session).
  bool drawnByView() const { return m_widget.isNull(); }
  // Why the paint layers are widgets over the view in this session ("" when
  // the view draws them).
  static QString widgetReason();

  // For OcctViewer::handleViewRedraw. Before OCCT draws a frame: paints the
  // picture again if update() asked for it or the camera, the size or the
  // ratio changed.
  void beforeFrame(const occ::handle<V3d_View>& view);
  // After OCCT drew it: what its upload and draw found (GL errors, times),
  // logged; a failure turns the layers into widgets for the session.
  void afterFrame();
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
  // For OcctViewer::handleViewRedraw (mitcad#2437): whether enough of its
  // textures were let go since the view last drew a full frame that the
  // next frame must be one (OCCT lets them go only in full frames), and
  // that the view drew one.
  bool wantsFullFrame() const { return m_replaced.wantsFullFrame(); }
  void fullFrameDrawn() { m_replaced.fullFrameDrawn(); }

protected:
  // The view's resizes, for the widget.
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  class Widget;
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
  // The view's path failed: every layer becomes a widget (useWidget), the
  // reason logged once.
  void fallBack(const QString& why);
  // Drawn as a widget over the view from now on: the picture out of OCCT.
  void useWidget();
  // The widget's paint event.
  void paintWidget(QWidget& widget);

  QWidget& m_view;
  occ::handle<AIS_InteractiveContext> m_context;
  QString m_name;
  Paint m_paint;
  std::function<void()> m_frame;
  // What OCCT's frame found about the picture (shared with its elements).
  std::shared_ptr<PaintLayerProbe> m_probe;
  occ::handle<AIS_InteractiveObject> m_picture;
  // The widget over the view after a failure of the view's path.
  QPointer<QWidget> m_widget;
  bool m_shown = false;
  bool m_dirty = true;
  int m_paints = 0;
  // Pictures uploaded (the first may compile OCCT's texture program).
  int m_uploads = 0;
  // The picture goes up as RGBA (the driver took no BGRA).
  bool m_rgba = false;
  // The driver path logged; slow frames logged (at most a few).
  bool m_pathLogged = false;
  int m_slowLogged = 0;
  // The widget's first paint since it was shown was logged; under the test
  // switch, the pixels its last paint drew.
  bool m_widgetLogged = false;
  qint64 m_widgetPixels = -1;
  // The textures it let go whose memory OCCT still holds.
  ReplacedPictures m_replaced;
  // What it was painted for.
  QSize m_pixels;
  qreal m_ratio = 0.0;
  Pose m_pose;
};

} // namespace mitcad
