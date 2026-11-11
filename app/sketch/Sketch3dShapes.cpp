// SPDX-License-Identifier: MIT
// The plane-bound drawing tools of a 3D sketch (mitcad#1550): Center
// Diameter, 2-Point and 3-Point Circle, Ellipse, 2-Point, Center and 3-Point
// Rectangle, Circumscribed, Inscribed and Edge Polygon, and Text, run by the
// SKETCH tab's planar tools' buttons in a 3D sketch. Each draws on the
// active XY, YZ or ZX plane (of the manipulator's frame, rotated or not:
// mitcad#1661) through its first point (placed like any point
// of the sketch: on the plane through the anchor, snapped, or by typed
// offsets); its later clicks stay in that plane, and its typed sizes
// (Radius, Diameter, Major and Minor Radius, Width and Height, Sides, Text
// and Text Height) put them as the planar tools' do and become its driving
// dimensions. The shape with its new points, its in-plane relations and
// its dimensions is one sketch3d.add_* command, its snaps' attachments and
// relations follow in the same undo step. Esc steps back one layer as the
// other tools do. The geometry and the commands are in Sketch3dDraw.hpp.

#include "Sketch3dController.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonDocument>
#include <QtLogging>

#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

constexpr double kDefaultTextHeight = 5.0; // mm, as the planar Text tool's
constexpr double kTiny = 1e-6;

QString xyz(const Xyz& at) {
  const auto n = [](double v) { return QString::number(std::abs(v) < 5e-7 ? 0.0 : v, 'f', 3); };
  return QStringLiteral("(%1, %2, %3)").arg(n(at[0]), n(at[1]), n(at[2]));
}

RectangleMode rectangleMode(Sketch3dTool tool) {
  return tool == Sketch3dTool::RectangleCenter       ? RectangleMode::Center
         : tool == Sketch3dTool::RectangleThreePoint ? RectangleMode::ThreePoint
                                                     : RectangleMode::TwoPoint;
}

bool isRectangle(Sketch3dTool tool) {
  return tool == Sketch3dTool::RectangleTwoPoint || tool == Sketch3dTool::RectangleCenter ||
         tool == Sketch3dTool::RectangleThreePoint;
}

bool isPolygon(Sketch3dTool tool) {
  return tool == Sketch3dTool::PolygonCircumscribed || tool == Sketch3dTool::PolygonInscribed ||
         tool == Sketch3dTool::PolygonEdge;
}

// The unit direction from `from` toward `to` in the plane through `from`.
std::optional<Xyz> inPlane(const Xyz& from, const Xyz& to, const ShapePlane& plane) {
  return unit(sub(ontoPlane(to, from, plane), from));
}

// A value typed and valid.
std::optional<double> typed(const TypedValue& value) {
  return value.edited && value.valid ? std::optional<double>(value.value) : std::nullopt;
}

// The id of an existing point a click snapped to; empty for a new place.
QString existingPoint(const PlacedPoint& point) {
  return point.snap.kind == SnapKind::Point || point.snap.kind == SnapKind::Center ? point.snap.point : QString();
}

QStringList strings(const QJsonValue& list) {
  QStringList out;
  for (const QJsonValue& v : list.toArray()) {
    out << v.toString();
  }
  return out;
}

} // namespace

bool isShapeTool(Sketch3dTool tool) {
  switch (tool) {
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
    return true;
  case Sketch3dTool::None:
  case Sketch3dTool::Point:
  case Sketch3dTool::Line:
  case Sketch3dTool::ArcThreePoint:
  case Sketch3dTool::ArcCenter:
  case Sketch3dTool::ArcTangent:
  case Sketch3dTool::SplineFit:
  case Sketch3dTool::SplineControl:
  case Sketch3dTool::Fillet:
    break;
  }
  return false;
}

int Sketch3dController::shapeClicks() const {
  switch (m_tool) {
  case Sketch3dTool::Text:
    return 1;
  case Sketch3dTool::CircleThreePoint:
  case Sketch3dTool::Ellipse:
  case Sketch3dTool::RectangleThreePoint:
  case Sketch3dTool::PolygonEdge:
    return 3;
  default:
    return 2;
  }
}

