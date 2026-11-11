// SPDX-License-Identifier: MIT
#pragma once

// Sketch mode (U2): the sketch being edited, its drawing tools, snapping,
// the constraints and dimensions drawn over the view, dragging with the
// solver, and the values typed in the view. The model does the geometry
// (`sketch.*` commands); this class turns mouse and keyboard input into
// those commands, one undo step per user action.

#include <array>
#include <functional>
#include <limits>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QHash>
#include <QJsonObject>
#include <QJsonValue>
#include <QMouseEvent>
#include <QObject>
#include <QPointF>
#include <QPointer>
#include <QPolygonF>
#include <QRectF>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <QVector>
#include <gp_Trsf.hxx>

#include "../framework/CommandSession.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/ValueStep.hpp"
#include "AutoConstraint.hpp"
#include "SketchModel.hpp"
#include "SketchSnap.hpp"

class QLabel;
class QLineEdit;
class QWidget;

namespace mitcad {

class OcctViewer;
class CommandRegistry;

// What sketch mode needs from the main window besides a command host.
class SketchHost : public CommandHost {
public:
  virtual const Selection& selection() const = 0;
  virtual void setSelection(const Selection& items) = 0;
  // A click on items, with the selection rules (Ctrl and Shift add or
  // remove).
  virtual void pick(const Selection& items, Qt::KeyboardModifiers modifiers) = 0;
  virtual void showError(const QString& message) = 0;
  // A message on the status line at its level (mitcad#1051; the main
  // window's DocumentHost::showStatus).
  virtual void showStatus(const QString& message, NoticeLevel level) = 0;
  // An error raised by a source that lasts only while it is open
  // (mitcad#1621, MessageEntry::source): `sketch/editor` (a value editor's
  // own check), `sketch/tool` (the running tool), `sketch` (sketch mode).
  // closeMessageSource takes it from the status line and the message band
  // when that source closes; the history keeps it.
  virtual void showSourceError(const QString& message, const QString& /*source*/) { showError(message); }
  virtual void closeMessageSource(const QString& /*source*/) {}
  // Starts a command of the registry (an edit panel opened by a
  // double-click).
  virtual void trigger(const QString& id) = 0;
  // Whether sketch mode's selection filter (mitcad#217) lets a click take
  // an item of the edited sketch that sketch mode itself picks: its
  // dimensions and constraints, and its curves and points a press would
  // drag. `expanded`: Shift is held.
  virtual bool sketchClickable(const SelectionItem& /*item*/, bool /*expanded*/) const { return true; }
  // Whether the input a command's panel picks for now takes the sketch's
  // x and y axes (InputDef::sketchAxes, mitcad#876: Mirror's line), and
  // the axes it holds (their axisPick() ids), which the overlay draws as
  // selected: the view does not show the axes.
  virtual bool sketchAxesPickable() const { return false; }
  virtual QStringList sketchAxisPicks() const { return {}; }
  // Whether the input a command's panel picks for now takes points with
  // the sketch's snaps (InputDef::sketchSnaps, mitcad#1563: Move/Copy's
  // Origin and Target Point), and the picks those inputs hold (point ids
  // and placePick ids), in the inputs' order.
  virtual bool sketchSnapsPickable() const { return false; }
  virtual QStringList sketchSnapPicks() const { return {}; }
  // The running command's handle in the sketch (CommandDef::sketchHandle,
  // mitcad#1563), and a drag of it setting its value inputs (mm).
  virtual std::optional<SketchHandle> sketchHandle() const { return std::nullopt; }
  virtual void dragSketchHandle(const SketchHandle& /*handle*/, double /*x*/, double /*y*/) {}
  // Handles the picks of the view that were handed over by a queued call
  // and not delivered yet, so that the selection and a command that starts
  // next are in the order of the clicks (mitcad#488).
  virtual void deliverPendingPicks() {}
  // Where the view shows the sketch's component (mitcad#505): the
  // placement of the occurrence sketch mode edits the sketch at. The
  // `sketch` query's frame is in the component's coordinates, the view's
  // in the design's; sketch mode applies this once to the frame.
  virtual gp_Trsf sketchPlacement(const QString& /*uid*/) const { return gp_Trsf(); }
  // The occurrence whose placement sketch mode edits; own-component body
  // preselection must come from this occurrence as well.
  virtual QString sketchOccurrence(const QString& /*uid*/) const { return QString(); }
  // A query that may take long read off the UI thread (mitcad#1656: the
  // Dimension tool's body geometry out of the plane in a large assembly):
  // `read` gets the answer (on the model's worker), then `done` runs on
  // this thread with whether it was read, or the query's error; neither
  // runs once `context` is gone. Not read (false, no error): the read gave
  // way to the user's next action and is asked for again later. False when
  // it cannot start now (the model computes). The default reads it at once.
  // Whether the model can be read now, a read in the background giving way
  // for it (the default: when the model computes nothing).
  virtual bool modelReadableNow() { return !modelBusy(); }
  using BackgroundRead = std::function<void(const QJsonValue& answer)>;
  using BackgroundDone = std::function<void(bool read, const QString& error)>;
  virtual bool queryInBackground(const QJsonObject& query, const BackgroundRead& read, QObject* context,
                                 const BackgroundDone& done) {
    Q_UNUSED(context);
    QString error;
    try {
      read(this->query(query));
    } catch (const std::exception& failure) {
      error = QString::fromUtf8(failure.what());
    }
    done(error.isEmpty(), error);
    return true;
  }
};

namespace sketch {

class SketchOverlay;
class SketchTool;
struct ToolMenuEntry;

// A value typed in the view while drawing (heads-up input).
struct FieldSpec {
  enum class Kind { Length, Angle, Count, Text };
  FieldSpec() = default;
  // A constructor, not an aggregate: the tools' brace lists leave the step
  // minimum out without a missing-initializer warning.
  FieldSpec(QString fieldId, QString fieldLabel, Kind fieldKind = Kind::Length,
            double minimum = -std::numeric_limits<double>::infinity(), bool aboveMinimum = false)
      : id(std::move(fieldId)), label(std::move(fieldLabel)), kind(fieldKind), stepMinimum(minimum),
        stepAboveMinimum(aboveMinimum) {}
  QString id;
  QString label;
  Kind kind = Kind::Length;
  // The smallest value Up, Down and the wheel step it to (mitcad#1254,
  // framework/ValueStep.hpp), in mm (a length): above it with
  // stepAboveMinimum (a circle's diameter above 0).
  double stepMinimum = -std::numeric_limits<double>::infinity();
  bool stepAboveMinimum = false;
};

// Entities that a picking tool or a drag accepts.
enum EntityFilter : unsigned {
  PickPoints = 1u << 0,
  PickCurves = 1u << 1,
  PickAll = PickPoints | PickCurves,
  // Also the body edges and vertices in the sketch plane (mitcad#256), by
  // their BodyGeometry ids; the sketch's own entities win ties.
  PickBody = 1u << 2,
  // Also the sketch's origin and its x and y axes (mitcad#500): the origin
  // point or an axis line where the sketch has it, else its pick id
  // (originPick(), axisPick()); everything else wins ties.
  PickAxes = 1u << 3,
};

class SketchController;

// One user action: several model commands that undo as one step, with
// the constraints that keep new points where they snapped.
class SketchOp {
public:
  explicit SketchOp(SketchController& controller);
  ~SketchOp();
  SketchOp(const SketchOp&) = delete;
  SketchOp& operator=(const SketchOp&) = delete;

