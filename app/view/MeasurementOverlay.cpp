// SPDX-License-Identifier: MIT
// Annotations in the view (mitcad#649, mitcad#702; app/COMMANDS.md
// "Annotations"): OcctViewer::setMeasurements and the overlay that draws
// them and takes the mouse on their texts and lines.
#include "MeasurementOverlay.hpp"

#include <cmath>
#include <initializer_list>
#include <optional>
#include <utility>
#include <vector>

#include <QContextMenuEvent>
#include <QEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QStringList>
#include <QTimer>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "MeasurementDrawing.hpp"

namespace mitcad {
namespace {

// A press moved this far (pixels) is a drag.
constexpr double kDragStart = 4.0;

} // namespace

void OcctViewer::setMeasurements(const std::vector<MeasurementDisplay>& measurements) {
  m_measurements = measurements;
  if (m_measurementOverlay == nullptr && !measurements.empty()) {
    m_measurementOverlay = new MeasurementOverlay(this);
  }
  if (m_measurementOverlay != nullptr) {
    m_measurementOverlay->refresh();
  }
  // For UI tests: what is drawn, when it changes; and where the labels
  // are, in the window's coordinates.
  QStringList drawn;
  for (const MeasurementDisplay& m : measurements) {
    drawn << QStringLiteral("%1 = %2 (%3)").arg(m.name, m.label, m.autoColor ? QStringLiteral("auto") : m.color.name());
  }
  const QString logged =
      QStringLiteral("Annotations drawn: %1").arg(drawn.isEmpty() ? QStringLiteral("none") : drawn.join(QStringLiteral(" | ")));
  if (logged != m_loggedMeasurements) {
    m_loggedMeasurements = logged;
    qDebug().noquote() << logged;
  }
  // Where their labels and dimension lines are, when that changes (a drag
  // moved one).
  logAnnotationPlaces();
}

void OcctViewer::setMeasurementPreview(const std::vector<MeasurementDisplay>& preview) {
  m_measurementPreview = preview;
  for (MeasurementDisplay& m : m_measurementPreview) {
    m.preview = true;
  }
  if (m_measurementOverlay == nullptr && !preview.empty()) {
    m_measurementOverlay = new MeasurementOverlay(this);
  }
  if (m_measurementOverlay != nullptr) {
    m_measurementOverlay->refresh();
  }
  QStringList drawn;
  for (const MeasurementDisplay& m : m_measurementPreview) {
    drawn << QStringLiteral("%1 = %2").arg(m.name, m.label);
  }
  const QString logged = QStringLiteral("Annotation preview drawn: %1")
                             .arg(drawn.isEmpty() ? QStringLiteral("none") : drawn.join(QStringLiteral(" | ")));
  if (logged != m_loggedMeasurementPreview) {
    m_loggedMeasurementPreview = logged;
    qDebug().noquote() << logged;
  }
  logAnnotationPlaces();
}

void OcctViewer::logAnnotationPlaces() {
  const MeasurementProjection project = [this](const gp_Pnt& p) -> std::optional<QPointF> { return toWidgetF(p); };
  QStringList places;
  // The shown ones, then a panel's preview (mitcad#1097), named so.
  std::vector<const MeasurementDisplay*> drawn;
  for (const MeasurementDisplay& m : m_measurements) {
    drawn.push_back(&m);
  }
  for (const MeasurementDisplay& m : m_measurementPreview) {
    drawn.push_back(&m);
  }
  for (const MeasurementDisplay* item : drawn) {
    const MeasurementDisplay& m = *item;
    const QString what = m.preview ? QStringLiteral("Annotation preview") : QStringLiteral("Annotation");
    const MeasurementShape shape = layoutMeasurement(m, project);
    if (!shape.valid) {
      continue;
    }
    // Where a click takes its text: the text's middle, or, where that is
    // past the view's edge (a text in the model's height can reach past it,
    // mitcad#894) or under a floating card (the Browser over the view's
    // side, mitcad#1168), the middle of the part of the text in the view's
    // free area.
    QPointF label = shape.labelAt;
    const QRectF area = freeArea().isEmpty() ? QRectF(rect()) : freeArea();
    const QRectF inside = area.adjusted(2.0, 2.0, -2.0, -2.0);
    if (!inside.contains(label)) {
      const QPolygonF seen = shape.textArea.intersected(QPolygonF(inside));
      if (seen.size() >= 3) {
        QPointF sum;
        for (const QPointF& corner : seen) {
          sum += corner;
        }
        label = sum / static_cast<double>(seen.size());
      }
    }
    const QPoint at = mapTo(window(), label.toPoint());
    places << QStringLiteral("%1 label %2 at %3,%4").arg(what, m.name).arg(at.x()).arg(at.y());
    // Its text's capital height on screen and a leader's landing with its
    // bend (mitcad#1168: the text to the design's scale, the landing in
    // proportion to it, the bend obtuse).
    places << QStringLiteral("%1 text %2 %3 px").arg(what, m.name).arg(shape.textPixels, 0, 'f', 1);
    if (shape.landingPixels > 0.0) {
      places << QStringLiteral("%1 landing %2 %3 px, bend %4 deg")
                    .arg(what, m.name)
                    .arg(shape.landingPixels, 0, 'f', 1)
                    .arg(shape.bendDegrees, 0, 'f', 1);
      const QPoint knee = mapTo(window(), shape.kneeAt.toPoint());
      const QPoint point = mapTo(window(), shape.pointAt.toPoint());
      places << QStringLiteral("%1 knee %2 at %3,%4").arg(what, m.name).arg(knee.x()).arg(knee.y());
      places << QStringLiteral("%1 point %2 at %3,%4").arg(what, m.name).arg(point.x()).arg(point.y());
    }
    if (!shape.grips.empty() && !shape.grips.front().isEmpty()) {
      const QPolygonF& grip = shape.grips.front();
      // A quarter along it: its middle is under the text.
      const QPointF on = grip.size() == 2 ? grip[0] + (grip[1] - grip[0]) * 0.25 : grip[grip.size() / 4];
      const QPoint line = mapTo(window(), on.toPoint());
      places << QStringLiteral("%1 line %2 at %3,%4").arg(what, m.name).arg(line.x()).arg(line.y());
    }
  }
  const QString placed = places.join(QLatin1Char('\n'));
  if (placed != m_loggedAnnotationPlaces) {
    m_loggedAnnotationPlaces = placed;
    for (const QString& line : std::as_const(places)) {
      qDebug().noquote() << line;
    }
  }
}

void OcctViewer::setHighlightedAnnotations(const QStringList& names) {
  if (names == m_highlightedAnnotations) {
    return;
  }
  m_highlightedAnnotations = names;
  qDebug().noquote() << QStringLiteral("Annotations highlighted: %1")
                            .arg(names.isEmpty() ? QStringLiteral("none") : names.join(QStringLiteral(", ")));
  if (m_measurementOverlay != nullptr) {
    m_measurementOverlay->update();
  }
}

MeasurementOverlay::MeasurementOverlay(OcctViewer* viewer) : QWidget(viewer), m_viewer(viewer) {
  setObjectName(QStringLiteral("measurementOverlay"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setAttribute(Qt::WA_TranslucentBackground);
  setFocusPolicy(Qt::NoFocus);
  hide();
  viewer->installEventFilter(this);
  // The camera moved (a frame was drawn): the annotations follow their
  // points.
  connect(viewer, &OcctViewer::viewChanged, this, [this] {
    if (isVisible()) {
      follow();
    }
  });
  // The background turned dark or light: the halos take its tone, and
  // Auto's ink what lies behind (mitcad#902) once the camera rests.
  connect(viewer, &OcctViewer::backgroundToneChanged, this, [this] {
    update();
    scheduleBackdrop();
  });
  connect(viewer, &OcctViewer::cameraRested, this, [this] { scheduleBackdrop(); });
}

void MeasurementOverlay::scheduleBackdrop() {
  if (m_backdropQueued) {
    return;
  }
  m_backdropQueued = true;
  QTimer::singleShot(0, this, [this] {
    m_backdropQueued = false;
    grabBackdrop();
  });
}

void MeasurementOverlay::grabBackdrop() {
  if (m_grabbing || !isVisible() || m_viewer->width() <= 0) {
    return;
  }
  // The view's own frame, without this overlay (a child widget of its own).
  m_grabbing = true;
  const QImage frame = m_viewer->grabFramebuffer();
  m_grabbing = false;
  if (frame.isNull()) {
    return;
  }
  m_backdrop.image = frame.convertToFormat(QImage::Format_RGB32);
  m_backdrop.pixels = static_cast<double>(frame.width()) / m_viewer->width();
  m_backdropKey = placesKey();
  qDebug().noquote() << QStringLiteral("Annotation backdrop: %1 x %2").arg(frame.width()).arg(frame.height());
  update();
}

void MeasurementOverlay::refresh() {
  if (m_viewer->measurements().empty() && m_viewer->measurementPreview().empty()) {
    hide();
    m_pressed.clear();
    m_dragging = false;
    return;
  }
  show();
  lower(); // under the view's own buttons and cards
  if (geometry() != m_viewer->rect()) {
    setGeometry(m_viewer->rect());
  }
  m_drawnKey = placesKey();
  update();
  scheduleBackdrop();
}

QString MeasurementOverlay::placesKey() const {
  QStringList places;
  for (const std::vector<MeasurementDisplay>* drawn : {&m_viewer->measurements(), &m_viewer->measurementPreview()}) {
    for (const MeasurementDisplay& m : *drawn) {
      const gp_Pnt p = m.planar ? m.textAt
                       : m.arc  ? m.corner
                       : m.circle ? m.center
                       : !m.points.empty() ? m.points.front()
                                           : gp_Pnt();
      const QPointF at = m_viewer->toWidgetF(p);
      places << QStringLiteral("%1,%2").arg(std::lround(at.x())).arg(std::lround(at.y()));
    }
  }
  // A turn about the text's point turns the text too.
  const gp_Dir eye = m_viewer->viewDirection();
  places << QStringLiteral("%1,%2,%3")
                .arg(std::lround(eye.X() * 1000.0))
                .arg(std::lround(eye.Y() * 1000.0))
                .arg(std::lround(eye.Z() * 1000.0));
  return places.join(QLatin1Char(' '));
}

void MeasurementOverlay::follow() {
  bool moved = false;
  if (geometry() != m_viewer->rect()) {
    setGeometry(m_viewer->rect());
    update();
    moved = true;
  }
  const QString key = placesKey();
  if (key != m_drawnKey) {
    m_drawnKey = key;
    update();
    moved = true;
  }
  // For UI tests: the labels' places again once the view is laid out or
  // the camera moved them (a fit, a turn), not only when they change.
  if (moved) {
    m_viewer->logAnnotationPlaces();
  }
}

int MeasurementOverlay::hitAt(const QPointF& at, QString* name) const {
  const MeasurementProjection project = [this](const gp_Pnt& p) -> std::optional<QPointF> {
    return m_viewer->toWidgetF(p);
  };
  // The last drawn is on top.
  const std::vector<MeasurementDisplay>& shown = m_viewer->measurements();
  for (auto m = shown.rbegin(); m != shown.rend(); ++m) {
    const int part = measurementHit(layoutMeasurement(*m, project), at);
    if (part != 0) {
      *name = m->name;
      return part;
    }
  }
  return 0;
}

bool MeasurementOverlay::mouse(QEvent* event) {
  const OcctViewer::AnnotationActions& actions = m_viewer->annotationActions();
  // A release waited for after a double or right click went to the menu
  // or the editor that opened instead: a new press starts afresh.
  // So does a press or drag whose release never came (a popup took it):
  // forgotten, with its cursor.
  if (event->type() == QEvent::MouseButtonPress || event->type() == QEvent::MouseButtonDblClick) {
    m_swallowRelease = false;
    if (!m_pressed.isEmpty() || m_dragging || m_hovering) {
      const bool drawnMoved = m_dragging;
      m_pressed.clear();
      m_dragging = false;
      m_hovering = false;
      m_viewer->restoreCursor();
      if (drawnMoved) {
        update();
      }
    }
  }
  // While a command takes picks or a sketch is edited, the view has the
  // mouse (a press taken already ends here, a waited-for release too).
  const bool waited = event->type() == QEvent::MouseButtonRelease && m_swallowRelease;
  if (actions.active && !actions.active() && m_pressed.isEmpty() && !waited) {
    if (m_hovering) {
      m_hovering = false;
      m_viewer->restoreCursor();
    }
    setHovered(QString());
    return false;
  }
  switch (event->type()) {
  case QEvent::MouseButtonPress: {
    auto* press = static_cast<QMouseEvent*>(event);
    QString name;
    const int part = hitAt(press->position(), &name);
    if (part == 0) {
      return false;
    }
    if (press->button() == Qt::RightButton) {
      m_swallowRelease = true;
      if (actions.menu) {
        actions.menu(name, press->globalPosition().toPoint());
      }
      return true;
    }
    if (press->button() != Qt::LeftButton) {
      return false;
    }
    m_pressed = name;
    m_part = part;
    m_pressAt = press->position();
    m_delta = QPointF();
    m_dragging = false;
    return true;
  }
  case QEvent::MouseMove: {
    auto* move = static_cast<QMouseEvent*>(event);
    if (m_pressed.isEmpty()) {
      // Over an annotation: a hand, which says it takes the mouse there,
      // and the annotation in the hover's colour (mitcad#902).
      QString name;
      const bool over = move->buttons() == Qt::NoButton && hitAt(move->position(), &name) != 0;
      setHovered(over ? name : QString());
      if (over != m_hovering) {
        m_hovering = over;
        if (over) {
          m_viewer->setCursor(Qt::OpenHandCursor);
        } else {
          m_viewer->restoreCursor();
        }
      }
      return false;
    }
    m_delta = move->position() - m_pressAt;
    if (!m_dragging && std::hypot(m_delta.x(), m_delta.y()) >= kDragStart) {
      m_dragging = true;
      m_viewer->setCursor(Qt::ClosedHandCursor);
    }
    if (m_dragging) {
      update();
    }
    return true;
  }
  case QEvent::MouseButtonRelease: {
    if (m_pressed.isEmpty()) {
      // The release of a double-click or a right click taken here.
      const bool taken = m_swallowRelease;
      m_swallowRelease = false;
      return taken;
    }
    const QString name = m_pressed;
    const bool dragged = m_dragging;
    const bool line = m_part == 2;
    const QPointF delta = m_delta;
    m_pressed.clear();
    m_dragging = false;
    m_viewer->restoreCursor();
    m_hovering = false;
    if (!dragged) {
      qDebug().noquote() << QStringLiteral("Annotation clicked: %1").arg(name);
      if (actions.clicked) {
        actions.clicked(name);
      }
      return true;
    }
    update();
    // The drag in millimetres along the plane's axes, where the text was
    // when it started.
    const MeasurementProjection project = [this](const gp_Pnt& p) -> std::optional<QPointF> {
      return m_viewer->toWidgetF(p);
    };
    for (const MeasurementDisplay& m : m_viewer->measurements()) {
      if (m.name != name) {
        continue;
      }
      const std::optional<std::array<double, 2>> moved =
          measurementDragMillimetres(layoutMeasurement(m, project), delta);
      if (!moved) {
        break;
      }
      qDebug().noquote() << QStringLiteral("Annotation %1 dragged: %2 by %3 x %4 mm")
                                .arg(name, line ? QStringLiteral("line") : QStringLiteral("text"))
                                .arg((*moved)[0], 0, 'f', 3)
                                .arg((*moved)[1], 0, 'f', 3);
      if (actions.moved) {
        actions.moved(name, line, (*moved)[0], (*moved)[1]);
      }
      break;
    }
    return true;
  }
  case QEvent::MouseButtonDblClick: {
    auto* click = static_cast<QMouseEvent*>(event);
    QString name;
    if (click->button() != Qt::LeftButton || hitAt(click->position(), &name) == 0) {
      return false;
    }
    m_pressed.clear();
    m_swallowRelease = true; // its release, which the view must not see alone
    m_dragging = false;
    qDebug().noquote() << QStringLiteral("Annotation double-clicked: %1").arg(name);
    if (actions.opened) {
      actions.opened(name);
    }
    return true;
  }
  case QEvent::ContextMenu: {
    // The menu was asked for on the press already.
    auto* menu = static_cast<QContextMenuEvent*>(event);
    QString name;
    return hitAt(QPointF(menu->pos()), &name) != 0;
  }
  default:
    return false;
  }
}

void MeasurementOverlay::setHovered(const QString& name) {
  if (name == m_hovered) {
    return;
  }
  m_hovered = name;
  qDebug().noquote() << QStringLiteral("Annotation hovered: %1").arg(name.isEmpty() ? QStringLiteral("none") : name);
  update();
}

bool MeasurementOverlay::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_viewer) {
    if (event->type() == QEvent::Resize && isVisible()) {
      follow();
    }
    // The mouse left the view: nothing hovered.
    if (event->type() == QEvent::Leave) {
      setHovered(QString());
    }
    if (isVisible() && mouse(event)) {
      return true;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void MeasurementOverlay::paintEvent(QPaintEvent* /*event*/) {
  const MeasurementProjection project = [this](const gp_Pnt& p) -> std::optional<QPointF> {
    return m_viewer->toWidgetF(p);
  };
  const QStringList& highlighted = m_viewer->highlightedAnnotations();
  std::vector<MeasurementShape> shapes;
  for (const MeasurementDisplay& m : m_viewer->measurements()) {
    MeasurementShape shape = layoutMeasurement(m, project);
    shape.highlighted = highlighted.contains(m.name);
    shape.hovered = !shape.highlighted && m.name == m_hovered;
    if (m_dragging && m.name == m_pressed) {
      shape = draggedMeasurement(shape, m_part == 2, m_delta);
    }
    shapes.push_back(shape);
  }
  // A panel's preview over them, dimmed (mitcad#1097).
  for (const MeasurementDisplay& m : m_viewer->measurementPreview()) {
    shapes.push_back(layoutMeasurement(m, project));
  }
  QPainter painter(this);
  // What lies behind, while the camera is where it was taken.
  const bool current = !m_backdrop.image.isNull() && m_backdropKey == m_drawnKey;
  paintMeasurements(painter, shapes, m_viewer->sketchHaloColor(), measurementFont(font()), 1.0,
                    current ? &m_backdrop : nullptr);
}

} // namespace mitcad