int Sketch3dController::sides() const {
  const auto n = typed(m_sidesField);
  return n ? std::clamp(static_cast<int>(std::lround(*n)), 3, 256) : 6;
}

QStringList Sketch3dController::shapeFields() const {
  const QStringList offsets{QStringLiteral("dx"), QStringLiteral("dy"), QStringLiteral("dz")};
  const QString sidesField = QStringLiteral("sides");
  const std::size_t placed = m_pending.size();
  if (m_tool == Sketch3dTool::Text) {
    return {QStringLiteral("text"), QStringLiteral("text_height")};
  }
  if (placed == 0) {
    // The first point by offsets from the anchor, as the Point tool's.
    return isPolygon(m_tool) ? offsets + QStringList{sidesField} : offsets;
  }
  switch (m_tool) {
  case Sketch3dTool::CircleCenter:
    return {QStringLiteral("radius")};
  case Sketch3dTool::CircleTwoPoint:
    return {QStringLiteral("diameter")};
  case Sketch3dTool::Ellipse:
    return placed == 1 ? QStringList{QStringLiteral("major")} : QStringList{QStringLiteral("minor")};
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
    return {QStringLiteral("width"), QStringLiteral("height")};
  case Sketch3dTool::RectangleThreePoint:
    return placed == 1 ? QStringList{QStringLiteral("width")} : QStringList{QStringLiteral("height")};
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
    return {QStringLiteral("radius"), sidesField};
  case Sketch3dTool::PolygonEdge:
    return placed == 1 ? QStringList{QStringLiteral("length"), sidesField} : QStringList{sidesField};
  default:
    return offsets; // a 3-point circle's points
  }
}

std::optional<Xyz> Sketch3dController::shapeHover(const Xyz& at) const {
  const ShapePlane plane = shapePlane();
  const Xyz first = m_pending.front().at;
  const Xyz last = m_pending.back().at;
  const std::size_t placed = m_pending.size();
  // A snap that fixes the place keeps it; any other stays in the shape's
  // plane.
  if (m_hoverSnap.fixesPlace()) {
    return at;
  }
  Xyz in = ontoPlane(at, first, plane);
  const auto toward = [&](const Xyz& from, double length) {
    if (const auto d = inPlane(from, in, plane)) {
      in = add(from, scaled(*d, length));
    }
  };
  switch (m_tool) {
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
    if (const auto r = typed(m_radius)) {
      toward(first, *r);
    }
    break;
  case Sketch3dTool::CircleTwoPoint:
    if (const auto d = typed(m_size)) {
      toward(first, *d);
    }
    break;
  case Sketch3dTool::PolygonEdge:
    if (placed == 1) {
      if (const auto l = typed(m_input.length)) {
        toward(first, *l);
      }
    }
    break;
  case Sketch3dTool::Ellipse:
    if (placed == 1) {
      if (const auto a = typed(m_size)) {
        toward(first, *a);
      }
    } else if (const auto b = typed(m_size2)) {
      // Across the major axis, on the pointer's side.
      if (const auto u = inPlane(first, m_pending[1].at, plane)) {
        Xyz w = cross(plane.normal(), *u);
        if (dot(sub(in, first), w) < 0.0) {
          w = scaled(w, -1.0);
        }
        in = add(first, scaled(w, *b));
      }
    }
    break;
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter: {
    // Width along the plane's first axis, height along its second, toward
    // the pointer; a centre rectangle's halves each side.
    const double factor = m_tool == Sketch3dTool::RectangleCenter ? 0.5 : 1.0;
    std::array<double, 2> uv = planarOf(in, plane);
    const std::array<double, 2> base = planarOf(first, plane);
    for (int k = 0; k < 2; ++k) {
      if (const auto size = typed(k == 0 ? m_size : m_size2)) {
        const double sign = uv[static_cast<std::size_t>(k)] < base[static_cast<std::size_t>(k)] ? -1.0 : 1.0;
        uv[static_cast<std::size_t>(k)] = base[static_cast<std::size_t>(k)] + sign * factor * *size;
      }
    }
    in = fromPlanar(uv, dot(first, plane.normal()), plane);
    break;
  }
  case Sketch3dTool::RectangleThreePoint:
    if (placed == 1) {
      if (const auto w = typed(m_size)) {
        toward(first, *w);
      }
    } else if (const auto h = typed(m_size2)) {
      if (const auto u = inPlane(first, last, plane)) {
        Xyz across = cross(plane.normal(), *u);
        if (dot(sub(in, last), across) < 0.0) {
          across = scaled(across, -1.0);
        }
        in = add(last, scaled(across, *h));
      }
    }
    break;
  default:
    break;
  }
  return in;
}

