// SPDX-License-Identifier: MIT
// The drawing tools of a 3D sketch (mitcad#1525): Point, Line, 3-Point,
// Center Point and Tangent Arc, Fit Point and Control Point Spline. The
// pointer places a point on the temporary plane through the anchor, along
// an axis or along the plane's normal (Sketch3dDraw.hpp), the snaps
// (Sketch3dSnaps.cpp) and the typed values decide where; each curve with
// its new points, references, relations and dimensions is one model edit
// and one undo step. Sketch3dController.hpp says how they fit together.

#include "Sketch3dController.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <limits>
#include <utility>

#include <QJsonDocument>
#include <QLineF>
#include <QMouseEvent>
#include <QPolygonF>
#include <QWidget>
#include <QtLogging>

#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

constexpr double kSnapPixels = 10.0;  // the snaps' reach on the screen
constexpr double kClickPixels = 6.0;  // a press and its release this close are a click
constexpr double kPlanePixels = 70.0; // the temporary plane's half size on the screen

const char* toolIcon(Sketch3dTool tool) {
  switch (tool) {
  case Sketch3dTool::Point:
    return "point";
  case Sketch3dTool::Line:
    return "line";
  case Sketch3dTool::ArcThreePoint:
    return "arc-3pt";
  case Sketch3dTool::ArcCenter:
    return "arc-center";
  case Sketch3dTool::ArcTangent:
    return "arc-tangent";
  case Sketch3dTool::SplineFit:
    return "spline-fit";
  case Sketch3dTool::SplineControl:
    return "spline-control";
  case Sketch3dTool::Fillet:
    return "sketch-fillet";
  case Sketch3dTool::CircleCenter:
    return "circle";
  case Sketch3dTool::CircleTwoPoint:
    return "circle-2pt";
  case Sketch3dTool::CircleThreePoint:
    return "circle-3pt";
  case Sketch3dTool::Ellipse:
    return "ellipse";
  case Sketch3dTool::RectangleTwoPoint:
    return "rectangle";
  case Sketch3dTool::RectangleCenter:
    return "rectangle-center";
  case Sketch3dTool::RectangleThreePoint:
    return "rectangle-3pt";
  case Sketch3dTool::PolygonCircumscribed:
    return "polygon";
  case Sketch3dTool::PolygonInscribed:
    return "polygon-inscribed";
  case Sketch3dTool::PolygonEdge:
    return "polygon-edge";
  case Sketch3dTool::Text:
    return "text";
  case Sketch3dTool::None:
    break;
  }
  return "";
}

// The tool's name in the logs, which UI tests read: English.
QString logName(Sketch3dTool tool) {
  switch (tool) {
  case Sketch3dTool::Point:
    return QStringLiteral("Point");
  case Sketch3dTool::Line:
    return QStringLiteral("Line");
  case Sketch3dTool::ArcThreePoint:
    return QStringLiteral("3-Point Arc");
  case Sketch3dTool::ArcCenter:
    return QStringLiteral("Center Point Arc");
  case Sketch3dTool::ArcTangent:
    return QStringLiteral("Tangent Arc");
  case Sketch3dTool::SplineFit:
    return QStringLiteral("Fit Point Spline");
  case Sketch3dTool::SplineControl:
    return QStringLiteral("Control Point Spline");
  case Sketch3dTool::Fillet:
    return QStringLiteral("Fillet");
  case Sketch3dTool::CircleCenter:
    return QStringLiteral("Center Diameter Circle");
  case Sketch3dTool::CircleTwoPoint:
    return QStringLiteral("2-Point Circle");
  case Sketch3dTool::CircleThreePoint:
    return QStringLiteral("3-Point Circle");
  case Sketch3dTool::Ellipse:
    return QStringLiteral("Ellipse");
  case Sketch3dTool::RectangleTwoPoint:
    return QStringLiteral("2-Point Rectangle");
  case Sketch3dTool::RectangleCenter:
    return QStringLiteral("Center Rectangle");
  case Sketch3dTool::RectangleThreePoint:
    return QStringLiteral("3-Point Rectangle");
  case Sketch3dTool::PolygonCircumscribed:
    return QStringLiteral("Circumscribed Polygon");
  case Sketch3dTool::PolygonInscribed:
    return QStringLiteral("Inscribed Polygon");
  case Sketch3dTool::PolygonEdge:
    return QStringLiteral("Edge Polygon");
  case Sketch3dTool::Text:
    return QStringLiteral("Text");
  case Sketch3dTool::None:
    break;
  }
  return QString();
}

QString xyzText(const Xyz& at) {
  const auto n = [](double v) { return QString::number(std::abs(v) < 5e-7 ? 0.0 : v, 'f', 3); };
  return QStringLiteral("(%1, %2, %3)").arg(n(at[0]), n(at[1]), n(at[2]));
}

QString compact(const QJsonObject& object) {
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

bool isArc(Sketch3dTool tool) {
  return tool == Sketch3dTool::ArcThreePoint || tool == Sketch3dTool::ArcCenter || tool == Sketch3dTool::ArcTangent;
}

bool isSpline(Sketch3dTool tool) { return tool == Sketch3dTool::SplineFit || tool == Sketch3dTool::SplineControl; }

// Keep Plane of a normal: the frame axis it lies along, with its sense.
std::optional<std::pair<Axis3, bool>> axisNormal(const Xyz& normal) {
  for (const Plane3 plane : {Plane3::XY, Plane3::YZ, Plane3::ZX}) {
    if (const auto kept = keptNormal(normal, plane)) {
      return kept;
    }
  }
  return std::nullopt;
}

// Keep Plane of an arc's normal with the placement: the active plane's
// normal axis, or with a rotated plane any axis of the sketch (Keep Plane
// keeps the arc in a plane of the sketch's axes, mitcad#1661).
std::optional<std::pair<Axis3, bool>> keptPlaneNormal(const Xyz& normal, const Placement& placement) {
  return placement.rotated() ? axisNormal(normal) : keptNormal(normal, placement.plane);
}

// The id of a command's main item: its result's `made` lists it first.
QString madeId(const QJsonObject& result) {
  const QJsonArray made = result.value(QStringLiteral("made")).toArray();
  return made.isEmpty() ? QString() : made.first().toString();
}

TypedValue& noField() {
  static TypedValue none;
  none = TypedValue();
  return none;
}

} // namespace

// ---------------------------------------------------------------------------
// Names

QString Sketch3dController::toolName(Sketch3dTool tool) {
  switch (tool) {
  case Sketch3dTool::Point:
    return tr("Point");
  case Sketch3dTool::Line:
    return tr("Line");
  case Sketch3dTool::ArcThreePoint:
    return tr("3-Point Arc");
  case Sketch3dTool::ArcCenter:
    return tr("Center Point Arc");
  case Sketch3dTool::ArcTangent:
    return tr("Tangent Arc");
  case Sketch3dTool::SplineFit:
    return tr("Fit Point Spline");
  case Sketch3dTool::SplineControl:
    return tr("Control Point Spline");
  case Sketch3dTool::Fillet:
    return tr("Fillet");
  case Sketch3dTool::CircleCenter:
    return tr("Center Diameter Circle");
  case Sketch3dTool::CircleTwoPoint:
    return tr("2-Point Circle");
  case Sketch3dTool::CircleThreePoint:
    return tr("3-Point Circle");
  case Sketch3dTool::Ellipse:
    return tr("Ellipse");
  case Sketch3dTool::RectangleTwoPoint:
    return tr("2-Point Rectangle");
  case Sketch3dTool::RectangleCenter:
    return tr("Center Rectangle");
  case Sketch3dTool::RectangleThreePoint:
    return tr("3-Point Rectangle");
  case Sketch3dTool::PolygonCircumscribed:
    return tr("Circumscribed Polygon");
  case Sketch3dTool::PolygonInscribed:
    return tr("Inscribed Polygon");
  case Sketch3dTool::PolygonEdge:
    return tr("Edge Polygon");
  case Sketch3dTool::Text:
    return tr("Text");
  case Sketch3dTool::None:
    break;
  }
  return QString();
}

QString Sketch3dController::toolName() const { return toolName(m_tool); }

QString Sketch3dController::toolCommand(Sketch3dTool tool) {
  switch (tool) {
  case Sketch3dTool::Point:
    return QStringLiteral("sketch.point");
  case Sketch3dTool::Line:
    return QStringLiteral("sketch.line");
  case Sketch3dTool::ArcThreePoint:
    return QStringLiteral("sketch.arc");
  case Sketch3dTool::ArcCenter:
    return QStringLiteral("sketch.arc_center");
  case Sketch3dTool::ArcTangent:
    return QStringLiteral("sketch.arc_tangent");
  case Sketch3dTool::SplineFit:
    return QStringLiteral("sketch.spline");
  case Sketch3dTool::SplineControl:
    return QStringLiteral("sketch.spline_control");
  case Sketch3dTool::Fillet:
    return QStringLiteral("sketch.fillet");
  case Sketch3dTool::CircleCenter:
    return QStringLiteral("sketch.circle");
  case Sketch3dTool::CircleTwoPoint:
    return QStringLiteral("sketch.circle_2pt");
  case Sketch3dTool::CircleThreePoint:
    return QStringLiteral("sketch.circle_3pt");
  case Sketch3dTool::Ellipse:
    return QStringLiteral("sketch.ellipse");
  case Sketch3dTool::RectangleTwoPoint:
    return QStringLiteral("sketch.rectangle");
  case Sketch3dTool::RectangleCenter:
    return QStringLiteral("sketch.rectangle_center");
  case Sketch3dTool::RectangleThreePoint:
    return QStringLiteral("sketch.rectangle_3pt");
  case Sketch3dTool::PolygonCircumscribed:
    return QStringLiteral("sketch.polygon");
  case Sketch3dTool::PolygonInscribed:
    return QStringLiteral("sketch.polygon_inscribed");
  case Sketch3dTool::PolygonEdge:
    return QStringLiteral("sketch.polygon_edge");
  case Sketch3dTool::Text:
    return QStringLiteral("sketch.text");
  case Sketch3dTool::None:
    break;
  }
  return QString();
}

Sketch3dTool Sketch3dController::toolForCommand(const QString& id) {
  for (const Sketch3dTool tool :
       {Sketch3dTool::Point, Sketch3dTool::Line, Sketch3dTool::ArcThreePoint, Sketch3dTool::ArcCenter,
        Sketch3dTool::ArcTangent, Sketch3dTool::SplineFit, Sketch3dTool::SplineControl, Sketch3dTool::Fillet,
        Sketch3dTool::CircleCenter, Sketch3dTool::CircleTwoPoint, Sketch3dTool::CircleThreePoint,
        Sketch3dTool::Ellipse, Sketch3dTool::RectangleTwoPoint, Sketch3dTool::RectangleCenter,
        Sketch3dTool::RectangleThreePoint, Sketch3dTool::PolygonCircumscribed, Sketch3dTool::PolygonInscribed,
        Sketch3dTool::PolygonEdge, Sketch3dTool::Text}) {
    if (toolCommand(tool) == id) {
      return tool;
    }
  }
  return Sketch3dTool::None;
}

