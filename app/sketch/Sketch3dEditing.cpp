// SPDX-License-Identifier: MIT
// Editing a 3D sketch in its editing context (mitcad#1526): relations and
// dimensions added from the selection, a dimension's value, driven state
// and removal, Fix and Unfix, Detach, Attach and Replace Reference by a
// source picked in the view, Delete naming what goes with a point still
// used, and dragging a point, a curve or a fit-point spline's tangent
// handle on a chosen plane or axis through the place grabbed: previewed
// by `sketch3d.drag` with `preview` (nothing changes), taken as one undo
// step on release, dropped by Esc. The commands and their wording are
// Sketch3dEdit.hpp's; Sketch3dController.hpp says how it fits together.

#include "Sketch3dController.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <optional>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QJsonDocument>
#include <QLineF>
#include <QMouseEvent>
#include <QRegularExpression>
#include <QTimer>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "../framework/UserText.hpp"
#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

constexpr double kGrabPixels = 8.0;      // a point or a handle's tip under the press
constexpr double kCurvePixels = 6.0;     // a curve under the press
constexpr double kDragClickPixels = 4.0; // a press moved less than this is a click
constexpr double kSourcePixels = 10.0;   // a source pick's reach, as the tools' snaps
constexpr double kShortMillimetres = 1e-3; // the reached place farther from the target: short

QString compactJson(const QJsonObject& object) {
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

QString xyzLog(const Xyz& at) {
  const auto n = [](double v) { return QString::number(std::abs(v) < 5e-7 ? 0.0 : v, 'f', 3); };
  return QStringLiteral("(%1, %2, %3)").arg(n(at[0]), n(at[1]), n(at[2]));
}

std::optional<Xyz> xyzValue(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() < 3) {
    return std::nullopt;
  }
  return Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()};
}

// The nearest place of a polyline to `pointer` on the screen, and how far.
template <typename Project>
std::pair<Xyz, double> nearestPlace(const std::vector<Xyz>& points, const QPointF& pointer, Project project) {
  std::pair<Xyz, double> best{Xyz{}, 1e300};
  for (std::size_t i = 1; i < points.size(); ++i) {
    const QPointF a = project(points[i - 1]);
    const QPointF b = project(points[i]);
    const QPointF along = b - a;
    const double squared = QPointF::dotProduct(along, along);
    double f = squared > 1e-12 ? QPointF::dotProduct(pointer - a, along) / squared : 0.0;
    f = std::clamp(f, 0.0, 1.0);
    const double pixels = QLineF(a + along * f, pointer).length();
    if (pixels < best.second) {
      best = {add(points[i - 1], scaled(sub(points[i], points[i - 1]), f)), pixels};
    }
  }
  return best;
}

bool isSourceKind(const SnapTarget& target, bool points, bool curves, bool faces) {
  if (target.source.isNull() || target.source.isUndefined()) {
    return false;
  }
  switch (target.kind) {
  case SnapKind::Origin:
  case SnapKind::LinkedPoint:
  case SnapKind::Midpoint:
    return points;
  case SnapKind::OnCurve:
  case SnapKind::OnEdge:
    return curves;
  case SnapKind::OnFace:
    return faces;
  default:
    return false;
  }
}

} // namespace

// ---------------------------------------------------------------------------
// Relations and dimensions

EditSelection Sketch3dController::editSelection() const {
  EditSelection out;
  if (!isActive()) {
    return out;
  }
  for (const SelectionItem& item : m_host.selection()) {
    if (item.owner != m_uid) {
      continue;
    }
    if (item.kind == SelectKind::SketchPoint && m_model.point(item.name) != nullptr) {
      out.points << item.name;
    } else if (item.kind == SelectKind::SketchCurve && m_model.curve(item.name) != nullptr) {
      out.curves << item.name;
    }
  }
  return out;
}

EditCommand Sketch3dController::relationFor(Relation3 relation) const {
  return relationCommand(m_model, m_uid, relation, editSelection());
}

EditCommand Sketch3dController::dimensionFor(Dimension3 dimension) const {
  return dimensionCommand(m_model, m_uid, dimension, editSelection());
}

void Sketch3dController::setEditNotice(const QString& notice) {
  if (notice == m_editNotice) {
    return;
  }
  m_editNotice = notice;
  emit changed();
}