  // A `sketch.*` command of the edited sketch.
  QJsonObject command(const QString& name) const;
  // Runs a command; false, keeping the model's reason, when it is refused.
  bool run(const QJsonObject& command, QJsonObject* result = nullptr);
  // A command that may be refused, such as a constraint a snap or an
  // inference adds: the action goes on without it, and the reason is shown
  // as a hint when the action ends (mitcad#287).
  bool attempt(const QJsonObject& command);
  // An existing point where the cursor snapped to one (made at the origin
  // when it snapped there; a body vertex's projected point, mitcad#258), else
  // the position.
  QJsonValue pointInput(const Snap& snap);
  // The constraints that keep `point` where it snapped (on a body vertex or
  // at a body edge's middle: on the projected vertex or edge, projected
  // first).
  void constrain(const QString& point, const Snap& snap);
  void addConstraint(const QJsonObject& constraint);
  // A constraint the drawn geometry already satisfies (mitcad#1024), added
  // only when safe (`sketch.add_constraint` with `auto`: the others do not
  // imply it, the sketch solves and nothing moves); otherwise skipped
  // without a hint. Logged either way ("Constraint added automatically:
  // coincident p7 on c2 (quadrant)", "Auto constraint skipped: tangent c3
  // c5: redundant"); `why` names the source in parentheses. The same
  // constraint is tried once per action.
  bool addAuto(const QJsonObject& constraint, const QString& why = QString());
  // The tangents the new line or arc `curve` already satisfies at its ends
  // on circles and arcs (tangentCandidates), each added by addAuto.
  void addTangents(const QString& curve);
  // A dimension with a typed value; refused ones are left out.
  void addDimension(const QJsonObject& dimension, const QString& value);
  // A body edge or vertex in the sketch plane projected into the sketch,
  // linked, for the dimension or point the action makes from it; or the
  // entities of an existing projection of the same source that still match
  // it, and within one action the projection made before (mitcad#256,
  // mitcad#258). Empty when the model refused the projection (error()).
  ProjectedItem projectBody(const BodyItem& item);
  // The entity of a pick of the sketch's origin or an axis (mitcad#500),
  // made now when the sketch lacks it (`sketch.axis`); any other id as it
  // is. Empty when the model refused (error()).
  QString axisInput(const QString& pick);
  // The picks with those of the axes and origin made (axisInput); false
  // when one was refused.
  bool axisInputs(QStringList& picks);
  // The sketch after the commands so far.
  const SketchModel& model();
  // The model's reason for the last refusal, in words: a refusal for
  // conflicting constraints without its ids line (mitcad#1566), whose
  // blocking items the view paints until the next action.
  const QString& error() const { return m_error; }
  // Whether the last refusal was for conflicting constraints.
  bool conflicted() const { return m_conflicted; }
  // Names the undo step in place of its first command's label ("Cut in
  // Sketch1", mitcad#788): commit() then relabels even a single step.
  void setLabel(const QString& label) {
    m_label = label;
    m_relabel = true;
  }
  // Ends the action as one undo step and shows the result; or takes it
  // all back.
  bool commit();
  void rollback();

private:
  // Logs the constraints a new one replaced (mitcad#287) and keeps them
  // for the hint at the end.
  void noteImplied(const QJsonObject& command, const QJsonObject& answer);
  // Logs a scale of the sketch to its first dimension (mitcad#860) and
  // keeps it for the hint at the end.
  void noteScaled(const QJsonObject& answer);