QString Sketch3dController::fieldLabel(const QString& id) {
  if (id == QStringLiteral("dx")) {
    return tr("Delta X");
  }
  if (id == QStringLiteral("dy")) {
    return tr("Delta Y");
  }
  if (id == QStringLiteral("dz")) {
    return tr("Delta Z");
  }
  if (id == QStringLiteral("length")) {
    return tr("Length");
  }
  if (id == QStringLiteral("radius")) {
    return tr("Radius");
  }
  if (id == QStringLiteral("arc_length")) {
    return tr("Arc Length");
  }
  if (id == QStringLiteral("diameter")) {
    return tr("Diameter");
  }
  if (id == QStringLiteral("major")) {
    return tr("Major Radius");
  }
  if (id == QStringLiteral("minor")) {
    return tr("Minor Radius");
  }
  if (id == QStringLiteral("width")) {
    return tr("Width");
  }
  if (id == QStringLiteral("height")) {
    return tr("Height");
  }
  if (id == QStringLiteral("sides")) {
    return tr("Sides");
  }
  if (id == QStringLiteral("text")) {
    return tr("Text");
  }
  if (id == QStringLiteral("text_height")) {
    return tr("Text Height");
  }
  return id;
}

// ---------------------------------------------------------------------------
// Starting and ending

void Sketch3dController::startTool(Sketch3dTool tool) {
  if (!isActive() || tool == Sketch3dTool::None) {
    return;
  }
  // A drag or a source pick under way ends (mitcad#1526).
  cancelDrag();
  cancelSourcePick();
  m_tool = tool;
  m_pending.clear();
  m_chainPoint.clear();
  m_chainCurve.clear();
  resetInput();
  if (tool != Sketch3dTool::Line) {
    m_input.mode = SegmentInput::Mode::Offsets; // Axis and Length is the Line's
  }
  m_pickingNormal = false;
  m_arcNormal.reset();
  m_arcNormalWhat.clear();
  m_arcNormalWords.clear();
  m_problem.clear();
  m_planeHeldPoint.clear();
  m_hoverAt.reset();
  m_hoverSnap = SnapTarget();
  m_loggedSnap.clear();
  m_loggedProblem.clear();
  dropFillet();
  m_filletHover.reset();
  m_filletHoverLine.clear();
  m_loggedFillet.clear();
  if (m_place.mode == Placement::Mode::Normal) {
    m_place.mode = Placement::Mode::Plane;
    m_place.normalBase.reset();
  }
  // A handle's drag ends; the rotation itself stays for the next tool
  // (mitcad#1661).
  m_rotation = RotationDrag();
  m_ringHover.reset();
  m_loggedHandles.clear();
  m_viewer.setSketchInput(true, QString::fromLatin1(toolIcon(tool)));
  toolLog(QStringLiteral("3D sketch tool %1: started; %2, anchor %3")
              .arg(logName(tool), placementText(toolPlacement()), xyzText(m_anchor)));
  if (m_hasPointer) {
    updateHover();
  }
  emit toolChanged();
  emit toolStateChanged();
  updateOverlay();
  logHandles();
}

void Sketch3dController::stopTool() {
  if (!toolActive()) {
    return;
  }
  const QString name = logName(m_tool);
  if (m_rotation.active) {
    endRotationDrag(false); // a handle's drag under way is taken back
  }
  m_rotation = RotationDrag();
  m_ringHover.reset();
  m_loggedHandles.clear();
  m_tool = Sketch3dTool::None;
  m_pending.clear();
  m_chainPoint.clear();
  m_chainCurve.clear();
  resetInput();
  dropFillet();
  m_filletHover.reset();
  m_filletHoverLine.clear();
  m_problem.clear();
  m_planeHeldPoint.clear();
  m_hoverAt.reset();
  m_hoverSnap = SnapTarget();
  paintSnap(SnapTarget()); // its source no longer painted (mitcad#1653)
  m_focusField.clear();
  m_viewer.setSketchSnapping(false);
  m_viewer.setSketchInput(false);
  toolLog(QStringLiteral("3D sketch tool %1: stopped").arg(name));
  emit toolChanged();
  emit toolStateChanged();
  updateOverlay();
}

void Sketch3dController::resetInput() {
  const SegmentInput::Mode mode = m_input.mode;
  const Axis3 axis = m_input.axis;
  const bool negative = m_input.negative;
  m_input = SegmentInput();
  // The mode and the axis chosen stay for the next segment; the typed
  // values are each segment's own.
  m_input.mode = mode;
  m_input.axis = axis;
  m_input.negative = negative;
  m_radius.clear();
  m_arcLength.clear();
  // A shape's sizes are its own; the sides, the text and its height stay
  // for the next one, as in the planar tools.
  m_size.clear();
  m_size2.clear();
  m_textFrame.reset();
  m_textPress.reset();
  m_textDragging = false;
}

void Sketch3dController::finishChain() {
  m_pending.clear();
  m_chainPoint.clear();
  m_chainCurve.clear();
  resetInput();
  m_planeHeldPoint.clear();
  toolLog(QStringLiteral("3D sketch %1: chain ended; waiting for a first point").arg(logName(m_tool)));
  updateHover();
  emit toolStateChanged();
}

bool Sketch3dController::cancel() {
  // Without a tool: a drag, a source pick, a delete waiting for its
  // dependents step back first (mitcad#1526).
  if (!toolActive()) {
    if (cancelDrag()) {
      return true;
    }
    if (m_sourcePick != SourcePick::None) {
      cancelSourcePick();
      return true;
    }
    if (!m_pendingDelete.isEmpty()) {
      m_pendingDelete.clear();
      m_pendingDependents.clear();
      setEditNotice(QString());
      toolLog(QStringLiteral("3D sketch Esc: the delete waiting for its dependents dropped"));
      return true;
    }
    return false;
  }
  if (m_rotation.active) {
    // A handle's drag: the plane back as it was (mitcad#1661).
    endRotationDrag(false);
    toolLog(QStringLiteral("3D sketch Esc: the rotation handle's drag taken back"));
    return true;
  }
  if (!m_focusField.isEmpty()) {
    // The field's typed value goes back to its measurement.
    const QString id = m_focusField;
    m_focusField.clear();
    setField(id, QString());
    toolLog(QStringLiteral("3D sketch Esc: field %1 back to its measurement").arg(id));
    focusView();
    return true;
  }
  if (m_pickingNormal) {
    m_pickingNormal = false;
    toolLog(QStringLiteral("3D sketch Esc: %1 picks no normal").arg(logName(m_tool)));
    emit toolChanged(); // the pick filter goes back
    updateHover();
    return true;
  }
  if (m_tool == Sketch3dTool::Fillet && dropFillet()) {
    // The corner or the first line picked goes; the tool stays.
    toolLog(QStringLiteral("3D sketch Esc: Fillet pending corner dropped"));
    updateHover();
    return true;
  }
  if (!m_pending.empty() || !m_chainPoint.isEmpty()) {
    toolLog(QStringLiteral("3D sketch Esc: %1 pending points dropped").arg(logName(m_tool)));
    finishChain();
    return true;
  }
  toolLog(QStringLiteral("3D sketch Esc: %1 ends").arg(logName(m_tool)));
  stopTool();
  return true;
}

bool Sketch3dController::confirm() {
  if (!toolActive()) {
    return false;
  }
  m_focusField.clear();
  updateHover();
  if (m_tool == Sketch3dTool::Fillet) {
    return confirmFillet(); // the picked corner with its radius
  }
  if (m_tool == Sketch3dTool::Text) {
    // The text placed (and its frame dragged) goes in with what is typed.
    if (m_pending.empty()) {
      return false;
    }
    commitShape();
    return true;
  }
  const bool typed = m_input.anyEdited() || m_radius.edited || m_arcLength.edited;
  if (isSpline(m_tool) && !typed) {
    // Enter ends the spline.
    const std::size_t least = 2;
    if (m_pending.size() >= least) {
      commitSpline(false);
      return true;
    }
    return false;
  }
  if (m_tool == Sketch3dTool::Line && m_pending.empty() && m_chainPoint.isEmpty()) {
    return false; // the start is clicked
  }
  if (m_tool == Sketch3dTool::Point && !typed) {
    return false;
  }
  if (m_tool == Sketch3dTool::ArcTangent && m_chainPoint.isEmpty()) {
    return false;
  }
  if (!m_hoverAt) {
    m_host.showHint(toolHint());
    return false;
  }
  place();
  return true;
}

void Sketch3dController::clearPending() {
  cancelDrag(); // a drag's preview is on the geometry before the undo (mitcad#1526)
  if (!toolActive()) {
    return;
  }
  if (!m_pending.empty() || !m_chainPoint.isEmpty()) {
    toolLog(QStringLiteral("3D sketch %1: pending points dropped before the undo").arg(logName(m_tool)));
  }
  m_pending.clear();
  m_chainPoint.clear();
  m_chainCurve.clear();
  resetInput();
  dropFillet(); // its lines may go with the undo
  m_hoverAt.reset();
  emit toolStateChanged();
  updateOverlay();
}

void Sketch3dController::focusView() { m_viewer.setFocus(Qt::OtherFocusReason); }

// ---------------------------------------------------------------------------
// Options

void Sketch3dController::setPlane(Plane3 plane) {
  m_place.plane = plane;
  if (m_place.mode == Placement::Mode::Normal) {
    m_place.normalBase = m_placed ? m_placed : std::optional<Xyz>(m_anchor);
  }
  toolLog(QStringLiteral("3D sketch placement: %1").arg(placementText(m_place)));
  updateHover();
  emit toolStateChanged();
}

void Sketch3dController::setPlacementMode(Placement::Mode mode, Axis3 axis) {
  m_place.mode = mode;
  m_place.axis = axis;
  m_place.normalBase.reset();
  if (mode == Placement::Mode::Normal) {
    // The in-plane place the pointer has now is held; the elevation moves.
    const PlacedPoint from = fromPoint();
    Xyz base = m_hoverAt.value_or(from.at);
    const Xyz n = placementNormal(m_place);
    base = sub(base, scaled(n, dot(sub(base, from.at), n)));
    m_place.normalBase = base;
  }
  toolLog(QStringLiteral("3D sketch placement: %1%2")
              .arg(placementText(m_place),
                   m_place.normalBase ? QStringLiteral(" through %1").arg(xyzText(*m_place.normalBase)) : QString()));
  updateHover();
  emit toolStateChanged();
}

void Sketch3dController::cyclePlane(bool backwards, const QString& key) {
  if (!takesPlacementKeys()) {
    return;
  }
  constexpr std::array<Plane3, 3> kOrder{Plane3::XY, Plane3::YZ, Plane3::ZX};
  std::size_t at = 0;
  for (std::size_t i = 0; i < kOrder.size(); ++i) {
    if (kOrder[i] == m_place.plane) {
      at = i;
    }
  }
  const std::size_t step = backwards ? kOrder.size() - 1 : std::size_t{1};
  const Plane3 next = kOrder[(at + step) % kOrder.size()];
  toolLog(QStringLiteral("3D placement plane: %1 (%2)").arg(planeLabel(next), key));
  setPlane(next);
}

