// SPDX-License-Identifier: MIT
#ifdef _WIN32
// Before any OCCT header: Standard_Macro.hxx defines NOGDI and NOUSER, which
// would hide wglGetCurrentDC() and WindowFromDC().
#include <windows.h>
#endif
// OCCT's OpenGL headers before Qt's: on macOS Qt's qopengl.h brings in
// <OpenGL/gl3.h>, whose function pointer types clash with OCCT's glext.h
// unless OCCT's come first (as in OCCT's own Qt samples).
#include <OpenGl_Context.hxx>
#include <OpenGl_FrameBuffer.hxx>
#include <OpenGl_GraphicDriver.hxx>
#include <OpenGl_View.hxx>
#include <OpenGl_Window.hxx>

#include "OcctViewer.hpp"

#include "PickContext.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/Icons.hpp"
#include "framework/Cursors.hpp"
#include "framework/Theme.hpp"
#include "platform/MacChrome.hpp"
#include "framework/Diagnostics.hpp"
#include "framework/TestSync.hpp"

#include <algorithm>
#include <cmath>
#include <initializer_list>
#include <tuple>

#include <QGuiApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QPainter>
#include <QSurfaceFormat>
#include <QTimer>
#include <QToolButton>
#include <QWheelEvent>
#include <QWindow>
#include <QtLogging>
#include <QtMath>

#include <AIS_AnimationCamera.hxx>
#include <Aspect_DisplayConnection.hxx>
#include <Aspect_NeutralWindow.hxx>
#include <Aspect_ScrollDelta.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepTools.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <Bnd_Box.hxx>
#include <Graphic3d_TransformPers.hxx>
#include <Graphic3d_ZLayerSettings.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Triangulation.hxx>
#include <Precision.hxx>
#include <Prs3d_IsoAspect.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Compound.hxx>
#include <V3d.hxx>
#include <Prs3d_Drawer.hxx>
#include <Prs3d_LineAspect.hxx>
#include <Prs3d_PointAspect.hxx>
#include <Prs3d_ShadingAspect.hxx>
#include <Quantity_Color.hxx>
#include <SelectMgr_Selection.hxx>
#include <SelectMgr_SensitiveEntity.hxx>
#include <SelectMgr_ViewerSelector.hxx>
#include <StdSelect_BRepOwner.hxx>
#include <StdSelect_BRepSelectionTool.hxx>
#include <StdSelect_ViewerSelector3d.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <V3d_AmbientLight.hxx>
#include <V3d_DirectionalLight.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>

namespace mitcad {
namespace {

const Quantity_Color kBodyColor(0.70, 0.74, 0.80, Quantity_TOC_RGB);
const Quantity_Color kEdgeColor(0.12, 0.13, 0.15, Quantity_TOC_RGB);
const Quantity_Color kSketchColor(0.10, 0.35, 0.85, Quantity_TOC_RGB);
const Quantity_Color kConstrainedColor(0.08, 0.09, 0.11, Quantity_TOC_RGB);
const Quantity_Color kCenterlineColor(0.25, 0.30, 0.40, Quantity_TOC_RGB);
const Quantity_Color kReferenceColor(0.55, 0.20, 0.70, Quantity_TOC_RGB);
const Quantity_Color kConstructionColor(0.85, 0.50, 0.10, Quantity_TOC_RGB);
const Quantity_Color kProfileFill(0.55, 0.70, 0.95, Quantity_TOC_RGB);
const Quantity_Color kPreviewColor(0.30, 0.55, 0.95, Quantity_TOC_RGB);
const Quantity_Color kCutPreviewColor(0.90, 0.20, 0.15, Quantity_TOC_RGB);
const Quantity_Color kPlaneColor(0.95, 0.70, 0.35, Quantity_TOC_RGB);
const Quantity_Color kAxisColor(0.85, 0.45, 0.10, Quantity_TOC_RGB);
const Quantity_Color kHiddenEdgeColor(0.45, 0.48, 0.54, Quantity_TOC_RGB);
// Section Analysis' caps: a colour bodies seldom have.
const Quantity_Color kCapColor(0.95, 0.62, 0.10, Quantity_TOC_RGB);
// Cosmetic threads' rings: darker than the body, lighter than its edges.
const Quantity_Color kThreadColor(0.30, 0.33, 0.40, Quantity_TOC_RGB);

// A left press that moves further than this is a window selection.
constexpr double kDragPixels = 4.0;
// The orientation cube's corner of the view and its centre's distance from the
// corner are OcctViewer::kCubeArea and kCubeOffset (widget pixels; the cube
// itself is drawn in device pixels and scaled by the device pixel ratio, see
// applyDevicePixelRatio).
constexpr int kCubeOffset = OcctViewer::kCubeOffset;
// A press on the cube orbits the view (Aspect_VKeyFlags of its own, which
// no key gives).
constexpr Aspect_VKeyFlags kCubeDrag = Aspect_VKeyFlags_META;
// A trackpad's scroll goes to the view as a drag of the left button with keys
// of their own, as for the cube, so that it pans and orbits the way a mouse
// drag does.
constexpr Aspect_VKeyFlags kScrollPan = Aspect_VKeyFlags_META | Aspect_VKeyFlags_SHIFT;
constexpr Aspect_VKeyFlags kScrollOrbit = Aspect_VKeyFlags_META | Aspect_VKeyFlags_CTRL;
// A scroll that pauses this long ends its drag (ms).
constexpr int kScrollEndMilliseconds = 150;
// The zoom steps (what a wheel notch of 15 gives, about 15 %) of a pixel of
// scrolling with Shift or Ctrl, and of 1.0 of a pinch's magnification
// (AIS_ViewController zooms by 1 + steps / 100).
constexpr double kScrollZoomPerPixel = 0.15;
constexpr double kPinchZoomSteps = 100.0;
// Qt's rotation gestures count degrees clockwise (the view turns with the
// fingers); flip this where a Qt version counts the other way.
constexpr double kRollDirection = 1.0;
// How long the camera must keep still before it is logged (ms).
constexpr int kRestMilliseconds = 250;

// The cube's place in the upper right corner: its centre's distance from the
// corner in device pixels (the overlay insets move the corner).
occ::handle<Graphic3d_TransformPers> cubePersistence(qreal ratio, const QMargins& insets = QMargins()) {
  return new Graphic3d_TransformPers(
      Graphic3d_TMF_TriedronPers, Aspect_TOTP_RIGHT_UPPER,
      NCollection_Vec2<int>(qRound((kCubeOffset + insets.right()) * ratio),
                            qRound((kCubeOffset + insets.top()) * ratio)));
}

// The icon of a cube arrow by its id.
QString arrowIconName(const QString& id) {
  const bool roll = id == QStringLiteral("cw") || id == QStringLiteral("ccw");
  return (roll ? QStringLiteral("view-roll-") : QStringLiteral("view-arrow-")) + id;
}

// The SF Symbol that stands for a button over the view, by the id of the
// cube arrow ("home" for the home button).
QString overlaySymbol(const QString& id) {
  static const std::pair<const char*, const char*> symbols[] = {
      {"home", "house"},         {"up", "chevron.up"},      {"down", "chevron.down"},
      {"left", "chevron.left"},  {"right", "chevron.right"}, {"cw", "arrow.clockwise"},
      {"ccw", "arrow.counterclockwise"}};
  for (const auto& [name, symbol] : symbols) {
    if (id == QLatin1String(name)) {
      return QString::fromLatin1(symbol);
    }
  }
  return {};
}

// A small round button over the view in the material of the floating
// cards: a translucent fill, a hairline border and a soft shadow; the icon
// is drawn by the tool button.
class GlassButton : public QToolButton {
public:
  explicit GlassButton(QWidget* parent) : QToolButton(parent) {
    setAutoRaise(true);
    setAttribute(Qt::WA_Hover);
  }

protected:
  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QRectF circle = QRectF(rect()).adjusted(2, 1.5, -2, -2.5);
    painter.setPen(Qt::NoPen);
    painter.setBrush(mitcad::cardShadowColor());
    painter.drawEllipse(circle.translated(0, 1));
    QColor fill = mitcad::cardFillColor(palette());
    painter.setBrush(fill);
    painter.drawEllipse(circle);
    if (isDown() || underMouse()) {
      QColor tint = mitcad::accentColor(palette());
      tint.setAlphaF(isDown() ? 0.35 : 0.16);
      painter.setBrush(tint);
      painter.drawEllipse(circle);
    }
    painter.setPen(QPen(mitcad::cardBorderColor(palette()), 1.0));
    painter.setBrush(Qt::NoBrush);
    painter.drawEllipse(circle);
    const QSize size = iconSize();
    const QPixmap pixmap = icon().pixmap(size, devicePixelRatioF(), isEnabled() ? QIcon::Normal : QIcon::Disabled);
    painter.drawPixmap(QPointF(circle.center() - QPointF(size.width() / 2.0, size.height() / 2.0)), pixmap);
  }
};

// The buttons over the view: round glass ones on the floating chrome.
QToolButton* makeOverlayButton(QWidget* parent) {
  if (chromeStyle() == ChromeStyle::Floating) {
    return new GlassButton(parent);
  }
  return new QToolButton(parent);
}

// The icon of a button over the view: on the floating chrome an SF Symbol
// where the system has it (drawn in the palette's colours, on the glass
// button), else the icon of the file, which follows the view's background.
QIcon overlayIcon(const QString& id, IconBackdrop backdrop) {
  const QString file = id == QLatin1String("home") ? QStringLiteral("home") : arrowIconName(id);
  const QIcon fallback = themeIcon(file, backdrop);
  if (chromeStyle() != ChromeStyle::Floating) {
    return fallback;
  }
  return mitcad::mac::symbolIcon(overlaySymbol(id), fallback);
}

Quantity_Color occtColor(const QColor& color) {
  return Quantity_Color(color.redF(), color.greenF(), color.blueF(), Quantity_TOC_sRGB);
}

// The layout grid's colours over a background: lighter on a dark one,
// darker on a light one, the major lines twice as far from it as the minor
// ones (13 and 24 percent of the way to white).
void setGridColors(LayoutGrid& grid, const QColor& background) {
  const float target = background.lightnessF() < 0.5f ? 1.0f : 0.0f;
  const auto mix = [&](float amount) {
    const auto towards = [&](float from) { return from + (target - from) * amount; };
    return occtColor(QColor::fromRgbF(towards(background.redF()), towards(background.greenF()),
                                      towards(background.blueF())));
  };
  grid.setColors(mix(0.13f), mix(0.24f));
}

// Wraps the framebuffer object Qt binds for paintGL() so that OCCT renders
// into it, with sRGB output enabled.
class QtFrameBuffer : public OpenGl_FrameBuffer {
  DEFINE_STANDARD_RTTI_INLINE(QtFrameBuffer, OpenGl_FrameBuffer)

public:
  void BindBuffer(const occ::handle<OpenGl_Context>& context) override {
    OpenGl_FrameBuffer::BindBuffer(context);
    context->SetFrameBufferSRGB(true, false);
  }

  void BindDrawBuffer(const occ::handle<OpenGl_Context>& context) override {
    OpenGl_FrameBuffer::BindDrawBuffer(context);
    context->SetFrameBufferSRGB(true, false);
  }

