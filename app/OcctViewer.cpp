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
#include <OpenGl_GlCore11.hxx>
#include <OpenGl_Group.hxx>
#include <OpenGl_Workspace.hxx>
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
#include "mitcad/geometry/import.hpp"
#include "view/CameraHeight.hpp"
#include "view/ViewCube.hpp"
#include "view/ViewTessellation.hpp"
#include "sketch/PointMarkers.hpp"
#ifdef __APPLE__
#include "render/MacFrameDisplay.hpp"
#endif

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <initializer_list>
#include <iterator>
#include <limits>
#include <map>
#include <optional>
#include <tuple>
#include <typeinfo>
#include <utility>

#include <QGuiApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QPainter>
#include <QScreen>
#include <QSurfaceFormat>
#include <QTimer>
#include <QToolButton>
#include <QWheelEvent>
#include <QWindow>
#include <QtLogging>
#include <QtMath>

#include <AIS_AnimationCamera.hxx>
#include <AIS_ColoredShape.hxx>
#include <Aspect_DisplayConnection.hxx>
#include <Aspect_NeutralWindow.hxx>
#include <Aspect_ScrollDelta.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepTools.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <Bnd_Box.hxx>
#include <Graphic3d_ArrayOfTriangles.hxx>
#include <Graphic3d_AspectMarker3d.hxx>
#include <Graphic3d_AspectFillArea3d.hxx>
#include <Graphic3d_Group.hxx>
#include <Graphic3d_MarkerImage.hxx>
#include <Graphic3d_MaterialAspect.hxx>
#include <Prs3d_Presentation.hxx>
#include <gp_Pnt2d.hxx>
#include <Graphic3d_TextureParams.hxx>
#include <Graphic3d_TransformPers.hxx>
#include <Image_PixMap.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <ShapeAnalysis_Curve.hxx>
#include <gp_Lin.hxx>
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
// The two above on a dark background (mitcad#253).
const Quantity_Color kConstrainedColorDark(0.88, 0.90, 0.94, Quantity_TOC_RGB);
const Quantity_Color kCenterlineColorDark(0.60, 0.66, 0.76, Quantity_TOC_RGB);
// The halo under fully constrained geometry and centre lines (mitcad#302):
// a wider line in the opposite tone, so that the line stands out over a
// body's face as well as over the background. Near the background's own
// tone, so that on the empty background the line looks as it did (sRGB:
// the colours above are linear and show lighter).
const Quantity_Color kSketchHaloDark(0.13, 0.14, 0.16, Quantity_TOC_sRGB);
const Quantity_Color kSketchHaloLight(0.98, 0.98, 0.99, Quantity_TOC_sRGB);
constexpr double kSketchLineWidth = 2.0;
constexpr double kSketchHaloWidth = 6.0;
// A feature's input lines on a dark background (mitcad#957): the
// selection blue lightened, over the dark halo, so that it stands out
// (about 4.6:1 against the dark background, where the plain selection
// blue has about 2.6:1). On a light background the lines keep the plain
// selection colour without a halo.
const QColor kInputLineColorDark(0x66, 0xb2, 0xff);
constexpr double kInputLineWidth = 3.5;
constexpr double kInputLineHiddenWidth = 2.0;
const Quantity_Color kReferenceColor(0.55, 0.20, 0.70, Quantity_TOC_RGB);
const Quantity_Color kConstructionColor(0.85, 0.50, 0.10, Quantity_TOC_RGB);
const Quantity_Color kProfileFill(0.55, 0.70, 0.95, Quantity_TOC_RGB);
const Quantity_Color kPreviewColor(0.30, 0.55, 0.95, Quantity_TOC_RGB);
const Quantity_Color kCutPreviewColor(0.90, 0.20, 0.15, Quantity_TOC_RGB);
// A split's extended tool (mitcad#712): the orange of the panel's second
// selection input, the splitting tool's.
const Quantity_Color kExtendedToolColor(0xff / 255.0, 0x8c / 255.0, 0x00 / 255.0, Quantity_TOC_sRGB);
const Quantity_Color kPlaneColor(0.95, 0.70, 0.35, Quantity_TOC_RGB);
const Quantity_Color kAxisColor(0.85, 0.45, 0.10, Quantity_TOC_RGB);
const Quantity_Color kHiddenEdgeColor(0.45, 0.48, 0.54, Quantity_TOC_RGB);
// Section Analysis' caps: a colour bodies seldom have.
const QColor kCapColor(0xf2, 0x9e, 0x1a);
// How far the hatch lines lie off the caps, of the caps' size (see hatchOf).
constexpr double kHatchLift = 5e-3;
constexpr double kHatchWidth = 1.5;
// The section's order in depth (mitcad#784). What lies in the cut plane
// comes in three layers at the same depth: a sketch's curves and points on
// it (Hide Above Sketch, depth-tested lines and markers, which OpenGL's
// polygon offset does not move), the caps, and the cut bodies' own faces
// under them (the same triangles). The caps go just behind the plane and
// the cut bodies' faces behind the caps, so that the sketch is always in
// front of both: units (the depth buffer's resolution) for a plane face on,
// a factor of the faces' depth slope for one seen at an angle, large enough
// for the pixels of a 2 px line, its 6 px halo and a point's marker beside
// the line's or the marker's centre, whose depth they take. The cut bodies'
// faces take the caps' factor (the same plane, the same slope) and more
// units. Geometry beyond the plane still hides the sketch: these are a few
// units of depth, not a distance.
constexpr float kSectionOffsetFactor = 4.0f;
constexpr float kSectionCapOffsetUnits = 2.0f;
constexpr float kSectionedBodyOffsetUnits = 6.0f;
// Cosmetic threads' rings: darker than the body, lighter than its edges.
const Quantity_Color kThreadColor(0.30, 0.33, 0.40, Quantity_TOC_RGB);
// The build volume's lines (mitcad#640): a muted blue.
const Quantity_Color kBuildVolumeColor(0.25, 0.45, 0.75, Quantity_TOC_RGB);

// A left press that moves further than this is a window selection.
constexpr double kDragPixels = 4.0;
// Whether a body edge is in sight (mitcad#217): a face nearer the eye than
// this (mm, the geometric tolerance of the imports) hides the point of the
// edge; the faces that meet at the edge pass through it.
constexpr double kOcclusionTolerance = 0.01;
// Points along an edge that a window classifies, its ends left out.
constexpr int kSightSamples = 32;
// The orientation cube's corner of the view and its centre's distance from the
// corner are OcctViewer::kCubeArea and kCubeOffset (widget pixels; the cube
// itself is drawn in device pixels and scaled by the device pixel ratio, see
// applyDevicePixelRatio).
constexpr int kCubeOffset = OcctViewer::kCubeOffset;
// A press on the cube orbits the view (Aspect_VKeyFlags of its own, which
// no key gives).
constexpr Aspect_VKeyFlags kCubeDrag = Aspect_VKeyFlags_META;
constexpr Aspect_VKeyFlags kF4Drag = Aspect_VKeyFlags_META | Aspect_VKeyFlags_ALT;
// Ctrl+Shift with a click or a window (mitcad#217: add and widen at once).
constexpr Aspect_VKeyFlags kCtrlShift = Aspect_VKeyFlags_CTRL | Aspect_VKeyFlags_SHIFT;
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
// After a release, when the navigation ends at the latest if no frame is
// drawn (ms).
constexpr int kReleaseFrameMilliseconds = 100;

// The cube's place in the upper right corner: its centre's distance from the
// corner in device pixels (the overlay insets move the corner). As the view
// projects it, or drawn and picked orthographic also in a perspective view
// (mitcad#766; mitcad#774: the cube's own perspective is made on its
// geometry, which is then drawn orthographic).
occ::handle<Graphic3d_TransformPers> cubePersistence(qreal ratio, const QMargins& insets, bool orthographic) {
  const NCollection_Vec2<int> offset(qRound((kCubeOffset + insets.right()) * ratio),
                                     qRound((kCubeOffset + insets.top()) * ratio));
  if (orthographic) {
    return new Graphic3d_TransformPers(Graphic3d_TMF_TriedronPers | Graphic3d_TMF_OrthoPers,
                                       Aspect_TOTP_RIGHT_UPPER, offset);
  }
  return new Graphic3d_TransformPers(Graphic3d_TMF_TriedronPers, Aspect_TOTP_RIGHT_UPPER, offset);
}

// The camera math of the buttons that turn the view (mitcad#773). OCCT's
// gp_Dir constructors and gp_Dir::Crossed throw Standard_ConstructionError
// for a null result: inline in this code, while OCCT's own libraries are
// built without those checks and go on with a direction that is no number.
// An exception out of a button's slot ended the application, so these never
// throw: a cross of parallel directions is nothing, a direction across
// another is made from the model's axes when the wanted one lies along it.
bool isFinite(const gp_XYZ& v) { return std::isfinite(v.X()) && std::isfinite(v.Y()) && std::isfinite(v.Z()); }

std::optional<gp_Dir> crossedDir(const gp_Dir& a, const gp_Dir& b) {
  const gp_XYZ cross = a.XYZ().Crossed(b.XYZ());
  const double length = cross.Modulus();
  if (!std::isfinite(length) || length <= 1e-9) {
    return std::nullopt;
  }
  return gp_Dir(cross.X() / length, cross.Y() / length, cross.Z() / length);
}

// The model axis nearest a direction: its largest component's (Z for a
// direction that is no number).
gp_Dir nearestAxis(const gp_XYZ& d) {
  const double x = std::abs(d.X());
  const double y = std::abs(d.Y());
  const double z = std::abs(d.Z());
  if (x >= y && x >= z) {
    return gp_Dir(d.X() < 0.0 ? -1.0 : 1.0, 0.0, 0.0);
  }
  if (y >= z) {
    return gp_Dir(0.0, d.Y() < 0.0 ? -1.0 : 1.0, 0.0);
  }
  return gp_Dir(0.0, 0.0, d.Z() < 0.0 ? -1.0 : 1.0);
}

// `wanted` made square to the unit `direction`; where it lies along the
// direction (or is no number), the model's Z made so, or Y when the
// direction lies along Z.
gp_Dir upAcross(const gp_Dir& direction, const gp_XYZ& wanted) {
  const gp_XYZ d = direction.XYZ();
  gp_XYZ across = wanted - d * wanted.Dot(d);
  double length = across.Modulus();
  if (!std::isfinite(length) || length <= 1e-9) {
    const gp_XYZ fallback = std::abs(d.Z()) < 0.9 ? gp_XYZ(0.0, 0.0, 1.0) : gp_XYZ(0.0, 1.0, 0.0);
    across = fallback - d * fallback.Dot(d);
    length = across.Modulus();
  }
  return gp_Dir(across.X() / length, across.Y() / length, across.Z() / length);
}

// How far the eye goes from the centre for a turn whose camera has no
// usable distance (mm; a fit follows).
constexpr double kTurnDistance = 1000.0;

// How far a turn about `centre` puts the eye from it: the camera's
// distance, unless it is no number or too short for the direction from the
// eye to the centre to come out to the last digits; OCCT keeps the old
// direction for an eye on the centre. An orthographic fit makes the
// distance the depth of the fitted box, none for a flat one seen face on (a
// sketch seen square on), and entering a sketch afterwards kept the old,
// oblique direction (mitcad#1009).
bool turnsAt(double distance, const gp_Pnt& centre) {
  const double shortest = std::max(Precision::Confusion(), 1e-6 * centre.XYZ().Modulus());
  return std::isfinite(distance) && distance > shortest;
}

double turnDistance(double distance, const gp_Pnt& centre) {
  return turnsAt(distance, centre) ? distance : std::max(kTurnDistance, 1e-3 * centre.XYZ().Modulus());
}

// After a fit: an orthographic camera's eye kept off its centre (see
// turnDistance), which shows the same; the orbits, the 3D mouse and the
// turns all need a direction from the eye to the centre (mitcad#1009).
void keepEyeOffCentre(Graphic3d_Camera& camera) {
  if (camera.IsOrthographic() && isFinite(camera.Center().XYZ()) && !turnsAt(camera.Distance(), camera.Center())) {
    camera.SetDistance(turnDistance(camera.Distance(), camera.Center()));
  }
}
// The share of the view's height the origin planes' box takes when the view
// frames the origin (a new design, a fit with nothing shown; mitcad#1000):
// about what a fit into the floating cards' free area gives in a window
// some 1500 pixels wide, but the same at any size, so that a new design's
// origin looks the same however large the window is.
constexpr double kOriginShare = 0.7;

// "1480x900", the view's size in the log lines of mitcad#1000.
QString viewSizeText(const QWidget& view) { return QStringLiteral("%1x%2").arg(view.width()).arg(view.height()); }

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

// How far a line, point or plane outside the active component (mitcad#420)
// is faded towards the background, and how see-through its faces are.
constexpr double kInactiveFade = 0.65;
// How far a sketch shown only for a command (mitcad#546) is faded: a
// little, so it reads as the same sketch that hides again.
constexpr double kTemporaryFade = 0.3;
constexpr double kInactivePlaneTransparency = 0.9;
constexpr double kInactiveProfileTransparency = 0.85;

// A colour faded most of the way to `paper` (the background's middle):
// pale, but still its own hue.
Quantity_Color inactiveColor(const Quantity_Color& color, const QColor& paper, double fade = kInactiveFade) {
  double r = 0.0;
  double g = 0.0;
  double b = 0.0;
  color.Values(r, g, b, Quantity_TOC_sRGB);
  const auto towards = [fade](double from, double to) { return from + (to - from) * fade; };
  return Quantity_Color(towards(r, paper.redF()), towards(g, paper.greenF()), towards(b, paper.blueF()),
                        Quantity_TOC_sRGB);
}

// The hatch's colour on a cap of `color` (mitcad#382): a darker shade of the
// cap's own, lighter than it instead when it is dark already.
QColor capHatchColor(const QColor& color) {
  float h = 0.0f;
  float s = 0.0f;
  float v = 0.0f;
  color.getHsvF(&h, &s, &v);
  v = v > 0.45f ? v * 0.45f : std::min(1.0f, v * 0.45f + 0.55f);
  return QColor::fromHsvF(std::max(h, 0.0f), std::min(1.0f, s * 1.1f), v);
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
  TopLoc_Location location;
  if (BRep_Tool::Surface(TopoDS::Face(face), location).IsNull()) {
    return QStringLiteral("mesh"); // a mesh body's triangles (mitcad#399)
  }
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

// A sketch point's marker or ball as an OCCT marker image (mitcad#946),
// one shared image per look and size (`key`, sketch/PointMarkers.hpp): the
// overlay paints the same over its highlights. Straight-alpha RGBA, as
// OCCT's coloured marker images are; its hover draws the image's shape in
// the hover colour.
occ::handle<Graphic3d_MarkerImage> sketchMarkerImage(const QString& key, const QImage& image) {
  static std::map<QString, occ::handle<Graphic3d_MarkerImage>> images;
  const auto found = images.find(key);
  if (found != images.end()) {
    return found->second;
  }
  const QImage rgba = image.convertToFormat(QImage::Format_RGBA8888);
  occ::handle<Image_PixMap> pixels = new Image_PixMap();
  if (rgba.isNull() ||
      !pixels->InitTrash(Image_Format_RGBA, static_cast<size_t>(rgba.width()), static_cast<size_t>(rgba.height()))) {
    return occ::handle<Graphic3d_MarkerImage>();
  }
  pixels->SetTopDown(true);
  for (int row = 0; row < rgba.height(); ++row) {
    std::memcpy(pixels->ChangeRow(static_cast<size_t>(row)), rgba.constScanLine(row),
                static_cast<size_t>(rgba.width()) * 4);
  }
  occ::handle<Graphic3d_MarkerImage> marker = new Graphic3d_MarkerImage(pixels);
  images.emplace(key, marker);
  return marker;
}

// A point drawn with a marker image: white as the object's colour, which
// OCCT multiplies the image's colours by.
void setPointImage(const occ::handle<AIS_Shape>& object, const occ::handle<Graphic3d_MarkerImage>& image) {
  const Quantity_Color white(Quantity_NOC_WHITE);
  object->SetColor(white);
  occ::handle<Graphic3d_AspectMarker3d> aspect = new Graphic3d_AspectMarker3d(Aspect_TOM_USERDEFINED, white, 1.0);
  aspect->SetMarkerImage(image);
  object->Attributes()->SetPointAspect(new Prs3d_PointAspect(aspect));
}

// Highlights (mitcad#149): a material that only emits its colour (no
// ambient, diffuse or specular reflection), so that light does not change
// it. A lit highlight washed out to white where the light falls on the
// face head-on, as the face itself does, and could not be told from it.
Graphic3d_MaterialAspect flatMaterial(float transparency) {
  Graphic3d_MaterialAspect material(Graphic3d_NameOfMaterial_UserDefined);
  material.SetMaterialType(Graphic3d_MATERIAL_ASPECT); // the colour is the aspect's
  material.SetAmbientColor(Quantity_Color(Quantity_NOC_BLACK));
  material.SetDiffuseColor(Quantity_Color(Quantity_NOC_BLACK));
  material.SetSpecularColor(Quantity_Color(Quantity_NOC_BLACK));
  material.SetEmissiveColor(Quantity_Color(Quantity_NOC_WHITE));
  material.SetTransparency(transparency);
  return material;
}

// How much of the face under a highlight shows through it: the hover's
// and OCCT's own selection's, and the command and idle selections'
// (setHighlights; a whole body's more, as it covers more).
constexpr float kHoverTransparency = 0.5f;
constexpr float kSelectedTransparency = 0.35f;
constexpr float kWholeTransparency = 0.55f;
// A plane picked as a reference: light, so that the plane a construction
// feature makes stands out over it (mitcad#446).
constexpr float kPickedPlaneTransparency = 0.8f;
// The occurrences a selected joint joins (mitcad#523): a faint tint.
constexpr float kLightTransparency = 0.82f;
constexpr float kDatumPreviewTransparency = 0.2f;
// A split's extended tool (mitcad#712): the bodies and the cut show through.
constexpr float kExtendedToolTransparency = 0.6f;
// OCCT's own selection highlight; the selections shown are setHighlights'.
const Quantity_Color kSelectedColor(0x1f / 255.0, 0x7a / 255.0, 0xff / 255.0, Quantity_TOC_sRGB);
// In the sketch points' layer (mitcad#897), a selected point's ball is
// drawn after the plain markers, which keep the default priority, so that
// it covers its point's marker at the same depth.
constexpr Graphic3d_DisplayPriority kSketchPointPriority = Graphic3d_DisplayPriority_Normal;
constexpr Graphic3d_DisplayPriority kSketchPointHighlightPriority = Graphic3d_DisplayPriority_Above;

// One of OCCT's highlight styles (the hover and OCCT's own selection) with
// the look of setHighlights(): faces see-through in a colour that light
// does not change (flatMaterial), so that the face's own shading shows
// through; the face's boundary, edges and vertices opaque in the colour,
// over the faces they lie on. In the top layer, which keeps the scene's
// depth, so that nearer geometry hides the highlight (OCCT draws the
// highlight of a face, edge or vertex in the topmost layer, which clears
// the depth: a face's highlight covered what lay in front of the face). A
// polygon offset brings a face's highlight just in front of the face.
void setHighlightStyle(const occ::handle<Prs3d_Drawer>& style, const Quantity_Color& color,
                       float transparency) {
  // The colour of lines and marks, and of faces through the fill below;
  // opaque, the faces' transparency is the fill's.
  style->SetColor(color);
  style->SetTransparency(0.0f);
  style->SetDisplayMode(AIS_Shaded);
  style->SetZLayer(Graphic3d_ZLayerId_Top);
  occ::handle<Graphic3d_AspectFillArea3d> fill = new Graphic3d_AspectFillArea3d();
  fill->SetInteriorStyle(Aspect_IS_SOLID);
  fill->SetInteriorColor(color);
  fill->SetFrontMaterial(flatMaterial(transparency));
  fill->SetBackMaterial(flatMaterial(transparency));
  fill->SetDistinguishOff();
  style->SetBasicFillAreaAspect(fill);
  // The aspects a face's, edge's or vertex's own highlight is drawn with
  // (the colour above replaces theirs).
  style->SetupOwnShadingAspect();
  style->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, 0.0f, -2.0f);
  style->SetFaceBoundaryDraw(true);
  style->SetFaceBoundaryAspect(new Prs3d_LineAspect(color, Aspect_TOL_SOLID, 2.0));
  style->WireAspect()->SetWidth(3.0);
  style->LineAspect()->SetWidth(3.0);
  style->FreeBoundaryAspect()->SetWidth(3.0);
  style->UnFreeBoundaryAspect()->SetWidth(3.0);
}

// The colour of a body's edges where they are drawn alone (wireframe): its
// appearance's colour, darker, or the default dark edges.
Quantity_Color wireColor(const QColor& color) {
  return color.isValid() ? occtColor(color.darker(170)) : kEdgeColor;
}

// Whether the bodies are drawn with their faces' colours.
bool shadedFaces(VisualStyle visual) {
  return visual == VisualStyle::Shaded || visual == VisualStyle::ShadedVisibleEdges ||
         visual == VisualStyle::ShadedHiddenEdges;
}

// The same faces in the same colours.
bool sameFaceColors(const std::vector<BodyDisplay::FaceLook>& a, const std::vector<BodyDisplay::FaceLook>& b) {
  return std::equal(a.begin(), a.end(), b.begin(), b.end(),
                    [](const BodyDisplay::FaceLook& x, const BodyDisplay::FaceLook& y) {
                      return x.faces == y.faces && x.color == y.color;
                    });
}

// The same faces with the same look in the rendered view.
bool sameFaceLooks(const std::vector<BodyDisplay::FaceLook>& a, const std::vector<BodyDisplay::FaceLook>& b) {
  return std::equal(a.begin(), a.end(), b.begin(), b.end(),
                    [](const BodyDisplay::FaceLook& x, const BodyDisplay::FaceLook& y) {
                      return x.faces == y.faces && x.appearance.sameLook(y.appearance);
                    });
}

// A body a section's plane cuts (Section Analysis, Hide Above Sketch): its
// faces behind the caps drawn on its cut faces, which lie behind the plane
// (mitcad#784, kSectionOffsetFactor). OCCT's own offset leaves a body's
// faces too near the plane: the caps had to come in front of the plane to
// cover them, and hid a sketch's curves and points on it.
void behindSection(const occ::handle<AIS_Shape>& body, bool sectioned) {
  if (!sectioned) {
    return;
  }
  const occ::handle<Prs3d_Drawer>& drawer = body->Attributes();
  if (!drawer->HasOwnShadingAspect()) {
    drawer->SetupOwnShadingAspect();
  }
  drawer->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, kSectionOffsetFactor,
                                                       kSectionedBodyOffsetUnits);
}