void Sketch3dController::toggleNormal(const QString& key) {
  if (!takesPlacementKeys()) {
    return;
  }
  const bool normal = m_place.mode != Placement::Mode::Normal;
  toolLog(QStringLiteral("3D placement: %1 (%2)").arg(normal ? QStringLiteral("Normal") : QStringLiteral("On Plane"), key));
  setPlacementMode(normal ? Placement::Mode::Normal : Placement::Mode::Plane);
}

QString Sketch3dController::placementKeysHint() const {
  const QString next = keyText(kNextPlaneCommand);
  const QString previous = keyText(kPreviousPlaneCommand);
  const QString normal = keyText(kNormalCommand);
  QStringList parts;
  if (!next.isEmpty() && !previous.isEmpty()) {
    parts << tr("%1 or %2 changes the plane").arg(next, previous);
  } else if (!next.isEmpty() || !previous.isEmpty()) {
    parts << tr("%1 changes the plane").arg(next.isEmpty() ? previous : next);
  }
  if (!normal.isEmpty()) {
    parts << tr("%1 switches between On Plane and Normal").arg(normal);
  }
  return parts.isEmpty() ? QString() : tr("%1.").arg(parts.join(QStringLiteral(", ")));
}

void Sketch3dController::setKeepAxis(bool keep) {
  m_keepAxis = keep;
  toolLog(QStringLiteral("3D sketch Keep Axis: %1").arg(keep ? QStringLiteral("on") : QStringLiteral("off")));
  updateHover();
  emit toolStateChanged();
}

void Sketch3dController::setKeepPlane(bool keep) {
  m_keepPlane = keep;
  toolLog(QStringLiteral("3D sketch Keep Plane: %1").arg(keep ? QStringLiteral("on") : QStringLiteral("off")));
  updateHover();
  emit toolStateChanged();
}

void Sketch3dController::setConstruction(bool construction) {
  m_construction = construction;
  toolLog(QStringLiteral("3D sketch construction: %1").arg(construction ? QStringLiteral("on") : QStringLiteral("off")));
  emit toolStateChanged();
  updateOverlay();
}

void Sketch3dController::setInputMode(SegmentInput::Mode mode) {
  if (m_input.mode == mode) {
    return;
  }
  m_input.mode = mode;
  // The other mode's typed values were requests for this segment only:
  // they go, visibly; committed relations stay.
  m_input.clearOtherMode();
  toolLog(QStringLiteral("3D sketch input: %1; the other mode's typed values cleared")
              .arg(mode == SegmentInput::Mode::Offsets ? QStringLiteral("Offsets") : QStringLiteral("Axis and Length")));
  updateHover();
  emit toolStateChanged();
}

void Sketch3dController::setInputAxis(Axis3 axis) {
  m_input.axis = axis;
  toolLog(QStringLiteral("3D sketch input axis: %1%2")
              .arg(m_input.negative ? QStringLiteral("-") : QStringLiteral("+"), axisLabel(axis)));
  updateHover();
  emit toolStateChanged();
}

void Sketch3dController::setInputNegative(bool negative) {
  m_input.negative = negative;
  toolLog(QStringLiteral("3D sketch input axis: %1%2")
              .arg(negative ? QStringLiteral("-") : QStringLiteral("+"), axisLabel(m_input.axis)));
  updateHover();
  emit toolStateChanged();
}

bool Sketch3dController::offersKeepPlane() const { return isArc(m_tool); }

Xyz Sketch3dController::arcNormal() const { return m_arcNormal.value_or(placementNormal(m_place)); }

QString Sketch3dController::arcNormalText() const {
  if (m_arcNormal) {
    // In words; its source's ids and its direction are the log's (mitcad#1653).
    return tr("from %1").arg(m_arcNormalWords);
  }
  if (m_place.rotated()) {
    return tr("the rotated plane's (%1)").arg(axisLabel(planeNormalAxis(m_place.plane)));
  }
  return tr("the active plane's (%1)").arg(axisLabel(planeNormalAxis(m_place.plane)));
}

void Sketch3dController::startNormalPick() {
  if (m_tool != Sketch3dTool::ArcCenter) {
    return;
  }
  m_pickingNormal = true;
  toolLog(QStringLiteral("3D sketch Center Point Arc: picking the normal"));
  emit toolChanged(); // faces and axes can be picked now
  emit toolStateChanged();
  updateOverlay();
}

void Sketch3dController::useActivePlaneNormal() {
  const bool picking = m_pickingNormal;
  m_pickingNormal = false;
  m_arcNormal.reset();
  m_arcNormalWhat.clear();
  m_arcNormalWords.clear();
  toolLog(QStringLiteral("3D sketch Center Point Arc: normal from the active plane"));
  if (picking) {
    emit toolChanged();
  }
  updateHover();
  emit toolStateChanged();
}

// ---------------------------------------------------------------------------
// The manipulator's rotation (mitcad#1661)

namespace {

constexpr double kRingPixels = 7.0; // a press this near a ring grabs it
constexpr double kRingSize = 1.2;   // the rings' radius in the plane's half sides
constexpr int kRingSegments = 64;

// The frame for agents and the log: "x (…), y (…), z (…)".
QString frameText(const Frame3& frame) {
  return QStringLiteral("x %1, y %2, z %3").arg(xyzText(frame[0]), xyzText(frame[1]), xyzText(frame[2]));
}

// The distance on the screen from `p` to the polyline `line`.
double polylineDistance(const QPointF& p, const QPolygonF& line) {
  double best = std::numeric_limits<double>::infinity();
  for (qsizetype i = 0; i + 1 < line.size(); ++i) {
    const QPointF a = line.at(i);
    const QPointF ab = line.at(i + 1) - a;
    const double length2 = QPointF::dotProduct(ab, ab);
    const double t = length2 > 1e-12 ? std::clamp(QPointF::dotProduct(p - a, ab) / length2, 0.0, 1.0) : 0.0;
    best = std::min(best, QLineF(p, a + ab * t).length());
  }
  return best;
}

} // namespace

bool Sketch3dController::offersRotation() const { return toolActive() && m_tool != Sketch3dTool::Fillet; }

Placement Sketch3dController::toolPlacement() const { return m_place; }

double Sketch3dController::manipulatorSize(const Xyz& anchor) const {
  double perMm = 0.0;
  for (const Xyz& along : placementPlaneAxes(toolPlacement())) {
    perMm = std::max(perMm, QLineF(onScreen(anchor), onScreen(add(anchor, along))).length());
  }
  return perMm > 1e-6 ? kPlanePixels / perMm : 10.0;
}

double Sketch3dController::ringRadius(const Xyz& anchor) const {
  // By the sketch's own axes: a ring keeps its size while it turns.
  double perMm = 0.0;
  for (const Axis3 axis : {Axis3::X, Axis3::Y, Axis3::Z}) {
    perMm = std::max(perMm, QLineF(onScreen(anchor), onScreen(add(anchor, axisVector(axis)))).length());
  }
  return kRingSize * (perMm > 1e-6 ? kPlanePixels / perMm : 10.0);
}

std::optional<Axis3> Sketch3dController::ringAt(const QPointF& position) const {
  if (!m_showHandles || !offersRotation() || !m_model.hasFrame) {
    return std::nullopt;
  }
  const Xyz center = fromPoint().at;
  const double radius = ringRadius(center);
  std::optional<Axis3> best;
  double bestPixels = kRingPixels;
  for (const Axis3 axis : {Axis3::X, Axis3::Y, Axis3::Z}) {
    QPolygonF line;
    for (const Xyz& p : handleRing(center, m_place.frame, axis, radius, kRingSegments)) {
      line << onScreen(p);
    }
    const double pixels = polylineDistance(position, line);
    if (pixels <= bestPixels) {
      bestPixels = pixels;
      best = axis;
    }
  }
  return best;
}

std::optional<std::pair<Axis3, double>> Sketch3dController::rotationDrag() const {
  if (!m_rotation.active) {
    return std::nullopt;
  }
  return std::make_pair(m_rotation.axis, m_rotation.angle);
}

void Sketch3dController::rotatePlane(Axis3 about, double angle) {
  if (!toolActive() || !std::isfinite(angle)) {
    return;
  }
  if (m_rotation.active) {
    endRotationDrag(false);
  }
  m_place.frame = turnedFrame(m_place.frame, about, angle);
  toolLog(QStringLiteral("3D sketch plane rotated %1 about %2: %3; frame %4")
              .arg(degreesText(angle), axisLabel(about), placementText(m_place), frameText(m_place.frame)));
  updateHover();
  emit toolStateChanged();
  updateOverlay();
  logHandles();
}

QString Sketch3dController::rotatePlaneBy(Axis3 about, const QString& text) {
  if (text.trimmed().isEmpty()) {
    return QString();
  }
  QString problem;
  double angle = 0.0;
  try {
    // A plain number is in degrees, as the angle fields read it.
    const QJsonObject answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                                   {QStringLiteral("expression"), text},
                                                   {QStringLiteral("kind"), QStringLiteral("angle")}});
    angle = answer.value(QStringLiteral("value")).toDouble();
    if (!std::isfinite(angle)) {
      problem = tr("not a finite value");
    }
  } catch (const std::exception& e) {
    problem = QString::fromUtf8(e.what());
  }
  if (!problem.isEmpty()) {
    toolLog(QStringLiteral("3D sketch rotate %1: %2 refused (%3)").arg(axisLabel(about), text, problem));
    const QString shown = tr("Rotate %1: %2").arg(axisLabel(about), problem);
    m_host.showError(shown);
    return shown;
  }
  rotatePlane(about, angle);
  return QString();
}

void Sketch3dController::resetPlane() {
  if (!toolActive()) {
    return;
  }
  m_rotation = RotationDrag();
  m_place.frame = sketchFrame();
  m_place.plane = Plane3::XY;
  m_place.mode = Placement::Mode::Plane;
  m_place.normalBase.reset();
  toolLog(QStringLiteral("3D sketch plane reset: %1 at %2").arg(placementText(m_place), xyzText(fromPoint().at)));
  updateHover();
  emit toolStateChanged();
  updateOverlay();
  logHandles();
}

void Sketch3dController::setShowPlane(bool show) {
  m_showPlane = show;
  toolLog(QStringLiteral("3D sketch manipulator: plane %1").arg(show ? QStringLiteral("shown") : QStringLiteral("hidden")));
  emit toolStateChanged();
  updateOverlay();
}

void Sketch3dController::setShowHandles(bool show) {
  m_showHandles = show;
  if (!show) {
    if (m_rotation.active) {
      endRotationDrag(false);
    }
    m_rotation.pressed.reset();
    m_ringHover.reset();
  }
  toolLog(QStringLiteral("3D sketch manipulator: rotation handles %1")
              .arg(show ? QStringLiteral("shown") : QStringLiteral("hidden")));
  emit toolStateChanged();
  updateOverlay();
  logHandles();
}

