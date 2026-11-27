// SPDX-License-Identifier: MIT
// Drawings over the whole 3D view painted with QPainter and drawn by the
// view in its overlay layer (mitcad#2296, ViewPaintLayer.hpp), or as
// widgets over the view where the view's path failed (mitcad#2406).
//
// OCCT's OpenGL headers before Qt's (as in OcctViewer.cpp): on macOS Qt's
// qopengl.h brings in <OpenGL/gl3.h>, whose function pointer types clash
// with OCCT's glext.h unless OCCT's come first.
#include <OpenGl_Context.hxx>
#include <OpenGl_Element.hxx>
#include <OpenGl_GlCore11.hxx>
#include <OpenGl_Group.hxx>
#include <OpenGl_Texture.hxx>
#include <OpenGl_TextureSet.hxx>
#include <OpenGl_Workspace.hxx>

#include "ViewPaintLayer.hpp"

#include <algorithm>
#include <cstddef>
#include <utility>

#include <QDebug>
#include <QElapsedTimer>
#include <QEvent>
#include <QMetaObject>
#include <QPaintEvent>
#include <QPainter>
#include <QPalette>
#include <QResizeEvent>
#include <QStringList>
#include <QWidget>
#include <QWindow>
#include <QtGlobal>

#include <Aspect_TypeOfTriedronPosition.hxx>
#include <Aspect_Window.hxx>
#include <Graphic3d_AlphaMode.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_Camera.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Graphic3d_TransformPers.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <Image_PixMap.hxx>
#include <Image_SupportedFormats.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Diagnostics.hpp"
#include "../framework/HangWatchdog.hpp"

namespace mitcad {

// What OCCT's frame found about a layer's picture (mitcad#2406): set by its
// elements while OCCT draws the frame (on the UI thread), read and cleared
// by ViewPaintLayer::afterFrame.
struct PaintLayerProbe {
  QString name; // the layer's, for the hang marker's step

  // The picture given to OCCT and not uploaded yet (PaintedPicture).
  bool pictureNew = false;
  int width = 0;
  int height = 0;
  bool rgba = false;
  bool textureReused = false;
  occ::handle<Image_PixMap> pixels; // the picture's pixels (BGRA turned RGBA in place)

  // The driver, read once in a frame.
  bool driverKnown = false;
  int maxTextureSize = 0;
  bool bgraTaken = true;
  // The picture was turned from BGRA to RGBA for a driver without BGRA.
  bool turnedRgba = false;

  // This frame (cleared by afterFrame).
  bool drawn = false;
  bool uploaded = false;
  int uploadedWidth = 0;
  int uploadedHeight = 0;
  bool uploadedRgba = false;
  bool uploadedReused = false;
  double uploadMs = 0.0;
  double drawMs = 0.0;
  bool textureRefused = false;
  QStringList errorsBefore; // left by what OCCT drew before the layer
  QStringList uploadErrors;
  QStringList drawErrors;
  QElapsedTimer clock;
};

namespace {

// MITCAD_TEST_PAINT_LAYER_FAILURE (ViewPaintLayer.hpp): "gl-error",
// "slow-frame" or "".
const QString& testFailure() {
  static const QString failure = qEnvironmentVariable("MITCAD_TEST_PAINT_LAYER_FAILURE").trimmed();
  return failure;
}

// The paint layers are widgets over the view for the rest of the session,
// and why ("" while the view draws them).
QString& widgetReasonText() {
  static QString reason;
  return reason;
}

// "GL_OUT_OF_MEMORY (0x0505)".
QString glErrorText(unsigned int code) {
  QString name;
  switch (code) {
  case 0x0500:
    name = QStringLiteral("GL_INVALID_ENUM");
    break;
  case 0x0501:
    name = QStringLiteral("GL_INVALID_VALUE");
    break;
  case 0x0502:
    name = QStringLiteral("GL_INVALID_OPERATION");
    break;
  case 0x0503:
    name = QStringLiteral("GL_STACK_OVERFLOW");
    break;
  case 0x0504:
    name = QStringLiteral("GL_STACK_UNDERFLOW");
    break;
  case 0x0505:
    name = QStringLiteral("GL_OUT_OF_MEMORY");
    break;
  case 0x0506:
    name = QStringLiteral("GL_INVALID_FRAMEBUFFER_OPERATION");
    break;
  case 0x0507:
    name = QStringLiteral("GL_CONTEXT_LOST");
    break;
  default:
    name = QStringLiteral("GL error");
    break;
  }
  return QStringLiteral("%1 (0x%2)").arg(name).arg(code, 4, 16, QLatin1Char('0'));
}

// The GL errors pending on the context (glGetError until none; at most a
// few, as a lost context may answer with one at every call).
QStringList takeGlErrors(const occ::handle<OpenGl_Context>& gl) {
  QStringList errors;
  if (gl.IsNull() || gl->core11fwd == nullptr) {
    return errors;
  }
  for (int i = 0; i < 8; ++i) {
    const unsigned int code = gl->core11fwd->glGetError();
    if (code == 0) {
      break;
    }
    errors << glErrorText(code);
  }
  return errors;
}

double elapsedMs(const QElapsedTimer& clock) {
  return clock.isValid() ? static_cast<double>(clock.nsecsElapsed()) / 1.0e6 : 0.0;
}

QString formatText(bool rgba) { return rgba ? QStringLiteral("RGBA") : QStringLiteral("BGRA"); }

// Before the picture's quad, in the same group (so drawn in the same pass):
// the GL errors that what OCCT drew before left, then the texture's upload
// (OCCT uploads it lazily when the group's aspects are applied, as
// RenderedUploadTiming in OcctViewer.cpp does), timed and checked.
class ProbeBefore final : public OpenGl_Element {
public:
  explicit ProbeBefore(std::shared_ptr<PaintLayerProbe> probe) : m_probe(std::move(probe)) {}

