// SPDX-License-Identifier: MIT
#pragma once

// The editing context of a 3D sketch (mitcad#1524): sketch mode for a
// `spatial_sketch` feature. The model solves and stores everything (the
// `sketch3d.*` commands, `sketch3d` query); this reads the sketch after
// each change, and draws over the view what the view's own display of its
// points and curves does not show: the sketch's frame (origin and X, Y and
// Z axes, independent of the camera), a control-point spline's control
// polygon and a fit-point spline's end tangent handles, the `In plane`
// badges of the relations and dimensions kept in the sketch plane (one per
// shape, beside it, mitcad#1592), the points On Sketch Plane keeps, and
// what is under the pointer (a reference curve with its source). Every
// badge keeps inside the part of the view no panel covers. Camera orbit,
// pan and zoom stay the view's: the overlay takes no mouse input.
//
// The drawing tools (mitcad#1525, Sketch3dTools.cpp and Sketch3dSnaps.cpp,
// the geometry in Sketch3dDraw.hpp): Point, Line, Three-Point, Center and
// Tangent Arc, Fit Point and Control Point Spline. While one is active the
// left button is the tool's; the temporary XY, YZ or ZX plane through the
// anchor (the sketch's origin, then the last committed end), the Axis and
// Normal placement, Keep Axis, Keep Plane, the typed values (Offsets, Axis
// and Length, Radius, Arc Length) and the snaps decide where a click puts
// a point. A point placed is pending until its curve commits: each curve
// with its new points, references, relations and dimensions is one model
// edit and one undo step. Esc steps back one layer: a field's typed value,
// the pending points and chain, the tool, the selection. The plane rotates
// freely (mitcad#1661): rotation handles about the manipulator's axes,
// dragged or given an angle, and Reset; never stored.
//
// Fillet (mitcad#1548, Sketch3dFilletTool.cpp, the plain logic in
// Sketch3dFillet.hpp): a corner where two lines meet, picked by its point
// or its two lines, rounded with a typed Radius at once, or with the
// radius the pointer drags along the corner's bisector (a click or a
// release takes it), previewed live; each corner is one sketch3d.fillet
// and one undo step, the tool stays for the next corner.
//
// Editing (mitcad#1526, Sketch3dEditing.cpp, the plain logic in
// Sketch3dEdit.hpp): relations and dimensions added from the selection,
// edited and removed in the 3D Sketch Palette; Fix and Unfix, Detach,
// Attach and Replace Reference (a source picked in the view); dragging a
// point, a curve or a spline's tangent handle on a chosen plane or axis
// through it, previewed live, taken as one undo step on release, taken
// back by Esc; Delete naming what goes with a point still used.

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QObject>
#include <QPointF>
#include <QString>
#include <QStringList>

#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_Trsf.hxx>

#include "../framework/Selection.hpp"
#include "Sketch3dDraw.hpp"
#include "Sketch3dEdit.hpp"
#include "Sketch3dFillet.hpp"
#include "Sketch3dModel.hpp"

class QEvent;
class QMouseEvent;

