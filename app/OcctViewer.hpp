// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <cstddef>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include <QColor>
#include <QElapsedTimer>
#include <QHash>
#include <QImage>
#include <QMargins>
#include <QMetaObject>
#include <QOpenGLWidget>
#include <QPoint>
#include <QRectF>
#include <QString>
#include <QStringList>
#include <QSurfaceFormat>
#include <QtGlobal>

#include <AIS_InteractiveContext.hxx>
#include <AIS_Shape.hxx>
#include <AIS_ViewController.hxx>
#include <AIS_ViewCube.hxx>
#include <Bnd_Box.hxx>
#include <Graphic3d_Camera.hxx>
#include <Graphic3d_Texture2D.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <SelectMgr_EntityOwner.hxx>
#include <SelectMgr_Filter.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <V3d_View.hxx>
#include <V3d_Viewer.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "LayoutGrid.hpp"
#include "framework/Appearances.hpp"
#include "framework/Selection.hpp"
#include "framework/SelectionScope.hpp"
#include "mitcad/geometry/shape.hpp"
#include "view/FirstBodiesFit.hpp"
#include "view/ImageReferences.hpp"
#include "view/MeasurementDrawing.hpp"
#include "view/ViewMatch.hpp"
#include "view/ViewNavigation.hpp"
#include "view/Navigation.hpp"

class QNativeGestureEvent;
class QTimer;
class QToolButton;
class QWheelEvent;

namespace mitcad {

#ifdef __APPLE__
namespace render {
class MacFrameLease;
class MacFrameDisplay;
struct FrameInfo;
struct Look;
}
#endif

class ViewCubeHomeButton;
class PrintOverlayLabels;
class MeasurementOverlay;
struct PrintOverlay;
struct ProxyMesh;

#ifdef Q_OS_MACOS
// The OpenGL format of every context Qt creates on macOS, the viewport's and
// the window's shared one (main() sets it as the default format): a core
// profile, which macOS only offers as a forward-compatible 4.1 context.
// Contexts of different profiles cannot share resources there.
QSurfaceFormat macSurfaceFormat();
#endif

// Identifies a sketch profile: (sketch feature uid, region key).
using ProfileKey = std::pair<std::string, std::string>;

// Whether two transformations are the same by value (TopLoc_Location's
// IsEqual compares its items, which differ for every new location).
bool sameTransformation(const gp_Trsf& a, const gp_Trsf& b);

// A body to show; its face names name what is picked on it.
struct BodyDisplay {
  enum class Style {
    Normal,
    New, // a body the previewed command makes: see-through blue, not pickable
    // A body a command picks through (mitcad#147): see-through in its own
    // colour, not pickable. setGhosts() draws whole occurrences so;
    // setActiveScope() draws inactive components with the same look, but
    // pickable (mitcad#420).
    Ghost,
    // A body the previewed command consumes as a tool, drawn where another
    // occurrence shows it (mitcad#561): see-through like the command's
    // tool preview (blue; red for a cut, CutTool), and pickable, so that
    // the body can be picked there too.
    Tool,
    CutTool,
  };
  std::string uid;
  std::shared_ptr<geometry::Shape> shape;
  Style style = Style::Normal;
  // Where an occurrence of a component puts the body (F6); identity in the
  // design's own coordinates. Picks on a placed body name the body's own
  // faces, edges and vertices, with the occurrence (U3).
  TopLoc_Location placement;
  // The occurrence that places the body (SelectionItem::occurrence): tells
  // repeated or placed bodies apart; empty in the root component.
  std::string occurrence;
  // Its appearance's colour (U4); the default grey when invalid.
  QColor color = QColor();
  // Its appearance's parameters for the rendered view (mitcad#46); an
  // empty id for the default look.
  Appearance appearance = Appearance();
  // Faces with appearances of their own (mitcad#53): their indices in the
  // shape (geometry::Shape's order), their colour and appearance.
  struct FaceLook {
    std::vector<int> faces;
    QColor color;
    Appearance appearance;
  };
  std::vector<FaceLook> faceLooks = {};
  // Owning component, shared by all its bodies and occurrences.
  std::string component;
  // Cut by a section's plane (Section Analysis, Hide Above Sketch): its
  // faces are drawn behind the section caps on its cut faces (mitcad#784).
  bool sectioned = false;
  // The shape its clip cut, while `sectioned`: the caps are the clip's
  // faces in the plane that this shape has not there (a feature preview's
  // body, or the body at the marker; mitcad#943).
  std::shared_ptr<geometry::Shape> uncut = nullptr;
};

// A sketch curve or point in model space.
struct SketchEntityDisplay {
  QString sketch;
  QString id;       // c3, p4
  QString geometry; // line, circle, arc, ellipse, point, ...
  TopoDS_Shape shape;
  bool construction = false;
  // How sketch mode draws it (U2): fully constrained geometry darker (light
  // on a dark background, mitcad#253), centre lines dash-dotted, projected
  // geometry purple.
  bool constrained = false;
  bool centerline = false;
  bool reference = false;
  // Shown only while a command uses its sketch (mitcad#546): drawn a little
  // lighter, as it hides again when the command ends.
  bool temporary = false;
  // The occurrence that places the sketch when it is a component's (the
  // shape is placed already); picks carry it.
  QString occurrence = QString();
  // What the shape is made from (the entity's solved geometry, the sketch
  // frame, the style and the placement): an entity shown with the same
  // signature keeps its display (setSketchEntities). Empty: always new.
  QString signature = QString();
};

// An origin or construction plane, axis or point.
struct DatumDisplay {
  QString uid;
  SelectKind kind = SelectKind::Plane;
  TopoDS_Shape shape;            // placed already
  QString occurrence = QString(); // as for sketch entities
};

// A selected item drawn in its input's colour.
struct HighlightDisplay {
  SelectionItem item;
  TopoDS_Shape shape;
  QColor color;
  // The joint highlight's (mitcad#523): a highlight that takes no clicks
  // (a click goes to what lies under it), and a light one, a faint tint
  // of a whole occurrence drawn under the others.
  bool pickable = true;
  bool light = false;
  // Original topology for a face/edge overlay's boundary picks, including
  // while a command preview replaces the displayed body.
  std::shared_ptr<geometry::Shape> body = nullptr;
  bool throughSolid = false; // feature inspection: boundaries remain visible behind solids
  // A feature's inputs where they were (mitcad#957): the shape is lines (a
  // Fillet's picked edges), drawn as edges in the colour whatever the
  // item's kind. With throughSolid, dashed where a solid hides them.
  bool inputLines = false;
  // Diagnostic alternatives for whole-body overlay cost (mitcad#770).
  // Production callers keep the normal transparent, bounded presentation.
  bool opaqueWholeBody = false;
  bool wholeBodyBoundaries = true;
};

// How bodies are drawn (U5).
enum class VisualStyle {
  Shaded,               // faces only
  ShadedVisibleEdges,   // faces and the edges in sight (the default)
  ShadedHiddenEdges,    // also the edges behind faces, dashed
  Wireframe,            // edges only
  WireframeHiddenEdges, // edges in sight, those behind faces dashed
};

// How the orientation cube is projected (mitcad#774): as the view is
// (the default), always orthographic, or always in a perspective of its own.
enum class CubeProjection { FollowView, Orthographic, Perspective };
// Its id in the settings and the logs: "follow-view", "orthographic" or
// "perspective".
QString cubeProjectionId(CubeProjection projection);

// What a left drag without keys that starts on a body does (mitcad#150):
// the view's window selection, the owner's drag, or neither (refused: the
// owner says why).
enum class OccurrenceDragStart { Select, Drag, Refuse };

// Dragging components in the view (mitcad#55): a left drag without keys
// that starts on a body `accepts` takes is the owner's instead of a window
// selection (a click there still picks); one it refuses is neither, and
// `refused` is called once the pointer has moved. Points are in the
// design's coordinates: where the press hit the body, and where the
// pointer is on the plane through it that faces the viewer.
struct OccurrenceDrag {
  std::function<OccurrenceDragStart(const SelectionItem&)> accepts;
  std::function<void(const SelectionItem&)> refused;
  std::function<void(const SelectionItem&, const gp_Pnt&)> started;
  std::function<void(const gp_Pnt&)> moved;
  // `cancelled`: a dialog took the mouse (P7), or the pointer left the plane.
  std::function<void(const gp_Pnt&, bool cancelled)> finished;
};

// A camera that can be saved (named views): in design coordinates, mm.
struct CameraState {
  gp_Pnt eye;
  gp_Pnt target;
  gp_Dir up = gp_Dir(0, 0, 1);
  bool perspective = false;
  double height = 250.0; // of the view at the target
};

// 3D view: Qt owns the OpenGL context and the widget's framebuffer object,
// OCCT renders the scene into it.
//
// Navigation by default: middle drag pans, Shift + middle drag orbits and
// the wheel zooms about the cursor (setNavigation offers the other
// schemes). Left click picks, left drag picks with a window (left to right:
// what lies inside; right to left: what it touches too). Picks are reported
// as model names (SelectionItem); the view itself keeps no selection, the
// owner draws it with setHighlights().
//
// The orientation cube (upper right) turns the view when its faces, edges
// and corners are clicked and orbits when it is dragged; the house above
// it goes home, a right click on it opens the owner's cube menu.
class OcctViewer : public QOpenGLWidget, public AIS_ViewController {
  Q_OBJECT

public:
  // How a command preview's tool body is drawn.
  enum class PreviewStyle {
    Add,    // blue: a new body or material a Join adds
    Remove, // red: material a Cut removes
    Failed, // red outline: the feature fails
    Datum,  // blue, nearly opaque, over picked references: a construction
            // feature's plane, axis or point (mitcad#446)
    ExtendedTool, // orange, see-through: a split's or trim's tool as the
                  // feature extends it (mitcad#712)
  };