bool Sketch3dController::runEdit(const QJsonArray& commands, const QString& label, const QString& log) {
  if (commands.isEmpty()) {
    return false;
  }
  const int depth = undoDepth();
  QStringList made;
  QStringList kept;
  for (const QJsonValue& next : commands) {
    toolLog(QStringLiteral("3D sketch command: %1").arg(compactJson(next.toObject())));
    QJsonObject result;
    if (!m_host.runCommand(next.toObject(), &result)) {
      // Nothing of the edit stays.
      for (int guard = 0; guard <= commands.size() && undoDepth() > depth; ++guard) {
        if (!m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}})) {
          break;
        }
      }
      toolLog(QStringLiteral("3D sketch edit %1: refused by the model; nothing kept").arg(log));
      m_host.refreshScene();
      return false;
    }
    for (const QJsonValue& id : result.value(QStringLiteral("made")).toArray()) {
      made << id.toString();
    }
    // A dimension's own parameter kept as a user parameter (mitcad#1535),
    // as a planar sketch's dimension reports it.
    for (const QJsonValue& value : result.value(QStringLiteral("user_parameters")).toArray()) {
      const QJsonObject k = value.toObject();
      // "Dimension k5: Reach kept as a user parameter, now d3", for UI tests.
      toolLog(QStringLiteral("Dimension %1: %2 kept as a user parameter, now %3")
                  .arg(k.value(QStringLiteral("dimension")).toString(), k.value(QStringLiteral("name")).toString(),
                       k.value(QStringLiteral("parameter")).toString()));
    }
    if (result.contains(QStringLiteral("user_parameters"))) {
      // In plain words (mitcad#1612).
      kept << shownTexts(result, QStringLiteral("notes"), QStringLiteral("note_messages"));
    }
  }
  if (undoDepth() > depth + 1) {
    m_host.runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                       {QStringLiteral("depth"), depth},
                       {QStringLiteral("label"), label}});
  }
  toolLog(QStringLiteral("3D sketch edit %1: done%2")
              .arg(log, made.isEmpty() ? QString() : QStringLiteral(", made %1").arg(made.join(QLatin1Char(' ')))));
  m_host.refreshScene();
  // The model's words: what moved and what the dimension is now.
  if (!kept.isEmpty()) {
    m_host.showStatus(kept.join(QStringLiteral("; ")), NoticeLevel::Information);
  }
  return true;
}

bool Sketch3dController::applyRelation(Relation3 relation) {
  if (!isActive()) {
    return false;
  }
  const EditCommand edit = relationFor(relation);
  if (!edit.valid()) {
    setEditNotice(edit.problem);
    m_host.showError(tr("%1: %2").arg(relationName(relation), edit.problem));
    toolLog(QStringLiteral("3D sketch edit %1 refused: %2").arg(relationKey(relation), edit.problem));
    return false;
  }
  // Symmetric and On Plane take their plane from a pick first
  // (mitcad#1661).
  if (takesPlane(relation)) {
    startPlanePick(relation);
    return true;
  }
  setEditNotice(QString());
  return runEdit(edit.commands, QStringLiteral("Edit Relations in %1").arg(m_model.name), relationKey(relation));
}

bool Sketch3dController::applyDimension(Dimension3 dimension) {
  if (!isActive()) {
    return false;
  }
  const EditCommand edit = dimensionFor(dimension);
  if (!edit.valid()) {
    setEditNotice(edit.problem);
    m_host.showError(tr("%1: %2").arg(dimensionName(dimension), edit.problem));
    toolLog(QStringLiteral("3D sketch edit %1 refused: %2").arg(dimensionKey(dimension), edit.problem));
    return false;
  }
  setEditNotice(QString());
  return runEdit(edit.commands, QStringLiteral("Add Dimension in %1").arg(m_model.name), dimensionKey(dimension));
}

bool Sketch3dController::applyAutoDimension() {
  if (!isActive()) {
    return false;
  }
  const EditCommand edit = autoDimensionCommand(m_model, m_uid, editSelection());
  if (!edit.valid()) {
    setEditNotice(edit.problem);
    m_host.showError(tr("Sketch Dimension: %1").arg(edit.problem));
    toolLog(QStringLiteral("3D sketch edit dimension refused: %1").arg(edit.problem));
    return false;
  }
  setEditNotice(QString());
  return runEdit(edit.commands, QStringLiteral("Add Dimension in %1").arg(m_model.name), QStringLiteral("dimension"));
}

bool Sketch3dController::toggleFix() { return applyRelation(fixToggle(m_model, editSelection())); }

bool Sketch3dController::setDimensionValue(const QString& id, const QString& text) {
  const Sketch3dItem* item = m_model.item(id);
  if (item == nullptr || !item->dimension) {
    return false;
  }
  const QString value = text.trimmed();
  if (value.isEmpty()) {
    return false;
  }
  return runEdit({QJsonObject{{QStringLiteral("cmd"), QStringLiteral("sketch3d.set_dimension")},
                              {QStringLiteral("sketch"), m_uid},
                              {QStringLiteral("dimension"), id},
                              {QStringLiteral("value"), value}}},
                 QStringLiteral("Edit Dimension in %1").arg(m_model.name),
                 QStringLiteral("value %1 = %2").arg(id, value));
}