  SketchController& m_c;
  int m_depth = 0;
  QString m_label;
  bool m_relabel = false; // setLabel() named the step
  QString m_error;
  bool m_conflicted = false;
  // What the action is to tell when it ends: refused attempts, constraints
  // a new one replaced (mitcad#287).
  QStringList m_notices;
  bool m_finished = false;
  bool m_stale = false;
  QHash<QString, ProjectedItem> m_projected; // by source, body and occurrence
  QSet<QString> m_autoTried; // addAuto's constraints so far (constraintText)
};

class SketchController : public QObject {
  Q_OBJECT

public:
  SketchController(SketchHost& host, OcctViewer& viewer, CommandRegistry& registry, QObject* parent = nullptr);
  ~SketchController() override;

  // --- Sketch mode
  void enter(const QString& uid);
  void leave();
  bool isActive() const { return !m_uid.isEmpty(); }
  const QString& uid() const { return m_uid; }
  const SketchModel& model() const { return m_model; }
  // Reads the sketch again after the model changed.
  void reload();
  // While a command's panel runs in the sketch, its picks are the view's:
  // no tools, drags or picking of constraints and dimensions; only the
  // sketch's axes for an input that takes them (mitcad#876).
  void setPaused(bool paused);
  // Turns the view to look at the sketch plane.
  void lookAt();
  // Logs where the sketch origin and axes are in the window, when that
  // changed ("Sketch view x,y x,y x,y" for (0, 0), (100, 0) and (0, 100)).
  void logView();
  // Logs where each inactive constraint's symbols are in the window, a line
  // for each ("Constraint k1 glyph at x,y (horizontal by c2)"), when the
  // view or the sketch moved them (mitcad#721, mitcad#723, mitcad#1578,
  // mitcad#1594).
  void logGlyphs();

  // --- Tools
  void startTool(std::unique_ptr<SketchTool> tool);
  void stopTool();
  // The icon of the tool command named `toolName` (as its tool names
  // itself), which the tool's cursor shows (mitcad#3).
  void setToolIcon(const QString& toolName, const QString& icon) { m_toolIcons.insert(toolName, icon); }
  SketchTool* tool() const { return m_tool.get(); }
  // Enter and Esc; false when sketch mode had nothing to do with them.
  bool confirm();
  bool cancel();
  // Takes back what the tool has half done (before an undo); the tool
  // stays.
  void resetTool();
  // Commands on the selection.
  void deleteSelection();
  void toggleConstruction();
  void toggleCenterline();
  // Convert to Sketch Geometry (mitcad#257): the selected projected curves
  // and points into ordinary free sketch geometry; whether the selection
  // holds any (the command is enabled only then).
  void convertProjected();
  bool selectionHasProjected() const;
  void updateConvertAction();
  void toggleFixed();
  void editDimension(const QString& id);
  // A horizontal or vertical distance made signed or unsigned, its point
  // kept where it is (`sketch.set_signed`, mitcad#409).
  void setDimensionSigned(const QString& id, bool on);
  // The parameter of dimension `id` when it is the sketch's own (a driving
  // dimension's, owned by the sketch, used by no other of its dimensions):
  // what Move to User Parameters moves (mitcad#1535); empty otherwise.
  QString ownParameter(const QString& id) const;
  // Move BuckleDiameter to User Parameters (mitcad#1535): the dimension's
  // own parameter becomes a user parameter, which the dimension uses at the
  // same value (`sketch.set_dimension` with its bare name), one undo step.
  void keepAsUserParameter(const QString& id);
  // The parameter of driving dimension `id` when the sketch owns it and it
  // has a name given to it (not an automatic `d<n>`): its value editor shows
  // `width = 20 mm` as text, whose `width = ` can be deleted, and Remove
  // Name offers to give it its automatic name back (mitcad#1577); empty
  // otherwise.
  QString namedParameter(const QString& id) const;
  // Remove Name (mitcad#1577): dimension `id`'s parameter gets its automatic
  // name back (`remove_parameter_name`), one undo step; the uses follow.
  void removeDimensionName(const QString& id);
  // Activates or deactivates constraints and dimensions (`sketch.set_active`,
  // mitcad#721). An activation the sketch does not solve with is refused
  // with the model's reason; nothing changes then.
  void setItemsActive(const QStringList& ids, bool on);
  // The selected constraints and dimensions of this sketch that are
  // inactive (`active` false) or active.
  QStringList selectedItems(bool active) const;
  // Copy, Cut and Paste of sketch geometry (mitcad#788, SketchClipboard.cpp):
  // the selected points, curves and texts with the constraints and
  // dimensions that reference only them or the origin and axes go to the
  // clipboard as a sketch fragment (the model's `sketch_fragment`); Cut then
  // deletes them. Paste is a tool that puts the fragment's base point where
  // a click or typed X and Y say (`sketch.paste`); Esc cancels it.
  void copySelection();
  void cutSelection();
  void paste();
  // Whether the clipboard holds a sketch fragment.
  bool canPaste() const;
  // Insert Point (mitcad#836, `sketch.insert_spline_point`): a point on the
  // spline `curve` where it is nearest to the view position `screen` (the
  // context menu's right click, a double-click on the curve). A fitted
  // spline gets a fit point between its neighbours, a spline by control
  // points a control point by knot insertion that keeps its shape. One
  // undo step; false, with the model's reason shown, when refused.
  bool insertSplinePoint(const QString& curve, const QPointF& screen);
  // The spline of this sketch, not projected, under a view position;
  // empty when there is none (Insert Point is offered only there).
  QString splineAt(const QPointF& screen) const;