  void BindReadBuffer(const occ::handle<OpenGl_Context>& context) override {
    OpenGl_FrameBuffer::BindReadBuffer(context);
  }
};

occ::handle<OpenGl_Context> glContextOf(const occ::handle<V3d_View>& view) {
  const occ::handle<OpenGl_View> glView = occ::handle<OpenGl_View>::DownCast(view->View());
  return glView->GlWindow()->GetGlContext();
}

Aspect_VKeyMouse toVKeyMouse(Qt::MouseButtons buttons) {
  Aspect_VKeyMouse result = Aspect_VKeyMouse_NONE;
  if (buttons & Qt::LeftButton) {
    result |= Aspect_VKeyMouse_LeftButton;
  }
  if (buttons & Qt::MiddleButton) {
    result |= Aspect_VKeyMouse_MiddleButton;
  }
  if (buttons & Qt::RightButton) {
    result |= Aspect_VKeyMouse_RightButton;
  }
  return result;
}

Aspect_VKeyFlags toVKeyFlags(Qt::KeyboardModifiers modifiers) {
  Aspect_VKeyFlags result = Aspect_VKeyFlags_NONE;
  if (modifiers & Qt::ShiftModifier) {
    result |= Aspect_VKeyFlags_SHIFT;
  }
  if (modifiers & Qt::ControlModifier) {
    result |= Aspect_VKeyFlags_CTRL;
  }
  if (modifiers & Qt::AltModifier) {
    result |= Aspect_VKeyFlags_ALT;
  }
  return result;
}

// A scroll of a trackpad or of a Magic Mouse: continuous, so with a phase or
// pixel deltas. A mouse wheel has neither.
bool isTrackpadScroll(const QWheelEvent* event) {
  return event->phase() != Qt::NoScrollPhase || !event->pixelDelta().isNull();
}

// How far a scroll moved the content, in pixels of the widget.
QPointF scrollPixels(const QWheelEvent* event) {
  if (!event->pixelDelta().isNull()) {
    return QPointF(event->pixelDelta());
  }
  return QPointF(event->angleDelta()) / 6.0; // a wheel notch (120) is about 20 pixels
}

NCollection_Vec2<int> devicePixel(const QPointF& point) {
  return NCollection_Vec2<int>(qRound(point.x()), qRound(point.y()));
}

QString surfaceType(const TopoDS_Shape& face) {
  switch (BRepAdaptor_Surface(TopoDS::Face(face)).GetType()) {
  case GeomAbs_Plane:
    return QStringLiteral("plane");
  case GeomAbs_Cylinder:
    return QStringLiteral("cylinder");
  case GeomAbs_Cone:
    return QStringLiteral("cone");
  case GeomAbs_Sphere:
    return QStringLiteral("sphere");
  case GeomAbs_Torus:
    return QStringLiteral("torus");
  default:
    return QStringLiteral("other");
  }
}

QString curveType(const TopoDS_Shape& edge) {
  switch (BRepAdaptor_Curve(TopoDS::Edge(edge)).GetType()) {
  case GeomAbs_Line:
    return QStringLiteral("line");
  case GeomAbs_Circle:
    return QStringLiteral("circle");
  case GeomAbs_Ellipse:
    return QStringLiteral("ellipse");
  default:
    return QStringLiteral("other");
  }
}

void setEdgeStyle(const occ::handle<AIS_Shape>& object, const Quantity_Color& color, double width,
                  Aspect_TypeOfLine type = Aspect_TOL_SOLID) {
  const occ::handle<Prs3d_Drawer>& drawer = object->Attributes();
  object->SetColor(color);
  object->SetWidth(width);
  drawer->SetWireAspect(new Prs3d_LineAspect(color, type, width));
  drawer->SetLineAspect(new Prs3d_LineAspect(color, type, width));
  drawer->SetFreeBoundaryAspect(new Prs3d_LineAspect(color, type, width));
  drawer->SetUnFreeBoundaryAspect(new Prs3d_LineAspect(color, type, width));
}

void setPointStyle(const occ::handle<AIS_Shape>& object, const Quantity_Color& color,
                   Aspect_TypeOfMarker marker, double scale) {
  object->SetColor(color);
  object->Attributes()->SetPointAspect(new Prs3d_PointAspect(marker, color, scale));
}

// The colour of a body's edges where they are drawn alone (wireframe): its
// appearance's colour, darker, or the default dark edges.
Quantity_Color wireColor(const QColor& color) {
  return color.isValid() ? occtColor(color.darker(170)) : kEdgeColor;
}

occ::handle<AIS_Shape> makeBody(const TopoDS_Shape& shape, BodyDisplay::Style style,
                                const QColor& color, VisualStyle visual, const QColor& paper) {
  occ::handle<AIS_Shape> body = new AIS_Shape(shape);
  if (style == BodyDisplay::Style::New) {
    body->SetColor(kPreviewColor);
    body->SetTransparency(0.5);
    return body;
  }
  body->SetColor(color.isValid() ? occtColor(color) : kBodyColor);
  const occ::handle<Prs3d_Drawer>& drawer = body->Attributes();
  switch (visual) {
  case VisualStyle::ShadedVisibleEdges:
    // Dark edges. Set after SetColor(), which recolours the
    // face boundaries too.
    drawer->SetFaceBoundaryDraw(true);
    drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(kEdgeColor, Aspect_TOL_SOLID, 1.0));
    break;
  case VisualStyle::Wireframe: {
    const Quantity_Color edges = wireColor(color);
    setEdgeStyle(body, edges, 1.0);
    // Edges only, no lines across the faces.
    drawer->SetUIsoAspect(new Prs3d_IsoAspect(edges, Aspect_TOL_SOLID, 1.0, 0));
    drawer->SetVIsoAspect(new Prs3d_IsoAspect(edges, Aspect_TOL_SOLID, 1.0, 0));
    break;
  }
  case VisualStyle::WireframeHiddenEdges:
    // The faces in the background's colour, unlit, as paper: they hide the
    // edges behind them, which are then drawn dashed. (OCCT draws
    // see-through faces after every layer that shares the depth buffer, so
    // invisible faces cannot hide edges.)
    body->SetColor(occtColor(paper));
    drawer->ShadingAspect()->Aspect()->SetShadingModel(Graphic3d_TypeOfShadingModel_Unlit);
    break;
  case VisualStyle::Shaded:
  case VisualStyle::ShadedHiddenEdges:
    break; // the hidden-edge style adds its edges separately
  }
  return body;
}

bool drawsHiddenEdges(VisualStyle style) {
  return style == VisualStyle::ShadedHiddenEdges || style == VisualStyle::WireframeHiddenEdges;
}

// The edges of a shape, each once.
TopoDS_Shape edgesOf(const TopoDS_Shape& shape) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  TopoDS_Compound compound;
  BRep_Builder builder;
  builder.MakeCompound(compound);
  for (int i = 1; i <= edges.Extent(); ++i) {
    builder.Add(compound, edges(i));
  }
  return compound;
}

// Rounds a camera vector for the log ("-0" as "0").
QString vectorText(const gp_XYZ& v) {
  QStringList parts;
  for (const double c : {v.X(), v.Y(), v.Z()}) {
    QString text = QString::number(std::round(c * 1000.0) / 1000.0, 'f', 3);
    while (text.endsWith(QLatin1Char('0'))) {
      text.chop(1);
    }
    if (text.endsWith(QLatin1Char('.'))) {
      text.chop(1);
    }
    parts << (text == QStringLiteral("-0") ? QStringLiteral("0") : text);
  }
  return parts.join(QLatin1Char(' '));
}

// A cube face, edge or corner by the directions it looks from ("front",
// "front-top", "front-right-top"), for the log.
QString cubeName(V3d_TypeOfOrientation orientation) {
  const gp_Dir d = V3d::GetProjAxis(orientation);
  QStringList parts;
  if (d.Y() < -1e-6) {
    parts << QStringLiteral("front");
  } else if (d.Y() > 1e-6) {
    parts << QStringLiteral("back");
  }
  if (d.X() > 1e-6) {
    parts << QStringLiteral("right");
  } else if (d.X() < -1e-6) {
    parts << QStringLiteral("left");
  }
  if (d.Z() > 1e-6) {
    parts << QStringLiteral("top");
  } else if (d.Z() < -1e-6) {
    parts << QStringLiteral("bottom");
  }
  return parts.join(QLatin1Char('-'));
}

occ::handle<AIS_Shape> makeProfile(const TopoDS_Shape& face) {
  occ::handle<AIS_Shape> profile = new AIS_Shape(face);
  profile->SetColor(kProfileFill);
  profile->SetTransparency(0.6);
  // In front of a body face the sketch lies on.
  profile->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, -1.0f,
                                                                      -2.0f);
  const occ::handle<Prs3d_Drawer>& drawer = profile->Attributes();
  drawer->SetFaceBoundaryDraw(true);
  drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(kSketchColor, Aspect_TOL_SOLID, 2.0));
  return profile;
}

} // namespace

bool sameTransformation(const gp_Trsf& a, const gp_Trsf& b) {
  for (int row = 1; row <= 3; ++row) {
    for (int column = 1; column <= 4; ++column) {
      if (std::abs(a.Value(row, column) - b.Value(row, column)) > 1e-12) {
        return false;
      }
    }
  }
  return true;
}

// Lets the view pick only what the active filter accepts, so that the
// highlight under the mouse already shows what a click would select.
class OcctViewer::Filter : public SelectMgr_Filter {
  DEFINE_STANDARD_RTTI_INLINE(OcctViewer::Filter, SelectMgr_Filter)

public:
  explicit Filter(const OcctViewer* viewer) : m_owner(viewer) {}

  bool IsOk(const occ::handle<SelectMgr_EntityOwner>& owner) const override {
    return m_owner->accepts(owner);
  }

private:
  const OcctViewer* m_owner;
};

OcctViewer::OcctViewer(QWidget* parent) : QOpenGLWidget(parent) {
  // The presentations' triangulations (BRepMesh through AIS) mesh the faces
  // of a shape on all cores instead of one after another (W1: a design of
  // many bodies displayed several times faster).
  BRepMesh_IncrementalMesh::SetParallelDefault(true);
  const occ::handle<Aspect_DisplayConnection> display = new Aspect_DisplayConnection();
  const occ::handle<OpenGl_GraphicDriver> driver = new OpenGl_GraphicDriver(display, false);
  driver->ChangeOptions().buffersNoSwap = true;      // Qt swaps the buffers
  driver->ChangeOptions().buffersOpaqueAlpha = true; // keep Qt's framebuffer opaque
  driver->ChangeOptions().useSystemBuffer = false;   // always render into FBOs
#ifdef Q_OS_MACOS
  // Qt's context is a core profile (macSurfaceFormat()); OCCT must not ask
  // for a compatibility one.
  driver->ChangeOptions().contextCompatible = false;
#endif

  m_viewer = new V3d_Viewer(driver);
  // A key light from the upper left of the camera instead of OCCT's default
  // headlight, which lights all three faces of an isometric view equally.
  m_viewer->AddLight(new V3d_AmbientLight(Quantity_Color(0.35, 0.35, 0.35, Quantity_TOC_RGB)));
  m_viewer->AddLight(new V3d_DirectionalLight(gp_Dir(0.3, -0.5, -1.0), Quantity_NOC_WHITE, true));
  m_viewer->SetLightOn();
  // The layout grid under everything else: drawn first, without depth.
  Graphic3d_ZLayerSettings underlay;
  underlay.SetName("layout grid");
  underlay.SetEnableDepthTest(false);
  underlay.SetEnableDepthWrite(false);
  m_viewer->InsertLayerBefore(m_gridLayer, underlay, Graphic3d_ZLayerId_Default);
  // The hidden-edge styles: edges behind faces are drawn dashed without a
  // depth test, then the edges in sight over them against the faces' depth.
  // Depth writes stay on (OpenGL writes no depth without the test anyway):
  // layers that share the depth buffer and its writes are one group for
  // picking, so highlights in the top layer do not win over closer items.
  Graphic3d_ZLayerSettings hidden;
  hidden.SetName("hidden edges");
  hidden.SetEnableDepthTest(false);
  hidden.SetClearDepth(false);
  m_viewer->InsertLayerAfter(m_hiddenLayer, hidden, Graphic3d_ZLayerId_Default);
  Graphic3d_ZLayerSettings inSight;
  inSight.SetName("visible edges");
  inSight.SetClearDepth(false);
  m_viewer->InsertLayerAfter(m_edgeLayer, inSight, m_hiddenLayer);

  m_context = makePickContext(m_viewer);
  m_context->SetPixelTolerance(qRound(kPickTolerance * devicePixelRatioF()));
  m_pickFilter = new Filter(this);
  m_context->AddFilter(m_pickFilter);
  setHoverColor(QColor(0x2f, 0x9b, 0xff));
  m_grid = new LayoutGrid();
  m_grid->SetZLayer(m_gridLayer);
  setGridColors(*m_grid, m_paper);

  m_orientationCube = new AIS_ViewCube();
  m_orientationCube->SetViewAnimation(ViewAnimation());
  m_orientationCube->SetFixedAnimationLoop(false);
  m_orientationCube->SetAutoStartAnimation(true);
  // In the upper right corner, with its labels in capitals.
  m_orientationCube->SetTransformPersistence(cubePersistence(1.0));
  m_cubeBaseSize = m_orientationCube->Size();
  m_cubeBaseFont = m_orientationCube->FontHeight();
  for (const auto& [side, label] :
       {std::pair{V3d_Yneg, "FRONT"}, std::pair{V3d_Ypos, "BACK"}, std::pair{V3d_Xneg, "LEFT"},
        std::pair{V3d_Xpos, "RIGHT"}, std::pair{V3d_Zpos, "TOP"}, std::pair{V3d_Zneg, "BOTTOM"}}) {
    m_orientationCube->SetBoxSideLabel(side, label);
  }

  m_restTimer = new QTimer(this);
  m_restTimer->setSingleShot(true);
  m_restTimer->setInterval(kRestMilliseconds);
  connect(m_restTimer, &QTimer::timeout, this, &OcctViewer::logCamera);
  TestSync::watch(m_restTimer);
  m_scrollTimer = new QTimer(this);
  m_scrollTimer->setSingleShot(true);
  m_scrollTimer->setInterval(kScrollEndMilliseconds);
  connect(m_scrollTimer, &QTimer::timeout, this, &OcctViewer::endScrollDrag);
  const bool glass = chromeStyle() == ChromeStyle::Floating;
  m_homeButton = makeOverlayButton(this);
  m_homeButton->setObjectName(QStringLiteral("viewHome"));
  m_homeButton->setIcon(overlayIcon(QStringLiteral("home"), IconBackdrop::Light));
  m_homeButton->setIconSize(glass ? QSize(15, 15) : QSize(18, 18));
  m_homeButton->setFixedSize(glass ? QSize(30, 30) : QSize(26, 26));
  m_homeButton->setAutoRaise(true);
  m_homeButton->setFocusPolicy(Qt::NoFocus);
  m_homeButton->setToolTip(tr("Home: the home view of the whole design"));
  connect(m_homeButton, &QToolButton::clicked, this, [this] {
    qDebug() << "View home";
    goHome();
  });
  // The cube's arrows (P9): triangles to the faces beside the one in
  // sight, curved arrows that roll the view; drawn as icons (see
  // updateOverlayIcons), not as text glyphs, so that they are sharp on any
  // screen and follow the view's background.
  for (const auto& [id, tip] :
       {std::pair{"up", QT_TR_NOOP("Turn to the face above")},
        std::pair{"down", QT_TR_NOOP("Turn to the face below")},
        std::pair{"left", QT_TR_NOOP("Turn to the face on the left")},
        std::pair{"right", QT_TR_NOOP("Turn to the face on the right")},
        std::pair{"cw", QT_TR_NOOP("Roll the view 90 degrees clockwise")},
        std::pair{"ccw", QT_TR_NOOP("Roll the view 90 degrees counter-clockwise")}}) {
    QToolButton* arrow = makeOverlayButton(this);
    const QString name = QString::fromLatin1(id);
    arrow->setObjectName(QStringLiteral("orientationCube_") + name);
    arrow->setIcon(overlayIcon(name, IconBackdrop::Light));
    arrow->setIconSize(glass ? QSize(12, 12) : QSize(16, 16));
    arrow->setToolTip(tr(tip));
    arrow->setFixedSize(glass ? QSize(26, 26) : QSize(22, 22));
    arrow->setAutoRaise(true);
    arrow->setFocusPolicy(Qt::NoFocus);
    arrow->hide();
    connect(arrow, &QToolButton::clicked, this, [this, name] { turnCube(name); });
    m_cubeArrows.emplace_back(name, arrow);
  }
  applyDevicePixelRatio(); // the ratio of the screen the view starts on

  m_view = m_viewer->CreateView();
  m_view->SetImmediateUpdate(false);
  m_view->ChangeRenderingParams().NbMsaaSamples = 4;
  m_view->SetBgGradientColors(Quantity_Color(0.93, 0.94, 0.96, Quantity_TOC_RGB),
                              Quantity_Color(0.74, 0.77, 0.82, Quantity_TOC_RGB),
                              Aspect_GradientFillMethod_Vertical);
  m_view->SetProj(V3d_TypeOfOrientation_Zup_AxoRight);

  bindGestures();
  // The view reports picks and keeps no selection of its own, so every pick
  // replaces OCCT's selection; the owner of the view decides what a pick
  // with Ctrl or Shift does. A click on the orientation cube is a pick of it.
  auto& schemes = ChangeMouseSelectionSchemes();
  schemes.Clear();
  for (const Aspect_VKeyFlags keys : std::initializer_list<Aspect_VKeyFlags>{
           Aspect_VKeyFlags_NONE, Aspect_VKeyFlags_CTRL, Aspect_VKeyFlags_SHIFT, kCubeDrag}) {
    schemes.Bind(Aspect_VKeyMouse_LeftButton | keys, AIS_SelectionScheme_Replace);
  }
  // Object dragging is meant for manipulators, which the view does not have
  // yet. Left unbound it turns a left drag into a view rotation that OCCT 8.0
  // starts without an initial camera state and fails (gp_Dir::Cross).
  SetAllowDragging(false);

  setMouseTracking(true);
  setFocusPolicy(Qt::StrongFocus);
  setUpdateBehavior(QOpenGLWidget::NoPartialUpdate);

#ifdef Q_OS_MACOS
  // macOS offers only a legacy 2.1 or a forward-compatible core 3.2-4.1
  // context, and OCCT needs shaders: core 4.1, as OCCT's own Qt sample asks
  // (a 4.5 request is mapped to 4.1 there). No DeprecatedFunctions: that
  // would drop the forward-compatible flag, which macOS requires.
  setFormat(macSurfaceFormat());
#else
  // Compatibility profile on purpose: with a core profile OCCT 8.0 reads the
  // window buffer bits through XGetWindowAttributes() on the current GLX
  // drawable, which under Qt 6 is an offscreen pbuffer, and crashes.
  // DeprecatedFunctions stops Qt from requesting a forward-compatible context,
  // which Mesa turns into a core profile.
  QSurfaceFormat format;
  format.setDepthBufferSize(24);
  format.setStencilBufferSize(8);
  format.setVersion(4, 5);
  format.setProfile(QSurfaceFormat::CompatibilityProfile);
  format.setOption(QSurfaceFormat::DeprecatedFunctions);
  setFormat(format);
#endif
}