bool Sketch3dController::rotationMouse(QEvent* event) {
  switch (event->type()) {
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    if ((e->buttons() & (Qt::MiddleButton | Qt::RightButton)) != 0) {
      return false; // the view turns, pans or zooms
    }
    if (!m_rotation.active && m_rotation.pressed && (e->buttons() & Qt::LeftButton) != 0 &&
        QLineF(e->position(), m_pressAt).length() > kClickPixels) {
      // Past a click from a ring: its drag turns the plane about its axis
      // through the anchor.
      const Axis3 axis = *m_rotation.pressed;
      m_rotation.pressed.reset();
      const Xyz center = fromPoint().at;
      const auto pressRay = rayAt(m_pressAt);
      const auto grabbed = pressRay ? onPlane(*pressRay, center, placementAxis(m_place, axis)) : std::nullopt;
      if (!grabbed) {
        return false; // the ring seen edge-on: the press is the tool's
      }
      m_rotation.active = true;
      m_rotation.axis = axis;
      m_rotation.center = center;
      m_rotation.grabbed = *grabbed;
      m_rotation.start = m_place.frame;
      m_rotation.angle = 0.0;
      m_ringHover = axis;
      toolLog(QStringLiteral("3D sketch rotation handle %1: dragged").arg(axisLabel(axis)));
      emit toolStateChanged();
      updateOverlay();
      // This move turns it already.
    }
    if (m_rotation.active) {
      if ((e->buttons() & Qt::LeftButton) == 0) {
        endRotationDrag(true); // the release was missed
        return true;
      }
      const auto ray = rayAt(e->position());
      const Xyz along = m_rotation.start[static_cast<std::size_t>(m_rotation.axis)];
      if (const auto turn = ray ? handleTurn(*ray, m_rotation.center, along, m_rotation.grabbed) : std::nullopt) {
        const double angle = wholeDegrees(*turn);
        if (angle != m_rotation.angle) {
          m_rotation.angle = angle;
          m_place.frame = turnedFrame(m_rotation.start, m_rotation.axis, angle);
          toolLog(QStringLiteral("3D sketch rotation handle %1: %2").arg(axisLabel(m_rotation.axis), degreesText(angle)));
          emit toolStateChanged();
          updateOverlay();
        }
      }
      return true;
    }
    if ((e->buttons() & Qt::LeftButton) == 0) {
      // The handle under the pointer is drawn strong.
      const std::optional<Axis3> hover = ringAt(e->position());
      if (hover != m_ringHover) {
        m_ringHover = hover;
        toolLog(QStringLiteral("3D sketch rotation handle hover: %1")
                    .arg(hover ? axisLabel(*hover) : QStringLiteral("none")));
        updateOverlay();
      }
    }
    return false;
  }
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    m_rotation.pressed.reset();
    if (e->button() == Qt::LeftButton && !m_viewer.isOnOrientationCube(e->position())) {
      // A press on a ring: a drag from it turns the plane, a click is the
      // tool's as anywhere else.
      m_rotation.pressed = ringAt(e->position());
    }
    return false;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    m_rotation.pressed.reset();
    if (e->button() == Qt::LeftButton && m_rotation.active) {
      m_leftPressed = false;
      endRotationDrag(true);
      return true;
    }
    return false;
  }
  default:
    break;
  }
  return false;
}

void Sketch3dController::endRotationDrag(bool keep) {
  if (!m_rotation.active) {
    return;
  }
  const RotationDrag drag = m_rotation;
  m_rotation = RotationDrag();
  if (keep && drag.angle != 0.0) {
    m_place.frame = turnedFrame(drag.start, drag.axis, drag.angle);
    toolLog(QStringLiteral("3D sketch plane rotated %1 about %2 by its handle: %3; frame %4")
                .arg(degreesText(drag.angle), axisLabel(drag.axis), placementText(m_place), frameText(m_place.frame)));
  } else {
    m_place.frame = drag.start;
    toolLog(QStringLiteral("3D sketch rotation handle %1: not turned").arg(axisLabel(drag.axis)));
  }
  updateHover();
  emit toolStateChanged();
  updateOverlay();
  logHandles();
}

void Sketch3dController::logHandles() {
  if (!isActive() || !toolActive() || m_rotation.active || !m_model.hasFrame) {
    return;
  }
  QStringList lines;
  if (!offersRotation()) {
    lines << QStringLiteral("3D sketch rotation handles: not offered");
  } else if (!m_showHandles) {
    lines << QStringLiteral("3D sketch rotation handles: hidden");
  } else {
    // Where each ring is farthest from the other two on the screen, in
    // the view and not under a card floating over it.
    const Xyz center = fromPoint().at;
    const double radius = ringRadius(center);
    std::array<QPolygonF, 3> rings;
    for (const Axis3 axis : {Axis3::X, Axis3::Y, Axis3::Z}) {
      for (const Xyz& p : handleRing(center, m_place.frame, axis, radius, kRingSegments)) {
        rings[static_cast<std::size_t>(axis)] << onScreen(p);
      }
    }
    const QPoint window = m_viewer.mapTo(m_viewer.window(), QPoint(0, 0));
    const QRectF view(m_viewer.rect());
    for (const Axis3 axis : {Axis3::X, Axis3::Y, Axis3::Z}) {
      const auto k = static_cast<std::size_t>(axis);
      std::optional<QPointF> best;
      double bestPixels = -1.0;
      for (const QPointF& p : std::as_const(rings[k])) {
        if (!view.contains(p) || m_viewer.isCovered(p)) {
          continue;
        }
        const double pixels =
            std::min(polylineDistance(p, rings[(k + 1) % 3]), polylineDistance(p, rings[(k + 2) % 3]));
        if (pixels > bestPixels) {
          bestPixels = pixels;
          best = p;
        }
      }
      if (best && bestPixels > kRingPixels) {
        lines << QStringLiteral("3D sketch rotation handle %1 at %2,%3")
                     .arg(axisLabel(axis))
                     .arg(qRound(best->x() + window.x()))
                     .arg(qRound(best->y() + window.y()));
      } else {
        lines << QStringLiteral("3D sketch rotation handle %1: out of sight").arg(axisLabel(axis));
      }
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedHandles) {
    m_loggedHandles = logged;
    for (const QString& line : std::as_const(lines)) {
      toolLog(line);
    }
  }
}

std::optional<Xyz> Sketch3dController::localDirection(const gp_Vec& design) const {
  gp_Trsf back = m_placement;
  back.Invert();
  const gp_Vec v = design.Transformed(back);
  const Xyz d{v.X(), v.Y(), v.Z()};
  return sketch::unit(Xyz{dot(d, m_model.xAxis), dot(d, m_model.yAxis), dot(d, m_model.normal)});
}

QStringList Sketch3dController::fieldIds() const {
  const QStringList offsets{QStringLiteral("dx"), QStringLiteral("dy"), QStringLiteral("dz")};
  switch (m_tool) {
  case Sketch3dTool::Line:
    if (m_pending.empty() && m_chainPoint.isEmpty()) {
      return {}; // the start is clicked
    }
    return m_input.mode == SegmentInput::Mode::AxisLength ? QStringList{QStringLiteral("length")} : offsets;
  case Sketch3dTool::ArcCenter:
    return offsets + QStringList{QStringLiteral("radius"), QStringLiteral("arc_length")};
  case Sketch3dTool::ArcTangent:
    if (m_chainPoint.isEmpty()) {
      return {}; // a curve's end is clicked
    }
    return offsets + QStringList{QStringLiteral("radius"), QStringLiteral("arc_length")};
  case Sketch3dTool::Point:
  case Sketch3dTool::ArcThreePoint:
  case Sketch3dTool::SplineFit:
  case Sketch3dTool::SplineControl:
    return offsets;
  case Sketch3dTool::Fillet:
    return {QStringLiteral("radius")};
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::CircleTwoPoint:
  case Sketch3dTool::CircleThreePoint:
  case Sketch3dTool::Ellipse:
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
  case Sketch3dTool::RectangleThreePoint:
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
  case Sketch3dTool::PolygonEdge:
  case Sketch3dTool::Text:
    return shapeFields();
  case Sketch3dTool::None:
    break;
  }
  return {};
}

const TypedValue& Sketch3dController::field(const QString& id) const {
  if (id == QStringLiteral("dx")) {
    return m_input.offsets[0];
  }
  if (id == QStringLiteral("dy")) {
    return m_input.offsets[1];
  }
  if (id == QStringLiteral("dz")) {
    return m_input.offsets[2];
  }
  if (id == QStringLiteral("length")) {
    return m_input.length;
  }
  if (id == QStringLiteral("radius")) {
    return m_radius;
  }
  if (id == QStringLiteral("arc_length")) {
    return m_arcLength;
  }
  if (id == QStringLiteral("diameter") || id == QStringLiteral("major") || id == QStringLiteral("width")) {
    return m_size;
  }
  if (id == QStringLiteral("minor") || id == QStringLiteral("height")) {
    return m_size2;
  }
  if (id == QStringLiteral("sides")) {
    return m_sidesField;
  }
  if (id == QStringLiteral("text")) {
    return m_textField;
  }
  if (id == QStringLiteral("text_height")) {
    return m_textHeight;
  }
  return noField();
}

void Sketch3dController::setField(const QString& id, const QString& text) {
  TypedValue& value = const_cast<TypedValue&>(field(id));
  if (&value == &noField()) {
    return;
  }
  if (text.trimmed().isEmpty()) {
    value.clear(); // a measurement again
  } else if (id == QStringLiteral("text")) {
    // The text itself, as typed (mitcad#1550).
    value.edited = true;
    value.text = text;
    value.expression = text;
    value.valid = true;
    value.problem.clear();
  } else if (id == QStringLiteral("sides")) {
    // A polygon's sides: a whole number from 3 to 256.
    bool number = false;
    const int count = text.trimmed().toInt(&number);
    value.edited = true;
    value.text = text;
    value.value = count;
    value.expression = QString::number(count);
    value.valid = number && count >= 3 && count <= 256;
    value.problem = value.valid ? QString() : tr("a polygon has 3 to 256 sides");
  } else {
    value.edited = true;
    value.text = text;
    value.valid = false;
    value.problem.clear();
    try {
      const QJsonObject answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                                     {QStringLiteral("expression"), text},
                                                     {QStringLiteral("kind"), QStringLiteral("length")}});
      value.value = answer.value(QStringLiteral("value")).toDouble();
      const QString typed = answer.value(QStringLiteral("expression")).toString(text);
      const QString shown = answer.value(QStringLiteral("text")).toString();
      // A plain number takes the document's unit with it, as the planar
      // fields send it.
      bool number = false;
      const double plain = QString(typed).replace(QLatin1Char(','), QLatin1Char('.')).trimmed().toDouble(&number);
      Q_UNUSED(plain);
      const qsizetype space = shown.lastIndexOf(QLatin1Char(' '));
      value.expression = number && space > 0 ? typed.trimmed() + shown.mid(space) : typed;
      value.valid = std::isfinite(value.value);
      if (!value.valid) {
        value.problem = tr("not a finite value");
      }
    } catch (const std::exception& e) {
      value.problem = QString::fromUtf8(e.what());
    }
  }
  toolLog(QStringLiteral("3D sketch field %1: %2%3")
              .arg(id, value.edited ? value.text : QStringLiteral("(measured)"),
                   value.edited && !value.valid ? QStringLiteral(" (%1)").arg(value.problem) : QString()));
  updateHover();
  emit toolStateChanged();
}