  void Render(const occ::handle<OpenGl_Workspace>& workspace) const override {
    PaintLayerProbe& p = *m_probe;
    const occ::handle<OpenGl_Context>& gl = workspace->GetGlContext();
    if (gl.IsNull()) {
      return;
    }
    if (!p.driverKnown) {
      p.driverKnown = true;
      p.maxTextureSize = gl->MaxTextureSize();
      const occ::handle<Image_SupportedFormats>& formats = gl->SupportedTextureFormats();
      p.bgraTaken = formats.IsNull() || formats->IsSupported(Image_Format_BGRA);
    }
    p.errorsBefore << takeGlErrors(gl);
    p.drawn = true;
    const bool upload = p.pictureNew;
    if (upload) {
      // A driver that takes no BGRA gets this picture as RGBA, turned in
      // place (its pixels are OCCT's alone), and the next ones painted so.
      if (!p.rgba && !p.bgraTaken && !p.pixels.IsNull() && p.pixels->Format() == Image_Format_BGRA &&
          Image_PixMap::SwapRgbaBgra(*p.pixels)) {
        p.pixels->SetFormat(Image_Format_RGBA);
        p.rgba = true;
        p.turnedRgba = true;
      }
      HangWatchdog::noteGraphicsStep(QStringLiteral("view paint layer '%1': texture upload, %2 x %3 %4, texture %5")
                                         .arg(p.name)
                                         .arg(p.width)
                                         .arg(p.height)
                                         .arg(formatText(p.rgba), p.textureReused ? QStringLiteral("updated")
                                                                                  : QStringLiteral("made")));
    } else {
      HangWatchdog::noteGraphicsStep(QStringLiteral("view paint layer '%1': texture bind").arg(p.name));
    }
    p.clock.start();
    workspace->ApplyAspects();
    if (upload) {
      p.pictureNew = false;
      p.uploaded = true;
      p.uploadedWidth = p.width;
      p.uploadedHeight = p.height;
      p.uploadedRgba = p.rgba;
      p.uploadedReused = p.textureReused;
      p.uploadMs = std::max(p.uploadMs, elapsedMs(p.clock));
      p.uploadErrors << takeGlErrors(gl);
      // The texture OCCT made of it: none or an invalid one when the driver
      // refused it (OpenGl_Texture::Init failed, its error taken there).
      const occ::handle<OpenGl_TextureSet>& textures = gl->ActiveTextures();
      p.textureRefused = textures.IsNull() || textures->IsEmpty() || textures->First().IsNull() ||
                         !textures->First()->IsValid();
      if (testFailure() == QLatin1String("gl-error")) {
        p.uploadErrors << QStringLiteral("GL_OUT_OF_MEMORY (0x0505) (MITCAD_TEST_PAINT_LAYER_FAILURE)");
      }
    }
    HangWatchdog::noteGraphicsStep(QStringLiteral("view paint layer '%1': draw").arg(p.name));
    p.clock.start();
  }

