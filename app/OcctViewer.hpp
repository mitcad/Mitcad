// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <cstddef>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <QColor>
#include <QMargins>
#include <QOpenGLWidget>
#include <QPoint>
#include <QString>
#include <QSurfaceFormat>
#include <QtGlobal>

#include <AIS_InteractiveContext.hxx>
#include <AIS_Shape.hxx>
#include <AIS_ViewController.hxx>
#include <AIS_ViewCube.hxx>
#include <Graphic3d_Camera.hxx>
#include <Graphic3d_ZLayerId.hxx>
#include <SelectMgr_EntityOwner.hxx>
#include <SelectMgr_Filter.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Shape.hxx>
#include <V3d_View.hxx>
#include <V3d_Viewer.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>

#include "LayoutGrid.hpp"
#include "framework/Selection.hpp"
#include "mitcad/geometry/shape.hpp"

class QNativeGestureEvent;
class QTimer;
class QToolButton;
class QWheelEvent;

namespace mitcad {

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
};

// A sketch curve or point in model space.
struct SketchEntityDisplay {
  QString sketch;
  QString id;       // c3, p4
  QString geometry; // line, circle, arc, ellipse, point, ...
  TopoDS_Shape shape;
  bool construction = false;
  // How sketch mode draws it (U2): fully constrained geometry darker,
  // centre lines dash-dotted, projected geometry purple.
  bool constrained = false;
  bool centerline = false;
  bool reference = false;
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
};

// How bodies are drawn (U5).
enum class VisualStyle {
  Shaded,               // faces only
  ShadedVisibleEdges,   // faces and the edges in sight (the default)
  ShadedHiddenEdges,    // also the edges behind faces, dashed
  Wireframe,            // edges only
  WireframeHiddenEdges, // edges in sight, those behind faces dashed
};

// Which mouse buttons pan, orbit and zoom (U5), named after what the mouse
// does. The wheel zooms in every scheme, and a trackpad's pinch (zoom),
// rotation (roll) and two-finger double tap (fit) work in every scheme too.
enum class NavigationScheme {
  MiddlePan,   // middle: pan; Shift + middle: orbit (the default)
  AltButtons,  // Alt + left: orbit; Alt + middle: pan; Alt + right: zoom
  MiddlePanF4, // middle: pan; Shift + middle or F4 + left: orbit
  MiddleOrbit, // middle: orbit; Ctrl + middle: pan; Shift + middle: zoom
  RightOrbit,  // right: orbit; middle or Shift + right: pan
  // For a trackpad (the default on macOS), appended last: two-finger scroll
  // pans, Alt + scroll orbits, Shift or Ctrl + scroll zooms; Alt + left
  // orbits, Shift + Alt + left, middle: pan; right: orbit. A mouse wheel
  // zooms as in the other schemes.
  Trackpad,
};

