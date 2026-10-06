// SPDX-License-Identifier: MIT
#include "SketchController.hpp"

#include <algorithm>
#include <cmath>
#include <limits>

#include <QCoreApplication>
#include <QJsonArray>
#include <QKeyEvent>
#include <QLineEdit>
#include <QResizeEvent>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "../framework/Json.hpp"
#include "../framework/ModelShapes.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/TestSync.hpp"
#include "SketchOverlay.hpp"
#include "SketchTool.hpp"

namespace mitcad::sketch {
namespace {

// A press that moves further than this starts a drag.
constexpr double kDragPixels = 4.0;
// Sketch geometry this near the cursor is under it.
constexpr double kPointPixels = 8.0;
constexpr double kCurvePixels = 6.0;

// The fields float over the view, whose background does not follow the
// palette: their own light background needs its own dark text.
const QString kFieldStyle = QStringLiteral(
    "QLineEdit { background: rgba(255, 255, 255, 240); color: #202428; border: 1px solid #7aa6dc;"
    " border-radius: 3px; padding: 1px 4px; selection-background-color: #7aa6dc;"
    " selection-color: #101418; }");
const QString kInvalidFieldStyle = QStringLiteral(
    "QLineEdit { background: rgba(255, 240, 240, 240); color: #202428; border: 1px solid #c02020;"
    " border-radius: 3px; padding: 1px 4px; selection-background-color: #7aa6dc;"
    " selection-color: #101418; }");

double unitScale(const QString& unit) {
  if (unit == QStringLiteral("cm")) {
    return 10.0;
  }
  if (unit == QStringLiteral("m")) {
    return 1000.0;
  }
  if (unit == QStringLiteral("in")) {
    return 25.4;
  }
  if (unit == QStringLiteral("ft")) {
    return 304.8;
  }
  if (unit == QStringLiteral("um")) {
    return 0.001;
  }
  return 1.0;
}

QString kindKey(FieldSpec::Kind kind) {
  switch (kind) {
  case FieldSpec::Kind::Angle:
    return QStringLiteral("angle");
  case FieldSpec::Kind::Count:
    return QStringLiteral("unitless");
  case FieldSpec::Kind::Length:
  case FieldSpec::Kind::Text:
    break;
  }
  return QStringLiteral("length");
}

double screenDistance(const QPointF& a, const QPointF& b) {
  return std::hypot(a.x() - b.x(), a.y() - b.y());
}

QJsonArray xy(const V2& p) { return QJsonArray{p.x, p.y}; }

} // namespace

// ---------------------------------------------------------------------------
// SketchOp

SketchOp::SketchOp(SketchController& controller) : m_c(controller), m_depth(controller.undoDepth()) {}

SketchOp::~SketchOp() {
  if (!m_finished) {
    rollback();
  }
}

QJsonObject SketchOp::command(const QString& name) const {
  return {{QStringLiteral("cmd"), name}, {QStringLiteral("sketch"), m_c.uid()}};
}

bool SketchOp::run(const QJsonObject& command, QJsonObject* result) {
  try {
    const QJsonObject answer = m_c.runModel(command);
    if (m_label.isEmpty()) {
      m_label = m_c.undoLabel();
    }
    m_stale = true;
    if (result != nullptr) {
      *result = answer;
    }
    return true;
  } catch (const std::exception& e) {
    m_error = errorText(e);
    qDebug().noquote() << QStringLiteral("Sketch command %1 refused: %2")
                              .arg(command.value(QStringLiteral("cmd")).toString(), m_error);
    return false;
  }
}

bool SketchOp::attempt(const QJsonObject& command) {
  const QString kept = m_error;
  const bool done = run(command);
  m_error = kept;
  return done;
}

QJsonValue SketchOp::pointInput(const Snap& snap) {
  if (snap.atPoint()) {
    return snap.point;
  }
  if (snap.kind == Snap::Kind::Origin) {
    const QString origin = model().originPoint();
    if (!origin.isEmpty()) {
      return origin;
    }
    // The origin is a fixed point of the sketch, made when something
    // first snaps there.
    QJsonObject cmd = command(QStringLiteral("sketch.add_point"));
    cmd.insert(QStringLiteral("at"), QJsonArray{0.0, 0.0});
    cmd.insert(QStringLiteral("fixed"), true);
    QJsonObject result;
    if (run(cmd, &result)) {
      const QJsonArray made = result.value(QStringLiteral("entities")).toArray();
      if (!made.isEmpty()) {
        return made.first().toString();
      }
    }
  }
  return xy(snap.at);
}

void SketchOp::constrain(const QString& point, const Snap& snap) {
  const auto coincident = [&](const QString& entity) {
    if (entity.isEmpty() || entity == point) {
      return;
    }
    addConstraint({{QStringLiteral("type"), QStringLiteral("coincident")},
                   {QStringLiteral("point"), point},
                   {QStringLiteral("entity"), entity}});
  };
  switch (snap.kind) {
  case Snap::Kind::Point:
  case Snap::Kind::Center:
    coincident(snap.point);
    break;
  case Snap::Kind::Origin: {
    const QJsonValue origin = pointInput(snap);
    coincident(origin.toString());
    break;
  }
  case Snap::Kind::Midpoint:
    addConstraint({{QStringLiteral("type"), QStringLiteral("midpoint")},
                   {QStringLiteral("point"), point},
                   {QStringLiteral("curve"), snap.curve}});
    break;
  case Snap::Kind::OnCurve:
    coincident(snap.curve);
    break;
  case Snap::Kind::Intersection:
    coincident(snap.curve);
    coincident(snap.curve2);
    break;
  default:
    break;
  }
}

void SketchOp::addConstraint(const QJsonObject& constraint) {
  QJsonObject cmd = command(QStringLiteral("sketch.add_constraint"));
  cmd.insert(QStringLiteral("constraint"), constraint);
  attempt(cmd);
}

void SketchOp::addDimension(const QJsonObject& dimension, const QString& value) {
  QJsonObject cmd = command(QStringLiteral("sketch.add_dimension"));
  cmd.insert(QStringLiteral("dimension"), dimension);
  cmd.insert(QStringLiteral("value"), value);
  attempt(cmd);
}

const SketchModel& SketchOp::model() {
  if (m_stale) {
    m_c.reload();
    m_stale = false;
  }
  return m_c.model();
}

bool SketchOp::commit() {
  m_finished = true;
  const int depth = m_c.undoDepth();
  if (depth > m_depth + 1) {
    try {
      m_c.runModel({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                    {QStringLiteral("depth"), m_depth},
                    {QStringLiteral("label"), m_label}});
    } catch (const std::exception& e) {
      qWarning().noquote() << "Could not merge the undo steps:" << errorText(e);
    }
  }
  m_c.host().refreshScene();
  return depth > m_depth;
}

void SketchOp::rollback() {
  m_finished = true;
  for (int guard = 0; guard < 64 && m_c.undoDepth() > m_depth; ++guard) {
    try {
      m_c.runModel({{QStringLiteral("cmd"), QStringLiteral("undo")}});
    } catch (const std::exception&) {
      break;
    }
  }
  m_c.host().refreshScene();
}

// ---------------------------------------------------------------------------
// SketchController

SketchController::SketchController(SketchHost& host, OcctViewer& viewer, QObject* parent)
    : QObject(parent), m_host(host), m_viewer(viewer) {
  m_overlay = new SketchOverlay(*this, &viewer);
  m_overlay->hide();
  viewer.installEventFilter(this);
  // After the frame, which may have moved the camera.
  connect(
      &viewer, &OcctViewer::viewChanged, m_overlay,
      [this] {
        if (!isActive()) {
          return;
        }
        m_overlay->update();
        placeFields();
        logView();
      },
      Qt::QueuedConnection);
  m_dragTimer.setSingleShot(true);
  connect(&m_dragTimer, &QTimer::timeout, this, &SketchController::applyDrag);
  TestSync::watch(&m_dragTimer);
}

SketchController::~SketchController() = default;

void SketchController::enter(const QString& uid) {
  m_uid = uid;
  m_status.clear();
  m_loggedView.clear();
  reload();
  m_viewer.setSketchPlane(toAx3(m_model.frame));
  m_overlay->resize(m_viewer.size());
  m_overlay->show();
  m_overlay->raise();
}

void SketchController::leave() {
  stopTool();
  closeEditor();
  m_pressed = false;
  m_dragging = false;
  m_dragTimer.stop();
  m_textOverride.reset();
  m_overlay->hide();
  m_uid.clear();
  m_model = SketchModel();
  m_hover.clear();
  m_picked.clear();
  m_hoverAnnotation.clear();
  emit changed();
}

void SketchController::reload() {
  if (!isActive()) {
    return;
  }
  try {
    m_model.load(m_host.queryObject(
        {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), m_uid}}));
    const QJsonObject units = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
                                  .value(QStringLiteral("units"))
                                  .toObject();
    m_unit = units.value(QStringLiteral("length")).toString(QStringLiteral("mm"));
    m_unitScale = unitScale(m_unit);
  } catch (const std::exception&) {
    // The sketch was just taken back; the main window leaves sketch mode.
  }
  const QString status = describeStatus();
  if (status != m_status) {
    m_status = status;
    qDebug().noquote() << status;
  }
  m_overlay->update();
  emit changed();
}