  // --- Options of the sketch palette
  bool construction() const { return m_construction; }
  void setConstruction(bool on);
  bool gridSnap() const { return m_gridSnap; }
  void setGridSnap(bool on);
  // The grid's fixed spacing (mm), or 0: snap steps follow the zoom (U5).
  void setGridStep(double step) { m_gridStep = step; }
  bool showDimensions() const { return m_showDimensions; }
  void setShowDimensions(bool on);
  bool showConstraints() const { return m_showConstraints; }
  void setShowConstraints(bool on);
  // Filled regions are rebuilt after a geometry drag, while curves stay live.
  bool draggingGeometry() const { return m_dragging && m_dragDimension.isEmpty(); }
  bool showProfiles() const { return m_showProfiles; }
  void setShowProfiles(bool on);
  // The bodies shown cut at the sketch plane, without what is in front of
  // it (on its normal's side), so that the sketch is not hidden.
  bool hideAbove() const { return m_hideAbove; }
  void setHideAbove(bool on);
  // What a dimension shows (mitcad#231): its value, its parameter's name or
  // both; a UI preference (QSettings sketch/dimensionText), not in the design.
  enum class DimensionText { Value, Name, NameValue };
  DimensionText dimensionTextMode() const { return m_dimensionText; }
  void setDimensionTextMode(DimensionText mode);
  // The Dimension tool makes horizontal and vertical distances signed: the
  // offset from the first point picked, negative to the left or below it
  // (mitcad#409).
  bool signedDistances() const { return m_signedDistances; }
  void setSignedDistances(bool on);

  // --- Auto constraints while drawing (mitcad#189): the session of the
  // active drawing tool, which the palette's Auto constraint section shows.
  const AutoConstraintSession& autoConstraints() const { return m_auto; }
  void setAutoLink(AutoConstraintSession::Link link);
  void setAutoConstraint(const QString& type, bool on);
  // The session's reference among the entities that still exist; empty
  // before the first entity (or when undo took them all back).
  QString autoReference() const;
  // Whether the palette shows the Auto constraint section: a tool that
  // takes auto constraints has drawn an entity that still exists.
  bool autoConstraintsShown() const;
  // The reference and the checked types, when the next entity gets any.
  std::optional<AutoPlan> autoPlan() const;
  // The pointer is over the palette's section: the reference is
  // highlighted.
  void setAutoHover(bool on);
  bool autoHover() const { return m_autoHover; }
  // Adds the checked constraints between the reference and the entity
  // `made` in `op`. When some are refused, asks whether to keep the entity
  // with the others: false when the user cancels (the caller then rolls
  // back `op`).
  bool applyAutoConstraints(SketchOp& op, const QString& made, const QString& what);
  // A drawing tool made `made` (after its op's commit): the session's new
  // last entity.
  void recordAutoEntity(const QString& made);

