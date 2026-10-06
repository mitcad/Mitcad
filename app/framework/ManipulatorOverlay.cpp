// SPDX-License-Identifier: MIT
#include "ManipulatorOverlay.hpp"

#include <algorithm>
#include <cmath>
#include <limits>

#include <QCoreApplication>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QStringList>
#include <QtLogging>

#include "../OcctViewer.hpp"

namespace mitcad {
namespace {

const QColor kHandleColor(0xe0, 0x7b, 0x1a);
const QColor kActiveColor(0x1f, 0x7a, 0xff);
constexpr double kArrowPixels = 34.0;
constexpr double kRingPixels = 70.0;
constexpr double kGrabPixels = 9.0;
constexpr double kPi = 3.14159265358979323846;

QPointF unit(const QPointF& p) {
  const double n = std::hypot(p.x(), p.y());
  return n > 1e-9 ? QPointF(p.x() / n, p.y() / n) : QPointF(0, -1);
}

double distanceToSegment(const QPointF& p, const QPointF& a, const QPointF& b) {
  const QPointF ab = b - a;
  const double length2 = QPointF::dotProduct(ab, ab);
  const double t =
      length2 > 1e-12 ? std::clamp(QPointF::dotProduct(p - a, ab) / length2, 0.0, 1.0) : 0.0;
  const QPointF d = a + ab * t - p;
  return std::hypot(d.x(), d.y());
}

// A step that is a few pixels long: 1, 2 or 5 times a power of ten.
double niceStep(double size) {
  const double power = std::pow(10.0, std::floor(std::log10(std::max(size, 1e-6))));
  for (const double factor : {1.0, 2.0, 5.0, 10.0}) {
    if (factor * power >= size) {
      return factor * power;
    }
  }
  return 10.0 * power;
}

} // namespace

ManipulatorOverlay::ManipulatorOverlay(OcctViewer& viewer, const QString& command)
    : QWidget(&viewer), m_viewer(viewer), m_command(command) {
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  resize(viewer.size());
  show();
  viewer.installEventFilter(this);
  connect(&viewer, &OcctViewer::viewChanged, this, [this] {
    update();
    logPlaces();
  });
}

ManipulatorOverlay::~ManipulatorOverlay() { m_viewer.removeEventFilter(this); }

void ManipulatorOverlay::setHandles(std::vector<Handle> handles) {
  if (m_drag >= 0) {
    // The dragged handle stays the one being dragged.
    const QString key = m_handles[static_cast<std::size_t>(m_drag)].key;
    m_drag = -1;
    for (std::size_t i = 0; i < handles.size(); ++i) {
      if (handles[i].key == key) {
        m_drag = static_cast<int>(i);
      }
    }
  }
  m_handles = std::move(handles);
  m_hover = -1;
  update();
  logPlaces();
}

double ManipulatorOverlay::pixelSize(const gp_Pnt& at) const {
  const gp_Dir up = m_viewer.viewUp();
  const QPointF a = m_viewer.toWidgetF(at);
  const QPointF b = m_viewer.toWidgetF(at.Translated(gp_Vec(up)));
  const double pixels = std::hypot(b.x() - a.x(), b.y() - a.y());
  return pixels > 1e-9 ? 1.0 / pixels : 1.0;
}

ManipulatorOverlay::Drawn ManipulatorOverlay::draw(const Handle& handle) const {
  Drawn drawn;
  const Manipulator& m = handle.manipulator;
  if (!std::isfinite(handle.value)) {
    return drawn;
  }
  if (m.kind == Manipulator::Kind::Toggle) {
    drawn.origin = m_viewer.toWidgetF(m.origin);
    drawn.handle = drawn.origin;
    drawn.tip = drawn.origin;
    drawn.valid = true;
    return drawn;
  }
  if (m.kind == Manipulator::Kind::Arrow) {
    const gp_Pnt end = m.origin.Translated(gp_Vec(m.direction) * ((handle.value - m.base) * m.factor));
    drawn.origin = m_viewer.toWidgetF(m.origin);
    drawn.handle = m_viewer.toWidgetF(end);
    const QPointF along = m_viewer.toWidgetF(end.Translated(gp_Vec(m.direction))) - drawn.handle;
    drawn.tip = drawn.handle + unit(along) * kArrowPixels;
    drawn.valid = true;
    return drawn;
  }
  const gp_Vec cross = gp_Vec(m.direction).Crossed(gp_Vec(m.reference));
  if (cross.Magnitude() < 1e-9) {
    return drawn; // a ring without a reference across its axis
  }
  const double radius = kRingPixels * pixelSize(m.origin);
  const gp_Dir side(cross);
  const auto at = [&](double angle) {
    return m_viewer.toWidgetF(m.origin.Translated(
        (gp_Vec(m.reference) * std::cos(angle) + gp_Vec(side) * std::sin(angle)) * radius));
  };
  for (int i = 0; i <= 64; ++i) {
    drawn.ring << at(2.0 * kPi * i / 64.0);
  }
  drawn.origin = m_viewer.toWidgetF(m.origin);
  drawn.handle = at(handle.value);
  drawn.tip = drawn.handle;
  drawn.valid = true;
  return drawn;
}

int ManipulatorOverlay::handleAt(const QPointF& position) const {
  int best = -1;
  double nearest = kGrabPixels;
  for (std::size_t i = 0; i < m_handles.size(); ++i) {
    const Drawn drawn = draw(m_handles[i]);
    if (!drawn.valid) {
      continue;
    }
    double distance = 0.0;
    if (m_handles[i].manipulator.kind == Manipulator::Kind::Toggle) {
      const QPointF d = position - drawn.handle;
      distance = std::hypot(d.x(), d.y()) - 4.0;
    } else if (m_handles[i].manipulator.kind == Manipulator::Kind::Arrow) {
      distance = distanceToSegment(position, drawn.handle, drawn.tip);
    } else {
      const QPointF d = position - drawn.handle;
      distance = std::hypot(d.x(), d.y()) - 3.0; // the handle's knob is the easiest to grab
      for (int k = 0; k + 1 < drawn.ring.size(); ++k) {
        distance = std::min(distance, distanceToSegment(position, drawn.ring[k], drawn.ring[k + 1]));
      }
    }
    if (distance < nearest) {
      nearest = distance;
      best = static_cast<int>(i);
    }
  }
  return best;
}

double ManipulatorOverlay::valueAt(const Handle& handle, const QPointF& position) {
  const Manipulator& m = handle.manipulator;
  if (m.kind == Manipulator::Kind::Arrow) {
    // Along the arrow's line on the screen: a millimetre is this long.
    const QPointF a = m_viewer.toWidgetF(m.origin);
    const QPointF b = m_viewer.toWidgetF(m.origin.Translated(gp_Vec(m.direction)));
    const QPointF ab = b - a;
    const double length2 = QPointF::dotProduct(ab, ab);
    if (length2 < 1e-4) {
      return handle.value; // looking along the arrow
    }
    // Where the arrow was grabbed, not its base, follows the cursor.
    const double factor = std::abs(m.factor) > 1e-12 ? m.factor : 1.0;
    const double raw = m.base + QPointF::dotProduct(position - m_grab - a, ab) / length2 / factor;
    const double step = niceStep(3.0 * pixelSize(m.origin) / std::abs(factor));
    return std::round(raw / step) * step;
  }
  const double angle = ringAngle(handle, position);
  if (!std::isfinite(angle)) {
    return handle.value; // the ring is edge-on
  }
  // Continuous from where the drag began, in whole degrees.
  double turn = angle - m_angleLast;
  while (turn > kPi) {
    turn -= 2.0 * kPi;
  }
  while (turn < -kPi) {
    turn += 2.0 * kPi;
  }
  m_angleLast = angle;
  m_turned += turn;
  const double degree = kPi / 180.0;
  return std::round((m_dragStart + m_turned) / degree) * degree;
}

double ManipulatorOverlay::ringAngle(const Handle& handle, const QPointF& position) const {
  // In the ring's plane as the screen shows it: position = c + u r + v s.
  const Manipulator& m = handle.manipulator;
  const gp_Vec cross = gp_Vec(m.direction).Crossed(gp_Vec(m.reference));
  if (m.kind != Manipulator::Kind::Ring || cross.Magnitude() < 1e-9) {
    return std::numeric_limits<double>::quiet_NaN();
  }
  const double radius = kRingPixels * pixelSize(m.origin);
  const gp_Dir side(cross);
  const QPointF c = m_viewer.toWidgetF(m.origin);
  const QPointF r = m_viewer.toWidgetF(m.origin.Translated(gp_Vec(m.reference) * radius)) - c;
  const QPointF s = m_viewer.toWidgetF(m.origin.Translated(gp_Vec(side) * radius)) - c;
  const double det = r.x() * s.y() - r.y() * s.x();
  if (std::abs(det) < 1e-3) {
    return std::numeric_limits<double>::quiet_NaN();
  }
  const QPointF p = position - c;
  const double u = (p.x() * s.y() - p.y() * s.x()) / det;
  const double v = (r.x() * p.y() - r.y() * p.x()) / det;
  return std::atan2(v, u);
}

bool ManipulatorOverlay::eventFilter(QObject* watched, QEvent* event) {
  if (watched != &m_viewer || m_forwarding) {
    return QWidget::eventFilter(watched, event);
  }
  // An exception must not leave a Qt event handler: a handle that cannot
  // be worked out (degenerate geometry) just takes no events.
  try {
    return viewerEvent(event);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Manipulator:" << e.what();
    m_drag = -1;
    m_pending = -1;
    return false;
  }
}

bool ManipulatorOverlay::viewerEvent(QEvent* event) {
  switch (event->type()) {
  case QEvent::Resize:
    resize(m_viewer.size());
    return false;
  case QEvent::WindowBlocked:
    // A modal dialog took the input (a long computation's progress, P7),
    // and with it the release: a drag ends at the value it has.
    m_pending = -1;
    if (m_drag >= 0) {
      const QString key = m_handles[static_cast<std::size_t>(m_drag)].key;
      const double value = m_handles[static_cast<std::size_t>(m_drag)].value;
      m_drag = -1;
      update();
      qDebug().noquote() << QStringLiteral("Manipulator %1 %2 dragged to %3 (ended by a dialog)")
                                .arg(m_command, key)
                                .arg(value, 0, 'g', 10);
      emit dragFinished(key);
    }
    return false;
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    const int at = handleAt(e->position());
    if (at < 0) {
      return false;
    }
    // A drag once the mouse moves; a click goes on to the view as a pick.
    m_pending = at;
    m_pressAt = e->position();
    m_pressModifiers = e->modifiers();
    return true;
  }
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (m_pending >= 0 && m_drag < 0) {
      const QPointF moved = e->position() - m_pressAt;
      if (std::hypot(moved.x(), moved.y()) < 3.0 ||
          m_handles[static_cast<std::size_t>(m_pending)].manipulator.kind == Manipulator::Kind::Toggle) {
        return true; // a toggle is clicked, not dragged
      }
      m_drag = m_pending;
      m_pending = -1;
      const Handle& handle = m_handles[static_cast<std::size_t>(m_drag)];
      m_dragStart = handle.value;
      // An arrow keeps the point grabbed under the cursor; a ring turns by
      // as much as the cursor turns about it from where it was pressed.
      m_grab = m_pressAt - draw(handle).handle;
      m_angleLast = ringAngle(handle, m_pressAt);
      m_turned = 0.0;
    }
    if (m_drag >= 0) {
      Handle& handle = m_handles[static_cast<std::size_t>(m_drag)];
      const double value = valueAt(handle, e->position());
      if (value != handle.value) {
        handle.value = value;
        update();
        emit dragged(handle.key, value);
      }
      return true;
    }
    const int hover = handleAt(e->position());
    if (hover != m_hover) {
      m_hover = hover;
      update();
    }
    return false;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (m_pending >= 0 &&
        m_handles[static_cast<std::size_t>(m_pending)].manipulator.kind == Manipulator::Kind::Toggle) {
      // A pattern's instance turned on or off (P9).
      const Handle& handle = m_handles[static_cast<std::size_t>(m_pending)];
      m_pending = -1;
      qDebug().noquote() << QStringLiteral("Manipulator %1 %2.%3 toggled")
                                .arg(m_command, handle.key)
                                .arg(handle.manipulator.element);
      emit toggled(handle.key, handle.manipulator.element);
      return true;
    }
    if (m_pending >= 0) {
      // Not dragged: the view gets the click.
      m_pending = -1;
      m_forwarding = true;
      QMouseEvent press(QEvent::MouseButtonPress, m_pressAt, e->globalPosition(), Qt::LeftButton,
                        Qt::LeftButton, m_pressModifiers);
      QCoreApplication::sendEvent(&m_viewer, &press);
      QMouseEvent release(QEvent::MouseButtonRelease, e->position(), e->globalPosition(), Qt::LeftButton,
                          Qt::NoButton, e->modifiers());
      QCoreApplication::sendEvent(&m_viewer, &release);
      m_forwarding = false;
      return true;
    }
    if (m_drag < 0) {
      return false;
    }
    const QString key = m_handles[static_cast<std::size_t>(m_drag)].key;
    const double value = m_handles[static_cast<std::size_t>(m_drag)].value;
    m_drag = -1;
    update();
    qDebug().noquote() << QStringLiteral("Manipulator %1 %2 dragged to %3")
                              .arg(m_command, key)
                              .arg(value, 0, 'g', 10);
    emit dragFinished(key);
    logPlaces();
    return true;
  }
  default:
    return false;
  }
}