QString Sketch3dController::shapeMeasured(const QString& id) const {
  const double unitMm = millimetres();
  if (id == QStringLiteral("sides")) {
    return QString::number(sides());
  }
  if (id == QStringLiteral("text_height")) {
    return formatLength(kDefaultTextHeight, unitMm);
  }
  if (!m_hoverAt || m_pending.empty()) {
    return QString();
  }
  const ShapePlane plane = shapePlane();
  const Xyz first = m_pending.front().at;
  const Xyz at = *m_hoverAt;
  if (id == QStringLiteral("radius")) {
    return formatLength(distance(first, ontoPlane(at, first, plane)), unitMm);
  }
  if (id == QStringLiteral("diameter") || id == QStringLiteral("major") ||
      (id == QStringLiteral("length") && m_tool == Sketch3dTool::PolygonEdge)) {
    return formatLength(distance(first, ontoPlane(at, first, plane)), unitMm);
  }
  if (id == QStringLiteral("minor") && m_pending.size() == 2) {
    return formatLength(ellipseMinor(first, m_pending[1].at, at, plane), unitMm);
  }
  if (isRectangle(m_tool)) {
    std::vector<Xyz> clicks;
    for (const PlacedPoint& p : m_pending) {
      clicks.push_back(p.at);
    }
    clicks.push_back(at);
    if (const auto corners = rectangleCorners(rectangleMode(m_tool), clicks, plane)) {
      // The first side from the first corner given, the second across.
      const bool turned = m_tool == Sketch3dTool::RectangleThreePoint &&
                          rectangleClickCorners(rectangleMode(m_tool), clicks, plane).at(1) == 3;
      const double width = turned ? distance((*corners)[0], (*corners)[3]) : distance((*corners)[0], (*corners)[1]);
      const double height = turned ? distance((*corners)[0], (*corners)[1]) : distance((*corners)[1], (*corners)[2]);
      if (id == QStringLiteral("width")) {
        return formatLength(width, unitMm);
      }
      if (id == QStringLiteral("height")) {
        return formatLength(height, unitMm);
      }
    } else if (id == QStringLiteral("width") && m_pending.size() == 1) {
      return formatLength(distance(first, ontoPlane(at, first, plane)), unitMm);
    }
  }
  return QString();
}

QString Sketch3dController::shapeProblem(const Xyz& at) const {
  for (const TypedValue* positive : {&m_size, &m_size2, &m_textHeight}) {
    if (positive->edited && positive->valid && !(positive->value > kTiny)) {
      return tr("A size must be greater than zero.");
    }
  }
  if (m_pending.empty()) {
    return QString();
  }
  const ShapePlane plane = shapePlane();
  const Xyz first = m_pending.front().at;
  const std::size_t placed = m_pending.size();
  std::vector<Xyz> clicks;
  for (const PlacedPoint& p : m_pending) {
    clicks.push_back(p.at);
  }
  clicks.push_back(at);
  switch (m_tool) {
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::CircleTwoPoint:
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
    if (distance(first, ontoPlane(at, first, plane)) <= kTiny) {
      return tr("The point is on the first one: the shape would have no size.");
    }
    break;
  case Sketch3dTool::CircleThreePoint:
    if (placed == 2 && !threePointCircle(clicks[0], clicks[1], clicks[2], plane)) {
      return tr("The three points lie on one line: move the third off it.");
    }
    break;
  case Sketch3dTool::Ellipse:
    if (placed == 1 && distance(first, ontoPlane(at, first, plane)) <= kTiny) {
      return tr("The point is on the center: the major axis would have no length.");
    }
    if (placed == 2 && ellipseMinor(first, m_pending[1].at, at, plane) <= kTiny) {
      return tr("The point is on the major axis: the minor radius would be zero.");
    }
    break;
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
    if (!rectangleCorners(rectangleMode(m_tool), clicks, plane)) {
      return tr("The rectangle would have no area: move the point off the first one's lines.");
    }
    break;
  case Sketch3dTool::RectangleThreePoint:
    if (placed == 1 && distance(first, ontoPlane(at, first, plane)) <= kTiny) {
      return tr("The first side would have no length.");
    }
    if (placed == 2 && !rectangleCorners(RectangleMode::ThreePoint, clicks, plane)) {
      return tr("The rectangle would have no area: move the point off the first side's line.");
    }
    break;
  case Sketch3dTool::PolygonEdge:
    if (placed == 1 && distance(first, ontoPlane(at, first, plane)) <= kTiny) {
      return tr("The edge would have no length.");
    }
    break;
  default:
    break;
  }
  return QString();
}