  // --- For tools and the overlay
  SketchHost& host() { return m_host; }
  const SketchHost& host() const { return m_host; }
  OcctViewer& viewer() { return m_viewer; }
  const SketchOverlay& overlay() const { return *m_overlay; }
  QPointF toScreen(const V2& p) const;
  // A point `height` above the sketch plane (along its normal; negative
  // below), and the path of a body item out of the plane (mitcad#866), in
  // the view.
  QPointF toScreen(const V2& p, double height) const;
  QPolygonF bodyPath(const BodyItem& item) const;
  // The sketch plane point under a view position.
  std::optional<V2> sketchAt(const QPointF& screen) const;
  // The unrounded inverse of toScreen, for dimension/editor layout.
  std::optional<V2> sketchAtF(const QPointF& screen) const;
  // Millimetres per pixel near the sketch origin.
  double pixel() const;
  const Snap& snapped() const { return m_snap; }
  // Whether the snap is a command input's (mitcad#1563: Point to Point's
  // points, the drag of Move/Copy's handle), which the overlay marks as a
  // tool's.
  bool commandSnapping() const { return m_snapPicking || m_handleDrag.has_value(); }
  // The constraints that hold Move/Copy's moved geometry where it was
  // dropped on a point or a curve (mitcad#1645, `sketch.move`'s `hold`),
  // empty when it was not: for its handle (Translate), the point of the
  // `objects` at the handle's place, while X and Y Distance are still the
  // drop's `dx` and `dy` (mm); for Point to Point, the Origin Point `from`
  // (a point of the objects) where the Target Point pick `to` snapped, not
  // on the objects themselves.
  std::vector<QJsonObject> handleHold(const QStringList& objects, double dx, double dy) const;
  std::vector<QJsonObject> pickHold(const QStringList& objects, const QString& from, const QString& to) const;
  // A running command's handle in the sketch (CommandDef::sketchHandle,
  // mitcad#1563) as the view shows it: its square and the tips of its
  // arrows along the sketch's x and y axes (or along a pattern's two
  // directions from their own starts, without a square, mitcad#1640), the
  // arrows shown faint, and the part dragged ('x', 'y', 'b' for the
  // square, else 0).
  struct HandleShape {
    QPointF center;
    QPointF xFrom;
    QPointF yFrom;
    QPointF xTip;
    QPointF yTip;
    bool square = true;
    std::array<bool, 2> faint{};
    char dragged = 0;
  };
  std::optional<HandleShape> handleShape() const;
  const QPointF& cursor() const { return m_cursor; }
  bool cursorInView() const { return m_cursorValid; }
  // The pointer's place in sketch coordinates and the document's length
  // unit ("x 12.00, y -29.00 mm"; where a tool snaps it), while the pointer
  // is over the sketch plane; empty otherwise (mitcad#254).
  const QString& pointerText() const { return m_pointerText; }
  // The point or curve under a view position.
  QString entityAt(const QPointF& screen, unsigned filter = PickAll) const;
  // Highlights for picking tools.
  void setHover(const QString& entity);
  const QString& hover() const { return m_hover; }
  void setPicked(const QStringList& entities);
  const QStringList& picked() const { return m_picked; }
  QString hoverAnnotation() const { return m_hoverAnnotation; }
  // The constraints and dimensions that block a refused edit (mitcad#1566,
  // SketchConflict.hpp), painted in the view while its message shows:
  // their symbols and values and the curves and points they are on, in the
  // error's colour; in the hover's while the pointer is over the message
  // (`hovered`). `source` keeps the command panel's ("panel") apart from a
  // sketch action's ("action"), which the next action clears. A sketch that
  // does not solve paints its own conflicts as well (the Sketch Palette's
  // status, conflicts()). Logged when what is painted changes ("Sketch
  // conflict painted: k5 on c3, c6", "Sketch conflict painted: none").
  void setConflict(const QString& source, const QStringList& items);
  void setConflictHovered(const QString& source, bool hovered);
  QStringList conflictItems(const QString& source) const { return m_conflicts.value(source); }
  // Every item painted, each once: the sources' and the sketch's own
  // (SketchModel::conflicts, its constraints and dimensions).
  QStringList conflicts() const;
  bool conflictHovered() const { return !m_conflictHovered.isEmpty(); }
  // The curves and points the painted items are on.
  QStringList conflictEntities() const;
  // Selects the painted constraints and dimensions (the Sketch Palette's
  // status link), as clicks on them would; how many it selected.
  int selectConflicts();
  // The dimensions and constraints a window holds (view pixels; with
  // `crossing` also those it touches), as selection items (mitcad#217).
  Selection annotationsIn(const QRectF& area, bool crossing) const;
  // Whether a dimension or a constraint's symbol is at a place in the view
  // (view pixels): a right click there is for it, not for the geometry
  // under it.
  bool annotationAt(const QPointF& position) const;
  // The running tool's own right-click menu at a place in the view (view
  // pixels), mitcad#1540; empty when it has none, and the right-click is
  // then the tool's Esc. Each entry runs on the tool only while it is
  // still the running one, and shows its change in the view.
  std::vector<ToolMenuEntry> toolMenu(const QPointF& position);
  // A dimension's text being dragged.
  std::optional<std::pair<QString, V2>> textOverride() const { return m_textOverride; }
  void refreshView();
  void hint(const QString& message);
  // A hint about what an action did (a constraint refused or replaced,
  // mitcad#287), logged, and kept in front of the next prompt hint so a
  // tool's prompt does not hide it at once. At a level of its own
  // (mitcad#1051): a warning (a dimension added as driven instead of
  // over-constraining the sketch) stays a warning with the prompt after it.
  void notice(const QString& message, NoticeLevel level = NoticeLevel::Hint);
  // An error of the running tool, or of sketch mode without one: it goes
  // from the status line and the message band when the tool ends or the
  // sketch finishes (mitcad#1621, SketchHost::showSourceError).
  void error(const QString& message);

  // Values typed in the view.
  void setFields(const QVector<FieldSpec>& fields);
  void clearFields();
  bool typed(const QString& id) const;
  // The typed value (mm, radians, a count) when it is valid.
  std::optional<double> fieldValue(const QString& id) const;
  // The typed expression as the model takes it ("20" gives "20 mm").
  QString fieldExpression(const QString& id) const;
  QString fieldText(const QString& id) const;
  QRectF fieldRect(const QString& id) const;
  // Shows what the cursor gives, while nothing is typed.
  void setLive(const QString& id, double value);
  // A value editor at a view position (a dimension's value); Up, Down and
  // the wheel step its value down to `stepLimit` (mitcad#1254). `name` is
  // the parameter's, shown before the text; with `nameInText` the text
  // starts with it (`width = 20 mm`), to be edited or deleted, and Remove
  // Name is offered (mitcad#1577).
  void openValueEditor(const QPointF& at, const QString& text, FieldSpec::Kind kind,
                       std::function<void(const QString&)> accept,
                       const QString& name = QString(), const ValueStepLimit& stepLimit = {},
                       bool nameInText = false);
  // Inserts a parameter's name at the open value editor's cursor.
  void insertIntoEditor(const QString& name);
  bool valueEditorOpen() const { return m_editor != nullptr; }

  // Evaluates an expression; the expression the model takes ("20 mm") and
  // its value, or the reason it is invalid.
  bool evaluate(const QString& text, FieldSpec::Kind kind, QString& expression, double& value,
                QString& problem) const;
  QString formatLength(double mm) const;
  QString formatAngle(double radians) const;
  const QString& lengthUnit() const { return m_unit; }
  // Millimetres per document length unit.
  double lengthScale() const { return m_unitScale; }

  // Model access for SketchOp: throws the model's error.
  QJsonObject runModel(const QJsonObject& command);
  int undoDepth() const;
  QString undoLabel() const;