occ::handle<AIS_Shape> makeBodyObject(const geometry::Shape& named, BodyDisplay::Style style, const QColor& color,
                                      const std::vector<BodyDisplay::FaceLook>& faceLooks, VisualStyle visual,
                                      const QColor& paper, bool sectioned) {
  const TopoDS_Shape& shape = named.occt();
  if (style == BodyDisplay::Style::New) {
    occ::handle<AIS_Shape> body = new AIS_Shape(shape);
    body->SetColor(kPreviewColor);
    behindSection(body, sectioned);
    body->SetTransparency(0.5);
    return body;
  }
  if (style == BodyDisplay::Style::Tool || style == BodyDisplay::Style::CutTool) {
    // As the command's tool preview (setPreview), but pickable (mitcad#561).
    occ::handle<AIS_Shape> body = new AIS_Shape(shape);
    body->SetColor(style == BodyDisplay::Style::CutTool ? kCutPreviewColor : kPreviewColor);
    behindSection(body, sectioned);
    body->SetTransparency(0.5);
    return body;
  }
  if (style == BodyDisplay::Style::Ghost) {
    // Faint enough to see and pick what lies behind (mitcad#147).
    occ::handle<AIS_Shape> body = new AIS_Shape(shape);
    body->SetColor(color.isValid() ? occtColor(color) : kBodyColor);
    behindSection(body, sectioned);
    body->SetTransparency(0.8);
    return body;
  }
  // Faces with appearances of their own (mitcad#53) in their colours, in
  // the shaded styles.
  const bool faceColors = !faceLooks.empty() && shadedFaces(visual);
  occ::handle<AIS_ColoredShape> colored = faceColors ? new AIS_ColoredShape(shape) : nullptr;
  occ::handle<AIS_Shape> body = faceColors ? occ::handle<AIS_Shape>(colored) : new AIS_Shape(shape);
  body->SetColor(color.isValid() ? occtColor(color) : kBodyColor);
  // Before the faces' own colours: their aspects copy the body's.
  behindSection(body, sectioned);
  if (faceColors) {
    for (const BodyDisplay::FaceLook& look : faceLooks) {
      for (const int face : look.faces) {
        if (face >= 0 && face < named.face_count()) {
          colored->SetCustomColor(named.face(face), look.color.isValid() ? occtColor(look.color) : kBodyColor);
          // Their edges stay the body's.
          if (visual == VisualStyle::ShadedVisibleEdges) {
            colored->CustomAspects(named.face(face))
                ->SetFaceBoundaryAspect(new Prs3d_LineAspect(kEdgeColor, Aspect_TOL_SOLID, 1.0));
          }
        }
      }
    }
  }
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

// A body's presentation. One with faces OCCT's mesher failed on when the
// job meshed it for display is never meshed here, on the UI thread
// (viewMeshes, mitcad#589): its presentation and selection draw the faces
// that have a triangulation. Nor is one a job meshed to the end (or an
// open gave its stored meshes, drawerMeshes, mitcad#878): its drawers would
// run the mesher over it again, only to keep every face.
// Whether a face of the shape has a triangulation (a job meshed it).
bool hasAnyTriangulation(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (!BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

occ::handle<AIS_Shape> makeBody(const geometry::Shape& named, BodyDisplay::Style style, const QColor& color,
                                const std::vector<BodyDisplay::FaceLook>& faceLooks, VisualStyle visual,
                                const QColor& paper, bool sectioned) {
  occ::handle<AIS_Shape> body = makeBodyObject(named, style, color, faceLooks, visual, paper, sectioned);
  if (!drawerMeshes(named)) {
    body->Attributes()->SetAutoTriangulation(false);
  }
  return body;
}

} // namespace

// The rendered view's image as a rectangle over the whole view (2D
// persistence, in device pixels from the lower left corner), unlit and
// textured with the image.
// Installed only for timing runs, before the quad's first primitive. OCCT
// creates and uploads the texture lazily in ApplyAspects(), on this context.
class RenderedUploadTiming final : public OpenGl_Element {
public:
  explicit RenderedUploadTiming(QString detail) : m_detail(std::move(detail)) {}

  void Render(const occ::handle<OpenGl_Workspace>& workspace) const override {
    if (m_measured) {
      return;
    }
    m_measured = true;
    static const bool synchronize = qEnvironmentVariableIntValue("MITCAD_RENDER_TEST_SYNC_UPLOAD") == 1;
    const auto& gl = workspace->GetGlContext();
    if (synchronize) {
      ScopedTiming drain("render upload prior drain");
      drain.setDetail(m_detail);
      gl->core11fwd->glFinish();
    }
    ScopedTiming upload("render upload submit");
    workspace->ApplyAspects();
    upload.setDetail(m_detail);
    upload.finish();
    if (synchronize) {
      ScopedTiming wait("render upload completion wait");
      wait.setDetail(m_detail);
      gl->core11fwd->glFinish();
    }
  }

  void Release(OpenGl_Context*) override {}
  bool IsFillDrawMode() const override { return true; }

private:
  QString m_detail;
  mutable bool m_measured = false;
};

#ifdef __APPLE__
class RenderedSurfaceElement final : public OpenGl_Element {
public:
  explicit RenderedSurfaceElement(std::shared_ptr<render::MacFrameDisplay> display) : m_display(std::move(display)) {}
  void Render(const occ::handle<OpenGl_Workspace>&) const override { m_display->draw(); }
  void Release(OpenGl_Context* context) override { m_display->release(context); }
  bool IsFillDrawMode() const override { return true; }
private:
  std::shared_ptr<render::MacFrameDisplay> m_display;
};
#endif

class OcctViewer::RenderedImageQuad : public AIS_InteractiveObject {
public:
  RenderedImageQuad() {
    SetTransformPersistence(new Graphic3d_TransformPers(Graphic3d_TMF_2d, Aspect_TOTP_LEFT_LOWER));
    SetInfiniteState(true);
  }

  // True when the size changed (the rectangle must be computed again).
  bool setImage(const occ::handle<Graphic3d_Texture2D>& texture, int width, int height,
                const QString& timingDetail) {
    bool resized = width != m_width || height != m_height || texture != m_texture;
#ifdef __APPLE__
    resized = resized || static_cast<bool>(m_surface);
    m_surface.reset();
#endif
    m_texture = texture;
    m_width = width;
    m_height = height;
    m_timingDetail = timingDetail;
    return resized;
  }

#ifdef __APPLE__
  bool setSurface(const std::shared_ptr<render::MacFrameDisplay>& display) {
    const bool changed = m_surface != display;
    m_surface = display;
    m_texture.Nullify();
    return changed;
  }
#endif

  bool AcceptDisplayMode(const int mode) const override { return mode == 0; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>&, const occ::handle<Prs3d_Presentation>& presentation,
               const int) override {
#ifdef __APPLE__
    if (m_surface) {
      occ::handle<Graphic3d_AspectFillArea3d> aspect = new Graphic3d_AspectFillArea3d();
      aspect->SetShadingModel(Graphic3d_TypeOfShadingModel_Unlit);
      const auto group = presentation->NewGroup();
      group->SetGroupPrimitivesAspect(aspect);
      // A custom GL element has no OCCT primitive array to establish bounds.
      // Keep its infinite 2D presentation nonempty and out of model fitting.
      group->SetMinMaxValues(0, 0, 0, 1, 1, 0);
      const auto glGroup = occ::handle<OpenGl_Group>::DownCast(group);
      if (!glGroup.IsNull()) glGroup->AddElement(new RenderedSurfaceElement(m_surface));
      return;
    }
#endif
    if (m_texture.IsNull() || m_width <= 0 || m_height <= 0) {
      return;
    }
    const auto w = static_cast<double>(m_width);
    const auto h = static_cast<double>(m_height);
    occ::handle<Graphic3d_ArrayOfTriangles> quad = new Graphic3d_ArrayOfTriangles(4, 6, Graphic3d_ArrayFlags_VertexTexel);
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
    aspect->SetTextureMap(m_texture);
    aspect->SetTextureMapOn();
    const occ::handle<Graphic3d_Group> group = presentation->NewGroup();
    group->SetGroupPrimitivesAspect(aspect);
    if (timingLogged()) {
      const auto glGroup = occ::handle<OpenGl_Group>::DownCast(group);
      if (!glGroup.IsNull()) {
        glGroup->AddElement(new RenderedUploadTiming(m_timingDetail));
      }
    }
    group->AddPrimitiveArray(quad);
  }
  void ComputeSelection(const occ::handle<SelectMgr_Selection>&, const int) override {}

private:
#ifdef __APPLE__
  std::shared_ptr<render::MacFrameDisplay> m_surface;
#endif
  occ::handle<Graphic3d_Texture2D> m_texture;
  int m_width = 0;
  int m_height = 0;
  QString m_timingDetail;
};

namespace {

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

// A hatch over the section caps: lines at 45 degrees to the plane's x axis
// (135 degrees when `mirrored`), lifted a little off the caps on the normal
// side (the side the cut took away) so that they are drawn over the caps'
// fill. Each cap face gets its own spacing: about eight lines across a
// small face, but never closer than 40 lines across all the caps (a large
// face does not turn into a solid colour) nor wider than a 400th of them.
TopoDS_Shape hatchOf(const TopoDS_Shape& caps, const gp_Ax3& plane, bool mirrored) {
  Bnd_Box box;
  BRepBndLib::Add(caps, box, false);
  if (box.IsVoid()) {
    return TopoDS_Shape();
  }
  const double size = std::sqrt(box.SquareExtent());
  const gp_Vec x(plane.XDirection());
  const gp_Vec y(plane.YDirection());
  const gp_Vec along = mirrored ? (x - y) / std::sqrt(2.0) : (x + y) / std::sqrt(2.0);
  const gp_Vec across = mirrored ? (x + y) / std::sqrt(2.0) : (x - y) / std::sqrt(2.0);
  const gp_Pnt origin = plane.Location();
  const gp_Vec lift = gp_Vec(plane.Direction()) * (size * kHatchLift);
  TopoDS_Compound lines;
  BRep_Builder builder;
  builder.MakeCompound(lines);
  bool any = false;
  for (TopExp_Explorer faces(caps, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face& face = TopoDS::Face(faces.Current());
    Bnd_Box faceBox;
    BRepBndLib::Add(face, faceBox, false);
    if (faceBox.IsVoid()) {
      continue;
    }
    const double spacing =
        std::max(size / 400.0, std::min(std::sqrt(faceBox.SquareExtent()) / 8.0, size / 40.0));
    // Per line k at (k + 0.5) * spacing across: its pieces along, from every
    // triangle of the face it crosses.
    std::map<long, std::vector<std::pair<double, double>>> pieces;
    TopLoc_Location location;
    occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull()) {
      BRepMesh_IncrementalMesh(face, size * 1e-3, false, 0.5);
      mesh = BRep_Tool::Triangulation(face, location);
      if (mesh.IsNull()) {
        continue;
      }
    }
    const gp_Trsf toModel = location.Transformation();
    for (int t = 1; t <= mesh->NbTriangles(); ++t) {
      int nodes[3];
      mesh->Triangle(t).Get(nodes[0], nodes[1], nodes[2]);
      double s[3];
      double u[3];
      for (int i = 0; i < 3; ++i) {
        const gp_Vec p(origin, mesh->Node(nodes[i]).Transformed(toModel));
        s[i] = p.Dot(across) / spacing - 0.5;
        u[i] = p.Dot(along);
      }
      const auto first = static_cast<long>(std::ceil(std::min({s[0], s[1], s[2]})));
      const auto last = static_cast<long>(std::floor(std::max({s[0], s[1], s[2]})));
      for (long k = first; k <= last; ++k) {
        double low = std::numeric_limits<double>::max();
        double high = -low;
        for (int i = 0; i < 3; ++i) {
          const int j = (i + 1) % 3;
          const double a = s[i] - static_cast<double>(k);
          const double b = s[j] - static_cast<double>(k);
          if ((a < 0.0 && b < 0.0) || (a > 0.0 && b > 0.0) || a == b) {
            continue;
          }
          const double at = u[i] + (u[j] - u[i]) * a / (a - b);
          low = std::min(low, at);
          high = std::max(high, at);
        }
        if (high > low) {
          pieces[k].emplace_back(low, high);
        }
      }
    }
    // Each line's pieces joined where they meet (neighbouring triangles).
    for (auto& [k, spans] : pieces) {
      std::sort(spans.begin(), spans.end());
      const gp_Pnt base = origin.Translated(across * ((static_cast<double>(k) + 0.5) * spacing) + lift);
      const double gap = size * 1e-9;
      for (std::size_t i = 0; i < spans.size();) {
        double end = spans[i].second;
        std::size_t next = i + 1;
        while (next < spans.size() && spans[next].first <= end + gap) {
          end = std::max(end, spans[next].second);
          ++next;
        }
        if (end - spans[i].first > gap) {
          builder.Add(lines, BRepBuilderAPI_MakeEdge(base.Translated(along * spans[i].first),
                                                     base.Translated(along * end))
                                 .Edge());
          any = true;
        }
        i = next;
      }
    }
  }
  return any ? TopoDS_Shape(lines) : TopoDS_Shape();
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

struct OcctViewer::Occluder {
  TopoDS_Shape placed; // the body where its occurrence puts it
  Bnd_Box box;
  // Made the first time a line passes the box (it classifies every face).
  std::unique_ptr<IntCurvesFace_ShapeIntersector> intersector;
};

OcctViewer::OcctViewer(QWidget* parent) : QOpenGLWidget(parent) {
  // The presentations' triangulations (BRepMesh through AIS) mesh the faces
  // of a shape on all cores instead of one after another (W1: a design of
  // many bodies displayed several times faster).
  BRepMesh_IncrementalMesh::SetParallelDefault(true);
  // And bound each face's triangles (mitcad#398): a sphere face of metres
  // took millions at the view's 0.01 mm and froze the window.
  useViewMeshing();
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
  // The halos of sketch entities (mitcad#302): drawn just before the top
  // layer that holds the entities, against the scene's depth as they are.
  Graphic3d_ZLayerSettings sketchHalos;
  sketchHalos.SetName("sketch halos");
  sketchHalos.SetClearDepth(false);
  m_viewer->InsertLayerBefore(m_sketchHaloLayer, sketchHalos, Graphic3d_ZLayerId_Top);
  // The sketch points and their selection highlights (mitcad#897): just
  // after the top layer, so that a point's marker is drawn over the
  // selection and hover highlights of the curve it lies on. In the top
  // layer with them, at the same depth, the wider highlight line was drawn
  // after the marker and covered it (a selected spline's fit points could
  // not be seen). Immediate, as the top layer is: OCCT draws the immediate
  // layers after all the others, so a layer that is not would come before
  // the top layer wherever it is in the list. Against the scene's depth as
  // the top layer, so that the model hides points behind it as before; its
  // depth writes stay on, so it is one group with the top layer for picking.
  Graphic3d_ZLayerSettings sketchPoints;
  sketchPoints.SetName("sketch points");
  sketchPoints.SetImmediate(true);
  sketchPoints.SetClearDepth(false);
  m_viewer->InsertLayerAfter(m_sketchPointLayer, sketchPoints, Graphic3d_ZLayerId_Top);
  // Image references drawn through the bodies (mitcad#782): over the
  // bodies and their edges, without depth, before the sketches, their
  // halos, previews and highlights, so that what is traced stays on top.
  // Depth writes stay on, as the hidden edges' do: a layer between the
  // default and the top layer that writes no depth splits them into two
  // groups for picking, and highlights in the top layer (a picked origin
  // plane or profile) then win over closer items. The images write no
  // depth all the same: they are blended, drawn in OCCT's transparent pass,
  // which turns depth writes off.
  Graphic3d_ZLayerSettings images;
  images.SetName("image references");
  images.SetEnableDepthTest(false);
  images.SetClearDepth(false);
  m_viewer->InsertLayerBefore(m_imageLayer, images, m_sketchHaloLayer);
  // A feature's input lines where they were (mitcad#957), in two layers of
  // their own after the top layer and the points' layer, before the
  // topmost one. First their dashed copy, without depth: over the bodies,
  // their edges and the top layer's see-through faces. Then the line in
  // sight, against the scene's depth, which covers the copy where nothing
  // hides it; only the parts a solid hides show dashed.
  // The copy writes no depth: without the test OCCT draws with every depth
  // passing and would write the hidden line's depth, through which faces
  // behind the solid would show over it. Both are immediate, as the top
  // layer is: OCCT lists (and picks by) the layers that are not immediate
  // first, and one that writes no depth starts a new group for picking, so
  // that layer before the top one split the bodies' group from the top
  // layer's, whose selection highlights then won over the profiles and
  // faces under them (the profile picks of tools/ui-profile-split-test.sh).
  // Here the groups change only after the points' layer.
  Graphic3d_ZLayerSettings inputLines;
  inputLines.SetName("feature input lines, hidden");
  inputLines.SetImmediate(true);
  inputLines.SetEnableDepthTest(false);
  inputLines.SetEnableDepthWrite(false);
  inputLines.SetClearDepth(false);
  m_viewer->InsertLayerBefore(m_inputLineLayer, inputLines, Graphic3d_ZLayerId_Topmost);
  Graphic3d_ZLayerSettings inputLinesInSight;
  inputLinesInSight.SetName("feature input lines, in sight");
  inputLinesInSight.SetImmediate(true);
  inputLinesInSight.SetClearDepth(false);
  m_viewer->InsertLayerBefore(m_inputLineSightLayer, inputLinesInSight, Graphic3d_ZLayerId_Topmost);
  // The rendered view (docs/rendering.md, "Overlays in depth"): the bodies
  // in a layer of their own, then the rendered image over the whole view
  // without depth (it covers their colour, their depth stays), then the
  // default layer's sketches, datums and previews, which the bodies' depth
  // hides as in the shaded view.
  Graphic3d_ZLayerSettings renderedBodies;
  renderedBodies.SetName("rendered bodies");
  renderedBodies.SetClearDepth(false);
  m_viewer->InsertLayerBefore(m_renderedBodyLayer, renderedBodies, Graphic3d_ZLayerId_Default);
  Graphic3d_ZLayerSettings renderedImage;
  renderedImage.SetName("rendered image");
  renderedImage.SetEnableDepthTest(false);
  renderedImage.SetEnableDepthWrite(false);
  renderedImage.SetClearDepth(false);
  m_viewer->InsertLayerBefore(m_renderedImageLayer, renderedImage, Graphic3d_ZLayerId_Default);
  m_renderedQuad = new RenderedImageQuad();

  m_context = makePickContext(m_viewer);
  // The active component first in picks (mitcad#420): its items before
  // nearer ones of other components.
  mitcad::setPickPriority(m_context, [this](const occ::handle<SelectMgr_EntityOwner>& owner) {
    if (!m_activePickPriority || !m_activeScope.on || owner.IsNull()) {
      return true; // no preference: the order by depth
    }
    // Planes by their depth (setPlanesByDepth): the origin's planes count
    // as the active component's, but a face of another component in front
    // of them is the pick (mitcad#762).
    if (m_planesByDepth) {
      const auto item = itemFor(owner);
      if (item && item->kind == SelectKind::Plane) {
        return false;
      }
    }
    // What is not the model's (the orientation cube) keeps its place.
    const auto info = m_pickInfo.find(dynamic_cast<const AIS_InteractiveObject*>(owner->Selectable().get()));
    return info == m_pickInfo.end() || !outsideActive(info->second.item);
  });
  // Construction planes behind the model while a placement input takes
  // picks (mitcad#656, setPlanesBehind).
  mitcad::setPickLast(m_context, [this](const occ::handle<SelectMgr_EntityOwner>& owner) {
    if (!m_planesBehind || owner.IsNull()) {
      return false;
    }
    const auto item = itemFor(owner);
    return item && item->kind == SelectKind::Plane;
  });
  // Sketch curves and points behind the bodies' faces, edges and vertices
  // while an input asks for it (mitcad#869, setSketchesBehind).
  mitcad::setPickYield(
      m_context,
      [this](const occ::handle<SelectMgr_EntityOwner>& owner) {
        if (!m_sketchesBehind || owner.IsNull()) {
          return false;
        }
        const auto info =
            m_pickInfo.find(dynamic_cast<const AIS_InteractiveObject*>(owner->Selectable().get()));
        if (info == m_pickInfo.end()) {
          return false;
        }
        const SelectionItem& item = info->second.item;
        return (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) &&
               (m_sketchKept.isEmpty() || item.owner != m_sketchKept);
      },
      [this](const occ::handle<SelectMgr_EntityOwner>& owner) {
        if (!m_sketchesBehind || owner.IsNull()) {
          return false;
        }
        // A body's face, edge or vertex, or the whole body.
        const auto info =
            m_pickInfo.find(dynamic_cast<const AIS_InteractiveObject*>(owner->Selectable().get()));
        return info != m_pickInfo.end() && info->second.body != nullptr;
      });
  mitcad::setPickSources(m_context, [this](const occ::handle<SelectMgr_EntityOwner>& owner)
      -> const SelectMgr_SelectableObject* {
    const auto* selectable = owner->Selectable().get();
    const auto info = m_pickInfo.find(dynamic_cast<const AIS_InteractiveObject*>(selectable));
    if (info == m_pickInfo.end() || !info->second.body) { return selectable; }
    const SelectionItem& item = info->second.item;
    // All overlays of this exact body occurrence share the same pick
    // identity, even when the preview no longer displays the source body.
    for (const auto& [object, other] : m_pickInfo) {
      if (other.body && other.item.owner == item.owner && other.item.occurrence == item.occurrence &&
          other.body->occt().IsSame(info->second.body->occt())) {
        return object;
      }
    }
    return selectable;
  });
  mitcad::setPickCovers(m_context, [this](const occ::handle<SelectMgr_EntityOwner>& owner)
      -> const SelectMgr_SelectableObject* {
    const auto info = m_pickInfo.find(dynamic_cast<const AIS_InteractiveObject*>(owner->Selectable().get()));
    if (info == m_pickInfo.end() || !info->second.highlight || !info->second.body) { return nullptr; }
    const SelectionItem& item = info->second.item;
    // The body a command's preview shows changed under the overlay: its
    // own shape is no longer the overlay's original topology.
    for (const auto& [object, other] : m_pickInfo) {
      if (!other.highlight && other.body && other.item.owner == item.owner &&
          other.item.occurrence == item.occurrence && !other.body->occt().IsSame(info->second.body->occt())) {
        return object;
      }
    }
    return nullptr;
  });
  m_context->SetPixelTolerance(qRound(kPickTolerance * devicePixelRatioF()));
  m_pickFilter = new Filter(this);
  m_context->AddFilter(m_pickFilter);
  // The hover of a sketch point, in the points' layer (mitcad#897): in the
  // top layer, under the plain marker drawn after it, it would not show.
  // Linked to OCCT's own hover style for the aspects it does not set.
  m_sketchPointHover = new Prs3d_Drawer();
  m_sketchPointHover->SetLink(m_context->HighlightStyle(Prs3d_TypeOfHighlight_Dynamic));
  setHoverColor(QColor(0x2f, 0x9b, 0xff));
  // OCCT's own selection is cleared as soon as it is made
  // (OnSelectionChanged); should it show, it looks as setHighlights' does.
  for (const Prs3d_TypeOfHighlight type :
       {Prs3d_TypeOfHighlight_Selected, Prs3d_TypeOfHighlight_LocalSelected}) {
    setHighlightStyle(m_context->HighlightStyle(type), kSelectedColor, kSelectedTransparency);
  }
  m_grid = new LayoutGrid();
  m_grid->SetZLayer(m_gridLayer);
  setGridColors(*m_grid, m_paper);

  // In glass of its own (mitcad#430).
  m_orientationCube = makeViewCube();
  m_orientationCube->SetViewAnimation(ViewAnimation());
  m_orientationCube->SetFixedAnimationLoop(false);
  m_orientationCube->SetAutoStartAnimation(true);
  // In the upper right corner, with its labels in capitals.
  m_orientationCube->SetTransformPersistence(cubePersistence(1.0, QMargins(), false));
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
  m_navigationClock.start();
  m_navigationTimer = new QTimer(this);
  m_navigationTimer->setSingleShot(true);
  connect(m_navigationTimer, &QTimer::timeout, this, &OcctViewer::updateNavigation);
  TestSync::watch(m_navigationTimer);
  const bool glass = chromeStyle() == ChromeStyle::Floating;
  // A capsule of the cube's glass (mitcad#430).
  m_homeButton = new ViewCubeHomeButton(this);
  m_homeButton->setObjectName(QStringLiteral("viewHome"));
  m_homeButton->setIcon(overlayIcon(QStringLiteral("home"), IconBackdrop::Light));
  m_homeButton->setIconSize(QSize(14, 14));
  m_homeButton->setFixedSize(QSize(36, 24));
  m_homeButton->setAutoRaise(true);
  m_homeButton->setFocusPolicy(Qt::NoFocus);
  m_homeButton->setToolTip(tr("Home: the home view of the whole design"));
  connect(m_homeButton, &QToolButton::clicked, this, [this] {
    qDebug() << "View home";
    runViewOperation(QStringLiteral("home"), [this] { goHome(); });
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
    connect(arrow, &QToolButton::clicked, this, [this, name] { pressCubeArrow(name); });
    m_cubeArrows.emplace_back(name, arrow);
  }
  applyDevicePixelRatio(); // the ratio of the screen the view starts on

  m_view = m_viewer->CreateView();
  m_view->SetImmediateUpdate(false);
  m_view->ChangeRenderingParams().NbMsaaSamples = 4;
  // Diagnostic overrides only: retain the normal four-sample policy until
  // physical-display measurements justify changing it (mitcad#552).
  const QString aa = qEnvironmentVariable("MITCAD_VIEWPORT_AA");
  if (aa == QLatin1String("msaa2")) {
    m_view->ChangeRenderingParams().NbMsaaSamples = 2;
  } else if (aa == QLatin1String("off") || aa == QLatin1String("ray-aa")) {
    m_view->ChangeRenderingParams().NbMsaaSamples = 0;
    // OCCT 8's flag affects its ray tracer, not this raster viewport.
    m_view->ChangeRenderingParams().IsAntialiasingEnabled = aa == QLatin1String("ray-aa");
  } else if (!aa.isEmpty() && aa != QLatin1String("msaa4")) {
    qWarning() << "Unknown MITCAD_VIEWPORT_AA; keeping MSAA 4:" << aa;
  }
  if (timingLogged()) {
    connect(this, &QOpenGLWidget::frameSwapped, this, [clock = QElapsedTimer()]() mutable {
      if (clock.isValid()) {
        qDebug().noquote() << QStringLiteral("Timing frame swap: %1 ms")
                                  .arg(static_cast<double>(clock.nsecsElapsed()) / 1.0e6, 0, 'f', 2);
      }
      clock.start();
    });
  }
  m_view->SetBgGradientColors(Quantity_Color(0.93, 0.94, 0.96, Quantity_TOC_RGB),
                              Quantity_Color(0.74, 0.77, 0.82, Quantity_TOC_RGB),
                              Aspect_GradientFillMethod_Vertical);
  m_view->SetProj(V3d_TypeOfOrientation_Zup_AxoRight);

  bindGestures();
  // The view reports picks and keeps no selection of its own, so every pick
  // replaces OCCT's selection; the owner of the view decides what a pick
  // with Ctrl or Shift does (in sketch mode Ctrl+Shift too, mitcad#217). A
  // click on the orientation cube is a pick of it.
  auto& schemes = ChangeMouseSelectionSchemes();
  schemes.Clear();
  for (const Aspect_VKeyFlags keys : std::initializer_list<Aspect_VKeyFlags>{
           Aspect_VKeyFlags_NONE, Aspect_VKeyFlags_CTRL, Aspect_VKeyFlags_SHIFT, kCtrlShift, kCubeDrag}) {
    schemes.Bind(Aspect_VKeyMouse_LeftButton | keys, AIS_SelectionScheme_Replace);
  }
  // Object dragging is meant for manipulators, which the view does not have
  // yet. Left unbound it turns a left drag into a view rotation that OCCT 8.0
  // starts without an initial camera state and fails (gp_Dir::Cross).
  SetAllowDragging(false);

  setMouseTracking(true);
  setFocusPolicy(Qt::StrongFocus);
  setUpdateBehavior(QOpenGLWidget::NoPartialUpdate);
  // Shift widens a selection scope's clicks wherever the keyboard is
  // (eventFilter, mitcad#217).
  if (QCoreApplication::instance() != nullptr) {
    QCoreApplication::instance()->installEventFilter(this);
  }

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

#ifdef __APPLE__
  QObject::disconnect(m_renderedContextCleanup);
  releaseRenderedSurfaceContext();
#endif

  if (QCoreApplication::instance() != nullptr) {
    QCoreApplication::instance()->removeEventFilter(this);
  }
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
  const int selectionMode = info.body
      ? AIS_Shape::SelectionMode(info.highlight && info.item.kind == SelectKind::Edge ? TopAbs_EDGE : TopAbs_FACE)
      : 0;
  m_pickInfo[object.get()] = std::move(info);
  m_context->Display(object, displayMode, selectionMode, false);
  activatePicking(object);
}

void OcctViewer::remove(const occ::handle<AIS_Shape>& object) {
  m_pickInfo.erase(object.get());
  m_inactiveObjects.erase(object.get());
  m_context->Remove(object, false);
}

void OcctViewer::activatePicking(const occ::handle<AIS_Shape>& object) {
  const auto info = m_pickInfo.find(object.get());
  if (info == m_pickInfo.end()) {
    return;
  }
  m_context->Deactivate(object);
  // A highlight on a ghost lets a click through to what lies behind
  // (mitcad#147); its item is deselected in the panel.
  if (info->second.highlight && isGhosted(info->second.item.occurrence)) {
    return;
  }
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
  const SelectFilter kinds = activeKinds();
  if (info->second.body) {
    if (kinds.testFlag(SelectKind::Body)) {
      activate(0, kFaceSensitivity);
    }
    if (kinds.testFlag(SelectKind::Face)) {
      activate(AIS_Shape::SelectionMode(TopAbs_FACE), kFaceSensitivity);
    }
    if (kinds.testFlag(SelectKind::Edge)) {
      activate(AIS_Shape::SelectionMode(TopAbs_EDGE), kEdgeSensitivity);
    }
    if (kinds.testFlag(SelectKind::Vertex)) {
      activate(AIS_Shape::SelectionMode(TopAbs_VERTEX), kVertexSensitivity);
    }
  } else if (info->second.highlight || kinds.testFlag(info->second.item.kind)) {
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
  // Bodies kept as they are drawn whose other parameters changed (what the
  // rendered view shows, mitcad#46).
  int restyled = 0;
  // Bodies kept as they are drawn, only placed elsewhere (mitcad#838).
  int placed = 0;
  // A design's first bodies coming into an empty view are fitted into it
  // (a design opened, a new design's first body), but not the new bodies a
  // command previews: the view would move under the items still to be
  // picked (mitcad#467). The body the command then makes replaces its
  // preview, so the view stays as the user had it. Once per design
  // (FirstBodiesFit): bodies shown again after the user hid every one stay
  // where the view has them.
  const auto designBodies = [](const std::map<std::string, BodyEntry>& entries) {
    return std::any_of(entries.begin(), entries.end(),
                       [](const auto& entry) { return entry.second.style != BodyDisplay::Style::New; });
  };
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
    // A normal body of an occurrence a command picks through: a ghost.
    const bool ghosted =
        body.style == BodyDisplay::Style::Normal && isGhosted(QString::fromStdString(body.occurrence));
    const BodyDisplay::Style style = ghosted ? BodyDisplay::Style::Ghost : body.style;
    // Outside the active component: drawn as a ghost that stays pickable.
    const bool inactive = isInactive(QString::fromStdString(body.occurrence));
    auto existing = m_bodies.find(key);
    if (existing != m_bodies.end() && existing->second.style == style && existing->second.inactive == inactive &&
        existing->second.object->Shape().IsEqual(shape) &&
        sameTransformation(existing->second.placement.Transformation(), body.placement.Transformation()) &&
        existing->second.component == body.component && existing->second.color == body.color &&
        existing->second.sectioned == body.sectioned &&
        sameFaceColors(existing->second.faceLooks, body.faceLooks)) {
      BodyEntry entry = existing->second;
      entry.shape = body.shape; // the same geometry may come with new names
      entry.ghosted = ghosted;
      if (!entry.appearance.sameLook(body.appearance) || !sameFaceLooks(entry.faceLooks, body.faceLooks)) {
        ++restyled;
      }
      entry.appearance = body.appearance;
      entry.faceLooks = body.faceLooks;
      const auto info = m_pickInfo.find(entry.object.get());
      if (info != m_pickInfo.end()) {
        info->second.body = body.shape;
      }
      m_bodies.erase(existing);
      kept.emplace(key, entry);
      continue;
    }
    if (existing != m_bodies.end() && existing->second.style == style && existing->second.inactive == inactive &&
        existing->second.object->Shape().IsEqual(shape) && existing->second.component == body.component &&
        existing->second.color == body.color && existing->second.sectioned == body.sectioned &&
        sameFaceColors(existing->second.faceLooks, body.faceLooks)) {
      // Only placed elsewhere (a component moved, a Move's drag, mitcad#838):
      // its objects move, nothing is drawn anew from its faces.
      BodyEntry entry = existing->second;
      entry.shape = body.shape;
      entry.ghosted = ghosted;
      if (!entry.appearance.sameLook(body.appearance) || !sameFaceLooks(entry.faceLooks, body.faceLooks)) {
        ++restyled;
      }
      entry.appearance = body.appearance;
      entry.faceLooks = body.faceLooks;
      const auto info = m_pickInfo.find(entry.object.get());
      if (info != m_pickInfo.end()) {
        info->second.body = body.shape;
      }
      placeBody(entry, body.placement);
      ++placed;
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
    entry.component = body.component;
    entry.occurrence = body.occurrence;
    entry.shape = body.shape;
    entry.style = style;
    entry.ghosted = ghosted;
    entry.inactive = inactive;
    entry.placement = body.placement;
    entry.color = body.color;
    entry.appearance = body.appearance;
    entry.faceLooks = body.faceLooks;
    entry.sectioned = body.sectioned;
    showBody(entry);
    ++shown;
    kept.emplace(key, entry);
  }
  timing.setDetail(placed > 0 ? QStringLiteral("%1 bodies, %2 new, %3 placed").arg(bodies.size()).arg(shown).arg(placed)
                              : QStringLiteral("%1 bodies, %2 new").arg(bodies.size()).arg(shown));
  for (auto& [uid, entry] : m_bodies) {
    removeBody(entry);
  }
  const bool changed = shown > 0 || restyled > 0 || placed > 0 || !m_bodies.empty();
  m_bodies = std::move(kept);
  if (m_firstFit.shown(wasEmpty, designBodies(m_bodies))) {
    m_fitPending = true;
    m_framingNewDesign = false; // the bodies, not the origin
  }
  logInactiveGhosts();
  sceneChanged();
  if (changed) {
    emit bodiesChanged();
  }
}

std::vector<BodyDisplay> OcctViewer::shownBodies() const {
  std::vector<BodyDisplay> bodies;
  for (const auto& [key, entry] : m_bodies) {
    if (entry.style != BodyDisplay::Style::Normal && !entry.ghosted) {
      continue;
    }
    BodyDisplay body;
    body.uid = entry.uid;
    body.component = entry.component;
    body.shape = entry.shape;
    body.placement = entry.placement;
    body.occurrence = entry.occurrence;
    body.color = entry.color;
    body.appearance = entry.appearance;
    body.faceLooks = entry.faceLooks;
    body.sectioned = entry.sectioned;
    bodies.push_back(body);
  }
  return bodies;
}

bool OcctViewer::isGhosted(const QString& occurrence) const {
  if (occurrence.isEmpty()) {
    return false;
  }
  return std::any_of(m_ghosts.begin(), m_ghosts.end(), [&occurrence](const QString& ghost) {
    return !ghost.isEmpty() && (occurrence == ghost || occurrence.startsWith(ghost + QLatin1Char('/')));
  });
}

void OcctViewer::setGhosts(const QStringList& occurrences) {
  if (occurrences == m_ghosts) {
    return;
  }
  m_ghosts = occurrences;
  int ghosts = 0;
  for (auto& [key, entry] : m_bodies) {
    const bool ghosted = (entry.style == BodyDisplay::Style::Normal || entry.ghosted) &&
                         isGhosted(QString::fromStdString(entry.occurrence));
    if (ghosted != entry.ghosted) {
      removeBody(entry);
      entry.style = ghosted ? BodyDisplay::Style::Ghost : BodyDisplay::Style::Normal;
      entry.ghosted = ghosted;
      showBody(entry);
    }
    ghosts += ghosted ? 1 : 0;
  }
  // The highlights on ghosts let clicks through; the others take them again.
  for (const occ::handle<AIS_Shape>& highlight : m_highlights) {
    activatePicking(highlight);
  }
  m_context->ClearDetected(false);
  qDebug().noquote() << QStringLiteral("View ghosts: %1")
                            .arg(m_ghosts.isEmpty() ? QStringLiteral("none")
                                                    : QStringLiteral("%1 (%2 %3)")
                                                          .arg(m_ghosts.join(QStringLiteral(", ")))
                                                          .arg(ghosts)
                                                          .arg(ghosts == 1 ? QStringLiteral("body")
                                                                           : QStringLiteral("bodies")));
  sceneChanged();
}

bool OcctViewer::outsideActive(const QString& occurrence) const {
  if (!m_activeScope.on) {
    return false;
  }
  // The root's own items (an empty occurrence) are outside every component.
  return std::none_of(m_activeScope.occurrences.begin(), m_activeScope.occurrences.end(),
                      [&occurrence](const QString& active) {
                        return active.isEmpty() || occurrence == active ||
                               occurrence.startsWith(active + QLatin1Char('/'));
                      });
}

bool OcctViewer::outsideActive(const SelectionItem& item) const {
  return outsideActive(item.occurrence) && !m_activeScope.normal.contains(item.owner);
}

bool OcctViewer::isInactive(const QString& occurrence) const {
  return m_ghostInactive && outsideActive(occurrence);
}

bool OcctViewer::isInactiveItem(const SelectionItem& item) const {
  return m_ghostInactive && outsideActive(item);
}

void OcctViewer::setActivePickPriority(bool on) {
  m_activePickPriority = on;
}

bool OcctViewer::markInactive(const occ::handle<AIS_Shape>& object, bool inactive) {
  if (inactive) {
    return m_inactiveObjects.insert(object.get()).second;
  }
  return m_inactiveObjects.erase(object.get()) > 0;
}

void OcctViewer::setActiveScope(const ActiveScope& scope) {
  if (scope == m_activeScope) {
    return;
  }
  m_activeScope = scope;
  applyInactiveGhosts();
}

void OcctViewer::setGhostInactive(bool on) {
  if (on == m_ghostInactive) {
    return;
  }
  m_ghostInactive = on;
  applyInactiveGhosts();
}

void OcctViewer::applyInactiveGhosts(bool faded) {
  for (auto& [key, entry] : m_bodies) {
    const bool inactive = isInactive(QString::fromStdString(entry.occurrence));
    if (inactive == entry.inactive) {
      continue;
    }
    entry.inactive = inactive;
    // A command's ghosts and previews keep their look.
    if (entry.style == BodyDisplay::Style::Normal) {
      removeBody(entry);
      showBody(entry);
    }
  }
  // Sketch entities, profiles and datums: pale or not, redrawn in place.
  const auto restyle = [this, faded](const occ::handle<AIS_Shape>& object) {
    const auto info = m_pickInfo.find(object.get());
    if (info == m_pickInfo.end()) {
      return false;
    }
    return markInactive(object, isInactiveItem(info->second.item)) || (faded && drawnInactive(object));
  };
  for (const occ::handle<AIS_Shape>& object : m_sketchEntities) {
    const auto look = m_sketchEntityLooks.find(object.get());
    if (look != m_sketchEntityLooks.end() && restyle(object)) {
      styleSketchEntity(object, look->second);
      m_context->Redisplay(object, false);
    }
  }
  for (const auto& [key, profile] : m_profiles) {
    if (restyle(profile)) {
      styleProfile(profile);
      m_context->Redisplay(profile, false);
    }
  }
  for (const occ::handle<AIS_Shape>& datum : m_datums) {
    const auto info = m_pickInfo.find(datum.get());
    if (info != m_pickInfo.end() && restyle(datum)) {
      styleDatum(datum, info->second.item.kind);
      m_context->Redisplay(datum, false);
    }
  }
  m_context->ClearDetected(false);
  logInactiveGhosts();
  sceneChanged();
}

void OcctViewer::logInactiveGhosts() {
  QString line = QStringLiteral("none");
  if (m_ghostInactive && m_activeScope.on) {
    // The bodies drawn as ghosts for it: a command's ghosts among them, not
    // its previews.
    const int ghosts = static_cast<int>(std::count_if(m_bodies.begin(), m_bodies.end(), [](const auto& body) {
      const BodyEntry& entry = body.second;
      return entry.inactive && (entry.style == BodyDisplay::Style::Normal || entry.ghosted);
    }));
    line = QStringLiteral("%1 %2 (active %3)")
               .arg(ghosts)
               .arg(ghosts == 1 ? QStringLiteral("body") : QStringLiteral("bodies"))
               .arg(m_activeScope.name);
  }
  if (line != m_loggedInactiveGhosts) {
    m_loggedInactiveGhosts = line;
    qDebug().noquote() << QStringLiteral("View inactive ghosts: %1").arg(line);
  }
}

void OcctViewer::applyRenderedVisibility(BodyEntry& entry) {
  // The bodies stay drawn (their depth hides the overlays behind them); in
  // the rendered mode they are in a layer of their own under the image.
  if (!entry.object.IsNull() && entry.style == BodyDisplay::Style::Normal) {
    const Graphic3d_ZLayerId layer = m_renderedMode ? m_renderedBodyLayer : Graphic3d_ZLayerId_Default;
    if (entry.object->ZLayer() != layer) {
      m_context->SetZLayer(entry.object, layer);
    }
  }
  // Their edges are in layers above the image: hidden under one.
  const bool visible = !showsRenderedImage();
  for (const occ::handle<AIS_Shape>* object : {&entry.hiddenEdges, &entry.visibleEdges}) {
    if (!object->IsNull()) {
      m_context->SetViewAffinity(*object, m_view, visible);
    }
  }
}

void OcctViewer::setRenderedMode(bool on) {
  if (on == m_renderedMode) {
    return;
  }
  m_renderedMode = on;
  if (!on) {
    setRenderedImage(QImage());
  }
  for (auto& [key, entry] : m_bodies) {
    applyRenderedVisibility(entry);
  }
  m_view->Invalidate();
  requestFrame();
}

void OcctViewer::setRenderedImage(const QImage& image, const QString& timingDetail) {
  const bool was = showsRenderedImage();
#ifdef __APPLE__
  m_renderedSurface.reset();
  if (m_renderedSurfaceDisplay) m_renderedSurfaceDisplay->setFrame({}, {}, {}, {}, {});
#endif
  if (image.isNull()) {
    if (!was) {
      return;
    }
    m_renderedImage = QImage();
    m_context->Erase(m_renderedQuad, false);
    m_renderedQuad->setImage({}, 0, 0, {});
    m_renderedTexture.Nullify();
    m_renderedPixels.Nullify();
  } else {
    ScopedTiming prepare("render texture prepare");
    prepare.setDetail(timingDetail);
    // What the image shows: the camera and size of now.
    m_renderedPose = viewPose();
    // OCCT reads the pixels when it draws: the image stays here meanwhile.
    m_renderedImage = image.convertToFormat(QImage::Format_RGB32);
    m_renderedPixels = new Image_PixMap();
    m_renderedPixels->InitWrapper(Image_Format_BGR32, m_renderedImage.bits(),
                                  static_cast<size_t>(m_renderedImage.width()),
                                  static_cast<size_t>(m_renderedImage.height()),
                                  static_cast<size_t>(m_renderedImage.bytesPerLine()));
    m_renderedPixels->SetTopDown(true);
    // A new texture for each frame: OCCT uploads a texture of an aspect
    // once (a new revision of the same one is not uploaded again).
    // Repeated, not clamped: with clamping OCCT samples only its corner.
    m_renderedTexture = new Graphic3d_Texture2D(m_renderedPixels);
    m_renderedTexture->GetParams()->SetFilter(Graphic3d_TOTF_BILINEAR);
    m_renderedTexture->GetParams()->SetModulate(false);
    // The image over the whole view (stretched: a refining frame is
    // smaller), in its layer between the bodies and the overlays.
    int width = 0;
    int height = 0;
    m_view->Window()->Size(width, height);
    if (m_renderedQuad->setImage(m_renderedTexture, width, height, timingDetail) || !was) {
      m_context->Redisplay(m_renderedQuad, false);
    }
    if (!was) {
      m_context->Display(m_renderedQuad, 0, -1, false);
      m_context->SetZLayer(m_renderedQuad, m_renderedImageLayer);
    }
  }
  if (was != showsRenderedImage()) {
    for (auto& [key, entry] : m_bodies) {
      applyRenderedVisibility(entry);
    }
    updateGrid(true);
  }
  m_view->Invalidate();
  requestFrame();
}

#ifdef __APPLE__
void OcctViewer::releaseRenderedSurfaceContext() {
  if (!m_renderedSurfaceDisplay) return;
  // Qt can replace its context when the viewport is reparented. Drop all
  // context-specific resources while the old native context still exists;
  // the retained frame is redrawn lazily in the replacement context.
  makeCurrent();
  const auto previous = glContextOf(m_view);
  m_renderedSurfaceDisplay->release(previous.get());
}

void OcctViewer::setRenderedSurface(std::shared_ptr<render::MacFrameLease> frame, const render::FrameInfo& info,
                                    const QColor& top, const QColor& bottom, const render::Look& look) {
  if (!frame || info.width == 0 || info.height == 0) { setRenderedImage(QImage()); return; }
  const bool was = showsRenderedImage();
  m_renderedPose = viewPose();
  m_renderedImage = QImage();
  m_renderedTexture.Nullify();
  m_renderedPixels.Nullify();
  m_renderedSurface = std::move(frame);
  if (!m_renderedSurfaceDisplay) m_renderedSurfaceDisplay = std::make_shared<render::MacFrameDisplay>();
  m_renderedSurfaceDisplay->setFrame(m_renderedSurface, info, top, bottom, look);
  if (m_renderedQuad->setSurface(m_renderedSurfaceDisplay) || !was) m_context->Redisplay(m_renderedQuad, false);
  if (!was) {
    m_context->Display(m_renderedQuad, 0, -1, false);
    m_context->SetZLayer(m_renderedQuad, m_renderedImageLayer);
    for (auto& [key, entry] : m_bodies) applyRenderedVisibility(entry);
    updateGrid(true);
  }
  m_view->Invalidate();
  requestFrame();
}
#endif

ViewPose OcctViewer::viewPose() const {
  ViewPose pose;
  if (!m_view->Window().IsNull()) {
    m_view->Window()->Size(pose.width, pose.height);
  }
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  const gp_XYZ eye = camera->Eye().XYZ();
  const gp_XYZ center = camera->Center().XYZ();
  const gp_XYZ up = camera->Up().XYZ();
  for (int i = 0; i < 3; ++i) {
    pose.eye[static_cast<std::size_t>(i)] = eye.Coord(i + 1);
    pose.center[static_cast<std::size_t>(i)] = center.Coord(i + 1);
    pose.up[static_cast<std::size_t>(i)] = up.Coord(i + 1);
  }
  pose.scale = camera->Scale();
  pose.orthographic = camera->IsOrthographic();
  return pose;
}

QString OcctViewer::renderedImageStale() const {
  if (!showsRenderedImage() || m_view->Window().IsNull()) {
    return QString();
  }
  // A tolerant comparison (mitcad#216): OCCT's IsEqual with a tolerance of
  // 0 found every camera moved, and the image was dropped on every frame.
  const ViewPose now = viewPose();
  if (now.width != m_renderedPose.width || now.height != m_renderedPose.height) {
    return QStringLiteral("size changed");
  }
  return sameView(now, m_renderedPose) ? QString() : QStringLiteral("camera moved");
}

void OcctViewer::backgroundColors(QColor& top, QColor& bottom) const {
  top = m_backgroundTop;
  bottom = m_backgroundGradient ? m_backgroundBottom : m_backgroundTop;
}

void OcctViewer::applyBackground() {
  if (m_backgroundGradient) {
    m_view->SetBgGradientColors(occtColor(m_backgroundTop), occtColor(m_backgroundBottom),
                                Aspect_GradientFillMethod_Vertical);
  } else {
    m_view->SetBgGradientStyle(Aspect_GradientFillMethod_None);
    m_view->SetBackgroundColor(occtColor(m_backgroundTop));
  }
}

void OcctViewer::showBody(BodyEntry& entry) {
  const TopoDS_Shape& shape = entry.shape->occt();
  QColor color = entry.color;
  const bool componentColors = m_componentColors && entry.style != BodyDisplay::Style::New;
  if (componentColors) {
    // Stable across visibility changes and sessions; consecutive component
    // ids get hues separated by the golden angle. Unknown ids use a stable hash.
    const QString component = QString::fromStdString(entry.component);
    bool numeric = false;
    quint32 index = component.mid(1).toUInt(&numeric);
    if (!numeric) {
      index = 0;
      for (const unsigned char ch : entry.component) {
        index = index * 31u + ch;
      }
    }
    color = QColor::fromHsv(int((quint64(index) * 137u + 30u) % 360u), 155, 220);
  }
  // A normal body outside the active component (mitcad#420) looks like a
  // command's ghost, and stays pickable.
  const bool inactive = entry.inactive && entry.style == BodyDisplay::Style::Normal;
  entry.object = makeBody(*entry.shape, inactive ? BodyDisplay::Style::Ghost : entry.style, color,
                         componentColors ? std::vector<BodyDisplay::FaceLook>{} : entry.faceLooks,
                         m_visualStyle, m_paper, entry.sectioned);
  const bool placed = !entry.placement.IsIdentity();
  if (placed) {
    entry.object->SetLocalTransformation(entry.placement.Transformation());
  }
  PickInfo info;
  info.item = {SelectKind::Body, QString::fromStdString(entry.uid), QString(), QString()};
  if (geometry::body_kind(shape) == geometry::BodyKind::Mesh) {
    // Inputs of surface operations do not take it (mitcad#399).
    info.item.geometry = QStringLiteral("mesh");
  }
  info.item.occurrence = QString::fromStdString(entry.occurrence);
  info.body = entry.shape;
  const bool normal = entry.style == BodyDisplay::Style::Normal;
  const bool wire = normal && !inactive && m_visualStyle == VisualStyle::Wireframe;
  // A consumed tool where another occurrence shows it is picked there
  // (mitcad#561).
  const bool tool = entry.style == BodyDisplay::Style::Tool || entry.style == BodyDisplay::Style::CutTool;
  display(entry.object, wire ? AIS_WireFrame : AIS_Shaded, std::move(info), normal || tool);
  entry.hiddenEdges.Nullify();
  entry.visibleEdges.Nullify();
  if (!normal || inactive || !drawsHiddenEdges(m_visualStyle)) {
    if (normal) {
      applyRenderedVisibility(entry);
    }
    return;
  }
  const TopoDS_Shape edges = edgesOf(shape);
  entry.hiddenEdges = new AIS_Shape(edges);
  setEdgeStyle(entry.hiddenEdges, kHiddenEdgeColor, 1.0, Aspect_TOL_DASH);
  entry.visibleEdges = new AIS_Shape(edges);
  setEdgeStyle(entry.visibleEdges,
               m_visualStyle == VisualStyle::WireframeHiddenEdges ? wireColor(color) : kEdgeColor,
               1.0);
  for (const auto& [object, layer] :
       {std::pair{entry.hiddenEdges, m_hiddenLayer}, std::pair{entry.visibleEdges, m_edgeLayer}}) {
    if (placed) {
      object->SetLocalTransformation(entry.placement.Transformation());
    }
    m_context->SetZLayer(object, layer);
    m_context->Display(object, AIS_WireFrame, -1, false);
  }
  applyRenderedVisibility(entry);
}

void OcctViewer::placeBody(BodyEntry& entry, const TopLoc_Location& placement) {
  entry.placement = placement;
  entry.occluder.reset(); // its faces placed anew when next asked
  for (const occ::handle<AIS_Shape>* object : {&entry.object, &entry.hiddenEdges, &entry.visibleEdges}) {
    if (!object->IsNull()) {
      m_context->SetLocation(*object, placement);
    }
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

void OcctViewer::setComponentColors(bool on) {
  if (m_componentColors == on) {
    return;
  }
  m_componentColors = on;
  logSectionCaps();
  for (auto& [key, entry] : m_bodies) {
    removeBody(entry);
    showBody(entry);
  }
  sceneChanged();
}

void OcctViewer::setVisualStyle(VisualStyle style) {
  if (style == m_visualStyle) {
    return;
  }
  m_visualStyle = style;
  logSectionCaps();
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
        // Pale outside the active component (mitcad#420).
        if (markInactive(existing->second, isInactiveItem(info->second.item))) {
          styleProfile(existing->second);
          m_context->Redisplay(existing->second, false);
        }
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
    if (markInactive(profile, isInactiveItem(info.item))) {
      styleProfile(profile); // pale outside the active component (mitcad#420)
    }
    display(profile, AIS_Shaded, std::move(info), true);
    m_profiles.emplace(key, profile);
  }
  for (const auto& [key, profile] : shown) {
    remove(profile);
  }
  timing.setDetail(QStringLiteral("%1 profiles, %2 new").arg(profiles.size()).arg(made));
  sceneChanged();
}

void OcctViewer::removeSketchProfiles(const QString& sketch) {
  ScopedTiming timing("defer sketch profiles");
  const std::string uid = sketch.toStdString();
  for (auto it = m_profiles.begin(); it != m_profiles.end();) {
    if (it->first.first == uid) {
      remove(it->second);
      it = m_profiles.erase(it);
    } else {
      ++it;
    }
  }
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
      // Pale outside the active component (mitcad#420): the same entity
      // may be the edited sketch's now, or no longer.
      const occ::handle<AIS_Shape>& kept = existing->second.second;
      const auto look = m_sketchEntityLooks.find(kept.get());
      const auto info = m_pickInfo.find(kept.get());
      if (look != m_sketchEntityLooks.end() && info != m_pickInfo.end() &&
          markInactive(kept, isInactiveItem(info->second.item))) {
        styleSketchEntity(kept, look->second);
        m_context->Redisplay(kept, false);
      }
      m_sketchEntities.push_back(existing->second.second);
      m_sketchEntityKeys.emplace(key, existing->second);
      shown.erase(existing);
      continue;
    }
    ++made;
    occ::handle<AIS_Shape> object = new AIS_Shape(entity.shape);
    const bool isPoint = entity.geometry == QStringLiteral("point");
    SketchEntityLook look;
    look.point = isPoint;
    look.construction = entity.construction;
    look.centerline = entity.centerline;
    look.constrained = entity.constrained;
    look.reference = entity.reference;
    look.temporary = entity.temporary;
    PickInfo info;
    info.item = {isPoint ? SelectKind::SketchPoint : SelectKind::SketchCurve, entity.sketch,
                 entity.id, entity.geometry};
    info.item.occurrence = entity.occurrence;
    markInactive(object, isInactiveItem(info.item)); // pale outside the active component (mitcad#420)
    styleSketchEntity(object, look);
    m_sketchEntityLooks[object.get()] = look;
    if (isPoint) {
      // Its hover in the points' layer too (mitcad#897).
      object->SetDynamicHilightAttributes(m_sketchPointHover);
    }
    display(object, AIS_WireFrame, std::move(info), true);
    // Over the faces a sketch lies on; points after the curves and their
    // highlights (mitcad#897).
    m_context->SetZLayer(object, isPoint ? m_sketchPointLayer : Graphic3d_ZLayerId_Top);
    if (isPoint) {
      m_context->SetDisplayPriority(object, kSketchPointPriority);
    }
    if (hasSketchHalo(look)) {
      // In a layer of its own just before the top layer: at the same depth
      // as the entity, whose line passes OCCT's depth test (less or equal)
      // over it.
      occ::handle<AIS_Shape> halo = new AIS_Shape(entity.shape);
      styleSketchHalo(halo, look);
      display(halo, AIS_WireFrame, PickInfo(), false);
      m_context->SetZLayer(halo, m_sketchHaloLayer);
      m_sketchEntityHalos[object.get()] = halo;
    }
    m_sketchEntities.push_back(object);
    // A key shown twice (not expected) keeps the first in the map; the
    // other is taken away with the rest below.
    if (!m_sketchEntityKeys.emplace(key, std::make_pair(entity.signature, object)).second) {
      shown.emplace(key + QStringLiteral("#%1").arg(made), std::make_pair(QString(), object));
      m_sketchEntities.pop_back();
    }
  }
  for (const auto& [key, entry] : shown) {
    m_sketchEntityLooks.erase(entry.second.get());
    const auto halo = m_sketchEntityHalos.find(entry.second.get());
    if (halo != m_sketchEntityHalos.end()) {
      m_context->Remove(halo->second, false);
      m_sketchEntityHalos.erase(halo);
    }
    remove(entry.second);
  }
  logSketchHalos();
  logSketchLayers();
  // For UI tests (mitcad#286): per shown sketch (and the occurrence placing
  // it), how many of its entities take the fully constrained colour and how
  // many the unconstrained blue; construction, centre lines and projected
  // geometry have colours of their own and are not counted.
  std::vector<std::pair<QString, std::pair<int, int>>> colours;
  for (const SketchEntityDisplay& entity : entities) {
    const QString shownAs = entity.occurrence.isEmpty()
                                ? entity.sketch
                                : QStringLiteral("%1 in %2").arg(entity.sketch, entity.occurrence);
    auto counts = std::find_if(colours.begin(), colours.end(),
                               [&shownAs](const auto& entry) { return entry.first == shownAs; });
    if (counts == colours.end()) {
      colours.emplace_back(shownAs, std::make_pair(0, 0));
      counts = std::prev(colours.end());
    }
    if (entity.shape.IsNull() || entity.reference || entity.construction || entity.centerline) {
      continue;
    }
    ++(entity.constrained ? counts->second.first : counts->second.second);
  }
  QStringList logged;
  for (const auto& [shownAs, counts] : colours) {
    logged << QStringLiteral("%1 %2 fully constrained, %3 unconstrained")
                  .arg(shownAs)
                  .arg(counts.first)
                  .arg(counts.second);
  }
  const QString line = logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral("; "));
  if (line != m_loggedSketchColours) {
    m_loggedSketchColours = line;
    qDebug().noquote() << QStringLiteral("Sketch colours: %1").arg(line);
  }
  timing.setDetail(QStringLiteral("%1 entities, %2 new").arg(entities.size()).arg(made));
  sceneChanged();
}

void OcctViewer::styleSketchEntity(const occ::handle<AIS_Shape>& object, const SketchEntityLook& look) const {
  if (look.point) {
    // A light fill with an outline that tells the point's state, apart
    // from the selection's blue on either background (mitcad#946); faded as
    // the curves are.
    const sketch::PointState state = look.reference      ? sketch::PointState::Reference
                                     : look.construction ? sketch::PointState::Construction
                                     : look.constrained  ? sketch::PointState::Constrained
                                                         : sketch::PointState::Free;
    const sketch::PointMarkerLook plain = sketch::pointMarkerLook(state, m_backgroundDark);
    const sketch::PointMarkerLook marker = drawnInactive(object) ? sketch::fadedLook(plain, m_paper, kInactiveFade)
                                           : look.temporary ? sketch::fadedLook(plain, m_paper, kTemporaryFade)
                                                            : plain;
    const double ratio = devicePixelRatioF();
    setPointImage(object, sketchMarkerImage(sketch::pointMarkerKey(marker, ratio),
                                            sketch::pointMarkerImage(marker, ratio)));
    return;
  }
  const Quantity_Color& own = look.reference      ? kReferenceColor
                              : look.construction ? kConstructionColor
                              : look.centerline   ? (m_backgroundDark ? kCenterlineColorDark : kCenterlineColor)
                              : look.constrained  ? (m_backgroundDark ? kConstrainedColorDark : kConstrainedColor)
                                                  : kSketchColor;
  // Pale outside the active component (mitcad#420); a little lighter when
  // shown only for a command (mitcad#546).
  const Quantity_Color color = drawnInactive(object) ? inactiveColor(own, m_paper)
                               : look.temporary      ? inactiveColor(own, m_paper, kTemporaryFade)
                                                     : own;
  setEdgeStyle(object, color, kSketchLineWidth,
               look.construction ? Aspect_TOL_DASH
               : look.centerline ? Aspect_TOL_DOTDASH
                                 : Aspect_TOL_SOLID);
}

bool OcctViewer::hasSketchHalo(const SketchEntityLook& look) {
  return !look.point && !look.reference && !look.construction && (look.centerline || look.constrained);
}

void OcctViewer::styleSketchHalo(const occ::handle<AIS_Shape>& halo, const SketchEntityLook& look) const {
  // The line's own pattern, so that a centre line's gaps stay gaps.
  setEdgeStyle(halo, m_backgroundDark ? kSketchHaloDark : kSketchHaloLight, kSketchHaloWidth,
               look.centerline ? Aspect_TOL_DOTDASH : Aspect_TOL_SOLID);
}

QColor OcctViewer::sketchHaloColor() const {
  double r = 0.0;
  double g = 0.0;
  double b = 0.0;
  (m_backgroundDark ? kSketchHaloDark : kSketchHaloLight).Values(r, g, b, Quantity_TOC_sRGB);
  return QColor::fromRgbF(static_cast<float>(r), static_cast<float>(g), static_cast<float>(b));
}

void OcctViewer::logSketchHalos() {
  const QString line =
      QStringLiteral("%1 %2")
          .arg(m_sketchEntityHalos.size())
          .arg(m_backgroundDark ? QStringLiteral("dark") : QStringLiteral("light"));
  if (line != m_loggedSketchHalos) {
    m_loggedSketchHalos = line;
    qDebug().noquote() << QStringLiteral("Sketch halos: %1").arg(line);
  }
}

void OcctViewer::logSketchLayers() {
  // The order OCCT draws the sketch layers in, read from the viewer: the
  // layers that are not immediate first, in their order in the list, then
  // the immediate ones (the top layer and the points') over them in theirs.
  NCollection_Sequence<int> layers;
  m_viewer->GetAllZLayers(layers);
  QStringList order;
  for (const bool immediate : {false, true}) {
    for (NCollection_Sequence<int>::Iterator it(layers); it.More(); it.Next()) {
      const Graphic3d_ZLayerId layer = it.Value();
      if (m_viewer->ZLayerSettings(layer).IsImmediate() != immediate) {
        continue;
      }
      if (layer == m_sketchHaloLayer) {
        order << QStringLiteral("halos");
      } else if (layer == Graphic3d_ZLayerId_Top) {
        order << QStringLiteral("curves and highlights");
      } else if (layer == m_sketchPointLayer) {
        order << QStringLiteral("points and point highlights");
      }
    }
  }
  // Whether the model's depth still hides the points behind it: the
  // points' layer tests depth and keeps the depth drawn before it.
  const Graphic3d_ZLayerSettings& points = m_viewer->ZLayerSettings(m_sketchPointLayer);
  const bool sceneDepth = points.ToEnableDepthTest() && !points.ToClearDepth();
  const QString line =
      QStringLiteral("%1; points %2 the scene's depth; point highlights %3 the markers")
          .arg(order.join(QStringLiteral(" < ")),
               sceneDepth ? QStringLiteral("keep") : QStringLiteral("lose"),
               kSketchPointHighlightPriority > kSketchPointPriority ? QStringLiteral("over")
                                                                    : QStringLiteral("under"));
  if (line != m_loggedSketchLayers) {
    m_loggedSketchLayers = line;
    qDebug().noquote() << QStringLiteral("Sketch layers: %1").arg(line);
  }
}

void OcctViewer::restyleSketchEntities() {
  // Only fully constrained geometry and centre lines change colour, and
  // every point's marker (mitcad#946: its outline is light on a dark
  // background), which the log does not count apart.
  int constrained = 0;
  int centerlines = 0;
  for (const occ::handle<AIS_Shape>& object : m_sketchEntities) {
    const auto look = m_sketchEntityLooks.find(object.get());
    if (look == m_sketchEntityLooks.end()) {
      continue;
    }
    const bool recoloured = !look->second.reference && !look->second.construction &&
                            (look->second.centerline || look->second.constrained);
    if (!recoloured && !look->second.point) {
      continue;
    }
    if (recoloured) {
      ++(look->second.centerline ? centerlines : constrained);
    }
    styleSketchEntity(object, look->second);
    m_context->Redisplay(object, false);
    const auto halo = m_sketchEntityHalos.find(object.get());
    if (halo != m_sketchEntityHalos.end()) {
      styleSketchHalo(halo->second, look->second);
      m_context->Redisplay(halo->second, false);
    }
  }
  logSketchHalos();
  if (!m_sketchEntities.empty()) {
    qDebug().noquote() << QStringLiteral("Sketch entities recoloured for a %1 background: %2 fully constrained, "
                                         "%3 centre lines")
                              .arg(m_backgroundDark ? QStringLiteral("dark") : QStringLiteral("light"))
                              .arg(constrained)
                              .arg(centerlines);
  }
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
    PickInfo info;
    info.item = {datum.kind, datum.uid, QString(), SelectionItem::kindName(datum.kind)};
    info.item.occurrence = datum.occurrence;
    markInactive(object, isInactiveItem(info.item)); // pale outside the active component (mitcad#420)
    const int mode = styleDatum(object, datum.kind);
    display(object, mode, std::move(info), true);
    m_datums.push_back(object);
  }
  sceneChanged();
}

int OcctViewer::styleDatum(const occ::handle<AIS_Shape>& object, SelectKind kind) const {
  const bool inactive = drawnInactive(object);
  const auto look = [this, inactive](const Quantity_Color& color) {
    return inactive ? inactiveColor(color, m_paper) : color;
  };
  switch (kind) {
  case SelectKind::Plane: {
    object->SetColor(look(kPlaneColor));
    object->SetTransparency(inactive ? kInactivePlaneTransparency : 0.75);
    const occ::handle<Prs3d_Drawer>& drawer = object->Attributes();
    drawer->SetFaceBoundaryDraw(true);
    drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(look(kAxisColor), Aspect_TOL_SOLID, 1.0));
    return AIS_Shaded;
  }
  case SelectKind::Axis:
    setEdgeStyle(object, look(kAxisColor), 2.0, Aspect_TOL_DOTDASH);
    return AIS_WireFrame;
  default:
    setPointStyle(object, look(kAxisColor), Aspect_TOM_BALL, 1.5);
    return AIS_WireFrame;
  }
}

void OcctViewer::styleProfile(const occ::handle<AIS_Shape>& profile) const {
  const bool inactive = drawnInactive(profile);
  profile->SetTransparency(inactive ? kInactiveProfileTransparency : 0.6);
  profile->Attributes()->SetFaceBoundaryAspect(new Prs3d_LineAspect(
      inactive ? inactiveColor(kSketchColor, m_paper) : kSketchColor, Aspect_TOL_SOLID, 2.0));
}

void OcctViewer::setHighlights(const std::vector<HighlightDisplay>& highlights) {
  for (const occ::handle<AIS_Shape>& highlight : m_highlights) {
    remove(highlight);
  }
  m_highlights.clear();
  if (&highlights != &m_shownHighlights) {
    m_shownHighlights = highlights;
  }
  QStringList bodies; // highlighted whole, for the log
  QStringList profiles; // "F1/r{...} 2 faces", for the log
  // Where faces, edges, vertices and datums are highlighted, for UI tests
  // ("face F2.b0/F2:side(c1) in O1 at 60.0,0.0,10.0": the middle of its
  // bounding box in the design, mitcad#432).
  const bool logPlaces = qEnvironmentVariableIsSet("MITCAD_LOG_PICKS");
  QStringList places;
  // The sketch curves and points highlighted, for the log (mitcad#897).
  int sketchCurveHighlights = 0;
  int sketchPointHighlights = 0;
  for (const HighlightDisplay& highlight : highlights) {
    if (highlight.shape.IsNull()) {
      continue;
    }
    const SelectKind kind = highlight.item.kind;
    if (logPlaces && (kind == SelectKind::Face || kind == SelectKind::Edge || kind == SelectKind::Vertex ||
                      kind == SelectKind::Plane || kind == SelectKind::Axis || kind == SelectKind::Point)) {
      Bnd_Box box;
      BRepBndLib::Add(highlight.shape, box);
      if (!box.IsVoid()) {
        const gp_Pnt middle((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0);
        const SelectionItem& item = highlight.item;
        places << QStringLiteral("%1 %2%3%4 at %5,%6,%7")
                      .arg(SelectionItem::kindName(kind), item.owner,
                           item.name.isEmpty() ? QString() : QStringLiteral("/") + item.name,
                           item.occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + item.occurrence)
                      .arg(middle.X(), 0, 'f', 1)
                      .arg(middle.Y(), 0, 'f', 1)
                      .arg(middle.Z(), 0, 'f', 1);
      }
    }
    if (highlight.item.kind == SelectKind::Body) {
      bodies << (highlight.item.occurrence.isEmpty()
                     ? highlight.item.owner
                     : QStringLiteral("%1 in %2").arg(highlight.item.owner, highlight.item.occurrence));
    } else if (highlight.item.kind == SelectKind::Profile) {
      int faces = 0;
      for (TopExp_Explorer face(highlight.shape, TopAbs_FACE); face.More(); face.Next()) {
        ++faces;
      }
      profiles << QStringLiteral("%1/%2 %3 face%4")
                      .arg(highlight.item.owner, highlight.item.name)
                      .arg(faces)
                      .arg(faces == 1 ? QString() : QStringLiteral("s"));
    }
    // Named body sub-shapes are selected in local coordinates, as on the
    // body itself. The overlay's own faces, edges and vertices remain
    // available when its original body disappears behind a preview.
    const bool boundaryPicks = highlight.body &&
        (kind == SelectKind::Face || kind == SelectKind::Edge);
    occ::handle<AIS_Shape> object = new AIS_Shape(boundaryPicks
        ? highlight.shape.Located(TopLoc_Location()) : highlight.shape);
    if (boundaryPicks && !highlight.shape.Location().IsIdentity()) {
      object->SetLocalTransformation(highlight.shape.Location().Transformation());
    }
    const Quantity_Color color = occtColor(highlight.color);
    int mode = AIS_WireFrame;
    // A feature's inputs where they were (mitcad#957): lines, as the
    // selected edges are drawn.
    const SelectKind drawnAs = highlight.inputLines ? SelectKind::Edge : highlight.item.kind;
    switch (drawnAs) {
    case SelectKind::Face:
    case SelectKind::Feature: // the faces it made (mitcad#28)
    case SelectKind::Profile:
    case SelectKind::Plane:
    case SelectKind::Body:
    case SelectKind::Component: {
      object->SetColor(color);
      // In a colour light does not change, as the hover's
      // (setHighlightStyle): lit, a light colour washed out to white.
      object->SetMaterial(flatMaterial(0.0f));
      const bool whole =
          highlight.item.kind == SelectKind::Body || highlight.item.kind == SelectKind::Component;
      object->SetTransparency(whole && highlight.opaqueWholeBody ? 0.0f
                              : highlight.light ? kLightTransparency
                              : whole         ? kWholeTransparency
                              : highlight.item.kind == SelectKind::Plane ? kPickedPlaneTransparency
                                                                         : kSelectedTransparency);
      // Drawn just in front of the face it covers.
      object->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, -1.0f,
                                                                         -4.0f);
      const occ::handle<Prs3d_Drawer>& drawer = object->Attributes();
      drawer->SetFaceBoundaryDraw(!highlight.light && (!whole || highlight.wholeBodyBoundaries));
      drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(color, Aspect_TOL_SOLID, 2.0));
      mode = AIS_Shaded;
      break;
    }
    case SelectKind::SketchPoint: {
      // A filled ball in the highlight's colour with a thin light outline,
      // apart from the plain marker and a curve's highlight (mitcad#946).
      const double ratio = devicePixelRatioF();
      setPointImage(object, sketchMarkerImage(sketch::pointBallKey(highlight.color, ratio),
                                              sketch::pointBallImage(highlight.color, ratio)));
      break;
    }
    case SelectKind::Vertex:
    case SelectKind::Point:
      setPointStyle(object, color, Aspect_TOM_BALL, 1.5);
      break;
    default:
      if (highlight.inputLines) {
        // Lighter on a dark background, where the plain selection blue
        // has little contrast (mitcad#957).
        setEdgeStyle(object, m_backgroundDark ? occtColor(kInputLineColorDark) : color, kInputLineWidth);
      } else {
        setEdgeStyle(object, color, 3.5);
      }
      break;
    }
    if (highlight.inputLines && m_backgroundDark) {
      // The dark halo of a sketch's lines under the part in sight, against
      // the scene's depth: the lines stand apart from the background and
      // from the amber faces of their feature.
      occ::handle<AIS_Shape> halo = new AIS_Shape(highlight.shape);
      setEdgeStyle(halo, kSketchHaloDark, kSketchHaloWidth);
      PickInfo haloInfo;
      haloInfo.item = highlight.item;
      haloInfo.highlight = true;
      display(halo, AIS_WireFrame, std::move(haloInfo), false);
      m_context->SetZLayer(halo, m_sketchHaloLayer);
      m_highlights.push_back(halo);
    }
    if (highlight.throughSolid) {
      // Topmost disables depth testing. A subdued dashed boundary locates
      // enclosed and rear faces; the normal shaded overlay is left intact.
      // A feature's input lines (mitcad#957) instead go to a layer of their
      // own after the top one, in the selection colour: the line in sight,
      // drawn after it, covers its copy, so only the hidden parts show
      // dashed, over the model's own edges.
      occ::handle<AIS_Shape> hidden = new AIS_Shape(highlight.shape);
      if (highlight.inputLines) {
        setEdgeStyle(hidden, color, kInputLineHiddenWidth, Aspect_TOL_DASH);
      } else {
        setEdgeStyle(hidden, occtColor(highlight.color.darker(135)), 1.5, Aspect_TOL_DASH);
      }
      // Boundaries only: no lines across the faces (OCCT's wireframe draws
      // one U and one V iso line on each face by default, grey, which was
      // drawn over the input lines in the same screen column).
      const occ::handle<Prs3d_Drawer>& hiddenDrawer = hidden->Attributes();
      hiddenDrawer->SetUIsoAspect(new Prs3d_IsoAspect(occtColor(highlight.color), Aspect_TOL_DASH, 1.0, 0));
      hiddenDrawer->SetVIsoAspect(new Prs3d_IsoAspect(occtColor(highlight.color), Aspect_TOL_DASH, 1.0, 0));
      PickInfo hiddenInfo;
      hiddenInfo.item = highlight.item;
      hiddenInfo.highlight = true;
      display(hidden, AIS_WireFrame, std::move(hiddenInfo), false);
      m_context->SetZLayer(hidden, highlight.inputLines ? m_inputLineLayer : Graphic3d_ZLayerId_Topmost);
      m_highlights.push_back(hidden);
    }
    PickInfo info;
    info.item = highlight.item;
    info.highlight = true;
    info.body = boundaryPicks ? highlight.body : nullptr;
    display(object, mode, std::move(info), highlight.pickable);
    if (kind == SelectKind::SketchPoint) {
      // With the plain markers, after the curves' highlights, and drawn
      // after them in that layer, so that the ball covers its point's
      // marker (mitcad#897).
      m_context->SetZLayer(object, m_sketchPointLayer);
      m_context->SetDisplayPriority(object, kSketchPointHighlightPriority);
      ++sketchPointHighlights;
    } else {
      // A light tint stays with the bodies, under the highlights on top;
      // a feature's input line in sight over its dashed copy (mitcad#957).
      m_context->SetZLayer(object, highlight.light        ? Graphic3d_ZLayerId_Default
                                   : highlight.inputLines ? m_inputLineSightLayer
                                                          : Graphic3d_ZLayerId_Top);
      if (kind == SelectKind::SketchCurve) {
        ++sketchCurveHighlights;
      }
    }
    m_highlights.push_back(object);
  }
  const QString loggedSketch = QStringLiteral("%1 curve%2, %3 point%4")
                                   .arg(sketchCurveHighlights)
                                   .arg(sketchCurveHighlights == 1 ? QString() : QStringLiteral("s"))
                                   .arg(sketchPointHighlights)
                                   .arg(sketchPointHighlights == 1 ? QString() : QStringLiteral("s"));
  if (loggedSketch != m_loggedSketchHighlights) {
    m_loggedSketchHighlights = loggedSketch;
    qDebug().noquote() << QStringLiteral("Sketch highlights: %1").arg(loggedSketch);
  }
  const QString logged = bodies.isEmpty() ? QStringLiteral("none") : bodies.join(QStringLiteral(", "));
  if (logged != m_loggedBodyHighlights) {
    m_loggedBodyHighlights = logged;
    qDebug().noquote() << QStringLiteral("View body highlights: %1").arg(logged);
  }
  const QString loggedProfiles = profiles.isEmpty() ? QStringLiteral("none") : profiles.join(QStringLiteral("; "));
  if (loggedProfiles != m_loggedProfileHighlights) {
    m_loggedProfileHighlights = loggedProfiles;
    qDebug().noquote() << QStringLiteral("View profile highlights: %1").arg(loggedProfiles);
  }
  const QString loggedPlaces = places.isEmpty() ? QStringLiteral("none") : places.join(QStringLiteral("; "));
  if (logPlaces && loggedPlaces != m_loggedHighlightPlaces) {
    m_loggedHighlightPlaces = loggedPlaces;
    qDebug().noquote() << QStringLiteral("View highlight places: %1").arg(loggedPlaces);
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
    // A tool the preview's job meshed (DisplayMeshes, a stand-in in the
    // interactive tier, mitcad#635) is drawn from those triangulations
    // only: never meshed again here, on the UI thread, also where a face
    // failed (mitcad#589).
    if (hasAnyTriangulation(shape)) {
      m_preview->Attributes()->SetAutoTriangulation(false);
    }
    m_previewMode = style == PreviewStyle::Failed ? AIS_WireFrame : AIS_Shaded;
    if (style == PreviewStyle::Failed) {
      setEdgeStyle(m_preview, kCutPreviewColor, 2.0, Aspect_TOL_DASH);
      m_context->Display(m_preview, AIS_WireFrame, -1, false);
    } else if (style == PreviewStyle::Datum) {
      // Stronger than the planes picked as its references, which are drawn
      // light (mitcad#446), and in front of them.
      setEdgeStyle(m_preview, kPreviewColor, 2.5);
      m_preview->SetMaterial(flatMaterial(0.0f));
      m_preview->SetTransparency(kDatumPreviewTransparency);
      const occ::handle<Prs3d_Drawer>& drawer = m_preview->Attributes();
      drawer->SetFaceBoundaryDraw(true);
      drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(kPreviewColor, Aspect_TOL_SOLID, 2.5));
      m_context->Display(m_preview, AIS_Shaded, -1, false);
      m_context->SetZLayer(m_preview, Graphic3d_ZLayerId_Top);
      // For UI tests, once.
      static bool logged = false;
      if (!logged) {
        logged = true;
        qDebug().noquote() << QStringLiteral("Preview datum drawn: transparency %1, picked planes %2")
                                  .arg(kDatumPreviewTransparency)
                                  .arg(kPickedPlaneTransparency);
      }
    } else if (style == PreviewStyle::ExtendedTool) {
      // Faces through the bodies, light enough to see the cut and the
      // bodies behind it; their outline marks where the tool ends.
      setEdgeStyle(m_preview, kExtendedToolColor, 1.5);
      m_preview->SetMaterial(flatMaterial(0.0f));
      m_preview->SetTransparency(kExtendedToolTransparency);
      const occ::handle<Prs3d_Drawer>& drawer = m_preview->Attributes();
      drawer->SetFaceBoundaryDraw(true);
      drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(kExtendedToolColor, Aspect_TOL_SOLID, 1.5));
      // The cut faces of the pieces lie on the tool: pull it towards the
      // viewer so that it wins the depth test there.
      drawer->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, -1.0f, -2.0f);
      m_context->Display(m_preview, AIS_Shaded, -1, false);
    } else {
      m_preview->SetColor(style == PreviewStyle::Remove ? kCutPreviewColor : kPreviewColor);
      m_preview->SetTransparency(0.5);
      // A Join/Cut tool shares faces with the previewed body (the walls of a
      // cut hole); pull it towards the viewer so it wins the depth test there.
      m_preview->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, -1.0f,
                                                                            -2.0f);
      m_context->Display(m_preview, AIS_Shaded, -1, false);
    }
    if (!m_provisional.IsNull()) {
      // A provisional drag presentation stands in for it (mitcad#636).
      m_context->Erase(m_preview, false);
    }
  }
  sceneChanged();
}