  void Release(OpenGl_Context*) override {}
  bool IsFillDrawMode() const override { return true; }

private:
  std::shared_ptr<PaintLayerProbe> m_probe;
};

// After the picture's quad: its draw, timed and checked.
class ProbeAfter final : public OpenGl_Element {
public:
  explicit ProbeAfter(std::shared_ptr<PaintLayerProbe> probe) : m_probe(std::move(probe)) {}

  void Render(const occ::handle<OpenGl_Workspace>& workspace) const override {
    PaintLayerProbe& p = *m_probe;
    p.drawMs = std::max(p.drawMs, elapsedMs(p.clock));
    p.clock.invalidate();
    p.drawErrors << takeGlErrors(workspace->GetGlContext());
    HangWatchdog::noteGraphicsStep(QString());
  }

  void Release(OpenGl_Context*) override {}
  bool IsFillDrawMode() const override { return true; }

private:
  std::shared_ptr<PaintLayerProbe> m_probe;
};

// The picture over the whole view, its lower left corner at the view's,
// one texel a pixel (Graphic3d_TMF_2d: the presentation's units are the
// view's pixels). Not pickable, and not part of what a fit takes in.
class PaintedPicture : public AIS_InteractiveObject {
public:
  explicit PaintedPicture(std::shared_ptr<PaintLayerProbe> probe) : m_probe(std::move(probe)) {
    SetTransformPersistence(new Graphic3d_TransformPers(Graphic3d_TMF_2d, Aspect_TOTP_LEFT_LOWER));
    SetInfiniteState(true);
  }

  // The picture, straight (not premultiplied) ARGB32 (BGRA in memory on
  // little-endian machines) or RGBA8888 of the view's size in pixels. OCCT
  // reads its pixels when it draws: kept here meanwhile. True when the quad
  // must be computed again (a new texture or another size); otherwise the
  // texture's pixels are updated in place (synchronizeTexture).
  bool setImage(QImage image, bool rgba) {
    m_image = std::move(image); // not shared: bits() copies nothing
    const QSize size = m_image.size();
    if (m_pixels.IsNull()) {
      m_pixels = new Image_PixMap();
    }
    m_pixels->InitWrapper(rgba ? Image_Format_RGBA : Image_Format_BGRA, m_image.bits(),
                          static_cast<std::size_t>(m_image.width()), static_cast<std::size_t>(m_image.height()),
                          static_cast<std::size_t>(m_image.bytesPerLine()));
    m_pixels->SetTopDown(true);
    // One texture while it is shown (mitcad#2406): a new revision of it has
    // OCCT update the same texture's pixels (glTexSubImage2D, or the same
    // texture redefined at another size), where a new texture per picture
    // made the driver allocate one for every picture.
    const bool reused = !m_texture.IsNull();
    if (reused) {
      m_texture->UpdateRevision();
    } else {
      m_texture = new Graphic3d_Texture2D(m_pixels);
      m_texture->GetParams()->SetModulate(false);
      m_texture->GetParams()->SetFilter(Graphic3d_TOTF_NEAREST); // texel for pixel
    }
    PaintLayerProbe& p = *m_probe;
    p.pictureNew = true;
    p.width = size.width();
    p.height = size.height();
    p.rgba = rgba;
    p.textureReused = reused;
    p.pixels = m_pixels;
    const bool resized = size != m_size;
    m_size = size;
    return !reused || resized;
  }