struct NavigationSettings {
  NavigationScheme scheme = NavigationScheme::MiddlePan;
  bool zoomToCursor = true;    // the wheel zooms about the cursor, else the middle
  bool reverseZoom = false;    // the wheel zooms the other way
  bool constrainedOrbit = false; // orbits keep Z up
  bool animate = true;         // view changes (standard views, named views) move smoothly
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
  };

  explicit OcctViewer(QWidget* parent = nullptr);
  ~OcctViewer() override;

  // Display settings (U5).
  void setVisualStyle(VisualStyle style);
  VisualStyle visualStyle() const { return m_visualStyle; }
  void setNavigation(const NavigationSettings& settings);
  const NavigationSettings& navigation() const { return m_navigation; }
  // The background: a vertical gradient from top to bottom, or one colour.
  void setBackground(const QColor& top, const QColor& bottom, bool gradient);
  // The layout grid on the grid plane (XY, or the sketch plane): `step`
  // mm between lines, or a step that follows the zoom when 0.
  void setGrid(bool shown, double step);
  void setGridPlane(const gp_Ax3& plane);
  // The grid's minor step now (mm), shown or not: what sketch points snap to.
  double gridStep() const;
  // The faces where Section Analysis cuts the bodies (caps), drawn over
  // the cut; null clears them.
  void setSectionCaps(const TopoDS_Shape& caps);
  // Cosmetic threads: rings on the threaded faces, drawn over the bodies
  // and not pickable; null clears them.
  void setThreads(const TopoDS_Shape& rings);

  // The camera (named views, U5).
  CameraState camera() const;
  // Moves the camera there, smoothly when the navigation settings say so.
  void setCamera(const CameraState& camera);
  void setPerspective(bool perspective);
  bool isPerspective() const;
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
  // Replaces the visible sketch profiles (planar faces, placed already);
  // `occurrences` are those that place the sketches of components.
  void setProfiles(const std::vector<std::pair<ProfileKey, TopoDS_Shape>>& profiles,
                   const std::map<std::string, QString>& occurrences = {});
  void setSketchEntities(const std::vector<SketchEntityDisplay>& entities);
  void setDatums(const std::vector<DatumDisplay>& datums);
  void setHighlights(const std::vector<HighlightDisplay>& highlights);
  // Semi-transparent preview of the active command; null clears it.
  void setPreview(const TopoDS_Shape& shape, PreviewStyle style = PreviewStyle::Add);
  // Outline of the sketch shape being drawn; null clears it.
  void setSketchPreview(const TopoDS_Shape& shape);

  // What can be picked: kinds, and optionally a finer test.
  void setPickFilter(SelectFilter filter, std::function<bool(const SelectionItem&)> test = {});
  SelectFilter pickFilter() const { return m_filter; }
  // Colour of the highlight under the mouse.
  void setHoverColor(const QColor& color);

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
  // saved when `saveCamera` was set.
  void lookAtSketchPlane(bool saveCamera = true);
  void restoreCamera();
  void fitAll();
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
  // Points of a plane datum in the order a click is best tried at.
  std::vector<gp_Pnt> planePickCandidates(const gp_Pnt& origin, const gp_Dir& normal,
                                          double size) const;
  // The camera's up direction (manipulators measure the zoom along it).
  gp_Dir viewUp() const;
  // What a click at a widget position would pick now, if anything.
  std::optional<SelectionItem> itemAt(const QPointF& position);
  // Everything under a widget position that the filter takes, the item a
  // click picks first.
  std::vector<SelectionItem> itemsAt(const QPointF& position);
  // For UI tests: a point in the widget where a click picks each face,
  // edge or vertex of the shown bodies that the filter takes (only those
  // a click there would pick).
  std::vector<std::pair<SelectionItem, QPoint>> pickPoints(SelectFilter kinds);

signals:
  void badgeWidthChanged();
  void glInitialized(const QString& renderer);
  void glFailed(const QString& message);
  // A frame was drawn (the camera may have moved): drawings over the view
  // follow.
  void viewChanged();
  // A click (window false) or a window selection; empty when the click hit
  // nothing. Emitted while the view paints: connect queued.
  void picked(const mitcad::Selection& items, Qt::KeyboardModifiers modifiers, bool window);
  // A right click; `item` is what lies under the mouse (invalid if nothing).
  void contextMenuRequested(const QPoint& globalPosition, const mitcad::SelectionItem& item);
  // A right click on the orientation cube.
  void orientationCubeMenuRequested(const QPoint& globalPosition);
  // The camera came to rest after it moved (logged as "Camera direction ...").
  void cameraRested();
  // The application's palette changed (a dark or light mode): what depends
  // on it outside the view follows.
  void themeChanged();