namespace mitcad {

class OcctViewer;
class SketchHost;

namespace sketch {

class Sketch3dOverlay;

// The drawing tools of a 3D sketch, its Fillet (mitcad#1548), and the
// plane-bound tools that draw on the active XY, YZ or ZX plane through
// their first point (mitcad#1550): circles, the ellipse, rectangles,
// polygons and the text.
enum class Sketch3dTool {
  None,
  Point,
  Line,
  ArcThreePoint,
  ArcCenter,
  ArcTangent,
  SplineFit,
  SplineControl,
  Fillet,
  CircleCenter,
  CircleTwoPoint,
  CircleThreePoint,
  Ellipse,
  RectangleTwoPoint,
  RectangleCenter,
  RectangleThreePoint,
  PolygonCircumscribed,
  PolygonInscribed,
  PolygonEdge,
  Text
};

// A plane-bound tool (mitcad#1550): its shape lies in the active plane
// through its first point.
bool isShapeTool(Sketch3dTool tool);

// What a tool shows over the view, in local coordinates.
struct Sketch3dPreview {
  enum class Style {
    Draw,         // what the next click makes
    Construction, // the same as construction geometry
    Guide,        // a drop line, a radius
    Plane,        // the temporary plane through the anchor
    Axis,         // an axis through the anchor (`axis`: 0, 1, 2)
    Ring,         // a rotation handle about an axis (mitcad#1661)
  };
  struct Path {
    std::vector<Xyz> points;
    Style style = Style::Draw;
    bool closed = false;
    int axis = -1;
    bool active = false; // the axis the placement follows
  };
  std::vector<Path> paths;
  std::vector<Xyz> placed;  // pending points
  std::optional<Xyz> hover; // where the next click puts a point
  bool snapped = false;
  QString badge;
  bool conflict = false;
  QString unstable; // the hint while the pointer's place is unstable
};

class Sketch3dController : public QObject {
  Q_OBJECT

public:
  Sketch3dController(SketchHost& host, OcctViewer& viewer, QObject* parent = nullptr);
  ~Sketch3dController() override;

  // What the main window gives the snaps: where an occurrence places its
  // component in the design, and the names of features and components for
  // the badges.
  struct Hooks {
    std::function<gp_Trsf(const QString& occurrence)> placementOf;
    std::function<QString(const QString& uid)> featureName;
    std::function<QString(const QString& occurrence)> componentName;
    // The names the palette's words give sources (mitcad#1652): empty
    // where there is none, never an id.
    SourceNames names;
    // Paints places in the view as a notice's places are painted while
    // the pointer is over it, for `source` (mitcad#1652: "3D Sketch
    // Palette", the row under the pointer with its source; mitcad#1653:
    // "3D sketch snap", the source a tool's snap is on); none clears them.
    std::function<void(const QString& source, const QJsonArray& places)> paintPlaces;
  };
  void setHooks(Hooks hooks) { m_hooks = std::move(hooks); }
  const SourceNames& sourceNames() const { return m_hooks.names; }

  // Opens the sketch `uid`: reads it, shows the overlay. Logs "3D sketch
  // frame in the design: origin (…), x axis (…), normal (…)" and the
  // status line (statusLog()).
  void enter(const QString& uid);
  void leave();
  bool isActive() const { return !m_uid.isEmpty(); }
  const QString& uid() const { return m_uid; }
  const Sketch3dModel& model() const { return m_model; }
  // Reads the sketch again after the model changed; logs the status when
  // it changed. A tool's chain and anchor whose points went are dropped.
  void reload();
  // What the view would pick under the pointer: one of the sketch's points
  // or curves is described in the palette and beside the pointer (a
  // reference curve with its source) in words, logged "3D sketch hover:
  // <the agents' text with ids> | shown: <the words>".
  void setHover(const SelectionItem& item);
  const QString& hoverText() const { return m_hoverText; }
  // The selected entities of the sketch, described in words (one line
  // each), and the same for agents and the log, with the ids.
  QStringList selectionTexts() const;
  QStringList selectionDetails() const;
  // The 3D Sketch Palette's row under the pointer (mitcad#1652): a point,
  // a curve, a relation, a dimension or a linked reference record by its
  // id, none for none. The view paints it and the source it uses
  // (entityPlaces) until the pointer leaves the row; logged "3D sketch
  // palette hover: <id>: <places>" ("none" when it ends).
  void setPaletteHover(const QString& id);
  const QString& paletteHover() const { return m_paletteHover; }
  // The overlay drawn again; the places of the points, curves, frame and
  // badges logged when they moved (also where the view never paints:
  // Windows session 0).
  void refreshView();

  // The document's length unit and its size in mm.
  QString unit() const;
  double millimetres() const;
  // A local point of the sketch in the design: the frame, then where the
  // view shows its component (once).
  gp_Pnt designPoint(const Xyz& local) const;
  gp_Pnt designOrigin() const;
  // A local direction of the sketch in the design.
  gp_Pnt designAlong(const Xyz& local, double length) const;
  // A point of the design in the sketch's local coordinates.
  Xyz localPoint(const gp_Pnt& design) const;
  const OcctViewer& viewer() const { return m_viewer; }
  const QPointF& pointer() const { return m_pointer; }
  bool pointerInView() const { return m_pointerInView; }

