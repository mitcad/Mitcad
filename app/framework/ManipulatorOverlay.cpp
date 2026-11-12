// SPDX-License-Identifier: MIT
#include "ManipulatorOverlay.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>
#include <utility>
#include <vector>

#include <QHelpEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QStringList>
#include <QToolTip>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "Diagnostics.hpp"
#include "HandleStep.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

// The handles' orange and the selection's blue for the one dragged or
// hovered; the ink of the boxes over the view (mitcad#1041). An
// axis-colored handle (Free Move's) dragged or hovered takes HandleActive
// instead, none of the axis colors (mitcad#1707: the selection's blue is
// close to the Z axis').
const QColor kHandleColor = viewColor(ColorRole::Handle, false);
const QColor kActiveColor = viewColor(ColorRole::Select, false);
const QColor kInk = viewColor(ColorRole::ViewInk, false);
// The dark halo under an arrow's or a ring's strokes and round its knob
// and head (mitcad#1581): the handle stays visible on a light and a dark
// background and over the highlighted bodies it moves.
const QColor kHalo = [] {
  QColor halo = viewColor(ColorRole::ViewInk, false);
  halo.setAlpha(175);
  return halo;
}();
// An arrow's or a ring's sizes in the view's pixels (Qt's
// device-independent ones: scaled by the screen's device pixel ratio): its
// length or radius, how far from it a press takes it, a ring's knob, and
// its strokes. Free Move's handles (Manipulator::large, mitcad#1581) are
// about 1.5 times larger, and their arrows 1.3 times as long as their
// rings' radius, so that each arrow's head lies outside the rings, where
// no ring's line covers it in a front or a top view (mitcad#1707); the
// other commands' keep the sizes they had, so that their handles take no
// click they did not take before (a BUILD side click flush with a part's
// end, beside a Profile's tilt ring).
struct HandleSize {
  double arrow;
  double ring;
  double grab;
  double knob;  // a ring's knob's radius
  double shaft; // an arrow's shaft
  double line;  // a ring's line, one more hovered or dragged
  double scale; // an arrow's head and knob
};
HandleSize sizeOf(const Manipulator& m) {
  return m.large ? HandleSize{130.0, 100.0, 12.0, 8.0, 4.5, 2.6, 1.5} : HandleSize{34.0, 70.0, 9.0, 6.0, 3.0, 1.8, 1.0};
}
constexpr double kToggleReach = 13.0; // a pattern's instance (P9)
constexpr double kHaloExtra = 2.5;    // the halo's width beyond a stroke
constexpr double kPi = 3.14159265358979323846;
// Seen edge-on (mitcad#1707): within 15 degrees of the view direction.
const double kEdgeOnSine = std::sin(15.0 * kPi / 180.0);
const double kEdgeOnCosine = std::cos(15.0 * kPi / 180.0);
constexpr double kFaintOpacity = 0.4; // a handle seen edge-on, not hovered

// An arrow's or a ring's color while neither hovered nor dragged: its
// role's (Manipulator::color; Free Move's by its axis, mitcad#1581).
QColor handleColor(const Manipulator& manipulator) {
  return manipulator.color == ColorRole::Handle ? kHandleColor : viewColor(manipulator.color, false);
}

// The role of a handle hovered or dragged: the selection's blue, or for an
// axis-colored one (Free Move's) HandleActive, none of the axis colors
// (mitcad#1707).
ColorRole activeRole(const Manipulator& manipulator) {
  const bool axis = manipulator.color == ColorRole::AxisX || manipulator.color == ColorRole::AxisY ||
                    manipulator.color == ColorRole::AxisZ;
  return axis ? ColorRole::HandleActive : ColorRole::Select;
}

QColor activeColor(const Manipulator& manipulator) {
  const ColorRole role = activeRole(manipulator);
  return role == ColorRole::Select ? kActiveColor : viewColor(role, false);
}

// A stroke drawn over its halo (mitcad#1581).
template <typename Draw> void haloed(QPainter& painter, const QPen& pen, Draw draw) {
  QPen halo(pen);
  halo.setColor(kHalo);
  halo.setStyle(Qt::SolidLine);
  halo.setWidthF(pen.widthF() + kHaloExtra);
  painter.setPen(halo);
  draw();
  painter.setPen(pen);
  draw();
}

// A coordinate of the design for the log: 6 decimals, no "-0".
QString worldNumber(double value) { return QString::number(std::abs(value) < 5e-7 ? 0.0 : value, 'f', 6); }

QPointF unit(const QPointF& p) {
  const double n = std::hypot(p.x(), p.y());
  return n > 1e-9 ? QPointF(p.x() / n, p.y() / n) : QPointF(0, -1);
}

// Whether a handle's points are on the screen at all: a model point with
// no number (NaN) draws nothing.
bool onScreen(const QPointF& p) { return std::isfinite(p.x()) && std::isfinite(p.y()); }

double distanceToSegment(const QPointF& p, const QPointF& a, const QPointF& b) {
  const QPointF ab = b - a;
  const double length2 = QPointF::dotProduct(ab, ab);
  const double t =
      length2 > 1e-12 ? std::clamp(QPointF::dotProduct(p - a, ab) / length2, 0.0, 1.0) : 0.0;
  const QPointF d = a + ab * t - p;
  return std::hypot(d.x(), d.y());
}

} // namespace

ManipulatorOverlay::ManipulatorOverlay(OcctViewer& viewer, const QString& command)
    : QWidget(&viewer), m_viewer(viewer), m_command(command) {
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  resize(viewer.size());
  show();
  m_viewerFilter.install(&viewer);
  connect(&viewer, &OcctViewer::viewChanged, this, [this] {
    update();
    publishBoxes();
    logPlaces();
  });
}