  explicit OcctViewer(QWidget* parent = nullptr);
  ~OcctViewer() override;

  // Display settings (U5).
  void setVisualStyle(VisualStyle style);
  void setComponentColors(bool on);
  VisualStyle visualStyle() const { return m_visualStyle; }
  void setNavigation(const NavigationSettings& settings);
  bool UpdateZoom(const Aspect_ScrollDelta& delta) override;
  const NavigationSettings& navigation() const { return m_navigation; }
  // A 3D mouse's motion (mitcad#951, view/SpaceMouseView.cpp): the camera
  // moves by these rates at each frame (the time between the frames)
  // until they are zero, in the 3D mouse's Navigation mode (turning about
  // the mouse orbit's centre, or about the eye in Fly) and with its Lock
  // horizon (mitcad#978). Zero rates end the motion and log "3D mouse
  // motion: …".
  void setSpaceMouseRates(const SpaceMouseRates& rates);
  // The background: a vertical gradient from top to bottom, or one colour.
  void setBackground(const QColor& top, const QColor& bottom, bool gradient);
  // The layout grid on the grid plane (XY, or the sketch plane): `step`
  // mm between lines, or a step that follows the zoom when 0.
  void setGrid(bool shown, double step);
  void setGridPlane(const gp_Ax3& plane);
  // The grid's minor step now (mm), shown or not: what sketch points snap to.
  double gridStep() const;
  // The faces where Section Analysis or Hide Above Sketch cuts the bodies
  // (caps), drawn over the cut and hatched along `plane`'s axes. One group
  // per colour (mitcad#382: the timeline colour of the bodies' component; an
  // invalid colour is the default one), each hatched in a darker shade of
  // its own; none clears them.
  struct SectionCap {
    TopoDS_Shape faces;
    QColor color = QColor();
  };
  void setSectionCaps(const std::vector<SectionCap>& caps, const gp_Ax3& plane);
  // Cosmetic threads: rings on the threaded faces, drawn over the bodies
  // and not pickable; null clears them.
  void setThreads(const TopoDS_Shape& rings);
  // Print Analysis (mitcad#620, view/PrintOverlay.cpp): the overhang and
  // thin-wall triangles drawn over the bodies in their colours, not
  // pickable, a marker and a label at the thinnest point and the legend in
  // the lower left corner; an empty overlay clears them. Logs "Print
  // overlay: …" when it changes.
  void setPrintOverlay(const PrintOverlay& overlay);
  // Show Build Volume (mitcad#640, view/BuildVolume.cpp): the design
  // printer's bed and printable height as lines, and the boxes of the bodies
  // that do not fit in `warning`; not pickable. Null shapes clear them.
  void setBuildVolume(const TopoDS_Shape& frame, const TopoDS_Shape& outside, const QColor& warning);
  // Image references (mitcad#782, view/ImageReferenceView.cpp): each image
  // of `shown` drawn on its plane by its corners, see-through by its
  // opacity, hidden behind the bodies or, `through`, over them; never
  // picked. The pictures come by their digest from `images` (one missing
  // is not drawn). Logs "Image references drawn: …" when it changes.
  void setImageReferences(const std::vector<ImageReferenceShown>& shown, const QHash<QString, QImage>& images);
  // Annotations (mitcad#649, mitcad#702; view/MeasurementOverlay.cpp): the
  // shown ones drawn in their planes (MeasurementOverlay), in their colours
  // over the halo tone; logs "Annotations drawn: …" when that changes and
  // their labels' and lines' places when those change.
  void setMeasurements(const std::vector<MeasurementDisplay>& measurements);
  const std::vector<MeasurementDisplay>& measurements() const { return m_measurements; }
  // For UI tests: "Annotation label <name> at x,y" and "Annotation line
  // <name> at x,y" in the window's coordinates, when they changed since
  // last logged; after setMeasurements and whenever the view's size or the
  // camera moved them (MeasurementOverlay::follow).
  void logAnnotationPlaces();
  // Annotations in the view (mitcad#702, view/MeasurementOverlay.cpp):
  // their texts and dimension lines take the mouse where they are drawn
  // (clicks elsewhere reach the model). A click selects one, a drag of its
  // text or its line moves it (in millimetres along its plane's axes), a
  // double-click opens it, a right click asks for its menu.
  struct AnnotationActions {
    // Whether they take the mouse now: not while a command takes picks or
    // a sketch is edited.
    std::function<bool()> active;
    std::function<void(const QString& name)> clicked;
    std::function<void(const QString& name)> opened;
    std::function<void(const QString& name, const QPoint& global)> menu;
    std::function<void(const QString& name, bool line, double alongX, double alongY)> moved;
  };
  void setAnnotationActions(AnnotationActions actions) { m_annotationActions = std::move(actions); }
  const AnnotationActions& annotationActions() const { return m_annotationActions; }
  // The annotations drawn highlighted (rows selected in the Annotations
  // window).
  void setHighlightedAnnotations(const QStringList& names);
  const QStringList& highlightedAnnotations() const { return m_highlightedAnnotations; }
  // The view's own cursor again (after an annotation's hand).
  void restoreCursor() { updateCursor(false); }
  // A snapshot of the view (mitcad#649, view/Snapshot.cpp): the bodies as
  // the view shows them rendered offscreen at `size` (OCCT's ToPixMap;
  // the view's aspect for a sharp picture of its contents), without the
  // orientation cube, the background left out (`transparent`: an alpha
  // channel, worked out from a render over black and one over white) and
  // the shown measurements drawn over it at the picture's scale. Null when
  // the view cannot render one. Only while no model job runs.
  // `group`: only the shown annotations of that group (mitcad#702); all
  // shown ones when empty.
  QImage snapshot(const QSize& size, bool transparent, bool withMeasurements, const QString& group = QString());