protected:
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

  struct BodyEntry {
    std::string uid;
    std::string occurrence;
    occ::handle<AIS_Shape> object;
    std::shared_ptr<geometry::Shape> shape;
    BodyDisplay::Style style;
    TopLoc_Location placement;
    QColor color;
    // The hidden-edge styles' edges: dashed through faces, solid in sight.
    occ::handle<AIS_Shape> hiddenEdges;
    occ::handle<AIS_Shape> visibleEdges;
  };

  Aspect_Drawable occtDrawable();
  NCollection_Vec2<int> toDevicePixels(const QPointF& point) const;
  Aspect_VKeyMouse viewButtons(Qt::MouseButtons buttons) const;
  Aspect_VKeyFlags viewFlags(Qt::KeyboardModifiers modifiers) const;
  void display(const occ::handle<AIS_Shape>& object, int displayMode, PickInfo info,
               bool pickable);
  // Shows a body in the visual style; the entry's objects are made here.
  void showBody(BodyEntry& entry);
  void removeBody(BodyEntry& entry);
  void bindGestures();
  // Starts a smooth camera move to `end` (or jumps when not animating).
  void moveCamera(const occ::handle<Graphic3d_Camera>& end);
  void fitCamera(const occ::handle<Graphic3d_Camera>& camera) const;
  // The part of the view the floating cards leave free (the whole view when
  // there are no cards).
  QRectF freeArea() const;
  // Zooms out and pans a camera that has been fitted to the whole view so
  // that the fitted objects are inside freeArea() and centred in it.
  void fitToFreeArea(const occ::handle<Graphic3d_Camera>& camera) const;
  QRectF cubeArea() const;
  void placeHomeButton();
  // The buttons over the view take dark or light icons by the background.
  void updateOverlayIcons();
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
  // Rebuilds the grid's lines when the zoom changes their step or the view
  // nears their edge (or always with `force`).
  void updateGrid(bool force = false);
  void remove(const occ::handle<AIS_Shape>& object);
  void activatePicking(const occ::handle<AIS_Shape>& object);
  void activateAllPicking();
  std::optional<SelectionItem> itemFor(const occ::handle<SelectMgr_EntityOwner>& owner) const;
  // Where the last click hit an owner, in its object's own coordinates.
  std::optional<std::array<double, 3>> pickedPoint(const occ::handle<SelectMgr_EntityOwner>& owner) const;
  bool accepts(const occ::handle<SelectMgr_EntityOwner>& owner) const;
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
  std::map<ProfileKey, occ::handle<AIS_Shape>> m_profiles;
  std::vector<occ::handle<AIS_Shape>> m_sketchEntities;
  // The shown sketch entities by "<sketch>/<id>@<occurrence>", with their
  // signatures.
  std::map<QString, std::pair<QString, occ::handle<AIS_Shape>>> m_sketchEntityKeys;
  std::vector<occ::handle<AIS_Shape>> m_datums;
  std::vector<occ::handle<AIS_Shape>> m_highlights;
  // What each pickable object shows.
  std::map<const AIS_InteractiveObject*, PickInfo> m_pickInfo;
  occ::handle<AIS_Shape> m_preview;
  occ::handle<AIS_Shape> m_sketchPreview;
  occ::handle<Graphic3d_Camera> m_savedCamera;
  SelectFilter m_filter;
  std::function<bool(const SelectionItem&)> m_filterTest;
  gp_Ax3 m_sketchPlane;
  bool m_sketchInput = false;
  // The sketch tool's cursor (mitcad#3): its icon, and whether it snaps.
  QString m_sketchToolIcon;
  bool m_sketchSnapping = false;
  bool m_fitPending = false;
  bool m_directFrameQueued = false; // requestFrame() in an unexposed window
  bool m_painting = false;          // in paintGL()
  // The left press that starts a pick, and whether its release was a drag.
  QPointF m_pressPosition;
  Qt::KeyboardModifiers m_pressModifiers;
  bool m_windowPick = false;
  bool m_cubeClick = false;
  bool m_cubePress = false; // the left button went down on the cube
  QPointF m_rightPressPosition;
  bool m_orbitKey = false; // F4 held (MiddlePanF4: the left button orbits)
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
  NavigationSettings m_navigation;
  Graphic3d_ZLayerId m_hiddenLayer = Graphic3d_ZLayerId_UNKNOWN;
  Graphic3d_ZLayerId m_edgeLayer = Graphic3d_ZLayerId_UNKNOWN;
  occ::handle<AIS_Shape> m_sectionCaps;
  occ::handle<AIS_Shape> m_threads;
  QColor m_paper = QColor(0xd8, 0xdc, 0xe2); // the hidden-line style's faces
  bool m_gridShown = true;
  double m_gridStep = 0.0; // mm; 0: follows the zoom
  occ::handle<LayoutGrid> m_grid;
  gp_Ax3 m_gridPlane;
  Graphic3d_ZLayerId m_gridLayer = Graphic3d_ZLayerId_UNKNOWN;
  std::optional<CameraState> m_home;
  QToolButton* m_homeButton = nullptr;
  bool m_backgroundDark = false; // the view's background is dark
  QMargins m_overlayInsets;
  int m_badgeWidth = 0;
  std::vector<QRect> m_covered;
  qreal m_cubeRatio = 1.0;       // the device pixel ratio applied to the cube
  double m_cubeBaseSize = 0.0;   // the cube's size and font height at ratio 1
  double m_cubeBaseFont = 0.0;
  // up, down, left, right, cw, ccw
  std::vector<std::pair<QString, QToolButton*>> m_cubeArrows;
  QTimer* m_restTimer = nullptr;
  QString m_loggedCamera;
  double m_loggedGridStep = -1.0;
  QString m_restingCamera; // the camera when the rest timer started
};

} // namespace mitcad