bool Sketch3dController::setDriven(const QString& id, bool driven) {
  const Sketch3dItem* item = m_model.item(id);
  if (item == nullptr || !item->dimension || item->driven == driven) {
    return false;
  }
  return runEdit({QJsonObject{{QStringLiteral("cmd"), QStringLiteral("sketch3d.set_driven")},
                              {QStringLiteral("sketch"), m_uid},
                              {QStringLiteral("dimension"), id},
                              {QStringLiteral("driven"), driven}}},
                 QStringLiteral("Edit Dimension in %1").arg(m_model.name),
                 QStringLiteral("%1 %2").arg(driven ? QStringLiteral("driven") : QStringLiteral("driving"), id));
}

bool Sketch3dController::removeItems(const QStringList& ids) {
  if (ids.isEmpty()) {
    return false;
  }
  return runEdit({QJsonObject{{QStringLiteral("cmd"), QStringLiteral("sketch3d.remove")},
                              {QStringLiteral("sketch"), m_uid},
                              {QStringLiteral("items"), QJsonArray::fromStringList(ids)}}},
                 QStringLiteral("Delete in %1").arg(m_model.name),
                 QStringLiteral("remove %1").arg(ids.join(QLatin1Char(' '))));
}

bool Sketch3dController::deleteSelection() {
  const EditSelection selection = editSelection();
  const QStringList items = selection.points + selection.curves;
  if (items.isEmpty()) {
    m_host.showHint(tr("Select points or curves of the 3D sketch to delete."));
    return false;
  }
  // The curves that use a listed point would go with it: say so first.
  // Relations and dimensions of the point (its Fix, its attachment) go
  // with it as in a 2D sketch.
  const QStringList dependents = dependentsOf(m_model, items);
  const bool curves = std::any_of(dependents.begin(), dependents.end(),
                                  [this](const QString& id) { return m_model.curve(id) != nullptr; });
  if (curves) {
    m_pendingDelete = items;
    m_pendingDependents = dependents;
    // In words; the ids are the log's (mitcad#1652).
    const QString notice = tr("Deleting %1 also deletes %2, which use it: Delete with Dependents deletes them all.")
                               .arg(entityNames(m_model, items), entityNames(m_model, dependents));
    toolLog(QStringLiteral("3D sketch delete: %1 is used by %2")
                .arg(items.join(QLatin1Char(' ')), dependents.join(QLatin1Char(' '))));
    m_host.showHint(notice);
    setEditNotice(notice);
    emit changed();
    return false;
  }
  QJsonObject result;
  const QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.remove")},
                        {QStringLiteral("sketch"), m_uid},
                        {QStringLiteral("items"), QJsonArray::fromStringList(items)},
                        {QStringLiteral("dependents"), !dependents.isEmpty()}};
  toolLog(QStringLiteral("3D sketch command: %1").arg(compactJson(cmd)));
  if (!m_host.runCommand(cmd, &result)) {
    return false; // the model's reason is shown
  }
  qDebug().noquote() << QStringLiteral("3D sketch %1: deleted %2")
                            .arg(m_uid, QString::fromUtf8(QJsonDocument(result.value(QStringLiteral("removed")).toArray())
                                                              .toJson(QJsonDocument::Compact)));
  m_host.refreshScene();
  return true;
}

bool Sketch3dController::deleteWithDependents() {
  QStringList items;
  for (const QString& id : std::as_const(m_pendingDelete)) {
    if (m_model.point(id) != nullptr || m_model.curve(id) != nullptr) {
      items << id;
    }
  }
  m_pendingDelete.clear();
  m_pendingDependents.clear();
  setEditNotice(QString());
  if (items.isEmpty()) {
    emit changed();
    return false;
  }
  QJsonObject result;
  const QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.remove")},
                        {QStringLiteral("sketch"), m_uid},
                        {QStringLiteral("items"), QJsonArray::fromStringList(items)},
                        {QStringLiteral("dependents"), true}};
  toolLog(QStringLiteral("3D sketch command: %1").arg(compactJson(cmd)));
  if (!m_host.runCommand(cmd, &result)) {
    emit changed();
    return false;
  }
  qDebug().noquote() << QStringLiteral("3D sketch %1: deleted %2")
                            .arg(m_uid, QString::fromUtf8(QJsonDocument(result.value(QStringLiteral("removed")).toArray())
                                                              .toJson(QJsonDocument::Compact)));
  m_host.setSelection({});
  m_host.refreshScene();
  return true;
}