QString Sketch3dController::shapeHint() const {
  const std::size_t placed = m_pending.size();
  // "XY", or on the rotated plane (mitcad#1661) "rotated XY".
  const QString plane = m_place.rotated() ? tr("rotated %1").arg(planeLabel(m_place.plane)) : planeLabel(m_place.plane);
  switch (m_tool) {
  case Sketch3dTool::CircleCenter:
    return placed == 0 ? tr("Center Diameter Circle: click its center; it lies on the %1 plane.").arg(plane)
                       : tr("Center Diameter Circle: click a point on it, or type the radius.");
  case Sketch3dTool::CircleTwoPoint:
    return placed == 0 ? tr("2-Point Circle: click one end of a diameter; it lies on the %1 plane.").arg(plane)
                       : tr("2-Point Circle: click the other end, or type the diameter.");
  case Sketch3dTool::CircleThreePoint:
    return placed == 0   ? tr("3-Point Circle: click a first point; it lies on the %1 plane.").arg(plane)
           : placed == 1 ? tr("3-Point Circle: click a second point.")
                         : tr("3-Point Circle: click a third point.");
  case Sketch3dTool::Ellipse:
    return placed == 0   ? tr("Ellipse: click the center; it lies on the %1 plane.").arg(plane)
           : placed == 1 ? tr("Ellipse: click the end of the major axis, or type the major radius.")
                         : tr("Ellipse: click a point on the ellipse, or type the minor radius.");
  case Sketch3dTool::RectangleTwoPoint:
    return placed == 0 ? tr("Rectangle: click the first corner; it lies on the %1 plane.").arg(plane)
                       : tr("Rectangle: click the opposite corner, or type the width (Tab: height).");
  case Sketch3dTool::RectangleCenter:
    return placed == 0 ? tr("Center Rectangle: click the center; it lies on the %1 plane.").arg(plane)
                       : tr("Center Rectangle: click a corner, or type the width (Tab: height).");
  case Sketch3dTool::RectangleThreePoint:
    return placed == 0   ? tr("3-Point Rectangle: click the first corner; it lies on the %1 plane.").arg(plane)
           : placed == 1 ? tr("3-Point Rectangle: click the end of the first side, or type the width.")
                         : tr("3-Point Rectangle: click the opposite side, or type the height.");
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
    return placed == 0 ? tr("%1: click the center; it lies on the %2 plane.").arg(toolName(), plane)
                       : tr("%1: click the size, or type the radius (Tab: sides).").arg(toolName());
  case Sketch3dTool::PolygonEdge:
    return placed == 0   ? tr("Edge Polygon: click the start of an edge; it lies on the %1 plane.").arg(plane)
           : placed == 1 ? tr("Edge Polygon: click the end of the edge (Tab: sides).")
                         : tr("Edge Polygon: click the side the polygon is on.");
  case Sketch3dTool::Text:
    return placed == 0 ? tr("Text: click where the text starts on the %1 plane, or drag its frame.").arg(plane)
                       : tr("Text: type the text (Tab: height) and press Enter.");
  default:
    break;
  }
  return QString();
}