  // Equal spacing (mitcad#221): the objects an Equal Spacing panel has
  // (its preview's chain is drawn numbered, in the order the model keeps).
  void setSpacingPreview(const QStringList& anchors);
  const QStringList& spacingPreview() const { return m_spacingPreview; }

  // Clean Up Stale Projections (mitcad#252): what its preview removes and
  // trims, from the command's answer (an empty object: nothing). The
  // overlay draws the removed curves as they were and the pieces cut off in
  // the remove colour, and highlights the trimmed lines, until the panel
  // ends.
  struct StalePreview {
    std::vector<Curve2> removed;
    QStringList trimmed;
    std::vector<std::pair<V2, V2>> cut;
    bool empty() const { return removed.empty() && trimmed.isEmpty() && cut.empty(); }
  };
  void setStalePreview(const QJsonObject& answer);
  const StalePreview& stalePreview() const { return m_stalePreview; }

  // Selects curves and points of the edited sketch by id, in place of the
  // selection, as clicks would (Delete and the other selection commands
  // take them); ids the sketch lacks are left out. With `reveal` the view
  // pans to them when none is in sight, its zoom kept unless they do not
  // fit; `fit` zooms to them anyway (SketchReveal.cpp: the Sketch Palette's
  // warnings, mitcad#264; live MCP's ui_select, mitcad#265). Returns how
  // many it selected.
  int selectEntities(const QStringList& ids, bool reveal, bool fit = false);
  // A double-click on a curve of this sketch with no tool running
  // (mitcad#1541, SketchReveal.cpp): selects the smallest closed loop of
  // curves of its construction kind it bounds (the model's `sketch_loop`
  // query), else its open chain through points where exactly two curves
  // meet; Ctrl or Shift adds them to the selection. False, selecting
  // nothing, for no curve there, a text, a pattern's or an offset's curve
  // or a spline (their double-click is their own) and the axes.
  bool selectChainAt(const QPointF& screen, Qt::KeyboardModifiers modifiers);
  // Body edges and vertices in the sketch plane (mitcad#256, mitcad#258), for
  // the tool that picks or snaps to them: the model's plane_geometry query,
  // read once per tool session and again after the bodies it reads changed
  // (the model's plane_geometry_key, mitcad#1646). With `outOfPlane` (the
  // Dimension tool, mitcad#866) also the others, as what they project to:
  // read once while that key stays, not after each dimension added.
  // With `outOfPlane` the items out of the plane are read off the UI thread
  // (mitcad#1656): until they are there, the tool has those in the plane,
  // and once they are, it is told again where the pointer is. `wait` reads
  // them now (a selection the tool takes as it starts).
  const BodyGeometry& bodyGeometry(bool outOfPlane = false, bool wait = false);
  // As last read, for drawing (no query).
  const BodyGeometry& bodyGeometryShown() const { return m_body; }
  // Changes whenever the sketch is read again (a tool's view of it is
  // stale then).
  int modelRevision() const { return m_modelRevision; }

signals:
  // The sketch or its status changed (the palette shows the DOF).
  void changed();
  // The option toggles changed.
  void optionsChanged();
  void toolChanged(const QString& name);
  void geometryDragStarted();
  // The auto constraint session changed: a new tool, a choice, an entity.
  void autoConstraintsChanged();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Field {
    FieldSpec spec;
    QPointer<QLineEdit> edit;
    QPointer<QLabel> label;
    bool typed = false;
    bool valid = false;
    double value = 0.0;
    QString expression;
    // The user types into this one: typed into it or moved to it with Tab.
    // Until then the mouse passes it to the view (mitcad#671).
    bool engaged = false;
  };

