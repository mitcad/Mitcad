// SPDX-License-Identifier: MIT
#include "SketchOverlay.hpp"

#include "../OcctViewer.hpp"

#include <algorithm>
#include <cmath>
#include <map>

#include <QFontMetricsF>
#include <QPainter>
#include <QPainterPath>
#include <QRegularExpression>

#include "SketchController.hpp"
#include "SketchTool.hpp"

namespace mitcad::sketch {
namespace {

const QColor kDimensionColor(0x2a, 0x30, 0x3a);
const QColor kDrivenColor(0x70, 0x76, 0x80);
const QColor kSelectedColor(0x1f, 0x7a, 0xff);
const QColor kHoverColor(0x5a, 0xa8, 0xff);
const QColor kPreviewColor(0x26, 0x6e, 0xe8);
const QColor kConstructionPreview(0xd9, 0x80, 0x1a);
const QColor kRemoveColor(0xe0, 0x30, 0x20);
const QColor kGuideColor(0x80, 0x88, 0x96);
const QColor kSnapColor(0xe0, 0x7b, 0x1a);
const QColor kPickColor(0x2c, 0xa0, 0x2c);

constexpr double kGlyphSize = 15.0;
constexpr double kGlyphStep = 17.0;
constexpr double kArrowLength = 8.0;

QPointF unitOf(const QPointF& p) {
  const double n = std::hypot(p.x(), p.y());
  return n > 1e-12 ? QPointF(p.x() / n, p.y() / n) : QPointF(1, 0);
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
struct Linear {
  V2 a, b, u;
};

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
    return Linear{*p, foot, u};
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
    return Linear{p, foot, u};
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
  return std::nullopt;
}

// The rays of an angle dimension: where the lines meet and their
// directions (start to end), whose angle the model measures.
struct Rays {
  V2 corner, da, db;
};

std::optional<Rays> raysOf(const DimensionData& d, const SketchModel& model) {
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
  return Rays{a->curve.a + da * t, da, db};
}

} // namespace

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
    return std::abs(dot(linear->b - linear->a, linear->u));
  }
  if (t == QStringLiteral("angle")) {
    if (const auto rays = raysOf(d, model)) {
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
    if (const auto rays = raysOf(d, model)) {
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
    const auto extension = [&](const V2& from, const V2& to, double s) {
      if (std::abs(s) < 3.0 * px) {
        return;
      }
      const double sign = s > 0 ? 1.0 : -1.0;
      shape.lines.emplace_back(S(from + n * (sign * 3.0 * px)), S(to + n * (sign * 5.0 * px)));
    };
    extension(linear->a, d1, s1);
    extension(linear->b, d2, s2);
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
    const auto rays = raysOf(d, model);
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
      if (line->curve.distanceTo(end) > 2.0 * px) {
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
    const double span = sweep(c->curve.start, c->curve.end);
    QPolygonF arc;
    for (int i = 0; i <= 32; ++i) {
      arc << S(c->curve.center + polar(c->curve.start + span * i / 32.0, radius));
    }
    shape.arcs.push_back(arc);
    for (const double angle : {c->curve.start, c->curve.start + span}) {
      shape.lines.emplace_back(S(c->curve.center + polar(angle, c->curve.radius)),
                               S(c->curve.center + polar(angle, radius)));
    }
    arrow(c->curve.center + polar(c->curve.start, radius), -perp(polar(c->curve.start)));
    arrow(c->curve.center + polar(c->curve.start + span, radius), perp(polar(c->curve.start + span)));
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
  }
  if (d.driven) {
    return QStringLiteral("(%1)").arg(value);
  }
  if (!plainNumber(d.expression)) {
    return QStringLiteral("fx: ") + value;
  }
  return value;
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
  std::vector<Glyph> all;
  const auto add = [&all](const QString& constraint, const QString& entity, const QString& type) {
    const bool shown = std::any_of(all.begin(), all.end(), [&](const Glyph& g) {
      return g.entity == entity && g.type == type;
    });
    if (!shown) {
      all.push_back({constraint, entity, type, QRectF()});
    }
  };
  for (const ConstraintData& k : model.constraints) {
    const QString& t = k.type;
    if (t == QStringLiteral("coincident") || t == QStringLiteral("midpoint")) {
      add(k.id, k.json.value(QStringLiteral("point")).toString(), t);
    } else if (t == QStringLiteral("symmetric")) {
      add(k.id, k.json.value(QStringLiteral("a")).toString(), t);
      add(k.id, k.json.value(QStringLiteral("b")).toString(), t);
    } else {
      for (const QString& entity : k.refs) {
        add(k.id, entity, t);
      }
    }
  }
  const QString origin = model.originPoint();
  for (const PointData& p : model.points) {
    if (p.fixed && !p.reference && p.id != origin) {
      add(QString(), p.id, QStringLiteral("fix"));
    }
  }
  for (const CurveData& c : model.curves) {
    if (c.fixed && !c.reference) {
      add(QString(), c.id, QStringLiteral("fix"));
    }
  }

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
    if (const auto p = model.pointAt(g.entity)) {
      center = m_c.toScreen(*p) + QPointF(12.0 + index * kGlyphStep, -12.0);
    } else if (const CurveData* c = model.curve(g.entity)) {
      double t = (c->curve.t0() + c->curve.t1()) / 2.0;
      if (c->curve.isClosed()) {
        t = 3.0 * kPi / 4.0;
      }
      const V2 anchor = c->curve.at(t);
      const QPointF a = m_c.toScreen(anchor);
      QPointF along = unitOf(m_c.toScreen(anchor + unit(c->curve.tangent(t))) - a);
      if (along.x() < 0) {
        along = -along;
      }
      QPointF normal(along.y(), -along.x());
      if (normal.y() > 0) {
        normal = -normal;
      }
      center = a + normal * 14.0 + along * ((index - (count - 1) / 2.0) * kGlyphStep);
    } else {
      continue;
    }
    g.rect = QRectF(center.x() - kGlyphSize / 2, center.y() - kGlyphSize / 2, kGlyphSize, kGlyphSize);
    result.push_back(g);
  }
  return result;
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
  painter.setPen(QPen(QColor(0x8a, 0x90, 0x9a), 1.0));
  painter.setBrush(selected  ? kSelectedColor
                   : hovered ? kHoverColor.lighter(150)
                             : QColor(255, 255, 255, 225));
  painter.drawRoundedRect(r, 2.5, 2.5);
  const QColor ink = selected ? QColor(Qt::white) : QColor(0x30, 0x36, 0x40);
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
  } else if (t == QStringLiteral("fix")) {
    painter.drawRect(QRectF(c.x() - s * 0.75, c.y() - s * 0.05, s * 1.5, s * 1.05));
    painter.drawArc(QRectF(c.x() - s * 0.45, c.y() - s * 0.85, s * 0.9, s * 1.4), 0, 180 * 16);
  }
}