  // --- Drawing tools (Sketch3dTools.cpp)

  // Starts a tool (the SKETCH tab's Point, Line, arcs and splines run it
  // in a 3D sketch); logs "3D sketch tool <name>: started".
  void startTool(Sketch3dTool tool);
  // Ends the tool, dropping what it has pending.
  void stopTool();
  Sketch3dTool tool() const { return m_tool; }
  bool toolActive() const { return m_tool != Sketch3dTool::None; }
  // "Line", "Tangent Arc" (translated).
  QString toolName() const;
  static QString toolName(Sketch3dTool tool);
  // The SKETCH tab command that runs a tool in a 3D sketch
  // (`sketch.line`), and back; None for another id.
  static QString toolCommand(Sketch3dTool tool);
  static Sketch3dTool toolForCommand(const QString& id);
  // Esc: the field typed in goes back to its measurement, else the
  // pending points and chain go (the tool stays), else the tool ends;
  // false when no tool was active (the main window clears the selection).
  bool cancel();
  // Enter: the typed values checked and the pending segment, arc, point
  // or spline committed once when it is valid; false when nothing was.
  bool confirm();
  // Before an undo: what is pending goes, so that no anchor or chain
  // survives on a point the undo takes away.
  void clearPending();
  // What to do next, for the palette and the hint line (translated).
  QString toolHint() const;
  // The snap under the pointer ("Linked Point: vertex …"), empty when none.
  QString snapText() const;
  // Why the next click would be refused (a typed value, a conflict), empty
  // when it would not; whether it names On Sketch Plane (Release from
  // Plane is then offered for that point).
  const QString& problem() const { return m_problem; }
  const QString& planeHeldPoint() const { return m_planeHeldPoint; }
  // Release from Plane of the point the problem names (one undo step).
  void releaseHeldPoint();
  Sketch3dPreview preview() const;

  // The temporary manipulator and the tool's options.
  const Placement& placement() const { return m_place; }
  void setPlane(Plane3 plane);
  // Plane, Axis (along `axis`) or Normal (holding the in-plane place the
  // pointer has now).
  void setPlacementMode(Placement::Mode mode, Axis3 axis = Axis3::X);