OcctViewer::~OcctViewer() {
  // Keep the X11 display connection alive until Qt's own context is current
  // again; releasing it earlier can crash in ~QOpenGLWidget().
  occ::handle<Aspect_DisplayConnection> display = m_viewer->Driver()->GetDisplayConnection();

  m_context->RemoveFilters();
  m_context->RemoveAll(false);
  m_context.Nullify();
  m_view->Remove();
  m_view.Nullify();
  m_viewer.Nullify();

  makeCurrent();
  display.Nullify();
}

// ---------------------------------------------------------------------------
// Scene

// OCCT 8.0 quirk: an object displayed without a selection mode (-1) and
// activated later with Activate() is never picked. Pickable objects are
// therefore displayed with a selection mode, and modes are switched with
// SetSelectionModeActive(..., Multiple), which keeps them pickable.
void OcctViewer::display(const occ::handle<AIS_Shape>& object, int displayMode, PickInfo info,
                         bool pickable) {
  if (!pickable) {
    m_context->Display(object, displayMode, -1, false);
    return;
  }
  const bool isBody = info.body != nullptr;
  m_pickInfo[object.get()] = std::move(info);
  m_context->Display(object, displayMode, isBody ? AIS_Shape::SelectionMode(TopAbs_FACE) : 0,
                     false);
  activatePicking(object);
}

void OcctViewer::remove(const occ::handle<AIS_Shape>& object) {
  m_pickInfo.erase(object.get());
  m_context->Remove(object, false);
}

void OcctViewer::activatePicking(const occ::handle<AIS_Shape>& object) {
  const auto info = m_pickInfo.find(object.get());
  if (info == m_pickInfo.end()) {
    return;
  }
  m_context->Deactivate(object);
  // Pick widths in device pixels (PickContext.hpp): edges and vertices
  // wider than faces.
  const qreal ratio = devicePixelRatioF();
  const auto activate = [this, &object, ratio](int mode, int sensitivity) {
    m_context->SetSelectionModeActive(object, mode, true, AIS_SelectionModesConcurrency_Multiple);
    const int pixels = qRound(sensitivity * ratio);
    const occ::handle<SelectMgr_Selection>& selection = object->Selection(mode);
    if (!selection.IsNull() && selection->Sensitivity() != pixels) {
      m_context->SetSelectionSensitivity(object, mode, pixels);
    }
  };
  if (info->second.body) {
    if (m_filter.testFlag(SelectKind::Body)) {
      activate(0, kFaceSensitivity);
    }
    if (m_filter.testFlag(SelectKind::Face)) {
      activate(AIS_Shape::SelectionMode(TopAbs_FACE), kFaceSensitivity);
    }
    if (m_filter.testFlag(SelectKind::Edge)) {
      activate(AIS_Shape::SelectionMode(TopAbs_EDGE), kEdgeSensitivity);
    }
    if (m_filter.testFlag(SelectKind::Vertex)) {
      activate(AIS_Shape::SelectionMode(TopAbs_VERTEX), kVertexSensitivity);
    }
  } else if (info->second.highlight || m_filter.testFlag(info->second.item.kind)) {
    // Highlights stay pickable, so a click on a selected item deselects it;
    // the filter decides whether the active input takes it. Curves and
    // points (sketch geometry, axes) are as wide as a body's edges and
    // vertices, which may lie under them.
    int sensitivity = kFaceSensitivity;
    bool outline = false;
    if (!object->Shape().IsNull()) {
      switch (object->Shape().ShapeType()) {
      case TopAbs_VERTEX:
        sensitivity = kVertexSensitivity;
        break;
      case TopAbs_EDGE:
      case TopAbs_WIRE:
        sensitivity = kEdgeSensitivity;
        break;
      case TopAbs_COMPOUND:
        // A text's outline: edges only.
        outline = !TopExp_Explorer(object->Shape(), TopAbs_FACE).More() &&
                  TopExp_Explorer(object->Shape(), TopAbs_EDGE).More();
        sensitivity = outline ? kEdgeSensitivity : kFaceSensitivity;
        break;
      default:
        break;
      }
    }
    activate(0, sensitivity);
    if (outline) {
      // OCCT ranks a compound after the edges at the same depth, so a curve
      // within its width beside a text (the text's frame) won over the text
      // under the cursor. The outline ranks as an edge: the nearer is picked.
      const occ::handle<SelectMgr_Selection>& selection = object->Selection(0);
      if (!selection.IsNull()) {
        const TopExp_Explorer edge(object->Shape(), TopAbs_EDGE);
        const int priority = StdSelect_BRepSelectionTool::GetStandardPriority(edge.Current(), TopAbs_SHAPE);
        for (const occ::handle<SelectMgr_SensitiveEntity>& entity : selection->Entities()) {
          entity->BaseSensitive()->OwnerId()->SetPriority(priority);
        }
      }
    }
  }
}

void OcctViewer::activateAllPicking() {
  for (const auto& [uid, body] : m_bodies) {
    activatePicking(body.object);
  }
  for (const auto& [key, profile] : m_profiles) {
    activatePicking(profile);
  }
  for (const auto* objects : {&m_sketchEntities, &m_datums, &m_highlights}) {
    for (const occ::handle<AIS_Shape>& object : *objects) {
      activatePicking(object);
    }
  }
}

void OcctViewer::setBodies(const std::vector<BodyDisplay>& bodies) {
  ScopedTiming timing("display");
  int shown = 0;
  const bool wasEmpty = m_bodies.empty();
  std::map<std::string, BodyEntry> kept;
  for (const BodyDisplay& body : bodies) {
    if (!body.shape) {
      continue;
    }
    const std::string key =
        body.occurrence.empty() ? body.uid : body.uid + "@" + body.occurrence;
    // A placed body is the body's own shape with the occurrence's
    // transformation on the object, so that picks find its named faces.
    const TopoDS_Shape& shape = body.shape->occt();
    auto existing = m_bodies.find(key);
    if (existing != m_bodies.end() && existing->second.style == body.style &&
        existing->second.object->Shape().IsEqual(shape) &&
        sameTransformation(existing->second.placement.Transformation(), body.placement.Transformation()) &&
        existing->second.color == body.color) {
      BodyEntry entry = existing->second;
      entry.shape = body.shape; // the same geometry may come with new names
      const auto info = m_pickInfo.find(entry.object.get());
      if (info != m_pickInfo.end()) {
        info->second.body = body.shape;
      }
      m_bodies.erase(existing);
      kept.emplace(key, entry);
      continue;
    }
    if (existing != m_bodies.end()) {
      removeBody(existing->second);
      m_bodies.erase(existing);
    }
    BodyEntry entry;
    entry.uid = body.uid;
    entry.occurrence = body.occurrence;
    entry.shape = body.shape;
    entry.style = body.style;
    entry.placement = body.placement;
    entry.color = body.color;
    showBody(entry);
    ++shown;
    kept.emplace(key, entry);
  }
  timing.setDetail(QStringLiteral("%1 bodies, %2 new").arg(bodies.size()).arg(shown));
  for (auto& [uid, entry] : m_bodies) {
    removeBody(entry);
  }
  m_bodies = std::move(kept);
  if (wasEmpty && !m_bodies.empty()) {
    m_fitPending = true;
  }
  sceneChanged();
}

void OcctViewer::showBody(BodyEntry& entry) {
  const TopoDS_Shape& shape = entry.shape->occt();
  entry.object = makeBody(shape, entry.style, entry.color, m_visualStyle, m_paper);
  const bool placed = !entry.placement.IsIdentity();
  if (placed) {
    entry.object->SetLocalTransformation(entry.placement.Transformation());
  }
  PickInfo info;
  info.item = {SelectKind::Body, QString::fromStdString(entry.uid), QString(), QString()};
  info.item.occurrence = QString::fromStdString(entry.occurrence);
  info.body = entry.shape;
  const bool normal = entry.style == BodyDisplay::Style::Normal;
  const bool wire = normal && m_visualStyle == VisualStyle::Wireframe;
  display(entry.object, wire ? AIS_WireFrame : AIS_Shaded, std::move(info), normal);
  entry.hiddenEdges.Nullify();
  entry.visibleEdges.Nullify();
  if (!normal || !drawsHiddenEdges(m_visualStyle)) {
    return;
  }
  const TopoDS_Shape edges = edgesOf(shape);
  entry.hiddenEdges = new AIS_Shape(edges);
  setEdgeStyle(entry.hiddenEdges, kHiddenEdgeColor, 1.0, Aspect_TOL_DASH);
  entry.visibleEdges = new AIS_Shape(edges);
  setEdgeStyle(entry.visibleEdges,
               m_visualStyle == VisualStyle::WireframeHiddenEdges ? wireColor(entry.color) : kEdgeColor,
               1.0);
  for (const auto& [object, layer] :
       {std::pair{entry.hiddenEdges, m_hiddenLayer}, std::pair{entry.visibleEdges, m_edgeLayer}}) {
    if (placed) {
      object->SetLocalTransformation(entry.placement.Transformation());
    }
    m_context->SetZLayer(object, layer);
    m_context->Display(object, AIS_WireFrame, -1, false);
  }
}

void OcctViewer::removeBody(BodyEntry& entry) {
  remove(entry.object);
  for (occ::handle<AIS_Shape>* edges : {&entry.hiddenEdges, &entry.visibleEdges}) {
    if (!edges->IsNull()) {
      m_context->Remove(*edges, false);
      edges->Nullify();
    }
  }
}

void OcctViewer::setVisualStyle(VisualStyle style) {
  if (style == m_visualStyle) {
    return;
  }
  m_visualStyle = style;
  for (auto& [key, entry] : m_bodies) {
    removeBody(entry);
    showBody(entry);
  }
  sceneChanged();
}

void OcctViewer::setProfiles(const std::vector<std::pair<ProfileKey, TopoDS_Shape>>& profiles,
                             const std::map<std::string, QString>& occurrences) {
  ScopedTiming timing("display profiles");
  // A profile whose face is the one shown keeps its display (the main
  // window passes the same face while the region stays the same).
  std::map<ProfileKey, occ::handle<AIS_Shape>> shown = std::move(m_profiles);
  m_profiles.clear();
  int made = 0;
  for (const auto& [key, face] : profiles) {
    const auto placed = occurrences.find(key.first);
    const QString occurrence = placed != occurrences.end() ? placed->second : QString();
    const auto existing = shown.find(key);
    if (existing != shown.end() && existing->second->Shape().IsEqual(face)) {
      const auto info = m_pickInfo.find(existing->second.get());
      if (info != m_pickInfo.end()) {
        info->second.item.occurrence = occurrence;
      }
      m_profiles.emplace(key, existing->second);
      shown.erase(existing);
      continue;
    }
    ++made;
    occ::handle<AIS_Shape> profile = makeProfile(face);
    PickInfo info;
    info.item = {SelectKind::Profile, QString::fromStdString(key.first),
                 QString::fromStdString(key.second), QStringLiteral("plane")};
    info.item.occurrence = occurrence;
    display(profile, AIS_Shaded, std::move(info), true);
    m_profiles.emplace(key, profile);
  }
  for (const auto& [key, profile] : shown) {
    remove(profile);
  }
  timing.setDetail(QStringLiteral("%1 profiles, %2 new").arg(profiles.size()).arg(made));
  sceneChanged();
}