void SketchOverlay::paintDimension(QPainter& painter, const DimensionShape& shape,
                                   const QRectF& rect, const QColor& color, bool driven) const {
  painter.setPen(QPen(color, 1.0));
  painter.setBrush(Qt::NoBrush);
  for (const QLineF& line : shape.lines) {
    painter.drawLine(line);
  }
  for (const QPolygonF& arc : shape.arcs) {
    painter.drawPolyline(arc);
  }
  painter.setBrush(color);
  for (const auto& [tip, towards] : shape.arrows) {
    const QPointF dir = unitOf(towards);
    const QPointF side(-dir.y(), dir.x());
    QPolygonF head;
    head << tip << tip - dir * kArrowLength + side * (kArrowLength * 0.32)
         << tip - dir * kArrowLength - side * (kArrowLength * 0.32);
    painter.drawPolygon(head);
  }
  painter.setPen(QPen(color.lighter(driven ? 130 : 160), 1.0));
  painter.setBrush(QColor(255, 255, 255, 235));
  painter.drawRoundedRect(rect, 3, 3);
  painter.setPen(color);
  painter.drawText(rect, Qt::AlignCenter, shape.text);
}

void SketchOverlay::paintEntityHighlight(QPainter& painter, const QString& entity,
                                         const QColor& color) const {
  const SketchModel& model = m_c.model();
  painter.setPen(QPen(color, 4.0, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
  painter.setBrush(Qt::NoBrush);
  if (const auto p = model.pointAt(entity)) {
    painter.setBrush(color);
    painter.drawEllipse(m_c.toScreen(*p), 4.5, 4.5);
    return;
  }
  if (const CurveData* c = model.curve(entity)) {
    QPolygonF polyline;
    for (const V2& p : c->curve.sample(2.0 * m_c.pixel())) {
      polyline << m_c.toScreen(p);
    }
    painter.drawPolyline(polyline);
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
    const DimensionShape shape = dimensionShape(d, m_c.model(), projection(), dimensionText(d));
    if (shape.valid) {
      paintDimension(painter, shape, textRect(shape.textAt, shape.text), kPreviewColor, false);
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
    painter.drawRect(QRectF(c.x() - r, c.y() - r, 2 * r, 2 * r));
    break;
  case Snap::Kind::Center:
    painter.drawEllipse(c, r, r);
    painter.drawLine(QPointF(c.x() - 2, c.y()), QPointF(c.x() + 2, c.y()));
    painter.drawLine(QPointF(c.x(), c.y() - 2), QPointF(c.x(), c.y() + 2));
    break;
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

  // The sketch origin, which drawing snaps to.
  const QPointF origin = m_c.toScreen(V2{});
  painter.setPen(QPen(QColor(0x50, 0x56, 0x60), 1.2));
  painter.setBrush(QColor(0xff, 0xd8, 0x80));
  painter.drawEllipse(origin, 3.5, 3.5);

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

  // What a picking tool points at and has picked.
  for (const QString& entity : m_c.picked()) {
    paintEntityHighlight(painter, entity, kPickColor);
  }
  if (!m_c.hover().isEmpty()) {
    paintEntityHighlight(painter, m_c.hover(), kHoverColor);
  }

  const QString hover = m_c.hoverAnnotation();
  if (m_c.showDimensions()) {
    for (const DimensionLayout& layout : dimensions()) {
      const DimensionData* d = model.dimension(layout.id);
      const bool selected = isSelected(QStringLiteral("dimension"), layout.id);
      const QColor color = selected ? kSelectedColor
                           : hover == layout.id ? kHoverColor
                           : d != nullptr && d->driven ? kDrivenColor
                                                       : kDimensionColor;
      paintDimension(painter, layout.shape, layout.textRect, color, d != nullptr && d->driven);
    }
  }
  if (m_c.showConstraints()) {
    for (const Glyph& g : glyphs()) {
      const bool selected = g.constraint.isEmpty() ? isSelected(QStringLiteral("fix"), g.entity)
                                                   : isSelected(QStringLiteral("constraint"), g.constraint);
      const bool hovered = g.constraint.isEmpty() ? hover == g.entity : hover == g.constraint;
      paintGlyph(painter, g, selected, hovered);
    }
  }

  if (SketchTool* tool = m_c.tool()) {
    ToolPreview preview;
    tool->paint(preview);
    paintPreview(painter, preview);
    paintSnap(painter);
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
}

void SketchOverlay::hideEvent(QHideEvent* event) {
  QWidget::hideEvent(event);
  if (auto* viewer = qobject_cast<OcctViewer*>(parentWidget())) {
    viewer->setBadgeWidth(0); // no badge: the status pill may use the corner
  }
}

} // namespace mitcad::sketch