  bool viewerEvent(QEvent* event);
  bool fieldEvent(QLineEdit* edit, QEvent* event);
  // The mouse on a field the user does not type into (`source`: the field
  // or its fx button) goes to the view while a tool waits for a point.
  bool fieldMouse(QLineEdit* edit, QWidget* source, QEvent* event);
  // For UI tests: where the fields are, once the pointer rests.
  void logFieldPlaces();
  void updateCursor(const QPointF& position);
  void placeFields();
  void fieldEdited(int index);
  void acceptEditor();
  void closeEditor();
  // An error of `source` (mitcad#1621): error() and the value editor's own.
  void sourceError(const QString& message, const QString& source);
  // A `sketch.set_dimension` result's own parameters kept as user
  // parameters (mitcad#1535): logged, and the model's notes shown.
  void noteKeptParameters(const QJsonObject& result);
  void selectAnnotation(const QString& kind, const QString& id, Qt::KeyboardModifiers modifiers);
  void startDrag();
  void applyDrag();
  void endDrag();
  // The selected points and curves of this sketch when `entity` is one of
  // several of them (mitcad#837): a press on it drags them together.
  // Empty otherwise.
  QStringList selectionDraggedWith(const QString& entity) const;
  // A single point dragged (mitcad#1538): where the pointer at `position`
  // snaps it to, as the Line tool snaps (points, curve ends, centres,
  // midpoints, quadrants, intersections, curves and the origin; not to the
  // grid or onto the axes across the view), into m_snap; with Shift or Alt
  // held, where the pointer is. Its own curves and what holds it already
  // are left out.
  void snapDrag(const QPointF& position, Qt::KeyboardModifiers modifiers);
  // The dropped point held where it snapped (SketchOp::constrain: point
  // on point, on curve, ...), in an undo step of its own that endDrag
  // merges into the drag's; false when the model refused it (the action's
  // notice says why).
  bool holdAtDrop(const QString& point, const Snap& snap);
  // A modal dialog took the input mid-drag (a long computation's progress,
  // P7), and with it the release: the drag ends when the model is free.
  void interruptDrag();
  void forwardClick(const QMouseEvent* release);
  // The clicks and the hover on the sketch's axes and origin, which the
  // view does not show: while a command's panel runs (paused), on the axes
  // for an input that takes them (mitcad#876); with no tool running
  // (`selecting`), on the axes and the origin, which a click selects like
  // construction geometry and a drag never moves (mitcad#899). True when
  // the event was taken.
  bool axisPickEvent(QEvent* event, bool selecting);
  // The axis under a view position, as its axisPick() id, when no entity
  // of the sketch is nearer; empty otherwise.
  QString axisAt(const QPointF& screen) const;
  // What a click with no tool running selects there of the axes and the
  // origin (their pick ids, SketchModel::axisSelectionPick), when no
  // dimension, constraint symbol or sketch entity wins it, nor anything
  // precise the view picks itself (OcctViewer::claimsClick: a text, a body
  // edge or vertex in sight or behind faces) (mitcad#899); empty
  // otherwise.
  QString axisSelectionAt(const QPointF& screen);
  // Whether the hover is an axis or the origin's pick.
  bool axisHovered() const;
  // While a command's panel runs (SketchCommandPicks.cpp, mitcad#1563):
  // the input that picks points with the sketch's snaps (the pointer snaps
  // as the drawing tools' does, a click picks the point or place it
  // snapped to) and the command's handle (a press on it drags its values).
  // True when the event was taken.
  bool snapPickEvent(QEvent* event);
  bool handleEvent(QEvent* event);
  // The snap for a command at the view position `screen` (`raw` its place
  // on the sketch plane), as the drawing tools snap, away from `moves`
  // (entities and their points).
  Snap commandSnap(const V2& raw, const QPointF& screen, const QStringList& moves);
  // The handle's part under a view position: 'x', 'y', 'b' or 0.
  char handlePartAt(const QPointF& screen) const;
  // The handle dragged to the view position `screen` with `modifiers`
  // held: its values set by how far it went, snapped.
  void dragHandle(const QPointF& screen, Qt::KeyboardModifiers modifiers);
  // Ends what the command's picks and handle hold (a panel closed).
  void endCommandPicks();
  // For UI tests: where the command's handle is, when that changes.
  void logHandle();
  int fieldIndex(const QString& id) const;
  QString describeStatus() const;
  // Logs what setConflict() paints when it changed, and draws it.
  void conflictChanged();

  CommandRegistry& m_registry;
  SketchHost& m_host;
  OcctViewer& m_viewer;
  QString m_uid;
  SketchModel m_model;
  QString m_unit = QStringLiteral("mm");
  double m_unitScale = 1.0; // mm per unit
  QString m_status;         // last logged status
  QString m_loggedProjections; // the linked projections last logged
  QString m_loggedView;     // last logged view placement
  QString m_loggedGlyphs;   // the inactive constraints' symbol places last logged
  // The blocking items of refused edits by source (setConflict), the
  // sources whose message the pointer is over, and the line last logged.
  QHash<QString, QStringList> m_conflicts;
  QSet<QString> m_conflictHovered;
  QString m_loggedConflict;
  SketchOverlay* m_overlay = nullptr;
  std::unique_ptr<SketchTool> m_tool;
  QHash<QString, QString> m_toolIcons; // by tool name

  bool m_paused = false;
  // A command input's snaps and handle (mitcad#1563): whether m_snap is a
  // snap input's, a press for it, the handle as its drag started with the
  // part dragged and the sketch place pressed, the values last sent, and
  // the handle's place last logged.
  bool m_snapPicking = false;
  bool m_snapPressed = false;
  QPointF m_snapPress;
  std::optional<SketchHandle> m_handleDrag;
  char m_handlePart = 0;
  V2 m_handlePress;
  QPointF m_handlePressScreen;
  bool m_handleMoved = false; // further than a click from the press
  std::array<double, 2> m_handleSent{};
  // Where a drop holds the moved geometry (mitcad#1645): the point of the
  // objects at the handle's place as its drag started, where the handle
  // went last, the last drop on a point or a curve (that point, the snap
  // and the values it set), and where each snap input's pick snapped.
  QString m_handleBase;
  V2 m_handlePlace;
  struct HandleDrop {
    QString point;
    Snap snap;
    std::array<double, 2> values{};
  };
  std::optional<HandleDrop> m_handleDrop;
  QHash<QString, Snap> m_pickSnaps;
  QString m_loggedHandle;
  bool m_construction = false;
  bool m_signedDistances = false;
  bool m_gridSnap = true;
  double m_gridStep = 0.0;
  bool m_showDimensions = true;
  bool m_showConstraints = true;
  bool m_showProfiles = true;
  bool m_hideAbove = false;

  AutoConstraintSession m_auto;
  bool m_autoHover = false;
  mutable QString m_autoTarget; // the reference last logged as a target

  Snap m_snap;
  QPointF m_cursor;
  bool m_cursorValid = false;
  // The last notice(), until the next prompt hint has shown it, and its
  // level (mitcad#1051).
  QString m_notice;
  NoticeLevel m_noticeLevel = NoticeLevel::Hint;
  // The pointer's place (pointerText()), the last one shown, and the one
  // last logged ("Sketch pointer x 12.00, y -29.00 mm", once the pointer
  // rests: logPointer()).
  QString m_pointerText;
  QString m_movedPointer;
  QString m_loggedPointer;
  QString m_loggedFieldPlaces;
  // A left press on a field the view took (fieldMouse()), until its release.
  QPointer<QWidget> m_fieldPress;
  QTimer m_pointerTimer;
  QString pointerTextAt(const V2& at) const;
  void setPointerText(const QString& text);
  void logPointer();
  QString m_hover;
  QStringList m_picked;
  QString m_hoverAnnotation;