void ManipulatorOverlay::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  for (std::size_t i = 0; i < m_handles.size(); ++i) {
    Drawn drawn;
    try {
      drawn = draw(m_handles[i]);
    } catch (const std::exception&) {
      continue; // degenerate: not drawn
    }
    if (!drawn.valid) {
      continue;
    }
    const bool active = static_cast<int>(i) == m_drag || static_cast<int>(i) == m_hover;
    const QColor color = active ? kActiveColor : kHandleColor;
    if (m_handles[i].manipulator.kind == Manipulator::Kind::Toggle) {
      // A pattern's instance (P9): filled while it is made, hollow when
      // suppressed.
      painter.setPen(QPen(active ? kActiveColor : QColor(0x2b, 0x3a, 0x4a), 1.5));
      painter.setBrush(m_handles[i].manipulator.on ? color : QColor(Qt::white));
      painter.drawEllipse(drawn.handle, 6.0, 6.0);
    } else if (m_handles[i].manipulator.kind == Manipulator::Kind::Arrow) {
      painter.setPen(QPen(color, 1.0, Qt::DashLine));
      painter.drawLine(drawn.origin, drawn.handle);
      painter.setPen(QPen(color, 3.0, Qt::SolidLine, Qt::RoundCap));
      painter.drawLine(drawn.handle, drawn.tip);
      const QPointF along = unit(drawn.tip - drawn.handle);
      const QPointF across(-along.y(), along.x());
      QPolygonF head;
      head << drawn.tip + along * 10.0 << drawn.tip + across * 6.0 << drawn.tip - across * 6.0;
      painter.setPen(QPen(color, 1.0));
      painter.setBrush(color);
      painter.drawPolygon(head);
      painter.drawEllipse(drawn.handle, 3.5, 3.5);
    } else {
      painter.setBrush(Qt::NoBrush);
      painter.setPen(QPen(color, active ? 2.5 : 1.8));
      painter.drawPolyline(drawn.ring);
      painter.setPen(QPen(color, 1.0, Qt::DashLine));
      painter.drawLine(drawn.origin, drawn.handle);
      painter.setPen(QPen(color.darker(130), 1.0));
      painter.setBrush(color);
      painter.drawEllipse(drawn.handle, 6.0, 6.0);
    }
  }
}

void ManipulatorOverlay::logPlaces() {
  if (!isVisible()) {
    return;
  }
  QStringList lines;
  for (const Handle& handle : m_handles) {
    Drawn drawn;
    try {
      drawn = draw(handle);
    } catch (const std::exception&) {
      continue;
    }
    if (!drawn.valid) {
      continue;
    }
    // Where a drag grabs it: the arrow's middle, the ring's knob.
    const QPointF grab = handle.manipulator.kind == Manipulator::Kind::Arrow
                             ? (drawn.handle + drawn.tip) / 2.0
                             : drawn.handle;
    const QPoint at = mapTo(window(), grab.toPoint());
    const QString key = handle.manipulator.kind == Manipulator::Kind::Toggle
                            ? QStringLiteral("%1.%2%3")
                                  .arg(handle.key)
                                  .arg(handle.manipulator.element)
                                  .arg(handle.manipulator.on ? QString() : QStringLiteral(" (off)"))
                            : handle.key;
    lines << QStringLiteral("Manipulator %1 %2 at %3,%4").arg(m_command, key).arg(at.x()).arg(at.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : lines) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
