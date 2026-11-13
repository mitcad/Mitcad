// SPDX-License-Identifier: MIT
#include "view/DatumHandles.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <utility>

#include <QMouseEvent>
#include <QPainter>
#include <QPolygonF>
#include <QStringList>
#include <QtLogging>

#include <gp_Ax3.hxx>
#include <gp_Vec.hxx>

#include "OcctViewer.hpp"
#include "framework/Theme.hpp"

namespace mitcad {
namespace {

constexpr double kKnob = 9.0;  // a handle's side (a plane's) or diameter (an axis's), pixels
constexpr double kReach = 8.0; // how far from its middle a press takes a handle, pixels
constexpr double kDragStart = 3.0; // pixels the pointer moves before a press drags

bool same(const gp_Pnt& a, const gp_Pnt& b) { return a.SquareDistance(b) <= 1e-18; }
bool same(const gp_Dir& a, const gp_Dir& b) { return a.IsEqual(b, 1e-12); }

} // namespace

bool DatumHandles::Target::operator==(const Target& other) const {
  return uid == other.uid && plane == other.plane && same(centre, other.centre) && same(xAxis, other.xAxis) &&
         same(yAxis, other.yAxis) && size == other.size;
}

DatumHandles::DatumHandles(OcctViewer& viewer) : QWidget(&viewer), m_viewer(viewer) {
  setObjectName(QStringLiteral("datumHandles"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  resize(viewer.size());
  hide();
  m_filter.install(&viewer);
  connect(&viewer, &OcctViewer::viewChanged, this, [this] {
    if (m_target) {
      update();
      publish();
    }
  });
  // A card that comes over a handle or goes (the selection card): which
  // handles take a press there.
  connect(
      &viewer, &OcctViewer::coveredChanged, this,
      [this] {
        if (m_target) {
          publish();
        }
      },
      Qt::QueuedConnection);
}

// The view's handle areas need no clearing: they hold only while their owner
// exists (OcctViewer::setHandleAreas), and at the window's close the view is
// already gone when its children are deleted.
DatumHandles::~DatumHandles() = default;

void DatumHandles::setTarget(const std::optional<Target>& target) {
  if (m_drag >= 0 || m_pressed >= 0) {
    return; // the drag keeps its target until the release
  }
  if (target.has_value() == m_target.has_value() && (!target || *target == *m_target)) {
    return;
  }
  m_target = target;
  m_hover = -1;
  if (!m_target) {
    hide();
    m_viewer.clearHandleAreas(this);
    m_loggedPlaces.clear();
    return;
  }
  resize(m_viewer.size());
  show();
  raise();
  update();
  publish();
}

std::vector<gp_Pnt> DatumHandles::handlePoints(double size) const {
  std::vector<gp_Pnt> points;
  if (!m_target) {
    return points;
  }
  const double half = size / 2.0;
  const gp_Vec x = gp_Vec(m_target->xAxis) * half;
  if (!m_target->plane) {
    points.push_back(m_target->centre.Translated(-x));
    points.push_back(m_target->centre.Translated(x));
    return points;
  }
  const gp_Vec y = gp_Vec(m_target->yAxis) * half;
  // Around the square: (+x, +y), (-x, +y), (-x, -y), (+x, -y).
  points.push_back(m_target->centre.Translated(x + y));
  points.push_back(m_target->centre.Translated(y - x));
  points.push_back(m_target->centre.Translated(-x - y));
  points.push_back(m_target->centre.Translated(x - y));
  return points;
}

int DatumHandles::handleAt(const QPointF& position) const {
  if (!m_target || isHidden()) {
    return -1;
  }
  const std::vector<gp_Pnt> points = handlePoints(m_target->size);
  int best = -1;
  double bestDistance = kReach;
  for (std::size_t i = 0; i < points.size(); ++i) {
    const QPointF at = m_viewer.toWidgetF(points[i]);
    const double distance = std::hypot(at.x() - position.x(), at.y() - position.y());
    if (distance <= bestDistance) {
      best = static_cast<int>(i);
      bestDistance = distance;
    }
  }
  return best;
}

std::optional<double> DatumHandles::sizeAt(int index, const QPointF& position) const {
  if (!m_target || index < 0) {
    return std::nullopt;
  }
  double size = 0.0;
  if (m_target->plane) {
    // Where the pointer's ray meets the plane: the square, centred on the
    // plane's origin, reaches it with its farther side.
    const gp_Ax3 frame(m_target->centre, m_target->xAxis.Crossed(m_target->yAxis), m_target->xAxis);
    const std::optional<gp_Pnt> at = m_viewer.planePoint(position, frame);
    if (!at) {
      return std::nullopt;
    }
    const gp_Vec offset(m_target->centre, *at);
    size = 2.0 * std::max(std::abs(offset.Dot(gp_Vec(m_target->xAxis))), std::abs(offset.Dot(gp_Vec(m_target->yAxis))));
  } else {
    // The axis's point nearest to the pointer's ray: the line, centred on
    // the axis's origin, reaches it.
    const auto ray = m_viewer.pointerRay(position);
    if (!ray) {
      return std::nullopt;
    }
    const gp_Vec along(m_target->xAxis);
    const gp_Vec look(ray->second);
    const gp_Vec apart(ray->first, m_target->centre);
    const double cosine = along.Dot(look);
    const double denominator = 1.0 - cosine * cosine;
    if (denominator < 1e-6) {
      return std::nullopt; // looking along the axis
    }
    const double t = (cosine * look.Dot(apart) - along.Dot(apart)) / denominator;
    size = 2.0 * std::abs(t);
  }
  // Rounded to a step that fits the zoom: a power of ten no finer than
  // about two pixels; never less than one step.
  const double perPixel = m_viewer.modelPerPixel(m_target->centre);
  const double step = std::pow(10.0, std::ceil(std::log10(std::max(perPixel * 2.0, 1e-6))));
  return std::max(step, std::round(size / step) * step);
}

void DatumHandles::publish() {
  if (!m_target || isHidden()) {
    m_viewer.clearHandleAreas(this);
    return;
  }
  const std::vector<gp_Pnt> points = handlePoints(m_target->size);
  std::vector<OcctViewer::HandleArea> areas;
  QStringList lines;
  const QString kind = m_target->plane ? QStringLiteral("corner") : QStringLiteral("end");
  // A handle outside the view or under a card over it (the selection card)
  // takes no press there: it is neither offered nor logged. The view's
  // covered places without these handles' own areas.
  m_viewer.clearHandleAreas(this);
  const QRectF inside(m_viewer.rect());
  for (std::size_t i = 0; i < points.size(); ++i) {
    const QPointF at = m_viewer.toWidgetF(points[i]);
    if (!inside.contains(at) || m_viewer.isCovered(at)) {
      continue;
    }
    OcctViewer::HandleArea area;
    area.line.append(at);
    area.reach = kReach;
    areas.push_back(area);
    const QPoint place = m_viewer.mapTo(m_viewer.window(), at.toPoint());
    lines << QStringLiteral("Datum handle %1 %2 %3 at %4,%5")
                 .arg(m_target->uid, kind)
                 .arg(static_cast<int>(i) + 1)
                 .arg(place.x())
                 .arg(place.y());
  }
  m_viewer.setHandleAreas(this, std::move(areas));
  const QString text = lines.join(QLatin1Char('\n'));
  if (text == m_loggedPlaces) {
    return;
  }
  m_loggedPlaces = text;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

void DatumHandles::paintEvent(QPaintEvent*) {
  if (!m_target) {
    return;
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QColor handle = viewColor(ColorRole::Handle, false);
  const QColor active = viewColor(ColorRole::Select, false);
  const QColor ink = viewColor(ColorRole::ViewInk, false);
  // While a drag lasts, the new outline: the square or the line at the
  // size the release gives.
  if (m_drag >= 0) {
    const std::vector<gp_Pnt> outline = handlePoints(m_dragSize);
    QPolygonF shown;
    for (const gp_Pnt& point : outline) {
      shown << m_viewer.toWidgetF(point);
    }
    QPen pen(active, 1.5, Qt::DashLine);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    if (m_target->plane) {
      painter.drawPolygon(shown);
    } else if (shown.size() == 2) {
      painter.drawLine(shown.at(0), shown.at(1));
    }
  }
  const std::vector<gp_Pnt> points = handlePoints(m_drag >= 0 ? m_dragSize : m_target->size);
  for (std::size_t i = 0; i < points.size(); ++i) {
    const QPointF at = m_viewer.toWidgetF(points[i]);
    const bool lit = static_cast<int>(i) == m_hover || static_cast<int>(i) == m_drag;
    painter.setPen(QPen(ink, 1.0));
    painter.setBrush(lit ? active : handle);
    const QRectF knob(at.x() - kKnob / 2.0, at.y() - kKnob / 2.0, kKnob, kKnob);
    if (m_target->plane) {
      painter.drawRect(knob);
    } else {
      painter.drawEllipse(knob);
    }
  }
}

bool DatumHandles::eventFilter(QObject* watched, QEvent* event) {
  if (watched != &m_viewer) {
    return QWidget::eventFilter(watched, event);
  }
  if (event->type() == QEvent::Resize) {
    resize(m_viewer.size());
    return false;
  }
  if (isHidden() || !m_target) {
    return false;
  }
  // An exception must not leave a Qt event handler: a handle that cannot
  // be worked out (degenerate geometry) takes no events.
  try {
    return viewerEvent(event);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Datum handles:" << e.what();
    return false;
  }
}

bool DatumHandles::viewerEvent(QEvent* event) {
  switch (event->type()) {
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    const int at = handleAt(e->position());
    if (at < 0) {
      return false;
    }
    // A drag once the mouse moves; a click stays with the handle.
    m_pressed = at;
    m_pressAt = e->position();
    return true;
  }
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (m_pressed >= 0 && m_drag < 0) {
      const QPointF moved = e->position() - m_pressAt;
      if (std::hypot(moved.x(), moved.y()) < kDragStart) {
        return true;
      }
      m_drag = m_pressed;
      m_dragSize = m_target->size;
    }
    if (m_drag >= 0) {
      if (const std::optional<double> size = sizeAt(m_drag, e->position())) {
        if (*size != m_dragSize) {
          m_dragSize = *size;
          update();
        }
      }
      return true;
    }
    const int hover = handleAt(e->position());
    if (hover != m_hover) {
      m_hover = hover;
      update();
    }
    return false; // the view's hover goes on
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || (m_pressed < 0 && m_drag < 0)) {
      return false;
    }
    const bool dragged = m_drag >= 0;
    m_pressed = -1;
    m_drag = -1;
    update();
    if (dragged && m_target && m_dragSize != m_target->size) {
      const QString uid = m_target->uid;
      const double size = m_dragSize;
      qDebug().noquote() << QStringLiteral("Datum handle %1 resized to %2 mm").arg(uid).arg(size, 0, 'g', 10);
      emit resized(uid, size);
    }
    return true;
  }
  case QEvent::WindowBlocked:
    // A modal dialog took the input and with it the release: nothing set.
    m_pressed = -1;
    m_drag = -1;
    update();
    return false;
  default:
    return false;
  }
}

} // namespace mitcad
