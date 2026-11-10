// SPDX-License-Identifier: MIT
// What a command's panel takes in the edited sketch while it runs
// (mitcad#1563, Sketch Move/Copy): an input that picks points with the
// sketch's snaps (Point to Point's Origin and Target Point: the pointer
// snaps as the drawing tools' does, a click picks the point it snapped to
// or the place where it snapped to none), and the command's handle
// (Translate's arrow pair at the objects' middle, whose drag sets X and Y
// Distance, snapping as a drawing tool's point does).
#include "SketchController.hpp"

#include <algorithm>
#include <cmath>

#include <QCoreApplication>
#include <QDebug>
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

QString partName(char part) {
  return part == 'x'   ? QStringLiteral("x arrow")
         : part == 'y' ? QStringLiteral("y arrow")
                       : QStringLiteral("square");
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
  // A step of about the arrows' length along each axis, for their
  // directions in the view.
  const double step = std::max(pixel(), 1e-9) * kArrowPixels;
  const QPointF center = toScreen(at);
  const QPointF alongX = toScreen(at + V2{step, 0.0}) - center;
  const QPointF alongY = toScreen(at + V2{0.0, step}) - center;
  const double lengthX = std::hypot(alongX.x(), alongX.y());
  const double lengthY = std::hypot(alongY.x(), alongY.y());
  if (lengthX < 1.0 || lengthY < 1.0) {
    return std::nullopt; // the sketch plane seen edge on
  }
  HandleShape shape;
  shape.center = center;
  shape.xTip = center + alongX * (kArrowPixels / lengthX);
  shape.yTip = center + alongY * (kArrowPixels / lengthY);
  shape.dragged = m_handleDrag ? m_handlePart : '\0';
  return shape;
}

char SketchController::handlePartAt(const QPointF& screen) const {
  const std::optional<HandleShape> shape = handleShape();
  if (!shape) {
    return 0;
  }
  if (std::abs(screen.x() - shape->center.x()) <= kSquarePixels &&
      std::abs(screen.y() - shape->center.y()) <= kSquarePixels) {
    return 'b';
  }
  const double toX = segmentDistance(screen, shape->center, shape->xTip);
  const double toY = segmentDistance(screen, shape->center, shape->yTip);
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
    m_handlePressScreen = e->position();
    m_handleMoved = false;
    m_handleSent = handle->values;
    m_cursor = e->position();
    m_cursorValid = true;
    qDebug().noquote() << QStringLiteral("Sketch handle drag started on its %1").arg(partName(part));
    hint(tr("Drag: the objects follow, snapping to points and curves (Shift or Alt: no snap)."));
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
  const V2 start{handle.at[0], handle.at[1]};
  // An arrow moves along its own axis only, the square both ways.
  const auto along = [this](V2 d) {
    if (m_handlePart == 'x') {
      d.y = 0.0;
    } else if (m_handlePart == 'y') {
      d.x = 0.0;
    }
    return d;
  };
  V2 delta = along(*raw - m_handlePress);
  const V2 place = start + delta;
  if (modifiers.testFlag(Qt::ShiftModifier) || modifiers.testFlag(Qt::AltModifier)) {
    m_snap = Snap();
    m_snap.at = place;
    m_snap.screen = toScreen(place);
  } else {
    m_snap = commandSnap(place, toScreen(place), handle.moves);
    if (m_snap.onGeometry()) {
      // Onto a point or a curve: the handle's place goes there.
      delta = along(m_snap.at - start);
    } else if (m_snap.kind == Snap::Kind::Grid) {
      // The grid: the distances go in its steps, not the place.
      const double step = m_gridStep > 0.0 ? m_gridStep : m_viewer.gridStep();
      if (step > 0.0) {
        delta = V2{std::round(delta.x / step) * step, std::round(delta.y / step) * step};
      }
    }
  }
  m_viewer.setSketchSnapping(m_snap.onGeometry() && m_snap.kind != Snap::Kind::OnCurve);
  m_overlay->update();
  const std::array<double, 2> values{handle.values[0] + delta.x, handle.values[1] + delta.y};
  if (values == m_handleSent) {
    return;
  }
  m_handleSent = values;
  m_host.dragSketchHandle(handle, values[0], values[1]);
}

void SketchController::endCommandPicks() {
  const bool snapped = m_snapPicking || m_handleDrag.has_value();
  m_snapPicking = false;
  m_snapPressed = false;
  m_handleDrag.reset();
  m_handlePart = 0;
  m_loggedHandle.clear();
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
  const QString place =
      handle ? QStringLiteral("(%1, %2)").arg(logged(handle->at[0]), logged(handle->at[1])) : QString();
  if (place == m_loggedHandle) {
    return;
  }
  m_loggedHandle = place;
  if (!place.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Sketch handle at %1").arg(place);
  }
}

} // namespace mitcad::sketch