QString SketchController::describeStatus() const {
  QString text = QStringLiteral("Sketch %1: %2 DOF").arg(m_model.name).arg(m_model.dof);
  if (m_model.fullyConstrained()) {
    text += QStringLiteral(", fully constrained");
  }
  if (!m_model.conflicts.empty()) {
    QStringList sets;
    for (const QStringList& set : m_model.conflicts) {
      sets << set.join(QLatin1Char('+'));
    }
    text += QStringLiteral(", conflicts %1").arg(sets.join(QStringLiteral(", ")));
  }
  return text;
}

void SketchController::lookAt() { m_viewer.lookAtSketchPlane(false); }

void SketchController::logView() {
  // Where sketch points are in the window, for UI tests that click at
  // sketch coordinates.
  QStringList places;
  for (const V2& p : {V2{0.0, 0.0}, V2{100.0, 0.0}, V2{0.0, 100.0}}) {
    const QPointF at = toScreen(p);
    const QPoint window = m_viewer.mapTo(m_viewer.window(), QPoint(0, 0));
    places << QStringLiteral("%1,%2").arg(at.x() + window.x(), 0, 'f', 2).arg(at.y() + window.y(), 0, 'f', 2);
  }
  const QString line = QStringLiteral("Sketch view %1").arg(places.join(QLatin1Char(' ')));
  if (line != m_loggedView) {
    m_loggedView = line;
    qDebug().noquote() << line;
  }
}

// ---------------------------------------------------------------------------
// Tools

void SketchController::startTool(std::unique_ptr<SketchTool> tool) {
  stopTool();
  closeEditor();
  m_tool = std::move(tool);
  m_viewer.setSketchInput(true, m_toolIcons.value(m_tool->name()));
  m_viewer.setPickFilter(SelectFilter());
  qDebug().noquote() << QStringLiteral("Sketch tool %1").arg(m_tool->name());
  emit toolChanged(m_tool->name());
  m_tool->start();
  if (m_cursorValid) {
    updateCursor(m_cursor);
    m_tool->move(m_snap);
  }
  m_overlay->update();
}