bool Sketch3dController::detachRecord(const QString& record) {
  const QStringList points = detachPoints(m_model, record);
  if (points.isEmpty()) {
    // In words; the record's id is the agents' (mitcad#1652).
    const QString why = tr("%1 has no point to detach").arg(entityName(m_model, record));
    setEditNotice(why);
    m_host.showError(tr("Detach: %1").arg(why));
    return false;
  }
  return runEdit({QJsonObject{{QStringLiteral("cmd"), QStringLiteral("sketch3d.detach")},
                              {QStringLiteral("sketch"), m_uid},
                              {QStringLiteral("points"), QJsonArray::fromStringList(points)}}},
                 QStringLiteral("Detach in %1").arg(m_model.name),
                 QStringLiteral("detach %1: %2").arg(record, points.join(QLatin1Char(' '))));
}

// ---------------------------------------------------------------------------
// Attach and Replace Reference: a source picked in the view

void Sketch3dController::startAttach() {
  const EditSelection selection = editSelection();
  if (selection.points.size() != 1 || !selection.curves.isEmpty()) {
    const QString why = tr("Select one point of the sketch to attach.");
    setEditNotice(why);
    m_host.showError(tr("Attach: %1").arg(why));
    return;
  }
  stopTool();
  cancelDrag();
  m_sourcePick = SourcePick::Attach;
  m_sourcePickItem = selection.points.front();
  m_sourceSnapped = false;
  setEditNotice(tr("Click a vertex, an edge, a face, a point or curve of another sketch, an Intersection Curve or "
                   "the origin to attach %1 to; Esc ends.")
                    .arg(m_sourcePickItem));
  toolLog(QStringLiteral("3D sketch attach %1: pick a source").arg(m_sourcePickItem));
  emit toolChanged(); // the pick filter takes sources
  emit changed();
}

void Sketch3dController::startReplace(const QString& record) {
  const Sketch3dReference* reference = m_model.reference(record);
  if (reference == nullptr) {
    return;
  }
  stopTool();
  cancelDrag();
  m_sourcePick = SourcePick::Replace;
  m_sourcePickItem = record;
  m_sourceSnapped = false;
  // The record and its source in words (mitcad#1652).
  const QString name = entityName(m_model, record);
  const QString source = sourceText(reference->source, m_hooks.names);
  if (reference->kind == QStringLiteral("face")) {
    setEditNotice(tr("Click a face of a body to replace %1 (%2); Esc ends.").arg(name, source));
  } else {
    setEditNotice(reference->kind == QStringLiteral("curve")
                      ? tr("Click an edge, a curve of another sketch or an Intersection Curve to replace %1 (%2); "
                           "Esc ends.")
                            .arg(name, source)
                      : tr("Click a vertex, an edge's end or middle, a point of another sketch or the origin to "
                           "replace %1 (%2); Esc ends.")
                            .arg(name, source));
  }
  toolLog(QStringLiteral("3D sketch replace %1: pick a source").arg(record));
  emit toolChanged();
  emit changed();
}

void Sketch3dController::startPlanePick(Relation3 relation) {
  stopTool();
  cancelDrag();
  m_planeRelation = relation;
  m_planeSelection = editSelection();
  m_sourcePick = SourcePick::Plane;
  m_sourcePickItem = relationKey(relation);
  m_sourceSnapped = false;
  setEditNotice(relation == Relation3::Symmetric
                    ? tr("Click a planar face, a construction plane or another sketch for the plane the two points "
                         "are mirrored across, or choose a plane of the sketch below; Esc ends.")
                    : tr("Click a planar face, a construction plane or another sketch for the plane the points are "
                         "kept on, or choose a plane of the sketch below; Esc ends."));
  toolLog(QStringLiteral("3D sketch %1: pick a plane").arg(relationKey(relation)));
  emit toolChanged(); // the pick filter takes faces
  emit changed();
}

void Sketch3dController::takeFramePlane(int axis) {
  if (m_sourcePick != SourcePick::Plane || axis < 0 || axis > 2) {
    return;
  }
  QJsonObject plane;
  plane.insert(QStringLiteral("frame"), QString(QChar(QLatin1Char("xyz"[axis]))));
  takePlane(plane);
}

void Sketch3dController::takePlane(const QJsonObject& plane) {
  EditSelection selection = m_planeSelection;
  selection.plane = plane;
  const Relation3 relation = m_planeRelation;
  const EditCommand edit = relationCommand(m_model, m_uid, relation, selection);
  if (!edit.valid()) {
    setEditNotice(edit.problem);
    m_host.showError(tr("%1: %2").arg(relationName(relation), edit.problem));
    cancelSourcePick();
    return;
  }
  const QString log = QStringLiteral("%1 about %2").arg(relationKey(relation), compactJson(plane));
  if (runEdit(edit.commands, QStringLiteral("Edit Relations in %1").arg(m_model.name), log)) {
    cancelSourcePick();
  }
}