QString Sketch3dController::measured(const QString& id) const {
  if (m_tool == Sketch3dTool::Fillet) {
    // The radius the pointer drags at the picked corner.
    return id == QStringLiteral("radius") && m_filletCorner && m_filletDragged
               ? formatLength(*m_filletDragged, millimetres())
               : QString();
  }
  if (isShapeTool(m_tool) && id != QStringLiteral("dx") && id != QStringLiteral("dy") && id != QStringLiteral("dz")) {
    return shapeMeasured(id); // a shape's sizes as the preview has them
  }
  if (!m_hoverAt) {
    return QString();
  }
  const PlacedPoint from = fromPoint();
  // Along the manipulator's axes (a rotated plane's, mitcad#1661).
  const Xyz d = inFrame(m_place.frame, sub(*m_hoverAt, from.at));
  const double unitMm = millimetres();
  if (id == QStringLiteral("dx")) {
    return formatLength(d[0], unitMm);
  }
  if (id == QStringLiteral("dy")) {
    return formatLength(d[1], unitMm);
  }
  if (id == QStringLiteral("dz")) {
    return formatLength(d[2], unitMm);
  }
  if (id == QStringLiteral("length")) {
    return formatLength(norm(d), unitMm);
  }
  // An arc's radius and arc length as the preview has them.
  std::optional<ArcShape> arc;
  if (m_tool == Sketch3dTool::ArcTangent && !m_chainPoint.isEmpty()) {
    if (const auto t = endTangent(m_model, m_chainCurve, m_chainPoint)) {
      arc = tangentArc(from.at, *t, *m_hoverAt);
    }
  } else if (m_tool == Sketch3dTool::ArcCenter && m_pending.size() == 2) {
    arc = centerArc(m_pending[0].at, m_pending[1].at, *m_hoverAt, arcNormal(), nullptr);
  } else if (m_tool == Sketch3dTool::ArcCenter && m_pending.size() == 1) {
    ArcShape around;
    around.radius = distance(m_pending[0].at, *m_hoverAt);
    arc = around;
  }
  if (!arc) {
    return QString();
  }
  if (id == QStringLiteral("radius")) {
    return formatLength(arc->radius, unitMm);
  }
  if (id == QStringLiteral("arc_length") && arc->sweep > 0.0) {
    return formatLength(arc->radius * arc->sweep, unitMm);
  }
  return QString();
}

// ---------------------------------------------------------------------------
// The pointer

std::optional<Ray3> Sketch3dController::rayAt(const QPointF& position) const {
  if (!m_model.hasFrame) {
    return std::nullopt;
  }
  const auto ray = m_viewer.pointerRay(position);
  if (!ray) {
    return std::nullopt;
  }
  gp_Trsf back = m_placement;
  back.Invert();
  gp_Vec direction(ray->second);
  direction.Transform(back);
  const Xyz d{direction.X(), direction.Y(), direction.Z()};
  const auto local = sketch::unit(Xyz{dot(d, m_model.xAxis), dot(d, m_model.yAxis), dot(d, m_model.normal)});
  if (!local) {
    return std::nullopt;
  }
  Ray3 out;
  out.from = localPoint(ray->first);
  out.direction = *local;
  return out;
}

QPointF Sketch3dController::onScreen(const Xyz& local) const { return m_viewer.toWidgetF(designPoint(local)); }

Xyz Sketch3dController::localPoint(const gp_Pnt& design) const {
  gp_Pnt p = design;
  gp_Trsf back = m_placement;
  back.Invert();
  p.Transform(back);
  const Xyz d{p.X() - m_model.origin[0], p.Y() - m_model.origin[1], p.Z() - m_model.origin[2]};
  return {dot(d, m_model.xAxis), dot(d, m_model.yAxis), dot(d, m_model.normal)};
}

PlacedPoint Sketch3dController::fromPoint() const {
  if (!m_pending.empty()) {
    return m_pending.back();
  }
  PlacedPoint from;
  const QString id = !m_chainPoint.isEmpty() ? m_chainPoint : m_anchorPoint;
  if (const Sketch3dPoint* point = id.isEmpty() ? nullptr : m_model.point(id)) {
    from.at = point->at;
    from.snap.kind = SnapKind::Point;
    from.snap.point = id;
    from.snap.at = point->at;
    from.snap.own = true;
    from.snap.what = id;
    from.snap.words = entityName(m_model, id);
    return from;
  }
  from.at = m_anchor;
  return from;
}

bool Sketch3dController::usesOffsets() const { return fieldIds().contains(QStringLiteral("dx")); }

void Sketch3dController::pointerAt(const QPointF& position) {
  m_pointer = position;
  m_hasPointer = true;
  updateHover();
}

void Sketch3dController::updateHover() {
  if (!toolActive() || !m_hasPointer) {
    return;
  }
  if (m_tool == Sketch3dTool::Fillet) {
    updateFilletHover(); // corners and lines, no place on a plane
    return;
  }
  const PlacedPoint from = fromPoint();
  const auto ray = rayAt(m_pointer);
  const std::optional<Xyz> placed = ray ? placePoint(*ray, from.at, toolPlacement()) : std::nullopt;
  m_unstable = !placed;
  if (placed) {
    m_placed = placed;
  }
  // Snaps where the pointer is in the view: while values are typed in the
  // palette, the place it left there snaps to nothing. A spline's or an
  // arc's own pending points are no snap targets: they are not points of
  // the sketch yet.
  std::vector<SnapTarget> candidates =
      m_pointerInView ? snapCandidates(m_pointer, from.at, m_placed) : std::vector<SnapTarget>();
  // The pending start is no place for the next point (mitcad#1661): with
  // the pointer still where the start was clicked, the origin's or a
  // point's snap there fixed the place and the typed offsets gave way to
  // it, a segment of no length. Typed offsets are frame differences from
  // the start on every placement plane (typedEnd).
  if (!m_pending.empty() || !m_chainPoint.isEmpty()) {
    candidates.erase(std::remove_if(candidates.begin(), candidates.end(),
                                    [&from](const SnapTarget& t) { return snapsOntoStart(t, from.at); }),
                     candidates.end());
  }
  const std::optional<SnapTarget> snap = chooseSnap(candidates, kSnapPixels);
  m_hoverSnap = snap.value_or(SnapTarget());
  std::optional<Xyz> at;
  if (snap) {
    at = snap->at;
  } else if (m_placed) {
    at = m_placed;
  }
  if (at && !m_hoverSnap.fixesPlace()) {
    // The typed values along the manipulator's axes (a rotated plane's
    // too, mitcad#1661).
    if (usesOffsets() && m_input.anyEdited()) {
      at = typedEnd(from.at, *at, m_input, m_place.frame);
    } else if (m_tool == Sketch3dTool::Line && m_input.mode == SegmentInput::Mode::AxisLength &&
               (!m_pending.empty() || !m_chainPoint.isEmpty())) {
      at = typedEnd(from.at, *at, m_input, m_place.frame);
    }
    // An arc's typed radius and arc length put its points.
    if (m_tool == Sketch3dTool::ArcCenter && m_pending.size() == 1 && m_radius.edited && m_radius.valid) {
      const Xyz n = arcNormal();
      const Xyz c = m_pending[0].at;
      const Xyz h = sub(*at, c);
      if (const auto toward = sketch::unit(sub(h, scaled(n, dot(h, n))))) {
        at = add(c, scaled(*toward, m_radius.value));
      }
    } else if (m_tool == Sketch3dTool::ArcCenter && m_pending.size() == 2) {
      Xyz end{};
      if (const auto arc = centerArc(m_pending[0].at, m_pending[1].at, *at, arcNormal(), &end)) {
        at = end;
        if (m_arcLength.edited && m_arcLength.valid && arc->radius > 0.0) {
          const double sweep = m_arcLength.value / arc->radius;
          at = add(arc->center, rotated(sub(m_pending[1].at, arc->center), arc->normal, sweep));
        }
      }
    } else if (m_tool == Sketch3dTool::ArcTangent && !m_chainPoint.isEmpty() &&
               (m_radius.edited || m_arcLength.edited)) {
      if (const auto t = endTangent(m_model, m_chainCurve, m_chainPoint)) {
        Xyz end{};
        const auto radius = m_radius.edited && m_radius.valid ? std::optional<double>(m_radius.value) : std::nullopt;
        const auto length =
            m_arcLength.edited && m_arcLength.valid ? std::optional<double>(m_arcLength.value) : std::nullopt;
        Xyz hint = *at;
        if (!tangentArc(from.at, *t, hint)) {
          // The pointer on the tangent's line (or away, typing): the arc
          // turns to the left of the tangent in the active plane.
          const auto left = sketch::unit(cross(placementNormal(m_place), *t));
          const auto side = left ? left : sketch::unit(cross(*t, Xyz{0.0, 0.0, 1.0}));
          if (side) {
            hint = add(from.at, scaled(*side, radius.value_or(10.0)));
          }
        }
        if (tangentArcSized(from.at, *t, hint, radius, length, &end)) {
          at = end;
        }
      }
    }
    // A shape's typed sizes put its next point (mitcad#1550).
    if (isShapeTool(m_tool) && !m_pending.empty()) {
      at = shapeHover(*at);
    }
  }
  m_hoverAt = at;
  m_problem = computeProblem();
  m_viewer.setSketchSnapping(snap.has_value() && snap->kind != SnapKind::Guide);
  // The badge says it in words; the log names it with its ids, and the
  // view paints its source (mitcad#1653).
  const QString badge = snapBadge(m_hoverSnap);
  const QString detail = snapDetail(m_hoverSnap);
  if (detail != m_loggedSnap) {
    m_loggedSnap = detail;
    if (!detail.isEmpty()) {
      toolLog(QStringLiteral("3D sketch snap: %1 at %2").arg(detail, xyzText(m_hoverSnap.at)));
      toolLog(QStringLiteral("3D sketch snap shown: %1").arg(badge));
      // Where its badge is drawn, kept inside the view (mitcad#1592).
      if (m_hoverAt) {
        toolLog(QStringLiteral("3D sketch snap badge %1 %2").arg(detail, badgePlace(*m_hoverAt, badge)));
      }
    }
  }
  paintSnap(snap.has_value() ? m_hoverSnap : SnapTarget());
  if (m_problem != m_loggedProblem) {
    m_loggedProblem = m_problem;
    if (!m_problem.isEmpty()) {
      toolLog(QStringLiteral("3D sketch problem: %1").arg(m_problem));
    }
  }
  emit toolStateChanged();
  updateOverlay();
  logHandles(); // the handles follow the anchor (mitcad#1661)
}