void OcctViewer::setSketchEntities(const std::vector<SketchEntityDisplay>& entities) {
  ScopedTiming timing("display sketch entities");
  // An entity with the signature it was shown with keeps its display.
  std::map<QString, std::pair<QString, occ::handle<AIS_Shape>>> shown = std::move(m_sketchEntityKeys);
  m_sketchEntityKeys.clear();
  m_sketchEntities.clear();
  int made = 0;
  for (const SketchEntityDisplay& entity : entities) {
    if (entity.shape.IsNull()) {
      continue;
    }
    const QString key = QStringLiteral("%1/%2@%3").arg(entity.sketch, entity.id, entity.occurrence);
    const auto existing = shown.find(key);
    if (existing != shown.end() && !entity.signature.isEmpty() &&
        existing->second.first == entity.signature) {
      m_sketchEntities.push_back(existing->second.second);
      m_sketchEntityKeys.emplace(key, existing->second);
      shown.erase(existing);
      continue;
    }
    ++made;
    occ::handle<AIS_Shape> object = new AIS_Shape(entity.shape);
    const bool isPoint = entity.geometry == QStringLiteral("point");
    const Quantity_Color& color = entity.reference      ? kReferenceColor
                                  : entity.construction ? kConstructionColor
                                  : entity.centerline   ? kCenterlineColor
                                  : entity.constrained  ? kConstrainedColor
                                                        : kSketchColor;
    if (isPoint) {
      setPointStyle(object, color, Aspect_TOM_O_POINT, 1.0);
    } else {
      setEdgeStyle(object, color, 2.0,
                   entity.construction ? Aspect_TOL_DASH
                   : entity.centerline ? Aspect_TOL_DOTDASH
                                       : Aspect_TOL_SOLID);
    }
    PickInfo info;
    info.item = {isPoint ? SelectKind::SketchPoint : SelectKind::SketchCurve, entity.sketch,
                 entity.id, entity.geometry};
    info.item.occurrence = entity.occurrence;
    display(object, AIS_WireFrame, std::move(info), true);
    // Over the faces a sketch lies on.
    m_context->SetZLayer(object, Graphic3d_ZLayerId_Top);
    m_sketchEntities.push_back(object);
    // A key shown twice (not expected) keeps the first in the map; the
    // other is taken away with the rest below.
    if (!m_sketchEntityKeys.emplace(key, std::make_pair(entity.signature, object)).second) {
      shown.emplace(key + QStringLiteral("#%1").arg(made), std::make_pair(QString(), object));
      m_sketchEntities.pop_back();
    }
  }
  for (const auto& [key, entry] : shown) {
    remove(entry.second);
  }
  timing.setDetail(QStringLiteral("%1 entities, %2 new").arg(entities.size()).arg(made));
  sceneChanged();
}

void OcctViewer::setDatums(const std::vector<DatumDisplay>& datums) {
  ScopedTiming timing("display datums");
  timing.setDetail(QStringLiteral("%1 datums").arg(datums.size()));
  for (const occ::handle<AIS_Shape>& datum : m_datums) {
    remove(datum);
  }
  m_datums.clear();
  for (const DatumDisplay& datum : datums) {
    if (datum.shape.IsNull()) {
      continue;
    }
    occ::handle<AIS_Shape> object = new AIS_Shape(datum.shape);
    int mode = AIS_WireFrame;
    switch (datum.kind) {
    case SelectKind::Plane: {
      object->SetColor(kPlaneColor);
      object->SetTransparency(0.75);
      const occ::handle<Prs3d_Drawer>& drawer = object->Attributes();
      drawer->SetFaceBoundaryDraw(true);
      drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(kAxisColor, Aspect_TOL_SOLID, 1.0));
      mode = AIS_Shaded;
      break;
    }
    case SelectKind::Axis:
      setEdgeStyle(object, kAxisColor, 2.0, Aspect_TOL_DOTDASH);
      break;
    default:
      setPointStyle(object, kAxisColor, Aspect_TOM_BALL, 1.5);
      break;
    }
    PickInfo info;
    info.item = {datum.kind, datum.uid, QString(), SelectionItem::kindName(datum.kind)};
    info.item.occurrence = datum.occurrence;
    display(object, mode, std::move(info), true);
    m_datums.push_back(object);
  }
  sceneChanged();
}

void OcctViewer::setHighlights(const std::vector<HighlightDisplay>& highlights) {
  for (const occ::handle<AIS_Shape>& highlight : m_highlights) {
    remove(highlight);
  }
  m_highlights.clear();
  for (const HighlightDisplay& highlight : highlights) {
    if (highlight.shape.IsNull()) {
      continue;
    }
    occ::handle<AIS_Shape> object = new AIS_Shape(highlight.shape);
    const Quantity_Color color = occtColor(highlight.color);
    int mode = AIS_WireFrame;
    switch (highlight.item.kind) {
    case SelectKind::Face:
    case SelectKind::Feature: // the faces it made (mitcad#28)
    case SelectKind::Profile:
    case SelectKind::Plane:
    case SelectKind::Body:
    case SelectKind::Component: {
      object->SetColor(color);
      const bool whole =
          highlight.item.kind == SelectKind::Body || highlight.item.kind == SelectKind::Component;
      object->SetTransparency(whole ? 0.55 : 0.35);
      // Drawn just in front of the face it covers.
      object->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, -1.0f,
                                                                         -4.0f);
      const occ::handle<Prs3d_Drawer>& drawer = object->Attributes();
      drawer->SetFaceBoundaryDraw(true);
      drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(color, Aspect_TOL_SOLID, 2.0));
      mode = AIS_Shaded;
      break;
    }
    case SelectKind::Vertex:
    case SelectKind::SketchPoint:
    case SelectKind::Point:
      setPointStyle(object, color, Aspect_TOM_BALL, 1.5);
      break;
    default:
      setEdgeStyle(object, color, 3.5);
      break;
    }
    PickInfo info;
    info.item = highlight.item;
    info.highlight = true;
    display(object, mode, std::move(info), true);
    m_context->SetZLayer(object, Graphic3d_ZLayerId_Top);
    m_highlights.push_back(object);
  }
  sceneChanged();
}

void OcctViewer::setPreview(const TopoDS_Shape& shape, PreviewStyle style) {
  if (!m_preview.IsNull()) {
    m_context->Remove(m_preview, false);
    m_preview.Nullify();
  }
  if (!shape.IsNull()) {
    m_preview = new AIS_Shape(shape);
    if (style == PreviewStyle::Failed) {
      setEdgeStyle(m_preview, kCutPreviewColor, 2.0, Aspect_TOL_DASH);
      m_context->Display(m_preview, AIS_WireFrame, -1, false);
    } else {
      m_preview->SetColor(style == PreviewStyle::Remove ? kCutPreviewColor : kPreviewColor);
      m_preview->SetTransparency(0.5);
      // A Join/Cut tool shares faces with the previewed body (the walls of a
      // cut hole); pull it towards the viewer so it wins the depth test there.
      m_preview->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, -1.0f,
                                                                            -2.0f);
      m_context->Display(m_preview, AIS_Shaded, -1, false);
    }
  }
  sceneChanged();
}

void OcctViewer::setSketchPreview(const TopoDS_Shape& shape) {
  if (!m_sketchPreview.IsNull()) {
    m_context->Remove(m_sketchPreview, false);
    m_sketchPreview.Nullify();
  }
  if (!shape.IsNull()) {
    m_sketchPreview = new AIS_Shape(shape);
    m_sketchPreview->SetColor(kSketchColor);
    m_sketchPreview->SetWidth(2.0);
    m_context->Display(m_sketchPreview, AIS_WireFrame, -1, false);
    m_context->SetZLayer(m_sketchPreview, Graphic3d_ZLayerId_Top);
  }
  sceneChanged();
}

// ---------------------------------------------------------------------------
// Picking

void OcctViewer::setPickFilter(SelectFilter filter, std::function<bool(const SelectionItem&)> test) {
  m_filter = filter;
  m_filterTest = std::move(test);
  m_context->ClearSelected(false);
  m_context->ClearDetected(false);
  activateAllPicking();
  sceneChanged();
}

void OcctViewer::setHoverColor(const QColor& color) {
  for (const Prs3d_TypeOfHighlight type :
       {Prs3d_TypeOfHighlight_Dynamic, Prs3d_TypeOfHighlight_LocalDynamic}) {
    const occ::handle<Prs3d_Drawer>& style = m_context->HighlightStyle(type);
    style->SetColor(occtColor(color));
    style->SetDisplayMode(AIS_Shaded);
  }
  sceneChanged();
}

std::optional<SelectionItem>
OcctViewer::itemFor(const occ::handle<SelectMgr_EntityOwner>& owner) const {
  if (owner.IsNull()) {
    return std::nullopt;
  }
  const auto info = m_pickInfo.find(
      dynamic_cast<const AIS_InteractiveObject*>(owner->Selectable().get()));
  if (info == m_pickInfo.end()) {
    return std::nullopt;
  }
  if (!info->second.body) {
    return info->second.item;
  }
  // A face, edge or vertex of a body is named through the body's shape; a
  // pick of the whole body is the body.
  const geometry::Shape& body = *info->second.body;
  SelectionItem item = info->second.item;
  const occ::handle<StdSelect_BRepOwner> brep = occ::handle<StdSelect_BRepOwner>::DownCast(owner);
  if (brep.IsNull() || !brep->HasShape()) {
    return item;
  }
  const TopoDS_Shape& part = brep->Shape();
  switch (part.ShapeType()) {
  case TopAbs_FACE: {
    const geometry::NameList& names = body.names_of_face(part);
    if (names.empty()) {
      return std::nullopt; // cannot be referred to
    }
    item.kind = SelectKind::Face;
    item.name = QString::fromStdString(names.front());
    item.geometry = surfaceType(part);
    return item;
  }
  case TopAbs_EDGE: {
    const auto name = body.name_of_edge(part);
    if (!name) {
      return std::nullopt;
    }
    item.kind = SelectKind::Edge;
    item.name = QString::fromStdString(*name);
    item.geometry = curveType(part);
    return item;
  }
  case TopAbs_VERTEX: {
    const auto name = body.name_of_vertex(part);
    if (!name) {
      return std::nullopt;
    }
    item.kind = SelectKind::Vertex;
    item.name = QString::fromStdString(*name);
    item.geometry = QStringLiteral("point");
    return item;
  }
  default:
    return item;
  }
}

std::optional<std::array<double, 3>>
OcctViewer::pickedPoint(const occ::handle<SelectMgr_EntityOwner>& owner) const {
  const occ::handle<StdSelect_ViewerSelector3d>& selector = m_context->MainSelector();
  for (int rank = 1; rank <= selector->NbPicked(); ++rank) {
    if (selector->Picked(rank) != owner) {
      continue;
    }
    gp_Pnt point = selector->PickedPoint(rank);
    // In the component's coordinates when an occurrence places the body.
    if (owner->HasSelectable() && owner->Selectable()->HasTransformation()) {
      gp_Trsf inverse = owner->Selectable()->Transformation();
      inverse.Invert();
      point.Transform(inverse);
    }
    return std::array<double, 3>{point.X(), point.Y(), point.Z()};
  }
  return std::nullopt;
}

std::vector<std::pair<SelectionItem, QPoint>> OcctViewer::pickPoints(SelectFilter kinds) {
  std::vector<std::pair<SelectionItem, QPoint>> found;
  if (m_view->Window().IsNull()) {
    return found;
  }
  // The middle of a face's largest triangle lies on the face.
  const auto middleOf = [](const TopoDS_Face& face) -> std::optional<gp_Pnt> {
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull()) {
      return std::nullopt;
    }
    double largest = -1.0;
    gp_Pnt middle;
    for (int t = 1; t <= mesh->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      mesh->Triangle(t).Get(a, b, c);
      const gp_Pnt pa = mesh->Node(a);
      const gp_Pnt pb = mesh->Node(b);
      const gp_Pnt pc = mesh->Node(c);
      const double area = gp_Vec(pa, pb).Crossed(gp_Vec(pa, pc)).Magnitude();
      if (area > largest) {
        largest = area;
        middle = gp_Pnt((pa.XYZ() + pb.XYZ() + pc.XYZ()) / 3.0);
      }
    }
    if (largest <= 0.0) {
      return std::nullopt;
    }
    return middle.Transformed(location.Transformation());
  };
  // The point at the middle of a face's parameter range, when it is on the
  // face.
  const auto parameterMiddleOf = [](const TopoDS_Face& face) -> std::optional<gp_Pnt> {
    double u1 = 0.0;
    double u2 = 0.0;
    double v1 = 0.0;
    double v2 = 0.0;
    BRepTools::UVBounds(face, u1, u2, v1, v2);
    const gp_Pnt2d uv((u1 + u2) / 2.0, (v1 + v2) / 2.0);
    if (BRepClass_FaceClassifier(face, uv, Precision::Confusion()).State() != TopAbs_IN) {
      return std::nullopt;
    }
    return BRepAdaptor_Surface(face).Value(uv.X(), uv.Y());
  };
  const QRect inside = rect().adjusted(4, 4, -4, -4);
  if (kinds.testFlag(SelectKind::Profile)) {
    for (const auto& [key, profile] : m_profiles) {
      for (TopExp_Explorer faces(profile->Shape(), TopAbs_FACE); faces.More(); faces.Next()) {
        const auto middle = middleOf(TopoDS::Face(faces.Current()));
        if (!middle) {
          continue;
        }
        const QPointF at = toWidgetF(*middle);
        if (!inside.contains(at.toPoint()) || isCovered(at)) {
          continue;
        }
        const NCollection_Vec2<int> pixel = toDevicePixels(at);
        m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
        const auto item = itemFor(m_context->DetectedOwner());
        if (item && item->kind == SelectKind::Profile && item->owner.toStdString() == key.first &&
            item->name.toStdString() == key.second && accepts(m_context->DetectedOwner())) {
          found.emplace_back(*item, at.toPoint());
          break;
        }
      }
    }
  }
  for (const auto& [key, entry] : m_bodies) {
    if (entry.style != BodyDisplay::Style::Normal || !entry.shape) {
      continue;
    }
    const geometry::Shape& body = *entry.shape;
    const gp_Trsf placement = entry.placement.Transformation();
    // Whether a click at the point picks the named item.
    const auto probe = [&](const gp_Pnt& local, SelectKind kind, const std::string& name) {
      const QPointF at = toWidgetF(local.Transformed(placement));
      if (!inside.contains(at.toPoint()) || isCovered(at)) {
        return false;
      }
      const NCollection_Vec2<int> pixel = toDevicePixels(at);
      m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
      const auto item = itemFor(m_context->DetectedOwner());
      if (item && item->kind == kind && item->name.toStdString() == name &&
          accepts(m_context->DetectedOwner())) {
        found.emplace_back(*item, at.toPoint());
        return true;
      }
      return false;
    };
    if (kinds.testFlag(SelectKind::Face)) {
      for (int i = 0; i < body.face_count(); ++i) {
        if (body.face_names(i).empty()) {
          continue;
        }
        // The middle of the face's parameters first: across a narrow face
        // (a fillet's strip) the middle, where neither edge beside it is
        // nearer (mitcad#29).
        const TopoDS_Face& face = body.face(i);
        if (const auto middle = parameterMiddleOf(face);
            middle && probe(*middle, SelectKind::Face, body.face_names(i).front())) {
          continue;
        }
        if (const auto middle = middleOf(face)) {
          probe(*middle, SelectKind::Face, body.face_names(i).front());
        }
      }
    }
    if (kinds.testFlag(SelectKind::Edge)) {
      for (int i = 0; i < body.edge_count(); ++i) {
        const BRepAdaptor_Curve curve(body.edge(i));
        probe(curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0), SelectKind::Edge,
              body.edge_name(i));
      }
    }
    if (kinds.testFlag(SelectKind::Vertex)) {
      for (int i = 0; i < body.vertex_count(); ++i) {
        probe(BRep_Tool::Pnt(body.vertex(i)), SelectKind::Vertex, body.vertex_name(i));
      }
    }
  }
  m_context->ClearDetected(false);
  sceneChanged();
  return found;
}