  // A press on geometry: a click (selection) or the start of a drag.
  bool m_pressed = false;
  bool m_dragging = false;
  bool m_forwarding = false;
  QPointF m_pressPosition;
  QPointF m_rightPressPosition;
  Qt::KeyboardModifiers m_pressModifiers;
  QString m_dragEntity;
  // The selection dragged together with m_dragEntity (mitcad#837); empty
  // when only m_dragEntity is dragged.
  QStringList m_dragSelection;
  QString m_dragDimension; // a dimension's text
  // A left press on an axis a command's input takes, or on an axis or the
  // origin a click selects (axisPickEvent()): its pick id, until the
  // release picks it or a drag hands the press on to the view.
  QString m_axisPress;
  V2 m_dragStart;
  V2 m_dragLast;
  V2 m_dragTarget;
  int m_dragDepth = 0;
  int m_dragSteps = 0;
  bool m_dragPending = false;
  // A single point dragged snaps (snapDrag, mitcad#1538): the points and
  // curves it does not snap to, the snap at the release (none when the
  // drag ended otherwise) and the snap last logged.
  bool m_dragSnaps = false;
  QSet<QString> m_dragSkipPoints;
  QSet<QString> m_dragSkipCurves;
  Snap m_dropSnap;
  QString m_loggedDragSnap;
  std::optional<std::pair<QString, V2>> m_textOverride;
  QTimer m_dragTimer;

  QVector<Field> m_fields;
  QPointer<QLineEdit> m_editor;
  FieldSpec::Kind m_editorKind = FieldSpec::Kind::Length;
  std::function<void(const QString&)> m_editorAccept;
  std::optional<int> m_editorToolSession; // a callback belonging to this tool session
  QPointer<QLabel> m_editorLabel; // "d3 =" before the editor (mitcad#231)
  QString m_editorDimension;       // the dimension it edits
  QString m_editorText;            // the text it opened with
  QString m_editorName;            // the name its text opened with (mitcad#1577)
  QString m_tooltipDimension;      // the dimension or constraint whose tooltip is shown
  // Logs what each dimension shows ("Dimension texts: k5 d1 = 60.00, ...").
  void logDimensionTexts() const;
  // Shows the tooltip of the dimension under the pointer, or of an inactive
  // constraint (mitcad#721; empty: none).
  void showDimensionTooltip(const QString& dimension, const QPointF& at);
  DimensionText m_dimensionText = DimensionText::Value;

  // Equal spacing (mitcad#221): a running panel's objects, and the chains
  // last logged (by constraint).
  QStringList m_spacingPreview;
  QHash<QString, QString> m_loggedSpacings;
  void logSpacings();

  // Clean Up Stale Projections' preview (mitcad#252).
  StalePreview m_stalePreview;

  // Body edges and vertices in the sketch plane (mitcad#256): what was read,
  // for which sketch, geometry key and tool session.
  BodyGeometry m_body;
  QString m_bodyKey;
  // The model's plane_geometry_key (mitcad#1646), read once per document
  // revision (m_geometryRevision): it stays while an edit of the sketch
  // itself leaves the bodies before it as they are.
  QString geometryKey();
  QString m_geometryRevision;
  QString m_geometryKey;
  // The items in the plane (per tool session) and out of it (mitcad#866:
  // per geometry key), and whether m_body has the latter.
  std::vector<BodyItem> m_inPlaneItems;
  std::vector<BodyItem> m_outOfPlaneItems;
  QString m_outOfPlaneKey;
  bool m_bodyOutOfPlane = false;
  // The key whose items out of the plane are being read off the UI thread
  // (mitcad#1656), and what makes the read and takes its answer.
  QString m_outOfPlaneReading;
  QJsonObject outOfPlaneQuery() const;
  void readOutOfPlaneLater(const QString& key);
  // `later`: read off the UI thread after the tool looked, which is then
  // told again where the pointer is.
  void takeOutOfPlane(const QString& key, std::vector<BodyItem> items, const QString& error, bool later);
  void setBodyItems();
  // Where the items out of the plane are in the view (mitcad#866), made
  // again only when the items or the camera change.
  struct BodyScreen {
    QPolygonF line;
    QRectF box;
    QPointF center;
  };
  mutable std::vector<BodyScreen> m_bodyScreen;
  mutable std::array<QPointF, 4> m_bodyScreenView{};
  mutable int m_bodyScreenRevision = -1;
  const std::vector<BodyScreen>& bodyScreens() const;
  // entityAt()'s items out of the plane (mitcad#866): the points (vertices,
  // edges seen end on, centres) or the edges within `reach` pixels where
  // the view shows them that would beat `bestDistance` counted `tie`
  // further, nearest first, the first in sight (OcctViewer::inSight).
  void outOfPlaneAt(const QPointF& screen, bool points, double reach, double tie, QString& best,
                    double& bestDistance) const;
  QString m_revision; // the document's, when the sketch was read
  int m_toolSession = 0;
  int m_modelRevision = 0;
};

} // namespace sketch
} // namespace mitcad