  // The camera (named views, U5).
  CameraState camera() const;
  // The camera after a pending animation/fit, without applying either.
  CameraState pendingCamera() const;
  // Moves the camera there, smoothly when the navigation settings say so.
  void setCamera(const CameraState& camera);
  void setPerspective(bool perspective);
  bool isPerspective() const;
  // The orientation cube's projection (mitcad#774); its size and place in
  // the corner are the same in each. Logs "Orientation cube projection …"
  // and the cube's places and outline again.
  void setCubeProjection(CubeProjection projection);
  CubeProjection cubeProjection() const { return m_cubeProjection; }
  // Looks along `direction` (from the eye to the model) with `up` upwards,
  // fitted, smoothly when the navigation settings say so (standard views,
  // the orientation cube's home).
  void turnTo(const gp_Dir& direction, const gp_Dir& up);
  // The home view: the document's (setHome) or the isometric one, fitted.
  void goHome();
  void setHome(const std::optional<CameraState>& home) { m_home = home; }
  // The orientation cube's corner of the view (widget pixels): the square
  // its arrows and the home button stay in, and its centre's distance from
  // the corner.
  static constexpr int kCubeArea = 190;
  static constexpr int kCubeOffset = 100;
  // The part of the view that is free for its own overlays (the orientation
  // cube, its arrows and the home button): the widget less these margins.
  // Floating chrome lays cards over the view and keeps these overlays out
  // from under them; the picking of the cube follows. Logs "View overlay
  // insets" when it changes.
  void setOverlayInsets(const QMargins& insets);
  // AppKit provides the general view controls in its native toolbar.
  void setNativeControls(bool native);
  QMargins overlayInsets() const { return m_overlayInsets; }
  // The width that the sketch's badge takes at the bottom left, margin
  // included (0: none), so that the floating status pill keeps clear of it.
  void setBadgeWidth(int width);
  int badgeWidth() const { return m_badgeWidth; }
  // Parts of the view that panels float over (widget pixels): the places
  // that logs and tests offer for clicking (picks, the datum planes) avoid
  // them, and the view itself is not picked there.
  void setCoveredRects(const std::vector<QRect>& rects);
  bool isCovered(const QPointF& position) const;
  // The value boxes of a command's handles (mitcad#656, widget pixels):
  // a click there edits a value, so they count as covered too.
  void setHandleBoxes(const std::vector<QRect>& boxes) { m_handleBoxes = boxes; }
  // The panels floating over the view and the cards at its top (widget
  // pixels): what the handles' value boxes keep out of (mitcad#656).
  std::vector<QRect> coveredRects() const {
    std::vector<QRect> all = m_covered;
    all.insert(all.end(), m_topCards.begin(), m_topCards.end());
    return all;
  }
  // The cards floating at the top of the view (mitcad#425: the title card and
  // the toolbar's group cards) and the bottom of their rows (widget
  // pixels; 0: none): the free area starts below them, and the view is not
  // picked inside a card's own rectangle, but is between the cards.
  void setTopCards(const std::vector<QRect>& cards, int bottom);
  // The cube's area in widget pixels, with the insets applied.
  QRect overlayCubeArea() const { return cubeArea().toRect(); }
  // Whether a point of the widget is on the orientation cube.
  bool isOnOrientationCube(const QPointF& position);
  // A left press on the cube (or its drag) is in progress: sketch mode
  // leaves the mouse to the view.
  bool cubeInteraction() const { return m_cubePress; }
  // A frame is being drawn: the view handles its events (picks) meanwhile,
  // and nothing may compute the model then (P7, MainWindow::runJob).
  bool isPainting() const { return m_painting; }

  // Replaces the bodies, keyed by their uids (F2.b0).
  void setBodies(const std::vector<BodyDisplay>& bodies);
  // A design is installed: the first design bodies setBodies() brings into
  // an empty view are fitted, once (mitcad#467; view/FirstBodiesFit.hpp),
  // not the bodies shown again after the user hid every one.
  void expectFirstBodies() { m_firstFit.arm(); }
  // The bodies shown (setBodies), without command previews; bodies that
  // setGhosts() draws as ghosts are among them, as given.
  std::vector<BodyDisplay> shownBodies() const;
  // Occurrences (paths of uids from the root, `O1/O4`) whose bodies, and
  // the bodies of the occurrences inside them, are drawn as ghosts
  // (BodyDisplay::Style::Ghost) instead of normally, now and by later
  // setBodies() calls, until the list changes (mitcad#147: a command picks
  // through them). Highlights of items in them are not pickable either, so
  // a click goes to what lies behind. Logs "View ghosts: ..." when the
  // list changes.
  void setGhosts(const QStringList& occurrences);
  // What the view draws normally while a component other than the root is
  // active (mitcad#420): the bodies, sketches, profiles and construction
  // items of `occurrences` (the active component's occurrences, paths from
  // the root, `O1/O4`) and of the occurrences inside them, and the items
  // whose owners `normal` names (the sketch being edited, the origin's
  // datums, which are the active component's own). Everything else is an
  // inactive ghost: bodies see-through in their own colour (as setGhosts()
  // draws them), lines, points and planes pale. Unlike setGhosts()' ghosts
  // they stay pickable as references, with the hover and highlights of any
  // item (after the active component's own: setActivePickPriority()); a
  // body that a command ghosts too is drawn once, as the command's
  // ghost. Not `on` (the root active): everything is drawn normally.
  struct ActiveScope {
    bool on = false;
    QStringList occurrences;
    QStringList normal;
    QString name; // the active component's occurrences, for the log
    bool operator==(const ActiveScope& other) const {
      return on == other.on && occurrences == other.occurrences && normal == other.normal && name == other.name;
    }
  };
  // Logs "View inactive ghosts: <n> bodies (active <name>)" or "View
  // inactive ghosts: none" when that changes.
  void setActiveScope(const ActiveScope& scope);
  // Preferences' Ghost Inactive Components (`view/ghostInactive`, default
  // on): off draws everything normally whatever the scope.
  void setGhostInactive(bool on);
  // Preferences' Pick the Active Component First (`view/activePickPriority`,
  // default on; mitcad#420): while the scope is on, a click, a command's
  // pick and the hover take the nearest item of the active component under
  // the cursor before nearer items of other components (PickContext.hpp's
  // setPickPriority); where none of its items is under the cursor, the
  // others are taken as usual. Logs "Pick priority: active <name>" when a
  // click took such an item before a nearer other one.
  void setActivePickPriority(bool on);
  // While a placement input takes picks (InputDef::places, mitcad#656):
  // origin and construction planes come behind the bodies, sketches and
  // other items under the cursor (PickContext.hpp's setPickLast), so that
  // a shaft or a face an origin plane runs through is picked rather than
  // the plane in front of it; a plane where nothing else is still is.
  void setPlanesBehind(bool on) { m_planesBehind = on; }
  // While an extent's object takes picks (InputDef::planesByDepth,
  // mitcad#762): planes are never preferred as the active component's
  // (setActivePickPriority), so a face of another component in front of an
  // origin plane is picked; otherwise the order by depth stays.
  void setPlanesByDepth(bool on) { m_planesByDepth = on; }
  // While an input puts sketches behind the bodies (InputDef::sketchesBehind,
  // mitcad#869: Measure's Faces, Edges and Vertices): a sketch's curves and
  // points come after the bodies' faces, edges and vertices under the cursor
  // (PickContext.hpp's setPickYield), so that a sketch point on a body's
  // corner or a sketch line on its edge gives the body's; a sketch where no
  // body is under the cursor still is picked. The curves and points of
  // `kept` (the sketch that sketch mode edits) keep their place.
  void setSketchesBehind(bool on, const QString& kept = QString()) {
    m_sketchesBehind = on;
    m_sketchKept = on ? kept : QString();
  }