QString Sketch3dController::computeProblem() {
  m_planeHeldPoint.clear();
  if (!m_hoverAt) {
    return QString();
  }
  const PlacedPoint from = fromPoint();
  const Xyz& at = *m_hoverAt;
  // Typed values that are not valid.
  for (const QString& id : fieldIds()) {
    const TypedValue& value = field(id);
    if (value.edited && !value.valid) {
      return tr("%1: %2").arg(fieldLabel(id), value.problem);
    }
  }
  for (const TypedValue* positive : {&m_radius, &m_arcLength}) {
    if (positive->edited && positive->valid && !(positive->value > 1e-6)) {
      return tr("%1 must be greater than zero.").arg(positive == &m_radius ? tr("Radius") : tr("Arc Length"));
    }
  }
  const bool segment = m_tool == Sketch3dTool::Line && (!m_pending.empty() || !m_chainPoint.isEmpty());
  // Keep Axis along the manipulator's axis: a rotated plane's too
  // (mitcad#1661, stored along its direction).
  const bool keepsAxis = segment && m_keepAxis && m_place.mode == Placement::Mode::Axis;
  const std::optional<Axis3> keepAxis = keepsAxis ? std::optional<Axis3>(m_place.axis) : std::nullopt;
  if (segment) {
    if (const QString problem = inputProblem(from.at, at, m_input, keepAxis); !problem.isEmpty()) {
      return problem;
    }
  }
  // A snap the typed values contradict is said, never preferred: a
  // point of the sketch that cannot move, the origin, a linked point, a
  // midpoint.
  if (usesOffsets() || segment) {
    const Sketch3dPoint* shared = m_model.point(m_hoverSnap.point);
    const bool movable = (m_hoverSnap.kind == SnapKind::Point || m_hoverSnap.kind == SnapKind::Center) &&
                         shared != nullptr && !shared->determined;
    if (movable && shared->onSketchPlane) {
      // Typed values that lift a point On Sketch Plane keeps.
      const Xyz wanted = typedEnd(from.at, at, m_input, m_place.frame);
      if (m_input.anyEdited() && std::abs(wanted[2] - shared->at[2]) > 1e-6) {
        m_planeHeldPoint = shared->id;
        return tr("%1 is kept in the sketch plane by On Sketch Plane: Release from Plane frees it.").arg(shared->id);
      }
    } else if (!movable) {
      if (const QString conflict = snapConflict(from.at, m_hoverSnap, m_input, keepAxis, m_place.frame);
          !conflict.isEmpty()) {
        return conflict;
      }
    }
  }
  switch (m_tool) {
  case Sketch3dTool::ArcThreePoint:
    if (m_pending.size() == 2 && !threePointArc(m_pending[0].at, m_pending[1].at, at)) {
      return tr("The three points lie on one line: move the third off it.");
    }
    if (m_pending.size() == 2 && m_keepPlane) {
      const auto arc = threePointArc(m_pending[0].at, m_pending[1].at, at);
      if (arc && !keptPlaneNormal(arc->normal, m_place)) {
        return m_place.rotated() ? tr("Keep Plane: the arc's normal lies along no axis of the sketch.")
                                 : tr("Keep Plane: the arc does not lie in a %1 plane.").arg(planeLabel(m_place.plane));
      }
    }
    break;
  case Sketch3dTool::ArcCenter:
    if (m_pending.size() >= 1 && distance(m_pending[0].at, at) <= 1e-6) {
      return tr("The point is on the arc's center.");
    }
    if (m_keepPlane && !axisNormal(arcNormal())) {
      return tr("Keep Plane: the arc's normal lies along no axis of the sketch.");
    }
    break;
  case Sketch3dTool::ArcTangent:
    if (!m_chainPoint.isEmpty()) {
      const auto t = endTangent(m_model, m_chainCurve, m_chainPoint);
      const auto arc = t ? tangentArc(from.at, *t, at) : std::nullopt;
      if (!arc) {
        return tr("The end lies on the tangent's line: move it off the line.");
      }
      if (m_keepPlane && !keptPlaneNormal(arc->normal, m_place)) {
        return m_place.rotated() ? tr("Keep Plane: the arc's normal lies along no axis of the sketch.")
                                 : tr("Keep Plane: the arc does not lie in a %1 plane.").arg(planeLabel(m_place.plane));
      }
    }
    break;
  case Sketch3dTool::SplineFit:
  case Sketch3dTool::SplineControl:
    if (!m_pending.empty() && distance(m_pending.back().at, at) <= 1e-6) {
      return tr("The point is on the one before.");
    }
    break;
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::CircleTwoPoint:
  case Sketch3dTool::CircleThreePoint:
  case Sketch3dTool::Ellipse:
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
  case Sketch3dTool::RectangleThreePoint:
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
  case Sketch3dTool::PolygonEdge:
  case Sketch3dTool::Text:
    return shapeProblem(at);
  case Sketch3dTool::Point:
  case Sketch3dTool::Line:
  case Sketch3dTool::Fillet:
  case Sketch3dTool::None:
    break;
  }
  return QString();
}

QString Sketch3dController::toolHint() const {
  QString hint;
  const std::size_t placed = m_pending.size();
  switch (m_tool) {
  case Sketch3dTool::Point:
    hint = tr("Point: click where it goes, or type Delta X, Y and Z from the anchor and press Enter.");
    break;
  case Sketch3dTool::Line:
    hint = placed == 0 && m_chainPoint.isEmpty()
               ? tr("Line: click its start. A point of the sketch, the origin, a vertex or an edge snaps.")
               : tr("Line: click its end, or type the values and press Enter. A double-click or Esc ends the chain.");
    break;
  case Sketch3dTool::ArcThreePoint:
    hint = placed == 0   ? tr("3-Point Arc: click its start.")
           : placed == 1 ? tr("3-Point Arc: click its end.")
                         : tr("3-Point Arc: click a point it passes through.");
    break;
  case Sketch3dTool::ArcCenter:
    if (m_pickingNormal) {
      hint = tr("Center Point Arc: click a line, a straight edge, an axis or a planar face; the arc's normal "
                "takes its direction.");
      break;
    }
    hint = placed == 0   ? tr("Center Point Arc: click its center. Its normal is %1; Pick Normal takes another.")
                               .arg(arcNormalText())
           : placed == 1 ? tr("Center Point Arc: click its start; Radius sets the radius.")
                         : tr("Center Point Arc: click its end; Arc Length sets the sweep.");
    break;
  case Sketch3dTool::ArcTangent:
    hint = m_chainPoint.isEmpty()
               ? tr("Tangent Arc: click the end of a line, an arc or a spline to continue it.")
               : tr("Tangent Arc: click its end, or type its Radius and Arc Length and press Enter.");
    break;
  case Sketch3dTool::SplineFit:
    hint = tr("Fit Point Spline: click its points; Enter or a double-click ends it, a click on its first point "
              "closes it.");
    break;
  case Sketch3dTool::SplineControl:
    hint = tr("Control Point Spline: click its control points; Enter or a double-click ends it.");
    break;
  case Sketch3dTool::Fillet:
    return filletHint();
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::CircleTwoPoint:
  case Sketch3dTool::CircleThreePoint:
  case Sketch3dTool::Ellipse:
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
  case Sketch3dTool::RectangleThreePoint:
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
  case Sketch3dTool::PolygonEdge:
  case Sketch3dTool::Text:
    hint = shapeHint();
    break;
  case Sketch3dTool::None:
    return QString();
  }
  // The manipulator's keys (mitcad#1661), as Keyboard Shortcuts has them.
  if (const QString keys = placementKeysHint(); !keys.isEmpty()) {
    hint += QLatin1Char(' ') + keys;
  }
  if (m_unstable) {
    hint += QLatin1Char(' ') + tr("The pointer's place is unstable here (%1 seen edge-on): choose another plane, "
                                  "type the values or turn the view.")
                                   .arg(placementText(toolPlacement()));
  }
  return hint;
}

QString Sketch3dController::snapText() const { return snapBadge(m_hoverSnap); }

// ---------------------------------------------------------------------------
// Placing and committing

void Sketch3dController::place() {
  if (m_pickingNormal) {
    pickNormal(); // the click picks the arc's normal, it places nothing
    return;
  }
  if (m_tool == Sketch3dTool::Fillet) {
    filletRelease(true); // a corner or a line, no point
    return;
  }
  if (!m_hoverAt) {
    m_host.showHint(toolHint());
    toolLog(QStringLiteral("3D sketch %1: no place under the pointer at %2,%3 (%4)")
                .arg(logName(m_tool))
                .arg(qRound(m_pointer.x()))
                .arg(qRound(m_pointer.y()))
                .arg(m_unstable ? QStringLiteral("unstable") : QStringLiteral("no ray")));
    return;
  }
  if (!m_problem.isEmpty()) {
    m_host.showError(m_problem);
    // With where it would go and the snap, for agents and the log.
    const QString snap = snapDetail(m_hoverSnap);
    toolLog(QStringLiteral("3D sketch %1: refused: %2 (from %3 to %4, %5, snap %6)")
                .arg(logName(m_tool), m_problem, xyzText(fromPoint().at), xyzText(*m_hoverAt),
                     placementText(m_place), snap.isEmpty() ? QStringLiteral("none") : snap));
    return;
  }
  PlacedPoint point;
  point.at = *m_hoverAt;
  point.snap = m_hoverSnap;
  if (point.snap.kind == SnapKind::Guide) {
    point.snap = SnapTarget(); // guidance adds no relation
  }
  point.snap.at = point.at;
  // A typed offset of a point (not a line's end, whose command takes them)
  // becomes a driving offset dimension from the point before it.
  if (m_tool != Sketch3dTool::Line && m_input.mode == SegmentInput::Mode::Offsets) {
    for (std::size_t k = 0; k < 3; ++k) {
      if (m_input.offsets[k].edited && m_input.offsets[k].valid) {
        point.offsets[k] = m_input.offsets[k].expression;
      }
    }
    point.offsetFrame = m_place.frame; // along a rotated plane's axes (mitcad#1661)
  }
  const auto pendingLog = [this](const PlacedPoint& p) {
    toolLog(QStringLiteral("3D sketch %1: pending point %2 at %3%4")
                .arg(logName(m_tool))
                .arg(m_pending.size())
                .arg(xyzText(p.at),
                     p.snap.kind == SnapKind::Free ? QString() : QStringLiteral(" (%1)").arg(snapDetail(p.snap))));
  };
  switch (m_tool) {
  case Sketch3dTool::Point:
    commitPoint(point);
    break;
  case Sketch3dTool::Line:
    if (m_pending.empty() && m_chainPoint.isEmpty()) {
      m_pending.push_back(point);
      pendingLog(point);
      resetInput();
    } else {
      commitLine(fromPoint(), point);
    }
    break;
  case Sketch3dTool::ArcThreePoint:
  case Sketch3dTool::ArcCenter:
    m_pending.push_back(point);
    if (m_pending.size() == 3) {
      commitArc();
    } else {
      pendingLog(point);
      // The radius stays typed for the end: it is the arc's.
      const TypedValue radius = m_radius;
      resetInput();
      m_radius = radius;
    }
    break;
  case Sketch3dTool::ArcTangent:
    if (m_chainPoint.isEmpty()) {
      const QStringList curves = point.snap.kind == SnapKind::Point || point.snap.kind == SnapKind::Center
                                     ? continuableCurves(m_model, point.snap.point)
                                     : QStringList();
      if (curves.isEmpty()) {
        m_host.showHint(tr("Click the end of a line, an arc or a spline to continue it."));
        toolLog(QStringLiteral("3D sketch Tangent Arc: no curve ends at the click"));
        return;
      }
      m_chainPoint = point.snap.point;
      m_chainCurve = curves.first();
      toolLog(QStringLiteral("3D sketch Tangent Arc: continues %1 at %2").arg(m_chainCurve, m_chainPoint));
      resetInput();
    } else {
      m_pending = {point};
      commitArc();
    }
    break;
  case Sketch3dTool::SplineFit:
  case Sketch3dTool::SplineControl:
    if (m_tool == Sketch3dTool::SplineFit && m_pending.size() >= 3 &&
        QLineF(onScreen(m_pending.front().at), m_pointer).length() <= kSnapPixels) {
      commitSpline(true); // a click on its first point closes it
      return;
    }
    m_pending.push_back(point);
    pendingLog(point);
    resetInput();
    break;
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::CircleTwoPoint:
  case Sketch3dTool::CircleThreePoint:
  case Sketch3dTool::Ellipse:
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
  case Sketch3dTool::RectangleThreePoint:
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
  case Sketch3dTool::PolygonEdge:
  case Sketch3dTool::Text:
    // A shape's clicks (mitcad#1550): its sizes typed stay for its commit;
    // offsets typed placed only this point. A text's new click moves it.
    if (m_tool == Sketch3dTool::Text) {
      m_pending.clear();
    }
    m_pending.push_back(point);
    pendingLog(point);
    m_input.clearAll();
    if (static_cast<int>(m_pending.size()) >= shapeClicks() &&
        (m_tool != Sketch3dTool::Text || m_textField.edited)) {
      commitShape();
      return;
    }
    if (m_tool == Sketch3dTool::Text) {
      m_host.showHint(tr("Text: type the text and press Enter."));
    }
    break;
  case Sketch3dTool::Fillet:
  case Sketch3dTool::None:
    return;
  }
  updateHover();
  emit toolStateChanged();
}