  void clear() {
    m_texture.Nullify();
    m_pixels.Nullify();
    m_probe->pixels.Nullify();
    m_probe->pictureNew = false;
    m_image = QImage();
    m_size = QSize();
  }

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int mode) override {
    if (mode != 0 || m_texture.IsNull() || m_image.isNull()) {
      return;
    }
    const auto w = static_cast<double>(m_image.width());
    const auto h = static_cast<double>(m_image.height());
    occ::handle<Graphic3d_ArrayOfTriangles> quad =
        new Graphic3d_ArrayOfTriangles(4, 6, Graphic3d_ArrayFlags_VertexTexel);
    quad->AddVertex(gp_Pnt(0, 0, 0), gp_Pnt2d(0, 0));
    quad->AddVertex(gp_Pnt(w, 0, 0), gp_Pnt2d(1, 0));
    quad->AddVertex(gp_Pnt(w, h, 0), gp_Pnt2d(1, 1));
    quad->AddVertex(gp_Pnt(0, h, 0), gp_Pnt2d(0, 1));
    quad->AddEdges(1, 2, 3);
    quad->AddEdges(1, 3, 4);
    occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
    aspect->SetInteriorStyle(Aspect_IS_SOLID);
    aspect->SetShadingModel(Graphic3d_TypeOfShadingModel_Unlit);
    aspect->SetInteriorColor(Quantity_Color(Quantity_NOC_WHITE));
    aspect->SetFaceCulling(Graphic3d_TypeOfBackfacingModel_DoubleSided);
    aspect->SetAlphaMode(Graphic3d_AlphaMode_Blend);
    aspect->SetTextureMap(m_texture);
    aspect->SetTextureMapOn();
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    // The upload and the draw between two probes (mitcad#2406).
    const auto glGroup = occ::handle<OpenGl_Group>::DownCast(group);
    if (!glGroup.IsNull()) {
      glGroup->AddElement(new ProbeBefore(m_probe));
    }
    group->AddPrimitiveArray(quad);
    if (!glGroup.IsNull()) {
      glGroup->AddElement(new ProbeAfter(m_probe));
    }
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
  std::shared_ptr<PaintLayerProbe> m_probe;
  QImage m_image;
  occ::handle<Image_PixMap> m_pixels;
  occ::handle<Graphic3d_Texture2D> m_texture;
  QSize m_size;
};

} // namespace

// The widget over the view a layer is drawn in after the view's path failed
// (as the sketch overlay and the shown dimensions were before 0.72): lets
// the mouse through to the view, painted by Qt after the view's frames.
class ViewPaintLayer::Widget : public QWidget {
public:
  Widget(QWidget& view, ViewPaintLayer& layer) : QWidget(&view), m_layer(layer) {
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setFocusPolicy(Qt::NoFocus);
    resize(view.size());
  }