void OcctViewer::logFirstFrame(const QString& what, const QElapsedTimer& started) {
  m_firstFrameOf = what;
  m_firstFrameClock = started;
  update();
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
  m_expandedFilter = filter;
  m_scoped = false;
  m_pickTest = nullptr;
  m_clickPickedNothing = false; // a click before the filter refused nothing of it
  if (test) {
    m_pickTest = [test = std::move(test)](const SelectionItem& item, const PickCase&) { return test(item); };
  }
  m_context->ClearSelected(false);
  m_context->ClearDetected(false);
  activateAllPicking();
  sceneChanged();
}

void OcctViewer::setPickScope(SelectFilter kinds, SelectFilter expanded, PickTest test) {
  m_filter = kinds;
  m_expandedFilter = kinds | expanded;
  m_scoped = true;
  m_pickTest = std::move(test);
  m_clickPickedNothing = false; // a click before the scope refused nothing of it
  // A scope set while Shift is down (a key that came before) picks as Shift
  // says now.
  m_shiftHeld = QGuiApplication::keyboardModifiers().testFlag(Qt::ShiftModifier);
  m_context->ClearSelected(false);
  m_context->ClearDetected(false);
  ResetPreviousMoveTo();
  activateAllPicking();
  sceneChanged();
}

void OcctViewer::setPickingSuspended(bool suspended) {
  if (m_pickingSuspended == suspended) {
    return;
  }
  m_pickingSuspended = suspended;
  m_context->ClearDetected(false);
  ResetPreviousMoveTo();
  requestFrame();
}