void Sketch3dController::cancelSourcePick() {
  if (m_sourcePick == SourcePick::None) {
    return;
  }
  m_sourcePick = SourcePick::None;
  m_sourcePickItem.clear();
  m_sourceSnapped = false;
  m_viewer.setSketchSnapping(false);
  setEditNotice(QString());
  toolLog(QStringLiteral("3D sketch source pick: ended"));
  paintSnap(SnapTarget()); // its source no longer painted (mitcad#1653)
  updateOverlay();
  emit toolChanged();
  emit changed();
}

std::optional<SnapTarget> Sketch3dController::sourceSnap() const {
  if (m_sourcePick == SourcePick::None || !m_sourceSnapped) {
    return std::nullopt;
  }
  return m_sourceSnap;
}

void Sketch3dController::updateSourcePick() {
  if (m_sourcePick == SourcePick::None) {
    return;
  }
  bool points = true;
  bool curves = true;
  bool faces = true;
  if (m_sourcePick == SourcePick::Replace) {
    // A record keeps its kind: a point, a curve or a face (mitcad#1549).
    const Sketch3dReference* reference = m_model.reference(m_sourcePickItem);
    const QString kind = reference != nullptr ? reference->kind : QString();
    curves = kind == QStringLiteral("curve");
    faces = kind == QStringLiteral("face");
    points = !curves && !faces;
  } else if (m_sourcePick == SourcePick::Plane) {
    // A relation's plane (mitcad#1661): faces under the pointer; a
    // construction plane or another sketch is taken from the view's pick
    // on release.
    points = false;
    curves = false;
  }
  std::vector<SnapTarget> candidates;
  if (m_pointerInView) {
    for (const SnapTarget& target : snapCandidates(m_pointer, Xyz{}, std::nullopt)) {
      if (isSourceKind(target, points, curves, faces)) {
        candidates.push_back(target);
      }
    }
  }
  const std::optional<SnapTarget> snap = chooseSnap(candidates, kSourcePixels);
  const QString before = m_sourceSnapped ? snapDetail(m_sourceSnap) : QString();
  m_sourceSnapped = snap.has_value();
  m_sourceSnap = snap.value_or(SnapTarget());
  m_viewer.setSketchSnapping(m_sourceSnapped);
  const QString badge = m_sourceSnapped ? snapDetail(m_sourceSnap) : QString();
  if (badge != before && !badge.isEmpty()) {
    toolLog(QStringLiteral("3D sketch source: %1 at %2").arg(badge, xyzLog(m_sourceSnap.at)));
  }
  paintSnap(m_sourceSnapped ? m_sourceSnap : SnapTarget()); // its source painted (mitcad#1653)
  updateOverlay();
}

void Sketch3dController::takeSource() {
  if (m_sourcePick == SourcePick::Plane) {
    takePickedPlane();
    return;
  }
  if (!m_sourceSnapped) {
    m_host.showHint(m_editNotice);
    return;
  }
  const SnapTarget source = m_sourceSnap;
  const QString item = m_sourcePickItem;
  const bool attach = m_sourcePick == SourcePick::Attach;
  QJsonObject cmd{{QStringLiteral("cmd"), attach ? QStringLiteral("sketch3d.attach")
                                                 : QStringLiteral("sketch3d.replace_reference")},
                  {QStringLiteral("sketch"), m_uid},
                  {QStringLiteral("source"), source.source}};
  cmd.insert(attach ? QStringLiteral("point") : QStringLiteral("item"), item);
  const QString log = QStringLiteral("%1 %2: %3")
                          .arg(attach ? QStringLiteral("attach") : QStringLiteral("replace"), item, snapDetail(source));
  if (runEdit({cmd}, attach ? QStringLiteral("Attach in %1").arg(m_model.name)
                            : QStringLiteral("Replace Reference in %1").arg(m_model.name),
              log)) {
    cancelSourcePick();
  }
}

void Sketch3dController::takePickedPlane() {
  // A face under the pointer (its source), else what the view picks there:
  // a construction plane, or a point or curve of another sketch (its
  // plane). The model refuses a face that is not planar, in words.
  if (m_sourceSnapped && m_sourceSnap.kind == SnapKind::OnFace) {
    QJsonObject plane;
    plane.insert(QStringLiteral("face"), m_sourceSnap.source);
    takePlane(plane);
    return;
  }
  static const QRegularExpression feature(QStringLiteral("^F[0-9]+$"));
  const std::vector<SelectionItem> picked =
      m_pointerInView ? m_viewer.itemsAt(m_pointer) : std::vector<SelectionItem>();
  for (const SelectionItem& item : picked) {
    QJsonObject plane;
    if (item.kind == SelectKind::Plane && feature.match(item.owner).hasMatch()) {
      plane.insert(QStringLiteral("construction"), item.owner);
    } else if ((item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) &&
               item.owner != m_uid && feature.match(item.owner).hasMatch()) {
      plane.insert(QStringLiteral("sketch"), item.owner);
    } else {
      continue;
    }
    takePlane(plane);
    return;
  }
  m_host.showHint(m_editNotice);
}