  // The free rotation of the placement plane (mitcad#1661): rotation
  // handles about the manipulator's X, Y and Z, dragged (whole degrees) or
  // given an angle in a Rotate field. The rotated plane, its axes and its
  // normal guide the placement; the typed Delta X, Y, Z, Axis and Length
  // and Keep Axis are along its axes (stored as relations and dimensions
  // along those directions), and the plane-bound shapes lie on it (stored
  // with its frame). The sketch's own frame never changes. Every drawing
  // tool takes it but Fillet, which places no point.
  bool offersRotation() const;
  bool planeRotated() const { return m_place.rotated(); }
  // Turns the manipulator about its own axis by `angle` radians; logs "3D
  // sketch plane rotated 30° about X: on the rotated XY plane; frame x
  // (…), y (…), z (…)".
  void rotatePlane(Axis3 about, double angle);
  // An angle typed in a Rotate field (a number in degrees, an expression,
  // a parameter): the plane rotated by it. Empty when it was; else why not
  // (the field shows it).
  QString rotatePlaneBy(Axis3 about, const QString& text);
  // Reset: the sketch's XY, not rotated, at the anchor, On Plane; logs "3D
  // sketch plane reset: on the XY plane at (…)".
  void resetPlane();
  // Show Plane and Show Handles: the temporary plane's square and the
  // rotation handles drawn or not (the placement is the same).
  bool showPlane() const { return m_showPlane; }
  void setShowPlane(bool show);
  bool showHandles() const { return m_showHandles; }
  void setShowHandles(bool show);
  // The handle dragged now and how far it has turned (radians).
  std::optional<std::pair<Axis3, double>> rotationDrag() const;
  // The manipulator's keys (mitcad#1661), commands of their own so that
  // the Shortcuts list lists them and they can be changed: Tab and
  // Shift+Tab cycle the plane XY, YZ, ZX (backwards) as the plane buttons
  // do, Up Arrow switches between On Plane and Normal as Place does. They
  // act while a drawing tool other than Fillet is active; a field with the
  // keyboard keeps them (MainWindow::runSketch3dKey). `key` names the key
  // in the log: "3D placement plane: YZ (Tab)", "3D placement: Normal
  // (Up)".
  static constexpr const char* kNextPlaneCommand = "sketch.next_plane";
  static constexpr const char* kPreviousPlaneCommand = "sketch.previous_plane";
  static constexpr const char* kNormalCommand = "sketch.placement_normal";
  bool takesPlacementKeys() const { return toolActive() && m_tool != Sketch3dTool::Fillet; }
  void cyclePlane(bool backwards, const QString& key);
  void toggleNormal(const QString& key);
  // A command's key as the Shortcuts list has it now (native text), for
  // the hint line and the tooltips; empty when it has none.
  void setKeyText(std::function<QString(const QString& id)> keyText) { m_keyText = std::move(keyText); }
  QString keyText(const char* id) const {
    return m_keyText ? m_keyText(QString::fromLatin1(id)) : QString();
  }
  // What the manipulator's keys do, for the hint line: "Tab or Shift+Tab
  // changes the plane, Up switches to Normal." (translated); empty when
  // they have no keys.
  QString placementKeysHint() const;
  bool keepAxis() const { return m_keepAxis; }
  void setKeepAxis(bool keep);
  bool keepPlane() const { return m_keepPlane; }
  void setKeepPlane(bool keep);
  bool construction() const { return m_construction; }
  void setConstruction(bool construction);
  const SegmentInput& input() const { return m_input; }
  // Offsets or Axis and Length: the other mode's typed values are dropped.
  void setInputMode(SegmentInput::Mode mode);
  void setInputAxis(Axis3 axis);
  void setInputNegative(bool negative);
  // Which fields the tool shows now: dx, dy, dz, length, radius,
  // arc_length; whether it offers Axis and Length (Line), Keep Axis
  // (Line) and Keep Plane (arcs).
  QStringList fieldIds() const;
  bool offersAxisLength() const { return m_tool == Sketch3dTool::Line; }
  bool offersKeepAxis() const { return m_tool == Sketch3dTool::Line; }
  bool offersKeepPlane() const;
  // A field typed (the model evaluates it: units, parameters); empty text
  // makes it a measurement again.
  void setField(const QString& id, const QString& text);
  const TypedValue& field(const QString& id) const;
  // An untouched field's live measurement, in the document's unit.
  QString measured(const QString& id) const;
  // The field that has the keyboard (Esc takes its value back first).
  void setFocusField(const QString& id) { m_focusField = id; }
  const QString& focusField() const { return m_focusField; }
  // Gives the keyboard back to the view.
  void focusView();
  // "Delta X", "Arc Length" (translated).
  static QString fieldLabel(const QString& id);
  // A Center Point Arc's normal: the active plane's, or the direction of
  // a line, a straight edge or an axis, or the normal of a planar face,
  // picked after Pick Normal (a direction, taken once: the arc's normal
  // starts there and is free unless Keep Plane binds it to an axis).
  void startNormalPick();
  void useActivePlaneNormal();
  bool pickingNormal() const { return m_pickingNormal; }
  QString arcNormalText() const;

  // --- Editing (Sketch3dEditing.cpp, mitcad#1526)