SelectFilter OcctViewer::activeKinds() const {
  return m_scoped && (m_shiftHeld || m_contextPick) ? m_expandedFilter : m_filter;
}

void OcctViewer::setShiftHeld(bool held, bool hover) {
  if (held == m_shiftHeld) {
    return;
  }
  m_shiftHeld = held;
  if (!m_scoped) {
    return;
  }
  if (m_expandedFilter != m_filter) {
    activateAllPicking();
  }
  // What lies under the pointer is found again with the scope as Shift
  // makes it, also for a click at the same place.
  m_context->ClearDetected(false);
  ResetPreviousMoveTo();
  emit shiftChanged(held);
  if (hover && underMouse() && PressedMouseButtons() == Aspect_VKeyMouse_NONE && !m_view->Window().IsNull()) {
    UpdateMousePosition(LastMousePosition(), Aspect_VKeyMouse_NONE,
                        viewFlags(QGuiApplication::keyboardModifiers()), false);
  }
  requestFrame();
}

bool OcctViewer::eventFilter(QObject* watched, QEvent* event) {
  const QEvent::Type type = event->type();
  if (type == QEvent::Paint && m_navigationState.active() && watched != this && watched->isWidgetType() &&
      isAncestorOf(static_cast<QWidget*>(watched))) {
    // A widget over the view painted while it is navigated (mitcad#768).
    ++m_navigationRepaints;
    const QString name = watched->objectName().isEmpty()
                             ? QString::fromLatin1(watched->metaObject()->className())
                             : watched->objectName();
    if (!m_navigationRepainted.contains(name)) {
      m_navigationRepainted << name;
    }
  }
  if (m_scoped && (type == QEvent::KeyPress || type == QEvent::KeyRelease)) {
    const auto* key = static_cast<const QKeyEvent*>(event);
    if (key->key() == Qt::Key_Shift && !key->isAutoRepeat() && window() != nullptr &&
        window()->isActiveWindow()) {
      setShiftHeld(type == QEvent::KeyPress, true);
    }
  } else if (m_scoped && type == QEvent::ApplicationDeactivate) {
    setShiftHeld(false, false); // the release goes to another application
  }
  return QOpenGLWidget::eventFilter(watched, event);
}