void SketchController::stopTool() {
  if (!m_tool) {
    return;
  }
  // A tool may end itself from one of its own handlers: delete it later.
  SketchTool* retired = m_tool.release();
  QTimer::singleShot(0, this, [retired] { delete retired; });
  clearFields();
  m_hover.clear();
  m_picked.clear();
  m_snap = Snap();
  m_viewer.setSketchInput(false);
  emit toolChanged(QString());
  m_overlay->update();
}

bool SketchController::confirm() {
  if (!isActive()) {
    return false;
  }
  if (m_editor) {
    acceptEditor();
    return true;
  }
  if (m_tool) {
    if (!m_tool->confirm()) {
      stopTool();
      hint(tr("Sketch: choose a tool, or Finish Sketch (Ctrl+Enter)."));
    }
    m_overlay->update();
    return true;
  }
  return false;
}

bool SketchController::cancel() {
  if (!isActive()) {
    return false;
  }
  if (m_editor) {
    closeEditor();
    if (m_tool) {
      m_tool->cancel();
    }
    m_overlay->update();
    return true;
  }
  if (m_dragging) {
    return true;
  }
  if (m_tool) {
    if (!m_tool->cancel()) {
      stopTool();
      hint(tr("Sketch: choose a tool, or Finish Sketch (Ctrl+Enter)."));
    }
    m_overlay->update();
    return true;
  }
  if (!m_host.selection().isEmpty()) {
    m_host.setSelection({});
    return true;
  }
  return false;
}

void SketchController::resetTool() {
  closeEditor();
  if (m_tool) {
    for (int guard = 0; guard < 16 && m_tool->cancel(); ++guard) {
    }
  }
  m_overlay->update();
}

void SketchController::deleteSelection() {
  QJsonArray items;
  QJsonArray unfix;
  for (const SelectionItem& item : m_host.selection()) {
    if (item.owner != m_uid) {
      continue;
    }
    switch (item.kind) {
    case SelectKind::SketchCurve:
    case SelectKind::SketchPoint:
    case SelectKind::SketchDimension:
      items.append(item.name);
      break;
    case SelectKind::SketchConstraint:
      if (item.name.startsWith(QStringLiteral("fix:"))) {
        unfix.append(item.name.mid(4));
      } else {
        items.append(item.name);
      }
      break;
    default:
      break;
    }
  }
  if (items.isEmpty() && unfix.isEmpty()) {
    hint(tr("Select sketch geometry, constraints or dimensions to delete."));
    return;
  }
  SketchOp op(*this);
  bool ok = true;
  if (!unfix.isEmpty()) {
    QJsonObject cmd = op.command(QStringLiteral("sketch.set_fixed"));
    cmd.insert(QStringLiteral("entities"), unfix);
    cmd.insert(QStringLiteral("fixed"), false);
    ok = op.run(cmd);
  }
  if (ok && !items.isEmpty()) {
    QJsonObject cmd = op.command(QStringLiteral("sketch.remove"));
    cmd.insert(QStringLiteral("items"), items);
    ok = op.run(cmd);
  }
  if (!ok) {
    const QString reason = op.error();
    op.rollback();
    error(tr("Could not delete: %1").arg(reason));
    return;
  }
  m_host.setSelection({});
  op.commit();
  QStringList names;
  for (const QJsonValue& v : items) {
    names << v.toString();
  }
  for (const QJsonValue& v : unfix) {
    names << QStringLiteral("fix:") + v.toString();
  }
  qDebug().noquote() << QStringLiteral("Deleted %1").arg(names.join(QStringLiteral(", ")));
}

void SketchController::toggleConstruction() {
  QJsonArray curves;
  bool allConstruction = true;
  for (const SelectionItem& item : m_host.selection()) {
    if (item.owner == m_uid && item.kind == SelectKind::SketchCurve) {
      curves.append(item.name);
      const CurveData* c = m_model.curve(item.name);
      allConstruction = allConstruction && c != nullptr && c->construction;
    }
  }
  if (curves.isEmpty() || m_tool) {
    setConstruction(!m_construction);
    hint(m_construction ? tr("New geometry is construction geometry (X).")
                        : tr("New geometry is normal geometry (X)."));
    return;
  }
  SketchOp op(*this);
  QJsonObject cmd = op.command(QStringLiteral("sketch.set_construction"));
  cmd.insert(QStringLiteral("curves"), curves);
  cmd.insert(QStringLiteral("construction"), !allConstruction);
  if (!op.run(cmd)) {
    const QString reason = op.error();
    op.rollback();
    error(reason);
    return;
  }
  op.commit();
  qDebug().noquote() << QStringLiteral("Construction %1: %2")
                            .arg(allConstruction ? QStringLiteral("off") : QStringLiteral("on"),
                                 QJsonDocument(curves).toJson(QJsonDocument::Compact));
}

void SketchController::toggleCenterline() {
  QJsonArray lines;
  bool all = true;
  for (const SelectionItem& item : m_host.selection()) {
    const CurveData* c = m_model.curve(item.name);
    if (item.owner == m_uid && item.kind == SelectKind::SketchCurve && c != nullptr && c->isLine()) {
      lines.append(item.name);
      all = all && c->centerline;
    }
  }
  if (lines.isEmpty()) {
    hint(tr("Centerline: select sketch lines first."));
    return;
  }
  SketchOp op(*this);
  if (!all) {
    // A centre line is not construction geometry.
    QJsonArray construction;
    for (const QJsonValue& line : lines) {
      const CurveData* c = m_model.curve(line.toString());
      if (c != nullptr && c->construction) {
        construction.append(line);
      }
    }
    if (!construction.isEmpty()) {
      QJsonObject cmd = op.command(QStringLiteral("sketch.set_construction"));
      cmd.insert(QStringLiteral("curves"), construction);
      cmd.insert(QStringLiteral("construction"), false);
      op.run(cmd);
    }
  }
  QJsonObject cmd = op.command(QStringLiteral("sketch.set_centerline"));
  cmd.insert(QStringLiteral("lines"), lines);
  cmd.insert(QStringLiteral("centerline"), !all);
  if (!op.run(cmd)) {
    const QString reason = op.error();
    op.rollback();
    error(reason);
    return;
  }
  op.commit();
  qDebug().noquote() << QStringLiteral("Centerline %1: %2")
                            .arg(all ? QStringLiteral("off") : QStringLiteral("on"),
                                 QJsonDocument(lines).toJson(QJsonDocument::Compact));
}