bool OcctViewer::accepts(const occ::handle<SelectMgr_EntityOwner>& owner) const {
  if (owner.IsNull()) {
    return false;
  }
  if (owner->Selectable() == m_orientationCube) {
    return true;
  }
  const auto item = itemFor(owner);
  if (!item || !m_filter.testFlag(item->kind)) {
    return false;
  }
  return !m_filterTest || m_filterTest(*item);
}

void OcctViewer::OnSelectionChanged(const occ::handle<AIS_InteractiveContext>& context,
                                    const occ::handle<V3d_View>& view) {
  AIS_ViewController::OnSelectionChanged(context, view);
  if (m_cubeClick) {
    m_cubeClick = false;
    m_context->ClearSelected(false);
    return;
  }
  Selection items;
  for (m_context->InitSelected(); m_context->MoreSelected(); m_context->NextSelected()) {
    if (auto item = itemFor(m_context->SelectedOwner())) {
      if (!m_windowPick) {
        item->at = pickedPoint(m_context->SelectedOwner());
      }
      if (!items.contains(*item)) {
        items.append(*item);
      }
    }
  }
  m_context->ClearSelected(false);
  m_context->MainSelector()->AllowOverlapDetection(false);
  emit picked(items, m_pressModifiers, m_windowPick);
}

// ---------------------------------------------------------------------------
// Camera

void OcctViewer::lookAtSketchPlane(bool saveCamera) {
  if (saveCamera || m_savedCamera.IsNull()) {
    m_savedCamera = new Graphic3d_Camera();
    m_savedCamera->Copy(m_view->Camera());
  }
  const gp_Dir& normal = m_sketchPlane.Direction();
  const gp_Dir& up = m_sketchPlane.YDirection();
  m_view->SetProj(normal.X(), normal.Y(), normal.Z());
  m_view->SetUp(up.X(), up.Y(), up.Z());
  sceneChanged();
}

void OcctViewer::restoreCamera() {
  if (!m_savedCamera.IsNull()) {
    m_view->Camera()->Copy(m_savedCamera);
    m_savedCamera.Nullify();
  }
  sceneChanged();
}

void OcctViewer::fitAll() {
  m_fitPending = true;
  requestFrame();
}

void OcctViewer::lookAlong(const gp_Dir& normal, const gp_Dir& up) { turnTo(normal.Reversed(), up); }

void OcctViewer::lookHome() { goHome(); }

void OcctViewer::fitCamera(const occ::handle<Graphic3d_Camera>& fitted) const {
  // What FitAll fits: the shown objects, without the orientation cube.
  const Bnd_Box box = m_view->View()->MinMaxValues();
  if (!box.IsVoid()) {
    m_view->FitMinMax(fitted, box, 0.15, 10.0 * Precision::Confusion());
    fitToFreeArea(fitted);
  }
}

QRectF OcctViewer::freeArea() const {
  // The cards are strips at the sides or along the bottom (the timeline,
  // as wide as the view): each takes the whole strip of the view it
  // reaches into, from its edge, so that the free area stays a rectangle.
  const QRectF whole(0.0, 0.0, width(), height());
  qreal left = 0.0;
  qreal right = whole.right();
  qreal top = 0.0;
  qreal bottom = whole.bottom();
  for (const QRect& card : m_covered) {
    if (card.width() > 0.6 * width()) {
      if (card.center().y() > height() / 2) {
        bottom = std::min<qreal>(bottom, card.top());
      } else {
        top = std::max<qreal>(top, card.bottom() + 1);
      }
    } else if (card.center().x() < width() / 2) {
      left = std::max<qreal>(left, card.right() + 1);
    } else {
      right = std::min<qreal>(right, card.left());
    }
  }
  const QRectF free(QPointF(left, top), QPointF(right, bottom));
  // A view that is nearly all covered keeps the whole view.
  return free.width() < 0.3 * width() || free.height() < 0.3 * height() ? whole : free;
}

void OcctViewer::fitToFreeArea(const occ::handle<Graphic3d_Camera>& camera) const {
  if (m_covered.empty() || width() <= 0 || height() <= 0) {
    return;
  }
  const QRectF free = freeArea();
  const qreal zoomOut = std::max(width() / free.width(), height() / free.height());
  if (camera->IsOrthographic()) {
    camera->SetScale(camera->Scale() * zoomOut);
  } else {
    camera->SetDistance(camera->Distance() * zoomOut);
  }
  // The model sits in the middle of the view; the middle of the free area
  // is dx, dy pixels from there. Moving the camera the other way moves the
  // model there.
  const QPointF shift = free.center() - QPointF(width() / 2.0, height() / 2.0);
  const gp_XYZ extent = camera->ViewDimensions(camera->Distance());
  const double perPixel = extent.Y() / height();
  const gp_Dir right = camera->Direction().Crossed(camera->Up());
  const gp_Vec move = (gp_Vec(right) * shift.x() - gp_Vec(camera->Up()) * shift.y()) * -perPixel;
  camera->SetEyeAndCenter(camera->Eye().Translated(move), camera->Center().Translated(move));
}

void OcctViewer::moveCamera(const occ::handle<Graphic3d_Camera>& end) {
  AbortViewAnimation();
  if (!m_navigation.animate || m_view->Window().IsNull() || !isVisible()) {
    m_view->Camera()->Copy(end);
    m_view->Invalidate();
    cameraMoved();
    requestFrame();
    return;
  }
  occ::handle<Graphic3d_Camera> start = new Graphic3d_Camera();
  start->Copy(m_view->Camera());
  const occ::handle<AIS_AnimationCamera>& animation = ViewAnimation();
  animation->SetView(m_view);
  animation->SetCameraStart(start);
  animation->SetCameraEnd(end);
  animation->SetOwnDuration(0.4);
  animation->StartTimer(0.0, 1.0, true, false);
  requestFrame();
}

void OcctViewer::turnTo(const gp_Dir& direction, const gp_Dir& up) {
  occ::handle<Graphic3d_Camera> end = new Graphic3d_Camera();
  end->Copy(m_view->Camera());
  const gp_Pnt center = end->Center();
  end->SetEyeAndCenter(center.Translated(gp_Vec(direction) * -end->Distance()), center);
  end->SetUp(up);
  end->OrthogonalizeUp();
  fitCamera(end);
  moveCamera(end);
}

void OcctViewer::goHome() {
  if (m_home) {
    setCamera(*m_home);
    return;
  }
  // Isometric from the front right, Z up (V3d_TypeOfOrientation_Zup_AxoRight).
  turnTo(gp_Dir(-1, 1, -1), gp::DZ());
}

// ---------------------------------------------------------------------------
// The orientation cube and the camera's log

QRectF OcctViewer::cubeArea() const {
  return QRectF(width() - m_overlayInsets.right() - kCubeArea, m_overlayInsets.top(), kCubeArea, kCubeArea);
}

void OcctViewer::updateOverlayIcons() {
  const IconBackdrop backdrop = m_backgroundDark ? IconBackdrop::Dark : IconBackdrop::Light;
  m_homeButton->setIcon(overlayIcon(QStringLiteral("home"), backdrop));
  for (const auto& [id, arrow] : m_cubeArrows) {
    arrow->setIcon(overlayIcon(id, backdrop));
  }
}

void OcctViewer::applyDevicePixelRatio() {
  // Qt's widget coordinates (the cube's area, the buttons over it) are in
  // device independent pixels, but OCCT places and sizes the cube and picks
  // in device pixels: at 2x the cube would be half its size among the
  // buttons. The same ratio keeps the pick tolerance as near as it feels.
  const qreal ratio = devicePixelRatioF();
  if (ratio <= 0.0 || qFuzzyCompare(ratio, m_cubeRatio)) {
    return;
  }
  m_cubeRatio = ratio;
  m_context->SetPixelTolerance(qRound(kPickTolerance * ratio));
  applyCubePlacement();
}

void OcctViewer::applyCubePlacement() {
  const bool displayed = m_context->IsDisplayed(m_orientationCube);
  if (displayed) {
    m_context->Remove(m_orientationCube, false);
  }
  m_orientationCube->SetSize(m_cubeBaseSize * m_cubeRatio);
  m_orientationCube->SetFontHeight(m_cubeBaseFont * m_cubeRatio);
  m_orientationCube->SetTransformPersistence(cubePersistence(m_cubeRatio, m_overlayInsets));
  if (displayed) {
    m_context->Display(m_orientationCube, 0, 0, false);
  }
  sceneChanged();
}

void OcctViewer::setCoveredRects(const std::vector<QRect>& rects) { m_covered = rects; }

bool OcctViewer::isCovered(const QPointF& position) const {
  return std::any_of(m_covered.begin(), m_covered.end(), [&position](const QRect& rect) {
    return QRectF(rect).adjusted(-6, -6, 6, 6).contains(position);
  });
}

void OcctViewer::setBadgeWidth(int width) {
  if (width != m_badgeWidth) {
    m_badgeWidth = width;
    emit badgeWidthChanged();
  }
}

void OcctViewer::setOverlayInsets(const QMargins& insets) {
  if (insets == m_overlayInsets) {
    return;
  }
  m_overlayInsets = insets;
  qDebug().noquote() << QStringLiteral("View overlay insets %1 %2 %3 %4")
                            .arg(insets.left())
                            .arg(insets.top())
                            .arg(insets.right())
                            .arg(insets.bottom());
  applyCubePlacement();
  placeHomeButton();
  placeCubeArrows();
  for (QWidget* overlay : findChildren<QWidget*>(Qt::FindDirectChildrenOnly)) {
    overlay->update(); // the sketch overlay's badge
  }
  m_loggedCamera.clear(); // the cube's places moved
  m_restTimer->start();
}

void OcctViewer::placeHomeButton() {
  // At the cube's upper left.
  m_homeButton->move(width() - m_overlayInsets.right() - kCubeOffset - 70, m_overlayInsets.top() + 6);
  m_homeButton->raise();
  const QPoint at = mapTo(window(), m_homeButton->geometry().center());
  qDebug().noquote() << QStringLiteral("View home at %1,%2").arg(at.x()).arg(at.y());
}

void OcctViewer::placeCubeArrows() {
  // Around the cube's middle (kCubeOffset from the corner).
  const QPoint middle(width() - m_overlayInsets.right() - kCubeOffset, m_overlayInsets.top() + kCubeOffset);
  for (const auto& [id, arrow] : m_cubeArrows) {
    QPoint at = middle;
    if (id == QStringLiteral("up")) {
      at += QPoint(0, -68);
    } else if (id == QStringLiteral("down")) {
      at += QPoint(0, 68);
    } else if (id == QStringLiteral("left")) {
      at += QPoint(-68, 0);
    } else if (id == QStringLiteral("right")) {
      at += QPoint(68, 0);
    } else if (id == QStringLiteral("cw")) {
      at += QPoint(58, -58);
    } else {
      at += QPoint(32, -70);
    }
    arrow->move(at - QPoint(arrow->width() / 2, arrow->height() / 2));
    arrow->raise();
  }
}

void OcctViewer::updateCubeArrows() {
  // Only while the view looks straight at a face.
  const gp_Dir direction = m_view->Camera()->Direction();
  const double along = std::max({std::abs(direction.X()), std::abs(direction.Y()), std::abs(direction.Z())});
  const bool faceOn = along > 1.0 - 1e-6;
  bool changed = false;
  for (const auto& [id, arrow] : m_cubeArrows) {
    changed = changed || arrow->isHidden() == faceOn;
    arrow->setVisible(faceOn);
  }
  if (faceOn) {
    for (const auto& [id, arrow] : m_cubeArrows) {
      const QPoint at = mapTo(window(), arrow->geometry().center());
      qDebug().noquote() << QStringLiteral("Orientation cube arrow %1 at %2,%3").arg(id).arg(at.x()).arg(at.y());
    }
  } else if (changed) {
    qDebug().noquote() << "Orientation cube arrows hidden";
  }
}