void OcctViewer::setHoverColor(const QColor& color) {
  // Whole objects (Dynamic) and faces, edges and vertices (LocalDynamic).
  for (const Prs3d_TypeOfHighlight type :
       {Prs3d_TypeOfHighlight_Dynamic, Prs3d_TypeOfHighlight_LocalDynamic}) {
    setHighlightStyle(m_context->HighlightStyle(type), occtColor(color), kHoverTransparency);
  }
  if (!m_sketchPointHover.IsNull()) {
    setHighlightStyle(m_sketchPointHover, occtColor(color), kHoverTransparency);
    m_sketchPointHover->SetZLayer(m_sketchPointLayer);
  }
  m_hoverColor = color;
  sceneChanged();
}

void OcctViewer::setHoverChain(const TopoDS_Shape& shape) {
  if (!m_hoverChain.IsNull()) {
    m_context->Remove(m_hoverChain, false);
    m_hoverChain.Nullify();
  }
  if (!shape.IsNull()) {
    // As the hover draws a face, edge (setHighlightStyle): faces
    // see-through in a colour light does not change, with their boundaries,
    // and edges opaque, in the top layer, which keeps the scene's depth.
    m_hoverChain = new AIS_Shape(shape);
    const Quantity_Color color = occtColor(m_hoverColor);
    bool faces = false;
    for (TopExp_Explorer it(shape, TopAbs_FACE); it.More() && !faces; it.Next()) {
      faces = true;
    }
    setEdgeStyle(m_hoverChain, color, 3.0);
    if (faces) {
      m_hoverChain->SetMaterial(flatMaterial(0.0f));
      m_hoverChain->SetTransparency(kHoverTransparency);
      m_hoverChain->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, 0.0f, -2.0f);
      m_hoverChain->Attributes()->SetFaceBoundaryDraw(true);
      m_hoverChain->Attributes()->SetFaceBoundaryAspect(new Prs3d_LineAspect(color, Aspect_TOL_SOLID, 2.0));
    }
    m_context->Display(m_hoverChain, faces ? AIS_Shaded : AIS_WireFrame, -1, false);
    m_context->SetZLayer(m_hoverChain, Graphic3d_ZLayerId_Top);
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
  // Candidate probes use the command's model-backed filter too. Respect
  // the same worker ownership boundary as hover and actual clicks.
  if (m_pickingSuspended || m_view->Window().IsNull()) {
    return found;
  }
  // Each owner's test once while the points are tried (accepts()); a
  // nested call keeps the outer one's.
  std::map<const SelectMgr_EntityOwner*, std::pair<OwnerTest, SelectKind>> ownerTests;
  const bool outer = m_ownerTests == nullptr;
  if (outer) {
    m_ownerTests = &ownerTests;
  }
  struct OwnerTestsEnd {
    OcctViewer* viewer;
    bool outer;
    ~OwnerTestsEnd() {
      if (outer) {
        viewer->m_ownerTests = nullptr;
      }
    }
  } ownerTestsEnd{this, outer};
  // A point well inside a face: the centre of the circle inscribed in its
  // fattest triangle (the largest such circle), as far from that
  // triangle's sides as any point of it. The middle of the largest
  // triangle can lie at an edge of the face where the triangulation has
  // long thin triangles (planes through Delabella, mitcad#644), where a
  // click a pixel off picks the face beside it.
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
      // Each corner weighted by the length of the side across it.
      const double acrossA = pb.Distance(pc);
      const double acrossB = pc.Distance(pa);
      const double acrossC = pa.Distance(pb);
      const double perimeter = acrossA + acrossB + acrossC;
      if (perimeter <= 0.0) {
        continue;
      }
      // Twice the area over the perimeter: the inscribed circle's radius.
      const double inscribed = gp_Vec(pa, pb).Crossed(gp_Vec(pa, pc)).Magnitude() / perimeter;
      if (inscribed > largest) {
        largest = inscribed;
        middle = gp_Pnt((acrossA * pa.XYZ() + acrossB * pb.XYZ() + acrossC * pc.XYZ()) / perimeter);
      }
    }
    if (largest <= 0.0) {
      return std::nullopt;
    }
    return middle.Transformed(location.Transformation());
  };
  // The middles of `count` triangles spread over a face's triangulation.
  const auto middlesOf = [](const TopoDS_Face& face, int count) {
    std::vector<gp_Pnt> middles;
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
    if (mesh.IsNull() || mesh->NbTriangles() == 0 || count <= 0) {
      return middles;
    }
    const int triangles = mesh->NbTriangles();
    for (int k = 0; k < std::min(count, triangles); ++k) {
      const int t = 1 + static_cast<int>(static_cast<long long>(k) * triangles / std::min(count, triangles));
      int a = 0;
      int b = 0;
      int c = 0;
      mesh->Triangle(t).Get(a, b, c);
      const gp_Pnt middle((mesh->Node(a).XYZ() + mesh->Node(b).XYZ() + mesh->Node(c).XYZ()) / 3.0);
      middles.push_back(middle.Transformed(location.Transformation()));
    }
    return middles;
  };
  // The point at the middle of a face's parameter range, when it is on the
  // face.
  const auto parameterMiddleOf = [](const TopoDS_Face& face) -> std::optional<gp_Pnt> {
    TopLoc_Location location;
    if (BRep_Tool::Surface(face, location).IsNull()) {
      return std::nullopt; // a mesh body's face has no parameters
    }
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
  struct PickBody {
    std::shared_ptr<geometry::Shape> shape;
    gp_Trsf placement;
    std::optional<SelectionItem> body; // its owner and occurrence, as a pick names them
  };
  const auto bodyItem = [this](const AIS_InteractiveObject* object) -> std::optional<SelectionItem> {
    const auto info = m_pickInfo.find(object);
    return info == m_pickInfo.end() ? std::nullopt : std::optional<SelectionItem>(info->second.item);
  };
  std::vector<PickBody> sources;
  for (const auto& [key, entry] : m_bodies) {
    (void)key;
    if (entry.style == BodyDisplay::Style::Normal && entry.shape) {
      sources.push_back({entry.shape, entry.placement.Transformation(), bodyItem(entry.object.get())});
    }
  }
  // Also log points on selected source topology after a preview replaced
  // the body. The actual selector below still decides which are pickable.
  for (const auto& highlight : m_highlights) {
    const auto info = m_pickInfo.find(highlight.get());
    if (info == m_pickInfo.end() || !info->second.body) { continue; }
    const gp_Trsf placement = highlight->Transformation();
    if (std::none_of(sources.begin(), sources.end(), [&](const PickBody& source) {
          return source.shape == info->second.body && sameTransformation(source.placement, placement);
        })) {
      sources.push_back({info->second.body, placement, info->second.item});
    }
  }
  // Whether the filter and the input's test take an item at all, wherever
  // it is clicked (accepts() without the pick's own conditions): an item
  // they refuse is not tried at its points (mitcad#697).
  const auto mayTake = [this](const SelectionItem& item) {
    if (!activeKinds().testFlag(item.kind)) {
      return false;
    }
    if (!m_pickTest || m_untestedPicks) {
      return true;
    }
    PickCase how;
    how.expanded = m_scoped && (m_shiftHeld || m_contextPick);
    if (item.kind != SelectKind::Edge) {
      return m_pickTest(item, how);
    }
    // An edge in sight or behind a face: either may be taken.
    for (const EdgeSight sight : {EdgeSight::Visible, EdgeSight::Occluded}) {
      how.sight = [sight] { return sight; };
      if (m_pickTest(item, how)) {
        return true;
      }
    }
    return false;
  };
  // What a click at each pixel tried picks, the item when accepted: the
  // points of a part small in the view (a toothed part's hundreds of faces,
  // edges and vertices, each face tried at up to 26 points) fall on the
  // same pixels again and again, and each try is a pick in the view.
  std::map<std::pair<int, int>, std::optional<SelectionItem>> picked;
  for (const PickBody& source : sources) {
    const geometry::Shape& body = *source.shape;
    const gp_Trsf& placement = source.placement;
    // Whether a click at the point picks the named item.
    const auto probe = [&](const gp_Pnt& local, SelectKind kind, const std::string& name) {
      // Probe the exact logical pixel that callers receive (the projection
      // rounded to the nearest pixel), including its device pixels at 2x.
      const QPoint at = toWidgetF(local.Transformed(placement)).toPoint();
      if (!inside.contains(at) || isCovered(at)) {
        return false;
      }
      auto there = picked.find({at.x(), at.y()});
      if (there == picked.end()) {
        const NCollection_Vec2<int> pixel = toDevicePixels(at);
        m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
        std::optional<SelectionItem> item = itemFor(m_context->DetectedOwner());
        if (item && !accepts(m_context->DetectedOwner())) {
          item.reset();
        }
        there = picked.emplace(std::make_pair(at.x(), at.y()), std::move(item)).first;
      }
      const std::optional<SelectionItem>& item = there->second;
      if (item && item->kind == kind && item->name.toStdString() == name) {
        found.emplace_back(*item, at);
        return true;
      }
      return false;
    };
    // Whether a click could take the body's named face, edge or vertex.
    const auto takes = [&source, &mayTake](SelectKind kind, const std::string& name, const QString& geometry) {
      if (!source.body) {
        return true; // tried at its points
      }
      SelectionItem item = *source.body;
      item.kind = kind;
      item.name = QString::fromStdString(name);
      item.geometry = geometry;
      return mayTake(item);
    };
    if (kinds.testFlag(SelectKind::Face)) {
      for (int i = 0; i < body.face_count(); ++i) {
        if (body.face_names(i).empty() ||
            !takes(SelectKind::Face, body.face_names(i).front(), surfaceType(body.face(i)))) {
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
        if (const auto middle = middleOf(face);
            middle && probe(*middle, SelectKind::Face, body.face_names(i).front())) {
          continue;
        }
        // Where those are hidden or covered (a curved face's far side, a
        // preview in front of it, mitcad#656): the middles of its next
        // largest triangles.
        for (const gp_Pnt& middle : middlesOf(face, 24)) {
          if (probe(middle, SelectKind::Face, body.face_names(i).front())) {
            break;
          }
        }
      }
    }
    if (kinds.testFlag(SelectKind::Edge)) {
      for (int i = 0; i < body.edge_count(); ++i) {
        if (!takes(SelectKind::Edge, body.edge_name(i), curveType(body.edge(i)))) {
          continue;
        }
        const BRepAdaptor_Curve curve(body.edge(i));
        // The middle first; where it is hidden or covered (a circle's far
        // side, under a card at the top of the view), its quarters
        // (mitcad#656: a shaft's top edge).
        for (const double at : {0.5, 0.75, 0.25}) {
          if (probe(curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * at),
                    SelectKind::Edge, body.edge_name(i))) {
            break;
          }
        }
      }
    }
    if (kinds.testFlag(SelectKind::Vertex)) {
      for (int i = 0; i < body.vertex_count(); ++i) {
        if (takes(SelectKind::Vertex, body.vertex_name(i), QStringLiteral("point"))) {
          probe(BRep_Tool::Pnt(body.vertex(i)), SelectKind::Vertex, body.vertex_name(i));
        }
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
  // MoveTo also runs from paintGL in a model job's nested event loop.
  // In particular, command predicates can query bodyShape() and must not
  // reach the document while the worker owns it.
  if (m_pickingSuspended) {
    return false;
  }
  // The owner's own test: kept while pickPoints() tries its points, unless
  // it asked which part of an edge the pick hit, which differs by point.
  const auto tested = [this, &owner]() -> std::pair<OwnerTest, SelectKind> {
    if (m_ownerTests != nullptr) {
      if (const auto known = m_ownerTests->find(owner.get()); known != m_ownerTests->end()) {
        return known->second;
      }
    }
    bool sightAsked = false;
    const auto test = [&]() -> std::pair<OwnerTest, SelectKind> {
      const auto item = itemFor(owner);
      if (!item || !activeKinds().testFlag(item->kind)) {
        return {OwnerTest::Refused, SelectKind::Body};
      }
      if (!m_pickTest || m_untestedPicks) {
        return {OwnerTest::Accepted, item->kind};
      }
      PickCase how;
      how.window = m_windowSelecting;
      how.expanded = m_scoped && (m_shiftHeld || m_contextPick) && !m_windowSelecting;
      if (item->kind == SelectKind::Edge) {
        how.sight = [this, &owner, &sightAsked] {
          sightAsked = true;
          return edgeSight(owner);
        };
      }
      return {m_pickTest(*item, how) ? OwnerTest::Tested : OwnerTest::Refused, item->kind};
    };
    const std::pair<OwnerTest, SelectKind> result = test();
    if (m_ownerTests != nullptr && !sightAsked) {
      m_ownerTests->emplace(owner.get(), result);
    }
    return result;
  };
  const auto [verdict, kind] = tested();
  if (verdict != OwnerTest::Tested) {
    return verdict == OwnerTest::Accepted;
  }
  const bool area = kind == SelectKind::Face || kind == SelectKind::Plane || kind == SelectKind::Body;
  if (m_scoped && !m_windowSelecting && area) {
    // A click within an edge's reach that the scope lets it take picks the
    // edge, also one behind the face or the plane (mitcad#217: occluded
    // edges offered, or reached with Shift, which also offers the sketch's
    // own plane in front of them); faces, planes and bodies are areas, an
    // edge a precise target.
    const occ::handle<StdSelect_ViewerSelector3d>& selector = m_context->MainSelector();
    for (int rank = 1; rank <= selector->NbPicked(); ++rank) {
      const occ::handle<SelectMgr_EntityOwner> other = selector->Picked(rank);
      const occ::handle<StdSelect_BRepOwner> brep = occ::handle<StdSelect_BRepOwner>::DownCast(other);
      if (brep.IsNull() || !brep->HasShape() || brep->Shape().ShapeType() != TopAbs_EDGE) {
        continue;
      }
      const std::optional<SelectionItem> edge = itemFor(other);
      if (edge && edge->kind == SelectKind::Edge && accepts(other)) {
        return false;
      }
    }
  }
  return true;
}

bool OcctViewer::occluded(const gp_Pnt& point) const {
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  // From the point towards the eye: along the view in an orthographic
  // view, to the eye in a perspective one.
  gp_Dir toEye = camera->Direction().Reversed();
  double reach = 1.0e9;
  if (!camera->IsOrthographic()) {
    const gp_Vec towards(point, camera->Eye());
    reach = towards.Magnitude();
    if (reach <= kOcclusionTolerance) {
      return false;
    }
    toEye = gp_Dir(towards);
  }
  const gp_Lin ray(point, toEye);
  for (const auto& [key, entry] : m_bodies) {
    // Bodies drawn see-through (previews, ghosts, inactive components'
    // ghosts) hide nothing; hidden bodies are not shown at all.
    if (entry.style != BodyDisplay::Style::Normal || entry.inactive || !entry.shape) {
      continue;
    }
    if (!entry.occluder) {
      entry.occluder = std::make_shared<Occluder>();
      entry.occluder->placed = entry.shape->occt().Moved(entry.placement);
      // From the shown body's mesh, with room for the faces between its
      // nodes.
      Bnd_Box& box = entry.occluder->box;
      BRepBndLib::Add(entry.occluder->placed, box, true);
      if (!box.IsVoid()) {
        box.Enlarge(kOcclusionTolerance + 0.01 * std::sqrt(box.SquareExtent()));
      }
    }
    Occluder& occluder = *entry.occluder;
    if (occluder.box.IsVoid() || occluder.box.IsOut(ray)) {
      continue;
    }
    if (!occluder.intersector) {
      occluder.intersector = std::make_unique<IntCurvesFace_ShapeIntersector>();
      occluder.intersector->Load(occluder.placed, Precision::Confusion());
    }
    occluder.intersector->PerformNearest(ray, kOcclusionTolerance, reach);
    if (occluder.intersector->IsDone() && occluder.intersector->NbPnt() > 0) {
      return true;
    }
  }
  return false;
}

EdgeSight OcctViewer::edgeSight(const occ::handle<SelectMgr_EntityOwner>& owner) const {
  const occ::handle<StdSelect_BRepOwner> brep = occ::handle<StdSelect_BRepOwner>::DownCast(owner);
  if (brep.IsNull() || !brep->HasShape()) {
    return EdgeSight::Visible;
  }
  // A body's edge, or a selection's highlight: a compound of the edges.
  std::vector<TopoDS_Edge> edges;
  for (TopExp_Explorer explorer(brep->Shape(), TopAbs_EDGE); explorer.More(); explorer.Next()) {
    edges.push_back(TopoDS::Edge(explorer.Current()));
  }
  if (edges.empty()) {
    return EdgeSight::Visible;
  }
  // A placed body's edges are in its component's coordinates.
  gp_Trsf toWorld;
  if (owner->HasSelectable() && owner->Selectable()->HasTransformation()) {
    toWorld = owner->Selectable()->Transformation();
  }
  if (m_windowSelecting) {
    bool visible = false;
    bool hidden = false;
    for (const TopoDS_Edge& edge : edges) {
      const EdgeSight sight = windowSight(edge, toWorld);
      visible = visible || sight != EdgeSight::Occluded;
      hidden = hidden || sight != EdgeSight::Visible;
    }
    return visible && hidden ? EdgeSight::Both : hidden ? EdgeSight::Occluded : EdgeSight::Visible;
  }
  // A click: the point of the edges nearest where it hit.
  const std::optional<gp_Pnt> picked = pickedWorldPoint(owner);
  std::optional<gp_Pnt> nearest;
  double nearestDistance = 0.0;
  for (const TopoDS_Edge& edge : edges) {
    const BRepAdaptor_Curve curve(edge);
    gp_Pnt onEdge = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
    if (picked) {
      const gp_Pnt local = picked->Transformed(toWorld.Inverted());
      double parameter = 0.0;
      ShapeAnalysis_Curve().Project(curve, local, Precision::Confusion(), onEdge, parameter);
    }
    const double distance = picked ? onEdge.Distance(picked->Transformed(toWorld.Inverted())) : 0.0;
    if (!nearest || distance < nearestDistance) {
      nearest = onEdge;
      nearestDistance = distance;
    }
  }
  return occluded(nearest->Transformed(toWorld)) ? EdgeSight::Occluded : EdgeSight::Visible;
}

EdgeSight OcctViewer::windowSight(const TopoDS_Edge& edge, const gp_Trsf& toWorld) const {
  // The part of the edge inside the window: its samples there.
  const BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  bool visible = false;
  bool hidden = false;
  for (int i = 1; i < kSightSamples && !(visible && hidden); ++i) {
    const gp_Pnt point =
        curve.Value(first + (last - first) * double(i) / double(kSightSamples)).Transformed(toWorld);
    if (!m_windowRect.contains(toWidgetF(point))) {
      continue;
    }
    (occluded(point) ? hidden : visible) = true;
  }
  if (!visible && !hidden) {
    // A short edge the window crosses between the samples: its middle.
    return occluded(curve.Value((first + last) / 2.0).Transformed(toWorld)) ? EdgeSight::Occluded
                                                                            : EdgeSight::Visible;
  }
  return visible && hidden ? EdgeSight::Both : hidden ? EdgeSight::Occluded : EdgeSight::Visible;
}

std::vector<OcctViewer::EdgePlace> OcctViewer::edgePlaces() {
  std::vector<EdgePlace> found;
  if (m_view->Window().IsNull()) {
    return found;
  }
  const QRect inside = rect().adjusted(4, 4, -4, -4);
  const occ::handle<StdSelect_ViewerSelector3d>& selector = m_context->MainSelector();
  constexpr int kSamples = 16;
  for (const auto& [key, entry] : m_bodies) {
    if (entry.style != BodyDisplay::Style::Normal || !entry.shape) {
      continue;
    }
    const geometry::Shape& body = *entry.shape;
    const gp_Trsf placement = entry.placement.Transformation();
    for (int i = 0; i < body.edge_count(); ++i) {
      const TopoDS_Edge& edge = body.edge(i);
      const BRepAdaptor_Curve curve(edge);
      const double first = curve.FirstParameter();
      const double last = curve.LastParameter();
      // Per part (in sight, behind faces): the place nearest the edge's
      // middle, and how far from it.
      std::array<std::optional<std::pair<QPoint, int>>, 2> best;
      for (int k = 1; k < kSamples; ++k) {
        const double parameter = first + (last - first) * double(k) / double(kSamples);
        const gp_Pnt world = curve.Value(parameter).Transformed(placement);
        const QPointF at = toWidgetF(world);
        if (!inside.contains(at.toPoint()) || isCovered(at)) {
          continue;
        }
        const std::size_t side = occluded(world) ? std::size_t{1} : std::size_t{0};
        const int distance = std::abs(k - kSamples / 2);
        if (best[side] && best[side]->second <= distance) {
          continue;
        }
        // The edge is what a click there hits first, filters aside, and no
        // other edge is within reach there.
        const NCollection_Vec2<int> pixel = toDevicePixels(at);
        selector->Pick(pixel.x(), pixel.y(), m_view);
        bool clean = selector->NbPicked() >= 1;
        for (int rank = 1; clean && rank <= selector->NbPicked(); ++rank) {
          const occ::handle<StdSelect_BRepOwner> hit =
              occ::handle<StdSelect_BRepOwner>::DownCast(selector->Picked(rank));
          const bool ours = !hit.IsNull() && hit->Selectable().get() == entry.object.get() && hit->HasShape() &&
                            hit->Shape().IsSame(edge);
          if (rank == 1) {
            clean = ours;
          } else if (!ours && !hit.IsNull() && hit->HasShape() && hit->Shape().ShapeType() == TopAbs_EDGE) {
            clean = false;
          }
        }
        if (!clean) {
          continue;
        }
        best[side] = std::make_pair(at.toPoint(), distance);
      }
      for (std::size_t side = 0; side < best.size(); ++side) {
        if (!best[side]) {
          continue;
        }
        EdgePlace place;
        place.item = {SelectKind::Edge, QString::fromStdString(entry.uid),
                      QString::fromStdString(body.edge_name(i)), curveType(edge),
                      QString::fromStdString(entry.occurrence)};
        place.at = best[side]->first;
        place.sight = side == 0 ? EdgeSight::Visible : EdgeSight::Occluded;
        found.push_back(place);
      }
    }
  }
  m_context->ClearDetected(false);
  sceneChanged();
  return found;
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
  // A click that took the active component's item before a nearer one of
  // another component (mitcad#420).
  if (!m_windowPick && items.size() == 1 && mitcad::pickPrioritized(m_context) && !outsideActive(items.first())) {
    qDebug().noquote() << QStringLiteral("Pick priority: active %1").arg(m_activeScope.name);
  }
  m_context->MainSelector()->AllowOverlapDetection(false);
  m_windowSelecting = false; // what follows is a hover or a click again
  m_clickPickedNothing = !m_windowPick && items.isEmpty();
  if (m_windowPick && m_windowClock.isValid()) {
    // The view's own part of a window selection (mitcad#670): from the
    // release to the selection OCCT reports.
    qDebug().noquote() << QStringLiteral("Timing window pick: %1 ms, %2 item(s)")
                              .arg(static_cast<double>(m_windowClock.nsecsElapsed()) / 1.0e6, 0, 'f', 2)
                              .arg(items.size());
  }
  emit picked(items, m_pressModifiers, m_windowPick);
}

void OcctViewer::windowSelectionApplied() {
  if (!m_windowClock.isValid()) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Timing window selection applied: %1 ms")
                            .arg(static_cast<double>(m_windowClock.nsecsElapsed()) / 1.0e6, 0, 'f', 2);
  m_windowShowPending = true;
  requestFrame();
}

// ---------------------------------------------------------------------------
// Camera

void OcctViewer::lookAtSketchPlane(bool saveCamera, const TopoDS_Shape& face) {
  // Where the view is going: an animation ended at its end, a pending fit
  // made (a new design's), so that neither moves the camera afterwards.
  settleCamera();
  if (saveCamera || m_savedCamera.IsNull()) {
    m_savedCamera = new Graphic3d_Camera();
    m_savedCamera->Copy(m_view->Camera());
  }
  // A fit moves the camera's centre off what the middle of the free area
  // shows (fitToFreeArea), and a turn about that centre took the sketch's
  // origin off the middle in a direction of each plane's own (mitcad#738).
  // The view turns about the point to show in the middle instead. Entering
  // a sketch (`saveCamera`) shows its origin there while the view shows
  // nothing (no bodies, no sketches), or shows only sketches and the origin
  // is in the free area. Otherwise, and always for Look At, the view keeps
  // its middle: what the middle of the free area shows, on the sketch plane
  // (a pan to a far part of a sketch stays; a sketch on a face keeps that
  // face in view, also when its origin is a corner of the face).
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  const gp_Pnt origin = m_sketchPlane.Location();
  const gp_Dir& normal = m_sketchPlane.Direction();
  bool onOrigin = false;
  if (saveCamera && bodiesBox().IsVoid()) {
    const bool sketchesShown =
        std::any_of(m_sketchEntities.begin(), m_sketchEntities.end(), [this](const occ::handle<AIS_Shape>& object) {
          return !object.IsNull() && m_context->IsDisplayed(object);
        });
    const bool inFront =
        camera->IsOrthographic() || gp_Vec(camera->Eye(), origin).Dot(gp_Vec(camera->Direction())) > 0.0;
    onOrigin = !sketchesShown || (inFront && freeArea().contains(toWidgetF(origin)));
  }
  gp_Pnt centre = origin;
  bool onFace = false;
  if (!onOrigin) {
    const gp_Pnt middle = camera->Center().Translated(freeAreaShift(camera).Reversed());
    const gp_Vec along(normal);
    centre = middle.Translated(along * -gp_Vec(origin, middle).Dot(along));
    // A face seen at a steep angle (mitcad#772): the middle, at the
    // camera's depth, projected along the normal can lie far from the face
    // in its plane, and the face would leave the view after the turn.
    // Entering a sketch on it then shows the middle of the face's box.
    if (saveCamera && !face.IsNull()) {
      bool onIt = false;
      Bnd_Box box;
      for (TopExp_Explorer faces(face, TopAbs_FACE); faces.More() && !onIt; faces.Next()) {
        const TopoDS_Face& one = TopoDS::Face(faces.Current());
        BRepBndLib::Add(one, box);
        try {
          const double tolerance = std::max(BRep_Tool::Tolerance(one), Precision::Confusion());
          const TopAbs_State state = BRepClass_FaceClassifier(one, centre, tolerance).State();
          onIt = state == TopAbs_IN || state == TopAbs_ON;
        } catch (const Standard_Failure&) {
          // A face without a surface to classify on: its box counts.
        }
      }
      if (!onIt && !box.IsVoid()) {
        double x0 = 0.0;
        double y0 = 0.0;
        double z0 = 0.0;
        double x1 = 0.0;
        double y1 = 0.0;
        double z1 = 0.0;
        box.Get(x0, y0, z0, x1, y1, z1);
        const gp_Pnt boxMiddle((x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0);
        centre = boxMiddle.Translated(along * -gp_Vec(origin, boxMiddle).Dot(along));
        onFace = true;
      }
    }
  }
  // Turned to the plane about that point (the view looks against the
  // normal, the sketch's y up), at the same distance (one of its own for an
  // eye on its centre, mitcad#1009); then that point is moved to the middle
  // of the free area as a fit moves it there, without the fit's zoom. Never
  // an exception out of the Sketch Palette's Look At button's slot
  // (mitcad#773).
  runViewOperation(QStringLiteral("look at the sketch"), [this, &camera, &centre, &normal] {
    const double distance = turnDistance(camera->Distance(), centre);
    camera->SetEyeAndCenter(centre.Translated(gp_Vec(normal) * distance), centre);
    camera->SetUp(m_sketchPlane.YDirection());
    camera->OrthogonalizeUp();
    const gp_Vec move = freeAreaShift(camera);
    camera->SetEyeAndCenter(camera->Eye().Translated(move), camera->Center().Translated(move));
  });
  // For UI tests: the sketch's origin and the free area's middle in the
  // window (the same place when the view centred on the origin).
  const QPointF offset(mapTo(window(), QPoint(0, 0)));
  const QPointF at = toWidgetF(origin) + offset;
  const QPointF free = freeArea().center() + offset;
  qDebug().noquote() << QStringLiteral("Sketch plane centred on %1: origin at %2,%3; free area centre at %4,%5")
                            .arg(onOrigin ? QStringLiteral("the origin")
                                 : onFace ? QStringLiteral("the sketch's face")
                                          : QStringLiteral("the view's middle"))
                            .arg(at.x(), 0, 'f', 2)
                            .arg(at.y(), 0, 'f', 2)
                            .arg(free.x(), 0, 'f', 2)
                            .arg(free.y(), 0, 'f', 2);
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
  m_framingNewDesign = false;
  requestFrame();
}

void OcctViewer::frameNewDesign() {
  // The isometric view from the front right, Z up, as Home without a
  // document's home view and the view at the start; the framing waits for
  // the next frame, so that it is made at the view's size then.
  AbortViewAnimation();
  m_view->SetProj(V3d_TypeOfOrientation_Zup_AxoRight);
  m_fitPending = true;
  m_framingNewDesign = true;
  requestFrame();
}

double OcctViewer::visibleHeight() const { return m_view->Camera()->ViewDimensions().Y(); }

void OcctViewer::frameBox(const Bnd_Box& box) {
  if (box.IsVoid()) {
    return;
  }
  AbortViewAnimation();
  m_fitPending = false;
  m_framingNewDesign = false;
  occ::handle<Graphic3d_Camera> fitted = new Graphic3d_Camera();
  fitted->Copy(m_view->Camera());
  if (!m_view->FitMinMax(fitted, box, 0.15, 10.0 * Precision::Confusion())) {
    return;
  }
  keepEyeOffCentre(*fitted);
  fitToFreeArea(fitted);
  m_view->Camera()->Copy(fitted);
  m_view->Invalidate();
  cameraMoved();
  requestFrame();
}

bool OcctViewer::bringClearOfPanels(const Bnd_Box& box, bool fit) {
  if (box.IsVoid() || (!fit && m_covered.empty()) || width() <= 0 || height() <= 0) {
    return false;
  }
  // The box's corners on the screen.
  double x0 = 0.0;
  double y0 = 0.0;
  double z0 = 0.0;
  double x1 = 0.0;
  double y1 = 0.0;
  double z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  QRectF shown;
  for (const double x : {x0, x1}) {
    for (const double y : {y0, y1}) {
      for (const double z : {z0, z1}) {
        const QPointF at = toWidgetF(gp_Pnt(x, y, z));
        shown = shown.isNull() ? QRectF(at, QSizeF(0.001, 0.001)) : shown.united(QRectF(at, QSizeF(0.001, 0.001)));
      }
    }
  }
  const QRectF free = freeArea().adjusted(12, 12, -12, -12);
  if (free.isEmpty() || free.contains(shown)) {
    return false;
  }
  if (fit && (shown.width() > free.width() || shown.height() > free.height())) {
    qDebug().noquote() << QStringLiteral("View fitted: the parts clear of the panels");
    frameBox(box);
    return true;
  }
  // The least move that brings it in (centred when it is larger).
  const auto along = [](double low, double high, double freeLow, double freeHigh) {
    if (high - low > freeHigh - freeLow) {
      return (freeLow + freeHigh) / 2.0 - (low + high) / 2.0;
    }
    if (low < freeLow) {
      return freeLow - low;
    }
    if (high > freeHigh) {
      return freeHigh - high;
    }
    return 0.0;
  };
  const QPointF shift(along(shown.left(), shown.right(), free.left(), free.right()),
                      along(shown.top(), shown.bottom(), free.top(), free.bottom()));
  if (std::abs(shift.x()) < 1.0 && std::abs(shift.y()) < 1.0) {
    return false;
  }
  occ::handle<Graphic3d_Camera> moved = new Graphic3d_Camera();
  moved->Copy(m_view->Camera());
  const gp_XYZ extent = moved->ViewDimensions(moved->Distance());
  const double perPixel = extent.Y() / height();
  const std::optional<gp_Dir> right = crossedDir(moved->Direction(), moved->Up());
  if (!right) {
    return false; // no screen axes to move along (mitcad#773)
  }
  const gp_Vec move = (gp_Vec(*right) * shift.x() - gp_Vec(moved->Up()) * shift.y()) * -perPixel;
  moved->SetEyeAndCenter(moved->Eye().Translated(move), moved->Center().Translated(move));
  qDebug().noquote() << QStringLiteral("View moved %1,%2 px: the part clear of the panels")
                            .arg(qRound(shift.x()))
                            .arg(qRound(shift.y()));
  moveCamera(moved);
  return true;
}

Bnd_Box OcctViewer::bodiesBox() const {
  Bnd_Box box;
  for (const auto& [key, entry] : m_bodies) {
    if (entry.object.IsNull() || entry.ghosted || entry.style != BodyDisplay::Style::Normal ||
        !m_context->IsDisplayed(entry.object)) {
      continue;
    }
    BRepBndLib::Add(entry.object->Shape().Moved(entry.placement), box);
  }
  return box;
}

void OcctViewer::settleCamera() {
  const occ::handle<AIS_AnimationCamera>& animation = ViewAnimation();
  if (!animation.IsNull() && !animation->IsStopped()) {
    const occ::handle<Graphic3d_Camera> end = animation->CameraEnd();
    AbortViewAnimation();
    if (!end.IsNull()) {
      m_view->Camera()->Copy(end);
    }
    m_view->Invalidate();
    cameraMoved();
  }
  if (m_fitPending) {
    applyPendingFit(); // as the next frame would (paintGL)
    cameraMoved();
  }
  requestFrame();
}

void OcctViewer::fitPending(const occ::handle<Graphic3d_Camera>& camera) const {
  if (m_framingNewDesign) {
    frameOrigin(camera);
  } else {
    fitCamera(camera);
  }
}

void OcctViewer::applyPendingFit() {
  fitPending(m_view->Camera());
  m_view->ZFitAll();
  m_view->Invalidate();
  m_fitPending = false;
  m_fitMade = true;
  if (m_framingNewDesign) {
    m_framingNewDesign = false;
    // For reports and UI tests (mitcad#1000): the same scale at any size.
    qInfo().noquote() << QStringLiteral("View: new design framed, scale %1 mm per view height at %2")
                             .arg(visibleHeight(), 0, 'f', 3)
                             .arg(viewSizeText(*this));
  }
}

double OcctViewer::fieldOfView() const { return m_view->Camera()->FOVy(); }

void OcctViewer::setFieldOfView(double degrees) {
  m_view->Camera()->SetFOVy(degrees);
  m_view->Invalidate();
  cameraMoved();
  requestFrame();
}

void OcctViewer::lookAlong(const gp_Dir& normal, const gp_Dir& up) { turnTo(normal.Reversed(), up); }

void OcctViewer::lookHome() { goHome(); }

void OcctViewer::fitCamera(const occ::handle<Graphic3d_Camera>& fitted) const {
  // What FitAll fits: the shown objects, without the orientation cube.
  m_viewStep = "the shown objects' box";
  const Bnd_Box box = m_view->View()->MinMaxValues();
  if (box.IsVoid()) {
    // Nothing to fit: an empty design, whose origin is hidden. OCCT's fit
    // would keep the camera as it was (the previous design's or the
    // start's), and the fit into the free area zoomed it out further on
    // each Fit (mitcad#1000); the origin is framed instead.
    frameOrigin(fitted);
    return;
  }
  if (box.IsWhole()) {
    return;
  }
  // OCCT leaves out boxes beyond float range, not those that are no number
  // (an object at a place that is none): the camera would become none too
  // (mitcad#773), so the turn keeps the camera's size and place then.
  double x0 = 0.0;
  double y0 = 0.0;
  double z0 = 0.0;
  double x1 = 0.0;
  double y1 = 0.0;
  double z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  if (!isFinite(gp_XYZ(x0, y0, z0)) || !isFinite(gp_XYZ(x1, y1, z1))) {
    qWarning().noquote() << "View fit left out: the shown objects' box is not finite";
    return;
  }
  m_viewStep = "the fit";
  if (!m_view->FitMinMax(fitted, box, 0.15, 10.0 * Precision::Confusion())) {
    // A box with no size (a point shown at the origin): framed as nothing.
    frameOrigin(fitted);
    return;
  }
  m_viewStep = "the fit to the free area";
  keepEyeOffCentre(*fitted);
  fitToFreeArea(fitted);
}

void OcctViewer::frameOrigin(const occ::handle<Graphic3d_Camera>& camera) const {
  if (m_originBox.IsVoid()) {
    return; // no origin known: the camera stays as it is
  }
  m_viewStep = "the origin's framing";
  // Fitted as in a view at least as wide as tall: the visible height then
  // depends only on the box and the direction, not on the view's size, and
  // a view that is narrow for a moment (a window being maximized) frames it
  // as the wide view it becomes.
  const double aspect = camera->Aspect();
  occ::handle<Graphic3d_Camera> wide = new Graphic3d_Camera();
  wide->Copy(camera);
  wide->SetAspect(std::max(1.0, aspect));
  if (!wide->FitMinMax(m_originBox, 10.0 * Precision::Confusion(), false)) {
    return;
  }
  const double height = wide->ViewDimensions().Y() / kOriginShare;
  camera->Copy(wide);
  camera->SetAspect(aspect);
  setVisibleHeight(*camera, height);
  // In the middle of the cards' free area, without the zoom out of a fit
  // into it (that depends on the cards' share of the view).
  m_viewStep = "the origin's move to the free area";
  const gp_Vec move = freeAreaShift(camera);
  camera->SetEyeAndCenter(camera->Eye().Translated(move), camera->Center().Translated(move));
}

QRectF OcctViewer::freeArea() const {
  // The cards are strips at the sides or along the bottom (the timeline,
  // as wide as the view): each takes the whole strip of the view it
  // reaches into, from its edge, so that the free area stays a rectangle.
  const QRectF whole(0.0, 0.0, width(), height());
  qreal left = 0.0;
  qreal right = whole.right();
  qreal top = m_topCardsBottom; // below the rows of cards at the top (mitcad#425)
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
  const gp_Vec move = freeAreaShift(camera);
  camera->SetEyeAndCenter(camera->Eye().Translated(move), camera->Center().Translated(move));
}

gp_Vec OcctViewer::freeAreaShift(const occ::handle<Graphic3d_Camera>& camera) const {
  if (width() <= 0 || height() <= 0) {
    return gp_Vec(0.0, 0.0, 0.0);
  }
  // The model sits in the middle of the view; the middle of the free area
  // is dx, dy pixels from there. Moving the camera the other way moves the
  // model there.
  const QPointF shift = freeArea().center() - QPointF(width() / 2.0, height() / 2.0);
  const gp_XYZ extent = camera->ViewDimensions(camera->Distance());
  const double perPixel = extent.Y() / height();
  const std::optional<gp_Dir> right = crossedDir(camera->Direction(), camera->Up());
  if (!right) {
    return gp_Vec(0.0, 0.0, 0.0); // no screen axes to pan along (mitcad#773)
  }
  return (gp_Vec(*right) * shift.x() - gp_Vec(camera->Up()) * shift.y()) * -perPixel;
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
  runViewOperation(QStringLiteral("turn"), [this, &direction, &up] { turnCameraTo(direction, up); });
}

void OcctViewer::turnCameraTo(const gp_Dir& direction, const gp_Dir& up) {
  if (!isFinite(direction.XYZ())) {
    qWarning().noquote() << "View turn left out: the direction is not finite";
    return;
  }
  m_viewStep = "the turned camera";
  occ::handle<Graphic3d_Camera> end = new Graphic3d_Camera();
  end->Copy(m_view->Camera());
  // OCCT turns the camera only for an eye away from the centre: a camera
  // whose centre or distance is no number, or whose eye reached its centre,
  // would keep its old direction, and an up along that direction makes no
  // view (mitcad#773). Such a camera turns about the origin from a distance
  // of its own, and the up direction is made square to the new direction.
  gp_Pnt center = end->Center();
  if (!isFinite(center.XYZ())) {
    center = gp_Pnt(0.0, 0.0, 0.0);
  }
  const double distance = turnDistance(end->Distance(), center);
  end->SetEyeAndCenter(center.Translated(gp_Vec(direction) * -distance), center);
  end->SetUp(upAcross(direction, up.XYZ()));
  end->OrthogonalizeUp();
  fitCamera(end);
  m_viewStep = "the camera's move";
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

void OcctViewer::runViewOperation(const QString& operation, const std::function<void()>& run) {
  occ::handle<Graphic3d_Camera> before = new Graphic3d_Camera();
  before->Copy(m_view->Camera());
  const char* outerStep = m_viewStep;
  m_viewStep = nullptr;
  QString failure;
  try {
    run();
  } catch (const Standard_Failure& error) {
    failure = QStringLiteral("%1: %2").arg(QString::fromLatin1(typeid(error).name()),
                                           QString::fromUtf8(error.what()));
  } catch (const std::exception& error) {
    failure = QStringLiteral("%1: %2").arg(QString::fromLatin1(typeid(error).name()), QString::fromUtf8(error.what()));
  } catch (...) {
    failure = QStringLiteral("an exception of an unknown type");
  }
  const char* step = m_viewStep;
  m_viewStep = outerStep;
  if (failure.isEmpty()) {
    return;
  }
  // "View operation failed: turn (the fit to the free area):
  // Standard_ConstructionError: …", for UI tests and reports (mitcad#773).
  qWarning().noquote() << QStringLiteral("View operation failed: %1%2: %3")
                              .arg(operation,
                                   step != nullptr ? QStringLiteral(" (%1)").arg(QString::fromLatin1(step)) : QString(),
                                   failure);
  AbortViewAnimation();
  m_view->Camera()->Copy(before);
  m_view->Invalidate();
  cameraMoved();
  requestFrame();
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
  scaleViewCube(*m_orientationCube);
  m_orientationCube->SetTransformPersistence(
      cubePersistence(m_cubeRatio, m_overlayInsets, m_cubeProjection != CubeProjection::FollowView));
  if (displayed) {
    m_context->Display(m_orientationCube, 0, 0, false);
  }
  sceneChanged();
}

void OcctViewer::setCoveredRects(const std::vector<QRect>& rects) {
  if (rects == m_covered) {
    return;
  }
  m_covered = rects;
  emit coveredChanged();
}

void OcctViewer::setTopCards(const std::vector<QRect>& cards, int bottom) {
  m_topCards = cards;
  m_topCardsBottom = std::max(0, bottom);
}

bool OcctViewer::isCovered(const QPointF& position) const {
  // The top cards exactly: the gaps between them are narrower than the margin.
  if (std::any_of(m_topCards.begin(), m_topCards.end(),
                  [&position](const QRect& rect) { return QRectF(rect).contains(position); })) {
    return true;
  }
  if (std::any_of(m_handleBoxes.begin(), m_handleBoxes.end(),
                  [&position](const QRect& rect) { return QRectF(rect).adjusted(-3, -3, 3, 3).contains(position); })) {
    return true;
  }
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
  if (m_nativeControls) {
    m_homeButton->hide();
    return;
  }
  // At the cube's upper left.
  m_homeButton->move(width() - m_overlayInsets.right() - kCubeOffset - 73, m_overlayInsets.top() + 9);
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
  publishCubeArrows();
}

void OcctViewer::updateCubeArrows() {
  // Only while the view looks straight at a face.
  const gp_Dir direction = m_view->Camera()->Direction();
  const double along = std::max({std::abs(direction.X()), std::abs(direction.Y()), std::abs(direction.Z())});
  const bool faceOn = along > 1.0 - 1e-6;
  m_cubeFaceOn = faceOn;
  if (m_nativeControls) {
    for (const auto& arrow : m_cubeArrows) {
      arrow.second->hide();
    }
    publishCubeArrows();
    return;
  }
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

void OcctViewer::setNativeControls(bool native) {
  m_nativeControls = native;
  if (m_homeButton != nullptr) {
    m_homeButton->setVisible(!native);
  }
  for (const auto& arrow : m_cubeArrows) {
    if (native) {
      arrow.second->hide();
    }
  }
  publishCubeArrows();
}

void OcctViewer::publishCubeArrows() {
  // The hidden Qt arrows keep their places (placeCubeArrows): the native
  // ones take them, the same buttons in the same spots around the cube.
  std::vector<CubeArrow> arrows;
  if (m_nativeControls && m_cubeFaceOn) {
    for (const auto& [id, arrow] : m_cubeArrows) {
      arrows.push_back({id, arrow->toolTip(), overlaySymbol(id), arrow->geometry()});
    }
  }
  if (arrows != m_shownCubeArrows) {
    m_shownCubeArrows = std::move(arrows);
    emit cubeArrowsChanged();
  }
}

void OcctViewer::pressCubeArrow(const QString& id) {
  runViewOperation(QStringLiteral("cube arrow ") + id, [this, id] { turnCube(id); });
}

void OcctViewer::turnCube(const QString& arrow) {
  // The axes of the view (from the eye to the model, up, right), snapped
  // to the model's axes: the arrows show only while they are face on. The
  // up direction is snapped across the snapped direction, so that the three
  // stay square whatever the camera holds: snapped on their own, an up that
  // leaned towards the direction would become it, and their cross would
  // throw (mitcad#773).
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  const gp_Dir direction = nearestAxis(camera->Direction().XYZ());
  const gp_Dir up = nearestAxis(upAcross(direction, camera->Up().XYZ()).XYZ());
  qDebug().noquote() << QStringLiteral("Orientation cube arrow %1").arg(arrow);
  const std::optional<gp_Dir> across = crossedDir(direction, up);
  if (!across) {
    return; // not expected: the two are square
  }
  const gp_Dir right = *across;
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
  updateCubeEye(); // picked as the camera looks now (mitcad#774)
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
  if (m_resizeToLog) {
    // The view's size and the height it shows once a resize rests, with
    // the window's state (mitcad#1000): a report's framing read from the
    // log.
    m_resizeToLog = false;
    const Qt::WindowStates state = window()->windowState();
    const QString stateText = state.testFlag(Qt::WindowFullScreen) ? QStringLiteral(", full screen")
                                : state.testFlag(Qt::WindowMaximized) ? QStringLiteral(", maximized")
                                                                      : QString();
    qInfo().noquote() << QStringLiteral("View: resized to %1%2, scale %3 mm per view height")
                             .arg(viewSizeText(*this), stateText)
                             .arg(visibleHeight(), 0, 'f', 3);
  }
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
  // Where the camera looks from and how far it shows: a zoom or a pan keeps
  // the direction (no new camera line) but moves the places listeners log
  // (the datums' pick places), so it rests too.
  const gp_Pnt eye = current->Eye();
  const QString view = QStringLiteral("%1 %2 %3 %4")
                           .arg(eye.X(), 0, 'g', 9)
                           .arg(eye.Y(), 0, 'g', 9)
                           .arg(eye.Z(), 0, 'g', 9)
                           .arg(current->Scale(), 0, 'g', 9);
  if (text == m_loggedCamera) {
    if (view != m_restedView) {
      m_restedView = view;
      emit cameraRested();
    }
    return;
  }
  m_loggedCamera = text;
  m_restedView = view;
  updateCubeArrows();
  // Only on request: every place of the cube's corner is tried. Before the
  // camera's line, so that the places are there when a test reads it. Not
  // before the view has its window: picking then crashes in OCCT's selector
  // (a design opened at start-up, before the window appears). The resize
  // that shows the window logs the camera again.
  if (qEnvironmentVariableIsSet("MITCAD_LOG_ORIENTATION_CUBE") && !m_view->Window().IsNull() && isVisible()) {
    logOrientationCube();
  }
  qDebug().noquote() << text;
  emit cameraRested();
}

void OcctViewer::logOrientationCube() {
  // Where a click hits each face, edge and corner of the cube in sight: of
  // the places on a grid that pick it, the one nearest their middle whose
  // neighbours pick it too. The cube's own perspective as the camera looks
  // now, as the next frame draws it.
  updateCubeEye();
  constexpr int kStep = 2;
  const QRect area = cubeArea().toRect();
  const int columns = area.width() / kStep;
  const int rows = area.height() / kStep;
  std::vector<int> grid(static_cast<std::size_t>(columns * rows), -1);
  const auto at = [&](int i, int j) { return grid[static_cast<std::size_t>(j * columns + i)]; };
  std::map<int, std::vector<std::pair<int, int>>> places;
  // The extent of the places that pick the cube (grid cells).
  int pickLeft = columns;
  int pickTop = rows;
  int pickRight = -1;
  int pickBottom = -1;
  for (int j = 0; j < rows; ++j) {
    for (int i = 0; i < columns; ++i) {
      const NCollection_Vec2<int> pixel =
          toDevicePixels(QPointF(area.left() + i * kStep, area.top() + j * kStep));
      m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
      const auto owner = occ::handle<AIS_ViewCubeOwner>::DownCast(m_context->DetectedOwner());
      if (!owner.IsNull()) {
        grid[static_cast<std::size_t>(j * columns + i)] = owner->MainOrientation();
        places[owner->MainOrientation()].emplace_back(i, j);
        pickLeft = std::min(pickLeft, i);
        pickTop = std::min(pickTop, j);
        pickRight = std::max(pickRight, i);
        pickBottom = std::max(pickBottom, j);
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
  // The cube's outline as drawn and the extent of its picks, in window
  // pixels, with the camera's direction and projection and the cube's
  // (mitcad#766, mitcad#774: an orthographic or perspective cube is the same
  // in either view projection; UI tests compare them).
  const QRectF outline = cubeOutline();
  if (!outline.isEmpty() && pickRight >= 0) {
    const QPoint from = mapTo(window(), QPoint(qRound(outline.left()), qRound(outline.top())));
    const QPoint to = mapTo(window(), QPoint(qRound(outline.right()), qRound(outline.bottom())));
    const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
    qDebug().noquote() << QStringLiteral("Orientation cube outline %1,%2 to %3,%4, picks %5,%6 to %7,%8, "
                                         "looking %9 (%10), cube %11")
                              .arg(from.x())
                              .arg(from.y())
                              .arg(to.x())
                              .arg(to.y())
                              .arg(corner.x() + pickLeft * kStep)
                              .arg(corner.y() + pickTop * kStep)
                              .arg(corner.x() + pickRight * kStep)
                              .arg(corner.y() + pickBottom * kStep)
                              .arg(vectorText(camera->Direction().XYZ()),
                                   isPerspective() ? QStringLiteral("perspective") : QStringLiteral("orthographic"),
                                   cubeProjectionId(m_cubeProjection));
  }
  requestFrame();
}

QRectF OcctViewer::cubeOutline() const {
  if (m_view->Window().IsNull() || m_orientationCube->TransformPersistence().IsNull()) {
    return {};
  }
  int width = 0;
  int height = 0;
  m_view->Window()->Size(width, height);
  if (width <= 0 || height <= 0) {
    return {};
  }
  // As OpenGl_Structure draws it: the view's projection after the world-view
  // matrix the persistence makes (for an orthographic cube in a perspective
  // view it undoes the perspective projection).
  const occ::handle<Graphic3d_Camera>& camera = m_view->Camera();
  const NCollection_Mat4<double>& projection = camera->ProjectionMatrix();
  NCollection_Mat4<double> worldView = camera->OrientationMatrix();
  m_orientationCube->TransformPersistence()->Apply(camera, projection, worldView, width, height);
  const NCollection_Mat4<double> toClip = projection * worldView;
  // The box: the faces lie this far from its middle (ViewCube.cpp).
  const double half = m_orientationCube->Size() * 0.5 + m_orientationCube->BoxFacetExtension();
  double left = std::numeric_limits<double>::max();
  double top = std::numeric_limits<double>::max();
  double right = std::numeric_limits<double>::lowest();
  double bottom = std::numeric_limits<double>::lowest();
  for (int i = 0; i < 8; ++i) {
    // Where the cube's own perspective (mitcad#774) puts the corner.
    const gp_XYZ at = viewCubePoint(*m_orientationCube, gp_XYZ((i & 1) != 0 ? half : -half,
                                                               (i & 2) != 0 ? half : -half,
                                                               (i & 4) != 0 ? half : -half));
    const NCollection_Vec4<double> boxCorner(at.X(), at.Y(), at.Z(), 1.0);
    const NCollection_Vec4<double> clip = toClip * boxCorner;
    if (clip.w() <= 0.0) {
      return {};
    }
    // Normalised device coordinates to device pixels, y down.
    const double x = (clip.x() / clip.w() + 1.0) * 0.5 * width;
    const double y = (1.0 - clip.y() / clip.w()) * 0.5 * height;
    left = std::min(left, x);
    top = std::min(top, y);
    right = std::max(right, x);
    bottom = std::max(bottom, y);
  }
  const qreal ratio = devicePixelRatioF();
  return QRectF(QPointF(left / ratio, top / ratio), QPointF(right / ratio, bottom / ratio));
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

CameraState OcctViewer::pendingCamera() const {
  occ::handle<Graphic3d_Camera> pending = new Graphic3d_Camera();
  pending->Copy(m_view->Camera());
  const occ::handle<AIS_AnimationCamera>& animation = ViewAnimation();
  if (!animation.IsNull() && !animation->IsStopped() && !animation->CameraEnd().IsNull()) {
    pending->Copy(animation->CameraEnd());
  }
  if (m_fitPending) {
    fitPending(pending);
  }
  return {pending->Eye(), pending->Center(), pending->Up(),
          pending->ProjectionType() == Graphic3d_Camera::Projection_Perspective,
          pending->ViewDimensions().Y()};
}

void OcctViewer::setCamera(const CameraState& state) {
  // Not a number (a stored view that is broken) fails these too.
  if (!(state.eye.Distance(state.target) > Precision::Confusion()) || !(state.height > 0.0) ||
      !std::isfinite(state.height)) {
    return;
  }
  runViewOperation(QStringLiteral("camera"), [this, &state] {
    m_viewStep = "the camera";
    occ::handle<Graphic3d_Camera> end = new Graphic3d_Camera();
    end->Copy(m_view->Camera());
    end->SetProjectionType(state.perspective ? Graphic3d_Camera::Projection_Perspective
                                             : Graphic3d_Camera::Projection_Orthographic);
    end->SetEyeAndCenter(state.eye, state.target);
    // Square to the direction (mitcad#773): an up along it makes no view.
    end->SetUp(upAcross(end->Direction(), state.up.XYZ()));
    end->OrthogonalizeUp();
    // The stored height is what the view shows (camera(): ViewDimensions);
    // OCCT's scale is the shorter side, so a view taller than wide takes
    // the height times its aspect (mitcad#959).
    setVisibleHeight(*end, state.height);
    m_viewStep = "the camera's move";
    moveCamera(end);
  });
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

QString cubeProjectionId(CubeProjection projection) {
  switch (projection) {
  case CubeProjection::Orthographic:
    return QStringLiteral("orthographic");
  case CubeProjection::Perspective:
    return QStringLiteral("perspective");
  case CubeProjection::FollowView:
    break;
  }
  return QStringLiteral("follow-view");
}

void OcctViewer::setCubeProjection(CubeProjection projection) {
  if (projection == m_cubeProjection) {
    return;
  }
  m_cubeProjection = projection;
  setViewCubePerspective(*m_orientationCube, projection == CubeProjection::Perspective);
  setViewCubeEye(*m_orientationCube, m_view->Camera()->Direction().Reversed());
  applyCubePlacement();
  if (m_context->IsDisplayed(m_orientationCube)) {
    m_context->RecomputeSelectionOnly(m_orientationCube); // picked as drawn
  }
  qDebug().noquote() << QStringLiteral("Orientation cube projection %1").arg(cubeProjectionId(projection));
  // Its places and outline logged again when the camera rests (UI tests),
  // also where the camera did not move.
  m_loggedCamera.clear();
  cameraMoved();
  requestFrame();
}

void OcctViewer::updateCubeEye() {
  if (m_cubeProjection != CubeProjection::Perspective ||
      !setViewCubeEye(*m_orientationCube, m_view->Camera()->Direction().Reversed())) {
    return;
  }
  if (m_context->IsDisplayed(m_orientationCube)) {
    m_context->Redisplay(m_orientationCube, false);
    m_context->RecomputeSelectionOnly(m_orientationCube);
  }
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
  gestures.Bind(Aspect_VKeyMouse_LeftButton | kCtrlShift, AIS_MouseGesture_SelectRectangle);
  // Dragging the orientation cube orbits.
  gestures.Bind(Aspect_VKeyMouse_LeftButton | kCubeDrag, AIS_MouseGesture_RotateOrbit);
  const auto gesture = [](NavigationAction action) {
    switch (action) {
    case NavigationAction::Orbit: return AIS_MouseGesture_RotateOrbit;
    case NavigationAction::Pan: return AIS_MouseGesture_Pan;
    case NavigationAction::Zoom: return AIS_MouseGesture_Zoom;
    case NavigationAction::None: return AIS_MouseGesture_NONE;
    }
    return AIS_MouseGesture_NONE;
  };
  const auto bindings = m_navigation.scheme == NavigationScheme::Custom ? m_navigation.customBindings
                                                                        : navigationBindings(m_navigation.scheme);
  for (const auto& binding : bindings) {
    if (!navigationBindingAllowed(binding) || binding.action == NavigationAction::None ||
        binding.input == NavigationInput::TrackpadScroll) {
      continue;
    }
    const Aspect_VKeyMouse button = binding.input == NavigationInput::LeftButton ? Aspect_VKeyMouse_LeftButton
                                  : binding.input == NavigationInput::MiddleButton ? Aspect_VKeyMouse_MiddleButton
                                                                                  : Aspect_VKeyMouse_RightButton;
    const auto flags = binding.f4 ? kF4Drag : toVKeyFlags(binding.modifiers);
    gestures.Bind(button | flags, gesture(binding.action));
  }
  if (m_navigation.scheme == NavigationScheme::Trackpad ||
      m_navigation.scheme == NavigationScheme::TrackpadShift ||
      m_navigation.scheme == NavigationScheme::Custom) {
    gestures.Bind(Aspect_VKeyMouse_LeftButton | kScrollPan, AIS_MouseGesture_Pan);
    gestures.Bind(Aspect_VKeyMouse_LeftButton | kScrollOrbit, AIS_MouseGesture_RotateOrbit);
  }
}

void OcctViewer::setNavigation(const NavigationSettings& settings) {
  endScrollDrag(); // the gestures are bound anew
  cancelPointerInteraction();
  m_navigation = settings;
  m_navigation.customBindings = normalizedNavigationBindings(settings.customBindings, NavigationScheme::Custom);
  m_navigation.orbitSpeed = sanitizeNavigationSpeed(settings.orbitSpeed);
  m_navigation.panSpeed = sanitizeNavigationSpeed(settings.panSpeed);
  m_navigation.zoomSpeed = sanitizeNavigationSpeed(settings.zoomSpeed);
  m_navigation.trackpadSpeed = sanitizeNavigationSpeed(settings.trackpadSpeed);
  m_navigation.spaceMouse.speed = sanitizeSpaceMouseSpeed(settings.spaceMouse.speed);
  m_panRemainder = {};
  bindGestures();
  SetOrbitAcceleration(float(m_navigation.orbitSpeed));
  SetLockOrbitZUp(settings.constrainedOrbit);
  emit navigationChanged(); // the 3D mouse follows (mitcad#951)
}

// OCCT exposes orbit acceleration, but its mouse acceleration only affects
// first-person rotation. Pan retains OCCT's picked anchor and projection;
// only the integer delta is scaled, carrying subpixel motion across frames.
void OcctViewer::handlePanning(const occ::handle<V3d_View>& view) {
  auto& pan = ChangeInputBuffer(AIS_ViewInputBufferType_GL).Panning;
  if (pan.ToStart) {
    m_panRemainder = {};
  }
  if (!pan.ToPan || m_navigation.panSpeed == 1.0) {
    AIS_ViewController::handlePanning(view);
    return;
  }
  const auto original = pan.Delta;
  const QPointF scaled = QPointF(pan.Delta.x(), pan.Delta.y()) * m_navigation.panSpeed + m_panRemainder;
  pan.Delta.SetValues(qRound(scaled.x()), qRound(scaled.y()));
  m_panRemainder = scaled - QPointF(pan.Delta.x(), pan.Delta.y());
  AIS_ViewController::handlePanning(view);
  pan.Delta = original;
}

bool OcctViewer::UpdateZoom(const Aspect_ScrollDelta& delta) {
  Aspect_ScrollDelta scaled = delta;
  scaled.Delta *= m_navigation.zoomSpeed;
  return AIS_ViewController::UpdateZoom(scaled);
}

void OcctViewer::setBackground(const QColor& top, const QColor& bottom, bool gradient) {
  // The hidden-line style's faces: the background's middle.
  const QColor paper = QColor::fromRgbF((top.redF() + bottom.redF()) / 2.0f,
                                        (top.greenF() + bottom.greenF()) / 2.0f,
                                        (top.blueF() + bottom.blueF()) / 2.0f);
  if ((paper.lightnessF() < 0.5f) != m_backgroundDark) {
    m_backgroundDark = !m_backgroundDark;
    updateOverlayIcons(); // the buttons over the view
    setViewCubeBackdrop(*m_orientationCube, m_backgroundDark); // the cube's glass
    m_homeButton->setDark(m_backgroundDark);
    m_context->Redisplay(m_orientationCube, false);
    restyleSketchEntities();
    // A feature's input lines have a variant for each tone (mitcad#957).
    if (std::any_of(m_shownHighlights.cbegin(), m_shownHighlights.cend(),
                    [](const HighlightDisplay& shown) { return shown.inputLines; })) {
      setHighlights(m_shownHighlights);
    }
    emit backgroundToneChanged();
  }
  if (paper != m_paper) {
    m_paper = paper;
    if (m_visualStyle == VisualStyle::WireframeHiddenEdges) {
      for (auto& [key, entry] : m_bodies) {
        removeBody(entry);
        showBody(entry);
      }
    }
    // What is pale outside the active component fades towards the new
    // background (mitcad#420).
    if (!m_inactiveObjects.empty()) {
      applyInactiveGhosts(true);
    }
  }
  m_backgroundTop = top;
  m_backgroundBottom = bottom;
  m_backgroundGradient = gradient;
  applyBackground();
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
  if (!m_gridShown || m_view->Window().IsNull() || showsRenderedImage()) {
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

void OcctViewer::setSectionCaps(const std::vector<SectionCap>& caps, const gp_Ax3& plane) {
  for (const occ::handle<AIS_Shape>& object : m_sectionCaps) {
    m_context->Remove(object, false);
  }
  for (const occ::handle<AIS_Shape>& object : m_sectionHatches) {
    m_context->Remove(object, false);
  }
  m_sectionCaps.clear();
  m_sectionHatches.clear();
  m_sectionLook.clear();
  for (std::size_t i = 0; i < caps.size(); ++i) {
    if (caps[i].faces.IsNull()) {
      continue;
    }
    const QColor color = caps[i].color.isValid() ? caps[i].color : kCapColor;
    const QColor hatchColor = capHatchColor(color);
    // The components' caps hatch in turn at 45 and 135 degrees, as drawings do.
    const bool mirrored = i % 2 == 1;
    occ::handle<AIS_Shape> object = new AIS_Shape(caps[i].faces);
    object->SetColor(occtColor(color));
    // In the colour itself, as the timeline shows it: light does not darken
    // it (a cap facing away from the light would hide its hatch).
    object->SetMaterial(flatMaterial(0.0f));
    // Over the clipped body's own face on the plane (the same triangles),
    // under a sketch's curves, points and profiles on it (Hide Above
    // Sketch): just behind the plane, not in front of it (mitcad#784; see
    // kSectionOffsetFactor).
    object->Attributes()->ShadingAspect()->Aspect()->SetPolygonOffsets(Aspect_POM_Fill, kSectionOffsetFactor,
                                                                       kSectionCapOffsetUnits);
    const occ::handle<Prs3d_Drawer>& drawer = object->Attributes();
    drawer->SetFaceBoundaryDraw(true);
    drawer->SetFaceBoundaryAspect(new Prs3d_LineAspect(kEdgeColor, Aspect_TOL_SOLID, 1.5));
    m_context->Display(object, AIS_Shaded, -1, false);
    m_sectionCaps.push_back(object);
    int lines = 0;
    const TopoDS_Shape hatch = hatchOf(caps[i].faces, plane, mirrored);
    if (!hatch.IsNull()) {
      for (TopExp_Explorer edges(hatch, TopAbs_EDGE); edges.More(); edges.Next()) {
        ++lines;
      }
      occ::handle<AIS_Shape> hatchObject = new AIS_Shape(hatch);
      setEdgeStyle(hatchObject, occtColor(hatchColor), kHatchWidth);
      // The layer of the edges in sight: after the caps' own layer, against
      // their depth.
      m_context->SetZLayer(hatchObject, m_edgeLayer);
      m_context->Display(hatchObject, AIS_WireFrame, -1, false);
      m_sectionHatches.push_back(hatchObject);
    }
    m_sectionLook.append(QStringLiteral("%1 hatch %2 at %3 deg, %4 lines")
                             .arg(color.name(), hatchColor.name())
                             .arg(mirrored ? 135 : 45)
                             .arg(lines));
  }
  logSectionCaps();
  sceneChanged();
}

// For UI tests: what the caps look like, when it changes (a new cut or a
// new visual style).
void OcctViewer::logSectionCaps() {
  if (m_sectionLook.isEmpty()) {
    m_loggedSectionLook.clear();
    return;
  }
  const char* style = "Shaded";
  switch (m_visualStyle) {
  case VisualStyle::Shaded:
    break;
  case VisualStyle::ShadedVisibleEdges:
    style = "Shaded with Visible Edges Only";
    break;
  case VisualStyle::ShadedHiddenEdges:
    style = "Shaded with Hidden Edges";
    break;
  case VisualStyle::Wireframe:
    style = "Wireframe";
    break;
  case VisualStyle::WireframeHiddenEdges:
    style = "Wireframe with Hidden Edges";
    break;
  }
  const QString logged = QStringLiteral("Section caps: %1 colour(s) [%2] in %3, bodies in %4 colours")
                             .arg(m_sectionLook.size())
                             .arg(m_sectionLook.join(QStringLiteral("; ")), QString::fromLatin1(style),
                                  m_componentColors ? QStringLiteral("component") : QStringLiteral("appearance"));
  if (logged != m_loggedSectionLook) {
    m_loggedSectionLook = logged;
    qDebug().noquote() << logged;
  }
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

void OcctViewer::setBuildVolume(const TopoDS_Shape& frame, const TopoDS_Shape& outside, const QColor& warning) {
  for (occ::handle<AIS_Shape>* shown : {&m_buildFrame, &m_buildOutside}) {
    if (!shown->IsNull()) {
      m_context->Remove(*shown, false);
      shown->Nullify();
    }
  }
  if (!frame.IsNull()) {
    m_buildFrame = new AIS_Shape(frame);
    setEdgeStyle(m_buildFrame, kBuildVolumeColor, 1.5);
    m_context->Display(m_buildFrame, AIS_WireFrame, -1, false);
  }
  if (!outside.IsNull()) {
    m_buildOutside = new AIS_Shape(outside);
    setEdgeStyle(m_buildOutside,
                 Quantity_Color(warning.redF(), warning.greenF(), warning.blueF(), Quantity_TOC_sRGB), 2.0,
                 Aspect_TOL_DASH);
    m_context->Display(m_buildOutside, AIS_WireFrame, -1, false);
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

std::vector<gp_Pnt> OcctViewer::planePickCandidates(const gp_Pnt& origin, const gp_Dir& normal, double size,
                                                     bool offDiagonals) const {
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
  if (axes.size() == 2 && offDiagonals) {
    // Towards the eye, nearer one axis than the other: in an isometric view
    // the diagonal towards the eye lies along the third axis's image.
    directions = {axes[0] + axes[1] * 0.5, axes[0] * 0.5 + axes[1], axes[0] - axes[1] * 0.5,
                  axes[1] - axes[0] * 0.5};
  } else if (axes.size() == 2) {
    directions.push_back(axes[0] - axes[1]);
    directions.push_back(axes[1] - axes[0]);
    directions.push_back(toward * -1.0);
  }
  std::vector<gp_Pnt> candidates;
  const QRectF inside = QRectF(rect()).adjusted(12, 12, -12, -12);
  const QRectF cube = cubeArea();
  for (const gp_XYZ& direction : directions) {
    for (const double fraction : {1.0 / 3.0, 0.25, 0.18, 0.12, 0.08, 0.05}) {
      const gp_Pnt candidate(origin.XYZ() + direction * (size * fraction));
      // Those off the diagonals only where a click reaches the plane, not
      // the orientation cube.
      const QPointF at = toWidgetF(candidate);
      if (offDiagonals && (!inside.contains(at) || cube.contains(at))) {
        continue;
      }
      candidates.push_back(candidate);
    }
  }
  return candidates;
}

gp_Dir OcctViewer::viewUp() const { return m_view->Camera()->Up(); }

gp_Dir OcctViewer::viewDirection() const { return m_view->Camera()->Direction(); }

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

bool OcctViewer::claimsClick(const QPointF& position) {
  // Precise targets: what a click there picks before an area (a face, a
  // plane, a body, a profile) under it.
  const auto precise = [](const SelectionItem& item) {
    return item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex || item.kind == SelectKind::SketchCurve ||
           item.kind == SelectKind::SketchPoint || item.kind == SelectKind::Axis || item.kind == SelectKind::Point;
  };
  const std::vector<SelectionItem> picked = itemsAt(position);
  bool claimed = std::any_of(picked.begin(), picked.end(), precise);
  if (!claimed) {
    // A body edge or vertex the finer test refuses: the view's all the
    // same, so that the filter's rules (behind faces, Shift) decide there.
    m_untestedPicks = true;
    const std::vector<SelectionItem> under = itemsAt(position);
    m_untestedPicks = false;
    claimed = std::any_of(under.begin(), under.end(), [](const SelectionItem& item) {
      return item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
    });
  }
  ResetPreviousMoveTo(); // the hover picks again as before
  return claimed;
}

std::optional<SelectionItem> OcctViewer::takeRefusedClick() {
  if (!m_clickPickedNothing || !m_pickTest) {
    return std::nullopt;
  }
  m_clickPickedNothing = false;
  return untestedClickItem();
}

std::optional<SelectionItem> OcctViewer::untestedClickItem() {
  if (!m_pickTest) {
    return std::nullopt;
  }
  // What the click would have taken without the finer test: the first item
  // under it of the filter's kinds.
  m_untestedPicks = true;
  std::optional<SelectionItem> item = isCovered(m_clickPosition) ? std::nullopt : itemAt(m_clickPosition);
  m_untestedPicks = false;
  ResetPreviousMoveTo(); // the hover picks again with the test
  return item;
}

std::vector<std::pair<SelectionItem, QPoint>> OcctViewer::refusedPickPoints(SelectFilter kinds) {
  const std::vector<std::pair<SelectionItem, QPoint>> taken = pickPoints(kinds);
  m_untestedPicks = true;
  std::vector<std::pair<SelectionItem, QPoint>> all = pickPoints(kinds);
  m_untestedPicks = false;
  ResetPreviousMoveTo();
  std::vector<std::pair<SelectionItem, QPoint>> refused;
  for (const auto& entry : all) {
    const bool accepted = std::any_of(taken.begin(), taken.end(), [&entry](const auto& other) {
      return other.first == entry.first;
    });
    if (!accepted) {
      refused.push_back(entry);
    }
  }
  return refused;
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
  // A surface made again (a re-parenting, a full screen switch) may come at
  // another size: the view keeps the height it shows, as a resize does
  // (mitcad#1000); OCCT would keep the shorter side.
  // The first window keeps a framing made before it (settleCamera).
  const double shownHeight = (firstTime && !m_fitMade) ? 0.0 : visibleHeight();
  window->SetNativeHandle(occtDrawable());
  window->SetSize(viewWidth, viewHeight);
  m_view->SetWindow(window, glContext->RenderingContext());
  if (shownHeight > 0.0 && std::isfinite(shownHeight)) {
    setVisibleHeight(*m_view->Camera(), shownHeight);
  }

#ifdef __APPLE__
  QObject::disconnect(m_renderedContextCleanup);
  m_renderedContextCleanup = connect(context(), &QOpenGLContext::aboutToBeDestroyed, this,
      &OcctViewer::releaseRenderedSurfaceContext, Qt::DirectConnection);
#endif

  if (firstTime) {
    if (!m_fitMade) {
      // Until a fit or a new design's framing (mitcad#1000) is made: one
      // made before the view had its window (settleCamera) stays.
      m_view->SetSize(250.0); // an empty scene shows about 250 mm
    }
    m_context->Display(m_orientationCube, 0, 0, false);
    updateGrid(true);
    const auto* renderer =
        reinterpret_cast<const char*>(context()->functions()->glGetString(GL_RENDERER));
    // The driver's version, for error reports (mitcad#61).
    const auto* version = reinterpret_cast<const char*>(context()->functions()->glGetString(GL_VERSION));
    m_glVersion = QString::fromLatin1(version != nullptr ? version : "");
    if (timingLogged()) {
      GLint maxSamples = 0;
      context()->functions()->glGetIntegerv(GL_MAX_SAMPLES, &maxSamples);
      const QScreen* display = screen();
      qDebug().noquote() << QStringLiteral("Viewport timing environment: renderer=%1; GL=%2; "
                                          "scale=%3; logical=%4x%5; backing=%6x%7; refresh=%8 Hz; "
                                          "GL_MAX_SAMPLES=%9; configured-msaa=%10; ray-aa=%11")
                                .arg(QString::fromLatin1(renderer != nullptr ? renderer : "unknown"), m_glVersion)
                                .arg(ratio).arg(width()).arg(height()).arg(viewWidth).arg(viewHeight)
                                .arg(display ? display->refreshRate() : 0).arg(maxSamples)
                                .arg(m_view->RenderingParams().NbMsaaSamples)
                                .arg(m_view->RenderingParams().IsAntialiasingEnabled ? 1 : 0);
    }
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
  // A window resized by dragging is resized in steps: one navigation.
  navigationStep();
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
  // The first frame after a window selection was applied (mitcad#670),
  // logged once the frame is done (after "frame").
  struct WindowShown {
    OcctViewer& viewer;
    ~WindowShown() {
      if (viewer.m_windowShowPending && viewer.m_windowClock.isValid()) {
        qDebug().noquote() << QStringLiteral("Timing window selection shown: %1 ms")
                                  .arg(static_cast<double>(viewer.m_windowClock.nsecsElapsed()) / 1.0e6, 0, 'f', 2);
        viewer.m_windowClock.invalidate();
      }
      viewer.m_windowShowPending = false;
    }
  } windowShown{*this};
  // The first frame after an open (logFirstFrame, mitcad#878), logged once
  // the frame is drawn.
  struct FirstFrame {
    OcctViewer& viewer;
    bool drawn = false;
    ~FirstFrame() {
      if (drawn && !viewer.m_firstFrameOf.isEmpty() && viewer.m_firstFrameClock.isValid()) {
        qInfo().noquote() << QStringLiteral("%1: first frame %2 ms after it began")
                                 .arg(viewer.m_firstFrameOf)
                                 .arg(viewer.m_firstFrameClock.elapsed());
        viewer.m_firstFrameOf.clear();
        viewer.m_firstFrameClock.invalidate();
      }
    }
  } firstFrame{*this};
  ScopedTiming timing("frame");
  if (m_view->Window()->NativeHandle() != occtDrawable()) {
    // Qt recreated its surface, e.g. after re-parenting: attach again.
    initializeGL();
    return;
  }
  ++m_framesDrawn;
  firstFrame.drawn = true;

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
  if (timingLogged()) {
    timing.setDetail(QStringLiteral("configured-msaa=%1; ray-aa=%2; scale=%3; backing=%4x%5; gpu-sync=%6")
                         .arg(m_view->RenderingParams().NbMsaaSamples)
                         .arg(m_view->RenderingParams().IsAntialiasingEnabled ? 1 : 0)
                         .arg(devicePixelRatioF()).arg(newSize.x()).arg(newSize.y())
                         .arg(qEnvironmentVariableIntValue("MITCAD_VIEWPORT_SYNC") != 0 ? 1 : 0));
  }
  if (newSize != oldSize) {
    // A resize keeps the framing of what the view shows (mitcad#1000): its
    // centre (OCCT keeps the camera's) and the height it shows. OCCT keeps
    // its scale, the shorter side, so a view that became taller than wide
    // on the way (a window being maximized or put to full screen) would
    // show another height afterwards.
    const double shownHeight = visibleHeight();
    window->SetSize(newSize.x(), newSize.y());
    m_view->MustBeResized();
    if (oldSize.x() > 0 && oldSize.y() > 0 && shownHeight > 0.0 && std::isfinite(shownHeight)) {
      setVisibleHeight(*m_view->Camera(), shownHeight);
    }
    m_view->Invalidate();
    // Logged once the view rests (logCamera): a dragged frame resizes in
    // many steps.
    m_resizeToLog = true;
    m_restTimer->start();
  }

  if (m_fitPending) {
    applyPendingFit();
  }

  // A 3D mouse moves the camera by the time since its last frame
  // (mitcad#951, view/SpaceMouseView.cpp).
  stepSpaceMouse();

  m_view->InvalidateImmediate();
  // An exception escaping a Qt event handler terminates the application, so
  // a failing view operation is logged and the frame skipped instead.
  try {
    FlushViewEvents(m_context, m_view, true);
  } catch (const Standard_Failure& failure) {
    qWarning() << "OCCT view event failed:" << failure.what();
  } catch (const std::exception& failure) {
    // A click on the cube turns the view here (mitcad#773).
    qWarning() << "OCCT view event failed:" << failure.what();
  }
  if (m_context->DetectedOwner() != m_hoveredOwner) {
    m_hoveredOwner = m_context->DetectedOwner();
    m_hoveredPoint.reset();
    emit hovered(itemFor(m_hoveredOwner).value_or(SelectionItem()));
  }
  // The point under the mouse on it (mitcad#417), once per move of the
  // mouse over the view: a frame drawn for anything else (a preview) after
  // the mouse left keeps the last detection, which is no hover any more.
  if (!m_hoveredOwner.IsNull() && m_hoverMoved) {
    m_hoverMoved = false;
    const std::optional<std::array<double, 3>> point = pickedPoint(m_hoveredOwner);
    if (point && point != m_hoveredPoint) {
      m_hoveredPoint = point;
      if (std::optional<SelectionItem> item = itemFor(m_hoveredOwner)) {
        item->at = point;
        emit hoverMoved(*item);
      }
    }
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
    // A wheel burst or a gesture lasts until the camera keeps still.
    m_navigationState.cameraMoved(m_navigationClock.elapsed());
  }
  // Whether this frame is part of a navigation, before what follows the
  // camera asks: the last frame after a release, an animation's end or the
  // quiet time ends it (navigationFinished).
  updateNavigation();
  emit viewChanged();
  // Optional diagnostic fence: include completion of this context's GPU
  // work in paint duration, excluding Qt composition and presentation.
  if (timingLogged() && qEnvironmentVariableIntValue("MITCAD_VIEWPORT_SYNC") != 0) {
    context()->functions()->glFinish();
  }
}

void OcctViewer::updateNavigation() {
  const qint64 now = m_navigationClock.elapsed();
  const ViewNavigation::Change change = m_navigationState.update(now, !ViewAnimation()->IsStopped());
  const int left = m_navigationState.quietLeft(now);
  if (left > 0) {
    m_navigationTimer->start(left);
  }
  if (change == ViewNavigation::Change::Started) {
    m_navigationFirstFrame = m_framesDrawn;
    m_navigationRepaints = 0;
    m_navigationRepainted.clear();
    m_navigationPrintOverlays = 0;
  }
  if (change == ViewNavigation::Change::Ended) {
    // For UI tests (app/COMMANDS.md, "Logs that UI tests read"): what the
    // navigation cost besides the view's own frames (mitcad#768).
    QString line = QStringLiteral("View navigation: %1 frame(s), %2 repaint(s) of widgets over the view")
                       .arg(m_framesDrawn - m_navigationFirstFrame)
                       .arg(m_navigationRepaints);
    if (!m_navigationRepainted.isEmpty()) {
      line += QStringLiteral(" (%1)").arg(m_navigationRepainted.join(QStringLiteral(", ")));
    }
    if (!m_loggedPrintOverlay.isEmpty() && m_loggedPrintOverlay != QStringLiteral("Print overlay: none")) {
      line += QStringLiteral(", Print Analysis overlay set %1 time(s)").arg(m_navigationPrintOverlays);
    }
    qDebug().noquote() << line;
    emit navigationFinished();
  }
}

void OcctViewer::navigationStep() {
  const qint64 now = m_navigationClock.elapsed();
  m_navigationState.stepped(now);
  m_navigationTimer->start(m_navigationState.quietLeft(now));
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

// Cube and F4 drags use separate synthetic flags, so custom F4 actions
// cannot replace the cube's orbit gesture.
Aspect_VKeyFlags OcctViewer::viewFlags(Qt::KeyboardModifiers modifiers) const {
  return m_f4Press ? kF4Drag : m_cubePress ? kCubeDrag : toVKeyFlags(modifiers);
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

void OcctViewer::setDragCursor(bool shown) {
  if (shown != m_dragCursor) {
    m_dragCursor = shown;
    updateCursor(false);
  }
}

void OcctViewer::updateCursor(bool log) {
  if (!m_sketchInput) {
    setCursor(m_dragCursor ? Qt::SizeAllCursor : Qt::ArrowCursor);
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

void OcctViewer::cancelPointerInteraction() {
  endScrollDrag();
  ResetViewInput();
  // ResetViewInput leaves pending pointer deltas and selection requests.
  // Drop those too, while keeping deliberate Fit, orientation and zoom
  // commands that a menu action may just have queued.
  for (const auto type : {AIS_ViewInputBufferType_UI, AIS_ViewInputBufferType_GL}) {
    AIS_ViewInputBuffer& input = ChangeInputBuffer(type);
    input.IsNewGesture = true;
    // Keep the tool until the next frame: an empty rubber-band tool
    // removes its rectangle, while a picking tool would leave it shown.
    input.Selection.Points.Clear();
    input.Selection.Scheme = AIS_SelectionScheme_UNKNOWN;
    input.Selection.ToApplyTool = false;
    input.Panning = {};
    input.OrbitRotation = {};
    input.ViewRotation = {};
    input.ZRotate = {};
    input.Dragging = {};
    input.Dragging.ToAbort = true;
  }
  m_cubePress = false;
  m_f4Press = false;
  m_panRemainder = {};
  m_cubeClick = false;
  m_windowPick = false;
  m_windowSelecting = false;
  m_orbitKey = false;
  m_navigationState.setButtons(false);
  if (m_dragItem) {
    endOccurrenceDrag(QPointF(), true);
  }
  requestFrame();
}

void OcctViewer::buttonsReleased() {
  // The navigation ends with the next frame, which applies the last move
  // (or, when no frame comes, shortly after).
  m_navigationState.setButtons(false);
  requestFrame();
  if (!m_navigationTimer->isActive()) {
    m_navigationTimer->start(kReleaseFrameMilliseconds);
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
    cancelPointerInteraction();
  }
  if (event->type() == QEvent::Leave && m_hoveredPoint) {
    // Out of the view (to a panel): the point is under the mouse no more.
    // A move still to be drawn is not dropped: posted input (a UI test in
    // a session without a cursor) gets a Leave right after each move,
    // before the frame that detects it.
    m_hoveredPoint.reset();
    emit hoverMoved(SelectionItem());
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
  // Any button held may orbit, pan or drag: a navigation until it goes up
  // (a click without a move leaves the camera as it was).
  m_navigationState.setButtons(event->buttons() != Qt::NoButton);
  updateNavigation();
  if (event->button() == Qt::RightButton) {
    m_rightPressPosition = event->position();
  }
  // Shift with a click widens a selection scope (mitcad#217).
  setShiftHeld(event->modifiers().testFlag(Qt::ShiftModifier), false);
  if (event->button() == Qt::LeftButton) {
    m_windowSelecting = false;
    const bool onCube = isOnOrientationCube(event->position());
    m_f4Press = !onCube && m_orbitKey &&
                navigationAction(m_navigation, NavigationInput::LeftButton, event->modifiers(), true) !=
                    NavigationAction::None;
    m_cubePress = onCube || m_f4Press;
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
    // A press on a component the joints move may drag it (mitcad#55), or
    // on a body that cannot be dragged say why (mitcad#150): the button is
    // not the view's then, so no window selection starts; a release
    // without a drag is passed on as a click.
    if (!m_cubePress && !m_cubeClick && event->modifiers() == Qt::NoModifier && m_occurrenceDrag.accepts) {
      const occ::handle<SelectMgr_EntityOwner> owner = m_context->DetectedOwner();
      const auto item = itemFor(owner);
      const auto grabbed = pickedWorldPoint(owner);
      const OccurrenceDragStart start =
          item && grabbed ? m_occurrenceDrag.accepts(*item) : OccurrenceDragStart::Select;
      if (start != OccurrenceDragStart::Select) {
        m_dragItem = item;
        m_dragGrab = *grabbed;
        m_dragging = false;
        m_dragRefused = start == OccurrenceDragStart::Refuse;
        return;
      }
    }
  }
  if (UpdateMouseButtons(toDevicePixels(event->position()), viewButtons(event->buttons()),
                         viewFlags(event->modifiers()), false)) {
    requestFrame();
  }
}

std::optional<gp_Pnt> OcctViewer::pickedWorldPoint(const occ::handle<SelectMgr_EntityOwner>& owner) const {
  if (owner.IsNull()) {
    return std::nullopt;
  }
  const occ::handle<StdSelect_ViewerSelector3d>& selector = m_context->MainSelector();
  for (int rank = 1; rank <= selector->NbPicked(); ++rank) {
    if (selector->Picked(rank) == owner) {
      return selector->PickedPoint(rank);
    }
  }
  return std::nullopt;
}

std::optional<gp_Pnt> OcctViewer::viewPlanePoint(const QPointF& position, const gp_Pnt& through) const {
  const NCollection_Vec2<int> pixel = toDevicePixels(position);
  double x = 0, y = 0, z = 0, dx = 0, dy = 0, dz = 0;
  m_view->ConvertWithProj(pixel.x(), pixel.y(), x, y, z, dx, dy, dz);
  const gp_XYZ from(x, y, z);
  const gp_XYZ direction(dx, dy, dz);
  const gp_XYZ normal = m_view->Camera()->Direction().XYZ();
  const double along = direction.Dot(normal);
  if (std::abs(along) < 1e-12) {
    return std::nullopt;
  }
  return gp_Pnt(from + direction * ((through.XYZ() - from).Dot(normal) / along));
}

void OcctViewer::endOccurrenceDrag(const QPointF& position, bool cancelled) {
  const bool dragging = m_dragging && !m_dragRefused;
  m_dragItem.reset();
  m_dragging = false;
  m_dragRefused = false;
  if (!dragging || !m_occurrenceDrag.finished) {
    return;
  }
  const auto target = cancelled ? std::nullopt : viewPlanePoint(position, m_dragGrab);
  m_occurrenceDrag.finished(target.value_or(m_dragGrab), !target);
}

void OcctViewer::mouseReleaseEvent(QMouseEvent* event) {
  QOpenGLWidget::mouseReleaseEvent(event);
  if (event->buttons() == Qt::NoButton && m_navigationState.buttons()) {
    buttonsReleased();
  }
  const bool rightClick = event->button() == Qt::RightButton &&
                          (PressedMouseButtons() & Aspect_VKeyMouse_RightButton) != 0;
  const bool cube = m_cubePress && event->button() == Qt::LeftButton;
  if (m_sketchInput && event->button() == Qt::LeftButton && !cube) {
    return;
  }
  if (m_dragItem && event->button() == Qt::LeftButton) {
    if (m_dragging) {
      endOccurrenceDrag(event->position(), false);
      return;
    }
    // No drag: the press and its release reach the view as a click.
    m_dragItem.reset();
    m_dragRefused = false;
    m_windowPick = false;
    m_clickPosition = m_pressPosition;
    const Aspect_VKeyFlags flags = viewFlags(m_pressModifiers);
    UpdateMouseButtons(toDevicePixels(m_pressPosition), Aspect_VKeyMouse_LeftButton, flags, false);
    if (UpdateMouseButtons(toDevicePixels(m_pressPosition), Aspect_VKeyMouse_NONE, flags, false)) {
      requestFrame();
    }
    return;
  }
  if (event->button() == Qt::LeftButton) {
    const QPointF moved = event->position() - m_pressPosition;
    m_windowPick = std::hypot(moved.x(), moved.y()) > kDragPixels;
    if (m_windowPick) {
      m_cubeClick = false;
      // Dragged to the left, a window also takes what it crosses.
      m_context->MainSelector()->AllowOverlapDetection(moved.x() < 0 && !cube);
      // The window's rectangle decides which part of a body edge it took
      // (mitcad#217) until the selection is reported.
      m_windowRect = QRectF(m_pressPosition, event->position()).normalized();
      m_windowCrossing = moved.x() < 0;
      m_windowSelecting = !cube;
      if (timingLogged()) {
        m_windowClock.start();
        m_windowShowPending = false;
      }
    } else {
      setShiftHeld(event->modifiers().testFlag(Qt::ShiftModifier), false);
      m_clickPosition = event->position();
    }
  }
  const Aspect_VKeyFlags flags = viewFlags(event->modifiers());
  if (cube) {
    m_cubePress = false;
    m_f4Press = false;
  }
  if (UpdateMouseButtons(toDevicePixels(event->position()), viewButtons(event->buttons()), flags,
                         false)) {
    requestFrame();
  }
  // Release the view's gesture before a context menu can take input,
  // including when a caller connects directly to these signals.
  if (rightClick) {
    const QPointF moved = event->position() - m_rightPressPosition;
    if (std::hypot(moved.x(), moved.y()) <= kDragPixels) {
      if (isOnOrientationCube(event->position())) {
        m_context->ClearDetected(false);
        emit orientationCubeMenuRequested(event->globalPosition().toPoint());
      } else {
        // The context menu is for what a Shift+click would take in a
        // selection scope (mitcad#217): a face in sketch mode offers Project.
        const bool widen = m_scoped && !m_shiftHeld && m_expandedFilter != m_filter;
        m_contextPick = true;
        if (widen) {
          activateAllPicking();
        }
        const NCollection_Vec2<int> pixel = toDevicePixels(event->position());
        m_context->MoveTo(pixel.x(), pixel.y(), m_view, false);
        const auto item = itemFor(m_context->DetectedOwner());
        const SelectionItem target = item && accepts(m_context->DetectedOwner()) ? *item : SelectionItem();
        m_contextPick = false;
        if (widen) {
          m_context->ClearDetected(false);
          activateAllPicking();
        }
        emit contextMenuRequested(event->globalPosition().toPoint(), target);
      }
    }
  }
}

void OcctViewer::mouseMoveEvent(QMouseEvent* event) {
  QOpenGLWidget::mouseMoveEvent(event);
  m_hoverMoved = true; // the next frame says where the mouse is on the item
  if (m_scrollDragging) {
    endScrollDrag();
  }
  if (event->buttons() == Qt::NoButton) {
    // The hover with Shift shows what a widened click picks (mitcad#217).
    setShiftHeld(event->modifiers().testFlag(Qt::ShiftModifier), false);
  }
  // AppKit menu tracking and other controls may receive a release that
  // never reaches this widget. OCCT does not reconcile buttons on motion;
  // discard the old gesture before hover motion can continue its orbit.
  if (event->buttons() == Qt::NoButton &&
      (PressedMouseButtons() != Aspect_VKeyMouse_NONE || m_cubePress || m_dragItem)) {
    cancelPointerInteraction();
  }
  if (event->buttons() == Qt::NoButton && m_navigationState.buttons()) {
    buttonsReleased(); // a release this widget did not get
  }
  if (m_dragItem) {
    // A component dragged (mitcad#55): the owner moves it, the view only
    // follows; a refused one stays, and the owner says why (mitcad#150).
    const QPointF moved = event->position() - m_pressPosition;
    if (!m_dragging && std::hypot(moved.x(), moved.y()) > kDragPixels) {
      m_dragging = true;
      if (m_dragRefused) {
        if (m_occurrenceDrag.refused) {
          m_occurrenceDrag.refused(*m_dragItem);
        }
      } else if (m_occurrenceDrag.started) {
        m_occurrenceDrag.started(*m_dragItem, m_dragGrab);
      }
    }
    if (m_dragging && !m_dragRefused && m_occurrenceDrag.moved) {
      if (const auto target = viewPlanePoint(event->position(), m_dragGrab)) {
        m_occurrenceDrag.moved(*target);
      }
    }
    return;
  }
  if (UpdateMousePosition(toDevicePixels(event->position()), viewButtons(event->buttons()),
                          viewFlags(event->modifiers()), false)) {
    requestFrame();
  }
}

void OcctViewer::wheelEvent(QWheelEvent* event) {
  QOpenGLWidget::wheelEvent(event);
  if ((m_navigation.scheme == NavigationScheme::Trackpad ||
       m_navigation.scheme == NavigationScheme::TrackpadShift ||
       m_navigation.scheme == NavigationScheme::Custom) && isTrackpadScroll(event)) {
    trackpadScroll(event);
    return;
  }
  double steps = event->angleDelta().y() / 8.0;
  if (isTrackpadScroll(event)) {
    steps *= m_navigation.trackpadSpeed;
  }
  if (m_navigation.reverseZoom) {
    steps = -steps;
  }
  const bool zoomed = m_navigation.zoomToCursor
                          ? UpdateZoom(Aspect_ScrollDelta(toDevicePixels(event->position()), steps))
                          : UpdateZoom(Aspect_ScrollDelta(steps));
  if (zoomed) {
    navigationStep();
    requestFrame();
  }
}

// The trackpad schemes: a two-finger scroll pans, with the chosen orbit key
// (Alt or Shift) it orbits, with the other key or Ctrl (Cmd on macOS) it
// zooms. Pan and orbit are one drag of the left button (with keys no one
// presses, kScrollPan and kScrollOrbit) that
// lasts until the scroll pauses, momentum included: the view handles it as a
// mouse drag, about the same centre and with the orbit's Z lock, and no
// step between two frames is lost. Each event asks for a frame; it is not
// drawn here.
void OcctViewer::trackpadScroll(QWheelEvent* event) {
  event->accept();
  if (event->phase() == Qt::ScrollBegin) {
    endScrollDrag(); // a new gesture: the momentum of the last one ends
  }
  const QPointF delta = scrollPixels(event) * m_navigation.trackpadSpeed;
  if (delta.isNull()) {
    return;
  }
  const Qt::KeyboardModifiers keys = event->modifiers();
  const NavigationAction action = navigationAction(m_navigation, NavigationInput::TrackpadScroll, keys);
  ScrollMode mode = action == NavigationAction::Orbit ? ScrollMode::Orbit
                    : action == NavigationAction::Pan ? ScrollMode::Pan
                    : action == NavigationAction::Zoom ? ScrollMode::Zoom : ScrollMode::None;
  if (event->phase() == Qt::ScrollMomentum && m_scrollMode != ScrollMode::None) {
    mode = m_scrollMode; // the inertia goes on as it began, whatever the keys do now
  }
  if (mode != m_scrollMode) {
    endScrollDrag();
  }
  if (mode == ScrollMode::None) {
    return;
  }
  m_scrollMode = mode;
  m_scrollTimer->start();
  navigationStep();
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
    m_navigationState.setScrollDrag(true);
    m_scrollPoint = event->position() * ratio;
    UpdateMouseButtons(devicePixel(m_scrollPoint), Aspect_VKeyMouse_LeftButton, flags, false);
  }
  // A modest gain for the two-finger orbit; pan and mouse drags retain
  // their one-to-one motion. Apply it to momentum through the same path.
  const qreal gain = mode == ScrollMode::Orbit ? 1.3 : 1.0;
  m_scrollPoint += delta * ratio * gain;
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
  m_navigationState.setScrollDrag(false); // ends with the next frame or the quiet time
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
                            ? AIS_ViewController::UpdateZoom(Aspect_ScrollDelta(toDevicePixels(event->position()), steps))
                            : AIS_ViewController::UpdateZoom(Aspect_ScrollDelta(steps));
    if (zoomed) {
      cameraMoved();
      navigationStep();
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
  navigationStep();
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
  // A rendered image of another camera or size, or one that a camera
  // animation is about to move, would show the bodies where they no longer
  // are: this frame draws them shaded (mitcad#214).
  if (showsRenderedImage()) {
    const QString why = !ViewAnimation()->IsStopped() ? QStringLiteral("animation") : renderedImageStale();
    if (!why.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Render image dropped: %1").arg(why);
      setRenderedImage(QImage());
    }
  }
  // The camera has moved for this frame: the grid follows the zoom at once,
  // the cube's own perspective the direction (mitcad#774).
  updateGrid();
  updateCubeEye();
  const gp_Dir drawnFrom = m_view->Camera()->Direction();
  AIS_ViewController::handleViewRedraw(context, view);
  // A camera animation moved the camera as the frame was drawn: the cube's
  // perspective follows in the next one.
  if (m_cubeProjection == CubeProjection::Perspective && !m_view->Camera()->Direction().IsEqual(drawnFrom, 1e-9)) {
    updateCubeEye();
    requestFrame();
  }
  if (toAskNextFrame()) {
    requestFrame(); // keep animations such as the orientation cube running
  }
}

} // namespace mitcad