// ---------------------------------------------------------------------------
// Dragging

void Sketch3dController::setDragPlane(Plane3 plane) {
  m_dragPlace.plane = plane;
  m_dragPlace.mode = Placement::Mode::Plane;
  toolLog(QStringLiteral("3D sketch drag on: %1").arg(placementText(m_dragPlace)));
  emit changed();
}

void Sketch3dController::setDragAxis(Axis3 axis) {
  m_dragPlace.axis = axis;
  m_dragPlace.mode = Placement::Mode::Axis;
  toolLog(QStringLiteral("3D sketch drag on: %1").arg(placementText(m_dragPlace)));
  emit changed();
}

bool Sketch3dController::grabAt(const QPointF& position) {
  if (!m_model.hasFrame || !m_model.solved || !m_pointerInView) {
    return false;
  }
  Drag drag;
  drag.pressAt = position;
  drag.pressed = true;
  // A tangent handle's tip first, then the nearest point, then a curve.
  double best = kGrabPixels;
  for (const TangentHandle& handle : tangentHandles(m_model)) {
    const double pixels = QLineF(onScreen(handle.tip), position).length();
    if (pixels <= best) {
      best = pixels;
      drag.kind = Drag::Kind::Handle;
      drag.entity = handle.curve;
      drag.end = handle.end;
      drag.grabbed = handle.tip;
      drag.endPoint = handle.at;
    }
  }
  if (drag.kind == Drag::Kind::None) {
    for (const Sketch3dPoint& point : m_model.points) {
      const double pixels = QLineF(onScreen(point.at), position).length();
      if (pixels <= best) {
        best = pixels;
        drag.kind = Drag::Kind::Point;
        drag.entity = point.id;
        drag.points = {point.id};
        drag.starts = {point.at};
        drag.grabbed = point.at;
      }
    }
  }
  if (drag.kind == Drag::Kind::None) {
    best = kCurvePixels;
    const auto pointAt = [this](const QString& id) -> std::optional<Xyz> {
      const Sketch3dPoint* p = m_model.point(id);
      return p != nullptr ? std::optional<Xyz>(p->at) : std::nullopt;
    };
    for (const Sketch3dCurve& curve : m_model.curves) {
      const QStringList points = dragPoints(m_model, curve.id);
      if (points.isEmpty()) {
        continue;
      }
      const std::vector<Xyz> samples =
          curveSamples(curve, curve.json.value(QStringLiteral("geometry")).toObject(), pointAt);
      const auto [at, pixels] = nearestPlace(samples, position, [this](const Xyz& p) { return onScreen(p); });
      if (pixels <= best) {
        best = pixels;
        drag.kind = Drag::Kind::Curve;
        drag.entity = curve.id;
        drag.points = points;
        drag.starts.clear();
        for (const QString& id : points) {
          drag.starts.push_back(m_model.point(id)->at);
        }
        drag.grabbed = at;
      }
    }
  }
  if (drag.kind == Drag::Kind::None) {
    return false;
  }
  m_drag = drag;
  return true;
}

void Sketch3dController::dragTo(const QPointF& position) {
  const auto ray = rayAt(position);
  const std::optional<Xyz> at = ray ? placePoint(*ray, m_drag.grabbed, m_dragPlace) : std::nullopt;
  if (!at) {
    // An edge-on plane or an end-on axis: the last place stays.
    setEditNotice(tr("The drag plane is seen edge-on here: choose another plane or axis, or turn the view."));
    return;
  }
  m_drag.target = at;
  if (!m_drag.pending) {
    m_drag.pending = true;
    QTimer::singleShot(0, this, [this] { applyDragPreview(); });
  }
}

namespace {

// sketch3d.drag of what a drag pulls toward where the pointer puts the
// place grabbed.
QJsonObject dragGoals(const QString& sketch, const QStringList& points, const std::vector<Xyz>& starts,
                      bool handle, const QString& curve, int end, const Xyz& endPoint, const Xyz& grabbed,
                      const Xyz& target, bool preview) {
  if (handle) {
    return handleDragCommand(sketch, curve, end, endPoint, target, preview);
  }
  const Xyz by = sub(target, grabbed);
  std::vector<Xyz> targets;
  targets.reserve(starts.size());
  for (const Xyz& start : starts) {
    targets.push_back(add(start, by));
  }
  return dragCommand(sketch, points, targets, preview);
}

} // namespace

