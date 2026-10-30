// SPDX-License-Identifier: MIT
#include "SketchOverlay.hpp"

#include "../OcctViewer.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <map>
#include <utility>
#include <vector>

#include <QDebug>
#include <QFontMetricsF>
#include <QPaintDevice>
#include <QPainter>
#include <QPainterPath>
#include <QRegularExpression>
#include <QSet>

#include "PointMarkers.hpp"
#include "SketchController.hpp"
#include "SketchTool.hpp"

namespace mitcad::sketch {
namespace {

const QColor kDimensionColor(0x2a, 0x30, 0x3a);
const QColor kDrivenColor(0x70, 0x76, 0x80);
// The lines of the two above on a dark background (their values stay dark
// in their light boxes).
const QColor kDimensionColorDark(0xd8, 0xde, 0xe8);
const QColor kDrivenColorDark(0xa0, 0xa8, 0xb4);
// Inactive constraints and dimensions (mitcad#721): pink, apart from the
// conflict red and the driven grey; the lines lighter on a dark background.
const QColor kInactiveColor(0xd6, 0x3c, 0x8f);
const QColor kInactiveColorDark(0xf2, 0x9c, 0xc8);
const QColor kInactiveFill(0xfd, 0xe4, 0xf1, 0xe6);
const QColor kSelectedColor(0x1f, 0x7a, 0xff);
const QColor kHoverColor(0x5a, 0xa8, 0xff);
const QColor kPreviewColor(0x26, 0x6e, 0xe8);
const QColor kConstructionPreview(0xd9, 0x80, 0x1a);
const QColor kRemoveColor(0xe0, 0x30, 0x20);
const QColor kGuideColor(0x80, 0x88, 0x96);
const QColor kSnapColor(0xe0, 0x7b, 0x1a);
const QColor kPickColor(0x2c, 0xa0, 0x2c);
// The sketch's axes and origin (mitcad#254): the origin axes' colour, the
// lines across the view half transparent, light enough on any background.
const QColor kAxisColor(0xd9, 0x73, 0x1a);
const QColor kAxisLineColor(0xd9, 0x73, 0x1a, 0x90);
// A sketch origin this near the design's (mm) is the design's.
constexpr double kOriginTolerance = 1e-6;

constexpr double kGlyphSize = 15.0;
constexpr double kGlyphStep = 17.0;
constexpr double kArrowLength = 8.0;

// The halo under the dimensions' lines and arrowheads (mitcad#633): 3 px
// under the 1 px lines, and a 2 px outline round the arrowheads. Thinner
// than the geometry's 6 px halo (mitcad#302), so that the dimensions stay
// lighter than the geometry.
constexpr double kDimensionHaloWidth = 3.0;
constexpr double kArrowHaloWidth = 2.0;

QPointF unitOf(const QPointF& p) {
  const double n = std::hypot(p.x(), p.y());
  return n > 1e-12 ? QPointF(p.x() / n, p.y() / n) : QPointF(1, 0);
}

// The arrowhead with its tip at `tip`, pointing along `towards`.
QPolygonF arrowHead(const QPointF& tip, const QPointF& towards) {
  const QPointF dir = unitOf(towards);
  const QPointF side(-dir.y(), dir.x());
  QPolygonF head;
  head << tip << tip - dir * kArrowLength + side * (kArrowLength * 0.32)
       << tip - dir * kArrowLength - side * (kArrowLength * 0.32);
  return head;
}

// A dimension's lines and arrow tips moved by less than a device pixel so
// that the 1 px lines cover whole device pixels (mitcad#633). Painted
// anti-aliased at a whole pixel, a level line straddles two rows at half
// its tone and its 3 px halo spreads over four: the line blurs into its
// halo and shows in neither its own tone nor the halo's. Aligned, a level
// or upright line takes one row (two at a device pixel ratio of 2) between
// whole rows of its halo; the halo and the line get the same points, so
// the line stays in the halo's middle. Oblique lines are anti-aliased
// anyway and move by less than a pixel.
DimensionShape onDevicePixels(const DimensionShape& shape, const QPainter& painter) {
  const double ratio = painter.device() != nullptr ? painter.device()->devicePixelRatioF() : 1.0;
  // The line, `ratio` device pixels wide, from a whole device pixel.
  const auto aligned = [ratio](double v) {
    return (std::floor(v * ratio - ratio / 2.0 + 0.5) + ratio / 2.0) / ratio;
  };
  const auto point = [&aligned](const QPointF& p) { return QPointF(aligned(p.x()), aligned(p.y())); };
  DimensionShape out = shape;
  for (QLineF& line : out.lines) {
    line = QLineF(point(line.p1()), point(line.p2()));
  }
  for (auto& arrow : out.arrows) {
    arrow.first = point(arrow.first);
  }
  return out;
}

// Whether `rect`, with a 2 px margin, holds a point of `outline` (a single
// point, or a polyline) or crosses one of its segments.
bool rectTouches(const QRectF& rect, const std::vector<QPointF>& outline) {
  const QRectF area = rect.adjusted(-2, -2, 2, 2);
  for (const QPointF& p : outline) {
    if (area.contains(p)) {
      return true;
    }
  }
  const QLineF sides[] = {QLineF(area.topLeft(), area.topRight()), QLineF(area.topRight(), area.bottomRight()),
                          QLineF(area.bottomRight(), area.bottomLeft()), QLineF(area.bottomLeft(), area.topLeft())};
  for (std::size_t i = 1; i < outline.size(); ++i) {
    const QLineF segment(outline[i - 1], outline[i]);
    for (const QLineF& side : sides) {
      if (segment.intersects(side, nullptr) == QLineF::BoundedIntersection) {
        return true;
      }
    }
  }
  return false;
}

// The part of the line through `at` along `direction` (not zero) inside
// `area`, from its end against the direction to the other.
std::optional<QLineF> lineAcross(const QPointF& at, const QPointF& direction, const QRectF& area) {
  double low = -std::numeric_limits<double>::infinity();
  double high = std::numeric_limits<double>::infinity();
  const auto clip = [&low, &high](double from, double along, double first, double last) {
    if (std::abs(along) < 1e-12) {
      return from >= first && from <= last;
    }
    double t0 = (first - from) / along;
    double t1 = (last - from) / along;
    if (t0 > t1) {
      std::swap(t0, t1);
    }
    low = std::max(low, t0);
    high = std::min(high, t1);
    return low < high;
  };
  if (!clip(at.x(), direction.x(), area.left(), area.right()) ||
      !clip(at.y(), direction.y(), area.top(), area.bottom())) {
    return std::nullopt;
  }
  return QLineF(at + direction * low, at + direction * high);
}

// Points of a dimension's model geometry.
struct Resolver {
  const SketchModel& model;