  // Painted again: by Qt's composition where the window is exposed; where
  // it is not (Windows session 0, where Qt never exposes it), nothing
  // paints a widget, so it is rendered once into a picture after the
  // events in hand, as an exposed window's next paint would draw it.
  void paintAgain() {
    update();
    const QWindow* handle = window()->windowHandle();
    if (m_unseenQueued || !isVisible() || (handle != nullptr && handle->isExposed())) {
      return;
    }
    m_unseenQueued = true;
    QMetaObject::invokeMethod(
        this,
        [this] {
          m_unseenQueued = false;
          const QWindow* now = window()->windowHandle();
          if (isVisible() && (now == nullptr || !now->isExposed())) {
            (void)grab(); // paintEvent() through render()
          }
        },
        Qt::QueuedConnection);
  }

protected:
  void paintEvent(QPaintEvent* event) override {
    Q_UNUSED(event);
    m_layer.paintWidget(*this);
  }

private:
  ViewPaintLayer& m_layer;
  bool m_unseenQueued = false;
};

ViewPaintLayer* OcctViewer::addPaintLayer(const QString& name, std::function<void(QPainter&)> paint,
                                          QObject* owner) {
  auto* layer = new ViewPaintLayer(*this, m_context, name, std::move(paint), [this] { requestFrame(); },
                                   owner != nullptr ? owner : this);
  m_paintLayers.erase(std::remove_if(m_paintLayers.begin(), m_paintLayers.end(),
                                     [](const QPointer<ViewPaintLayer>& gone) { return gone.isNull(); }),
                      m_paintLayers.end());
  m_paintLayers.emplace_back(layer);
  return layer;
}

ViewPaintLayer::ViewPaintLayer(QWidget& view, occ::handle<AIS_InteractiveContext> context, QString name,
                               Paint painter, std::function<void()> frame, QObject* owner)
    : QObject(owner), m_view(view), m_context(std::move(context)), m_name(std::move(name)),
      m_paint(std::move(painter)), m_frame(std::move(frame)), m_probe(std::make_shared<PaintLayerProbe>()) {
  m_probe->name = m_name;
  m_picture = new PaintedPicture(m_probe);
  // A layer made after the view's path failed in this session is a widget
  // from the start.
  if (!widgetReasonText().isEmpty()) {
    useWidget();
  }
}

ViewPaintLayer::~ViewPaintLayer() {
  // A layer that goes while the view stays takes its picture with it (the
  // view's own end removes everything it draws first), and its widget,
  // which paints through it.
  if (!m_context.IsNull() && m_context->IsDisplayed(m_picture)) {
    m_context->Remove(m_picture, false);
  }
  delete m_widget.data();
}

QString ViewPaintLayer::widgetReason() { return widgetReasonText(); }

namespace {

// Exactly the same numbers. Not gp_XYZ::IsEqual with a tolerance of 0: it
// compares with `<`, so that no point equals itself, every frame looked
// like a camera move, the layer was painted again at each one and asked
// for the next (a frame loop that kept the timers its paints start from
// ever ending).
bool sameXyz(const gp_XYZ& a, const gp_XYZ& b) { return a.X() == b.X() && a.Y() == b.Y() && a.Z() == b.Z(); }

} // namespace

bool ViewPaintLayer::Pose::operator==(const Pose& other) const {
  return sameXyz(eye, other.eye) && sameXyz(center, other.center) && sameXyz(up, other.up) &&
         sameXyz(axial, other.axial) && scale == other.scale && fov == other.fov && orthographic == other.orthographic;
}

ViewPaintLayer::Pose ViewPaintLayer::poseOf(const occ::handle<V3d_View>& view) {
  const occ::handle<Graphic3d_Camera>& camera = view->Camera();
  Pose pose;
  pose.eye = camera->Eye().XYZ();
  pose.center = camera->Center().XYZ();
  pose.up = camera->Up().XYZ();
  pose.axial = camera->AxialScale();
  pose.scale = camera->Scale();
  pose.fov = camera->FOVy();
  pose.orthographic = camera->IsOrthographic();
  return pose;
}

void ViewPaintLayer::setShown(bool shown) {
  if (shown == m_shown) {
    return;
  }
  m_shown = shown;
  if (m_widget) {
    if (shown) {
      m_widget->resize(m_view.size());
      m_widget->show();
      m_widget->raise();
      m_widgetLogged = false; // its first paint is logged
      static_cast<Widget*>(m_widget.data())->paintAgain();
    } else {
      m_widget->hide();
    }
    return;
  }
  if (shown) {
    m_dirty = true;
    m_frame();
    return;
  }
  // Removed, not erased: shown again, its presentation is computed anew
  // with a new texture.
  if (m_context->IsDisplayed(m_picture)) {
    m_context->Remove(m_picture, false);
    m_replaced.replaced(); // its texture waits for a full frame (mitcad#2437)
  }
  // The picture's pixels and texture are not kept while it is not drawn.
  static_cast<PaintedPicture*>(m_picture.get())->clear();
  m_pixels = QSize();
  m_frame();
}

void ViewPaintLayer::update() {
  if (!m_shown) {
    return;
  }
  if (m_widget) {
    static_cast<Widget*>(m_widget.data())->paintAgain();
    return;
  }
  m_dirty = true;
  m_frame();
}

void ViewPaintLayer::beforeFrame(const occ::handle<V3d_View>& view) {
  // Another layer's failure in this session (of this view or another).
  if (m_widget.isNull() && !widgetReasonText().isEmpty()) {
    useWidget();
  }
  if (!m_shown || view.IsNull() || view->Window().IsNull()) {
    return;
  }
  int width = 0;
  int height = 0;
  view->Window()->Size(width, height);
  if (width <= 0 || height <= 0) {
    return;
  }
  const QSize pixels(width, height);
  const qreal ratio = m_view.devicePixelRatioF();
  const Pose pose = poseOf(view);
  if (!m_dirty && pixels == m_pixels && ratio == m_ratio && pose == m_pose) {
    return;
  }
  m_dirty = false;
  m_pixels = pixels;
  m_ratio = ratio;
  m_pose = pose;
  if (m_widget) {
    // A widget follows the camera after the frame, as the widgets over the
    // view did before 0.72 (after the view's viewChanged).
    QMetaObject::invokeMethod(
        m_widget.data(),
        [widget = m_widget] {
          if (widget) {
            static_cast<Widget*>(widget.data())->paintAgain();
          }
        },
        Qt::QueuedConnection);
    return;
  }
  // A size the driver takes (mitcad#2406): a view larger than its largest
  // texture is drawn as a widget.
  const PaintLayerProbe& p = *m_probe;
  if (p.driverKnown && p.maxTextureSize > 0 && (width > p.maxTextureSize || height > p.maxTextureSize)) {
    fallBack(QStringLiteral("the view's %1 x %2 pixels are more than the driver's largest texture (%3)")
                 .arg(width)
                 .arg(height)
                 .arg(p.maxTextureSize));
    return;
  }
  paint(pixels);
}

void ViewPaintLayer::afterFrame() {
  PaintLayerProbe& p = *m_probe;
  if (!p.drawn) {
    return;
  }
  // The frame is over: no graphics step of the layer's goes on (also when
  // the probe after the quad was not reached).
  HangWatchdog::noteGraphicsStep(QString());
  // Taken for this frame.
  const bool uploaded = p.uploaded;
  const QString picture = QStringLiteral("%1 x %2 %3")
                              .arg(p.uploadedWidth)
                              .arg(p.uploadedHeight)
                              .arg(formatText(p.uploadedRgba));
  double uploadMs = p.uploadMs;
  double drawMs = p.drawMs;
  const QStringList before = p.errorsBefore;
  const QStringList uploadErrors = p.uploadErrors;
  const QStringList drawErrors = p.drawErrors;
  const bool refused = uploaded && p.textureRefused;
  const bool reused = p.uploadedReused;
  const bool uploadedRgba = p.uploadedRgba;
  p.drawn = false;
  p.uploaded = false;
  p.uploadMs = 0.0;
  p.drawMs = 0.0;
  p.textureRefused = false;
  p.errorsBefore.clear();
  p.uploadErrors.clear();
  p.drawErrors.clear();
  if (p.turnedRgba) {
    p.turnedRgba = false;
    m_rgba = true;
    qInfo().noquote() << QStringLiteral("View paint layer '%1': the driver takes no BGRA texture; pictures go up as RGBA")
                             .arg(m_name);
  }
  if (!m_widget.isNull()) {
    return; // another layer's failure in this frame
  }
  if (testFailure() == QLatin1String("slow-frame") && uploaded) {
    uploadMs = 5000.0;
  }

  // What OCCT drew before the layer left: not the layer's, logged (the
  // first few times).
  static int beforeLogged = 0;
  if (!before.isEmpty() && beforeLogged < 5) {
    ++beforeLogged;
    qInfo().noquote() << QStringLiteral("View paint layer '%1': GL error %2 pending before the layer was drawn "
                                        "(left by the frame's earlier drawing)")
                             .arg(m_name, before.join(QStringLiteral(", ")));
  }
  if (uploaded) {
    ++m_uploads;
    if (timingLogged()) {
      qDebug().noquote() << QStringLiteral("Timing view paint layer upload: %1 ms, %2 %3, texture %4")
                                .arg(uploadMs, 0, 'f', 2)
                                .arg(m_name, picture, reused ? QStringLiteral("updated") : QStringLiteral("made"));
    }
    if (!m_pathLogged && uploadErrors.isEmpty() && !refused) {
      // The driver path, once a session per layer, for a report's log.
      m_pathLogged = true;
      qInfo().noquote() << QStringLiteral("View paint layer '%1' drawn by the view: %2 picture, one texture "
                                          "updated in place; the driver's largest texture %3, BGRA %4")
                               .arg(m_name, picture)
                               .arg(p.maxTextureSize)
                               .arg(p.bgraTaken ? QStringLiteral("taken") : QStringLiteral("not taken"));
    }
  }
  if (timingLogged()) {
    qDebug().noquote()
        << QStringLiteral("Timing view paint layer draw: %1 ms, %2").arg(drawMs, 0, 'f', 2).arg(m_name);
  }
  const double total = uploadMs + drawMs;
  if (total > kSlowMs && m_slowLogged < 5) {
    ++m_slowLogged;
    qWarning().noquote() << QStringLiteral("View paint layer '%1': slow frame: texture upload %2 ms, draw %3 ms (%4)")
                                .arg(m_name)
                                .arg(uploadMs, 0, 'f', 1)
                                .arg(drawMs, 0, 'f', 1)
                                .arg(uploaded ? picture : QStringLiteral("no upload"));
  }

  if (!uploadErrors.isEmpty() || refused) {
    const QString what = !uploadErrors.isEmpty()
                             ? QStringLiteral("GL error %1 after its texture upload (%2)")
                                   .arg(uploadErrors.join(QStringLiteral(", ")), picture)
                             : QStringLiteral("the driver refused its texture (%1)").arg(picture);
    if (!uploadedRgba) {
      // A BGRA upload failed: once more as RGBA, the format every driver
      // takes.
      m_rgba = true;
      m_dirty = true;
      qWarning().noquote() << QStringLiteral("View paint layer '%1': %2; painted again as RGBA").arg(m_name, what);
      m_frame();
      return;
    }
    fallBack(what);
    return;
  }
  if (!drawErrors.isEmpty()) {
    fallBack(QStringLiteral("GL error %1 after it was drawn").arg(drawErrors.join(QStringLiteral(", "))));
    return;
  }
  // Not the frame of its first upload, which may compile OCCT's texture
  // program.
  if ((uploaded ? m_uploads > 1 : m_uploads >= 1) && total > kTooLongMs) {
    fallBack(QStringLiteral("its frame took %1 ms (texture upload %2 ms, draw %3 ms; %4)")
                 .arg(total, 0, 'f', 0)
                 .arg(uploadMs, 0, 'f', 1)
                 .arg(drawMs, 0, 'f', 1)
                 .arg(picture));
  }
}

bool ViewPaintLayer::behindCamera(const occ::handle<V3d_View>& view) const {
  return m_shown && m_widget.isNull() && !m_pixels.isEmpty() && !view.IsNull() && !(poseOf(view) == m_pose);
}

int ViewPaintLayer::takePaints() {
  const int paints = m_paints;
  m_paints = 0;
  return paints;
}

bool ViewPaintLayer::takeOutPicture() {
  if (!m_context->IsDisplayed(m_picture)) {
    return false;
  }
  m_context->Erase(m_picture, false); // its presentation and picture kept
  return true;
}

void ViewPaintLayer::putBackPicture() {
  if (!m_shown || m_widget || m_context->IsDisplayed(m_picture)) {
    return;
  }
  m_context->Display(m_picture, 0, -1, false);
  m_context->SetZLayer(m_picture, Graphic3d_ZLayerId_TopOSD);
}

void ViewPaintLayer::paint(const QSize& pixels) {
  ScopedTiming timing("view paint layer");
  timing.setDetail(m_name);
  // A new picture each time (the texture's pixels are updated from it):
  // the one OCCT drew last stays until the new one replaces it.
  QImage image(pixels, QImage::Format_ARGB32_Premultiplied);
  image.setDevicePixelRatio(m_ratio);
  image.fill(Qt::transparent);
  {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    // As a painter of the widget would start.
    painter.setFont(m_view.font());
    painter.setPen(m_view.palette().color(QPalette::WindowText));
    m_paint(painter);
  }
  // OCCT blends straight alpha (Graphic3d_AlphaMode_Blend); converted in
  // place, to B, G, R, A in memory (ARGB32 on little-endian machines) or
  // R, G, B, A where the driver takes no BGRA.
  image.convertTo(m_rgba ? QImage::Format_RGBA8888 : QImage::Format_ARGB32);
  auto* picture = static_cast<PaintedPicture*>(m_picture.get());
  const bool recompute = picture->setImage(std::move(image), m_rgba);
  ++m_paints;
  if (m_context->IsDisplayed(m_picture)) {
    if (recompute) {
      // A new size: the quad is computed again; its texture is the same
      // one, redefined at the new size when OCCT draws it.
      m_context->Redisplay(m_picture, false);
    } else {
      // The same quad and texture: OCCT updates the texture's pixels when
      // it draws (the texture's new revision).
      m_picture->SynchronizeAspects();
    }
    return;
  }
  m_context->Display(m_picture, 0, -1, false);
  m_context->SetZLayer(m_picture, Graphic3d_ZLayerId_TopOSD);
}

void ViewPaintLayer::fallBack(const QString& why) {
  QString& reason = widgetReasonText();
  if (reason.isEmpty()) {
    reason = QStringLiteral("%1: %2").arg(m_name, why);
    if (!testFailure().isEmpty()) {
      reason += QStringLiteral(" (MITCAD_TEST_PAINT_LAYER_FAILURE=%1)").arg(testFailure());
    }
    qWarning().noquote() << QStringLiteral("View paint layers drawn as widgets over the view for the rest of the "
                                           "session: %1")
                                .arg(reason);
  }
  useWidget();
}

void ViewPaintLayer::useWidget() {
  if (m_widget) {
    return;
  }
  // The picture out of OCCT's frames, its texture let go.
  if (m_context->IsDisplayed(m_picture)) {
    m_context->Remove(m_picture, false);
    m_replaced.replaced();
  }
  static_cast<PaintedPicture*>(m_picture.get())->clear();
  m_pixels = QSize();
  auto* widget = new Widget(m_view, *this);
  // Named as its owner (sketchOverlay, shownSketchDimensions): the
  // "View navigation" line names the widgets painted over the view.
  if (parent() != nullptr && !parent()->objectName().isEmpty()) {
    widget->setObjectName(parent()->objectName());
  }
  m_widget = widget;
  m_view.installEventFilter(this);
  m_widgetLogged = false;
  if (m_shown) {
    widget->show();
    widget->raise();
    widget->paintAgain();
  } else {
    widget->hide();
  }
  // The view drawn again without the picture.
  m_frame();
}

bool ViewPaintLayer::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_view && event->type() == QEvent::Resize && m_widget) {
    m_widget->resize(static_cast<QResizeEvent*>(event)->size());
  }
  return QObject::eventFilter(watched, event);
}