void Sketch3dController::addShapePreview(Sketch3dPreview& out, const Xyz& at) const {
  using Style = Sketch3dPreview::Style;
  const ShapePlane plane = shapePlane();
  if (m_pending.empty()) {
    if (m_tool == Sketch3dTool::Text && m_textDragging && m_textPress) {
      // A text's frame being dragged: from the press to the pointer in the
      // plane through the press, as the planar Text tool shows it.
      const Xyz from = *m_textPress;
      const std::vector<Xyz> frame{from, ontoPlane(at, from, plane)};
      out.placed.push_back(from);
      if (const auto corners = rectangleCorners(RectangleMode::TwoPoint, frame, plane)) {
        Sketch3dPreview::Path p;
        p.points = {(*corners)[0], (*corners)[1], (*corners)[2], (*corners)[3]};
        p.style = Style::Construction;
        p.closed = true;
        out.paths.push_back(std::move(p));
      }
    }
    return;
  }
  const Style draw = m_construction ? Style::Construction : Style::Draw;
  const auto path = [&out](std::vector<Xyz> points, Style style, bool closed) {
    Sketch3dPreview::Path p;
    p.points = std::move(points);
    p.style = style;
    p.closed = closed;
    out.paths.push_back(std::move(p));
  };
  const Xyz first = m_pending.front().at;
  std::vector<Xyz> clicks;
  for (const PlacedPoint& p : m_pending) {
    clicks.push_back(p.at);
  }
  clicks.push_back(at);
  switch (m_tool) {
  case Sketch3dTool::CircleCenter:
    path({first, at}, Style::Guide, false);
    path(circlePoints(first, distance(first, ontoPlane(at, first, plane)), plane, 72), draw, false);
    break;
  case Sketch3dTool::CircleTwoPoint:
    if (const auto circle = twoPointCircle(first, at, plane)) {
      path(circlePoints(circle->center, circle->radius, plane, 72), draw, false);
    }
    break;
  case Sketch3dTool::CircleThreePoint:
    if (clicks.size() == 3) {
      if (const auto circle = threePointCircle(clicks[0], clicks[1], clicks[2], plane)) {
        path(circlePoints(circle->center, circle->radius, plane, 72), draw, false);
      }
    } else {
      path({first, at}, Style::Guide, false);
    }
    break;
  case Sketch3dTool::Ellipse:
    if (clicks.size() == 2) {
      path({first, at}, Style::Guide, false);
    } else {
      path({first, m_pending[1].at}, Style::Guide, false);
      path(ellipsePoints(first, m_pending[1].at, ellipseMinor(first, m_pending[1].at, at, plane), plane, 72), draw,
           false);
    }
    break;
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
  case Sketch3dTool::RectangleThreePoint:
    if (const auto corners = rectangleCorners(rectangleMode(m_tool), clicks, plane)) {
      path({(*corners)[0], (*corners)[1], (*corners)[2], (*corners)[3]}, draw, true);
    } else {
      path({first, at}, draw, false);
    }
    break;
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
    path(polygonCorners(first, at, sides(), m_tool == Sketch3dTool::PolygonInscribed, plane), draw, true);
    break;
  case Sketch3dTool::PolygonEdge:
    if (clicks.size() == 2) {
      path({first, at}, draw, false);
    } else if (const auto center = edgePolygonCenter(clicks[0], clicks[1], at, sides(), plane)) {
      path(polygonCorners(*center, first, sides(), true, plane), draw, true);
    }
    break;
  case Sketch3dTool::Text:
    if (m_textFrame) {
      // The frame dragged, as it was drawn while dragged: the construction
      // rectangle the text fills.
      std::vector<Xyz> frame{first, *m_textFrame};
      if (const auto corners = rectangleCorners(RectangleMode::TwoPoint, frame, plane)) {
        path({(*corners)[0], (*corners)[1], (*corners)[2], (*corners)[3]}, Style::Construction, true);
      }
    }
    break;
  default:
    break;
  }
}