int Sketch3dController::undoDepth() const {
  try {
    return m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
        .value(QStringLiteral("undo_depth"))
        .toInt();
  } catch (const std::exception&) {
    return 0;
  }
}

bool Sketch3dController::commitSteps(const QJsonObject& first,
                                     const std::function<QJsonArray(const QJsonObject&)>& then, const QString& label,
                                     QJsonObject* result) {
  const int depth = undoDepth();
  QJsonObject made;
  toolLog(QStringLiteral("3D sketch command: %1").arg(compact(first)));
  if (!m_host.runCommand(first, &made)) {
    toolLog(QStringLiteral("3D sketch %1: refused by the model").arg(logName(m_tool)));
    m_host.refreshScene();
    return false;
  }
  const QJsonArray more = then ? then(made) : QJsonArray();
  for (const QJsonValue& next : more) {
    toolLog(QStringLiteral("3D sketch command: %1").arg(compact(next.toObject())));
    if (!m_host.runCommand(next.toObject())) {
      // Nothing of the edit stays.
      for (int guard = 0; guard <= more.size() && undoDepth() > depth; ++guard) {
        if (!m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}})) {
          break;
        }
      }
      toolLog(QStringLiteral("3D sketch %1: refused by the model; nothing kept").arg(logName(m_tool)));
      m_host.refreshScene();
      return false;
    }
  }
  if (undoDepth() > depth + 1) {
    m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                       {QStringLiteral("depth"), depth},
                       {QStringLiteral("label"), label}});
  }
  if (result != nullptr) {
    *result = made;
  }
  m_host.refreshScene();
  return true;
}

void Sketch3dController::afterCommit(const QString& endPoint) {
  if (const Sketch3dPoint* point = m_model.point(endPoint)) {
    m_anchor = point->at;
    m_anchorPoint = endPoint;
  }
  if (m_place.mode == Placement::Mode::Normal) {
    m_place.normalBase.reset();
    m_place.mode = Placement::Mode::Plane; // Normal guided the one point
  }
  resetInput();
  m_planeHeldPoint.clear();
  toolLog(QStringLiteral("3D sketch anchor at %1 %2").arg(m_anchorPoint, xyzText(m_anchor)));
  updateHover();
  emit toolStateChanged();
}

void Sketch3dController::commitPoint(const PlacedPoint& point) {
  const QString previous = m_anchorPoint;
  QJsonObject result;
  if (!commitSteps(
          pointCommand(m_uid, point),
          [&](const QJsonObject& made) {
            return followUps(m_uid, {point}, {madeId(made)}, previous, false);
          },
          QStringLiteral("Add Point in %1").arg(m_model.name), &result)) {
    return;
  }
  const QString id = madeId(result);
  toolLog(QStringLiteral("3D sketch Point: made %1").arg(id));
  afterCommit(id);
}

void Sketch3dController::commitLine(const PlacedPoint& start, const PlacedPoint& end) {
  const std::optional<Axis3> keepAxis =
      m_keepAxis && m_place.mode == Placement::Mode::Axis ? std::optional<Axis3>(m_place.axis) : std::nullopt;
  // The axis and the offsets along the manipulator's axes (mitcad#1661).
  const QJsonObject cmd = lineCommand(m_uid, start, end, m_input, keepAxis, m_construction, m_place.frame);
  QJsonObject result;
  if (!commitSteps(
          cmd,
          [&](const QJsonObject& made) {
            return followUps(m_uid, {start, end},
                             {made.value(QStringLiteral("start")).toString(), made.value(QStringLiteral("end")).toString()},
                             QString(), false);
          },
          QStringLiteral("Add Line in %1").arg(m_model.name), &result)) {
    return;
  }
  const QString endId = result.value(QStringLiteral("end")).toString();
  toolLog(QStringLiteral("3D sketch Line: made %1 from %2 to %3")
              .arg(madeId(result), result.value(QStringLiteral("start")).toString(),
                   endId));
  // The next segment starts at this end, shared.
  m_pending.clear();
  m_chainPoint = endId;
  afterCommit(endId);
}

void Sketch3dController::commitArc() {
  ArcRequest arc;
  arc.construction = m_construction;
  if (m_radius.edited && m_radius.valid) {
    arc.radius = m_radius.expression;
  }
  if (m_arcLength.edited && m_arcLength.valid) {
    arc.arcLength = m_arcLength.expression;
  }
  QString previous = m_anchorPoint;
  switch (m_tool) {
  case Sketch3dTool::ArcThreePoint: {
    arc.mode = QStringLiteral("three_point");
    arc.start = m_pending[0];
    arc.end = m_pending[1];
    arc.through = m_pending[2].at;
    if (m_keepPlane) {
      if (const auto shape = threePointArc(arc.start.at, arc.end.at, arc.through)) {
        arc.keepPlane = keptPlaneNormal(shape->normal, m_place);
      }
    }
    break;
  }
  case Sketch3dTool::ArcCenter:
    arc.mode = QStringLiteral("center");
    arc.center = m_pending[0];
    arc.start = m_pending[1];
    arc.end = m_pending[2];
    arc.normal = arcNormal();
    if (m_keepPlane) {
      arc.keepPlane = axisNormal(arc.normal);
    }
    break;
  case Sketch3dTool::ArcTangent: {
    arc.mode = QStringLiteral("tangent");
    arc.from = m_chainCurve;
    arc.start = fromPointOfChain();
    arc.end = m_pending[0];
    previous = m_chainPoint;
    if (m_keepPlane) {
      if (const auto t = endTangent(m_model, m_chainCurve, m_chainPoint)) {
        if (const auto shape = tangentArc(arc.start.at, *t, arc.end.at)) {
          arc.keepPlane = keptPlaneNormal(shape->normal, m_place);
        }
      }
    }
    break;
  }
  default:
    return;
  }
  const QJsonObject cmd = arcCommand(m_uid, arc);
  QJsonObject result;
  const bool ok = commitSteps(
      cmd,
      [&](const QJsonObject& made) {
        const QString center = made.value(QStringLiteral("center")).toString();
        const QString start = made.value(QStringLiteral("start")).toString();
        const QString end = made.value(QStringLiteral("end")).toString();
        QJsonArray steps;
        if (arc.mode == QStringLiteral("center")) {
          // The center's own source: add_arc takes the ends' only.
          for (const QJsonValue& step : followUps(m_uid, {arc.center}, {center}, previous, true)) {
            steps.append(step);
          }
          for (const QJsonValue& step : followUps(m_uid, {arc.start, arc.end}, {start, end}, center, false)) {
            steps.append(step);
          }
        } else if (arc.mode == QStringLiteral("three_point")) {
          for (const QJsonValue& step : followUps(m_uid, {arc.start, arc.end}, {start, end}, previous, false)) {
            steps.append(step);
          }
        } else {
          for (const QJsonValue& step : followUps(m_uid, {arc.end}, {end}, previous, false)) {
            steps.append(step);
          }
        }
        return steps;
      },
      QStringLiteral("Add Arc in %1").arg(m_model.name), &result);
  if (!ok) {
    if (m_tool == Sketch3dTool::ArcTangent) {
      m_pending.clear(); // the chain stays: another end can be tried
    } else {
      m_pending.pop_back(); // the last point can be placed again
    }
    updateHover();
    return;
  }
  const QString made = madeId(result);
  const QString endId = result.value(QStringLiteral("end")).toString();
  toolLog(QStringLiteral("3D sketch %1: made %2 from %3 to %4")
              .arg(logName(m_tool), made, result.value(QStringLiteral("start")).toString(), endId));
  m_pending.clear();
  if (m_tool == Sketch3dTool::ArcTangent) {
    // The next tangent arc continues this one.
    m_chainPoint = endId;
    m_chainCurve = made;
  }
  afterCommit(endId);
}

PlacedPoint Sketch3dController::fromPointOfChain() const {
  PlacedPoint start;
  if (const Sketch3dPoint* point = m_model.point(m_chainPoint)) {
    start.at = point->at;
    start.snap.kind = SnapKind::Point;
    start.snap.point = m_chainPoint;
    start.snap.at = point->at;
    start.snap.own = true;
  }
  return start;
}

void Sketch3dController::commitSpline(bool closed) {
  const bool control = m_tool == Sketch3dTool::SplineControl;
  if (m_pending.size() < (closed ? 3u : 2u)) {
    return;
  }
  const std::vector<PlacedPoint> points = m_pending;
  const QString previous = m_anchorPoint;
  QJsonObject result;
  if (!commitSteps(
          splineCommand(m_uid, points, control, closed, m_construction),
          [&](const QJsonObject& made) {
            QStringList ids;
            for (const QJsonValue& id : made.value(QStringLiteral("spline_points")).toArray()) {
              ids << id.toString();
            }
            return followUps(m_uid, points, ids, previous, true);
          },
          QStringLiteral("Add Spline in %1").arg(m_model.name), &result)) {
    return;
  }
  const QJsonArray ids = result.value(QStringLiteral("spline_points")).toArray();
  toolLog(QStringLiteral("3D sketch %1: made %2%3 through %4 points")
              .arg(logName(m_tool), madeId(result),
                   closed ? QStringLiteral(" (closed)") : QString())
              .arg(ids.size()));
  m_pending.clear();
  afterCommit(ids.isEmpty() ? QString() : ids.last().toString());
}