  // The sketch's selected points and curves.
  EditSelection editSelection() const;
  // The relation or dimension the selection takes now, or why not.
  EditCommand relationFor(Relation3 relation) const;
  EditCommand dimensionFor(Dimension3 dimension) const;
  // Adds or takes it away as one undo step ("Coincident in 3DSketch1");
  // false with the reason shown. Logged "3D sketch edit coincident: …".
  bool applyRelation(Relation3 relation);
  bool applyDimension(Dimension3 dimension);
  // The SKETCH tab's Sketch Dimension and Fix on the selection.
  bool applyAutoDimension();
  bool toggleFix();
  // A dimension's value typed (a number, an expression or a parameter's
  // name: set_dimension), made driven or driving (set_driven); relations
  // and dimensions removed by their ids.
  bool setDimensionValue(const QString& id, const QString& text);
  bool setDriven(const QString& id, bool driven);
  bool removeItems(const QStringList& ids);
  // Delete of the selection: points that curves or relations still use
  // are not deleted at once; what would go with them is named
  // (pendingDelete), and deleteWithDependents() takes it all.
  bool deleteSelection();
  bool deleteWithDependents();
  const QStringList& pendingDelete() const { return m_pendingDelete; }
  const QStringList& pendingDependents() const { return m_pendingDependents; }
  // Detach of a linked reference record (an attachment k…, a reference
  // curve c…): its points come off it.
  bool detachRecord(const QString& record);

  // Picking a source in the view: Attach of the selected point (a point
  // source attaches it, a curve source puts it on the curve), or Replace
  // Reference of a record (a source of its kind). The next click on a
  // body vertex or edge, another sketch's point or curve, an Intersection
  // Curve or the origin takes it; Esc ends it.
  //
  // Plane (mitcad#1661): the plane of a Symmetric or On Plane relation for
  // the selection its button had: a planar face, a construction plane, or
  // a point or curve of another sketch (its plane) clicked in the view, or
  // a plane of the sketch's frame from the palette (takeFramePlane).
  enum class SourcePick { None, Attach, Replace, Plane };
  void startAttach();
  void startReplace(const QString& record);
  void startPlanePick(Relation3 relation);
  // The frame plane square to axis 0 (YZ), 1 (ZX) or 2 (XY) for the
  // relation waiting for its plane.
  void takeFramePlane(int axis);
  void cancelSourcePick();
  SourcePick sourcePick() const { return m_sourcePick; }
  const QString& sourcePickItem() const { return m_sourcePickItem; }
  // The source under the pointer while picking; none when nothing is near.
  std::optional<SnapTarget> sourceSnap() const;
  // A tool or a source pick takes the view's clicks and picks body and
  // other sketches' geometry; Fillet picks the sketch's own only.
  bool picksSources() const {
    return (toolActive() && m_tool != Sketch3dTool::Fillet) || m_sourcePick != SourcePick::None;
  }

  // Dragging a point, a curve or a spline's tangent handle (with no tool):
  // on the drag plane XY, YZ or ZX, or along X, Y or Z, through the place
  // grabbed. The relations hold, the pointer is a soft goal; release takes
  // it as one undo step (`Drag in 3DSketch1`), Esc takes it back.
  const Placement& dragPlacement() const { return m_dragPlace; }
  void setDragPlane(Plane3 plane);
  void setDragAxis(Axis3 axis);
  bool dragging() const { return m_drag.active; }
  bool cancelDrag();
  // What the drag does now (translated): "Dragging p3 on the XY plane:
  // it stops 2.50 mm short of the pointer".
  QString dragText() const;
  // The drag's preview in local coordinates: the curves as they would be,
  // the grabbed place reached and the pointer's target.
  struct DragPreview {
    std::vector<std::vector<Xyz>> curves;
    std::vector<Xyz> points;
    std::optional<Xyz> reached;
    std::optional<Xyz> target;
    bool shortOf = false;
    bool refused = false;
  };
  DragPreview dragPreview() const;
  // What the last edit or pick says (a refusal, a prompt), translated.
  const QString& editNotice() const { return m_editNotice; }

  // --- Clipboard (Sketch3dClipboard.cpp, mitcad#1661)