void OcctViewer::turnCube(const QString& arrow) {
  // The axes of the view (from the eye to the model, up, right), snapped
  // to the model's axes: the arrows show only while they are face on.
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  const auto snap = [](const gp_Dir& d) {
    const auto round = [](double v) { return std::abs(v) < 0.5 ? 0.0 : (v > 0.0 ? 1.0 : -1.0); };
    const gp_XYZ snapped(round(d.X()), round(d.Y()), round(d.Z()));
    return snapped.Modulus() > 0.5 ? gp_Dir(snapped) : d;
  };
  const gp_Dir direction = snap(camera->Direction());
  const gp_Dir up = snap(camera->Up());
  const gp_Dir right = direction.Crossed(up);
  qDebug().noquote() << QStringLiteral("Orientation cube arrow %1").arg(arrow);
  if (arrow == QStringLiteral("up")) {
    turnTo(up.Reversed(), direction);
  } else if (arrow == QStringLiteral("down")) {
    turnTo(up, direction.Reversed());
  } else if (arrow == QStringLiteral("left")) {
    turnTo(right, up);
  } else if (arrow == QStringLiteral("right")) {
    turnTo(right.Reversed(), up);
  } else if (arrow == QStringLiteral("cw")) {
    turnTo(direction, right.Reversed());
  } else {
    turnTo(direction, right);
  }
}

bool OcctViewer::isOnOrientationCube(const QPointF& position) {
  if (m_view->Window().IsNull() || !cubeArea().contains(position)) {
    return false;
  }
  const NCollection_Vec2<int> pixel = toDevicePixels(position);
  m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
  return !occ::handle<AIS_ViewCubeOwner>::DownCast(m_context->DetectedOwner()).IsNull();
}

void OcctViewer::cameraMoved() { m_restTimer->start(); }

void OcctViewer::logCamera() {
  const bool moving = !ViewAnimation()->IsStopped() || QGuiApplication::mouseButtons() != Qt::NoButton;
  if (moving) {
    m_restTimer->start();
    return;
  }
  const occ::handle<Graphic3d_Camera>& current = m_view->Camera();
  const QString text = QStringLiteral("Camera direction %1 up %2 (%3)")
                           .arg(vectorText(current->Direction().XYZ()), vectorText(current->Up().XYZ()),
                                isPerspective() ? QStringLiteral("perspective")
                                                : QStringLiteral("orthographic"));
  // The grid's spacing once it changes (UI tests check it against the zoom).
  const double step = m_gridShown ? gridStep() : 0.0;
  if (step != m_loggedGridStep) {
    m_loggedGridStep = step;
    if (step > 0.0) {
      qDebug().noquote() << QStringLiteral("Layout grid %1 mm, major %2 mm, %3 px apart")
                                .arg(step)
                                .arg(step * kLayoutGridMajor)
                                .arg(step * height() / current->ViewDimensions().Y(), 0, 'f', 1);
    }
  }
  if (text == m_loggedCamera) {
    return;
  }
  m_loggedCamera = text;
  updateCubeArrows();
  // Only on request: every place of the cube's corner is tried. Before the
  // camera's line, so that the places are there when a test reads it.
  if (qEnvironmentVariableIsSet("MITCAD_LOG_ORIENTATION_CUBE")) {
    logOrientationCube();
  }
  qDebug().noquote() << text;
  emit cameraRested();
}

void OcctViewer::logOrientationCube() {
  // Where a click hits each face, edge and corner of the cube in sight: of
  // the places on a grid that pick it, the one nearest their middle whose
  // neighbours pick it too.
  constexpr int kStep = 2;
  const QRect area = cubeArea().toRect();
  const int columns = area.width() / kStep;
  const int rows = area.height() / kStep;
  std::vector<int> grid(static_cast<std::size_t>(columns * rows), -1);
  const auto at = [&](int i, int j) { return grid[static_cast<std::size_t>(j * columns + i)]; };
  std::map<int, std::vector<std::pair<int, int>>> places;
  for (int j = 0; j < rows; ++j) {
    for (int i = 0; i < columns; ++i) {
      const NCollection_Vec2<int> pixel =
          toDevicePixels(QPointF(area.left() + i * kStep, area.top() + j * kStep));
      m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
      const auto owner = occ::handle<AIS_ViewCubeOwner>::DownCast(m_context->DetectedOwner());
      if (!owner.IsNull()) {
        grid[static_cast<std::size_t>(j * columns + i)] = owner->MainOrientation();
        places[owner->MainOrientation()].emplace_back(i, j);
      }
    }
  }
  m_context->ClearDetected(false);
  const QPoint corner = mapTo(window(), area.topLeft());
  qDebug().noquote() << "Orientation cube places:";
  for (const auto& [orientation, cells] : places) {
    double mx = 0.0;
    double my = 0.0;
    for (const auto& [i, j] : cells) {
      mx += i;
      my += j;
    }
    mx /= double(cells.size());
    my /= double(cells.size());
    std::optional<std::pair<int, int>> best;
    double bestDistance = 0.0;
    for (const auto& [i, j] : cells) {
      const bool inside = i > 0 && j > 0 && i + 1 < columns && j + 1 < rows &&
                          at(i - 1, j) == orientation && at(i + 1, j) == orientation &&
                          at(i, j - 1) == orientation && at(i, j + 1) == orientation;
      const double distance = std::hypot(i - mx, j - my);
      if (inside && (!best || distance < bestDistance)) {
        best = std::make_pair(i, j);
        bestDistance = distance;
      }
    }
    if (!best) {
      continue; // seen edge-on
    }
    qDebug().noquote() << QStringLiteral("Orientation cube %1 at %2,%3")
                              .arg(cubeName(static_cast<V3d_TypeOfOrientation>(orientation)))
                              .arg(corner.x() + best->first * kStep)
                              .arg(corner.y() + best->second * kStep);
  }
  requestFrame();
}

CameraState OcctViewer::camera() const {
  const occ::handle<Graphic3d_Camera>& current = m_view->Camera();
  CameraState state;
  state.eye = current->Eye();
  state.target = current->Center();
  state.up = current->Up();
  state.perspective = current->ProjectionType() == Graphic3d_Camera::Projection_Perspective;
  state.height = current->ViewDimensions().Y();
  return state;
}

void OcctViewer::setCamera(const CameraState& state) {
  if (state.eye.Distance(state.target) <= Precision::Confusion() || !(state.height > 0.0)) {
    return;
  }
  occ::handle<Graphic3d_Camera> end = new Graphic3d_Camera();
  end->Copy(m_view->Camera());
  end->SetProjectionType(state.perspective ? Graphic3d_Camera::Projection_Perspective
                                           : Graphic3d_Camera::Projection_Orthographic);
  end->SetEyeAndCenter(state.eye, state.target);
  end->SetUp(state.up);
  end->OrthogonalizeUp();
  end->SetScale(state.height);
  moveCamera(end);
}

void OcctViewer::setPerspective(bool perspective) {
  m_view->Camera()->SetProjectionType(perspective ? Graphic3d_Camera::Projection_Perspective
                                                  : Graphic3d_Camera::Projection_Orthographic);
  m_view->Invalidate();
  cameraMoved();
  requestFrame();
}

bool OcctViewer::isPerspective() const {
  return m_view->Camera()->ProjectionType() == Graphic3d_Camera::Projection_Perspective;
}

// ---------------------------------------------------------------------------
// Display settings (U5)

void OcctViewer::bindGestures() {
  auto& gestures = ChangeMouseGestureMap();
  gestures.Clear();
  // A window selection with or without the keys that add to a selection.
  gestures.Bind(Aspect_VKeyMouse_LeftButton, AIS_MouseGesture_SelectRectangle);
  gestures.Bind(Aspect_VKeyMouse_LeftButton | Aspect_VKeyFlags_CTRL,
                AIS_MouseGesture_SelectRectangle);
  gestures.Bind(Aspect_VKeyMouse_LeftButton | Aspect_VKeyFlags_SHIFT,
                AIS_MouseGesture_SelectRectangle);
  // Dragging the orientation cube orbits.
  gestures.Bind(Aspect_VKeyMouse_LeftButton | kCubeDrag, AIS_MouseGesture_RotateOrbit);
  const Aspect_VKeyMouse left = Aspect_VKeyMouse_LeftButton;
  const Aspect_VKeyMouse middle = Aspect_VKeyMouse_MiddleButton;
  const Aspect_VKeyMouse right = Aspect_VKeyMouse_RightButton;
  switch (m_navigation.scheme) {
  case NavigationScheme::MiddlePan:
    gestures.Bind(middle, AIS_MouseGesture_Pan);
    gestures.Bind(middle | Aspect_VKeyFlags_SHIFT, AIS_MouseGesture_RotateOrbit);
    break;
  case NavigationScheme::MiddlePanF4:
    gestures.Bind(middle, AIS_MouseGesture_Pan);
    gestures.Bind(middle | Aspect_VKeyFlags_SHIFT, AIS_MouseGesture_RotateOrbit);
    // F4 held: the left button orbits (viewButtons sends it as a cube drag).
    break;
  case NavigationScheme::MiddleOrbit:
    gestures.Bind(middle, AIS_MouseGesture_RotateOrbit);
    gestures.Bind(middle | Aspect_VKeyFlags_CTRL, AIS_MouseGesture_Pan);
    gestures.Bind(middle | Aspect_VKeyFlags_SHIFT, AIS_MouseGesture_Zoom);
    break;
  case NavigationScheme::AltButtons:
    gestures.Bind(left | Aspect_VKeyFlags_ALT, AIS_MouseGesture_RotateOrbit);
    gestures.Bind(middle | Aspect_VKeyFlags_ALT, AIS_MouseGesture_Pan);
    gestures.Bind(right | Aspect_VKeyFlags_ALT, AIS_MouseGesture_Zoom);
    gestures.Bind(middle, AIS_MouseGesture_Pan);
    break;
  case NavigationScheme::RightOrbit:
    gestures.Bind(right, AIS_MouseGesture_RotateOrbit);
    gestures.Bind(right | Aspect_VKeyFlags_SHIFT, AIS_MouseGesture_Pan);
    gestures.Bind(middle, AIS_MouseGesture_Pan);
    break;
  case NavigationScheme::Trackpad:
    // The two-finger scroll is passed on as a drag (trackpadScroll).
    gestures.Bind(left | kScrollPan, AIS_MouseGesture_Pan);
    gestures.Bind(left | kScrollOrbit, AIS_MouseGesture_RotateOrbit);
    gestures.Bind(left | Aspect_VKeyFlags_ALT, AIS_MouseGesture_RotateOrbit);
    gestures.Bind(left | Aspect_VKeyFlags_ALT | Aspect_VKeyFlags_SHIFT, AIS_MouseGesture_Pan);
    gestures.Bind(middle, AIS_MouseGesture_Pan);
    gestures.Bind(middle | Aspect_VKeyFlags_SHIFT, AIS_MouseGesture_RotateOrbit);
    gestures.Bind(right, AIS_MouseGesture_RotateOrbit);
    gestures.Bind(right | Aspect_VKeyFlags_SHIFT, AIS_MouseGesture_Pan);
    break;
  }
}

void OcctViewer::setNavigation(const NavigationSettings& settings) {
  endScrollDrag(); // the gestures are bound anew
  m_navigation = settings;
  bindGestures();
  SetLockOrbitZUp(settings.constrainedOrbit);
}

void OcctViewer::setBackground(const QColor& top, const QColor& bottom, bool gradient) {
  // The hidden-line style's faces: the background's middle.
  const QColor paper = QColor::fromRgbF((top.redF() + bottom.redF()) / 2.0f,
                                        (top.greenF() + bottom.greenF()) / 2.0f,
                                        (top.blueF() + bottom.blueF()) / 2.0f);
  if ((paper.lightnessF() < 0.5f) != m_backgroundDark) {
    m_backgroundDark = !m_backgroundDark;
    updateOverlayIcons(); // the buttons over the view
  }
  if (paper != m_paper) {
    m_paper = paper;
    if (m_visualStyle == VisualStyle::WireframeHiddenEdges) {
      for (auto& [key, entry] : m_bodies) {
        removeBody(entry);
        showBody(entry);
      }
    }
  }
  if (gradient) {
    m_view->SetBgGradientColors(occtColor(top), occtColor(bottom), Aspect_GradientFillMethod_Vertical);
  } else {
    m_view->SetBgGradientStyle(Aspect_GradientFillMethod_None);
    m_view->SetBackgroundColor(occtColor(top));
  }
  setGridColors(*m_grid, paper);
  updateGrid(true);
  sceneChanged();
}

void OcctViewer::setGrid(bool shown, double step) {
  m_gridShown = shown;
  m_gridStep = step;
  updateGrid(true);
  sceneChanged();
}

void OcctViewer::setGridPlane(const gp_Ax3& plane) {
  m_gridPlane = plane;
  gp_Trsf placement;
  placement.SetDisplacement(gp_Ax3(), plane);
  m_grid->SetLocalTransformation(placement);
  updateGrid(true);
  sceneChanged();
}

double OcctViewer::gridStep() const {
  if (m_gridStep > 0.0) {
    return m_gridStep;
  }
  // Logical pixels, as the screen distances of sketch mode.
  return layoutGridStep(height() / m_view->Camera()->ViewDimensions().Y());
}

void OcctViewer::updateGrid(bool force) {
  if (!m_gridShown || m_view->Window().IsNull()) {
    if (m_context->IsDisplayed(m_grid)) {
      m_context->Erase(m_grid, false);
    }
    return;
  }
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  const double perMm = height() / camera->ViewDimensions().Y();
  if (!(perMm > 0.0) || !std::isfinite(perMm)) {
    return;
  }
  LayoutGrid::Lines lines;
  lines.step = gridStep();
  lines.minor = lines.step * perMm >= 4.0; // a fixed step far out: majors only
  // The view's middle on the grid plane, in the plane's coordinates.
  gp_Trsf toPlane;
  toPlane.SetTransformation(m_gridPlane);
  const gp_Pnt middle = camera->Center().Transformed(toPlane);
  const double across = std::max(width(), height()) / perMm; // the view's size (mm)
  // Lines reach well past the view (an orbit or a perspective shows more
  // of the plane), and are rebuilt when the view nears their edge.
  const LayoutGrid::Lines& drawn = m_grid->lines();
  const double offset = std::max(std::abs(middle.X() - drawn.centreX), std::abs(middle.Y() - drawn.centreY));
  const double major = lines.step * kLayoutGridMajor;
  const bool rebuild = force || lines.step != drawn.step || lines.minor != drawn.minor ||
                       drawn.extent - offset < across ||
                       drawn.extent > std::max(8.0 * across, 2.0 * major);
  if (!rebuild && m_context->IsDisplayed(m_grid)) {
    return;
  }
  lines.centreX = std::round(middle.X() / major) * major;
  lines.centreY = std::round(middle.Y() / major) * major;
  lines.extent = std::ceil(3.0 * across / major) * major;
  m_grid->setLines(lines);
  if (m_context->IsDisplayed(m_grid)) {
    m_context->Redisplay(m_grid, false);
  } else {
    m_context->Display(m_grid, 0, -1, false);
  }
  m_view->Invalidate();
}