  std::optional<V2> point(const QJsonObject& d, const char* key) const {
    return model.pointAt(d.value(QLatin1String(key)).toString());
  }
  const CurveData* curve(const QJsonObject& d, const char* key) const {
    return model.curve(d.value(QLatin1String(key)).toString());
  }
};

bool plainNumber(const QString& expression) {
  static const QRegularExpression number(
      QStringLiteral(R"(^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*(mm|cm|m|um|in|ft|deg|rad|°)?\s*$)"));
  return expression.isEmpty() || number.match(expression).hasMatch();
}

// The two points a linear dimension measures between, and its direction.
// `lineB`: the line `b` is the foot on, which may lie beyond its ends; the
// extension line then starts from the line's nearest end.
struct Linear {
  V2 a, b, u;
  const Curve2* lineB = nullptr;
};

// The line a dimension's extension line starts on: none for the line that
// stands for a sketch axis (mitcad#500), which is drawn across the view, so
// that the extension starts at the foot on it.
const Curve2* lineOf(const CurveData& line) { return line.axis.isEmpty() ? &line.curve : nullptr; }

std::optional<Linear> linearOf(const DimensionData& d, const SketchModel& model, const V2& text) {
  const Resolver r{model};
  const QString& t = d.type;
  if (t == QStringLiteral("distance") || t == QStringLiteral("horizontal_distance") ||
      t == QStringLiteral("vertical_distance")) {
    const auto a = r.point(d.json, "a");
    const auto b = r.point(d.json, "b");
    if (!a || !b) {
      return std::nullopt;
    }
    V2 u = unit(*b - *a);
    if (t == QStringLiteral("horizontal_distance")) {
      u = {1, 0};
    } else if (t == QStringLiteral("vertical_distance")) {
      u = {0, 1};
    }
    return Linear{*a, *b, u};
  }
  if (t == QStringLiteral("length")) {
    const CurveData* line = r.curve(d.json, "line");
    if (line == nullptr || !line->isLine()) {
      return std::nullopt;
    }
    return Linear{line->curve.a, line->curve.b, unit(line->curve.b - line->curve.a)};
  }
  if (t == QStringLiteral("point_line_distance")) {
    const auto p = r.point(d.json, "point");
    const CurveData* line = r.curve(d.json, "line");
    if (!p || line == nullptr || !line->isLine()) {
      return std::nullopt;
    }
    const V2 dir = unit(line->curve.b - line->curve.a);
    const V2 foot = line->curve.a + dir * dot(*p - line->curve.a, dir);
    const V2 u = dist(foot, *p) > 1e-9 ? unit(foot - *p) : perp(dir);
    return Linear{*p, foot, u, lineOf(*line)};
  }
  if (t == QStringLiteral("line_distance")) {
    const CurveData* a = r.curve(d.json, "a");
    const CurveData* b = r.curve(d.json, "b");
    if (a == nullptr || b == nullptr || !a->isLine() || !b->isLine()) {
      return std::nullopt;
    }
    const V2 p = a->curve.nearest(text);
    const V2 dir = unit(b->curve.b - b->curve.a);
    const V2 foot = b->curve.a + dir * dot(p - b->curve.a, dir);
    const V2 u = dist(foot, p) > 1e-9 ? unit(foot - p) : perp(dir);
    return Linear{p, foot, u, lineOf(*b)};
  }
  if (t == QStringLiteral("tangent_distance")) {
    const CurveData* circle = r.curve(d.json, "circle");
    const CurveData* line = r.curve(d.json, "line");
    if (circle == nullptr || line == nullptr || !circle->isCircular() || !line->isLine()) {
      return std::nullopt;
    }
    // From the circle's nearest point to the line, along the perpendicular.
    const V2 dir = unit(line->curve.b - line->curve.a);
    const V2 centre = circle->curve.center;
    const V2 foot = line->curve.a + dir * dot(centre - line->curve.a, dir);
    const V2 towards = dist(foot, centre) > 1e-9 ? unit(foot - centre) : perp(dir);
    return Linear{centre + towards * circle->curve.radius, foot, towards, lineOf(*line)};
  }
  if (t == QStringLiteral("concentric_distance")) {
    // Between the rims along the radius toward the value (mitcad#445).
    const CurveData* a = r.curve(d.json, "a");
    const CurveData* b = r.curve(d.json, "b");
    if (a == nullptr || b == nullptr || !a->isCircular() || !b->isCircular()) {
      return std::nullopt;
    }
    const V2 centre = a->curve.center;
    const V2 u = dist(text, centre) > 1e-9 ? unit(text - centre) : V2{1, 0};
    return Linear{centre + u * a->curve.radius, b->curve.center + u * b->curve.radius, u};
  }
  if (t == QStringLiteral("linear_diameter")) {
    const CurveData* axis = r.curve(d.json, "axis");
    const QString entity = d.json.value(QStringLiteral("entity")).toString();
    if (axis == nullptr || !axis->isLine()) {
      return std::nullopt;
    }
    std::optional<V2> p = model.pointAt(entity);
    if (!p) {
      if (const CurveData* other = model.curve(entity); other != nullptr && other->isLine()) {
        p = other->curve.nearest(text);
      }
    }
    if (!p) {
      return std::nullopt;
    }
    const V2 dir = unit(axis->curve.b - axis->curve.a);
    const V2 foot = axis->curve.a + dir * dot(*p - axis->curve.a, dir);
    const V2 mirrored = foot * 2.0 - *p;
    const V2 u = dist(mirrored, *p) > 1e-9 ? unit(mirrored - *p) : perp(dir);
    return Linear{*p, mirrored, u};
  }
  if (t == QStringLiteral("spacing")) {
    // An equal spacing's common spacing (mitcad#221), drawn on its first
    // interval as the constraint measures it.
    const ConstraintData* k = model.constraint(d.json.value(QStringLiteral("constraint")).toString());
    if (k == nullptr || k->refs.size() < 2) {
      return std::nullopt;
    }
    const auto a = model.anchorAt(k->refs.at(0));
    const auto b = model.anchorAt(k->refs.at(1));
    if (!a || !b) {
      return std::nullopt;
    }
    const QString mode = k->json.value(QStringLiteral("mode")).toString();
    V2 u = dist(*a, *b) > 1e-9 ? unit(*b - *a) : V2{1, 0};
    if (mode == QStringLiteral("horizontal")) {
      u = {1, 0};
    } else if (mode == QStringLiteral("vertical")) {
      u = {0, 1};
    } else if (mode == QStringLiteral("along")) {
      const CurveData* line = model.curve(k->json.value(QStringLiteral("direction")).toString());
      if (line != nullptr && line->isLine()) {
        u = unit(line->curve.b - line->curve.a);
      }
    }
    return Linear{*a, *b, u};
  }
  return std::nullopt;
}

// Whether an angle dimension says which rays it measures; one stored
// without (before mitcad#296) measures the angle closer to its value.
bool hasSide(const DimensionData& d) {
  return d.json.contains(QStringLiteral("reverse_a")) || d.json.contains(QStringLiteral("reverse_b"));
}

} // namespace

std::optional<AngleRays> angleRays(const DimensionData& d, const SketchModel& model, bool byValue) {
  const Resolver r{model};
  const CurveData* a = r.curve(d.json, "a");
  const CurveData* b = r.curve(d.json, "b");
  if (a == nullptr || b == nullptr || !a->isLine() || !b->isLine()) {
    return std::nullopt;
  }
  const V2 da = unit(a->curve.b - a->curve.a);
  const V2 db = unit(b->curve.b - b->curve.a);
  const double c = cross(da, db);
  if (std::abs(c) < 1e-9) {
    return std::nullopt;
  }
  const double t = cross(b->curve.a - a->curve.a, db) / c;
  const V2 corner = a->curve.a + da * t;
  if (hasSide(d)) {
    const double sa = d.json.value(QStringLiteral("reverse_a")).toBool() ? -1.0 : 1.0;
    const double sb = d.json.value(QStringLiteral("reverse_b")).toBool() ? -1.0 : 1.0;
    return AngleRays{corner, da * sa, db * sb};
  }
  if (!byValue) {
    return AngleRays{corner, da, db};
  }
  const double between = std::acos(std::clamp(dot(da, db), -1.0, 1.0));
  const bool supplement = std::abs(kPi - between - d.measured) < std::abs(between - d.measured);
  AngleRays rays{corner, da, supplement ? -db : db};
  if (d.text && dot(rays.da + rays.db, *d.text - corner) < 0.0) {
    rays.da = -rays.da;
    rays.db = -rays.db;
  }
  return rays;
}

std::optional<double> measure(const DimensionData& d, const SketchModel& model) {
  const Resolver r{model};
  const QString& t = d.type;
  if (const auto linear = linearOf(d, model, V2{})) {
    if (t == QStringLiteral("line_distance")) {
      const CurveData* a = r.curve(d.json, "a");
      const CurveData* b = r.curve(d.json, "b");
      const V2 dir = unit(b->curve.b - b->curve.a);
      return std::abs(cross(dir, a->curve.a - b->curve.a));
    }
    // A signed horizontal or vertical distance: the offset itself
    // (mitcad#404).
    const double offset = dot(linear->b - linear->a, linear->u);
    return d.json.value(QStringLiteral("signed")).toBool() ? offset : std::abs(offset);
  }
  if (t == QStringLiteral("angle")) {
    // As the model measures it: without a side, between the directions.
    if (const auto rays = angleRays(d, model, false)) {
      return std::acos(std::clamp(dot(rays->da, rays->db), -1.0, 1.0));
    }
    return std::nullopt;
  }
  if (t == QStringLiteral("radius") || t == QStringLiteral("diameter")) {
    const CurveData* c = r.curve(d.json, "curve");
    if (c == nullptr || !c->isCircular()) {
      return std::nullopt;
    }
    return t == QStringLiteral("radius") ? c->curve.radius : 2.0 * c->curve.radius;
  }
  if (t == QStringLiteral("arc_length")) {
    const CurveData* c = r.curve(d.json, "arc");
    if (c == nullptr || !c->isArc()) {
      return std::nullopt;
    }
    return c->curve.radius * sweep(c->curve.start, c->curve.end);
  }
  if (t == QStringLiteral("major_radius") || t == QStringLiteral("minor_radius")) {
    const CurveData* c = r.curve(d.json, "ellipse");
    if (c == nullptr || !c->isEllipse()) {
      return std::nullopt;
    }
    return t == QStringLiteral("major_radius") ? c->curve.radius : c->curve.minor;
  }
  return std::nullopt;
}

V2 defaultTextPosition(const DimensionData& d, const SketchModel& model, double pixel) {
  const Resolver r{model};
  const QString& t = d.type;
  const double off = 26.0 * pixel;
  if (t == QStringLiteral("concentric_distance")) {
    // On the radius at 45 degrees, between the rims.
    const CurveData* a = r.curve(d.json, "a");
    const CurveData* b = r.curve(d.json, "b");
    if (a != nullptr && b != nullptr && a->isCircular() && b->isCircular()) {
      return a->curve.center + polar(kPi / 4.0, (a->curve.radius + b->curve.radius) / 2.0);
    }
  }
  if (const auto linear = linearOf(d, model, V2{})) {
    const V2 middle = (linear->a + linear->b) / 2.0;
    if (t == QStringLiteral("horizontal_distance")) {
      return {middle.x, std::max(linear->a.y, linear->b.y) + off};
    }
    if (t == QStringLiteral("vertical_distance")) {
      return {std::max(linear->a.x, linear->b.x) + off, middle.y};
    }
    V2 n = perp(linear->u);
    if (t == QStringLiteral("length") || t == QStringLiteral("distance")) {
      // Outside a closed shape is usually away from the sketch's middle.
      V2 centre;
      for (const PointData& p : model.points) {
        centre = centre + p.at;
      }
      if (!model.points.empty()) {
        centre = centre / static_cast<double>(model.points.size());
      }
      if (dot(middle - centre, n) < 0.0) {
        n = -n;
      }
      return middle + n * off;
    }
    return middle + n * off;
  }
  if (t == QStringLiteral("angle")) {
    if (const auto rays = angleRays(d, model)) {
      return rays->corner + unit(rays->da + rays->db) * (60.0 * pixel);
    }
  }
  const CurveData* c = r.curve(d.json, t == QStringLiteral("arc_length") ? "arc"
                                      : t.endsWith(QStringLiteral("radius")) &&
                                              t != QStringLiteral("radius")
                                          ? "ellipse"
                                          : "curve");
  if (c != nullptr) {
    if (c->isArc()) {
      const double middle = c->curve.start + sweep(c->curve.start, c->curve.end) / 2.0;
      return c->curve.center + polar(middle, c->curve.radius + off);
    }
    if (c->isEllipse()) {
      const double angle = c->curve.rotation + (t == QStringLiteral("minor_radius") ? kPi / 2 : 0.0);
      const double length = t == QStringLiteral("minor_radius") ? c->curve.minor : c->curve.radius;
      return c->curve.center + polar(angle, length / 2.0) + perp(polar(angle)) * (14.0 * pixel);
    }
    return c->curve.center + polar(kPi / 4.0, c->curve.radius + off);
  }
  return {};
}

DimensionShape dimensionShape(const DimensionData& d, const SketchModel& model,
                              const Projection& projection, const QString& text) {
  DimensionShape shape;
  shape.text = text;
  const double px = projection.pixel;
  const V2 at = d.text ? *d.text : defaultTextPosition(d, model, px);
  const auto& S = projection.toScreen;
  shape.textAt = S(at);
  const Resolver r{model};
  const QString& t = d.type;

  const auto arrow = [&](const V2& tip, const V2& towards) {
    shape.arrows.emplace_back(S(tip), S(tip + towards * px) - S(tip));
  };

  if (const auto linear = linearOf(d, model, at)) {
    const V2 u = linear->u;
    const V2 n = perp(u);
    const double s1 = dot(at - linear->a, n);
    const double s2 = dot(at - linear->b, n);
    const V2 d1 = linear->a + n * s1;
    const V2 d2 = linear->b + n * s2;
    const auto extension = [&](const V2& from, const V2& to) {
      const double s = dot(to - from, n);
      if (std::abs(s) < 3.0 * px) {
        return;
      }
      const double sign = s > 0 ? 1.0 : -1.0;
      shape.lines.emplace_back(S(from + n * (sign * 3.0 * px)), S(to + n * (sign * 5.0 * px)));
    };
    extension(linear->a, d1);
    extension(linear->lineB != nullptr ? linear->lineB->nearest(d2) : linear->b, d2);
    shape.lines.emplace_back(S(d1), S(d2));
    // A value placed beyond the ends gets the dimension line to it.
    const double along = dot(at - d1, unit(d2 - d1));
    const double length = dist(d1, d2);
    if (length > 1e-9) {
      const V2 dir = unit(d2 - d1);
      if (along < 0.0) {
        shape.lines.emplace_back(S(d1 + dir * along), S(d1));
      } else if (along > length) {
        shape.lines.emplace_back(S(d2), S(d1 + dir * along));
      }
      arrow(d1, -dir);
      arrow(d2, dir);
    }
    shape.valid = true;
    return shape;
  }
  if (t == QStringLiteral("angle")) {
    const auto rays = angleRays(d, model);
    if (!rays) {
      return shape;
    }
    const double radius = std::max(dist(at, rays->corner), 12.0 * px);
    const double start = angleOf(rays->da);
    const double span = std::atan2(cross(rays->da, rays->db), dot(rays->da, rays->db));
    QPolygonF arc;
    for (int i = 0; i <= 32; ++i) {
      arc << S(rays->corner + polar(start + span * i / 32.0, radius));
    }
    shape.arcs.push_back(arc);
    const double sign = span >= 0 ? 1.0 : -1.0;
    arrow(rays->corner + polar(start, radius), -perp(polar(start)) * sign);
    arrow(rays->corner + polar(start + span, radius), perp(polar(start + span)) * sign);
    // Extension lines from the lines to the arc where it lies beyond them.
    const CurveData* a = r.curve(d.json, "a");
    const CurveData* b = r.curve(d.json, "b");
    for (const auto& [line, dir] : {std::pair{a, rays->da}, std::pair{b, rays->db}}) {
      const V2 end = rays->corner + dir * radius;
      if (line->axis.isEmpty() && line->curve.distanceTo(end) > 2.0 * px) {
        shape.lines.emplace_back(S(line->curve.nearest(end)), S(end + dir * (4.0 * px)));
      }
    }
    shape.valid = true;
    return shape;
  }
  if (t == QStringLiteral("radius") || t == QStringLiteral("diameter")) {
    const CurveData* c = r.curve(d.json, "curve");
    if (c == nullptr || !c->isCircular()) {
      return shape;
    }
    const V2 center = c->curve.center;
    const double radius = c->curve.radius;
    const V2 dir = dist(at, center) > 1e-9 ? unit(at - center) : polar(kPi / 4.0);
    const V2 edge = center + dir * radius;
    const V2 from = t == QStringLiteral("diameter") ? center - dir * radius : center;
    shape.lines.emplace_back(S(from), S(edge));
    if (dist(at, center) > radius) {
      shape.lines.emplace_back(S(edge), S(at));
    }
    arrow(edge, dir);
    if (t == QStringLiteral("diameter")) {
      arrow(from, -dir);
    }
    if (c->isArc() && !c->curve.contains(angleOf(dir))) {
      // The arc's circle, dashed in spirit: a short extension to the value.
      QPolygonF extension;
      const double startAngle = angleOf(dir);
      const double end = std::abs(normalizeAngle(c->curve.start - startAngle)) <
                                 std::abs(normalizeAngle(startAngle - c->curve.end))
                             ? c->curve.start
                             : c->curve.end;
      double delta = std::remainder(end - startAngle, kTwoPi);
      for (int i = 0; i <= 12; ++i) {
        extension << S(center + polar(startAngle + delta * i / 12.0, radius));
      }
      shape.arcs.push_back(extension);
    }
    shape.valid = true;
    return shape;
  }
  if (t == QStringLiteral("arc_length")) {
    const CurveData* c = r.curve(d.json, "arc");
    if (c == nullptr || !c->isArc()) {
      return shape;
    }
    const double radius = std::max(dist(at, c->curve.center), 1e-6);
    const double span = d.previewSweep.value_or(sweep(c->curve.start, c->curve.end));
    QPolygonF arc;
    for (int i = 0; i <= 32; ++i) {
      arc << S(c->curve.center + polar(c->curve.start + span * i / 32.0, radius));
    }
    shape.arcs.push_back(arc);
    for (const double angle : {c->curve.start, c->curve.start + span}) {
      shape.lines.emplace_back(S(c->curve.center + polar(angle, c->curve.radius)),
                               S(c->curve.center + polar(angle, radius)));
    }
    const double sign = span >= 0.0 ? 1.0 : -1.0;
    arrow(c->curve.center + polar(c->curve.start, radius), -perp(polar(c->curve.start)) * sign);
    arrow(c->curve.center + polar(c->curve.start + span, radius), perp(polar(c->curve.start + span)) * sign);
    shape.valid = true;
    return shape;
  }
  if (t == QStringLiteral("major_radius") || t == QStringLiteral("minor_radius")) {
    const CurveData* c = r.curve(d.json, "ellipse");
    if (c == nullptr || !c->isEllipse()) {
      return shape;
    }
    const bool minor = t == QStringLiteral("minor_radius");
    const V2 dir = polar(c->curve.rotation + (minor ? kPi / 2.0 : 0.0));
    const V2 end = c->curve.center + dir * (minor ? c->curve.minor : c->curve.radius);
    shape.lines.emplace_back(S(c->curve.center), S(end));
    arrow(end, dir);
    shape.valid = true;
    return shape;
  }
  return shape;
}

SketchOverlay::SketchOverlay(SketchController& controller, QWidget* view)
    : QWidget(view), m_c(controller) {
  setObjectName(QStringLiteral("sketchOverlay"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setFocusPolicy(Qt::NoFocus);
  QFont small = font();
  small.setPointSizeF(9.0);
  setFont(small);
  connect(&m_c.viewer(), &OcctViewer::backgroundToneChanged, this, &SketchOverlay::logDimensionHalos);
}

void SketchOverlay::logDimensionHalos() {
  if (!m_c.isActive() || !m_c.showDimensions()) {
    return;
  }
  // How many dimensions have a halo, and its tone.
  const QColor halo = m_c.viewer().sketchHaloColor();
  const QString line = QStringLiteral("%1 %2")
                           .arg(dimensions().size())
                           .arg(halo.lightnessF() < 0.5 ? QStringLiteral("dark") : QStringLiteral("light"));
  if (line != m_loggedDimensionHalos) {
    m_loggedDimensionHalos = line;
    qDebug().noquote() << QStringLiteral("Sketch dimension halos: %1").arg(line);
  }
}

Projection SketchOverlay::projection() const {
  return {[this](const V2& p) { return m_c.toScreen(p); }, m_c.pixel()};
}

QString SketchOverlay::dimensionText(const DimensionData& d) const {
  QString value = d.type == QStringLiteral("angle") ? m_c.formatAngle(d.measured)
                                                    : m_c.formatLength(d.measured);
  if (d.type == QStringLiteral("radius")) {
    value = QStringLiteral("R") + value;
  } else if (d.type == QStringLiteral("diameter") || d.type == QStringLiteral("linear_diameter")) {
    value = QStringLiteral("Ø") + value;
  } else if (d.type == QStringLiteral("arc_length")) {
    value = QStringLiteral("⌒") + value;
  } else if (d.json.value(QStringLiteral("signed")).toBool() && !value.startsWith(QLatin1Char('-'))) {
    // A signed distance shows its sign, + too (mitcad#409).
    value = QStringLiteral("+") + value;
  }
  if (!d.driven && !plainNumber(d.expression)) {
    value = QStringLiteral("fx: ") + value;
  }
  // The name instead of the value, or before it (mitcad#231).
  if (!d.parameter.isEmpty()) {
    switch (m_c.dimensionTextMode()) {
    case SketchController::DimensionText::Value:
      break;
    case SketchController::DimensionText::Name:
      value = d.parameter;
      break;
    case SketchController::DimensionText::NameValue:
      value = QStringLiteral("%1 = %2").arg(d.parameter, value);
      break;
    }
  }
  return d.driven ? QStringLiteral("(%1)").arg(value) : value;
}

QString SketchOverlay::dimensionTooltip(const DimensionData& d) const {
  const bool angle = d.type == QStringLiteral("angle");
  const QString value = angle ? m_c.formatAngle(d.measured)
                              : QStringLiteral("%1 %2").arg(m_c.formatLength(d.measured), m_c.lengthUnit());
  QString text = d.parameter.isEmpty() ? QString() : d.parameter + QStringLiteral(" = ");
  if (d.driven) {
    // A reference's comment already says it (mitcad#231).
    text += d.comment.endsWith(QStringLiteral("(driven)")) ? value : value + QStringLiteral(", driven");
  } else if (!plainNumber(d.expression)) {
    text += QStringLiteral("%1 → %2").arg(d.expression, value);
  } else {
    text += value;
  }
  if (!d.comment.isEmpty()) {
    text += QStringLiteral(" (%1)").arg(d.comment);
  }
  // A limit of its parameter took effect (mitcad#665).
  if (!d.limited.isEmpty()) {
    const QString mark = d.limited == QStringLiteral("max")   ? QStringLiteral("▲max")
                         : d.limited == QStringLiteral("min") ? QStringLiteral("▼min")
                                                              : QStringLiteral("◆step");
    text += QStringLiteral(" %1\n%2").arg(mark, d.limitMessage);
  }
  // Inactive (mitcad#721): why, first.
  const QString inactive = inactiveTooltip(d.active, d.inactiveReason);
  return inactive.isEmpty() ? text : inactive + QLatin1Char('\n') + text;
}

QString SketchOverlay::inactiveTooltip(bool active, const QString& reason) {
  if (active) {
    return QString();
  }
  return reason.isEmpty() ? tr("Not applied: inactive (Activate applies it)")
                          : tr("Not applied: %1").arg(reason);
}

std::map<QString, QRectF> SketchOverlay::constraintGlyphRects() const {
  std::map<QString, QRectF> rects;
  for (const Glyph& g : glyphs()) {
    if (!g.constraint.isEmpty()) {
      rects.emplace(g.constraint, g.rect);
    }
  }
  return rects;
}

std::vector<QRectF> SketchOverlay::constraintGlyphBounds() const {
  std::vector<QRectF> bounds;
  if (m_c.showConstraints()) {
    for (const Glyph& g : glyphs()) {
      bounds.push_back(g.rect);
    }
  }
  return bounds;
}

void SketchOverlay::logInactive() {
  if (!m_c.isActive()) {
    return;
  }
  const SketchModel& model = m_c.model();
  QStringList drawn;
  if (m_c.showConstraints()) {
    for (const ConstraintData& k : model.constraints) {
      if (!k.active) {
        drawn << QStringLiteral("%1 constraint").arg(k.id);
      }
    }
  }
  if (m_c.showDimensions()) {
    for (const DimensionData& d : model.dimensions) {
      if (!d.active) {
        drawn << QStringLiteral("%1 dimension").arg(d.id);
      }
    }
  }
  const QString line = drawn.join(QStringLiteral(", "));
  if (line != m_loggedInactive) {
    m_loggedInactive = line;
    qDebug().noquote() << QStringLiteral("Sketch inactive drawn: %1").arg(line.isEmpty() ? QStringLiteral("none") : line);
  }
}

QRectF SketchOverlay::textRect(const QPointF& at, const QString& text) const {
  const QFontMetricsF metrics(font());
  const double w = metrics.horizontalAdvance(text) + 8.0;
  const double h = metrics.height() + 2.0;
  return QRectF(at.x() - w / 2.0, at.y() - h / 2.0, w, h);
}

std::vector<SketchOverlay::DimensionLayout> SketchOverlay::dimensions() const {
  std::vector<DimensionLayout> result;
  const SketchModel& model = m_c.model();
  const Projection proj = projection();
  const auto override = m_c.textOverride();
  for (const DimensionData& source : model.dimensions) {
    DimensionData d = source;
    if (override && override->first == d.id) {
      d.text = override->second;
    }
    DimensionLayout layout;
    layout.id = d.id;
    layout.shape = dimensionShape(d, model, proj, dimensionText(d));
    if (!layout.shape.valid) {
      continue;
    }
    layout.textRect = textRect(layout.shape.textAt, layout.shape.text);
    result.push_back(layout);
  }
  return result;
}

std::vector<SketchOverlay::Glyph> SketchOverlay::glyphs() const {
  const SketchModel& model = m_c.model();
  // Glyphs per entity, in order, then laid out in a row beside it.
  // One glyph per entity and type: a polygon's side equal to both its
  // neighbours shows one "=", which picks the first of them.
  // An inactive constraint (mitcad#721) has a glyph of its own beside an
  // active one of its type.
  std::vector<Glyph> all;
  const auto add = [&all](const QString& constraint, const QString& entity, const QString& type,
                          bool inactive) {
    const bool shown = std::any_of(all.begin(), all.end(), [&](const Glyph& g) {
      return g.entity == entity && g.type == type && g.inactive == inactive;
    });
    if (!shown) {
      all.push_back({constraint, entity, type, QRectF(), inactive});
    }
  };
  for (const ConstraintData& k : model.constraints) {
    const QString& t = k.type;
    const bool off = !k.active;
    if (t == QStringLiteral("coincident") || t == QStringLiteral("midpoint")) {
      add(k.id, k.json.value(QStringLiteral("point")).toString(), t, off);
    } else if (t == QStringLiteral("symmetric")) {
      add(k.id, k.json.value(QStringLiteral("a")).toString(), t, off);
      add(k.id, k.json.value(QStringLiteral("b")).toString(), t, off);
    } else if ((t == QStringLiteral("tangent") || t == QStringLiteral("smooth")) &&
               !k.json.value(QStringLiteral("at")).toString().isEmpty()) {
      // Beside the point it applies at (mitcad#783): a curve tangent at
      // both ends shows both.
      add(k.id, k.json.value(QStringLiteral("at")).toString(), t, off);
    } else {
      for (const QString& entity : k.refs) {
        add(k.id, entity, t, off);
      }
    }
  }
  const QString origin = model.originPoint();
  for (const PointData& p : model.points) {
    if (p.fixed && !p.reference && p.id != origin && p.axis.isEmpty()) {
      add(QString(), p.id, QStringLiteral("fix"), false);
    }
  }
  for (const CurveData& c : model.curves) {
    if (c.fixed && !c.reference && c.axis.isEmpty()) {
      add(QString(), c.id, QStringLiteral("fix"), false);
    }
  }

  std::vector<QRectF> dimensionRects;
  if (m_c.showDimensions()) {
    for (const DimensionLayout& d : dimensions()) {
      dimensionRects.push_back(d.textRect.adjusted(-3, -3, 3, 3));
    }
  }
  const auto distanceToClear = [&dimensionRects](const QRectF& row, const QPointF& direction) {
    double distance = 0;
    // Along a fixed direction, each move leaves at least one text box
    // behind. At most one move per box is needed, even in a dense sketch.
    for (std::size_t pass = 0; pass <= dimensionRects.size(); ++pass) {
      const QRectF at = row.translated(direction * distance);
      double move = 0;
      for (const QRectF& text : dimensionRects) {
        if (!at.intersects(text)) continue;
        double exit = std::numeric_limits<double>::infinity();
        if (direction.x() > 1e-9) exit = std::min(exit, (text.right() - at.left()) / direction.x());
        if (direction.x() < -1e-9) exit = std::min(exit, (text.left() - at.right()) / direction.x());
        if (direction.y() > 1e-9) exit = std::min(exit, (text.bottom() - at.top()) / direction.y());
        if (direction.y() < -1e-9) exit = std::min(exit, (text.top() - at.bottom()) / direction.y());
        move = std::max(move, exit + 1.0);
      }
      if (move == 0) break;
      distance += move;
    }
    return distance;
  };
  std::map<QString, QPointF> rowOffsets;
  std::map<QString, int> counts;
  for (const Glyph& g : all) {
    ++counts[g.entity];
  }
  std::map<QString, int> placed;
  std::vector<Glyph> result;
  for (Glyph g : all) {
    const int index = placed[g.entity]++;
    const int count = counts[g.entity];
    QPointF center;
    QPointF along(1.0, 0.0);
    QPointF normal(0.0, -1.0);
    if (const auto p = model.pointAt(g.entity)) {
      center = m_c.toScreen(*p) + QPointF(12.0 + index * kGlyphStep, -12.0);
    } else if (const CurveData* c = model.curve(g.entity)) {
      double t = (c->curve.t0() + c->curve.t1()) / 2.0;
      if (c->curve.isClosed()) {
        t = 3.0 * kPi / 4.0;
      }
      // An axis's line (mitcad#500): beside the axis near the origin, where
      // the axis is drawn.
      const V2 anchor =
          c->axis.isEmpty() ? c->curve.at(t) : unit(c->curve.b - c->curve.a) * (60.0 * m_c.pixel());
      const QPointF a = m_c.toScreen(anchor);
      along = unitOf(m_c.toScreen(anchor + unit(c->curve.tangent(t))) - a);
      if (along.x() < 0) {
        along = -along;
      }
      normal = QPointF(along.y(), -along.x());
      if (normal.y() > 0) {
        normal = -normal;
      }
      center = a + normal * 14.0 + along * ((index - (count - 1) / 2.0) * kGlyphStep);
    } else {
      continue;
    }
    // Keep an entity's glyph row together. Only a collision moves it,
    // to the nearer side of its original normal; dimension positions and
    // model constraints stay intact. Painting and picking use this layout.
    // A move never puts the row over its own point or curve: a symbol
    // there would hide it and take the clicks meant for it.
    auto [offset, inserted] = rowOffsets.try_emplace(g.entity);
    if (inserted) {
      const QPointF first = center - along * (index * kGlyphStep);
      const QPointF last = first + along * ((count - 1) * kGlyphStep);
      const auto box = [](const QPointF& at) {
        return QRectF(at.x() - kGlyphSize / 2, at.y() - kGlyphSize / 2, kGlyphSize, kGlyphSize);
      };
      const QRectF row = box(first).united(box(last));
      if (normal.isNull()) normal = QPointF(0.0, -1.0);
      const double forward = distanceToClear(row, normal);
      const double backward = distanceToClear(row, -normal);
      if (forward > 0 || backward > 0) {
        const std::vector<QPointF> own = entityOutline(g.entity);
        const bool forwardOver = rectTouches(row.translated(normal * forward), own);
        const bool backwardOver = rectTouches(row.translated(-normal * backward), own);
        // The nearer side clear of it; when neither is, the original side.
        const bool useForward = forwardOver == backwardOver ? forwardOver || forward <= backward : !forwardOver;
        offset->second = normal * (useForward ? forward : -backward);
      }
    }
    center += offset->second;
    g.rect = QRectF(center.x() - kGlyphSize / 2, center.y() - kGlyphSize / 2, kGlyphSize, kGlyphSize);
    result.push_back(g);
  }
  return result;
}

std::vector<QPointF> SketchOverlay::entityOutline(const QString& entity) const {
  const SketchModel& model = m_c.model();
  std::vector<QPointF> outline;
  if (const auto p = model.pointAt(entity)) {
    outline.push_back(m_c.toScreen(*p));
  } else if (const CurveData* c = model.curve(entity)) {
    if (!c->axis.isEmpty()) {
      return outline; // an axis's line runs across the view
    }
    constexpr int kSamples = 64;
    const double t0 = c->curve.t0();
    const double t1 = c->curve.t1();
    for (int i = 0; i <= kSamples; ++i) {
      outline.push_back(m_c.toScreen(c->curve.at(t0 + (t1 - t0) * i / kSamples)));
    }
  }
  return outline;
}

SketchOverlay::Hit SketchOverlay::hitTest(const QPointF& position) const {
  Hit hit;
  if (!m_c.isActive()) {
    return hit;
  }
  if (m_c.showDimensions()) {
    for (const DimensionLayout& d : dimensions()) {
      if (d.textRect.adjusted(-2, -2, 2, 2).contains(position)) {
        hit.kind = Hit::Kind::Dimension;
        hit.id = d.id;
        return hit;
      }
    }
  }
  if (m_c.showConstraints()) {
    for (const Glyph& g : glyphs()) {
      if (g.rect.adjusted(-1, -1, 1, 1).contains(position)) {
        hit.kind = g.constraint.isEmpty() ? Hit::Kind::Fixed : Hit::Kind::Constraint;
        hit.id = g.constraint.isEmpty() ? g.entity : g.constraint;
        return hit;
      }
    }
  }
  return hit;
}

std::vector<SketchOverlay::Hit> SketchOverlay::hitsIn(const QRectF& area, bool crossing) const {
  std::vector<Hit> hits;
  if (!m_c.isActive()) {
    return hits;
  }
  const auto held = [&area, crossing](const QRectF& rect) {
    return crossing ? area.intersects(rect) : area.contains(rect);
  };
  if (m_c.showDimensions()) {
    for (const DimensionLayout& d : dimensions()) {
      if (held(d.textRect)) {
        Hit hit;
        hit.kind = Hit::Kind::Dimension;
        hit.id = d.id;
        hits.push_back(hit);
      }
    }
  }
  if (m_c.showConstraints()) {
    for (const Glyph& g : glyphs()) {
      if (held(g.rect)) {
        Hit hit;
        hit.kind = g.constraint.isEmpty() ? Hit::Kind::Fixed : Hit::Kind::Constraint;
        hit.id = g.constraint.isEmpty() ? g.entity : g.constraint;
        hits.push_back(hit);
      }
    }
  }
  return hits;
}

QRectF SketchOverlay::dimensionTextRect(const QString& id) const {
  for (const DimensionLayout& d : dimensions()) {
    if (d.id == id) {
      return d.textRect;
    }
  }
  return QRectF();
}

bool SketchOverlay::isSelected(const QString& kind, const QString& id) const {
  for (const SelectionItem& item : m_c.host().selection()) {
    if (item.owner != m_c.uid()) {
      continue;
    }
    if (kind == QStringLiteral("constraint") && item.kind == SelectKind::SketchConstraint &&
        item.name == id) {
      return true;
    }
    if (kind == QStringLiteral("dimension") && item.kind == SelectKind::SketchDimension &&
        item.name == id) {
      return true;
    }
    if (kind == QStringLiteral("fix") && item.kind == SelectKind::SketchConstraint &&
        item.name == QStringLiteral("fix:") + id) {
      return true;
    }
  }
  return false;
}

void SketchOverlay::paintGlyph(QPainter& painter, const Glyph& glyph, bool selected,
                               bool hovered) const {
  const QRectF r = glyph.rect;
  // An inactive constraint's (mitcad#721): pink, its frame dashed.
  painter.setPen(glyph.inactive ? QPen(kInactiveColor, 1.0, Qt::DashLine) : QPen(QColor(0x8a, 0x90, 0x9a), 1.0));
  painter.setBrush(selected           ? kSelectedColor
                   : hovered          ? kHoverColor.lighter(150)
                   : glyph.inactive   ? kInactiveFill
                                      : QColor(255, 255, 255, 225));
  painter.drawRoundedRect(r, 2.5, 2.5);
  const QColor ink = selected ? QColor(Qt::white) : glyph.inactive ? kInactiveColor : QColor(0x30, 0x36, 0x40);
  painter.setPen(QPen(ink, 1.4, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
  painter.setBrush(Qt::NoBrush);
  const QPointF c = r.center();
  const double s = r.width() / 2.0 - 3.0; // half size of the symbol
  const auto L = [&](double x1, double y1, double x2, double y2) {
    painter.drawLine(QPointF(c.x() + x1 * s, c.y() + y1 * s), QPointF(c.x() + x2 * s, c.y() + y2 * s));
  };
  const QString& t = glyph.type;
  if (t == QStringLiteral("horizontal") || t == QStringLiteral("horizontal_points")) {
    L(-1, 0, 1, 0);
  } else if (t == QStringLiteral("vertical") || t == QStringLiteral("vertical_points")) {
    L(0, -1, 0, 1);
  } else if (t == QStringLiteral("parallel")) {
    L(-0.9, 0.8, -0.1, -0.8);
    L(0.1, 0.8, 0.9, -0.8);
  } else if (t == QStringLiteral("perpendicular")) {
    L(-1, 0.9, 1, 0.9);
    L(0, 0.9, 0, -1);
  } else if (t == QStringLiteral("coincident")) {
    painter.setBrush(ink);
    painter.drawEllipse(c, s * 0.45, s * 0.45);
  } else if (t == QStringLiteral("tangent")) {
    painter.drawEllipse(QPointF(c.x(), c.y() + s * 0.25), s * 0.6, s * 0.6);
    L(-1, -0.35, 1, -0.35);
  } else if (t == QStringLiteral("equal")) {
    L(-0.9, -0.35, 0.9, -0.35);
    L(-0.9, 0.35, 0.9, 0.35);
  } else if (t == QStringLiteral("concentric")) {
    painter.drawEllipse(c, s, s);
    painter.drawEllipse(c, s * 0.45, s * 0.45);
  } else if (t == QStringLiteral("midpoint")) {
    QPolygonF triangle;
    triangle << QPointF(c.x(), c.y() - s) << QPointF(c.x() + s, c.y() + s * 0.8)
             << QPointF(c.x() - s, c.y() + s * 0.8);
    painter.drawPolygon(triangle);
  } else if (t == QStringLiteral("collinear")) {
    L(-1, 0.6, 1, -0.6);
    painter.setBrush(ink);
    painter.drawEllipse(QPointF(c.x() - s * 0.5, c.y() + s * 0.3), 1.3, 1.3);
    painter.drawEllipse(QPointF(c.x() + s * 0.5, c.y() - s * 0.3), 1.3, 1.3);
  } else if (t == QStringLiteral("symmetric")) {
    L(-1, -1, -0.4, 0);
    L(-0.4, 0, -1, 1);
    L(1, -1, 0.4, 0);
    L(0.4, 0, 1, 1);
  } else if (t == QStringLiteral("smooth")) {
    QPainterPath path(QPointF(c.x() - s, c.y() + s * 0.6));
    path.cubicTo(QPointF(c.x() - s * 0.2, c.y() + s * 0.6), QPointF(c.x() + s * 0.2, c.y() - s * 0.6),
                 QPointF(c.x() + s, c.y() - s * 0.6));
    painter.drawPath(path);
  } else if (t == QStringLiteral("equal_spacing")) {
    // Three ticks equally apart on a line (mitcad#221).
    L(-1, 0, 1, 0);
    L(-1, -0.7, -1, 0.7);
    L(0, -0.7, 0, 0.7);
    L(1, -0.7, 1, 0.7);
  } else if (t == QStringLiteral("fix")) {
    painter.drawRect(QRectF(c.x() - s * 0.75, c.y() - s * 0.05, s * 1.5, s * 1.05));
    painter.drawArc(QRectF(c.x() - s * 0.45, c.y() - s * 0.85, s * 0.9, s * 1.4), 0, 180 * 16);
  }
}

void SketchOverlay::paintDimensionHalo(QPainter& painter, const DimensionShape& placed,
                                       const QColor& halo) const {
  const DimensionShape shape = onDevicePixels(placed, painter);
  painter.setPen(QPen(halo, kDimensionHaloWidth, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
  painter.setBrush(Qt::NoBrush);
  for (const QLineF& line : shape.lines) {
    painter.drawLine(line);
  }
  for (const QPolygonF& arc : shape.arcs) {
    painter.drawPolyline(arc);
  }
  // The arrowheads outlined, then filled by `paintDimension`.
  painter.setPen(QPen(halo, kArrowHaloWidth, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
  painter.setBrush(halo);
  for (const auto& [tip, towards] : shape.arrows) {
    painter.drawPolygon(arrowHead(tip, towards));
  }
}

void SketchOverlay::paintDimension(QPainter& painter, const DimensionShape& placed,
                                   const QRectF& rect, const QColor& color, const QColor& lines,
                                   bool driven, bool inactive) const {
  const DimensionShape shape = onDevicePixels(placed, painter);
  // An inactive one (mitcad#721) dashed, its value on pink.
  painter.setPen(QPen(lines, 1.0, inactive ? Qt::DashLine : Qt::SolidLine));
  painter.setBrush(Qt::NoBrush);
  for (const QLineF& line : shape.lines) {
    painter.drawLine(line);
  }
  for (const QPolygonF& arc : shape.arcs) {
    painter.drawPolyline(arc);
  }
  painter.setPen(QPen(lines, 1.0));
  painter.setBrush(lines);
  for (const auto& [tip, towards] : shape.arrows) {
    painter.drawPolygon(arrowHead(tip, towards));
  }
  painter.setPen(QPen(color.lighter(driven ? 130 : 160), 1.0));
  painter.setBrush(inactive ? kInactiveFill : QColor(255, 255, 255, 235));
  painter.drawRoundedRect(rect, 3, 3);
  painter.setPen(color);
  painter.drawText(rect, Qt::AlignCenter, shape.text);
}

std::vector<std::pair<QString, QColor>> SketchOverlay::idleHighlights() const {
  std::vector<std::pair<QString, QColor>> highlighted;
  // The selected curves and points, over OCCT's highlight, which only
  // recolours them in lines one pixel wide on macOS. A tool takes the
  // clicks and the view shows no selection then.
  if (m_c.tool() == nullptr) {
    for (const SelectionItem& item : m_c.host().selection()) {
      if (item.owner == m_c.uid() &&
          (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint)) {
        highlighted.emplace_back(item.name, kSelectedColor);
      }
    }
  }
  // The axes a command's input holds (Mirror's line, mitcad#876), which
  // the view's highlight cannot show.
  for (const QString& axis : m_c.host().sketchAxisPicks()) {
    highlighted.emplace_back(axis, kSelectedColor);
  }
  // What a picking tool points at and has picked.
  for (const QString& entity : m_c.picked()) {
    highlighted.emplace_back(entity, kPickColor);
  }
  if (!m_c.hover().isEmpty()) {
    highlighted.emplace_back(m_c.hover(), kHoverColor);
  }
  return highlighted;
}

HighlightLayers SketchOverlay::highlightLayers(const std::vector<std::pair<QString, QColor>>& entities,
                                               HighlightSummary* summary) const {
  const SketchModel& model = m_c.model();
  // A body edge or vertex in the sketch plane (mitcad#256) as a sketch entity.
  const BodyGeometry& body = m_c.bodyGeometryShown();
  HighlightLayers layers;
  QStringList stroked;    // the entities drawn as strokes, once each
  QStringList balled;     // the entities drawn as balls, once each
  QStringList curves;     // the sketch curves among them, whose points get markers
  QSet<QString> points;   // the sketch points highlighted: their balls, no plain marker
  const auto strokeOf = [&layers, &stroked](const QString& id, const QPolygonF& path, const QColor& tone) {
    layers.strokes.push_back({path, tone, false});
    if (!stroked.contains(id)) {
      stroked << id;
    }
  };
  const auto ballOf = [&layers, &balled](const QString& id, const QPointF& at, const QColor& tone) {
    layers.balls.push_back({at, tone});
    if (!balled.contains(id)) {
      balled << id;
    }
  };
  for (const auto& [entity, color] : entities) {
    const bool onBody = BodyGeometry::isBodyId(entity);
    // The sketch's origin and axes as picks (mitcad#500): the origin before
    // it has a point, an axis across the view.
    if (entity == originPick()) {
      ballOf(entity, m_c.toScreen(V2{}), color);
      continue;
    }
    if (const QString axis = onBody ? QString() : model.axisOf(entity); !axis.isEmpty()) {
      const QPointF origin = m_c.toScreen(V2{});
      const double step = 100.0 * m_c.pixel();
      const QPointF along = m_c.toScreen(axis == QStringLiteral("x") ? V2{step, 0.0} : V2{0.0, step}) - origin;
      if (std::hypot(along.x(), along.y()) >= 1.0) {
        if (const std::optional<QLineF> line = lineAcross(origin, along, rect())) {
          QPolygonF across;
          across << line->p1() << line->p2();
          strokeOf(entity, across, color);
        }
      }
      continue;
    }
    // A body edge or vertex out of the plane (mitcad#866): where it is, thin
    // and dashed, and below what it projects to, as for one in the plane.
    if (const BodyItem* item = onBody ? body.item(entity) : nullptr; item != nullptr && !item->inPlane) {
      layers.strokes.push_back({m_c.bodyPath(*item), color, true});
    }
    if (const auto p = onBody ? body.pointAt(entity) : model.pointAt(entity)) {
      ballOf(entity, m_c.toScreen(*p), color);
      if (!onBody) {
        points.insert(entity);
      }
      continue;
    }
    const CurveData* c = onBody ? nullptr : model.curve(entity);
    if (const Curve2* curve = onBody ? body.curve(entity) : c != nullptr ? &c->curve : nullptr) {
      QPolygonF polyline;
      for (const V2& at : curve->sample(2.0 * m_c.pixel())) {
        polyline << m_c.toScreen(at);
      }
      strokeOf(entity, polyline, color);
      if (c != nullptr && !curves.contains(entity)) {
        curves << entity;
      }
    }
  }
  // The highlighted curves' end, centre, fit and control points as the view
  // shows them (ModelShapes' sketchEntities: not an axis line's far end, of
  // a derived offset's spline only its ends), each once, unless highlighted
  // itself.
  const bool dark = m_c.viewer().backgroundDark();
  QSet<QString> marked;
  for (const QString& id : curves) {
    const CurveData* c = model.curve(id);
    if (c == nullptr) {
      continue;
    }
    for (const QString& pointId : c->points) {
      const PointData* point = model.point(pointId);
      if (point == nullptr || marked.contains(pointId) || points.contains(pointId) || !point->axis.isEmpty() ||
          (point->derived && pointId != c->start && pointId != c->end && pointId != c->center)) {
        continue;
      }
      marked.insert(pointId);
      const PointState state = point->reference      ? PointState::Reference
                               : point->construction ? PointState::Construction
                               : point->constrained  ? PointState::Constrained
                                                     : PointState::Free;
      layers.markers.push_back({m_c.toScreen(point->at), pointMarkerLook(state, dark)});
    }
  }
  if (summary != nullptr) {
    summary->curves = static_cast<int>(stroked.size());
    summary->points = static_cast<int>(balled.size());
    summary->markers = static_cast<int>(layers.markers.size());
    // The order paintHighlightLayers paints in.
    summary->markersOver = markersOverHighlights(highlightPassOrder());
  }
  return layers;
}

void SketchOverlay::paintHighlights(QPainter& painter,
                                    const std::vector<std::pair<QString, QColor>>& entities) const {
  paintHighlightLayers(painter, highlightLayers(entities, nullptr));
}

void SketchOverlay::logHighlights() {
  if (!m_c.isActive()) {
    return;
  }
  HighlightSummary summary;
  highlightLayers(idleHighlights(), &summary);
  logHighlightSummary(summary);
}

void SketchOverlay::logHighlightSummary(const HighlightSummary& summary) {
  const QString line =
      QStringLiteral("point markers %1 highlights; %2 curve%3, %4 point%5 highlighted, %6 marker%7 over them")
          .arg(summary.markersOver ? QStringLiteral("over") : QStringLiteral("under"))
          .arg(summary.curves)
          .arg(summary.curves == 1 ? QString() : QStringLiteral("s"))
          .arg(summary.points)
          .arg(summary.points == 1 ? QString() : QStringLiteral("s"))
          .arg(summary.markers)
          .arg(summary.markers == 1 ? QString() : QStringLiteral("s"));
  if (line != m_loggedHighlights) {
    m_loggedHighlights = line;
    qDebug().noquote() << QStringLiteral("Sketch overlay: %1").arg(line);
  }
}

void SketchOverlay::paintChain(QPainter& painter, const QStringList& anchors, const QColor& color) const {
  const SketchModel& model = m_c.model();
  QPolygonF chain;
  for (const QString& id : anchors) {
    if (const auto p = model.anchorAt(id)) {
      chain << m_c.toScreen(*p);
    }
  }
  painter.setPen(QPen(color, 1.5, Qt::DashLine, Qt::RoundCap, Qt::RoundJoin));
  painter.setBrush(Qt::NoBrush);
  painter.drawPolyline(chain);
  for (int i = 0; i < chain.size(); ++i) {
    const QPointF at = chain.at(i);
    painter.setPen(QPen(color, 1.0));
    painter.setBrush(color);
    painter.drawEllipse(at, 3.0, 3.0);
    const QString number = QString::number(i + 1);
    const QRectF rect = textRect(at + QPointF(-12.0, 14.0), number);
    painter.setBrush(QColor(255, 255, 255, 230));
    painter.drawRoundedRect(rect, 3, 3);
    painter.drawText(rect, Qt::AlignCenter, number);
  }
}

void SketchOverlay::paintPreview(QPainter& painter, const ToolPreview& preview) const {
  for (const ToolPreview::Path& path : preview.paths) {
    QColor color = kPreviewColor;
    Qt::PenStyle style = Qt::SolidLine;
    double width = 2.0;
    switch (path.style) {
    case ToolPreview::Style::Draw:
      if (m_c.construction()) {
        color = kConstructionPreview;
        style = Qt::DashLine;
      }
      break;
    case ToolPreview::Style::Construction:
      color = kConstructionPreview;
      style = Qt::DashLine;
      break;
    case ToolPreview::Style::Highlight:
      color = kPickColor;
      width = 3.5;
      break;
    case ToolPreview::Style::Remove:
      color = kRemoveColor;
      width = 3.5;
      style = Qt::DashLine;
      break;
    case ToolPreview::Style::Guide:
      color = kGuideColor;
      width = 1.0;
      style = Qt::DashLine;
      break;
    }
    painter.setPen(QPen(color, width, style, Qt::RoundCap, Qt::RoundJoin));
    painter.setBrush(Qt::NoBrush);
    QPolygonF polyline;
    for (const V2& p : path.points) {
      polyline << m_c.toScreen(p);
    }
    if (path.closed && !polyline.isEmpty()) {
      polyline << polyline.first();
    }
    painter.drawPolyline(polyline);
  }
  painter.setPen(QPen(kPreviewColor, 1.2));
  painter.setBrush(Qt::white);
  for (const V2& p : preview.points) {
    const QPointF s = m_c.toScreen(p);
    painter.drawRect(QRectF(s.x() - 3, s.y() - 3, 6, 6));
  }
  for (const auto& [at, text] : preview.labels) {
    const QRectF rect = textRect(m_c.toScreen(at), text);
    painter.setPen(QPen(kPreviewColor, 1.0));
    painter.setBrush(QColor(255, 255, 255, 230));
    painter.drawRoundedRect(rect, 3, 3);
    painter.drawText(rect, Qt::AlignCenter, text);
  }
  if (preview.dimension) {
    const DimensionData& d = *preview.dimension;
    const SketchModel& model = preview.model != nullptr ? *preview.model : m_c.model();
    const DimensionShape shape = dimensionShape(d, model, projection(), dimensionText(d));
    if (shape.valid) {
      paintDimensionHalo(painter, shape, m_c.viewer().sketchHaloColor());
      paintDimension(painter, shape, textRect(shape.textAt, shape.text), kPreviewColor, kPreviewColor,
                     false);
    }
  }
  for (const auto& value : preview.fieldDimensions) {
    const QRectF field = m_c.fieldRect(value.field);
    const auto at = m_c.sketchAtF(field.center());
    if (field.isEmpty() || !at) {
      continue;
    }
    DimensionData dimension = value.dimension;
    dimension.text = *at;
    const auto shape = dimensionShape(dimension, value.model, projection(), QString());
    if (shape.valid) {
      paintDimensionHalo(painter, shape, m_c.viewer().sketchHaloColor());
      paintDimension(painter, shape, field, kPreviewColor, kPreviewColor, false);
    }
  }
  if (!preview.glyph.isEmpty() && m_c.cursorInView()) {
    Glyph glyph;
    glyph.type = preview.glyph;
    // Beside the geometry it constrains, or above left of the cursor (the
    // typed values are below right of it).
    const QPointF at = preview.glyphAt ? m_c.toScreen(*preview.glyphAt) + QPointF(8, -24)
                                       : m_c.cursor() + QPointF(-34, -30);
    glyph.rect = QRectF(at.x(), at.y(), kGlyphSize, kGlyphSize);
    paintGlyph(painter, glyph, false, true);
  }
  // The auto constraints the next click adds, in a row below right of
  // their place (the inferred glyph is above it).
  if (!preview.marks.isEmpty() && preview.marksAt && m_c.cursorInView()) {
    const QPointF at = m_c.toScreen(*preview.marksAt) + QPointF(8, 8);
    for (int i = 0; i < preview.marks.size(); ++i) {
      Glyph glyph;
      glyph.type = preview.marks.at(i);
      glyph.rect = QRectF(at.x() + i * (kGlyphSize + 3), at.y(), kGlyphSize, kGlyphSize);
      paintGlyph(painter, glyph, false, true);
    }
  }
}

void SketchOverlay::paintSnap(QPainter& painter) const {
  const Snap& snap = m_c.snapped();
  if (!snap.onGeometry() || !m_c.cursorInView()) {
    return;
  }
  const QPointF c = m_c.toScreen(snap.at);
  painter.setPen(QPen(kSnapColor, 1.8));
  painter.setBrush(Qt::NoBrush);
  constexpr double r = 5.5;
  switch (snap.kind) {
  case Snap::Kind::Point:
  case Snap::Kind::Origin:
  case Snap::Kind::BodyVertex: // a body vertex looks like a point (mitcad#258)
    painter.drawRect(QRectF(c.x() - r, c.y() - r, 2 * r, 2 * r));
    break;
  case Snap::Kind::Center:
    painter.drawEllipse(c, r, r);
    painter.drawLine(QPointF(c.x() - 2, c.y()), QPointF(c.x() + 2, c.y()));
    painter.drawLine(QPointF(c.x(), c.y() - 2), QPointF(c.x(), c.y() + 2));
    break;
  case Snap::Kind::BodyMidpoint:
  case Snap::Kind::Midpoint: {
    QPolygonF triangle;
    triangle << QPointF(c.x(), c.y() - r) << QPointF(c.x() + r, c.y() + r * 0.8)
             << QPointF(c.x() - r, c.y() + r * 0.8);
    painter.drawPolygon(triangle);
    break;
  }
  case Snap::Kind::Quadrant: {
    QPolygonF diamond;
    diamond << QPointF(c.x(), c.y() - r) << QPointF(c.x() + r, c.y()) << QPointF(c.x(), c.y() + r)
            << QPointF(c.x() - r, c.y());
    painter.drawPolygon(diamond);
    break;
  }
  case Snap::Kind::Intersection:
    painter.drawLine(QPointF(c.x() - r, c.y() - r), QPointF(c.x() + r, c.y() + r));
    painter.drawLine(QPointF(c.x() - r, c.y() + r), QPointF(c.x() + r, c.y() - r));
    break;
  case Snap::Kind::OnCurve:
    painter.drawEllipse(c, 3.5, 3.5);
    break;
  default:
    break;
  }
}

void SketchOverlay::paintEvent(QPaintEvent* event) {
  Q_UNUSED(event);
  if (!m_c.isActive()) {
    return;
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const SketchModel& model = m_c.model();

  paintAxes(painter);
  logAxes();

  // Texts the view shows as their glyph outlines; a text without them (no
  // fonts, a sketch that did not evaluate) as plain text at its anchor.
  painter.save();
  for (const TextData& t : model.texts) {
    if (t.outlined) {
      continue;
    }
    const QPointF at = m_c.toScreen(t.at);
    const QPointF along = m_c.toScreen(t.at + polar(t.angle, 1.0)) - at;
    const double perMm = std::hypot(along.x(), along.y());
    if (perMm <= 0.0 || t.height * perMm < 2.0) {
      continue;
    }
    QFont font = painter.font();
    font.setPixelSize(std::max(1, static_cast<int>(std::lround(t.height * perMm))));
    painter.setFont(font);
    painter.setPen(QColor(0x1a, 0x55, 0xc0));
    painter.save();
    painter.translate(at);
    painter.rotate(std::atan2(along.y(), along.x()) * 180.0 / kPi);
    painter.drawText(QPointF(0, 0), t.text);
    painter.restore();
  }
  painter.restore();

  // The selection's, the picks' and the hover's highlights, with the
  // highlighted curves' point markers over them (paintHighlights,
  // mitcad#946): painted after OCCT's scene, a highlight would cover the
  // view's markers whatever OCCT's layers are.
  HighlightSummary summary;
  paintHighlightLayers(painter, highlightLayers(idleHighlights(), &summary));
  logHighlightSummary(summary);

  const QString hover = m_c.hoverAnnotation();
  if (m_c.showDimensions()) {
    const bool dark = m_c.viewer().backgroundDark();
    const std::vector<DimensionLayout> layouts = dimensions();
    // All the halos first, then the lines, so that a halo never cuts
    // another dimension's line (mitcad#633).
    const QColor halo = m_c.viewer().sketchHaloColor();
    for (const DimensionLayout& layout : layouts) {
      paintDimensionHalo(painter, layout.shape, halo);
    }
    logDimensionHalos();
    for (const DimensionLayout& layout : layouts) {
      const DimensionData* d = model.dimension(layout.id);
      const bool selected = isSelected(QStringLiteral("dimension"), layout.id);
      const bool driven = d != nullptr && d->driven;
      const bool inactive = d != nullptr && !d->active;
      const bool marked = selected || hover == layout.id;
      const QColor color = selected ? kSelectedColor
                           : hover == layout.id ? kHoverColor
                           : inactive ? kInactiveColor
                           : driven ? kDrivenColor
                                    : kDimensionColor;
      const QColor lines = !dark || marked ? color
                           : inactive      ? kInactiveColorDark
                           : driven        ? kDrivenColorDark
                                           : kDimensionColorDark;
      paintDimension(painter, layout.shape, layout.textRect, color, lines, driven, inactive);
    }
  }
  logInactive();
  if (m_c.showConstraints()) {
    for (const Glyph& g : glyphs()) {
      const bool selected = g.constraint.isEmpty() ? isSelected(QStringLiteral("fix"), g.entity)
                                                   : isSelected(QStringLiteral("constraint"), g.constraint);
      const bool hovered = g.constraint.isEmpty() ? hover == g.entity : hover == g.constraint;
      paintGlyph(painter, g, selected, hovered);
    }
  }
  // Equal spacings' chains (mitcad#221), numbered in the order the model
  // keeps: a selected or hovered one's, and the one an Equal Spacing panel
  // previews (its objects in the panel's order until the model has it).
  const QStringList previewed = m_c.spacingPreview();
  const QSet<QString> previewSet(previewed.begin(), previewed.end());
  bool previewShown = false;
  for (const ConstraintData& k : model.constraints) {
    if (k.type != QStringLiteral("equal_spacing")) {
      continue;
    }
    const bool mine = !previewSet.isEmpty() && QSet<QString>(k.refs.begin(), k.refs.end()) == previewSet;
    previewShown = previewShown || mine;
    const bool marked = m_c.showConstraints() && (isSelected(QStringLiteral("constraint"), k.id) || hover == k.id);
    if (mine || marked) {
      paintChain(painter, k.refs, mine ? kPreviewColor : kSelectedColor);
    }
  }
  if (!previewShown && previewed.size() >= 2) {
    paintChain(painter, previewed, kGuideColor);
  }

  if (SketchTool* tool = m_c.tool()) {
    // The auto constraints' reference while the pointer is over the
    // palette's section (mitcad#189); the tools show it while previewing.
    if (m_c.autoHover()) {
      const QString reference = m_c.autoReference();
      if (!reference.isEmpty()) {
        std::vector<std::pair<QString, QColor>> referenced;
        referenced.emplace_back(reference, kPickColor);
        paintHighlights(painter, referenced);
      }
    }
    ToolPreview preview;
    tool->paint(preview);
    paintPreview(painter, preview);
    paintSnap(painter);
  }
  // What a Clean Up Stale Projections preview removes and trims
  // (mitcad#252): the removed curves as they were and the pieces cut off,
  // dashed in the remove colour, over the trimmed lines highlighted.
  const SketchController::StalePreview& stale = m_c.stalePreview();
  if (!stale.empty()) {
    std::vector<std::pair<QString, QColor>> trimmed;
    for (const QString& line : stale.trimmed) {
      trimmed.emplace_back(line, kPickColor);
    }
    paintHighlights(painter, trimmed);
    ToolPreview removal;
    for (const Curve2& curve : stale.removed) {
      removal.curve(curve, 2.0 * m_c.pixel(), ToolPreview::Style::Remove);
    }
    for (const auto& [from, to] : stale.cut) {
      removal.line(from, to, ToolPreview::Style::Remove);
    }
    paintPreview(painter, removal);
  }

  // Degrees of freedom, as the sketch palette shows them.
  QString status = model.fullyConstrained()
                       ? tr("%1: fully constrained").arg(model.name)
                       : tr("%1: %n degree(s) of freedom", nullptr, model.dof).arg(model.name);
  QColor statusColor = model.fullyConstrained() ? QColor(0x20, 0x20, 0x24) : QColor(0x1a, 0x55, 0xc0);
  if (!model.error.isEmpty() || !model.conflicts.empty()) {
    status = tr("%1: conflicting constraints").arg(model.name);
    statusColor = kRemoveColor;
  }
  const QFontMetricsF metrics(font());
  // Above what floats over the bottom of the view (the timeline card).
  int bottomInset = 0;
  auto* viewer = qobject_cast<OcctViewer*>(parentWidget());
  if (viewer != nullptr) {
    bottomInset = viewer->overlayInsets().bottom();
    // The status pill of the floating layout keeps right of the badge.
    viewer->setBadgeWidth(bottomInset > 0 ? static_cast<int>(metrics.horizontalAdvance(status)) + 14 + 14 : 0);
  }
  // At the left then, clear of the command card on the right.
  const qreal badgeLeft = bottomInset > 0 ? 14.0 : width() - metrics.horizontalAdvance(status) - 22.0;
  const QRectF badge(badgeLeft, height() - bottomInset - metrics.height() - 14.0,
                     metrics.horizontalAdvance(status) + 14.0, metrics.height() + 6.0);
  painter.setPen(QPen(statusColor.lighter(160), 1.0));
  painter.setBrush(QColor(255, 255, 255, 220));
  painter.drawRoundedRect(badge, 4, 4);
  painter.setPen(statusColor);
  painter.drawText(badge, Qt::AlignCenter, status);

  // The pointer's place in the sketch's coordinates (mitcad#254), above the
  // badge, at least as wide as a place of three digits so that it does not
  // jump as the pointer moves.
  const QString pointer = m_c.pointerText();
  if (!pointer.isEmpty()) {
    const qreal pointerWidth =
        std::max(metrics.horizontalAdvance(pointer),
                 metrics.horizontalAdvance(QStringLiteral("x -000.00, y -000.00 ") + m_c.lengthUnit())) +
        14.0;
    const QRectF place(bottomInset > 0 ? badge.left() : badge.right() - pointerWidth,
                       badge.top() - badge.height() - 4.0, pointerWidth, badge.height());
    painter.setPen(QPen(QColor(0xb0, 0xb6, 0xc0), 1.0));
    painter.setBrush(QColor(255, 255, 255, 220));
    painter.drawRoundedRect(place, 4, 4);
    painter.setPen(QColor(0x20, 0x20, 0x24));
    painter.drawText(place, Qt::AlignCenter, pointer);
  }
}

SketchOverlay::AxesShape SketchOverlay::axesShape() const {
  AxesShape shape;
  shape.origin = m_c.toScreen(V2{});
  const QRectF area = rect();
  // Each axis's direction in the view from a point about 100 pixels along it.
  const double step = 100.0 * m_c.pixel();
  const QPointF alongX = m_c.toScreen(V2{step, 0.0}) - shape.origin;
  const QPointF alongY = m_c.toScreen(V2{0.0, step}) - shape.origin;
  // None for the sketch plane seen edge on.
  if (std::hypot(alongX.x(), alongX.y()) >= 1.0) {
    shape.x = lineAcross(shape.origin, alongX, area);
  }
  if (std::hypot(alongY.x(), alongY.y()) >= 1.0) {
    shape.y = lineAcross(shape.origin, alongY, area);
  }
  return shape;
}

void SketchOverlay::logAxes() {
  if (!m_c.isActive() || rect().isEmpty()) {
    return;
  }
  const AxesShape shape = axesShape();
  const QPoint offset = mapTo(window(), QPoint(0, 0));
  const auto windowPlace = [&offset](const QPointF& p) {
    return QStringLiteral("%1,%2").arg(qRound(p.x() + offset.x())).arg(qRound(p.y() + offset.y()));
  };
  const auto axisText = [&windowPlace](const std::optional<QLineF>& drawn) {
    return drawn ? QStringLiteral("%1 to %2").arg(windowPlace(drawn->p1()), windowPlace(drawn->p2()))
                 : QStringLiteral("none");
  };
  const bool inView = QRectF(rect()).contains(shape.origin);
  const QString text = QStringLiteral("origin at %1 %2; x axis %3; y axis %4")
                           .arg(windowPlace(shape.origin),
                                inView ? QStringLiteral("in the view") : QStringLiteral("outside the view"),
                                axisText(shape.x), axisText(shape.y));
  if (text != m_loggedAxes) {
    m_loggedAxes = text;
    qDebug().noquote() << QStringLiteral("Sketch axes drawn: %1").arg(text);
  }
}

void SketchOverlay::paintAxes(QPainter& painter) const {
  const SketchModel& model = m_c.model();
  const QRectF area = rect();
  // Drawn whatever the Origin folder's light bulb says (mitcad#739): the
  // sketch's own reference, which the tools snap to and pick.
  const AxesShape shape = axesShape();
  const QPointF origin = shape.origin;
  painter.save();
  painter.setBrush(Qt::NoBrush);

  // The axes across the view, each named at its positive end.
  QFont bold = painter.font();
  bold.setBold(true);
  const std::pair<const std::optional<QLineF>*, QString> axes[] = {{&shape.x, QStringLiteral("X")},
                                                                   {&shape.y, QStringLiteral("Y")}};
  for (const auto& [drawn, name] : axes) {
    if (!*drawn) {
      continue;
    }
    const QLineF& line = **drawn;
    painter.setPen(QPen(kAxisLineColor, 1.0));
    painter.drawLine(line);
    const QPointF u = unitOf(line.p2() - line.p1());
    const QPointF at = line.p2() - u * 14.0 + QPointF(-u.y(), u.x()) * 10.0;
    painter.setFont(bold);
    painter.setPen(kAxisColor);
    painter.drawText(QRectF(at.x() - 8.0, at.y() - 8.0, 16.0, 16.0), Qt::AlignCenter, name);
  }
  painter.setFont(font());

  // The sketch origin, which drawing snaps to: ringed in the axes' colour.
  painter.setPen(QPen(kAxisColor, 1.5));
  painter.drawEllipse(origin, 7.0, 7.0);
  painter.setPen(QPen(QColor(0x50, 0x56, 0x60), 1.2));
  painter.setBrush(QColor(0xff, 0xd8, 0x80));
  painter.drawEllipse(origin, 3.5, 3.5);
  painter.setBrush(Qt::NoBrush);

  // A sketch whose origin is not the design's (on a face, a construction
  // plane, a component placed elsewhere): its origin's place in the
  // design beside it, and the design origin seen along the sketch's normal.
  const gp_Pnt& placed = model.frame.origin;
  painter.setPen(kAxisColor);
  if (placed.XYZ().Modulus() > kOriginTolerance) {
    painter.drawText(origin + QPointF(10.0, -10.0),
                     tr("Sketch origin at %1, %2, %3 %4")
                         .arg(m_c.formatLength(placed.X()), m_c.formatLength(placed.Y()),
                              m_c.formatLength(placed.Z()), m_c.lengthUnit()));
  }
  const V2 design = toSketch(model.frame, gp_Pnt(0.0, 0.0, 0.0));
  const QPointF at = m_c.toScreen(design);
  if (norm(design) > kOriginTolerance && area.adjusted(-10.0, -10.0, 10.0, 10.0).contains(at)) {
    painter.setPen(QPen(kAxisColor, 1.5));
    painter.drawEllipse(at, 5.0, 5.0);
    painter.drawLine(at - QPointF(8.0, 0.0), at + QPointF(8.0, 0.0));
    painter.drawLine(at - QPointF(0.0, 8.0), at + QPointF(0.0, 8.0));
    painter.drawText(at + QPointF(10.0, -8.0), tr("Design origin"));
  }
  painter.restore();
}

void SketchOverlay::hideEvent(QHideEvent* event) {
  QWidget::hideEvent(event);
  m_loggedAxes.clear(); // the next sketch edit logs its axes again
  m_loggedHighlights.clear(); // and its highlights
  if (auto* viewer = qobject_cast<OcctViewer*>(parentWidget())) {
    viewer->setBadgeWidth(0); // no badge: the status pill may use the corner
  }
}

} // namespace mitcad::sketch
