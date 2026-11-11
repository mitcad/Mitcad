// SPDX-License-Identifier: MIT
// What a command's panel takes in the edited sketch while it runs
// (mitcad#1563, Sketch Move/Copy): an input that picks points with the
// sketch's snaps (Point to Point's Origin and Target Point: the pointer
// snaps as the drawing tools' does, a click picks the point it snapped to
// or the place where it snapped to none), and the command's handle
// (Translate's arrow pair at the objects' middle, whose drag sets X and Y
// Distance, snapping as a drawing tool's point does; Rectangular Pattern's
// arrows at its first copies along its two directions, whose drags set
// Spacing and Spacing 2 in the grid's steps, mitcad#1640).
#include "SketchController.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <limits>
#include <vector>

#include <QCoreApplication>
#include <QDebug>
#include <QJsonObject>
#include <QMouseEvent>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "SketchOverlay.hpp"

namespace mitcad::sketch {
namespace {

// A press on a snap input that moves further than this is the view's (a
// window, an orbit).
constexpr double kPressPixels = 4.0;
// The handle's arrows (view pixels), half its square, and how near the
// pointer takes an arrow.
constexpr double kArrowPixels = 48.0;
constexpr double kSquarePixels = 7.0;
constexpr double kArrowNear = 6.0;

double pixelDistance(const QPointF& a, const QPointF& b) {
  return std::hypot(a.x() - b.x(), a.y() - b.y());
}

// How far `p` is from the segment from `a` to `b` (view pixels).
double segmentDistance(const QPointF& p, const QPointF& a, const QPointF& b) {
  const QPointF d = b - a;
  const double length2 = d.x() * d.x() + d.y() * d.y();
  if (length2 <= 0.0) {
    return pixelDistance(p, a);
  }
  const double t = std::clamp(((p.x() - a.x()) * d.x() + (p.y() - a.y()) * d.y()) / length2, 0.0, 1.0);
  return pixelDistance(p, a + d * t);
}

// A coordinate or a distance for the log, without "-0.000".
QString logged(double value) {
  return QString::number(std::abs(value) < 5e-4 ? 0.0 : value, 'f', 3);
}

// The square, or an arrow by the value input it drags ("square", "dx
// arrow", "distance2 arrow").
QString partName(const SketchHandle& handle, char part) {
  return part == 'x'   ? handle.x + QStringLiteral(" arrow")
         : part == 'y' ? handle.y + QStringLiteral(" arrow")
                       : QStringLiteral("square");
}

// An arrow's direction (0: the first, 1: the second) and where it starts
// (mitcad#1640: a pattern's arrows at their first copies).
V2 handleAxis(const SketchHandle& handle, std::size_t arrow) {
  return V2{handle.axes[arrow][0], handle.axes[arrow][1]};
}

V2 handleStart(const SketchHandle& handle, std::size_t arrow) {
  if (handle.starts) {
    return V2{(*handle.starts)[arrow][0], (*handle.starts)[arrow][1]};
  }
  return V2{handle.at[0], handle.at[1]};
}

// The points the objects move: the points among them and their curves'
// points (ends, centres, control points).
QStringList movedPoints(const SketchModel& model, const QStringList& objects) {
  QStringList points;
  const auto take = [&points](const QString& id) {
    if (!id.isEmpty() && !points.contains(id)) {
      points << id;
    }
  };
  for (const QString& id : objects) {
    if (model.point(id) != nullptr) {
      take(id);
    } else if (const CurveData* curve = model.curve(id)) {
      for (const QString& own : curve->points) {
        take(own);
      }
      for (const QString& own : {curve->start, curve->end, curve->center}) {
        take(own);
      }
    }
  }
  return points;
}

// The constraints that hold `point` where `snap` put it (mitcad#1645), for
// a command that runs later: as SketchOp::constrain adds them, but nothing
// is made for them (not the origin's point nor an axis's line the sketch
// lacks, not a body's projection). Empty off geometry.
std::vector<QJsonObject> snapHold(const SketchModel& model, const QString& point, const Snap& snap) {
  std::vector<QJsonObject> hold;
  const auto coincident = [&hold, &point](const QString& entity) {
    if (!entity.isEmpty() && entity != point) {
      hold.push_back({{QStringLiteral("type"), QStringLiteral("coincident")},
                      {QStringLiteral("point"), point},
                      {QStringLiteral("entity"), entity}});
    }
  };
  if (point.isEmpty()) {
    return hold;
  }
  switch (snap.kind) {
  case Snap::Kind::Point:
  case Snap::Kind::Center:
    coincident(snap.point);
    break;
  case Snap::Kind::Origin:
    coincident(snap.point.isEmpty() ? model.originPoint() : snap.point);
    break;
  case Snap::Kind::Midpoint:
    if (!snap.curve.isEmpty()) {
      hold.push_back({{QStringLiteral("type"), QStringLiteral("midpoint")},
                      {QStringLiteral("point"), point},
                      {QStringLiteral("curve"), snap.curve}});
    }
    break;
  case Snap::Kind::OnCurve:
    // Onto an axis: on its line where the sketch has one (mitcad#948).
    if (model.curve(snap.curve) != nullptr) {
      coincident(snap.curve);
    }
    break;
  case Snap::Kind::Quadrant:
    hold = quadrantHold(model, point, snap);
    break;
  case Snap::Kind::Intersection:
    coincident(snap.curve);
    coincident(snap.curve2);
    break;
  default:
    break;
  }
  return hold;
}

// Whether `snap` went onto any of `entities` (points and curves).
bool snapsOnto(const Snap& snap, const QStringList& entities) {
  for (const QString& id : {snap.point, snap.curve, snap.curve2}) {
    if (!id.isEmpty() && entities.contains(id)) {
      return true;
    }
  }
  return false;
}

} // namespace

std::optional<SketchController::HandleShape> SketchController::handleShape() const {
  if (!isActive() || !m_paused) {
    return std::nullopt;
  }
  const std::optional<SketchHandle> handle = m_host.sketchHandle();
  if (!handle) {
    return std::nullopt;
  }
  const V2 at{handle->at[0], handle->at[1]};
  // A step of about the arrows' length along each arrow's direction, for
  // their directions in the view.
  const double step = std::max(pixel(), 1e-9) * kArrowPixels;
  std::array<QPointF, 2> from;
  std::array<QPointF, 2> tip;
  for (std::size_t k = 0; k < 2; ++k) {
    const V2 start = handleStart(*handle, k);
    from[k] = toScreen(start);
    const QPointF along = toScreen(start + handleAxis(*handle, k) * step) - from[k];
    const double length = std::hypot(along.x(), along.y());
    if (length < 1.0) {
      return std::nullopt; // the sketch plane seen edge on
    }
    tip[k] = from[k] + along * (kArrowPixels / length);
  }
  HandleShape shape;
  shape.center = toScreen(at);
  shape.xFrom = from[0];
  shape.yFrom = from[1];
  shape.xTip = tip[0];
  shape.yTip = tip[1];
  shape.square = handle->square;
  shape.faint = handle->faint;
  shape.dragged = m_handleDrag ? m_handlePart : '\0';
  return shape;
}

char SketchController::handlePartAt(const QPointF& screen) const {
  const std::optional<HandleShape> shape = handleShape();
  if (!shape) {
    return 0;
  }
  if (shape->square && std::abs(screen.x() - shape->center.x()) <= kSquarePixels &&
      std::abs(screen.y() - shape->center.y()) <= kSquarePixels) {
    return 'b';
  }
  // A faint arrow is a hint only (mitcad#1640): it takes no press.
  constexpr double kFar = std::numeric_limits<double>::infinity();
  const double toX = shape->faint[0] ? kFar : segmentDistance(screen, shape->xFrom, shape->xTip);
  const double toY = shape->faint[1] ? kFar : segmentDistance(screen, shape->yFrom, shape->yTip);
  if (std::min(toX, toY) > kArrowNear) {
    return 0;
  }
  return toX <= toY ? 'x' : 'y';
}

Snap SketchController::commandSnap(const V2& raw, const QPointF& screen, const QStringList& moves) {
  SnapSettings settings;
  settings.model = &m_model;
  settings.toScreen = [this](const V2& p) { return toScreen(p); };
  settings.grid = m_gridSnap;
  settings.gridStep = m_gridStep > 0.0 ? m_gridStep : m_viewer.gridStep(); // the lines shown
  settings.axes = true;
  // Not onto what moves with the handle: its own points and curves.
  for (const QString& id : moves) {
    if (const CurveData* curve = m_model.curve(id)) {
      settings.skipCurves.insert(id);
      for (const QString& point : curve->points) {
        settings.skipPoints.insert(point);
      }
    } else {
      settings.skipPoints.insert(id);
    }
  }
  return snap(settings, raw, screen);
}

bool SketchController::snapPickEvent(QEvent* event) {
  if (!m_host.sketchSnapsPickable()) {
    if (m_snapPicking || m_snapPressed) {
      m_snapPicking = false;
      m_snapPressed = false;
      m_snap = Snap();
      m_viewer.setSketchSnapping(false);
      m_overlay->update();
    }
    return false;
  }
  const QEvent::Type type = event->type();
  if (type == QEvent::Leave) {
    m_cursorValid = false;
    m_overlay->update();
    return false;
  }
  if (type != QEvent::MouseMove && type != QEvent::MouseButtonPress && type != QEvent::MouseButtonRelease) {
    return false;
  }
  const auto* e = static_cast<const QMouseEvent*>(event);
  // Where the pointer snaps, as the drawing tools' does (with the grid
  // and the sketch's axes).
  const auto snapAt = [this](const QPointF& position) {
    const auto raw = sketchAt(position);
    m_cursor = position;
    m_cursorValid = raw.has_value();
    if (raw) {
      m_snap = commandSnap(*raw, position, {});
      m_snapPicking = true;
    }
    m_viewer.setSketchSnapping(m_cursorValid && m_snap.onGeometry() && m_snap.kind != Snap::Kind::OnCurve);
    m_overlay->update();
    return raw.has_value();
  };
  switch (type) {
  case QEvent::MouseButtonPress:
    if (e->button() != Qt::LeftButton || m_viewer.isOnOrientationCube(e->position())) {
      return false;
    }
    m_snapPressed = true;
    m_snapPress = e->position();
    snapAt(e->position());
    return true;
  case QEvent::MouseMove:
    if (m_snapPressed && (e->buttons() & Qt::LeftButton) != 0 &&
        pixelDistance(e->position(), m_snapPress) > kPressPixels) {
      // A drag from the press: the view's (a window, an orbit), from where
      // it was pressed.
      m_snapPressed = false;
      QMouseEvent press(QEvent::MouseButtonPress, m_snapPress, m_viewer.mapToGlobal(m_snapPress), Qt::LeftButton,
                        Qt::LeftButton, e->modifiers());
      m_forwarding = true;
      QCoreApplication::sendEvent(&m_viewer, &press);
      m_forwarding = false;
      return false;
    }
    snapAt(e->position());
    return false;
  case QEvent::MouseButtonRelease: {
    if (e->button() != Qt::LeftButton || !m_snapPressed) {
      return false;
    }
    m_snapPressed = false;
    if (!snapAt(e->position())) {
      return true;
    }
    // A point of the sketch where it snapped to one, else the place.
    SelectionItem item;
    item.kind = SelectKind::SketchPoint;
    item.owner = m_uid;
    item.occurrence = m_host.sketchOccurrence(m_uid);
    const bool atPoint = m_model.point(m_snap.point) != nullptr &&
                         (m_snap.atPoint() || m_snap.kind == Snap::Kind::Origin);
    item.name = atPoint ? m_snap.point : placePick(m_snap.at);
    // What the pick snapped onto, for a hold there (mitcad#1645, pickHold).
    if (m_snap.onGeometry()) {
      m_pickSnaps.insert(item.name, m_snap);
    } else {
      m_pickSnaps.remove(item.name);
    }
    // For UI tests: what the click picked and what it snapped to.
    qDebug().noquote() << QStringLiteral("Sketch point picked for the command: %1 at (%2, %3), %4")
                              .arg(item.name, logged(m_snap.at.x), logged(m_snap.at.y), m_snap.describe());
    // The view's picks of earlier clicks first (mitcad#488).
    m_host.deliverPendingPicks();
    m_host.pick({item}, e->modifiers());
    m_overlay->update();
    return true;
  }
  default:
    return false;
  }
}

bool SketchController::handleEvent(QEvent* event) {
  const QEvent::Type type = event->type();
  if (type != QEvent::MouseMove && type != QEvent::MouseButtonPress && type != QEvent::MouseButtonRelease) {
    return false;
  }
  const auto* e = static_cast<const QMouseEvent*>(event);
  if (!m_handleDrag) {
    if (type != QEvent::MouseButtonPress || e->button() != Qt::LeftButton ||
        m_viewer.isOnOrientationCube(e->position())) {
      return false;
    }
    const std::optional<SketchHandle> handle = m_host.sketchHandle();
    const char part = handle ? handlePartAt(e->position()) : '\0';
    const auto at = sketchAt(e->position());
    if (part == 0 || !at) {
      return false;
    }
    m_handleDrag = handle;
    m_handlePart = part;
    m_handlePress = *at;
    m_handlePlace = V2{handle->at[0], handle->at[1]};
    // The point of the objects at the handle's place (a circle's centre, a
    // lone point), which a drop on a point or a curve holds there
    // (mitcad#1645); none at a line's middle.
    m_handleBase.clear();
    if (handle->snapsToGeometry) {
      for (const QString& p : movedPoints(m_model, handle->moves)) {
        const auto where = m_model.pointAt(p);
        if (where && dist(*where, m_handlePlace) <= 1e-6) {
          m_handleBase = p;
          break;
        }
      }
    }
    m_handlePressScreen = e->position();
    m_handleMoved = false;
    m_handleSent = handle->values;
    m_cursor = e->position();
    m_cursorValid = true;
    qDebug().noquote() << QStringLiteral("Sketch handle drag started on its %1").arg(partName(*handle, part));
    hint(handle->dragHint.isEmpty()
             ? tr("Drag: the objects follow, snapping to points and curves (Shift or Alt: no snap).")
             : handle->dragHint);
    m_overlay->update();
    return true;
  }
  if (type == QEvent::MouseMove) {
    dragHandle(e->position(), e->modifiers());
    return true;
  }
  if (type == QEvent::MouseButtonRelease && e->button() == Qt::LeftButton) {
    dragHandle(e->position(), e->modifiers());
    const SketchHandle handle = *m_handleDrag;
    if (m_handleDrop) {
      // For UI tests: what the drop holds (dragHandle set it).
      qDebug().noquote() << QStringLiteral("Sketch handle holds %1 where it was dropped: %2")
                                .arg(m_handleDrop->point, m_handleDrop->snap.describe());
    }
    m_handleDrag.reset();
    m_handlePart = 0;
    m_snap = Snap();
    m_viewer.setSketchSnapping(false);
    qDebug().noquote() << QStringLiteral("Sketch handle dropped: %1 %2, %3 %4")
                              .arg(handle.x, logged(m_handleSent[0]), handle.y, logged(m_handleSent[1]));
    m_overlay->update();
    return true;
  }
  return true; // another button while the handle is dragged
}

void SketchController::dragHandle(const QPointF& screen, Qt::KeyboardModifiers modifiers) {
  if (!m_handleDrag) {
    return;
  }
  // A click on the handle moves nothing.
  m_handleMoved = m_handleMoved || pixelDistance(screen, m_handlePressScreen) > kPressPixels;
  if (!m_handleMoved) {
    return;
  }
  const SketchHandle& handle = *m_handleDrag;
  const auto raw = sketchAt(screen);
  if (!raw) {
    return;
  }
  m_cursor = screen;
  m_cursorValid = true;
  // The part's start: the square at the handle's place, an arrow where it
  // starts (a pattern's at its first copy, mitcad#1640).
  const std::size_t arrow = m_handlePart == 'y' ? 1 : 0;
  const V2 start = m_handlePart == 'b' ? V2{handle.at[0], handle.at[1]} : handleStart(handle, arrow);
  const V2 first = handleAxis(handle, 0);
  const V2 second = handleAxis(handle, 1);
  // How far a move goes along the arrows' directions (at right angles to
  // each other): an arrow moves along its own only, the square both ways.
  const auto along = [this, first, second](const V2& d) {
    return std::array<double, 2>{m_handlePart == 'y' ? 0.0 : dot(d, first),
                                 m_handlePart == 'x' ? 0.0 : dot(d, second)};
  };
  const auto offset = [first, second](const std::array<double, 2>& by) { return first * by[0] + second * by[1]; };
  std::array<double, 2> moved = along(*raw - m_handlePress);
  const V2 place = start + offset(moved);
  bool grid = false;
  if (modifiers.testFlag(Qt::ShiftModifier) || modifiers.testFlag(Qt::AltModifier)) {
    m_snap = Snap();
    m_snap.at = place;
    m_snap.screen = toScreen(place);
  } else if (!handle.snapsToGeometry) {
    // A pattern's spacing (mitcad#1640): the grid's steps only, not onto
    // the copies the drag moves.
    m_snap = Snap();
    m_snap.at = place;
    m_snap.screen = toScreen(place);
    grid = m_gridSnap;
  } else {
    m_snap = commandSnap(place, toScreen(place), handle.moves);
    grid = m_snap.kind == Snap::Kind::Grid;
  }
  if (m_snap.onGeometry()) {
    // Onto a point or a curve: the handle's place goes there.
    moved = along(m_snap.at - start);
  } else if (grid) {
    // The grid: the distances go in its steps, not the place.
    const double step = m_gridStep > 0.0 ? m_gridStep : m_viewer.gridStep();
    if (step > 0.0) {
      moved = {std::round(moved[0] / step) * step, std::round(moved[1] / step) * step};
    }
  }
  m_handlePlace = start + offset(moved);
  m_viewer.setSketchSnapping(m_snap.onGeometry() && m_snap.kind != Snap::Kind::OnCurve);
  m_overlay->update();
  const std::array<double, 2> values{handle.values[0] + moved[0], handle.values[1] + moved[1]};
  // With its place on a point or a curve, the moved point there is held
  // there by the command (mitcad#1645, handleHold), set before the values
  // go in so that the preview they start has it; any other place (off
  // geometry, Shift or Alt, an arrow that leaves it short of the snap)
  // takes it back.
  const bool hadDrop = m_handleDrop.has_value();
  m_handleDrop.reset();
  if (m_snap.onGeometry() && !m_handleBase.isEmpty() && dist(m_handlePlace, m_snap.at) <= 1e-6) {
    HandleDrop drop;
    drop.point = m_handleBase;
    drop.snap = m_snap;
    drop.values = values;
    m_handleDrop = drop;
  }
  // The same values with a hold now (or none now): sent again, so that the
  // preview is built again with it.
  if (values == m_handleSent && hadDrop == m_handleDrop.has_value()) {
    return;
  }
  m_handleSent = values;
  // Only the values the part drags: an arrow leaves the other input as it
  // is typed (an expression stays one).
  SketchHandle sent = handle;
  if (m_handlePart == 'x') {
    sent.y.clear();
  } else if (m_handlePart == 'y') {
    sent.x.clear();
  }
  m_host.dragSketchHandle(sent, values[0], values[1]);
}

void SketchController::endCommandPicks() {
  const bool snapped = m_snapPicking || m_handleDrag.has_value();
  m_snapPicking = false;
  m_snapPressed = false;
  m_handleDrag.reset();
  m_handlePart = 0;
  m_loggedHandle.clear();
  m_handleBase.clear();
  m_handleDrop.reset();
  m_pickSnaps.clear();
  if (snapped && !m_tool) {
    m_snap = Snap();
    m_viewer.setSketchSnapping(false);
  }
}

void SketchController::logHandle() {
  std::optional<SketchHandle> handle;
  if (isActive() && m_paused) {
    handle = m_host.sketchHandle();
  }
  QString place =
      handle ? QStringLiteral("(%1, %2)").arg(logged(handle->at[0]), logged(handle->at[1])) : QString();
  if (handle && handle->starts) {
    // A pattern's arrows (mitcad#1640): ", arrows distance1 from (-10.000,
    // -15.000) along (1.000, 0.000), distance2 from (-20.000, -5.000)
    // along (0.000, 1.000) faint".
    QStringList arrows;
    for (std::size_t k = 0; k < 2; ++k) {
      const V2 start = handleStart(*handle, k);
      const V2 axis = handleAxis(*handle, k);
      arrows << QStringLiteral("%1 from (%2, %3) along (%4, %5)%6")
                    .arg(k == 0 ? handle->x : handle->y, logged(start.x), logged(start.y), logged(axis.x),
                         logged(axis.y), handle->faint[k] ? QStringLiteral(" faint") : QString());
    }
    place += QStringLiteral(", arrows ") + arrows.join(QStringLiteral(", "));
  }
  if (place == m_loggedHandle) {
    return;
  }
  m_loggedHandle = place;
  if (!place.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Sketch handle at %1").arg(place);
  }
}


std::vector<QJsonObject> SketchController::handleHold(const QStringList& objects, double dx, double dy) const {
  if (!m_handleDrop || !std::isfinite(dx) || !std::isfinite(dy) ||
      !movedPoints(m_model, objects).contains(m_handleDrop->point)) {
    return {};
  }
  // The values as the panel holds them: written with four decimals in the
  // document's unit (CommandSession::manipulatorDragged), so a value typed
  // since differs by more than half the last one.
  const double tolerance = 0.6e-4 * std::max(m_unitScale, 1e-9) + 1e-9;
  if (std::abs(dx - m_handleDrop->values[0]) > tolerance || std::abs(dy - m_handleDrop->values[1]) > tolerance) {
    return {};
  }
  return snapHold(m_model, m_handleDrop->point, m_handleDrop->snap);
}

std::vector<QJsonObject> SketchController::pickHold(const QStringList& objects, const QString& from,
                                                    const QString& to) const {
  const QStringList moved = movedPoints(m_model, objects);
  if (from.isEmpty() || to.isEmpty() || from == to || !moved.contains(from)) {
    return {};
  }
  Snap snap;
  if (m_pickSnaps.contains(to)) {
    snap = m_pickSnaps.value(to);
  } else if (m_model.point(to) != nullptr) {
    // A point picked otherwise (selected before the input took snaps).
    snap.kind = Snap::Kind::Point;
    snap.point = to;
  }
  // Not onto what moves with it: that would hold it to itself.
  QStringList moving = moved;
  moving << objects;
  if (snapsOnto(snap, moving)) {
    return {};
  }
  return snapHold(m_model, from, snap);
}

} // namespace mitcad::sketch