  // Copy and Cut of the selected points and curves: the model's
  // `sketch_fragment` answer (z, the relations and dimensions among them,
  // the linked references they use) on the clipboard, as a 2D sketch's
  // copy is (SketchClipboard.hpp); Cut then deletes them as one undo step
  // (`Cut in 3DSketch1`). Paste runs `sketch3d.paste` at once as one undo
  // step (`Paste in 3DSketch1`): a 3D sketch's copy where it was, a 2D
  // sketch's onto the active plane with its base point at the anchor; the
  // pasted points and curves are selected and what was left out is said
  // in words. False, with the reason shown, when nothing was done.
  bool copySelection();
  bool cutSelection();
  bool paste();

signals:
  // The sketch was read again, or the hover or the selection changed: the
  // palette follows.
  void changed();
  // A tool started or ended: the main window's pick filter and actions.
  void toolChanged();
  // The tool's state (snap, typed values, hint, problem) changed.
  void toolStateChanged();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void logPlaces();
  // The overlay drawn again (its class is Sketch3dController.cpp's).
  void updateOverlay();
  // Where the overlay draws a badge of `text` beside the sketch's point
  // `local`, for UI tests (mitcad#1592): "at x0,y0 to x1,y1, inside the view
  // x0,y0 to x1,y1" in the window ("cut by the view" when it is not).
  QString badgePlace(const Xyz& local, const QString& text) const;

  // Tools (Sketch3dTools.cpp).
  bool toolMouse(QEvent* event);
  void pointerAt(const QPointF& position);
  void updateHover();
  // The source a snap is on painted in the view while it is the snap
  // (snapPlaces; mitcad#1653), logged "3D sketch snap places: <places>"
  // when it changes; a snap with none, or no snap, clears it.
  void paintSnap(const SnapTarget& snap);
  void place();
  void finishChain();
  bool commitSteps(const QJsonObject& first, const std::function<QJsonArray(const QJsonObject&)>& then,
                   const QString& label, QJsonObject* result);
  void commitPoint(const PlacedPoint& point);
  void commitLine(const PlacedPoint& start, const PlacedPoint& end);
  void commitArc();
  void commitSpline(bool closed);
  // A plane-bound shape's clicks as one model edit (mitcad#1550,
  // Sketch3dShapes.cpp); its typed sizes, the hover place they give, its
  // preview, measurements, problems and hints.
  void commitShape();
  int shapeClicks() const;
  std::optional<Xyz> shapeHover(const Xyz& at) const;
  void addShapePreview(Sketch3dPreview& out, const Xyz& at) const;
  QString shapeMeasured(const QString& id) const;
  QString shapeProblem(const Xyz& at) const;
  QString shapeHint() const;
  QStringList shapeFields() const;
  int sides() const;
  void afterCommit(const QString& endPoint);
  PlacedPoint fromPoint() const;
  bool usesOffsets() const;
  QString computeProblem();
  PlacedPoint fromPointOfChain() const;
  Xyz arcNormal() const;
  // Pick Normal's click (Sketch3dSnaps.cpp): a line, a straight edge, an
  // axis or a planar face under the pointer.
  void pickNormal();
  std::optional<Xyz> localDirection(const gp_Vec& design) const;
  void resetInput();
  void toolLog(const QString& text) const;
  std::optional<Ray3> rayAt(const QPointF& position) const;
  QPointF onScreen(const Xyz& local) const;
  int undoDepth() const;

  // The manipulator's rotation (Sketch3dTools.cpp, mitcad#1661).
  // The placement the tool places by (the manipulator's, rotated or not).
  Placement toolPlacement() const;
  // The plane a plane-bound shape is drawn on: the active plane of the
  // manipulator's frame (mitcad#1661).
  ShapePlane shapePlane() const { return ShapePlane(m_place.plane, m_place.frame); }
  // The manipulator's size (mm) for its plane's half side on the screen at
  // `anchor`, whatever the zoom; the rings are a little larger.
  double manipulatorSize(const Xyz& anchor) const;
  double ringRadius(const Xyz& anchor) const;
  // The rotation handle within a few pixels of `position`; none when the
  // handles are hidden, not offered or not near.
  std::optional<Axis3> ringAt(const QPointF& position) const;
  // A press on a ring, its drag (the plane turns) and release; false when
  // the event is not the handles'.
  bool rotationMouse(QEvent* event);
  // Ends a handle's drag: taken, or the plane back as it was.
  void endRotationDrag(bool keep);
  // Where each handle can be grabbed, in the window, for UI tests: "3D
  // sketch rotation handle X at x,y" (once per change).
  void logHandles();