void ViewPaintLayer::paintWidget(QWidget& widget) {
  ScopedTiming timing("view paint layer widget");
  timing.setDetail(m_name);
  const auto start = [this](QPainter& painter) {
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setFont(m_view.font());
    painter.setPen(m_view.palette().color(QPalette::WindowText));
  };
  {
    QPainter painter(&widget);
    start(painter);
    m_paint(painter);
  }
  ++m_paints;
  // Under the test switch, what the paint drew (the same paint into a
  // picture, its pixels with any cover counted), whenever it changes.
  qint64 drawn = -1;
  if (!testFailure().isEmpty()) {
    const qreal ratio = widget.devicePixelRatioF();
    QImage image(widget.size() * ratio, QImage::Format_ARGB32_Premultiplied);
    image.setDevicePixelRatio(ratio);
    image.fill(Qt::transparent);
    {
      QPainter painter(&image);
      start(painter);
      m_paint(painter);
    }
    drawn = 0;
    for (int y = 0; y < image.height(); ++y) {
      const auto* line = reinterpret_cast<const QRgb*>(image.constScanLine(y));
      for (int x = 0; x < image.width(); ++x) {
        if (qAlpha(line[x]) != 0) {
          ++drawn;
        }
      }
    }
  }
  if (m_widgetLogged && drawn == m_widgetPixels) {
    return;
  }
  m_widgetLogged = true;
  m_widgetPixels = drawn;
  QString line = QStringLiteral("View paint layer '%1' painted as a widget over the view: %2 x %3")
                     .arg(m_name)
                     .arg(widget.width())
                     .arg(widget.height());
  if (drawn >= 0) {
    line += QStringLiteral(", %1 pixel(s) drawn").arg(drawn);
  }
  qInfo().noquote() << line;
}

} // namespace mitcad