  // The rendered view (docs/rendering.md): the image fills the view
  // (stretched; a frame that refines is smaller) over the bodies, which
  // keep their depth and stay pickable; their edges and the layout grid
  // are not drawn; the orientation cube, sketches, datums, highlights and
  // previews are drawn over it, hidden by the bodies as in the shaded view.
  // A null image draws the bodies again. While the rendered mode is on
  // (setRenderedMode), the bodies are in a layer of their own under the
  // image's. The image is of the camera and size of the view when it is
  // set: once either changes, the view draws the bodies again before its
  // next frame (no image of an old camera over a moved view, mitcad#214).
  void setRenderedImage(const QImage& image, const QString& timingDetail = QString());
#ifdef __APPLE__
  void setRenderedSurface(std::shared_ptr<render::MacFrameLease> frame, const render::FrameInfo& info,
                          const QColor& top, const QColor& bottom, const render::Look& look);
#endif
  void setRenderedMode(bool on);
  bool showsRenderedImage() const {
#ifdef __APPLE__
    if (m_renderedSurface) return true;
#endif
    return !m_renderedImage.isNull();
  }
  // The camera the view draws with, for the renderer to follow.
  const occ::handle<Graphic3d_Camera>& viewCamera() const { return m_view->Camera(); }
  // Whether the user is navigating the view (a button held, a wheel burst
  // or gesture that has not kept still for a moment, a camera animation;
  // ViewNavigation), as of the last frame drawn; navigationFinished() when
  // it ends (mitcad#214).
  bool isNavigating() const { return m_navigationState.active(); }
  // The background's colours, top and bottom (the same for one colour).
  void backgroundColors(QColor& top, QColor& bottom) const;
  // Whether the background is dark (what is drawn over it needs light lines).
  bool backgroundDark() const { return m_backgroundDark; }
  // The tone of the sketch halos (mitcad#302) as an sRGB colour: dark on a
  // dark background, light on a light one. The geometry's halos and the
  // dimensions' (painted by `SketchOverlay`, mitcad#633) take it from here
  // so that they cannot drift apart.
  QColor sketchHaloColor() const;
  // Replaces the visible sketch profiles (planar faces, placed already);
  // `occurrences` are those that place the sketches of components.
  void setProfiles(const std::vector<std::pair<ProfileKey, TopoDS_Shape>>& profiles,
                   const std::map<std::string, QString>& occurrences = {});
  // Remove only this sketch's filled regions while its geometry is dragged.
  void removeSketchProfiles(const QString& sketch);
  std::size_t profileCount() const { return m_profiles.size(); }
  void setSketchEntities(const std::vector<SketchEntityDisplay>& entities);
  void setDatums(const std::vector<DatumDisplay>& datums);
  void setHighlights(const std::vector<HighlightDisplay>& highlights);
  // Semi-transparent preview of the active command; null clears it.
  void setPreview(const TopoDS_Shape& shape, PreviewStyle style = PreviewStyle::Add);
  // A provisional drag presentation (mitcad#636, framework/DragProxy.hpp):
  // the mesh at each placement, see-through amber with dashed edges and
  // not pickable, standing in for the command preview's tool, which is
  // hidden meanwhile (setPreview keeps it for when this is cleared). Null
  // clears it. Drawn from the mesh only: nothing is read from the model.
  void setProvisional(const std::shared_ptr<const ProxyMesh>& mesh, const std::vector<gp_Trsf>& placements);
  bool provisionalShown() const { return !m_provisional.IsNull(); }
  // Frames drawn so far (paintGL), for the drag timings' logs (mitcad#636):
  // what was submitted, not what the screen showed.
  quint64 framesDrawn() const { return m_framesDrawn; }
  // Logs "<what>: first frame <ms> ms after it began" once the next frame
  // is drawn, the time taken from `started` (an open, mitcad#878): what an
  // open costs until the design shows. A later call replaces a pending one.
  void logFirstFrame(const QString& what, const QElapsedTimer& started);
  // Outline of the sketch shape being drawn; null clears it.
  void setSketchPreview(const TopoDS_Shape& shape);

  // What can be picked: kinds, and optionally a finer test.
  void setPickFilter(SelectFilter filter, std::function<bool(const SelectionItem&)> test = {});
  // A model job owns the document and its geometry. Keep drawing cached
  // presentations, but do not resolve owners or call command predicates.
  void setPickingSuspended(bool suspended);
  // A selection scope (mitcad#217, framework/SelectionScope.hpp): `kinds`
  // can be picked, `expanded` (which holds them) while Shift is held over a
  // click; `test` decides each item by how it is picked: a click or a
  // window, Shift, and for body edges which part of the edge was hit (in
  // sight or behind the faces of the shown bodies, seen from the camera
  // now). Shift pressed or released picks again under the pointer, so the
  // hover shows what the click would select.
  using PickTest = std::function<bool(const SelectionItem&, const PickCase&)>;
  void setPickScope(SelectFilter kinds, SelectFilter expanded, PickTest test);
  bool shiftHeld() const { return m_shiftHeld; }
  // The last window selection in widget pixels, and whether it also took
  // what it crosses (dragged to the left).
  QRectF lastWindow() const { return m_windowRect; }
  bool lastWindowCrossing() const { return m_windowCrossing; }
  // With MITCAD_LOG_TIMING (mitcad#670): the main window has applied the
  // last window selection; logs the time since its release ("window
  // selection applied") and, at the end of the next frame, until it shows
  // ("window selection shown").
  void windowSelectionApplied();
  // For UI tests (MITCAD_LOG_EDGE_PLACES): per edge of the shown bodies, a place
  // in the widget where a click hits that edge first among what the view
  // offers now, one where the edge is in sight and one where it is behind
  // faces (when it has such parts in the view).
  struct EdgePlace {
    SelectionItem item;
    QPoint at;
    EdgeSight sight = EdgeSight::Visible;
  };
  std::vector<EdgePlace> edgePlaces();
  // Whether a point of the design is in sight, not behind a face of a shown
  // body seen from the camera now: the test a click on a body edge makes
  // (mitcad#217). Sketch Dimension takes body geometry out of the sketch
  // plane only where it is in sight (mitcad#866).
  bool inSight(const gp_Pnt& point) const { return !occluded(point); }
  // Dragging components (mitcad#55).
  void setOccurrenceDrag(OccurrenceDrag drag) { m_occurrenceDrag = std::move(drag); }
  // The move cursor over a body a drag would move (mitcad#150).
  void setDragCursor(bool shown);
  // The point under a widget position on the plane through `through` that
  // faces the viewer; none when the view looks along it.
  std::optional<gp_Pnt> viewPlanePoint(const QPointF& position, const gp_Pnt& through) const;
  SelectFilter pickFilter() const { return m_filter; }
  // Colour of the highlight under the mouse: see-through and unlit, with
  // the face's boundary, edges and vertices opaque, hidden by nearer
  // geometry (mitcad#149; as the selections of setHighlights()).
  void setHoverColor(const QColor& color);
  // More of the hover (mitcad#290): the rest of the tangent chain of the
  // edge or face under the mouse, drawn as the hover draws that one (edges
  // in a compound) and not pickable; null takes it away.
  void setHoverChain(const TopoDS_Shape& shape);

  // While a sketch tool draws, the left button is the tool's (sketch mode
  // handles it), not the view's: no picking or window selection. The
  // pointer is then the precision cursor (framework/Cursors.hpp) with the
  // tool's icon `toolIcon`; a changed device pixel ratio rebuilds it.
  void setSketchInput(bool enabled, const QString& toolIcon = QString());
  // While a sketch tool's pick snaps to a point: the cursor shows a square.
  void setSketchSnapping(bool snapping);
  // The sketch plane: sketchPlanePoint() gives points in its frame.
  void setSketchPlane(const gp_Ax3& plane) { m_sketchPlane = plane; }
  // The point of the sketch plane under a widget position (logical pixels).
  std::optional<gp_Pnt2d> sketchPlanePoint(const QPointF& position) const;

  // Turns to look at the sketch plane; restoreCamera() returns to the view
  // saved when `saveCamera` was set. Entering a sketch (`saveCamera`) shows
  // the sketch's origin in the middle of the free area while the view shows
  // nothing, or only sketches with the origin in the free area; else, and
  // for Look At, what that middle showed stays there, on the plane
  // (mitcad#738). `face`, the faces the sketch lies on (placed in the
  // design; null for a plane), keeps a face seen at a steep angle in view
  // as a sketch on it starts (mitcad#772): when what the middle showed is,
  // on the plane, on none of them, the middle of their box is shown there
  // instead. Logged "Sketch plane centred on the origin: origin at x,y; free
  // area centre at x,y" ("on the view's middle", "on the sketch's face"),
  // in window coordinates.
  void lookAtSketchPlane(bool saveCamera = true, const TopoDS_Shape& face = TopoDS_Shape());
  void restoreCamera();
  void fitAll();
  // Fits the view to a box (model coordinates) as Fit fits everything:
  // along the current view direction, with Fit's margin, in the part of the
  // view no card covers; at once (mitcad#405).
  void frameBox(const Bnd_Box& box);
  // Moves the view sideways, as little as it takes, so that a box (model
  // coordinates: a part just placed) is out from under the panels floating
  // over the view (mitcad#656); false when it is clear already. Logged
  // "View moved dx,dy px: the part clear of the panels".
  // With `fit`, a box larger than the free area is fitted into it
  // (frameBox: zoomed out).
  bool bringClearOfPanels(const Bnd_Box& box, bool fit = false);
  // The box of the bodies shown as placed (not previews, ghosts, datums,
  // sketches or the grid); void when none is shown.
  Bnd_Box bodiesBox() const;
  // Ends a camera animation at its end and makes a pending fit now, so that
  // camera() and the next frame are where the view is going (live MCP's
  // replies and screenshots, mitcad#405).
  void settleCamera();
  // The perspective camera's vertical field of view, degrees.
  double fieldOfView() const;
  void setFieldOfView(double degrees);
  // Looks at a plane from the side its normal points to, `up` upwards, and
  // fits the view (Look At, named views; U3).
  void lookAlong(const gp_Dir& normal, const gp_Dir& up);
  // The isometric home view, fitted.
  void lookHome();