void Sketch3dController::applyDragPreview() {
  m_drag.pending = false;
  if (!m_drag.active || !m_drag.target) {
    return;
  }
  if (m_host.modelBusy()) {
    // A job computes the model: after it.
    m_drag.pending = true;
    QTimer::singleShot(10, this, [this] { applyDragPreview(); });
    return;
  }
  const QJsonObject cmd =
      dragGoals(m_uid, m_drag.points, m_drag.starts, m_drag.kind == Drag::Kind::Handle, m_drag.entity, m_drag.end,
                m_drag.endPoint, m_drag.grabbed, *m_drag.target, true);
  QString problem;
  try {
    m_drag.preview = m_host.modelCommand(cmd);
    m_drag.refused = false;
  } catch (const std::exception& error) {
    m_drag.preview = QJsonObject();
    m_drag.refused = true;
    problem = QString::fromUtf8(error.what());
  }
  const DragPreview shown = dragPreview();
  const QString state = m_drag.refused ? QStringLiteral("refused: %1").arg(problem)
                        : shown.shortOf ? QStringLiteral("short of the pointer")
                                        : QStringLiteral("at the pointer");
  if (state != m_loggedDrag) {
    m_loggedDrag = state;
    toolLog(QStringLiteral("3D sketch drag preview: %1").arg(state));
  }
  setEditNotice(dragText());
  updateOverlay();
}

Sketch3dController::DragPreview Sketch3dController::dragPreview() const {
  DragPreview out;
  if (!m_drag.active) {
    return out;
  }
  out.target = m_drag.target;
  out.refused = m_drag.refused;
  if (m_drag.refused || m_drag.preview.isEmpty()) {
    return out;
  }
  const QJsonObject points = m_drag.preview.value(QStringLiteral("points")).toObject();
  const QJsonObject curves = m_drag.preview.value(QStringLiteral("curves")).toObject();
  const auto pointAt = [this, &points](const QString& id) -> std::optional<Xyz> {
    if (const auto moved = xyzValue(points.value(id))) {
      return moved;
    }
    const Sketch3dPoint* p = m_model.point(id);
    return p != nullptr ? std::optional<Xyz>(p->at) : std::nullopt;
  };
  for (const Sketch3dCurve& curve : m_model.curves) {
    if (curve.type == QStringLiteral("reference")) {
      continue;
    }
    const QJsonObject geometry = curves.contains(curve.id) ? curves.value(curve.id).toObject()
                                                          : curve.json.value(QStringLiteral("geometry")).toObject();
    std::vector<Xyz> samples = curveSamples(curve, geometry, pointAt);
    if (samples.size() >= 2) {
      out.curves.push_back(std::move(samples));
    }
  }
  for (const QString& id : m_drag.points) {
    if (const auto at = pointAt(id)) {
      out.points.push_back(*at);
    }
  }
  if (m_drag.kind == Drag::Kind::Handle) {
    const QJsonArray tangents =
        curves.value(m_drag.entity).toObject().value(QStringLiteral("end_tangents")).toArray();
    if (const auto t = tangents.size() == 2 ? xyzValue(tangents.at(m_drag.end)) : std::nullopt) {
      out.reached = handleTip(m_drag.endPoint, *t);
      out.points.push_back(*out.reached);
    }
  } else if (!m_drag.points.isEmpty() && !m_drag.starts.empty()) {
    if (const auto first = pointAt(m_drag.points.front())) {
      out.reached = add(m_drag.grabbed, sub(*first, m_drag.starts.front()));
    }
  }
  out.shortOf = out.reached && out.target && sketch::distance(*out.reached, *out.target) > kShortMillimetres;
  return out;
}

QString Sketch3dController::dragText() const {
  if (!m_drag.active) {
    return QString();
  }
  // In words; the id is the log's (mitcad#1652).
  const QString what = m_drag.kind == Drag::Kind::Handle
                           ? tr("the tangent handle of %1").arg(entityName(m_model, m_drag.entity))
                           : entityName(m_model, m_drag.entity);
  const QString head = tr("Dragging %1 %2").arg(what, placementText(m_dragPlace));
  if (m_drag.refused) {
    return tr("%1: the relations refuse this place; release takes nothing").arg(head);
  }
  const DragPreview preview = dragPreview();
  if (preview.shortOf && preview.reached && preview.target) {
    return tr("%1: it stops %2 %3 short of the pointer, as far as the relations let it")
        .arg(head,
             formatLength(sketch::distance(*preview.reached, *preview.target), millimetres()), unit());
  }
  return head;
}