void OcctViewer::setSectionCaps(const TopoDS_Shape& caps) {
  if (!m_sectionCaps.IsNull()) {
    m_context->Remove(m_sectionCaps, false);
    m_sectionCaps.Nullify();
  }
  if (!caps.IsNull()) {
    m_sectionCaps = new AIS_Shape(caps);
    m_sectionCaps->SetColor(kCapColor);
    // Over the clipped body's own face on the plane.
    m_sectionCaps->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, -1.0f,
                                                                              -3.0f);
    const occ::handle<Prs3d_Drawer>& drawer = m_sectionCaps->Attributes();
    drawer->SetFaceBoundaryDraw(true);
    drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(kEdgeColor, Aspect_TOL_SOLID, 1.5));
    m_context->Display(m_sectionCaps, AIS_Shaded, -1, false);
  }
  sceneChanged();
}

void OcctViewer::setThreads(const TopoDS_Shape& rings) {
  if (!m_threads.IsNull()) {
    m_context->Remove(m_threads, false);
    m_threads.Nullify();
  }
  if (!rings.IsNull()) {
    m_threads = new AIS_Shape(rings);
    setEdgeStyle(m_threads, kThreadColor, 1.0);
    m_context->Display(m_threads, AIS_WireFrame, -1, false);
  }
  sceneChanged();
}

QPoint OcctViewer::toWidget(const gp_Pnt& point) const {
  if (m_view->Window().IsNull()) {
    return QPoint(); // not shown yet
  }
  int x = 0;
  int y = 0;
  m_view->Convert(point.X(), point.Y(), point.Z(), x, y);
  const qreal ratio = devicePixelRatioF();
  return QPoint(qRound(x / ratio), qRound(y / ratio));
}

QPointF OcctViewer::toWidgetF(const gp_Pnt& point) const {
  // Normalized device coordinates of the camera, -1 to 1 across the view.
  const gp_Pnt ndc = m_view->Camera()->Project(point);
  return QPointF((ndc.X() + 1.0) * 0.5 * width(), (1.0 - ndc.Y()) * 0.5 * height());
}

gp_Pnt OcctViewer::planePickPoint(const gp_Pnt& origin, const gp_Dir& normal, double size) const {
  // Towards the eye, a third of the size or less when that is outside the
  // view (a zoomed-in view); inside the view and not on the orientation cube in
  // its upper right corner.
  const std::vector<gp_Pnt> candidates = planePickCandidates(origin, normal, size);
  const QRectF inside = QRectF(rect()).adjusted(12, 12, -12, -12);
  const QRectF cube = cubeArea();
  for (std::size_t i = 0; i < 6 && i < candidates.size(); ++i) {
    const QPointF at = toWidgetF(candidates[i]);
    if (inside.contains(at) && !cube.contains(at) && !isCovered(at)) {
      return candidates[i];
    }
  }
  return candidates[std::min<std::size_t>(5, candidates.size() - 1)];
}

std::vector<gp_Pnt> OcctViewer::planePickCandidates(const gp_Pnt& origin, const gp_Dir& normal,
                                                     double size) const {
  // In the plane, step along the two model axes that lie in it, each
  // towards the eye: other planes through the origin then lie behind the
  // point. A third of the size, or less; then the other quarters.
  const gp_Dir eye = m_view->Camera()->Direction().Reversed();
  std::vector<gp_XYZ> axes;
  gp_XYZ toward(0, 0, 0);
  for (const gp_Dir axis : {gp::DX(), gp::DY(), gp::DZ()}) {
    if (std::abs(axis.Dot(normal)) > 0.5) {
      continue;
    }
    const gp_XYZ step = axis.XYZ() * (eye.Dot(axis) >= 0 ? 1.0 : -1.0);
    axes.push_back(step);
    toward += step;
  }
  std::vector<gp_XYZ> directions = {toward};
  if (axes.size() == 2) {
    directions.push_back(axes[0] - axes[1]);
    directions.push_back(axes[1] - axes[0]);
    directions.push_back(toward * -1.0);
  }
  std::vector<gp_Pnt> candidates;
  for (const gp_XYZ& direction : directions) {
    for (const double fraction : {1.0 / 3.0, 0.25, 0.18, 0.12, 0.08, 0.05}) {
      candidates.emplace_back(origin.XYZ() + direction * (size * fraction));
    }
  }
  return candidates;
}

gp_Dir OcctViewer::viewUp() const { return m_view->Camera()->Up(); }

std::optional<SelectionItem> OcctViewer::itemAt(const QPointF& position) {
  if (m_view->Window().IsNull() || !QRectF(rect()).contains(position)) {
    return std::nullopt;
  }
  const NCollection_Vec2<int> pixel = toDevicePixels(position);
  m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
  const occ::handle<SelectMgr_EntityOwner> owner = m_context->DetectedOwner();
  std::optional<SelectionItem> item;
  if (!owner.IsNull() && owner->Selectable() != m_orientationCube && accepts(owner)) {
    item = itemFor(owner);
  }
  m_context->ClearDetected(false);
  return item;
}

std::vector<SelectionItem> OcctViewer::itemsAt(const QPointF& position) {
  std::vector<SelectionItem> items;
  if (m_view->Window().IsNull() || !QRectF(rect()).contains(position) || isCovered(position)) {
    return items;
  }
  const NCollection_Vec2<int> pixel = toDevicePixels(position);
  m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
  for (m_context->InitDetected(); m_context->MoreDetected(); m_context->NextDetected()) {
    const occ::handle<SelectMgr_EntityOwner> owner = m_context->DetectedCurrentOwner();
    if (!owner.IsNull() && owner->Selectable() != m_orientationCube && accepts(owner)) {
      if (auto item = itemFor(owner)) {
        items.push_back(*item);
      }
    }
  }
  m_context->ClearDetected(false);
  return items;
}

void OcctViewer::sceneChanged() {
  if (m_view.IsNull()) {
    return; // still being constructed
  }
  m_view->Invalidate(); // a cached frame is no longer valid
  requestFrame();
}

void OcctViewer::requestFrame() {
  update();
  const QWindow* handle = window()->windowHandle();
  if (m_directFrameQueued || handle == nullptr || handle->isExposed() || !isVisible() ||
      window()->isMinimized()) {
    return;
  }
  m_directFrameQueued = true;
  QTimer::singleShot(0, this, [this] {
    m_directFrameQueued = false;
    const QWindow* now = window()->windowHandle();
    if (now != nullptr && !now->isExposed() && isVisible()) {
      grabFramebuffer(); // renders through paintGL()
    }
  });
}

// ---------------------------------------------------------------------------
// OpenGL

void OcctViewer::initializeGL() {
  const qreal ratio = devicePixelRatioF();
  const int viewWidth = qRound(width() * ratio);
  const int viewHeight = qRound(height() * ratio);

  const occ::handle<OpenGl_Context> glContext = new OpenGl_Context();
  if (!glContext->Init(context()->format().profile() == QSurfaceFormat::CoreProfile)) {
    emit glFailed(tr("OCCT could not attach to the OpenGL context."));
    return;
  }

  occ::handle<Aspect_NeutralWindow> window =
      occ::handle<Aspect_NeutralWindow>::DownCast(m_view->Window());
  const bool firstTime = window.IsNull();
  if (firstTime) {
    window = new Aspect_NeutralWindow();
    window->SetVirtual(true);
  }
  window->SetNativeHandle(occtDrawable());
  window->SetSize(viewWidth, viewHeight);
  m_view->SetWindow(window, glContext->RenderingContext());

  if (firstTime) {
    m_view->SetSize(250.0); // an empty scene shows about 250 mm
    m_context->Display(m_orientationCube, 0, 0, false);
    updateGrid(true);
    const auto* renderer =
        reinterpret_cast<const char*>(context()->functions()->glGetString(GL_RENDERER));
    emit glInitialized(QString::fromLatin1(renderer != nullptr ? renderer : "unknown"));
  }
}

void OcctViewer::resizeGL(int width, int height) {
  QOpenGLWidget::resizeGL(width, height);
  applyDevicePixelRatio(); // a move to another screen resizes the view
  // Where the view is in the window, for UI tests that click in it.
  const QPoint corner = mapTo(window(), QPoint(0, 0));
  qDebug().noquote() << QStringLiteral("View area %1 %2 %3 %4")
                            .arg(corner.x())
                            .arg(corner.y())
                            .arg(width)
                            .arg(height);
  placeHomeButton();
  placeCubeArrows();
  m_loggedCamera.clear(); // the cube's places moved
  m_restTimer->start();
}

#ifdef Q_OS_MACOS
QSurfaceFormat macSurfaceFormat() {
  QSurfaceFormat format;
  format.setDepthBufferSize(24);
  format.setStencilBufferSize(8);
  format.setVersion(4, 1);
  format.setProfile(QSurfaceFormat::CoreProfile);
  return format;
}
#endif

Aspect_Drawable OcctViewer::occtDrawable() {
#ifdef _WIN32
  // On Windows Qt's offscreen surface is a hidden window OCCT can use directly.
  return reinterpret_cast<Aspect_Drawable>(WindowFromDC(wglGetCurrentDC()));
#elif defined(Q_OS_MACOS)
  // OCCT's Apple backend takes an NSView as the window and uses the
  // NSOpenGLContext that is current. OCCT's Qt sample passes the widget's own
  // winId(), but that turns the widget into a native child view, which Qt
  // stacks above its siblings; the top-level window's view is already native,
  // does not change the layout and stays the same while the widget lives.
  return static_cast<Aspect_Drawable>(window()->winId());
#else
  if (!m_glxWindow) {
    m_glxWindow = std::make_unique<QWindow>();
    m_glxWindow->setTitle(QStringLiteral("Mitcad OpenGL helper"));
    m_glxWindow->setSurfaceType(QSurface::OpenGLSurface);
    m_glxWindow->setFormat(context()->format());
    m_glxWindow->create();
  }
  return static_cast<Aspect_Drawable>(m_glxWindow->winId());
#endif
}

void OcctViewer::paintGL() {
  if (m_view->Window().IsNull()) {
    return;
  }
  m_painting = true;
  struct Painted {
    bool& painting;
    ~Painted() { painting = false; }
  } painted{m_painting};
  ScopedTiming timing("frame");
  if (m_view->Window()->NativeHandle() != occtDrawable()) {
    // Qt recreated its surface, e.g. after re-parenting: attach again.
    initializeGL();
    return;
  }

  const occ::handle<OpenGl_Context> glContext = glContextOf(m_view);
  occ::handle<OpenGl_FrameBuffer> framebuffer = glContext->DefaultFrameBuffer();
  if (framebuffer.IsNull()) {
    framebuffer = new QtFrameBuffer();
    glContext->SetDefaultFrameBuffer(framebuffer);
  }
  if (!framebuffer->InitWrapper(glContext)) {
    emit glFailed(tr("OCCT could not wrap Qt's framebuffer."));
    return;
  }

  const occ::handle<Aspect_NeutralWindow> window =
      occ::handle<Aspect_NeutralWindow>::DownCast(m_view->Window());
  NCollection_Vec2<int> oldSize;
  window->Size(oldSize.x(), oldSize.y());
  const NCollection_Vec2<int> newSize = framebuffer->GetVPSize();
  if (newSize != oldSize) {
    window->SetSize(newSize.x(), newSize.y());
    m_view->MustBeResized();
    m_view->Invalidate();
  }

  if (m_fitPending) {
    m_view->FitAll(0.15, false);
    fitToFreeArea(m_view->Camera());
    m_view->ZFitAll();
    m_view->Invalidate();
    m_fitPending = false;
  }

  m_view->InvalidateImmediate();
  // An exception escaping a Qt event handler terminates the application, so
  // a failing view operation is logged and the frame skipped instead.
  try {
    FlushViewEvents(m_context, m_view, true);
  } catch (const Standard_Failure& failure) {
    qWarning() << "OCCT view event failed:" << failure.what();
  }
  // The camera is logged once it has kept still for a moment.
  const occ::handle<Graphic3d_Camera>& current = m_view->Camera();
  const QString key = QStringLiteral("%1 %2 %3 %4 %5")
                          .arg(vectorText(current->Eye().XYZ()), vectorText(current->Center().XYZ()),
                               vectorText(current->Up().XYZ()))
                          .arg(current->Scale())
                          .arg(int(current->ProjectionType()));
  if (key != m_restingCamera) {
    m_restingCamera = key;
    m_restTimer->start();
  }
  emit viewChanged();
}

// ---------------------------------------------------------------------------
// Input

NCollection_Vec2<int> OcctViewer::toDevicePixels(const QPointF& point) const {
  const qreal ratio = devicePixelRatioF();
  return NCollection_Vec2<int>(qRound(point.x() * ratio), qRound(point.y() * ratio));
}

std::optional<gp_Pnt2d> OcctViewer::sketchPlanePoint(const QPointF& position) const {
  const NCollection_Vec2<int> pixel = toDevicePixels(position);
  double x = 0, y = 0, z = 0, dx = 0, dy = 0, dz = 0;
  m_view->ConvertWithProj(pixel.x(), pixel.y(), x, y, z, dx, dy, dz);
  const gp_XYZ from(x, y, z);
  const gp_XYZ direction(dx, dy, dz);
  const gp_XYZ normal = m_sketchPlane.Direction().XYZ();
  const double along = direction.Dot(normal);
  if (std::abs(along) < 1e-9) {
    return std::nullopt; // looking along the sketch plane
  }
  const gp_XYZ origin = m_sketchPlane.Location().XYZ();
  const gp_XYZ hit = from + direction * ((origin - from).Dot(normal) / along);
  return gp_Pnt2d((hit - origin).Dot(m_sketchPlane.XDirection().XYZ()),
                  (hit - origin).Dot(m_sketchPlane.YDirection().XYZ()));
}