ManipulatorOverlay::~ManipulatorOverlay() = default;

void ManipulatorOverlay::layoutBoxes() {
  // Each value box where its handle puts it, then pushed off the boxes
  // placed before it and out of the panels floating over the view
  // (mitcad#656). A box of a value that is 0 shows only while its handle
  // or the box is under the mouse (Manipulator::boxAtZero).
  m_boxes.assign(m_handles.size(), QRectF());
  const std::vector<QRect> covered = m_viewer.coveredRects();
  const QRectF area = QRectF(rect()).adjusted(2, 2, -2, -2);
  std::vector<QRectF> placed;
  for (std::size_t i = 0; i < m_handles.size(); ++i) {
    const Handle& handle = m_handles[i];
    if (!handle.manipulator.valueBox) {
      continue;
    }
    const int index = static_cast<int>(i);
    if (!handle.manipulator.boxAtZero && std::abs(handle.value) < 1e-12 && index != m_hover &&
        index != m_drag && index != m_boxHover) {
      continue;
    }
    Handle shown = handle;
    if (index == m_drag && m_visualValue) {
      shown.value = *m_visualValue;
    }
    QRectF box;
    try {
      const Drawn drawn = draw(shown);
      if (!drawn.valid) {
        continue;
      }
      box = drawn.box;
    } catch (const std::exception&) {
      continue; // degenerate: not drawn
    }
    if (box.isEmpty()) {
      continue;
    }
    for (int round = 0; round < 12; ++round) {
      bool moved = false;
      for (const QRectF& other : placed) {
        const QRectF overlap = box.intersected(other.adjusted(-3, -3, 3, 3));
        if (overlap.isEmpty()) {
          continue;
        }
        // Apart along the smaller overlap, away from the other's middle.
        if (overlap.height() <= overlap.width()) {
          box.translate(0, box.center().y() >= other.center().y() ? overlap.height() : -overlap.height());
        } else {
          box.translate(box.center().x() >= other.center().x() ? overlap.width() : -overlap.width(), 0);
        }
        moved = true;
      }
      for (const QRect& card : covered) {
        const QRectF panel = QRectF(card).adjusted(-4, -4, 4, 4);
        if (!panel.intersects(box)) {
          continue;
        }
        // Out by the shortest way that stays in the view.
        const double left = box.right() - panel.left();
        const double right = panel.right() - box.left();
        const double up = box.bottom() - panel.top();
        const double down = panel.bottom() - box.top();
        std::vector<std::pair<double, QPointF>> ways{{left, QPointF(-left, 0)}, {right, QPointF(right, 0)},
                                                     {up, QPointF(0, -up)}, {down, QPointF(0, down)}};
        std::sort(ways.begin(), ways.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
        for (const auto& way : ways) {
          if (area.contains(box.translated(way.second))) {
            box.translate(way.second);
            break;
          }
        }
        moved = true;
      }
      if (!moved) {
        break;
      }
    }
    placed.push_back(box);
    m_boxes[i] = box;
  }
}

void ManipulatorOverlay::publishBoxes() {
  // Where a click edits a value: the view offers no picks there.
  layoutBoxes();
  std::vector<QRect> boxes;
  for (const QRectF& box : m_boxes) {
    if (!box.isEmpty() && !isHidden()) { // none while hidden with the preview (mitcad#1421)
      boxes.push_back(box.toAlignedRect());
    }
  }
  m_viewer.setHandleBoxes(boxes);
  // Nor where a press stays with a handle (mitcad#1105), as handleAt()
  // measures: within its reach (sizeOf) of an arrow, of a ring or of a
  // knob's edge. None once hidden (the command ended; deleted later).
  std::vector<OcctViewer::HandleArea> areas;
  for (const Handle& handle : m_handles) {
    if (isHidden()) {
      break;
    }
    if (handle.manipulator.kind == Manipulator::Kind::Marker) {
      continue;
    }
    if (handle.manipulator.kind == Manipulator::Kind::Arrow && seenEdgeOn(handle.manipulator)) {
      continue; // a large arrow seen end-on takes no press (mitcad#1707)
    }
    Drawn drawn;
    try {
      drawn = draw(handle);
    } catch (const std::exception&) {
      continue; // degenerate: not drawn, takes nothing
    }
    if (!drawn.valid) {
      continue;
    }
    const HandleSize size = sizeOf(handle.manipulator);
    OcctViewer::HandleArea knob;
    knob.line.append(drawn.handle);
    if (handle.manipulator.kind == Manipulator::Kind::Toggle) {
      knob.reach = kToggleReach;
      areas.push_back(knob);
    } else if (handle.manipulator.kind == Manipulator::Kind::Arrow) {
      knob.line.append(drawn.tip); // the whole arrow
      knob.reach = size.grab;
      areas.push_back(knob);
    } else {
      knob.reach = size.grab + size.knob / 2.0; // a ring's knob
      areas.push_back(knob);
      if (drawn.ring.size() > 1) {
        OcctViewer::HandleArea ring;
        ring.line = drawn.ring;
        ring.reach = size.grab;
        areas.push_back(ring);
      }
    }
  }
  m_viewer.setHandleAreas(this, std::move(areas));
}

QStringList ManipulatorOverlay::boxKeys() const {
  QStringList keys;
  for (std::size_t i = 0; i < m_handles.size() && i < m_boxes.size(); ++i) {
    if (!m_boxes[i].isEmpty() || m_handles[i].manipulator.valueBox) {
      keys << m_handles[i].key;
    }
  }
  return keys;
}

std::optional<QPoint> ManipulatorOverlay::boxCentre(const QString& key) const {
  for (std::size_t i = 0; i < m_handles.size() && i < m_boxes.size(); ++i) {
    if (m_handles[i].key != key) {
      continue;
    }
    if (!m_boxes[i].isEmpty()) {
      return m_boxes[i].center().toPoint();
    }
    // A box hidden at 0: where its handle is.
    try {
      const Drawn drawn = draw(m_handles[i]);
      if (drawn.valid) {
        return drawn.handle.toPoint();
      }
    } catch (const std::exception&) {
    }
  }
  return std::nullopt;
}

void ManipulatorOverlay::setLines(std::vector<std::vector<gp_Pnt>> lines) {
  if (lines.size() != m_lines.size()) {
    // For UI tests: the command's own lines (a belt's pitch line) came or went.
    qDebug().noquote() << QStringLiteral("Lines %1: %2").arg(m_command).arg(static_cast<qlonglong>(lines.size()));
  }
  m_lines = std::move(lines);
  update();
}

void ManipulatorOverlay::setHandles(std::vector<Handle> handles) {
  const auto retained = [&](int index) {
    if (index < 0) { return -1; }
    const Handle& previous = m_handles[static_cast<std::size_t>(index)];
    for (std::size_t i = 0; i < handles.size(); ++i) {
      const Handle& next = handles[i];
      const Manipulator& a = previous.manipulator;
      const Manipulator& b = next.manipulator;
      if (next.key == previous.key && a.kind == b.kind && a.element == b.element &&
          a.origin.IsEqual(b.origin, 1e-9) && a.direction.IsEqual(b.direction, 1e-12) &&
          (a.kind != Manipulator::Kind::Ring || a.reference.IsEqual(b.reference, 1e-12)) &&
          (a.kind != Manipulator::Kind::Arrow || (a.base == b.base && a.factor == b.factor)) &&
          std::isfinite(next.value)) {
        return static_cast<int>(i);
      }
    }
    return -1;
  };
  // A normal form refresh retains the local pointer feedback. A changed
  // placement/kind, or a removed handle, invalidates its grab geometry.
  m_drag = retained(m_drag);
  m_pending = retained(m_pending);
  if (m_drag < 0) {
    m_visualValue.reset();
  }
  m_handles = std::move(handles);
  m_hover = -1;
  m_boxHover = -1;
  update();
  publishBoxes();
  logPlaces();
}

void ManipulatorOverlay::setHint(const QString& text) {
  if (text == m_hint) {
    return;
  }
  m_hint = text;
  update();
  qDebug().noquote() << (text.isEmpty() ? QStringLiteral("View hint %1 cleared").arg(m_command)
                                        : QStringLiteral("View hint %1: %2").arg(m_command, text));
}

double ManipulatorOverlay::pixelSize(const gp_Pnt& at) const {
  const gp_Dir up = m_viewer.viewUp();
  const QPointF a = m_viewer.toWidgetF(at);
  const QPointF b = m_viewer.toWidgetF(at.Translated(gp_Vec(up)));
  const double pixels = std::hypot(b.x() - a.x(), b.y() - a.y());
  return pixels > 1e-9 ? 1.0 / pixels : 1.0;
}

bool ManipulatorOverlay::seenEdgeOn(const Manipulator& m) const {
  // How far the handle's axis (a ring's) or direction (an arrow's) lies
  // along the view direction: a ring whose axis is nearly across it lies
  // nearly in it, an arrow nearly along it points at the viewer.
  const double along = std::abs(gp_Vec(m.direction).Dot(gp_Vec(m_viewer.viewDirection())));
  if (m.kind == Manipulator::Kind::Ring) {
    return along < kEdgeOnSine;
  }
  return m.kind == Manipulator::Kind::Arrow && m.large && along > kEdgeOnCosine;
}

ManipulatorOverlay::Drawn ManipulatorOverlay::draw(const Handle& handle) const {
  Drawn drawn;
  const Manipulator& m = handle.manipulator;
  if (!std::isfinite(handle.value)) {
    return drawn;
  }
  // The value box (mitcad#656) centred at a point, as large as its text.
  const auto around = [this, &handle](const QPointF& centre) {
    const QRectF text = fontMetrics().boundingRect(handle.text);
    const QSizeF size(text.width() + 12.0, text.height() + 6.0);
    return QRectF(centre - QPointF(size.width() / 2.0, size.height() / 2.0), size);
  };
  const bool boxed = m.valueBox && !handle.text.isEmpty();
  if (m.kind == Manipulator::Kind::Toggle || m.kind == Manipulator::Kind::Marker) {
    drawn.origin = m_viewer.toWidgetF(m.origin);
    drawn.handle = drawn.origin;
    drawn.tip = drawn.origin;
    drawn.valid = onScreen(drawn.origin) && onScreen(drawn.handle) && onScreen(drawn.tip);
    return drawn;
  }
  if (m.kind == Manipulator::Kind::Arrow) {
    const gp_Pnt end = m.origin.Translated(gp_Vec(m.direction) * ((handle.value - m.base) * m.factor));
    drawn.origin = m_viewer.toWidgetF(m.origin);
    drawn.handle = m_viewer.toWidgetF(end);
    const QPointF along = m_viewer.toWidgetF(end.Translated(gp_Vec(m.direction))) - drawn.handle;
    drawn.tip = drawn.handle + unit(along) * sizeOf(m).arrow;
    if (boxed) {
      // A dimension's value on its line: at its middle, or, where the
      // dimension is too short on the screen to hold it, on the line
      // beyond the reference, away from the point.
      const QPointF middle = (drawn.origin + drawn.handle) / 2.0;
      const QPointF span = drawn.handle - drawn.origin;
      const double length = std::hypot(span.x(), span.y());
      const QPointF line = length > 1.0 ? unit(span) : unit(along);
      const QRectF box = around(QPointF());
      const double half = std::hypot(box.width(), box.height()) / 2.0;
      drawn.box = m.boxAtTip ? around(drawn.tip + unit(along) * (half + 12.0))
                  : length > box.width() + 24.0 ? around(middle)
                                                : around(drawn.origin - line * (half + 10.0));
    }
    drawn.valid = onScreen(drawn.origin) && onScreen(drawn.handle) && onScreen(drawn.tip);
    return drawn;
  }
  const gp_Vec cross = gp_Vec(m.direction).Crossed(gp_Vec(m.reference));
  if (cross.Magnitude() < 1e-9) {
    return drawn; // a ring without a reference across its axis
  }
  const double radius = sizeOf(m).ring * pixelSize(m.origin);
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
  if (boxed) {
    // An angle's dimension (mitcad#656): its zero line along the
    // reference it is measured from, out past the ring, and the arc from
    // there to the knob; the value beyond the arc's middle.
    drawn.zero = m_viewer.toWidgetF(m.origin.Translated(gp_Vec(m.reference) * radius * 1.35));
    const int steps = std::max(2, static_cast<int>(std::abs(handle.value) / (2.0 * kPi) * 64.0) + 2);
    for (int i = 0; i <= steps; ++i) {
      drawn.arc << at(handle.value * i / steps);
    }
    const QPointF arcMiddle = at(handle.value / 2.0);
    drawn.box = around(arcMiddle + unit(arcMiddle - drawn.origin) * 26.0);
  }
  drawn.valid = onScreen(drawn.origin) && onScreen(drawn.handle) && onScreen(drawn.tip);
  return drawn;
}

int ManipulatorOverlay::boxAt(const QPointF& position) const {
  for (std::size_t i = 0; i < m_boxes.size() && i < m_handles.size(); ++i) {
    if (!m_boxes[i].isEmpty() && m_boxes[i].adjusted(-2, -2, 2, 2).contains(position)) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

int ManipulatorOverlay::handleAt(const QPointF& position) const {
  // The handle the position is the most within the reach of (sizeOf: a
  // larger handle reaches further, mitcad#1581). Seen edge-on
  // (seenEdgeOn, mitcad#1707), a large arrow takes no press, and a ring
  // none within an arrow's reach: seen head-on, a ring about an axis in
  // the view's plane is drawn as a line over the arrows, which win.
  bool nearArrow = false;
  for (const Handle& handle : m_handles) {
    const Manipulator& m = handle.manipulator;
    if (m.kind != Manipulator::Kind::Arrow || seenEdgeOn(m)) {
      continue;
    }
    const Drawn drawn = draw(handle);
    if (drawn.valid && distanceToSegment(position, drawn.handle, drawn.tip) < sizeOf(m).grab) {
      nearArrow = true;
      break;
    }
  }
  int best = -1;
  double nearest = 0.0; // how far within its reach; none outside it
  for (std::size_t i = 0; i < m_handles.size(); ++i) {
    const Manipulator& m = m_handles[i].manipulator;
    if (m.kind == Manipulator::Kind::Marker) {
      continue; // annotations never intercept point selection or dragging
    }
    if ((m.kind == Manipulator::Kind::Arrow || (m.kind == Manipulator::Kind::Ring && nearArrow)) && seenEdgeOn(m)) {
      continue;
    }
    const Drawn drawn = draw(m_handles[i]);
    if (!drawn.valid) {
      continue;
    }
    const HandleSize size = sizeOf(m);
    double beyond = 0.0; // the distance less the reach
    if (m.kind == Manipulator::Kind::Toggle) {
      const QPointF d = position - drawn.handle;
      beyond = std::hypot(d.x(), d.y()) - kToggleReach;
    } else if (m.kind == Manipulator::Kind::Arrow) {
      beyond = distanceToSegment(position, drawn.handle, drawn.tip) - size.grab;
    } else {
      const QPointF d = position - drawn.handle;
      double distance = std::hypot(d.x(), d.y()) - size.knob / 2.0; // the handle's knob is the easiest to grab
      for (int k = 0; k + 1 < drawn.ring.size(); ++k) {
        distance = std::min(distance, distanceToSegment(position, drawn.ring[k], drawn.ring[k + 1]));
      }
      beyond = distance - size.grab;
    }
    if (beyond < nearest) {
      nearest = beyond;
      best = static_cast<int>(i);
    }
  }
  return best;
}

ManipulatorOverlay::DragValue ManipulatorOverlay::valueAt(const Handle& handle, const QPointF& position) {
  const Manipulator& m = handle.manipulator;
  if (m.kind == Manipulator::Kind::Arrow) {
    // Along the arrow's line on the screen: a millimetre is this long.
    const QPointF a = m_viewer.toWidgetF(m.origin);
    const QPointF b = m_viewer.toWidgetF(m.origin.Translated(gp_Vec(m.direction)));
    const QPointF ab = b - a;
    const double length2 = QPointF::dotProduct(ab, ab);
    if (length2 < 1e-4) {
      return {handle.value, handle.value}; // looking along the arrow
    }
    // Where the arrow was grabbed, not its base, follows the cursor.
    const double factor = std::abs(m.factor) > 1e-12 ? m.factor : 1.0;
    const double raw = std::max(
        m.minimum, m.base + QPointF::dotProduct(position - m_grab - a, ab) / length2 / factor);
    // The step fixed when the drag began (mitcad#1431); a value asked
    // for outside a drag at the step it would take now.
    const double step = m_dragStep > 0.0 ? m_dragStep : arrowStep(handle).step;
    return {std::max(m.minimum, std::round(raw / step) * step), raw};
  }
  const double angle = ringAngle(handle, position);
  if (!std::isfinite(angle)) {
    return {handle.value, handle.value}; // the ring is edge-on
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
  const double raw = m_dragStart + m_turned;
  return {std::round(raw / degree) * degree, raw};
}

ManipulatorOverlay::ArrowStep ManipulatorOverlay::arrowStep(const Handle& handle) const {
  // About three pixels of the view along the arrow, in the value's unit;
  // finer by the geometry's size, else the value the drag starts from
  // (HandleStep.hpp, mitcad#1431).
  const Manipulator& m = handle.manipulator;
  const double factor = std::abs(m.factor) > 1e-12 ? std::abs(m.factor) : 1.0;
  HandleStepInput input;
  input.zoom = 3.0 * pixelSize(m.origin) / factor;
  input.scale = m.scale;
  if (handle.length && m.valueScale) {
    input.value = m_drag >= 0 ? m_dragStart : handle.value;
  }
  input.finest = handle.finest;
  return {handleStep(input), niceHandleStep(input.zoom)};
}

double ManipulatorOverlay::ringAngle(const Handle& handle, const QPointF& position) const {
  // In the ring's plane as the screen shows it: position = c + u r + v s.
  const Manipulator& m = handle.manipulator;
  const gp_Vec cross = gp_Vec(m.direction).Crossed(gp_Vec(m.reference));
  if (m.kind != Manipulator::Kind::Ring || cross.Magnitude() < 1e-9) {
    return std::numeric_limits<double>::quiet_NaN();
  }
  const double radius = sizeOf(m).ring * pixelSize(m.origin);
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

void ManipulatorOverlay::setHandlesShown(bool shown) {
  if (shown == !isHidden()) {
    return;
  }
  setVisible(shown);
  publishBoxes(); // none while hidden
  if (shown) {
    logPlaces();
  }
}

bool ManipulatorOverlay::eventFilter(QObject* watched, QEvent* event) {
  if (watched != &m_viewer) {
    return QWidget::eventFilter(watched, event);
  }
  if (isHidden() && event->type() != QEvent::Resize) {
    return false; // hidden with the preview (mitcad#1421) or the command ended: the view's
  }
  // An exception must not leave a Qt event handler: a handle that cannot
  // be worked out (degenerate geometry) just takes no events.
  try {
    return viewerEvent(event);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Manipulator:" << e.what();
    m_drag = -1;
    m_pending = -1;
    m_visualValue.reset();
    return false;
  }
}

bool ManipulatorOverlay::viewerEvent(QEvent* event) {
  switch (event->type()) {
  case QEvent::Resize:
    resize(m_viewer.size());
    return false;
  case QEvent::ToolTip: {
    // A value box says what a click on it does (mitcad#1181).
    const auto* help = static_cast<const QHelpEvent*>(event);
    const int box = boxAt(help->pos());
    if (box < 0) {
      return false;
    }
    QToolTip::showText(help->globalPos(),
                       tr("%1: click to type a value; Enter keeps it, Esc cancels")
                           .arg(m_handles[static_cast<std::size_t>(box)].label),
                       &m_viewer, m_boxes[static_cast<std::size_t>(box)].toAlignedRect());
    return true;
  }
  case QEvent::WindowBlocked:
    // A modal dialog took the input (a long computation's progress, P7),
    // and with it the release: a drag ends at the value it has.
    m_pending = -1;
    m_boxKey.clear();
    if (m_drag >= 0) {
      const QString key = m_handles[static_cast<std::size_t>(m_drag)].key;
      const double value = m_handles[static_cast<std::size_t>(m_drag)].value;
      logDragFeedback();
      m_drag = -1;
      m_visualValue.reset();
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
    // A value box takes the click: it edits the value (mitcad#656).
    if (const int box = boxAt(e->position()); box >= 0) {
      m_boxKey = m_handles[static_cast<std::size_t>(box)].key;
      return true;
    }
    const int at = handleAt(e->position());
    if (at < 0) {
      return false;
    }
    // A drag once the mouse moves; a click stays with the handle and does
    // nothing (mitcad#1105).
    m_pending = at;
    m_pressAt = e->position();
    m_visualValue.reset();
    m_dragMoves = m_semanticChanges = m_visualUpdates = m_dragPaints = 0;
    return true;
  }
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (!m_boxKey.isEmpty()) {
      return true; // a value box held: its click comes with the release
    }
    if (m_pending >= 0 || m_drag >= 0) { ++m_dragMoves; }
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
      m_visualValue = handle.value;
      m_dragStep = 0.0;
      // For UI tests: the color it is drawn in while dragged (mitcad#1707).
      qDebug().noquote() << QStringLiteral("Handle drag %1 %2: color %3")
                                .arg(m_command, handle.key, colorRoleName(activeRole(handle.manipulator)));
      if (handle.manipulator.kind == Manipulator::Kind::Arrow) {
        // One step for the whole drag (mitcad#1431), logged for the UI
        // tests with the zoom's step it may be finer than.
        const ArrowStep step = arrowStep(handle);
        m_dragStep = step.step;
        qDebug().noquote() << QStringLiteral("Manipulator %1 %2 drag step %3 (zoom step %4)")
                                  .arg(m_command, handle.key)
                                  .arg(step.step, 0, 'g', 6)
                                  .arg(step.zoom, 0, 'g', 6);
      }
    }
    if (m_drag >= 0) {
      Handle& handle = m_handles[static_cast<std::size_t>(m_drag)];
      const DragValue value = valueAt(handle, e->position());
      if (!m_visualValue || *m_visualValue != value.continuous) {
        m_visualValue = value.continuous;
        ++m_visualUpdates;
        update();
      }
      if (value.snapped != handle.value) {
        handle.value = value.snapped;
        ++m_semanticChanges;
        emit dragged(handle.key, value.snapped);
      }
      return true;
    }
    const int hover = handleAt(e->position());
    const int boxHover = boxAt(e->position());
    if (hover != m_hover || boxHover != m_boxHover) {
      if (hover != m_hover) {
        // For UI tests: the handle under the mouse and the color it is
        // drawn in now (mitcad#1707).
        if (hover >= 0) {
          const Handle& handle = m_handles[static_cast<std::size_t>(hover)];
          qDebug().noquote() << QStringLiteral("Handle hover %1 %2: color %3")
                                    .arg(m_command, handle.key, colorRoleName(activeRole(handle.manipulator)));
        } else {
          qDebug().noquote() << QStringLiteral("Handle hover %1: none").arg(m_command);
        }
      }
      // A box shown only while its handle or itself is under the mouse
      // comes or goes (Manipulator::boxAtZero).
      m_hover = hover;
      m_boxHover = boxHover;
      publishBoxes();
      update();
      logPlaces();
    }
    return false;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (!m_boxKey.isEmpty()) {
      const QString key = std::exchange(m_boxKey, QString());
      const int box = boxAt(e->position());
      if (box >= 0 && m_handles[static_cast<std::size_t>(box)].key == key) {
        const QPoint centre = m_boxes[static_cast<std::size_t>(box)].center().toPoint();
        qDebug().noquote() << QStringLiteral("Value box %1 %2 clicked").arg(m_command, key);
        emit valueBoxClicked(key, centre);
      }
      return true;
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
      // Not dragged: the press was meant for the handle, so the click does
      // nothing. It never goes on to the view, where it would pick what is
      // under the handle or turn the view on the orientation cube
      // (mitcad#1105).
      const QString key = m_handles[static_cast<std::size_t>(m_pending)].key;
      m_pending = -1;
      qDebug().noquote() << QStringLiteral("Manipulator %1 %2 clicked without a drag: kept by the handle")
                                .arg(m_command, key);
      return true;
    }
    if (m_drag < 0) {
      return false;
    }
    const QString key = m_handles[static_cast<std::size_t>(m_drag)].key;
    const double value = m_handles[static_cast<std::size_t>(m_drag)].value;
    logDragFeedback();
    m_drag = -1;
    m_visualValue.reset();
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
  if (m_drag >= 0) { ++m_dragPaints; }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  layoutBoxes(); // the dragged value's box where its handle is now
  // Lines of the command's own (a belt's pitch line, mitcad#656).
  painter.setPen(QPen(QColor(0x1f, 0x9d, 0x8b), 1.6));
  painter.setBrush(Qt::NoBrush);
  for (const std::vector<gp_Pnt>& line : m_lines) {
    QPolygonF points;
    for (const gp_Pnt& point : line) {
      points << m_viewer.toWidgetF(point);
    }
    painter.drawPolyline(points);
  }
  for (std::size_t i = 0; i < m_handles.size(); ++i) {
    Drawn drawn;
    Drawn snapped;
    try {
      snapped = draw(m_handles[i]);
      drawn = snapped;
      if (static_cast<int>(i) == m_drag && m_visualValue && *m_visualValue != m_handles[i].value) {
        Handle visual = m_handles[i];
        visual.value = *m_visualValue;
        drawn = draw(visual);
      }
    } catch (const std::exception&) {
      continue; // degenerate: not drawn
    }
    if (!drawn.valid) {
      continue;
    }
    const bool active = static_cast<int>(i) == m_drag || static_cast<int>(i) == m_hover;
    const QColor highlight = activeColor(m_handles[i].manipulator);
    const QColor color = active ? highlight : handleColor(m_handles[i].manipulator);
    // Seen edge-on and not under the mouse: fainter (mitcad#1707), Free
    // Move's; the other commands' handles are drawn as they were.
    const bool faint = !active && m_handles[i].manipulator.large && seenEdgeOn(m_handles[i].manipulator);
    painter.setOpacity(faint ? kFaintOpacity : 1.0);
    if (m_handles[i].manipulator.kind == Manipulator::Kind::Marker) {
      const QPointF at = drawn.handle;
      painter.setPen(QPen(kInk, 2.0));
      painter.setBrush(QColor(0xff, 0xd1, 0x80));
      painter.drawEllipse(at, 7.0, 7.0);
      painter.drawLine(at + QPointF(-11, 0), at + QPointF(11, 0));
      painter.drawLine(at + QPointF(0, -11), at + QPointF(0, 11));
      const QRectF label = painter.fontMetrics().boundingRect(m_handles[i].label).adjusted(-5, -3, 5, 3);
      const QRectF box(at + QPointF(15, -label.height() / 2), label.size());
      painter.setPen(Qt::NoPen);
      painter.setBrush(QColor(0xff, 0xf7, 0xe8, 235));
      painter.drawRoundedRect(box, 3, 3);
      painter.setPen(kInk);
      painter.drawText(box, Qt::AlignCenter, m_handles[i].label);
    } else if (m_handles[i].manipulator.kind == Manipulator::Kind::Toggle) {
      // A pattern's instance (P9): filled while it is made, hollow when
      // suppressed.
      painter.setPen(QPen(active ? highlight : kInk, 1.5));
      painter.setBrush(m_handles[i].manipulator.on ? color : QColor(Qt::white));
      painter.drawEllipse(drawn.handle, 6.0, 6.0);
    } else if (m_handles[i].manipulator.kind == Manipulator::Kind::Arrow) {
      if (m_handles[i].manipulator.valueBox) {
        // A dimension (mitcad#656): a solid line from the reference, a
        // short bar across it there.
        painter.setPen(QPen(color, 1.4));
        painter.drawLine(drawn.origin, drawn.handle);
        const QPointF line = unit(drawn.handle - drawn.origin);
        const QPointF bar(-line.y() * 6.0, line.x() * 6.0);
        painter.drawLine(drawn.origin - bar, drawn.origin + bar);
      } else {
        painter.setPen(QPen(color, 1.0, Qt::DashLine));
        painter.drawLine(drawn.origin, drawn.handle);
      }
      // The shaft, its head and its knob over a dark halo (mitcad#1581).
      const HandleSize size = sizeOf(m_handles[i].manipulator);
      painter.setBrush(Qt::NoBrush);
      haloed(painter, QPen(color, size.shaft, Qt::SolidLine, Qt::RoundCap),
             [&painter, &drawn] { painter.drawLine(drawn.handle, drawn.tip); });
      const QPointF along = unit(drawn.tip - drawn.handle);
      const QPointF across(-along.y(), along.x());
      QPolygonF head;
      head << drawn.tip + along * (10.0 * size.scale) << drawn.tip + across * (6.0 * size.scale)
           << drawn.tip - across * (6.0 * size.scale);
      painter.setPen(QPen(kHalo, 1.5));
      painter.setBrush(color);
      painter.drawPolygon(head);
      painter.drawEllipse(drawn.handle, 3.5 * size.scale, 3.5 * size.scale);
    } else {
      const HandleSize size = sizeOf(m_handles[i].manipulator);
      painter.setBrush(Qt::NoBrush);
      haloed(painter, QPen(color, active ? size.line + 1.0 : size.line),
             [&painter, &drawn] { painter.drawPolyline(drawn.ring); });
      if (!drawn.arc.isEmpty()) {
        // An angle's dimension (mitcad#656): the zero line along the
        // reference, the arc to the knob.
        painter.setPen(QPen(color, 1.4));
        painter.drawLine(drawn.origin, drawn.zero);
        haloed(painter, QPen(color, std::max(size.shaft - 1.0, 3.0), Qt::SolidLine, Qt::RoundCap),
               [&painter, &drawn] { painter.drawPolyline(drawn.arc); });
      }
      painter.setPen(QPen(color, 1.0, Qt::DashLine));
      painter.drawLine(drawn.origin, drawn.handle);
      painter.setPen(QPen(kHalo, 1.5));
      painter.setBrush(color);
      painter.drawEllipse(drawn.handle, size.knob, size.knob);
    }
    if (i < m_boxes.size() && !m_boxes[i].isEmpty()) {
      // The value, where a click edits it (mitcad#656), as the layout
      // pass placed it; a thin leader back to where it belongs when the
      // pass moved it (off another box or a panel).
      const QRectF& placed = m_boxes[i];
      if (!drawn.box.isEmpty() && !placed.contains(drawn.box.center())) {
        const QPointF anchor = drawn.box.center();
        const QPointF nearest(std::clamp(anchor.x(), placed.left(), placed.right()),
                              std::clamp(anchor.y(), placed.top(), placed.bottom()));
        painter.setPen(QPen(color, 1.0));
        painter.drawLine(anchor, nearest);
        painter.setBrush(color);
        painter.drawEllipse(anchor, 2.0, 2.0);
      }
      // Under the mouse, the box stands out in the selection's blue: a
      // click there types the value (mitcad#1181; its tooltip says so).
      const bool boxHovered = static_cast<int>(i) == m_boxHover;
      painter.setPen(QPen(boxHovered ? kActiveColor : color, boxHovered ? 2.0 : 1.0));
      painter.setBrush(QColor(0xff, 0xff, 0xff, 235));
      painter.drawRoundedRect(m_boxes[i], 4, 4);
      painter.setPen(kInk);
      painter.drawText(m_boxes[i], Qt::AlignCenter, m_handles[i].text);
    }
    if (static_cast<int>(i) == m_drag && m_visualValue && snapped.valid) {
      // The moving arrow/knob follows the pointer continuously. This
      // separate tick and the numeric field identify what will be kept.
      painter.setPen(QPen(highlight, 1.0, Qt::DotLine));
      painter.drawLine(drawn.handle, snapped.handle);
      painter.setPen(QPen(highlight, 1.0));
      painter.setBrush(kHandleColor);
      painter.drawEllipse(snapped.handle, 2.5, 2.5);
      painter.drawText(snapped.handle + QPointF(10, 18), tr("Snapped"));
    }
  }
  painter.setOpacity(1.0);
  if (!m_hint.isEmpty()) {
    // A pill at the top of the view, over the model.
    const QRectF text = painter.fontMetrics().boundingRect(m_hint).adjusted(-14, -7, 14, 7);
    const QRectF box(QPointF((width() - text.width()) / 2.0, 16.0), text.size());
    painter.setPen(Qt::NoPen);
    QColor pill = kInk;
    pill.setAlpha(215);
    painter.setBrush(pill);
    painter.drawRoundedRect(box, box.height() / 2.0, box.height() / 2.0);
    painter.setPen(Qt::white);
    painter.drawText(box, Qt::AlignCenter, m_hint);
  }
}

void ManipulatorOverlay::logDragFeedback() const {
  if (timingLogged() && m_drag >= 0) {
    qDebug().noquote() << QStringLiteral("Manipulator feedback %1 %2: moves=%3 semantic=%4 visual=%5 paints=%6")
        .arg(m_command, m_handles[static_cast<std::size_t>(m_drag)].key)
        .arg(m_dragMoves).arg(m_semanticChanges).arg(m_visualUpdates).arg(m_dragPaints);
  }
}

void ManipulatorOverlay::logPlaces() {
  if (!isVisible()) {
    return;
  }
  layoutBoxes();
  QStringList lines;
  for (std::size_t index = 0; index < m_handles.size(); ++index) {
    const Handle& handle = m_handles[index];
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
    if (handle.manipulator.kind == Manipulator::Kind::Marker) {
      const gp_Pnt& point = handle.manipulator.origin;
      lines << QStringLiteral("Preview marker %1 %2 world %3,%4,%5 at %6,%7")
                   .arg(m_command, handle.label).arg(point.X()).arg(point.Y()).arg(point.Z()).arg(at.x()).arg(at.y());
      continue;
    }
    const QString key = handle.manipulator.kind == Manipulator::Kind::Toggle
                            ? QStringLiteral("%1.%2%3")
                                  .arg(handle.key)
                                  .arg(handle.manipulator.element)
                                  .arg(handle.manipulator.on ? QString() : QStringLiteral(" (off)"))
                            : handle.key;
    lines << QStringLiteral("Manipulator %1 %2 at %3,%4").arg(m_command, key).arg(at.x()).arg(at.y());
    if (handle.manipulator.kind == Manipulator::Kind::Arrow) {
      // Which way the arrow points in the design, for UI tests (mitcad#958:
      // a sketch in a turned occurrence). Its own prefix, so that the
      // lines above stay the newest "Manipulator ..." of the handle.
      const auto rounded = [](double value) {
        return QString::number(std::abs(value) < 5e-4 ? 0.0 : value, 'f', 3);
      };
      const gp_Dir& way = handle.manipulator.direction;
      lines << QStringLiteral("Arrow direction %1 %2: %3,%4,%5")
                   .arg(m_command, key, rounded(way.X()), rounded(way.Y()), rounded(way.Z()));
    }
    if (handle.manipulator.kind == Manipulator::Kind::Ring) {
      // What a ring turns about on the screen, for UI tests: its knob lies
      // the way the pointer does from here (mitcad#1459).
      const QPoint centre = mapTo(window(), m_viewer.toWidgetF(handle.manipulator.origin).toPoint());
      lines << QStringLiteral("Ring centre %1 %2 at %3,%4").arg(m_command, key).arg(centre.x()).arg(centre.y());
    }
    if (handle.manipulator.kind == Manipulator::Kind::Arrow || handle.manipulator.kind == Manipulator::Kind::Ring) {
      // Where it is in the design and its color, for UI tests (mitcad#1580,
      // mitcad#1581, mitcad#1583): an arrow's start and knob, a ring's
      // center.
      const Manipulator& m = handle.manipulator;
      const auto point = [](const gp_Pnt& p) {
        return QStringLiteral("%1,%2,%3").arg(worldNumber(p.X()), worldNumber(p.Y()), worldNumber(p.Z()));
      };
      const QString place =
          m.kind == Manipulator::Kind::Arrow
              ? QStringLiteral("origin %1, knob %2")
                    .arg(point(m.origin),
                         point(m.origin.Translated(gp_Vec(m.direction) * ((handle.value - m.base) * m.factor))))
              : QStringLiteral("center %1").arg(point(m.origin));
      lines << QStringLiteral("Handle world %1 %2: %3, color %4").arg(m_command, key, place, colorRoleName(m.color));
      if (m.kind == Manipulator::Kind::Arrow) {
        // Where its head is on the screen, for UI tests (mitcad#1707:
        // Free Move's beyond the rings).
        const QPoint head = mapTo(window(), drawn.tip.toPoint());
        lines << QStringLiteral("Arrow head %1 %2 at %3,%4").arg(m_command, key).arg(head.x()).arg(head.y());
      }
      if (seenEdgeOn(m)) {
        // Seen edge-on (mitcad#1707): a ring yields to the arrows, a large
        // arrow takes no press.
        lines << QStringLiteral("Handle seen edge-on %1 %2: %3")
                     .arg(m_command, key,
                          m.kind == Manipulator::Kind::Ring ? QStringLiteral("no press within an arrow's reach")
                                                            : QStringLiteral("no press"));
      }
    }
    if (index < m_boxes.size() && !m_boxes[index].isEmpty()) {
      // Where a click edits the value (mitcad#656).
      const QPoint box = mapTo(window(), m_boxes[index].center().toPoint());
      lines << QStringLiteral("Value box %1 %2 at %3,%4: %5")
                   .arg(m_command, handle.key)
                   .arg(box.x())
                   .arg(box.y())
                   .arg(handle.text);
    }
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