void Sketch3dController::commitDrag() {
  const Drag drag = m_drag;
  m_drag = Drag();
  m_loggedDrag.clear();
  setEditNotice(QString());
  updateOverlay();
  if (!drag.target || drag.refused) {
    toolLog(QStringLiteral("3D sketch drag: nothing taken, nothing changed"));
    emit changed();
    return;
  }
  const QJsonObject cmd = dragGoals(m_uid, drag.points, drag.starts, drag.kind == Drag::Kind::Handle, drag.entity,
                                    drag.end, drag.endPoint, drag.grabbed, *drag.target, false);
  toolLog(QStringLiteral("3D sketch command: %1").arg(compactJson(cmd)));
  if (!m_host.runCommand(cmd)) {
    toolLog(QStringLiteral("3D sketch drag: refused by the model; nothing changed"));
    m_host.refreshScene();
    return;
  }
  m_host.refreshScene();
  QString reached;
  if (drag.kind == Drag::Kind::Handle) {
    for (const TangentHandle& handle : tangentHandles(m_model)) {
      if (handle.curve == drag.entity && handle.end == drag.end) {
        reached = QStringLiteral("tip %1, tangent %2")
                      .arg(xyzLog(handle.tip), xyzLog(tangentOfTip(handle.at, handle.tip)));
      }
    }
  } else if (const Sketch3dPoint* first = m_model.point(drag.points.front())) {
    reached = QStringLiteral("%1 at %2").arg(first->id, xyzLog(first->at));
  }
  toolLog(QStringLiteral("3D sketch drag: %1%2 taken; %3")
              .arg(drag.kind == Drag::Kind::Handle ? QStringLiteral("handle ") : QString(), drag.entity, reached));
}

bool Sketch3dController::cancelDrag() {
  if (!m_drag.pressed && !m_drag.active) {
    return false;
  }
  const bool active = m_drag.active;
  m_drag = Drag();
  m_loggedDrag.clear();
  if (active) {
    toolLog(QStringLiteral("3D sketch drag: cancelled, nothing changed"));
  }
  setEditNotice(QString());
  updateOverlay();
  emit changed();
  return active;
}

void Sketch3dController::forwardClick(const QPointF& release, Qt::KeyboardModifiers modifiers) {
  // A press on geometry that did not become a drag is a click: the view
  // picks as usual.
  QMouseEvent press(QEvent::MouseButtonPress, m_drag.pressAt, m_viewer.mapToGlobal(m_drag.pressAt), Qt::LeftButton,
                    Qt::LeftButton, modifiers);
  QMouseEvent up(QEvent::MouseButtonRelease, release, m_viewer.mapToGlobal(release), Qt::LeftButton, Qt::NoButton,
                 modifiers);
  m_forwarding = true;
  QCoreApplication::sendEvent(&m_viewer, &press);
  QCoreApplication::sendEvent(&m_viewer, &up);
  m_forwarding = false;
}

bool Sketch3dController::editMouse(QEvent* event) {
  if (m_forwarding) {
    return false;
  }
  switch (event->type()) {
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (m_sourcePick != SourcePick::None) {
      if ((e->buttons() & (Qt::MiddleButton | Qt::RightButton)) == 0) {
        m_pointer = e->position();
        updateSourcePick();
      }
      return (e->buttons() & Qt::LeftButton) != 0;
    }
    if (!m_drag.pressed) {
      return false;
    }
    if (!m_drag.active && QLineF(e->position(), m_drag.pressAt).length() > kDragClickPixels) {
      m_drag.active = true;
      toolLog(QStringLiteral("3D sketch drag: started %1%2 %3")
                  .arg(m_drag.kind == Drag::Kind::Handle ? QStringLiteral("handle ") : QString(), m_drag.entity,
                       placementText(m_dragPlace)));
      emit changed();
    }
    if (m_drag.active) {
      dragTo(e->position());
    }
    return true;
  }
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || m_viewer.isOnOrientationCube(e->position())) {
      return false;
    }
    if (m_sourcePick != SourcePick::None) {
      m_pointer = e->position();
      m_pointerInView = !m_viewer.isCovered(m_pointer);
      updateSourcePick();
      return true; // taken on release
    }
    if ((e->modifiers() & (Qt::ControlModifier | Qt::ShiftModifier)) != 0) {
      return false; // adds to the selection, as the view does
    }
    if (grabAt(e->position())) {
      m_drag.modifiers = e->modifiers();
      return true;
    }
    return false;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (m_sourcePick != SourcePick::None) {
      m_pointer = e->position();
      updateSourcePick();
      takeSource();
      return true;
    }
    if (m_drag.active) {
      // The last place the pointer gave, previewed or not.
      dragTo(e->position());
      commitDrag();
      return true;
    }
    if (m_drag.pressed) {
      const Qt::KeyboardModifiers modifiers = m_drag.modifiers;
      forwardClick(e->position(), modifiers);
      m_drag = Drag();
      return true;
    }
    return false;
  }
  case QEvent::MouseButtonDblClick:
    return m_sourcePick != SourcePick::None;
  default:
    break;
  }
  return false;
}

} // namespace mitcad::sketch