void SketchController::toggleFixed() {
  QJsonArray entities;
  bool all = true;
  for (const SelectionItem& item : m_host.selection()) {
    if (item.owner != m_uid) {
      continue;
    }
    if (item.kind == SelectKind::SketchPoint) {
      const PointData* p = m_model.point(item.name);
      entities.append(item.name);
      all = all && p != nullptr && p->fixed;
    } else if (item.kind == SelectKind::SketchCurve) {
      const CurveData* c = m_model.curve(item.name);
      entities.append(item.name);
      all = all && c != nullptr && c->fixed;
    } else if (item.kind == SelectKind::SketchConstraint && item.name.startsWith(QStringLiteral("fix:"))) {
      entities.append(item.name.mid(4));
    }
  }
  if (entities.isEmpty()) {
    return;
  }
  SketchOp op(*this);
  QJsonObject cmd = op.command(QStringLiteral("sketch.set_fixed"));
  cmd.insert(QStringLiteral("entities"), entities);
  cmd.insert(QStringLiteral("fixed"), !all);
  if (!op.run(cmd)) {
    const QString reason = op.error();
    op.rollback();
    error(reason);
    return;
  }
  op.commit();
  qDebug().noquote() << QStringLiteral("%1 %2")
                            .arg(all ? QStringLiteral("Unfixed") : QStringLiteral("Fixed"),
                                 QJsonDocument(entities).toJson(QJsonDocument::Compact));
}

void SketchController::editDimension(const QString& id) {
  const DimensionData* d = m_model.dimension(id);
  if (d == nullptr) {
    return;
  }
  const QRectF rect = m_overlay->dimensionTextRect(id);
  const FieldSpec::Kind kind =
      d->type == QStringLiteral("angle") ? FieldSpec::Kind::Angle : FieldSpec::Kind::Length;
  QString text = d->expression;
  if (text.isEmpty() || d->driven) {
    text = kind == FieldSpec::Kind::Angle ? QString::number(d->measured * 180.0 / kPi, 'g', 10)
                                          : QString::number(d->measured / m_unitScale, 'g', 10);
  }
  qDebug().noquote() << QStringLiteral("Editing dimension %1 (%2)").arg(id, text);
  openValueEditor(rect.center(), text, kind, [this, id](const QString& expression) {
    SketchOp op(*this);
    QJsonObject cmd = op.command(QStringLiteral("sketch.set_dimension"));
    cmd.insert(QStringLiteral("dimension"), id);
    cmd.insert(QStringLiteral("value"), expression);
    if (!op.run(cmd)) {
      const QString reason = op.error();
      op.rollback();
      error(reason);
      return;
    }
    op.commit();
    const DimensionData* changed = m_model.dimension(id);
    qDebug().noquote() << QStringLiteral("Dimension %1 = %2 (%3)")
                              .arg(id, expression, changed != nullptr ? changed->parameter : QString());
  });
}

// ---------------------------------------------------------------------------
// Options

void SketchController::setConstruction(bool on) {
  if (on != m_construction) {
    m_construction = on;
    qDebug().noquote() << QStringLiteral("Construction mode %1").arg(on ? QStringLiteral("on")
                                                                         : QStringLiteral("off"));
    emit optionsChanged();
    m_overlay->update();
  }
}

void SketchController::setGridSnap(bool on) {
  if (on == m_gridSnap) {
    return;
  }
  m_gridSnap = on;
  emit optionsChanged();
}

void SketchController::setShowDimensions(bool on) {
  m_showDimensions = on;
  emit optionsChanged();
  m_overlay->update();
}

void SketchController::setShowConstraints(bool on) {
  m_showConstraints = on;
  emit optionsChanged();
  m_overlay->update();
}

void SketchController::setShowProfiles(bool on) {
  m_showProfiles = on;
  emit optionsChanged();
  m_host.refreshScene();
}

void SketchController::setHideAbove(bool on) {
  if (on == m_hideAbove) {
    return;
  }
  m_hideAbove = on;
  qDebug().noquote() << QStringLiteral("Hide Above Sketch %1").arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  emit optionsChanged();
  m_host.refreshScene();
}

// ---------------------------------------------------------------------------
// View

QPointF SketchController::toScreen(const V2& p) const {
  return m_viewer.toWidgetF(toModel(m_model.frame, p));
}

std::optional<V2> SketchController::sketchAt(const QPointF& screen) const {
  if (const auto p = m_viewer.sketchPlanePoint(screen)) {
    return V2{p->X(), p->Y()};
  }
  return std::nullopt;
}

double SketchController::pixel() const {
  const QPointF o = toScreen(V2{});
  const double sx = screenDistance(toScreen(V2{1.0, 0.0}), o);
  const double sy = screenDistance(toScreen(V2{0.0, 1.0}), o);
  const double perMm = std::max(sx, sy);
  return perMm > 1e-9 ? 1.0 / perMm : 1.0;
}