void Sketch3dController::commitShape() {
  const ShapePlane plane = shapePlane();
  const std::vector<PlacedPoint> clicks = m_pending;
  if (clicks.empty()) {
    return;
  }
  const QString previous = m_anchorPoint;
  const QString name = m_model.name;
  const auto expression = [](const TypedValue& value) {
    return value.edited && value.valid ? value.expression : QString();
  };
  QJsonObject cmd;
  QString label;
  // The snaps' attachments and relations of the shape's clicks, once the
  // model has named the points they became.
  std::function<QJsonArray(const QJsonObject&)> then;
  switch (m_tool) {
  case Sketch3dTool::CircleCenter:
  case Sketch3dTool::CircleTwoPoint:
  case Sketch3dTool::CircleThreePoint: {
    PlacedPoint center;
    double radius = 0.0;
    QStringList through;
    if (m_tool == Sketch3dTool::CircleCenter) {
      center = clicks[0];
      radius = distance(clicks[0].at, ontoPlane(clicks[1].at, clicks[0].at, plane));
      if (const QString rim = existingPoint(clicks[1]); !rim.isEmpty()) {
        through << rim;
      }
    } else {
      const auto circle = m_tool == Sketch3dTool::CircleTwoPoint
                              ? twoPointCircle(clicks[0].at, clicks[1].at, plane)
                              : threePointCircle(clicks[0].at, clicks[1].at, clicks[2].at, plane);
      if (!circle) {
        return;
      }
      center.at = circle->center;
      radius = circle->radius;
      for (const PlacedPoint& click : clicks) {
        if (const QString id = existingPoint(click); !id.isEmpty() && !through.contains(id)) {
          through << id;
        }
      }
    }
    // A typed radius (or a 2-point circle's diameter) drives it, as the
    // planar tools dimension a typed size.
    const bool diameter = m_tool == Sketch3dTool::CircleTwoPoint;
    const QString typedSize = m_tool == Sketch3dTool::CircleCenter ? expression(m_radius)
                              : diameter                           ? expression(m_size)
                                                                   : QString();
    cmd = circleCommand(m_uid, center, radius, plane, typedSize, diameter, through, m_construction);
    label = QStringLiteral("Add Circle in %1").arg(name);
    if (m_tool == Sketch3dTool::CircleCenter) {
      then = [this, clicks, previous](const QJsonObject& made) {
        const QString id = existingPoint(clicks[0]).isEmpty() ? strings(made.value(QStringLiteral("points"))).value(0)
                                                             : existingPoint(clicks[0]);
        return followUps(m_uid, {clicks[0]}, {id}, previous, true);
      };
    }
    break;
  }
  case Sketch3dTool::Ellipse: {
    const double minor = typed(m_size2).value_or(ellipseMinor(clicks[0].at, clicks[1].at, clicks[2].at, plane));
    cmd = ellipseCommand(m_uid, clicks[0], clicks[1], minor, plane, expression(m_size), expression(m_size2),
                         m_construction);
    label = QStringLiteral("Add Ellipse in %1").arg(name);
    then = [this, clicks, previous](const QJsonObject& made) {
      // The centre and the major point the command made (new ones in
      // order), and a rim click on an existing point held on the ellipse.
      const QStringList fresh = strings(made.value(QStringLiteral("points")));
      qsizetype next = 0;
      QStringList ids;
      for (int k = 0; k < 2; ++k) {
        const QString existing = existingPoint(clicks[static_cast<std::size_t>(k)]);
        ids << (existing.isEmpty() ? fresh.value(next++) : existing);
      }
      QJsonArray steps = followUps(m_uid, {clicks[0], clicks[1]}, ids, previous, true);
      const QString rim = existingPoint(clicks[2]);
      const QString ellipse = strings(made.value(QStringLiteral("made"))).value(0);
      if (!rim.isEmpty() && !ellipse.isEmpty()) {
        steps.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_constraint")},
                                 {QStringLiteral("sketch"), m_uid},
                                 {QStringLiteral("constraint"), QJsonObject{{QStringLiteral("type"), QStringLiteral("point_on")},
                                                                            {QStringLiteral("point"), rim},
                                                                            {QStringLiteral("curve"), ellipse}}}});
      }
      return steps;
    };
    break;
  }
  case Sketch3dTool::RectangleTwoPoint:
  case Sketch3dTool::RectangleCenter:
  case Sketch3dTool::RectangleThreePoint: {
    const RectangleMode mode = rectangleMode(m_tool);
    cmd = rectangleCommand(m_uid, mode, clicks, plane, expression(m_size), expression(m_size2), m_construction);
    label = QStringLiteral("Add Rectangle in %1").arg(name);
    std::vector<Xyz> at;
    for (const PlacedPoint& click : clicks) {
      at.push_back(click.at);
    }
    const std::vector<int> corners = rectangleClickCorners(mode, at, plane);
    then = [this, clicks, corners, previous, mode](const QJsonObject& made) {
      const QStringList made4 = strings(made.value(QStringLiteral("corners")));
      std::vector<PlacedPoint> placed;
      QStringList ids;
      for (std::size_t i = 0; i < clicks.size() && i < corners.size(); ++i) {
        QString id;
        if (corners[i] >= 0) {
          id = made4.value(corners[i]);
        } else if (i == 0 && mode == RectangleMode::Center) {
          id = made.value(QStringLiteral("center")).toString();
        }
        if (!id.isEmpty()) {
          placed.push_back(clicks[i]);
          ids << id;
        }
      }
      return followUps(m_uid, placed, ids, previous, true);
    };
    break;
  }
  case Sketch3dTool::PolygonCircumscribed:
  case Sketch3dTool::PolygonInscribed:
  case Sketch3dTool::PolygonEdge: {
    const bool edge = m_tool == Sketch3dTool::PolygonEdge;
    const bool inscribed = m_tool != Sketch3dTool::PolygonCircumscribed;
    PlacedPoint center;
    PlacedPoint vertex;
    if (edge) {
      const auto c = edgePolygonCenter(clicks[0].at, clicks[1].at, clicks[2].at, sides(), plane);
      if (!c) {
        return;
      }
      center.at = *c;
      vertex = clicks[0];
    } else {
      center = clicks[0];
      vertex = clicks[1];
    }
    cmd = polygonCommand(m_uid, center, vertex, sides(), inscribed, plane, edge ? QString() : expression(m_radius),
                         m_construction);
    label = QStringLiteral("Add Polygon in %1").arg(name);
    then = [this, clicks, previous, edge, inscribed](const QJsonObject& made) {
      // The centre's and the first corner's snaps.
      std::vector<PlacedPoint> placed;
      QStringList ids;
      const QString first = strings(made.value(QStringLiteral("corners"))).value(0);
      if (edge) {
        placed.push_back(clicks[0]);
        ids << first;
      } else {
        placed.push_back(clicks[0]);
        ids << made.value(QStringLiteral("center")).toString();
        if (inscribed) {
          placed.push_back(clicks[1]);
          ids << first;
        }
      }
      return followUps(m_uid, placed, ids, previous, true);
    };
    break;
  }
  case Sketch3dTool::Text: {
    if (!m_textField.edited || m_textField.text.isEmpty()) {
      m_host.showHint(tr("Text: type the text first."));
      return;
    }
    const double height = typed(m_textHeight).value_or(kDefaultTextHeight);
    cmd = textCommand(m_uid, m_textField.text, clicks[0].at, m_textFrame, plane, height);
    label = QStringLiteral("Add Text in %1").arg(name);
    break;
  }
  default:
    return;
  }
  QJsonObject result;
  if (!commitSteps(cmd, then, label, &result)) {
    m_pending.pop_back(); // the last click can be placed again
    updateHover();
    return;
  }
  const QStringList made = strings(result.value(QStringLiteral("made")));
  const QString texts = strings(result.value(QStringLiteral("texts"))).join(QStringLiteral(", "));
  toolLog(QStringLiteral("3D sketch %1: made %2 on the %3 plane through %4")
              .arg(toolName(), m_tool == Sketch3dTool::Text ? texts : made.join(QStringLiteral(", ")),
                   (plane.rotated() ? QStringLiteral("rotated ") : QString()) + planeLabel(plane.plane),
                   xyz(clicks[0].at)));
  // The anchor stays at the shape's first point.
  QString anchor = existingPoint(clicks[0]);
  if (anchor.isEmpty()) {
    const QString center = result.value(QStringLiteral("center")).toString();
    const QStringList corners = strings(result.value(QStringLiteral("corners")));
    const QStringList points = strings(result.value(QStringLiteral("points")));
    anchor = !center.isEmpty() ? center : !corners.isEmpty() ? corners.first() : points.value(0);
  }
  m_pending.clear();
  if (m_tool == Sketch3dTool::Text) {
    m_textField.clear();
    m_textFrame.reset();
  }
  afterCommit(anchor);
}

} // namespace mitcad::sketch