  // Position of a model point in the widget (logical pixels), for tests.
  QPoint toWidget(const gp_Pnt& point) const;
  // The same without rounding, for drawing over the view.
  QPointF toWidgetF(const gp_Pnt& point) const;
  // A point of a plane datum that faces the viewer, away from the others.
  gp_Pnt planePickPoint(const gp_Pnt& origin, const gp_Dir& normal, double size) const;
  // Points of a plane datum in the order a click is best tried at; with
  // `offDiagonals`, points off its diagonals instead, which an axis seen
  // along a diagonal does not cover (a placement input puts the planes
  // behind the axes, mitcad#656).
  std::vector<gp_Pnt> planePickCandidates(const gp_Pnt& origin, const gp_Dir& normal, double size,
                                          bool offDiagonals = false) const;
  // The camera's up direction (manipulators measure the zoom along it).
  gp_Dir viewUp() const;
  // The direction the camera looks along, from the eye to the model.
  gp_Dir viewDirection() const;
  // What a click at a widget position would pick now, if anything.
  std::optional<SelectionItem> itemAt(const QPointF& position);
  // Everything under a widget position that the filter takes, the item a
  // click picks first.
  std::vector<SelectionItem> itemsAt(const QPointF& position);
  // Whether a click at a widget position is the view's own (mitcad#899):
  // it would pick a precise target there now (an edge, a vertex, a sketch
  // curve, point or text, a datum axis or point; not only an area such as
  // a face or a profile), or a body edge or vertex of the filter's kinds is
  // under it that the finer test refuses (behind faces, which Shift
  // picks). Sketch mode's axes take only the clicks that are not.
  bool claimsClick(const QPointF& position);
  // For UI tests: a point in the widget where a click picks each face,
  // edge or vertex of the shown bodies that the filter takes (only those
  // a click there would pick).
  std::vector<std::pair<SelectionItem, QPoint>> pickPoints(SelectFilter kinds);
  // The same for what the filter's kinds take but its finer test refuses
  // (mitcad#474: the faces Create Sketch cannot sketch on).
  std::vector<std::pair<SelectionItem, QPoint>> refusedPickPoints(SelectFilter kinds);
  // What the last click hit of the filter's kinds that the finer test
  // refused, when the click picked nothing (mitcad#474: Create Sketch says
  // why a face cannot take a sketch); once per click.
  std::optional<SelectionItem> takeRefusedClick();
  // What the last click would have taken of the filter's kinds without the
  // finer test, whatever it picked (mitcad#844: a plane behind the model
  // picked where a face the test refuses lies in front of it); nothing
  // without a test or where a card covers the view.
  std::optional<SelectionItem> untestedClickItem();
  // The OpenGL version string (with the driver's version), once the view
  // is initialised; for error reports (mitcad#61).
  QString glVersion() const { return m_glVersion; }