QString SketchController::entityAt(const QPointF& screen, unsigned filter) const {
  QString best;
  double bestDistance = std::numeric_limits<double>::max();
  if (filter & PickPoints) {
    for (const PointData& p : m_model.points) {
      const double d = screenDistance(toScreen(p.at), screen);
      if (d <= kPointPixels && d < bestDistance) {
        best = p.id;
        bestDistance = d;
      }
    }
    if (!best.isEmpty()) {
      return best;
    }
  }
  if (filter & PickCurves) {
    const auto raw = sketchAt(screen);
    if (!raw) {
      return best;
    }
    const double px = pixel();
    for (const CurveData& c : m_model.curves) {
      const double d = c.curve.distanceTo(*raw) / px;
      if (d <= kCurvePixels && d < bestDistance) {
        best = c.id;
        bestDistance = d;
      }
    }
  }
  return best;
}

void SketchController::setHover(const QString& entity) {
  if (entity != m_hover) {
    m_hover = entity;
    m_overlay->update();
  }
}

void SketchController::setPicked(const QStringList& entities) {
  m_picked = entities;
  m_overlay->update();
}

void SketchController::refreshView() { m_overlay->update(); }

void SketchController::hint(const QString& message) { m_host.showHint(message); }

void SketchController::error(const QString& message) { m_host.showError(message); }

void SketchController::updateCursor(const QPointF& position) {
  m_cursor = position;
  const auto raw = sketchAt(position);
  m_cursorValid = raw.has_value();
  if (!raw) {
    return;
  }
  if (m_tool && m_tool->snaps()) {
    SnapSettings settings;
    settings.model = &m_model;
    settings.toScreen = [this](const V2& p) { return toScreen(p); };
    settings.grid = m_gridSnap;
    settings.gridStep = m_gridStep > 0.0 ? m_gridStep : m_viewer.gridStep(); // the lines shown
    m_snap = snap(settings, *raw, position);
  } else {
    m_snap = Snap();
    m_snap.at = *raw;
    m_snap.screen = position;
  }
  // Snapped to a point (not onto a curve or the grid): the cursor shows it.
  m_viewer.setSketchSnapping(m_tool && m_snap.onGeometry() && m_snap.kind != Snap::Kind::OnCurve);
}

// ---------------------------------------------------------------------------
// Mouse

bool SketchController::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_viewer) {
    return viewerEvent(event);
  }
  if (auto* edit = qobject_cast<QLineEdit*>(watched)) {
    return fieldEvent(edit, event);
  }
  return QObject::eventFilter(watched, event);
}

bool SketchController::viewerEvent(QEvent* event) {
  if (event->type() == QEvent::Resize) {
    m_overlay->resize(static_cast<QResizeEvent*>(event)->size());
    return false;
  }
  if (event->type() == QEvent::WindowBlocked) {
    interruptDrag();
    return false;
  }
  if (!isActive() || m_forwarding || m_paused) {
    return false;
  }
  // The orientation cube is the view's: clicks turn the view, drags orbit (U5).
  if (event->type() == QEvent::MouseButtonPress &&
      static_cast<QMouseEvent*>(event)->button() == Qt::LeftButton &&
      m_viewer.isOnOrientationCube(static_cast<QMouseEvent*>(event)->position())) {
    return false;
  }
  if ((event->type() == QEvent::MouseMove || event->type() == QEvent::MouseButtonRelease) &&
      m_viewer.cubeInteraction()) {
    return false;
  }
  switch (event->type()) {
  case QEvent::Leave:
    m_cursorValid = false;
    if (!m_hoverAnnotation.isEmpty()) {
      m_hoverAnnotation.clear();
    }
    m_overlay->update();
    return false;
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    updateCursor(e->position());
    if (m_pressed && !m_dragging && screenDistance(e->position(), m_pressPosition) > kDragPixels) {
      startDrag();
    }
    if (m_dragging) {
      if (m_cursorValid) {
        m_dragTarget = m_snap.at;
        m_dragPending = true;
        if (!m_dragTimer.isActive()) {
          m_dragTimer.start(0);
        }
      }
      return true;
    }
    if (m_tool) {
      m_tool->move(m_snap);
      placeFields();
      m_overlay->update();
      return (e->buttons() & Qt::LeftButton) != 0;
    }
    const SketchOverlay::Hit hit = m_overlay->hitTest(e->position());
    if (hit.id != m_hoverAnnotation) {
      m_hoverAnnotation = hit.id;
      m_overlay->update();
    }
    return m_pressed;
  }
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (m_editor) {
      acceptEditor();
      return true;
    }
    updateCursor(e->position());
    if (m_tool) {
      if (m_cursorValid) {
        m_tool->press(m_snap);
      }
      placeFields();
      m_overlay->update();
      return true;
    }
    const SketchOverlay::Hit hit = m_overlay->hitTest(e->position());
    if (hit.valid()) {
      if (hit.kind == SketchOverlay::Hit::Kind::Dimension) {
        selectAnnotation(QStringLiteral("dimension"), hit.id, e->modifiers());
        m_pressed = true;
        m_dragDimension = hit.id;
        m_pressPosition = e->position();
      } else {
        selectAnnotation(QStringLiteral("constraint"),
                         hit.kind == SketchOverlay::Hit::Kind::Fixed ? QStringLiteral("fix:") + hit.id
                                                                       : hit.id,
                         e->modifiers());
      }
      return true;
    }
    const QString entity = entityAt(e->position());
    if (!entity.isEmpty() && !(e->modifiers() & (Qt::ControlModifier | Qt::ShiftModifier))) {
      m_pressed = true;
      m_dragEntity = entity;
      m_pressPosition = e->position();
      m_pressModifiers = e->modifiers();
      return true;
    }
    return false;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (m_tool) {
      updateCursor(e->position());
      if (m_cursorValid) {
        m_tool->release(m_snap);
      }
      m_overlay->update();
      return true;
    }
    if (m_dragging) {
      endDrag();
      return true;
    }
    if (m_pressed) {
      m_pressed = false;
      if (!m_dragEntity.isEmpty()) {
        forwardClick(e);
      }
      m_dragEntity.clear();
      m_dragDimension.clear();
      return true;
    }
    return false;
  }
  case QEvent::MouseButtonDblClick: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (m_tool) {
      updateCursor(e->position());
      m_tool->doubleClick(m_snap);
      m_overlay->update();
      return true;
    }
    const SketchOverlay::Hit hit = m_overlay->hitTest(e->position());
    if (hit.kind == SketchOverlay::Hit::Kind::Dimension) {
      m_pressed = false;
      m_dragDimension.clear();
      editDimension(hit.id);
      return true;
    }
    // A text, a pattern's curve or an offset's curve the first click picked
    // opens its edit panel (P3, P4).
    const Selection& selected = m_host.selection();
    if (selected.size() == 1 && selected.first().owner == m_uid &&
        (selected.first().kind == SelectKind::SketchCurve || selected.first().kind == SelectKind::SketchPoint)) {
      const QString id = selected.first().name;
      const char* command = m_model.text(id) != nullptr       ? "sketch.edit_text"
                            : m_model.offsetOf(id) != nullptr ? "sketch.edit_offset"
                            : m_model.patternOf(id) != nullptr ? "sketch.edit_pattern"
                                                               : nullptr;
      if (command != nullptr) {
        m_pressed = false;
        m_dragEntity.clear();
        qDebug().noquote() << QStringLiteral("Double-click on %1: %2").arg(id, QString::fromLatin1(command));
        m_host.trigger(QString::fromLatin1(command));
        return true;
      }
    }
    return false;
  }
  default:
    return false;
  }
}