  // Snaps (Sketch3dSnaps.cpp).
  std::vector<SnapTarget> snapCandidates(const QPointF& pointer, const Xyz& from,
                                         const std::optional<Xyz>& placed);
  void addPickedCandidates(const SelectionItem& item, const QPointF& pointer, std::vector<SnapTarget>& out);
  // A curve's points in the design (placed), sampled; cached until the
  // model changes.
  const std::vector<gp_Pnt>& polylineOf(const SelectionItem& item);
  QJsonObject withOccurrence(QJsonObject source, const SelectionItem& item) const;
  QString sourceOwnerText(const SelectionItem& item) const;

  // Fillet (Sketch3dFilletTool.cpp, mitcad#1548).
  void updateFilletHover();
  // A press on a corner picks it at once, so a drag from it sets the
  // radius; a release after a drag, or a click on a picked corner, rounds it.
  void filletPress();
  void filletRelease(bool click);
  bool confirmFillet();
  void commitFillet();
  // The pending corner and first line go; false when there were none.
  bool dropFillet();
  // The radius the next click takes: the typed one, else the dragged one.
  std::optional<double> filletRadius() const;
  QString filletHint() const;
  void addFilletPreview(Sketch3dPreview& out) const;

  // Clipboard (Sketch3dClipboard.cpp): `items` on the clipboard; false,
  // with the model's reason shown, when it refused them.
  bool copyToClipboard(const QStringList& items, bool cut);

  // Editing (Sketch3dEditing.cpp).
  bool editMouse(QEvent* event);
  bool runEdit(const QJsonArray& commands, const QString& label, const QString& log);
  void setEditNotice(const QString& notice);
  // A press on a tangent handle, a point or a curve of the sketch.
  bool grabAt(const QPointF& position);
  void dragTo(const QPointF& position);
  void applyDragPreview();
  void commitDrag();
  void updateSourcePick();
  // A Symmetric or On Plane relation with its plane picked (mitcad#1661):
  // the plane clicked in the view, or the plane given.
  void takePickedPlane();
  void takePlane(const QJsonObject& plane);
  void takeSource();
  void forwardClick(const QPointF& release, Qt::KeyboardModifiers modifiers);

  SketchHost& m_host;
  OcctViewer& m_viewer;
  Hooks m_hooks;
  std::function<QString(const QString& id)> m_keyText; // mitcad#1661
  QString m_uid;
  Sketch3dModel m_model;
  gp_Trsf m_placement;
  Sketch3dOverlay* m_overlay = nullptr;
  QString m_hoverText;
  QString m_paletteHover;        // the palette's row under the pointer
  QJsonArray m_paletteHoverPlaces; // what the view paints for it
  QJsonArray m_snapPlaces;         // the snap's source the view paints (mitcad#1653)
  QString m_loggedStatus;
  QString m_loggedPlaces;
  QString m_loggedHover;
  QPointF m_pointer;
  bool m_pointerInView = false;

