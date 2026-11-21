// SPDX-License-Identifier: MIT
#include "CommentMarkup.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QFont>
#include <QFontMetricsF>
#include <QObject>
#include <QPainter>
#include <QPainterPath>
#include <QPainterPathStroker>
#include <QPen>
#include <QPolygonF>

namespace mitcad {
namespace {

constexpr double kPi = 3.14159265358979323846;

// The arrow's head: its length for a line width, and its half angle.
double headLength(double width) { return std::max(12.0, width * 4.0); }
constexpr double kHeadHalfAngle = 25.0 * kPi / 180.0;

double length(const QPointF& v) { return std::hypot(v.x(), v.y()); }

// The angle of `p` about `centre` as QPainterPath::arcTo counts it
// (degrees, counter-clockwise as seen with y down).
double arcAngle(const QPointF& centre, const QPointF& p) {
  return std::atan2(-(p.y() - centre.y()), p.x() - centre.x()) * 180.0 / kPi;
}

double positiveDegrees(double angle) {
  const double wrapped = std::fmod(angle, 360.0);
  return wrapped < 0.0 ? wrapped + 360.0 : wrapped;
}

// The circle through three points; false when they are on one line.
bool circleThrough(const QPointF& a, const QPointF& b, const QPointF& c, QPointF& centre, double& radius) {
  const double d = 2.0 * (a.x() * (b.y() - c.y()) + b.x() * (c.y() - a.y()) + c.x() * (a.y() - b.y()));
  if (std::abs(d) < 1e-9) {
    return false;
  }
  const double aa = a.x() * a.x() + a.y() * a.y();
  const double bb = b.x() * b.x() + b.y() * b.y();
  const double cc = c.x() * c.x() + c.y() * c.y();
  centre = QPointF((aa * (b.y() - c.y()) + bb * (c.y() - a.y()) + cc * (a.y() - b.y())) / d,
                   (aa * (c.x() - b.x()) + bb * (a.x() - c.x()) + cc * (b.x() - a.x())) / d);
  radius = length(a - centre);
  return std::isfinite(radius) && radius > 0.0;
}

QFont textFont(double width) {
  QFont font;
  font.setPixelSize(std::max(6, static_cast<int>(std::lround(markupTextSize(width)))));
  return font;
}

// The arrow's head: its tip and its two back corners.
QPolygonF arrowHead(const QPointF& from, const QPointF& to, double width) {
  const QPointF along = to - from;
  const double size = length(along);
  if (size < 1e-9) {
    return {};
  }
  const double angle = std::atan2(along.y(), along.x());
  const double head = std::min(headLength(width), size);
  const QPointF left(to.x() - head * std::cos(angle - kHeadHalfAngle), to.y() - head * std::sin(angle - kHeadHalfAngle));
  const QPointF right(to.x() - head * std::cos(angle + kHeadHalfAngle), to.y() - head * std::sin(angle + kHeadHalfAngle));
  return QPolygonF({to, left, right});
}

QPen shapePen(const MarkupShape& shape) {
  QPen pen(shape.color, shape.width, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
  return pen;
}

} // namespace

QVector<MarkupTool> markupTools() {
  return {MarkupTool::Select,    MarkupTool::Freehand,      MarkupTool::Line,  MarkupTool::Polyline, MarkupTool::Arc,
          MarkupTool::Rectangle, MarkupTool::Parallelogram, MarkupTool::Arrow, MarkupTool::Text};
}

QString markupToolKey(MarkupTool tool) {
  switch (tool) {
  case MarkupTool::Select:
    return QStringLiteral("select");
  case MarkupTool::Freehand:
    return QStringLiteral("freehand");
  case MarkupTool::Line:
    return QStringLiteral("line");
  case MarkupTool::Polyline:
    return QStringLiteral("polyline");
  case MarkupTool::Arc:
    return QStringLiteral("arc");
  case MarkupTool::Rectangle:
    return QStringLiteral("rectangle");
  case MarkupTool::Parallelogram:
    return QStringLiteral("parallelogram");
  case MarkupTool::Arrow:
    return QStringLiteral("arrow");
  case MarkupTool::Text:
    return QStringLiteral("text");
  }
  return QString();
}

QString markupToolLabel(MarkupTool tool) {
  switch (tool) {
  case MarkupTool::Select:
    return QObject::tr("Select");
  case MarkupTool::Freehand:
    return QObject::tr("Freehand");
  case MarkupTool::Line:
    return QObject::tr("Line");
  case MarkupTool::Polyline:
    return QObject::tr("Polyline");
  case MarkupTool::Arc:
    return QObject::tr("Arc");
  case MarkupTool::Rectangle:
    return QObject::tr("Rectangle");
  case MarkupTool::Parallelogram:
    return QObject::tr("Parallelogram");
  case MarkupTool::Arrow:
    return QObject::tr("Arrow");
  case MarkupTool::Text:
    return QObject::tr("Text");
  }
  return QString();
}

bool markupDragTool(MarkupTool tool) {
  return tool == MarkupTool::Line || tool == MarkupTool::Arc || tool == MarkupTool::Rectangle ||
         tool == MarkupTool::Parallelogram || tool == MarkupTool::Arrow;
}

QVector<QColor> markupInks() {
  return {QColor(QStringLiteral("#e03131")), QColor(QStringLiteral("#fcc419")), QColor(QStringLiteral("#2f9e44")),
          QColor(QStringLiteral("#1971c2")), QColor(QStringLiteral("#212529")), QColor(QStringLiteral("#ffffff"))};
}

QString markupInkName(int index) {
  switch (index) {
  case 0:
    return QObject::tr("Red");
  case 1:
    return QObject::tr("Yellow");
  case 2:
    return QObject::tr("Green");
  case 3:
    return QObject::tr("Blue");
  case 4:
    return QObject::tr("Black");
  case 5:
    return QObject::tr("White");
  default:
    return QString();
  }
}

QVector<double> markupWidths() { return {2.0, 4.0, 8.0}; }

bool MarkupShape::operator==(const MarkupShape& other) const {
  return kind == other.kind && points == other.points && color == other.color && width == other.width &&
         text == other.text;
}

MarkupShape markupFromDrag(MarkupTool tool, const QPointF& from, const QPointF& to, const QColor& color,
                           double width) {
  MarkupShape shape;
  shape.kind = tool;
  shape.color = color;
  shape.width = width;
  switch (tool) {
  case MarkupTool::Arc: {
    // A half circle over the drag, bulging to its left (y down).
    const QPointF middle = (from + to) / 2.0;
    const QPointF half = (to - from) / 2.0;
    shape.points = {from, middle + QPointF(half.y(), -half.x()), to};
    break;
  }
  case MarkupTool::Parallelogram: {
    // In the drag's box, its top and bottom edges shifted apart by a
    // quarter of the box's width.
    const double lean = (to.x() - from.x()) / 4.0;
    shape.points = {QPointF(from.x() + lean, from.y()), QPointF(to.x(), from.y()), QPointF(to.x() - lean, to.y())};
    break;
  }
  default:
    shape.points = {from, to};
    break;
  }
  return shape;
}

QPointF parallelogramFourth(const MarkupShape& shape) {
  if (shape.points.size() < 3) {
    return shape.points.isEmpty() ? QPointF() : shape.points.first();
  }
  return shape.points.at(0) + shape.points.at(2) - shape.points.at(1);
}

double markupTextSize(double width) { return 10.0 + width * 3.5; }

QPainterPath markupPath(const MarkupShape& shape) {
  QPainterPath path;
  const QVector<QPointF>& p = shape.points;
  if (p.isEmpty()) {
    return path;
  }
  switch (shape.kind) {
  case MarkupTool::Select:
    break;
  case MarkupTool::Text: {
    const QFontMetricsF metrics(textFont(shape.width));
    path.addRect(QRectF(p.first(), QSizeF(std::max(metrics.horizontalAdvance(shape.text), 1.0), metrics.height())));
    break;
  }
  case MarkupTool::Rectangle:
    if (p.size() >= 2) {
      path.addRect(QRectF(p.at(0), p.at(1)).normalized());
    }
    break;
  case MarkupTool::Parallelogram:
    if (p.size() >= 3) {
      path.addPolygon(QPolygonF({p.at(0), p.at(1), p.at(2), parallelogramFourth(shape), p.at(0)}));
      path.closeSubpath();
    }
    break;
  case MarkupTool::Arc: {
    QPointF centre;
    double radius = 0.0;
    if (p.size() < 3 || !circleThrough(p.at(0), p.at(1), p.at(2), centre, radius)) {
      // On one line: the straight line from start to end.
      path.moveTo(p.first());
      path.lineTo(p.last());
      break;
    }
    const double start = arcAngle(centre, p.at(0));
    const double through = positiveDegrees(arcAngle(centre, p.at(1)) - start);
    const double end = positiveDegrees(arcAngle(centre, p.at(2)) - start);
    // Counter-clockwise when the point passed through comes first that
    // way, clockwise otherwise.
    const double sweep = through <= end ? end : end - 360.0;
    path.moveTo(p.at(0));
    path.arcTo(QRectF(centre.x() - radius, centre.y() - radius, 2.0 * radius, 2.0 * radius), start, sweep);
    break;
  }
  case MarkupTool::Arrow:
    if (p.size() >= 2) {
      // The shaft ends where the head starts, so that its round cap does
      // not show past the tip.
      const QPointF along = p.at(1) - p.at(0);
      const double size = length(along);
      const double head = std::min(headLength(shape.width), size) * std::cos(kHeadHalfAngle);
      path.moveTo(p.at(0));
      path.lineTo(size > 1e-9 ? p.at(1) - along * (head / size) : p.at(1));
    }
    break;
  case MarkupTool::Freehand:
  case MarkupTool::Line:
  case MarkupTool::Polyline:
    path.moveTo(p.first());
    for (qsizetype i = 1; i < p.size(); ++i) {
      path.lineTo(p.at(i));
    }
    if (p.size() == 1) {
      // A dot.
      path.lineTo(p.first());
    }
    break;
  }
  return path;
}

QRectF markupBounds(const MarkupShape& shape) {
  const QPainterPath path = markupPath(shape);
  if (shape.kind == MarkupTool::Text) {
    return path.boundingRect();
  }
  QRectF bounds = path.boundingRect();
  if (shape.kind == MarkupTool::Arrow && shape.points.size() >= 2) {
    bounds = bounds.united(arrowHead(shape.points.at(0), shape.points.at(1), shape.width).boundingRect());
  }
  const double half = shape.width / 2.0;
  return bounds.adjusted(-half, -half, half, half);
}

void paintMarkupShape(QPainter& painter, const MarkupShape& shape) {
  if (shape.points.isEmpty()) {
    return;
  }
  painter.save();
  painter.setRenderHint(QPainter::Antialiasing, true);
  if (shape.kind == MarkupTool::Text) {
    const QFont font = textFont(shape.width);
    painter.setFont(font);
    painter.setPen(shape.color);
    painter.drawText(shape.points.first() + QPointF(0.0, QFontMetricsF(font).ascent()), shape.text);
    painter.restore();
    return;
  }
  painter.setPen(shapePen(shape));
  painter.setBrush(Qt::NoBrush);
  painter.drawPath(markupPath(shape));
  if (shape.kind == MarkupTool::Arrow && shape.points.size() >= 2) {
    const QPolygonF head = arrowHead(shape.points.at(0), shape.points.at(1), shape.width);
    if (!head.isEmpty()) {
      QPen outline(shape.color, 1.0, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
      painter.setPen(outline);
      painter.setBrush(shape.color);
      painter.drawPolygon(head);
    }
  }
  painter.restore();
}

QVector<QPointF> markupHandles(const MarkupShape& shape) {
  if (shape.kind == MarkupTool::Freehand) {
    return {};
  }
  return shape.points;
}

int markupShapeAt(const QVector<MarkupShape>& shapes, const QPointF& at, double tolerance) {
  for (qsizetype i = shapes.size() - 1; i >= 0; --i) {
    const MarkupShape& shape = shapes.at(i);
    if (shape.kind == MarkupTool::Text) {
      if (markupBounds(shape).adjusted(-tolerance, -tolerance, tolerance, tolerance).contains(at)) {
        return static_cast<int>(i);
      }
      continue;
    }
    QPainterPathStroker stroker;
    stroker.setWidth(shape.width + 2.0 * tolerance);
    stroker.setCapStyle(Qt::RoundCap);
    stroker.setJoinStyle(Qt::RoundJoin);
    QPainterPath area = stroker.createStroke(markupPath(shape));
    if (shape.kind == MarkupTool::Arrow && shape.points.size() >= 2) {
      QPainterPath head;
      head.addPolygon(arrowHead(shape.points.at(0), shape.points.at(1), shape.width));
      area.addPath(head);
    }
    if (area.contains(at)) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

int markupHandleAt(const MarkupShape& shape, const QPointF& at, double tolerance) {
  const QVector<QPointF> handles = markupHandles(shape);
  int found = -1;
  double nearest = tolerance;
  for (qsizetype i = 0; i < handles.size(); ++i) {
    const double distance = length(handles.at(i) - at);
    if (distance <= nearest) {
      nearest = distance;
      found = static_cast<int>(i);
    }
  }
  return found;
}

MarkupShape markupMoved(const MarkupShape& shape, const QPointF& delta) {
  MarkupShape moved = shape;
  for (QPointF& point : moved.points) {
    point += delta;
  }
  return moved;
}

MarkupShape markupWithHandle(const MarkupShape& shape, int handle, const QPointF& to) {
  MarkupShape edited = shape;
  if (handle >= 0 && handle < edited.points.size() && shape.kind != MarkupTool::Freehand) {
    edited.points[handle] = to;
  }
  return edited;
}

QImage flattenMarkup(const QImage& picture, const QVector<MarkupShape>& shapes) {
  QImage flat = picture.convertToFormat(QImage::Format_ARGB32_Premultiplied);
  // The shapes are in the picture's pixels, whatever its device pixel ratio.
  const qreal ratio = picture.devicePixelRatio();
  flat.setDevicePixelRatio(1.0);
  {
    QPainter painter(&flat);
    for (const MarkupShape& shape : shapes) {
      paintMarkupShape(painter, shape);
    }
  }
  flat.setDevicePixelRatio(ratio);
  return flat;
}

QString markupSummary(const QVector<MarkupShape>& shapes) {
  QStringList kinds;
  for (const MarkupShape& shape : shapes) {
    kinds << markupToolKey(shape.kind);
  }
  return kinds.isEmpty() ? QStringLiteral("none") : kinds.join(QStringLiteral(", "));
}

MarkupHistory::MarkupHistory(QVector<MarkupShape> start) { m_states.append(std::move(start)); }

void MarkupHistory::push(const QVector<MarkupShape>& shapes) {
  if (shapes == m_states.at(m_at)) {
    return;
  }
  m_states.resize(m_at + 1);
  m_states.append(shapes);
  m_at = m_states.size() - 1;
}

bool MarkupHistory::undo() {
  if (!canUndo()) {
    return false;
  }
  --m_at;
  return true;
}

bool MarkupHistory::redo() {
  if (!canRedo()) {
    return false;
  }
  ++m_at;
  return true;
}

} // namespace mitcad