void Sketch3dController::releaseHeldPoint() {
  if (m_planeHeldPoint.isEmpty()) {
    return;
  }
  const QString point = m_planeHeldPoint;
  if (commitSteps({{QStringLiteral("cmd"), QStringLiteral("sketch3d.release_from_plane")},
                   {QStringLiteral("sketch"), m_uid},
                   {QStringLiteral("points"), QJsonArray{point}}},
                  {}, QStringLiteral("Release from Plane in %1").arg(m_model.name), nullptr)) {
    toolLog(QStringLiteral("3D sketch: %1 released from the sketch plane").arg(point));
  }
  updateHover();
}

// ---------------------------------------------------------------------------
// The view's events while a tool draws

bool Sketch3dController::toolMouse(QEvent* event) {
  // A rotation handle's press, drag and release (mitcad#1661).
  if (rotationMouse(event)) {
    return true;
  }
  switch (event->type()) {
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    // While the view is turned, panned or zoomed, the pending input waits;
    // the place follows once the camera rests.
    if ((e->buttons() & (Qt::MiddleButton | Qt::RightButton)) != 0) {
      m_pointer = e->position();
      return false;
    }
    if (m_tool == Sketch3dTool::Text && m_leftPressed && m_textPress && !m_textDragging &&
        QLineF(e->position(), m_pressAt).length() > kClickPixels) {
      // Past a click: a frame is dragged, previewed from the press on.
      m_textDragging = true;
      toolLog(QStringLiteral("3D sketch Text: frame preview from %1").arg(xyzText(*m_textPress)));
    }
    pointerAt(e->position());
    return (e->buttons() & Qt::LeftButton) != 0;
  }
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || m_viewer.isOnOrientationCube(e->position())) {
      return false;
    }
    m_leftPressed = true;
    m_pressAt = e->position();
    if (m_tool == Sketch3dTool::Text && m_pending.empty()) {
      // Where a dragged frame starts: the place under the press.
      m_pointerInView = !m_viewer.isCovered(e->position());
      pointerAt(e->position());
      m_textPress = m_hoverAt;
      m_textDragging = false;
    }
    if (m_tool == Sketch3dTool::Fillet) {
      // A press on a corner picks it: a drag from it sets the radius.
      m_pointerInView = !m_viewer.isCovered(e->position());
      pointerAt(e->position());
      filletPress();
    }
    return true;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || !m_leftPressed) {
      return false;
    }
    m_leftPressed = false;
    // The frame's preview ends with the press; a drag's frame is placed below.
    m_textPress.reset();
    m_textDragging = false;
    if (m_tool == Sketch3dTool::Fillet) {
      m_pointerInView = !m_viewer.isCovered(e->position());
      pointerAt(e->position());
      filletRelease(QLineF(e->position(), m_pressAt).length() <= kClickPixels);
      return true;
    }
    if (QLineF(e->position(), m_pressAt).length() <= kClickPixels) {
      // A click posted without a move before it (Windows UI tests) is in
      // the view where it is: it snaps there.
      m_pointerInView = !m_viewer.isCovered(e->position());
      if (m_tool == Sketch3dTool::Text) {
        m_textFrame.reset(); // a click: a text without a frame
      }
      pointerAt(e->position());
      place();
    } else if (m_tool == Sketch3dTool::Text) {
      // A drag draws a text's frame (mitcad#1550): from the press to here.
      m_pointerInView = !m_viewer.isCovered(e->position());
      pointerAt(e->position());
      m_textFrame = m_hoverAt;
      pointerAt(m_pressAt);
      toolLog(QStringLiteral("3D sketch Text: frame to %1").arg(m_textFrame ? xyzText(*m_textFrame) : QString()));
      place();
    }
    return true;
  }
  case QEvent::MouseButtonDblClick: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    m_leftPressed = false;
    // The first click placed the point; the double-click ends the chain or
    // the spline.
    if (isSpline(m_tool)) {
      if (m_pending.size() >= 2) {
        commitSpline(false);
      }
    } else if (m_tool == Sketch3dTool::Line || m_tool == Sketch3dTool::ArcTangent) {
      if (!m_pending.empty() || !m_chainPoint.isEmpty()) {
        finishChain();
      }
    }
    return true;
  }
  default:
    break;
  }
  return false;
}

void Sketch3dController::toolLog(const QString& text) const { qDebug().noquote() << text; }

// ---------------------------------------------------------------------------
// The preview

Sketch3dPreview Sketch3dController::preview() const {
  Sketch3dPreview out;
  if (!toolActive() || !m_model.hasFrame) {
    return out;
  }
  if (m_tool == Sketch3dTool::Fillet) {
    addFilletPreview(out); // no temporary plane: it places no point
    return out;
  }
  using Style = Sketch3dPreview::Style;
  const PlacedPoint from = fromPoint();
  const Xyz anchor = from.at;
  // The temporary plane and the axes through the anchor, as large on the
  // screen whatever the zoom, in the manipulator's frame: the sketch's own,
  // or as the rotation handles turned it (mitcad#1661).
  const Placement placing = toolPlacement();
  const double size = manipulatorSize(anchor);
  const std::array<Xyz, 2> inPlane = placementPlaneAxes(placing);
  const Xyz u = scaled(inPlane[0], size);
  const Xyz v = scaled(inPlane[1], size);
  if (m_showPlane) {
    Sketch3dPreview::Path plane;
    plane.style = Style::Plane;
    plane.closed = true;
    plane.axis = static_cast<int>(planeNormalAxis(placing.plane));
    plane.points = {add(add(anchor, u), v), add(sub(anchor, u), v), sub(sub(anchor, u), v), sub(add(anchor, u), v)};
    out.paths.push_back(plane);
  }
  for (const Axis3 axis : {Axis3::X, Axis3::Y, Axis3::Z}) {
    Sketch3dPreview::Path line;
    line.style = Style::Axis;
    line.axis = static_cast<int>(axis);
    const Xyz reach = scaled(placementAxis(placing, axis), size * 1.4);
    line.points = {sub(anchor, reach), add(anchor, reach)};
    line.active = placing.mode == Placement::Mode::Axis && placing.axis == axis;
    out.paths.push_back(line);
  }
  if (placing.mode == Placement::Mode::Normal) {
    Sketch3dPreview::Path normal;
    normal.style = Style::Axis;
    normal.axis = static_cast<int>(planeNormalAxis(placing.plane));
    normal.active = true;
    const Xyz base = placing.normalBase.value_or(anchor);
    const Xyz reach = scaled(placementNormal(placing), size * 1.4);
    normal.points = {sub(base, reach), add(base, reach)};
    out.paths.push_back(normal);
  }
  // The rotation handles: a ring about each of the manipulator's axes; the
  // one under the pointer or dragged drawn strong.
  if (m_showHandles && offersRotation()) {
    const Xyz center = m_rotation.active ? m_rotation.center : anchor;
    const double radius = ringRadius(center);
    for (const Axis3 axis : {Axis3::X, Axis3::Y, Axis3::Z}) {
      Sketch3dPreview::Path ring;
      ring.style = Style::Ring;
      ring.axis = static_cast<int>(axis);
      ring.closed = true;
      ring.points = handleRing(center, m_place.frame, axis, radius, kRingSegments);
      ring.active = m_rotation.active ? m_rotation.axis == axis : m_ringHover == axis;
      out.paths.push_back(ring);
    }
  }
  for (const PlacedPoint& point : m_pending) {
    out.placed.push_back(point.at);
  }
  if (!m_chainPoint.isEmpty()) {
    out.placed.push_back(from.at);
  }
  out.hover = m_hoverAt;
  out.snapped = m_hoverSnap.kind != SnapKind::Free && m_hoverSnap.kind != SnapKind::Guide;
  out.badge = snapBadge(m_hoverSnap);
  out.conflict = !m_problem.isEmpty();
  if (m_unstable) {
    out.unstable = tr("%1 seen edge-on: the last place is kept").arg(placementText(placing));
  }
  if (!m_hoverAt) {
    return out;
  }
  const Xyz& at = *m_hoverAt;
  const Style draw = m_construction ? Style::Construction : Style::Draw;
  const auto path = [&out](std::vector<Xyz> points, Style style) {
    Sketch3dPreview::Path p;
    p.points = std::move(points);
    p.style = style;
    out.paths.push_back(std::move(p));
  };
  switch (m_tool) {
  case Sketch3dTool::Line:
    if (!m_pending.empty() || !m_chainPoint.isEmpty()) {
      path({from.at, at}, draw);
    }
    break;
  case Sketch3dTool::ArcThreePoint:
    if (m_pending.size() == 1) {
      path({m_pending[0].at, at}, Style::Guide);
    } else if (m_pending.size() == 2) {
      if (const auto arc = threePointArc(m_pending[0].at, m_pending[1].at, at)) {
        path(arcPoints(*arc, m_pending[0].at, 48), draw);
      }
    }
    break;
  case Sketch3dTool::ArcCenter:
    if (m_pending.size() == 1) {
      path({m_pending[0].at, at}, Style::Guide);
    } else if (m_pending.size() == 2) {
      path({m_pending[0].at, m_pending[1].at}, Style::Guide);
      if (const auto arc = centerArc(m_pending[0].at, m_pending[1].at, at, arcNormal(), nullptr)) {
        ArcShape shown = *arc;
        shown.sweep = angleAbout(sub(m_pending[1].at, arc->center), sub(at, arc->center), arc->normal);
        path(arcPoints(shown, m_pending[1].at, 48), draw);
      }
    }
    break;
  case Sketch3dTool::ArcTangent:
    if (!m_chainPoint.isEmpty()) {
      if (const auto t = endTangent(m_model, m_chainCurve, m_chainPoint)) {
        if (const auto arc = tangentArc(from.at, *t, at)) {
          path(arcPoints(*arc, from.at, 48), draw);
        }
      }
    }
    break;
  case Sketch3dTool::SplineFit:
  case Sketch3dTool::SplineControl: {
    std::vector<Xyz> points;
    for (const PlacedPoint& point : m_pending) {
      points.push_back(point.at);
    }
    points.push_back(at);
    if (points.size() >= 2) {
      if (m_tool == Sketch3dTool::SplineControl) {
        path(points, Style::Guide);
        path(controlSplinePreview(points), draw);
      } else {
        path(fitSplinePreview(points, false), draw);
      }
    }
    break;
  }
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::CircleTwoPoint:
  case Sketch3dTool::CircleThreePoint:
  case Sketch3dTool::Ellipse:
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
  case Sketch3dTool::RectangleThreePoint:
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
  case Sketch3dTool::PolygonEdge:
  case Sketch3dTool::Text:
    addShapePreview(out, at);
    break;
  case Sketch3dTool::Point:
  case Sketch3dTool::Fillet:
  case Sketch3dTool::None:
    break;
  }
  // Depth: a drop line from the next point to the temporary plane through
  // the anchor when it is off it.
  const Xyz n = placementNormal(placing);
  const double off = dot(sub(at, anchor), n);
  if (std::abs(off) > 1e-6) {
    path({at, sub(at, scaled(n, off))}, Style::Guide);
  }
  return out;
}

} // namespace mitcad::sketch