  // The tool's state.
  Sketch3dTool m_tool = Sketch3dTool::None;
  Placement m_place; // the temporary manipulator
  bool m_keepAxis = false;
  bool m_keepPlane = false;
  bool m_construction = false;
  SegmentInput m_input;
  TypedValue m_radius;
  TypedValue m_arcLength;
  // A shape's typed sizes (mitcad#1550): the first (diameter, major
  // radius, width) and the second (minor radius, height), the polygon's
  // sides, the text and its height.
  TypedValue m_size;
  TypedValue m_size2;
  TypedValue m_sidesField;
  TypedValue m_textField;
  TypedValue m_textHeight;
  std::optional<Xyz> m_textFrame; // a text's frame corner, dragged
  // A text's frame while it is dragged (mitcad#1550): the place under the
  // press, and whether the pointer has gone past a click from it; the
  // preview frames it to the pointer, as the planar Text tool does.
  std::optional<Xyz> m_textPress;
  bool m_textDragging = false;
  std::vector<PlacedPoint> m_pending;
  QString m_chainPoint; // the committed end the next segment starts from
  QString m_chainCurve; // the curve a Tangent Arc continues
  Xyz m_anchor{};
  QString m_anchorPoint;
  std::optional<Xyz> m_placed; // the pointer's last stable place
  std::optional<Xyz> m_hoverAt;
  SnapTarget m_hoverSnap;
  bool m_unstable = false;
  QString m_problem;
  QString m_planeHeldPoint;
  QString m_focusField;
  bool m_leftPressed = false;
  QPointF m_pressAt;
  bool m_hasPointer = false;
  QString m_loggedSnap;
  QString m_loggedProblem;
  QHash<QString, std::vector<gp_Pnt>> m_polylines;
  QHash<QString, QJsonArray> m_bodyEdges;
  bool m_pickingNormal = false;
  std::optional<Xyz> m_arcNormal; // picked; else the active plane's
  QString m_arcNormalWhat;  // the log's, with the ids
  QString m_arcNormalWords; // as shown (mitcad#1653)

  // The rotation handles' state (mitcad#1661).
  struct RotationDrag {
    std::optional<Axis3> pressed; // the ring under the last press
    bool active = false;          // dragged past a click: the plane turns
    Axis3 axis = Axis3::X;
    Xyz center{};                 // the anchor it turns about
    Xyz grabbed{};                // where the ring's plane was grabbed
    Frame3 start = sketchFrame(); // the frame before the drag
    double angle = 0.0;           // turned so far, whole degrees
  };
  RotationDrag m_rotation;
  std::optional<Axis3> m_ringHover; // the handle under the pointer
  bool m_showPlane = true;
  bool m_showHandles = true;
  QString m_loggedHandles;

  // Fillet's state (mitcad#1548).
  std::optional<FilletCorner3> m_filletCorner; // picked: its radius is typed or dragged
  std::optional<FilletCorner3> m_filletHover;  // the corner the next click picks
  QString m_filletLine;                        // the first of two lines picked
  QString m_filletHoverLine;                   // a line under the pointer
  std::optional<double> m_filletDragged;       // the radius the pointer gives
  bool m_filletPressPicked = false;            // the last press picked the corner
  QString m_loggedFillet;

  // Editing's state (mitcad#1526).
  struct Drag {
    enum class Kind { None, Point, Curve, Handle };
    Kind kind = Kind::None;
    QString entity;            // the point, the curve or the handle's spline
    int end = 0;               // a handle's end
    QStringList points;        // the points pulled
    std::vector<Xyz> starts;   // where they were
    Xyz grabbed{};             // the place pressed (a handle's tip)
    Xyz endPoint{};            // a handle's end point
    QPointF pressAt;
    Qt::KeyboardModifiers modifiers;
    bool pressed = false;
    bool active = false;       // moved past a click
    std::optional<Xyz> target; // where the pointer puts the grabbed place
    QJsonObject preview;       // the last preview's answer
    bool refused = false;
    bool pending = false;      // a preview waits for the timer
  };
  Drag m_drag;
  Placement m_dragPlace;
  bool m_forwarding = false;
  QStringList m_pendingDelete;
  QStringList m_pendingDependents;
  SourcePick m_sourcePick = SourcePick::None;
  QString m_sourcePickItem;
  // The relation and selection a plane pick is for (mitcad#1661).
  Relation3 m_planeRelation = Relation3::Symmetric;
  EditSelection m_planeSelection;
  SnapTarget m_sourceSnap;
  bool m_sourceSnapped = false;
  QString m_editNotice;
  QString m_loggedDrag;
};

} // namespace sketch
} // namespace mitcad