  // A popup or native control can consume the pointer's release. End its
  // interaction without turning the interrupted drag into a selection.
  void cancelPointerInteraction();

signals:
  void badgeWidthChanged();
  void glInitialized(const QString& renderer);
  void glFailed(const QString& message);
  // A frame was drawn (the camera may have moved): drawings over the view
  // follow.
  void viewChanged();
  // The panels floating over the view moved or changed size
  // (`setCoveredRects`): what keeps clear of them (a drawing) follows.
  void coveredChanged();
  // A click (window false) or a window selection; empty when the click hit
  // nothing. Emitted while the view paints: connect queued.
  void picked(const mitcad::Selection& items, Qt::KeyboardModifiers modifiers, bool window);
  // What a click would pick changed as the mouse moved (invalid when
  // nothing), for drawings over the view that show it more boldly than
  // OCCT's highlight, whose lines are one pixel wide on macOS. Emitted while
  // the view paints: connect queued.
  void hovered(const mitcad::SelectionItem& item);
  // The point under the mouse moved on what a click would pick: the item
  // with `at` where the mouse is now (in its component's coordinates), or
  // an invalid item when the mouse left the view (mitcad#417: BUILD shows
  // an end click's part before the click). Emitted while the view paints:
  // connect queued.
  void hoverMoved(const mitcad::SelectionItem& item);
  // A right click; `item` is what lies under the mouse (invalid if nothing).
  void contextMenuRequested(const QPoint& globalPosition, const mitcad::SelectionItem& item);
  // A right click on the orientation cube.
  void orientationCubeMenuRequested(const QPoint& globalPosition);
  // The camera came to rest after it moved (logged as "Camera direction ...").
  void cameraRested();
  // A navigation of the view ended (isNavigating()): the camera is where the
  // user left it.
  void navigationFinished();
  // The application's palette changed (a dark or light mode): what depends
  // on it outside the view follows.
  void themeChanged();
  // The background turned from light to dark or back (backgroundDark()):
  // drawings over the view that log their tone follow (mitcad#633).
  void backgroundToneChanged();
  // setBodies() changed what is shown.
  void bodiesChanged();
  // Shift went down or up while a selection scope is set (setPickScope).
  void shiftChanged(bool held);
  // setNavigation() applied new settings (the 3D mouse's among them).
  void navigationChanged();

protected:
  // Shift anywhere in the window widens a scope's clicks (setPickScope).
  bool eventFilter(QObject* watched, QEvent* event) override;
  // A modal dialog that opens meanwhile (a long computation's progress, P7)
  // takes the release of a button held down: the view lets go of it then.
  bool event(QEvent* event) override;
  void initializeGL() override;
  void paintGL() override;
  void resizeGL(int width, int height) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void wheelEvent(QWheelEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void keyReleaseEvent(QKeyEvent* event) override;

  void handlePanning(const occ::handle<V3d_View>& view) override;
  void handleViewRedraw(const occ::handle<AIS_InteractiveContext>& context,
                        const occ::handle<V3d_View>& view) override;
  void contextLazyMoveTo(const occ::handle<AIS_InteractiveContext>& context,
                         const occ::handle<V3d_View>& view, const NCollection_Vec2<int>& point) override;
  void OnSelectionChanged(const occ::handle<AIS_InteractiveContext>& context,
                          const occ::handle<V3d_View>& view) override;

private:
  // What an object displays, for naming its picks.
  struct PickInfo {
    SelectionItem item;                    // complete, except for bodies
    std::shared_ptr<geometry::Shape> body; // bodies: names sub-shapes
    bool highlight = false;
  };
  class Filter;
  // A shown body's faces for looking along a line (occluded()), made when
  // first needed.
  struct Occluder;

  struct BodyEntry {
    std::string uid;
    std::string component;
    std::string occurrence;
    occ::handle<AIS_Shape> object;
    std::shared_ptr<geometry::Shape> shape;
    BodyDisplay::Style style;
    TopLoc_Location placement;
    QColor color;
    Appearance appearance;
    std::vector<BodyDisplay::FaceLook> faceLooks;
    // A normal body drawn as a ghost by setGhosts() (`style` is Ghost).
    bool ghosted = false;
    // Outside the active component (setActiveScope): a normal body is drawn
    // as a ghost that stays pickable (`style` stays Normal).
    bool inactive = false;
    // BodyDisplay::sectioned.
    bool sectioned = false;
    // The hidden-edge styles' edges: dashed through faces, solid in sight.
    occ::handle<AIS_Shape> hiddenEdges;
    occ::handle<AIS_Shape> visibleEdges;
    // Its faces placed, for occluded() (mitcad#217).
    mutable std::shared_ptr<Occluder> occluder;
  };

  Aspect_Drawable occtDrawable();
  NCollection_Vec2<int> toDevicePixels(const QPointF& point) const;
  Aspect_VKeyMouse viewButtons(Qt::MouseButtons buttons) const;
  Aspect_VKeyFlags viewFlags(Qt::KeyboardModifiers modifiers) const;
  void display(const occ::handle<AIS_Shape>& object, int displayMode, PickInfo info,
               bool pickable);
  // Shows a body in the visual style; the entry's objects are made here.
  void showBody(BodyEntry& entry);
  // A body shown elsewhere (mitcad#838): its objects take the placement,
  // drawn from what they have (no presentation computed again).
  void placeBody(BodyEntry& entry, const TopLoc_Location& placement);
  void removeBody(BodyEntry& entry);
  // Whether setGhosts() names an occurrence or one it lies in.
  bool isGhosted(const QString& occurrence) const;
  // Inactive ghosts (setActiveScope, mitcad#420): whether what an occurrence
  // places lies outside the active component, and whether an item (a
  // sketch entity, profile or datum) is drawn as one.
  bool isInactive(const QString& occurrence) const;
  bool isInactiveItem(const SelectionItem& item) const;
  // The same by the scope alone, whatever the setting (the pick priority).
  bool outsideActive(const QString& occurrence) const;
  bool outsideActive(const SelectionItem& item) const;
  // Draws the bodies, sketch entities, profiles and datums by the scope
  // and the setting again; `faded` restyles the pale ones too (their
  // colour follows the background).
  void applyInactiveGhosts(bool faded = false);
  // Marks an object as drawn pale or not; true when that changed.
  bool markInactive(const occ::handle<AIS_Shape>& object, bool inactive);
  bool drawnInactive(const occ::handle<AIS_Shape>& object) const {
    return m_inactiveObjects.count(object.get()) != 0;
  }
  // A profile's and a datum's look, pale when drawnInactive(); a datum's
  // display mode.
  void styleProfile(const occ::handle<AIS_Shape>& profile) const;
  int styleDatum(const occ::handle<AIS_Shape>& object, SelectKind kind) const;
  void logInactiveGhosts();
  // The rendered mode's layer for a body, and its edges hidden under a
  // rendered image.
  void applyRenderedVisibility(BodyEntry& entry);
  // The background colours again (after a rendered image).
  void applyBackground();
  void bindGestures();
  // Starts a smooth camera move to `end` (or jumps when not animating).
  void moveCamera(const occ::handle<Graphic3d_Camera>& end);
  void fitCamera(const occ::handle<Graphic3d_Camera>& camera) const;
  // turnTo's work, which runViewOperation guards.
  void turnCameraTo(const gp_Dir& direction, const gp_Dir& up);
  // Runs a camera change that a button, a command or a slot starts
  // (mitcad#773): an exception out of a Qt slot ends the application, so a
  // failure (OCCT's or a standard one) is logged instead, "View operation
  // failed: <operation> (<step>): <type>: <message>", and the camera is put
  // back as it was. The step is the last m_viewStep the operation set.
  void runViewOperation(const QString& operation, const std::function<void()>& run);
  // The part of the view the floating cards leave free (the whole view when
  // there are no cards).
  QRectF freeArea() const;
  // Zooms out and pans a camera that has been fitted to the whole view so
  // that the fitted objects are inside freeArea() and centred in it.
  void fitToFreeArea(const occ::handle<Graphic3d_Camera>& camera) const;
  // The camera move that brings what the middle of the view shows (at the
  // depth of the camera's centre) to the middle of freeArea(), without a
  // zoom; none while the view has no size.
  gp_Vec freeAreaShift(const occ::handle<Graphic3d_Camera>& camera) const;
  QRectF cubeArea() const;
  void placeHomeButton();
  // The buttons over the view take dark or light icons by the background.
  void updateOverlayIcons();
  // What a shown sketch entity's colour and line follow (setSketchEntities):
  // fully constrained geometry and centre lines are light on a dark
  // background (mitcad#253).
  struct SketchEntityLook {
    bool point = false;
    bool construction = false;
    bool centerline = false;
    bool constrained = false;
    bool reference = false;
    bool temporary = false; // shown only for a command (mitcad#546)
  };
  void styleSketchEntity(const occ::handle<AIS_Shape>& object, const SketchEntityLook& look) const;
  // Whether the entity is drawn over a halo (mitcad#302): fully constrained
  // curves and centre lines, whose colour follows the background and would
  // otherwise vanish over a body's face of the same tone.
  static bool hasSketchHalo(const SketchEntityLook& look);
  // The halo: a wider line in the background's tone under the entity's.
  void styleSketchHalo(const occ::handle<AIS_Shape>& halo, const SketchEntityLook& look) const;
  // Logs "Sketch halos: <n> dark|light" when it changed (for UI tests).
  void logSketchHalos();
  // Logs "Sketch layers: halos < curves and highlights < points and point
  // highlights; …" when it changed: the order the sketch layers are drawn
  // in, read from the viewer (for UI tests, mitcad#897).
  void logSketchLayers();
  // Recolours the sketch entities shown after the background turned dark
  // or light.
  void restyleSketchEntities();
  // The orientation cube and the pick tolerance are in device pixels; this
  // scales them by the widget's device pixel ratio (no change at 1).
  void applyDevicePixelRatio();
  // Sets the cube's size and place (device pixel ratio, insets) and shows it
  // again.
  void applyCubePlacement();
  // The orientation cube's arrows (P9): shown while the view looks
  // straight at a face; the triangles turn the view to the face beside it,
  // the curved arrows roll it 90 degrees about the line of sight.
  void placeCubeArrows();
  void updateCubeArrows();
  void turnCube(const QString& arrow);
  // Logs the camera, and with MITCAD_LOG_PICKS where the cube's faces,
  // edges and corners can be clicked, once the camera rests.
  void cameraMoved();
  void logCamera();
  void logOrientationCube();
  // The cube's own perspective (mitcad#774) seen from where the camera
  // looks from now: its presentation and picking made again when the
  // direction changed. Nothing in the other projections.
  void updateCubeEye();
  // The orientation cube's box as drawn, in widget pixels: its corners
  // through its own perspective (if any) and its transform persistence as
  // the renderer applies it (mitcad#766: an orthographic cube is the same
  // in a perspective view as in an orthographic one). Empty before the view
  // has its window.
  QRectF cubeOutline() const;
  // Navigation (mitcad#214): the state evaluated now, and a step of input
  // without a release (wheel, pinch, roll, resize).
  void updateNavigation();
  void navigationStep();
  // The last button held on the view went up.
  void buttonsReleased();
  // The view's camera and size now; why the rendered image no longer
  // fits the view (its camera or size changed), or nothing.
  ViewPose viewPose() const;
  QString renderedImageStale() const;
#ifdef __APPLE__
  void releaseRenderedSurfaceContext();
#endif
  // Rebuilds the grid's lines when the zoom changes their step or the view
  // nears their edge (or always with `force`).
  void updateGrid(bool force = false);
  void remove(const occ::handle<AIS_Shape>& object);
  void activatePicking(const occ::handle<AIS_Shape>& object);
  void activateAllPicking();
  std::optional<SelectionItem> itemFor(const occ::handle<SelectMgr_EntityOwner>& owner) const;
  // Where the last click hit an owner, in its object's own coordinates.
  std::optional<std::array<double, 3>> pickedPoint(const occ::handle<SelectMgr_EntityOwner>& owner) const;
  // The same in the design's coordinates.
  std::optional<gp_Pnt> pickedWorldPoint(const occ::handle<SelectMgr_EntityOwner>& owner) const;
  // Ends a drag of a component (mitcad#55), cancelled or where it is.
  void endOccurrenceDrag(const QPointF& position, bool cancelled);
  bool accepts(const occ::handle<SelectMgr_EntityOwner>& owner) const;
  // Selection scopes (mitcad#217): the kinds pickable now (the expanded
  // ones while Shift is held), Shift's state (`hover`: pick again where the
  // pointer is), whether a point of the design is behind a face of a shown
  // body seen from the camera, and which part of an edge a click (at its
  // picked point) or the last window (its samples inside) hit.
  SelectFilter activeKinds() const;
  void setShiftHeld(bool held, bool hover);
  bool occluded(const gp_Pnt& point) const;
  EdgeSight edgeSight(const occ::handle<SelectMgr_EntityOwner>& owner) const;
  EdgeSight windowSight(const TopoDS_Edge& edge, const gp_Trsf& toWorld) const;
  void sceneChanged();
  // Asks for a frame (update()). The view handles its events (picks,
  // camera animations) while it draws a frame; where Qt never exposes the
  // window, so that update() paints nothing (Windows session 0, where the
  // UI tests run over SSH), the frame is drawn directly instead.
  void requestFrame();
  // Trackpad navigation: a two-finger scroll (wheelEvent), and the pinch,
  // rotation and smart zoom gestures (event, every scheme).
  void trackpadScroll(QWheelEvent* event);
  void nativeGesture(QNativeGestureEvent* event);
  // Ends the drag that a scroll is passed to the view as (pan or orbit).
  void endScrollDrag();
  // Rolls the view about its direction of sight (clockwise positive, as Qt's rotation
  // gesture counts).
  void rollView(double degrees);
  // The arrow, or the sketch tool's precision cursor for the screen's ratio;
  // `log` tells which (for UI tests), not each snap.
  void updateCursor(bool log);

#if !defined(_WIN32) && !defined(Q_OS_MACOS)
  // Qt 6 makes the widget's context current on an offscreen pbuffer, but
  // OCCT's GLX backend needs an X11 window to make the context current on.
  // This hidden window is created with the context's own format, so the two
  // are compatible.
  std::unique_ptr<QWindow> m_glxWindow;
#endif
  occ::handle<V3d_Viewer> m_viewer;
  occ::handle<V3d_View> m_view;
  occ::handle<AIS_InteractiveContext> m_context;
  occ::handle<AIS_ViewCube> m_orientationCube;
  occ::handle<SelectMgr_Filter> m_pickFilter;
  std::map<std::string, BodyEntry> m_bodies;
  QStringList m_ghosts; // setGhosts()
  // Inactive ghosts (mitcad#420): the scope, the setting, the sketch
  // entities, profiles and datums drawn pale, and the log line last written.
  ActiveScope m_activeScope;
  bool m_ghostInactive = true;
  bool m_activePickPriority = true; // setActivePickPriority()
  bool m_planesBehind = false;       // setPlanesBehind()
  bool m_planesByDepth = false;      // setPlanesByDepth()
  bool m_sketchesBehind = false;     // setSketchesBehind()
  QString m_sketchKept;              // its sketch that keeps its place
  std::set<const AIS_InteractiveObject*> m_inactiveObjects;
  QString m_loggedInactiveGhosts = QStringLiteral("none");
  std::map<ProfileKey, occ::handle<AIS_Shape>> m_profiles;
  std::vector<occ::handle<AIS_Shape>> m_sketchEntities;
  // The shown sketch entities by "<sketch>/<id>@<occurrence>", with their
  // signatures.
  std::map<QString, std::pair<QString, occ::handle<AIS_Shape>>> m_sketchEntityKeys;
  // How each of them is drawn, for restyleSketchEntities().
  std::map<const AIS_InteractiveObject*, SketchEntityLook> m_sketchEntityLooks;
  // Their halos (mitcad#302), not pickable, drawn before the entities.
  std::map<const AIS_InteractiveObject*, occ::handle<AIS_Shape>> m_sketchEntityHalos;
  // The halos as last logged ("Sketch halos:").
  QString m_loggedSketchHalos;
  // The sketch layers' order and the sketch highlights as last logged
  // ("Sketch layers:", "Sketch highlights:", mitcad#897).
  QString m_loggedSketchLayers;
  QString m_loggedSketchHighlights;
  // The shown sketches' colours as last logged ("Sketch colours:", for UI
  // tests; mitcad#286).
  QString m_loggedSketchColours;
  std::vector<occ::handle<AIS_Shape>> m_datums;
  std::vector<occ::handle<AIS_Shape>> m_highlights;
  // What setHighlights() drew last: drawn again when the background's tone
  // changes, as input lines have a variant for a dark one (mitcad#957).
  std::vector<HighlightDisplay> m_shownHighlights;
  // The whole bodies highlighted, as last logged ("View body highlights:",
  // for UI tests: the bodies a command's selection input holds).
  QString m_loggedBodyHighlights = QStringLiteral("none");
  // The profiles highlighted with their faces, as last logged ("View
  // profile highlights:", for UI tests: every piece of a split region,
  // mitcad#410).
  QString m_loggedProfileHighlights = QStringLiteral("none");
  // Where faces, edges, vertices and datums are highlighted, as last logged
  // ("View highlight places:", with MITCAD_LOG_PICKS; mitcad#432).
  QString m_loggedHighlightPlaces = QStringLiteral("none");
  // What each pickable object shows.
  std::map<const AIS_InteractiveObject*, PickInfo> m_pickInfo;
  occ::handle<AIS_Shape> m_preview;
  int m_previewMode = 1; // AIS_Shaded, or AIS_WireFrame for a failed one
  // setProvisional's (mitcad#636), and the frames drawn.
  occ::handle<AIS_InteractiveObject> m_provisional;
  quint64 m_framesDrawn = 0;
  occ::handle<AIS_Shape> m_sketchPreview;
  occ::handle<Graphic3d_Camera> m_savedCamera;
  SelectFilter m_filter;
  // setPickFilter's test, or setPickScope's (m_scoped).
  PickTest m_pickTest;
  bool m_pickingSuspended = false;
  SelectFilter m_expandedFilter; // the kinds while Shift is held
  bool m_scoped = false;
  bool m_shiftHeld = false;
  bool m_contextPick = false; // a right click finds what a Shift+click would take
  // A window selection is being applied (from its release until the
  // selection is reported): its rectangle and direction.
  bool m_windowSelecting = false;
  QRectF m_windowRect;
  bool m_windowCrossing = false;
  // With MITCAD_LOG_TIMING: from a window selection's release until the
  // frame that shows it (windowSelectionApplied).
  QElapsedTimer m_windowClock;
  bool m_windowShowPending = false;
  // logFirstFrame: what to log after the next frame, and since when.
  QString m_firstFrameOf;
  QElapsedTimer m_firstFrameClock;
  gp_Ax3 m_sketchPlane;
  bool m_sketchInput = false;
  // The sketch tool's cursor (mitcad#3): its icon, and whether it snaps.
  QString m_sketchToolIcon;
  bool m_sketchSnapping = false;
  bool m_fitPending = false;
  FirstBodiesFit m_firstFit; // the design's first bodies fitted once (expectFirstBodies)
  bool m_directFrameQueued = false; // requestFrame() in an unexposed window
  bool m_painting = false;          // in paintGL()
  QString m_glVersion;              // GL_VERSION (glVersion())
  // The left press that starts a pick, and whether its release was a drag.
  QPointF m_pressPosition;
  // Where the last click was released, and whether it picked nothing
  // (takeRefusedClick); accepts() skips the finer test while
  // m_untestedPicks is set.
  QPointF m_clickPosition;
  bool m_clickPickedNothing = false;
  bool m_untestedPicks = false;
  // While pickPoints() tries its points (the view, the filter and the
  // input stand still): what accepts() found of each owner it was asked
  // about, so that the hundreds of a toothed part's items under each point
  // are tested once (mitcad#697).
  enum class OwnerTest { Refused, Accepted, Tested };
  mutable std::map<const SelectMgr_EntityOwner*, std::pair<OwnerTest, SelectKind>>* m_ownerTests = nullptr;
  Qt::KeyboardModifiers m_pressModifiers;
  bool m_windowPick = false;
  bool m_cubeClick = false;
  bool m_cubePress = false; // the left button went down on the cube
  QPointF m_rightPressPosition;
  bool m_orbitKey = false; // F4 held (MiddlePanF4: the left button orbits)
  // Dragging components (mitcad#55): the body pressed on, where, and
  // whether the pointer has moved far enough to drag.
  OccurrenceDrag m_occurrenceDrag;
  std::optional<SelectionItem> m_dragItem;
  gp_Pnt m_dragGrab;
  bool m_dragging = false;
  bool m_dragRefused = false; // the press is neither a drag nor a window selection
  bool m_dragCursor = false;
  // What the scrolling of a trackpad does until it pauses; a pan or an orbit
  // is a drag of the left button that is passed to the view in steps, so
  // that it goes the way of a mouse drag (m_scrollPoint: device pixels).
  enum class ScrollMode { None, Pan, Orbit, Zoom };
  ScrollMode m_scrollMode = ScrollMode::None;
  bool m_scrollDragging = false;
  QPointF m_scrollPoint;
  QTimer* m_scrollTimer = nullptr;

  // Display settings (U5).
  VisualStyle m_visualStyle = VisualStyle::ShadedVisibleEdges;
  bool m_componentColors = false;
  NavigationSettings m_navigation;
  QPointF m_panRemainder;
  bool m_f4Press = false;
  Graphic3d_ZLayerId m_hiddenLayer = Graphic3d_ZLayerId_UNKNOWN;
  Graphic3d_ZLayerId m_edgeLayer = Graphic3d_ZLayerId_UNKNOWN;
  // Sketch entities' halos, just before the top layer (mitcad#302).
  Graphic3d_ZLayerId m_sketchHaloLayer = Graphic3d_ZLayerId_UNKNOWN;
  // Sketch points and their selection highlights, just after the top layer
  // that holds the curves and their highlights (mitcad#897), with the
  // points' own hover style in it.
  Graphic3d_ZLayerId m_sketchPointLayer = Graphic3d_ZLayerId_UNKNOWN;
  occ::handle<Prs3d_Drawer> m_sketchPointHover;
  // Image references drawn through the bodies, just before the halos
  // (mitcad#782).
  Graphic3d_ZLayerId m_imageLayer = Graphic3d_ZLayerId_UNKNOWN;
  // A feature's input lines where they were (mitcad#957), after the top
  // layer and the points' layer: their dashed copy drawn through the
  // solids, writing no depth, then the line in sight against the scene's
  // depth, which covers the copy, so that only the hidden parts show
  // dashed, over the model's own edges.
  Graphic3d_ZLayerId m_inputLineLayer = Graphic3d_ZLayerId_UNKNOWN;
  Graphic3d_ZLayerId m_inputLineSightLayer = Graphic3d_ZLayerId_UNKNOWN;
  std::vector<occ::handle<AIS_Shape>> m_sectionCaps;
  std::vector<occ::handle<AIS_Shape>> m_sectionHatches;
  QStringList m_sectionLook;      // per cap group: colour, hatch colour, direction, lines
  QString m_loggedSectionLook;
  void logSectionCaps();
  occ::handle<AIS_Shape> m_threads;
  // Print Analysis' triangles, and its marker, label and legend over the
  // view (setPrintOverlay, mitcad#620).
  occ::handle<AIS_InteractiveObject> m_printTriangles;
  // The build volume's lines and the boxes of bodies outside it
  // (setBuildVolume, mitcad#640).
  occ::handle<AIS_Shape> m_buildFrame;
  occ::handle<AIS_Shape> m_buildOutside;
  PrintOverlayLabels* m_printLabels = nullptr;
  QString m_loggedPrintOverlay = QStringLiteral("Print overlay: none");
  // Measurement objects (setMeasurements, mitcad#649).
  std::vector<MeasurementDisplay> m_measurements;
  MeasurementOverlay* m_measurementOverlay = nullptr;
  QString m_loggedMeasurements = QStringLiteral("Annotations drawn: none");
  // Annotations (mitcad#702): what clicks on them do, and those
  // highlighted.
  AnnotationActions m_annotationActions;
  QStringList m_highlightedAnnotations;
  QString m_loggedAnnotationPlaces; // their labels' and lines' places as last logged
  QColor m_paper = QColor(0xd8, 0xdc, 0xe2); // the hidden-line style's faces
  bool m_gridShown = true;
  double m_gridStep = 0.0; // mm; 0: follows the zoom
  occ::handle<LayoutGrid> m_grid;
  gp_Ax3 m_gridPlane;
  Graphic3d_ZLayerId m_gridLayer = Graphic3d_ZLayerId_UNKNOWN;
  std::optional<CameraState> m_home;
  // The step of a camera change under way (runViewOperation), which a
  // failure names: set by fitCamera too, hence mutable.
  mutable const char* m_viewStep = nullptr;
  ViewCubeHomeButton* m_homeButton = nullptr;
  bool m_nativeControls = false;
  bool m_backgroundDark = false; // the view's background is dark
  QColor m_backgroundTop = QColor(237, 240, 245);
  QColor m_backgroundBottom = QColor(189, 196, 209);
  bool m_backgroundGradient = true;
  // The rendered view: the image, the pixels OCCT reads from it, and the
  // background texture that shows them.
  QImage m_renderedImage;
#ifdef __APPLE__
  std::shared_ptr<render::MacFrameLease> m_renderedSurface;
  std::shared_ptr<render::MacFrameDisplay> m_renderedSurfaceDisplay;
  QMetaObject::Connection m_renderedContextCleanup;
#endif
  occ::handle<Image_PixMap> m_renderedPixels;
  occ::handle<Graphic3d_Texture2D> m_renderedTexture;
  bool m_renderedMode = false;
  // The camera and the window's size when the image was set.
  ViewPose m_renderedPose;
  // Navigation (isNavigating()): its state, its clock, and the timer that
  // ends a step's quiet time.
  ViewNavigation m_navigationState;
  QElapsedTimer m_navigationClock;
  QTimer* m_navigationTimer = nullptr;
  // What a navigation cost besides the view's frames, logged when it ends
  // (mitcad#768, "View navigation: ..."): the widgets over the view painted
  // again (each makes Qt compose the window's area under the view again at
  // that frame) and the Print Analysis overlays set.
  quint64 m_navigationFirstFrame = 0;
  int m_navigationRepaints = 0;
  QStringList m_navigationRepainted; // their object names
  int m_navigationPrintOverlays = 0;
  class RenderedImageQuad;
  occ::handle<RenderedImageQuad> m_renderedQuad;
  Graphic3d_ZLayerId m_renderedBodyLayer = Graphic3d_ZLayerId_UNKNOWN;
  Graphic3d_ZLayerId m_renderedImageLayer = Graphic3d_ZLayerId_UNKNOWN;
  QMargins m_overlayInsets;
  int m_badgeWidth = 0;
  std::vector<QRect> m_covered;
  std::vector<QRect> m_handleBoxes; // setHandleBoxes
  std::vector<QRect> m_topCards; // setTopCards
  int m_topCardsBottom = 0;
  qreal m_cubeRatio = 1.0;       // the device pixel ratio applied to the cube
  double m_cubeBaseSize = 0.0;   // the cube's size and font height at ratio 1
  double m_cubeBaseFont = 0.0;
  CubeProjection m_cubeProjection = CubeProjection::FollowView; // mitcad#774
  // up, down, left, right, cw, ccw
  std::vector<std::pair<QString, QToolButton*>> m_cubeArrows;
  QTimer* m_restTimer = nullptr;
  QString m_loggedCamera;
  QString m_restedView; // the eye and scale when the camera last rested
  double m_loggedGridStep = -1.0;
  QString m_restingCamera; // the camera when the rest timer started
  // What the mouse was last over, for hovered().
  occ::handle<SelectMgr_EntityOwner> m_hoveredOwner;
  // Where the mouse was last on it, for hoverMoved(), and whether it moved
  // over the view since the last frame.
  std::optional<std::array<double, 3>> m_hoveredPoint;
  bool m_hoverMoved = false;
  // The hover's colour, and the rest of the hovered tangent chain
  // (setHoverChain, mitcad#290).
  QColor m_hoverColor = QColor(0x2f, 0x9b, 0xff);
  occ::handle<AIS_Shape> m_hoverChain;
  // The image references drawn and their textures, by digest and opacity
  // (setImageReferences, mitcad#782).
  std::vector<occ::handle<AIS_InteractiveObject>> m_imageReferences;
  QHash<QString, occ::handle<Graphic3d_Texture2D>> m_imageTextures;
  QString m_loggedImageReferences = QStringLiteral("Image references drawn: none");
  bool m_loggedImageLayer = false;
  // A 3D mouse's motion (setSpaceMouseRates, mitcad#951): its rates, the
  // clock between its frames, the orbit's centre taken as it began, and
  // what it did so far (logged when it ends).
  void stepSpaceMouse();
  void endSpaceMouse();
  SpaceMouseRates m_spaceMouse;
  QElapsedTimer m_spaceMouseClock;
  std::optional<gp_Pnt> m_spaceMousePivot;
  int m_spaceMouseFrames = 0;
  double m_spaceMouseTurned = 0.0; // degrees
  double m_spaceMousePanned = 0.0; // view heights
  double m_spaceMouseStartScale = 0.0; // the camera's scale as it began (the zoom logged)
};

} // namespace mitcad