void SketchController::forwardClick(const QMouseEvent* release) {
  // A press on geometry that did not become a drag is a click: the view
  // picks as usual.
  QMouseEvent press(QEvent::MouseButtonPress, m_pressPosition, m_viewer.mapToGlobal(m_pressPosition),
                    Qt::LeftButton, Qt::LeftButton, m_pressModifiers);
  QMouseEvent up(QEvent::MouseButtonRelease, release->position(), release->globalPosition(),
                 Qt::LeftButton, Qt::NoButton, release->modifiers());
  m_forwarding = true;
  QCoreApplication::sendEvent(&m_viewer, &press);
  QCoreApplication::sendEvent(&m_viewer, &up);
  m_forwarding = false;
}

void SketchController::selectAnnotation(const QString& kind, const QString& id,
                                        Qt::KeyboardModifiers modifiers) {
  SelectionItem item;
  item.kind = kind == QStringLiteral("dimension") ? SelectKind::SketchDimension
                                                  : SelectKind::SketchConstraint;
  item.owner = m_uid;
  item.name = id;
  m_host.pick({item}, modifiers);
  m_overlay->update();
}

// ---------------------------------------------------------------------------
// Drag (the solver's drag mode)

void SketchController::startDrag() {
  m_dragging = true;
  m_dragDepth = undoDepth();
  m_dragSteps = 0;
  m_dragStart = sketchAt(m_pressPosition).value_or(V2{});
  m_dragLast = m_dragStart;
  m_dragTarget = m_dragStart;
  if (!m_dragDimension.isEmpty()) {
    m_textOverride = std::make_pair(m_dragDimension, m_dragStart);
  }
  hint(tr("Drag: the geometry follows as far as its constraints let it."));
}

void SketchController::applyDrag() {
  if (!m_dragging || !m_dragPending) {
    return;
  }
  if (m_host.modelBusy()) {
    // A job computes the model (this timer ran out meanwhile): after it.
    m_dragTimer.start(10);
    return;
  }
  m_dragPending = false;
  const V2 target = m_dragTarget;
  if (!m_dragDimension.isEmpty()) {
    m_textOverride = std::make_pair(m_dragDimension, target);
    m_overlay->update();
    return;
  }
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch.drag")},
                  {QStringLiteral("sketch"), m_uid},
                  {QStringLiteral("entity"), m_dragEntity}};
  if (m_model.point(m_dragEntity) != nullptr) {
    cmd.insert(QStringLiteral("to"), xy(target));
  } else if (const CurveData* c = m_model.curve(m_dragEntity)) {
    if (c->isCircle()) {
      cmd.insert(QStringLiteral("radius"), std::max(dist(target, c->curve.center), 1e-3));
    } else {
      // The point of the curve that was grabbed follows the cursor.
      const V2 grabbed = c->curve.nearest(m_dragLast);
      cmd.insert(QStringLiteral("by"), xy(target - grabbed));
    }
  } else {
    return;
  }
  try {
    runModel(cmd);
    ++m_dragSteps;
    m_dragLast = target;
  } catch (const std::exception&) {
    // Fixed geometry does not move.
  }
  reload();
  m_host.refreshScene();
}

void SketchController::interruptDrag() {
  if (!m_pressed && !m_dragging) {
    return;
  }
  m_pressed = false;
  if (!m_dragging) {
    m_dragEntity.clear();
    m_dragDimension.clear();
    return;
  }
  // The steps so far stay; the one waiting is dropped. The drag's last
  // step may be the job now running: what ends it waits for that.
  m_dragging = false;
  m_dragPending = false;
  m_dragTimer.stop();
  qDebug().noquote() << QStringLiteral("Drag of %1 ended by a dialog").arg(
      m_dragEntity.isEmpty() ? m_dragDimension : m_dragEntity);
  m_host.whenIdle(this, [this] {
    if (isActive()) {
      endDrag();
    }
  });
}