// While a sketch tool is active, the left button belongs to it, not to
// OCCT, except on the orientation cube.
Aspect_VKeyMouse OcctViewer::viewButtons(Qt::MouseButtons buttons) const {
  return toVKeyMouse(m_sketchInput && !m_cubePress ? (buttons & ~Qt::LeftButton) : buttons);
}

// A drag that began on the orientation cube, or a left drag with F4 held
// (MiddlePanF4), orbits: it is passed on with a flag of its own.
Aspect_VKeyFlags OcctViewer::viewFlags(Qt::KeyboardModifiers modifiers) const {
  return m_cubePress ? kCubeDrag : toVKeyFlags(modifiers);
}

void OcctViewer::setSketchInput(bool enabled, const QString& toolIcon) {
  m_sketchInput = enabled;
  m_sketchToolIcon = enabled ? toolIcon : QString();
  m_sketchSnapping = false;
  updateCursor(true);
}

void OcctViewer::setSketchSnapping(bool snapping) {
  if (m_sketchInput && snapping != m_sketchSnapping) {
    m_sketchSnapping = snapping;
    updateCursor(false);
  }
}

void OcctViewer::updateCursor(bool log) {
  if (!m_sketchInput) {
    setCursor(Qt::ArrowCursor);
  } else {
    setCursor(precisionCursor(m_sketchToolIcon, devicePixelRatioF(), m_sketchSnapping));
  }
  if (log) {
    const QCursor shown = cursor();
    if (shown.shape() != Qt::BitmapCursor) {
      qDebug().noquote() << QStringLiteral("Sketch cursor: arrow");
      return;
    }
    const QSizeF size = shown.pixmap().deviceIndependentSize();
    qDebug().noquote() << QStringLiteral("Sketch cursor: bitmap %1x%2, hot spot %3,%4, ratio %5, icon %6")
                              .arg(qRound(size.width()))
                              .arg(qRound(size.height()))
                              .arg(shown.hotSpot().x())
                              .arg(shown.hotSpot().y())
                              .arg(devicePixelRatioF())
                              .arg(m_sketchToolIcon.isEmpty() ? QStringLiteral("none") : m_sketchToolIcon);
  }
}

bool OcctViewer::event(QEvent* event) {
  if (event->type() == QEvent::NativeGesture) {
    nativeGesture(static_cast<QNativeGestureEvent*>(event));
    return true;
  }
  if (event->type() == QEvent::DevicePixelRatioChange) {
    // Another screen or scale: the pick widths and the cursor for the new
    // ratio.
    m_context->SetPixelTolerance(qRound(kPickTolerance * devicePixelRatioF()));
    m_context->ClearDetected(false);
    activateAllPicking();
    if (m_sketchInput) {
      updateCursor(true);
    }
  }
  if (event->type() == QEvent::WindowBlocked) {
    // The release goes to the dialog: an orbit or a window selection in
    // progress ends here, without a pick.
    endScrollDrag();
    ResetViewInput();
    m_cubePress = false;
    m_cubeClick = false;
    m_orbitKey = false;
  }
  const bool handled = QOpenGLWidget::event(event);
  if (event->type() == QEvent::DevicePixelRatioChange) {
    applyDevicePixelRatio();
  } else if (event->type() == QEvent::ApplicationPaletteChange) {
    emit themeChanged();
  }
  return handled;
}

void OcctViewer::mousePressEvent(QMouseEvent* event) {
  QOpenGLWidget::mousePressEvent(event);
  endScrollDrag();
  if (event->button() == Qt::RightButton) {
    m_rightPressPosition = event->position();
  }
  if (event->button() == Qt::LeftButton) {
    m_cubePress = isOnOrientationCube(event->position()) ||
                  (m_orbitKey && m_navigation.scheme == NavigationScheme::MiddlePanF4);
    if (m_sketchInput && !m_cubePress) {
      return;
    }
    m_pressPosition = event->position();
    m_pressModifiers = event->modifiers();
    m_windowPick = false;
    const NCollection_Vec2<int> pixel = toDevicePixels(event->position());
    m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
    const auto cubeOwner = occ::handle<AIS_ViewCubeOwner>::DownCast(m_context->DetectedOwner());
    m_cubeClick = !cubeOwner.IsNull();
    if (m_cubeClick) {
      const QPoint at = mapTo(window(), event->position().toPoint());
      qDebug().noquote() << QStringLiteral("Orientation cube pressed on %1 at %2,%3")
                                .arg(cubeName(cubeOwner->MainOrientation()))
                                .arg(at.x())
                                .arg(at.y());
    }
    if (m_cubePress && !m_cubeClick) {
      m_cubeClick = true; // F4: an orbit, not a pick
    }
  }
  if (UpdateMouseButtons(toDevicePixels(event->position()), viewButtons(event->buttons()),
                         viewFlags(event->modifiers()), false)) {
    requestFrame();
  }
}

void OcctViewer::mouseReleaseEvent(QMouseEvent* event) {
  QOpenGLWidget::mouseReleaseEvent(event);
  if (event->button() == Qt::RightButton) {
    const QPointF moved = event->position() - m_rightPressPosition;
    if (std::hypot(moved.x(), moved.y()) <= kDragPixels) {
      if (isOnOrientationCube(event->position())) {
        m_context->ClearDetected(false);
        emit orientationCubeMenuRequested(event->globalPosition().toPoint());
      } else {
        const NCollection_Vec2<int> pixel = toDevicePixels(event->position());
        m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
        const auto item = itemFor(m_context->DetectedOwner());
        emit contextMenuRequested(event->globalPosition().toPoint(),
                                  item && accepts(m_context->DetectedOwner()) ? *item
                                                                              : SelectionItem());
      }
    }
  }
  const bool cube = m_cubePress && event->button() == Qt::LeftButton;
  if (m_sketchInput && event->button() == Qt::LeftButton && !cube) {
    return;
  }
  if (event->button() == Qt::LeftButton) {
    const QPointF moved = event->position() - m_pressPosition;
    m_windowPick = std::hypot(moved.x(), moved.y()) > kDragPixels;
    if (m_windowPick) {
      m_cubeClick = false;
      // Dragged to the left, a window also takes what it crosses.
      m_context->MainSelector()->AllowOverlapDetection(moved.x() < 0 && !cube);
    }
  }
  const Aspect_VKeyFlags flags = viewFlags(event->modifiers());
  if (cube) {
    m_cubePress = false;
  }
  if (UpdateMouseButtons(toDevicePixels(event->position()), viewButtons(event->buttons()), flags,
                         false)) {
    requestFrame();
  }
}

void OcctViewer::mouseMoveEvent(QMouseEvent* event) {
  QOpenGLWidget::mouseMoveEvent(event);
  if (m_scrollDragging) {
    endScrollDrag();
  }
  if (UpdateMousePosition(toDevicePixels(event->position()), viewButtons(event->buttons()),
                          viewFlags(event->modifiers()), false)) {
    requestFrame();
  }
}

void OcctViewer::wheelEvent(QWheelEvent* event) {
  QOpenGLWidget::wheelEvent(event);
  if (m_navigation.scheme == NavigationScheme::Trackpad && isTrackpadScroll(event)) {
    trackpadScroll(event);
    return;
  }
  double steps = event->angleDelta().y() / 8.0;
  if (m_navigation.reverseZoom) {
    steps = -steps;
  }
  const bool zoomed = m_navigation.zoomToCursor
                          ? UpdateZoom(Aspect_ScrollDelta(toDevicePixels(event->position()), steps))
                          : UpdateZoom(Aspect_ScrollDelta(steps));
  if (zoomed) {
    requestFrame();
  }
}

// The Trackpad scheme: a two-finger scroll pans, with Alt it orbits, with
// Shift or Ctrl (Cmd on macOS) it zooms. Pan and orbit are one drag of the
// left button (with keys no one presses, kScrollPan and kScrollOrbit) that
// lasts until the scroll pauses, momentum included: the view handles it as a
// mouse drag, about the same centre and with the orbit's Z lock, and no
// step between two frames is lost. Each event asks for a frame; it is not
// drawn here.
void OcctViewer::trackpadScroll(QWheelEvent* event) {
  event->accept();
  if (event->phase() == Qt::ScrollBegin) {
    endScrollDrag(); // a new gesture: the momentum of the last one ends
  }
  const QPointF delta = scrollPixels(event);
  if (delta.isNull()) {
    return;
  }
  const Qt::KeyboardModifiers keys = event->modifiers();
  ScrollMode mode = (keys & (Qt::ShiftModifier | Qt::ControlModifier)) ? ScrollMode::Zoom
                    : (keys & Qt::AltModifier)                         ? ScrollMode::Orbit
                                                                       : ScrollMode::Pan;
  if (event->phase() == Qt::ScrollMomentum && m_scrollMode != ScrollMode::None) {
    mode = m_scrollMode; // the inertia goes on as it began, whatever the keys do now
  }
  if (mode != m_scrollMode) {
    endScrollDrag();
  }
  m_scrollMode = mode;
  m_scrollTimer->start();
  if (mode == ScrollMode::Zoom) {
    // Shift turns a vertical scroll into a horizontal one on some systems.
    const double along = std::abs(delta.y()) >= std::abs(delta.x()) ? delta.y() : delta.x();
    const double steps = (m_navigation.reverseZoom ? -along : along) * kScrollZoomPerPixel;
    const bool zoomed = m_navigation.zoomToCursor
                            ? UpdateZoom(Aspect_ScrollDelta(toDevicePixels(event->position()), steps))
                            : UpdateZoom(Aspect_ScrollDelta(steps));
    if (zoomed) {
      requestFrame();
    }
    return;
  }
  const Aspect_VKeyFlags flags = mode == ScrollMode::Orbit ? kScrollOrbit : kScrollPan;
  const qreal ratio = devicePixelRatioF();
  if (!m_scrollDragging) {
    m_scrollDragging = true;
    m_scrollPoint = event->position() * ratio;
    UpdateMouseButtons(devicePixel(m_scrollPoint), Aspect_VKeyMouse_LeftButton, flags, false);
  }
  m_scrollPoint += delta * ratio;
  UpdateMousePosition(devicePixel(m_scrollPoint), Aspect_VKeyMouse_LeftButton, flags, false);
  cameraMoved();
  requestFrame();
}

void OcctViewer::endScrollDrag() {
  const ScrollMode mode = m_scrollMode;
  m_scrollMode = ScrollMode::None;
  m_scrollTimer->stop();
  if (!m_scrollDragging) {
    return;
  }
  m_scrollDragging = false;
  UpdateMouseButtons(devicePixel(m_scrollPoint), Aspect_VKeyMouse_NONE,
                     mode == ScrollMode::Orbit ? kScrollOrbit : kScrollPan, false);
  requestFrame();
}

// A pinch zooms towards the fingers (about the middle where the zoom is not
// set to the cursor), a rotation rolls the view and a two-finger double tap
// fits everything, in every scheme. The pinch's direction is the fingers',
// the reverse-zoom setting is for the wheel only.
void OcctViewer::nativeGesture(QNativeGestureEvent* event) {
  event->accept();
  switch (event->gestureType()) {
  case Qt::ZoomNativeGesture: {
    const double steps = event->value() * kPinchZoomSteps; // the increment of the magnification
    const bool zoomed = m_navigation.zoomToCursor
                            ? UpdateZoom(Aspect_ScrollDelta(toDevicePixels(event->position()), steps))
                            : UpdateZoom(Aspect_ScrollDelta(steps));
    if (zoomed) {
      cameraMoved();
      requestFrame();
    }
    break;
  }
  case Qt::RotateNativeGesture:
    rollView(event->value());
    break;
  case Qt::SmartZoomNativeGesture:
    fitAll();
    break;
  default: // begin, end, the system's pan and swipe
    break;
  }
}

void OcctViewer::rollView(double degrees) {
  if (m_navigation.constrainedOrbit || degrees == 0.0) {
    return; // an orbit with Z up would turn it back
  }
  AbortViewAnimation();
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  // The up vector turns against the content: clockwise fingers roll the view clockwise.
  camera->SetUp(camera->Up().Rotated(gp_Ax1(gp_Pnt(), camera->Direction()),
                                     -kRollDirection * qDegreesToRadians(degrees)));
  camera->OrthogonalizeUp();
  m_view->Invalidate();
  cameraMoved();
  requestFrame();
}

void OcctViewer::keyPressEvent(QKeyEvent* event) {
  if (event->key() == Qt::Key_F4 && !event->isAutoRepeat()) {
    m_orbitKey = true;
  }
  QOpenGLWidget::keyPressEvent(event);
}

void OcctViewer::keyReleaseEvent(QKeyEvent* event) {
  if (event->key() == Qt::Key_F4 && !event->isAutoRepeat()) {
    m_orbitKey = false;
  }
  QOpenGLWidget::keyReleaseEvent(event);
}

void OcctViewer::contextLazyMoveTo(const occ::handle<AIS_InteractiveContext>& context,
                                   const occ::handle<V3d_View>& view, const NCollection_Vec2<int>& point) {
  // What lies under the cursor is found here, while a frame is drawn.
  ScopedTiming timing("hover");
  AIS_ViewController::contextLazyMoveTo(context, view, point);
}

void OcctViewer::handleViewRedraw(const occ::handle<AIS_InteractiveContext>& context,
                                  const occ::handle<V3d_View>& view) {
  // The camera has moved for this frame: the grid follows the zoom at once.
  updateGrid();
  AIS_ViewController::handleViewRedraw(context, view);
  if (toAskNextFrame()) {
    requestFrame(); // keep animations such as the orientation cube running
  }
}

} // namespace mitcad