void SketchController::endDrag() {
  m_dragTimer.stop();
  applyDrag();
  m_dragging = false;
  m_pressed = false;
  if (!m_dragDimension.isEmpty()) {
    const QString id = m_dragDimension;
    const V2 at = m_textOverride ? m_textOverride->second : m_dragTarget;
    m_dragDimension.clear();
    m_textOverride.reset();
    SketchOp op(*this);
    QJsonObject cmd = op.command(QStringLiteral("sketch.set_dimension_text"));
    cmd.insert(QStringLiteral("dimension"), id);
    cmd.insert(QStringLiteral("text"), xy(at));
    if (op.run(cmd)) {
      op.commit();
      qDebug().noquote() << QStringLiteral("Moved dimension %1 to (%2, %3)").arg(id).arg(at.x).arg(at.y);
    } else {
      op.rollback();
    }
    return;
  }
  const QString entity = m_dragEntity;
  m_dragEntity.clear();
  if (m_dragSteps > 1) {
    try {
      runModel({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                {QStringLiteral("depth"), m_dragDepth},
                {QStringLiteral("label"), QStringLiteral("Drag in %1").arg(m_model.name)}});
    } catch (const std::exception&) {
      // Each step stays its own.
    }
  }
  if (m_dragSteps == 0) {
    hint(tr("Fully constrained or fixed geometry does not move."));
    qDebug().noquote() << QStringLiteral("Drag of %1 did not move it").arg(entity);
    return;
  }
  // Where the dragged point is, or the curve's point nearest the cursor.
  V2 at = m_dragLast;
  if (const auto p = m_model.pointAt(entity)) {
    at = *p;
  } else if (const CurveData* c = m_model.curve(entity)) {
    at = c->curve.nearest(m_dragLast);
  }
  qDebug().noquote() << QStringLiteral("Dragged %1 to (%2, %3)")
                            .arg(entity)
                            .arg(at.x, 0, 'f', 3)
                            .arg(at.y, 0, 'f', 3);
  m_host.refreshScene();
}

// ---------------------------------------------------------------------------
// Typed values

int SketchController::fieldIndex(const QString& id) const {
  for (int i = 0; i < m_fields.size(); ++i) {
    if (m_fields[i].spec.id == id) {
      return i;
    }
  }
  return -1;
}

void SketchController::setFields(const QVector<FieldSpec>& fields) {
  clearFields();
  for (const FieldSpec& spec : fields) {
    Field field;
    field.spec = spec;
    auto* edit = new QLineEdit(&m_viewer);
    edit->setObjectName(QStringLiteral("sketchField_") + spec.id);
    edit->setStyleSheet(kFieldStyle);
    edit->setFixedWidth(spec.kind == FieldSpec::Kind::Text ? 140 : 84);
    edit->setToolTip(tr("%1: type a value or an expression; Tab moves on, Enter accepts")
                         .arg(spec.label));
    edit->setPlaceholderText(spec.label);
    edit->installEventFilter(this);
    const int index = static_cast<int>(m_fields.size());
    connect(edit, &QLineEdit::textEdited, this, [this, index] { fieldEdited(index); });
    field.edit = edit;
    m_fields.append(field);
  }
  placeFields();
  for (const Field& field : m_fields) {
    field.edit->show();
  }
  if (!m_fields.isEmpty()) {
    m_fields.first().edit->setFocus();
    m_fields.first().edit->selectAll();
  }
}

void SketchController::clearFields() {
  const bool hadFocus =
      std::any_of(m_fields.begin(), m_fields.end(), [](const Field& f) { return f.edit && f.edit->hasFocus(); });
  for (Field& field : m_fields) {
    if (field.edit) {
      field.edit->hide();
      field.edit->deleteLater();
    }
  }
  m_fields.clear();
  if (hadFocus) {
    m_viewer.setFocus();
  }
}

void SketchController::placeFields() {
  if (m_fields.isEmpty()) {
    return;
  }
  QPointF at = m_cursor + QPointF(20.0, 16.0);
  for (Field& field : m_fields) {
    if (!field.edit) {
      continue;
    }
    const QSize size = field.edit->sizeHint();
    const int x = std::clamp(static_cast<int>(at.x()), 0, std::max(0, m_viewer.width() - field.edit->width()));
    const int y = std::clamp(static_cast<int>(at.y()), 0, std::max(0, m_viewer.height() - size.height()));
    field.edit->move(x, y);
    at.ry() += size.height() + 3.0;
  }
}

void SketchController::fieldEdited(int index) {
  if (index < 0 || index >= m_fields.size()) {
    return;
  }
  Field& field = m_fields[index];
  const QString text = field.edit ? field.edit->text().trimmed() : QString();
  field.typed = !text.isEmpty();
  field.valid = false;
  QString problem;
  if (field.typed) {
    field.valid = evaluate(text, field.spec.kind, field.expression, field.value, problem);
  }
  if (field.edit) {
    field.edit->setStyleSheet(field.typed && !field.valid ? kInvalidFieldStyle : kFieldStyle);
    field.edit->setToolTip(problem);
  }
  qDebug().noquote() << QStringLiteral("Sketch value %1: %2%3")
                            .arg(field.spec.id, text,
                                 field.valid ? QStringLiteral(" = %1").arg(field.expression)
                                 : field.typed ? QStringLiteral(" is invalid: %1").arg(problem)
                                               : QString());
  if (m_tool) {
    m_tool->fieldsEdited();
  }
  m_overlay->update();
}

bool SketchController::fieldEvent(QLineEdit* edit, QEvent* event) {
  if (event->type() != QEvent::KeyPress) {
    return false;
  }
  auto* key = static_cast<QKeyEvent*>(event);
  if (key->key() != Qt::Key_Tab && key->key() != Qt::Key_Backtab) {
    return false;
  }
  // Tab moves between the values.
  int index = -1;
  for (int i = 0; i < m_fields.size(); ++i) {
    if (m_fields[i].edit == edit) {
      index = i;
    }
  }
  if (index < 0 || m_fields.isEmpty()) {
    return false;
  }
  const int step = key->key() == Qt::Key_Backtab ? -1 : 1;
  const int next = (index + step + static_cast<int>(m_fields.size())) % static_cast<int>(m_fields.size());
  if (QLineEdit* target = m_fields[next].edit) {
    target->setFocus();
    target->selectAll();
  }
  return true;
}

bool SketchController::typed(const QString& id) const {
  const int i = fieldIndex(id);
  return i >= 0 && m_fields[i].typed && m_fields[i].valid;
}

std::optional<double> SketchController::fieldValue(const QString& id) const {
  const int i = fieldIndex(id);
  if (i < 0 || !m_fields[i].typed || !m_fields[i].valid) {
    return std::nullopt;
  }
  return m_fields[i].value;
}

QString SketchController::fieldExpression(const QString& id) const {
  const int i = fieldIndex(id);
  return i >= 0 && m_fields[i].valid ? m_fields[i].expression : QString();
}

QString SketchController::fieldText(const QString& id) const {
  const int i = fieldIndex(id);
  return i >= 0 && m_fields[i].edit ? m_fields[i].edit->text() : QString();
}

void SketchController::setLive(const QString& id, double value) {
  const int i = fieldIndex(id);
  if (i < 0 || m_fields[i].typed || !m_fields[i].edit) {
    return;
  }
  QString text;
  switch (m_fields[i].spec.kind) {
  case FieldSpec::Kind::Angle:
    text = QString::number(value * 180.0 / kPi, 'f', 1);
    break;
  case FieldSpec::Kind::Count:
    text = QString::number(static_cast<int>(std::lround(value)));
    break;
  case FieldSpec::Kind::Length:
    text = QString::number(value / m_unitScale, 'f', 2);
    break;
  case FieldSpec::Kind::Text:
    return;
  }
  QLineEdit* edit = m_fields[i].edit;
  if (edit->text() != text) {
    edit->setText(text);
    if (edit->hasFocus()) {
      edit->selectAll();
    }
  }
}

void SketchController::openValueEditor(const QPointF& at, const QString& text, FieldSpec::Kind kind,
                                       std::function<void(const QString&)> accept) {
  closeEditor();
  auto* edit = new QLineEdit(&m_viewer);
  edit->setObjectName(QStringLiteral("sketchValueEditor"));
  edit->setStyleSheet(kFieldStyle);
  edit->setText(text);
  edit->setFixedWidth(110);
  edit->setToolTip(tr("A value or an expression, such as 20, 2 in or d1 * 2; Enter accepts"));
  const QSize size = edit->sizeHint();
  edit->move(static_cast<int>(at.x() - 55.0), static_cast<int>(at.y() - size.height() / 2.0));
  edit->show();
  edit->raise();
  edit->setFocus();
  edit->selectAll();
  m_editor = edit;
  m_editorKind = kind;
  m_editorAccept = std::move(accept);
  const QPoint corner = m_viewer.mapTo(m_viewer.window(), edit->geometry().center());
  qDebug().noquote() << QStringLiteral("Sketch value editor at %1,%2 with %3")
                            .arg(corner.x())
                            .arg(corner.y())
                            .arg(text);
}

void SketchController::acceptEditor() {
  if (!m_editor) {
    return;
  }
  const QString text = m_editor->text().trimmed();
  QString expression;
  double value = 0.0;
  QString problem;
  if (!evaluate(text, m_editorKind, expression, value, problem)) {
    m_editor->setStyleSheet(kInvalidFieldStyle);
    error(tr("%1: %2").arg(text, problem));
    return;
  }
  auto accept = std::move(m_editorAccept);
  closeEditor();
  if (accept) {
    accept(expression);
  }
}

void SketchController::closeEditor() {
  if (m_editor) {
    m_editor->hide();
    m_editor->deleteLater();
    m_editor = nullptr;
    m_viewer.setFocus();
  }
  m_editorAccept = nullptr;
}

bool SketchController::evaluate(const QString& text, FieldSpec::Kind kind, QString& expression,
                                double& value, QString& problem) const {
  if (kind == FieldSpec::Kind::Text) {
    expression = text;
    return !text.isEmpty();
  }
  if (text.trimmed().isEmpty()) {
    problem = tr("Enter a value.");
    return false;
  }
  try {
    const QJsonObject answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("evaluate")},
                                                   {QStringLiteral("expression"), text},
                                                   {QStringLiteral("kind"), kindKey(kind)}});
    value = answer.value(QStringLiteral("value")).toDouble();
    const QString shown = answer.value(QStringLiteral("text")).toString();
    // With decimal points where commas were typed (mitcad#2).
    const QString typed = answer.value(QStringLiteral("expression")).toString(text);
    const bool isNumber = parsePlainNumber(text).has_value();
    const int space = static_cast<int>(shown.lastIndexOf(QLatin1Char(' ')));
    expression = isNumber && space > 0 && kind != FieldSpec::Kind::Count ? typed + shown.mid(space) : typed;
    return true;
  } catch (const std::exception& e) {
    problem = errorText(e);
    return false;
  }
}

QString SketchController::formatLength(double mm) const {
  return QString::number(mm / m_unitScale, 'f', 2);
}

QString SketchController::formatAngle(double radians) const {
  return QString::number(radians * 180.0 / kPi, 'f', 1) + QStringLiteral("°");
}

// ---------------------------------------------------------------------------
// Model

QJsonObject SketchController::runModel(const QJsonObject& command) { return m_host.modelCommand(command); }

int SketchController::undoDepth() const {
  return m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
      .value(QStringLiteral("undo_depth"))
      .toInt();
}

QString SketchController::undoLabel() const {
  return m_host.queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
      .value(QStringLiteral("undo"))
      .toString();
}

} // namespace mitcad::sketch
