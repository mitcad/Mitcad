// SPDX-License-Identifier: MIT
#include "BodyGeometry.hpp"

#include <algorithm>
#include <cmath>

namespace mitcad::sketch {
namespace {

// Projected entities this near the item are where it is (mm, relative to
// the size of the coordinates).
constexpr double kMatchTolerance = 1e-6;

double matchTolerance(const V2& p) { return kMatchTolerance * std::max({1.0, std::abs(p.x), std::abs(p.y)}); }

// The sketch model's points and curves by id (without SketchModel's own
// lookups, so that this file stays free of the model's JSON).
const PointData* pointOf(const SketchModel& model, const QString& id) {
  const auto it = std::find_if(model.points.begin(), model.points.end(),
                               [&id](const PointData& p) { return p.id == id; });
  return it == model.points.end() ? nullptr : &*it;
}

const CurveData* curveOf(const SketchModel& model, const QString& id) {
  const auto it = std::find_if(model.curves.begin(), model.curves.end(),
                               [&id](const CurveData& c) { return c.id == id; });
  return it == model.curves.end() ? nullptr : &*it;
}

bool sameType(const QString& itemType, Curve2::Type type) {
  switch (type) {
  case Curve2::Type::Line:
    return itemType == QStringLiteral("line");
  case Curve2::Type::Circle:
    return itemType == QStringLiteral("circle");
  case Curve2::Type::Arc:
    return itemType == QStringLiteral("arc");
  case Curve2::Type::Ellipse:
    return itemType == QStringLiteral("ellipse");
  case Curve2::Type::EllipticalArc:
    return itemType == QStringLiteral("elliptical_arc");
  case Curve2::Type::Spline:
    return itemType == QStringLiteral("spline");
  }
  return false;
}

} // namespace

QString BodyItem::describe() const {
  if (section) {
    return QStringLiteral("the section of the body face %1").arg(source);
  }
  return isVertex() ? QStringLiteral("the body vertex %1").arg(source)
                    : QStringLiteral("the body edge %1").arg(source);
}

void BodyGeometry::load(const QJsonArray& answer, CurveReader read) {
  std::vector<BodyItem> loaded;
  for (const QJsonValue& value : answer) {
    const QJsonObject o = value.toObject();
    BodyItem entry;
    entry.source = o.value(QStringLiteral("source")).toString();
    entry.body = o.value(QStringLiteral("body")).toString();
    entry.linked = o.contains(QStringLiteral("occurrence"));
    entry.occurrence = o.value(QStringLiteral("occurrence")).toString();
    entry.type = o.value(QStringLiteral("kind")).toString();
    const QJsonObject geometry = o.value(QStringLiteral("geometry")).toObject();
    if (entry.source.isEmpty() || entry.type.isEmpty()) {
      continue;
    }
    if (entry.isPoint()) {
      const QJsonArray at = geometry.value(QStringLiteral("at")).toArray();
      entry.at = V2{at.at(0).toDouble(), at.at(1).toDouble()};
    } else {
      entry.curve = read(entry.type, geometry);
    }
    // Out of the plane (mitcad#866): where it is.
    entry.inPlane = o.value(QStringLiteral("in_plane")).toBool(true);
    // Where the plane cuts a body (mitcad#2324).
    entry.section = o.value(QStringLiteral("section")).toBool(false);
    entry.height = o.value(QStringLiteral("height")).toDouble();
    for (const QJsonValue& p : o.value(QStringLiteral("path")).toArray()) {
      const QJsonArray xyz = p.toArray();
      entry.path.push_back({V2{xyz.at(0).toDouble(), xyz.at(1).toDouble()}, xyz.at(2).toDouble()});
    }
    if (!entry.inPlane && entry.path.empty()) {
      continue; // nowhere to pick it
    }
    loaded.push_back(std::move(entry));
  }
  set(std::move(loaded));
}

void BodyGeometry::set(std::vector<BodyItem> list) {
  m_items = std::move(list);
  ++m_revision;
}

int BodyGeometry::edgeCount() const {
  return static_cast<int>(std::count_if(m_items.begin(), m_items.end(),
                                        [](const BodyItem& entry) { return entry.inPlane && !entry.isVertex(); }));
}

int BodyGeometry::vertexCount() const {
  return static_cast<int>(std::count_if(m_items.begin(), m_items.end(),
                                        [](const BodyItem& entry) { return entry.inPlane && entry.isVertex(); }));
}

int BodyGeometry::outOfPlaneCount() const {
  return static_cast<int>(
      std::count_if(m_items.begin(), m_items.end(), [](const BodyItem& entry) { return !entry.inPlane; }));
}

const BodyItem* BodyGeometry::item(const QString& id, QString* part) const {
  if (!isBodyId(id)) {
    return nullptr;
  }
  const QStringList pieces = id.split(QLatin1Char(':'));
  bool ok = false;
  const int index = pieces.value(1).toInt(&ok);
  if (!ok || index < 0 || static_cast<std::size_t>(index) >= m_items.size() || pieces.size() > 3) {
    return nullptr;
  }
  const QString which = pieces.value(2);
  const BodyItem& found = m_items[static_cast<std::size_t>(index)];
  if (!which.isEmpty() && (found.isPoint() || (which != QStringLiteral("start") && which != QStringLiteral("end") &&
                                               which != QStringLiteral("center")))) {
    return nullptr;
  }
  if (part != nullptr) {
    *part = which;
  }
  return &found;
}

std::optional<V2> BodyGeometry::pointAt(const QString& id) const {
  QString part;
  const BodyItem* found = item(id, &part);
  if (found == nullptr) {
    return std::nullopt;
  }
  if (found->isPoint()) {
    return found->at;
  }
  if (part == QStringLiteral("start") && found->curve.isOpen()) {
    return found->curve.startPoint();
  }
  if (part == QStringLiteral("end") && found->curve.isOpen()) {
    return found->curve.endPoint();
  }
  if (part == QStringLiteral("center") && found->curve.type != Curve2::Type::Line &&
      found->curve.type != Curve2::Type::Spline) {
    return found->curve.center;
  }
  return std::nullopt;
}

const Curve2* BodyGeometry::curve(const QString& id) const {
  QString part;
  const BodyItem* found = item(id, &part);
  return found != nullptr && !found->isPoint() && part.isEmpty() ? &found->curve : nullptr;
}

std::vector<std::pair<QString, V2>> BodyGeometry::points() const {
  std::vector<std::pair<QString, V2>> out;
  for (std::size_t i = 0; i < m_items.size(); ++i) {
    const BodyItem& entry = m_items[i];
    if (entry.isPoint()) {
      out.emplace_back(itemId(i), entry.at);
    } else if (entry.hasCenter()) {
      out.emplace_back(itemId(i) + QStringLiteral(":center"), entry.curve.center);
    }
  }
  return out;
}

void BodyGeometry::addTo(SketchModel& model) const {
  const auto point = [&model](const QString& id, const V2& at) {
    PointData p;
    p.id = id;
    p.at = at;
    p.fixed = true;
    p.reference = true;
    p.constrained = true;
    model.points.push_back(p);
  };
  for (std::size_t i = 0; i < m_items.size(); ++i) {
    const BodyItem& entry = m_items[i];
    const QString id = itemId(i);
    if (entry.isPoint()) {
      point(id, entry.at);
      continue;
    }
    CurveData c;
    c.id = id;
    c.type = entry.type;
    c.curve = entry.curve;
    c.fixed = true;
    c.reference = true;
    c.constrained = true;
    if (entry.curve.isOpen()) {
      c.start = id + QStringLiteral(":start");
      c.end = id + QStringLiteral(":end");
      point(c.start, entry.curve.startPoint());
      point(c.end, entry.curve.endPoint());
      c.points << c.start << c.end;
    }
    if (entry.curve.type != Curve2::Type::Line && entry.curve.type != Curve2::Type::Spline) {
      c.center = id + QStringLiteral(":center");
      point(c.center, entry.curve.center);
      c.points << c.center;
    }
    model.curves.push_back(c);
  }
}

bool sameCurve(const Curve2& a, const Curve2& b, double tolerance) {
  if (a.type != b.type) {
    return false;
  }
  if (a.isOpen()) {
    const bool along = dist(a.startPoint(), b.startPoint()) <= tolerance && dist(a.endPoint(), b.endPoint()) <= tolerance;
    const bool reversed =
        dist(a.startPoint(), b.endPoint()) <= tolerance && dist(a.endPoint(), b.startPoint()) <= tolerance;
    if (!along && !reversed) {
      return false;
    }
  }
  // Every point of one on the other, sampled.
  constexpr int kSamples = 16;
  for (int i = 0; i <= kSamples; ++i) {
    const double ta = a.t0() + (a.t1() - a.t0()) * i / kSamples;
    const double tb = b.t0() + (b.t1() - b.t0()) * i / kSamples;
    if (b.distanceTo(a.at(ta)) > tolerance || a.distanceTo(b.at(tb)) > tolerance) {
      return false;
    }
  }
  return true;
}

namespace {

// The point or curve among a projection's entities that is where the item
// projects to.
ProjectedItem matchIn(const SketchModel& model, const ProjectionData& p, const BodyItem& item) {
  ProjectedItem found;
  for (const QString& id : p.entities) {
    if (item.isPoint()) {
      const PointData* point = pointOf(model, id);
      if (point != nullptr && dist(point->at, item.at) <= matchTolerance(item.at)) {
        found.point = id;
        break;
      }
      continue;
    }
    const CurveData* curve = curveOf(model, id);
    const V2 size{std::max(std::abs(item.curve.startPoint().x), std::abs(item.curve.center.x)),
                  std::max(std::abs(item.curve.startPoint().y), std::abs(item.curve.center.y))};
    if (curve != nullptr && sameType(item.type, curve->curve.type) &&
        sameCurve(curve->curve, item.curve, matchTolerance(size) + kMatchTolerance * item.curve.radius)) {
      found.curve = id;
      break;
    }
  }
  return found;
}

} // namespace

std::optional<ProjectedItem> existingProjection(const SketchModel& model, const BodyItem& item) {
  for (const ProjectionData& p : model.projections) {
    if (p.stale || p.section != item.section || p.source != item.source || p.body != item.body || p.linked != item.linked ||
        (item.linked && p.occurrence != item.occurrence)) {
      continue;
    }
    if (const ProjectedItem found = matchIn(model, p, item); !found.empty()) {
      return found;
    }
  }
  return std::nullopt;
}

std::optional<ProjectedItem> projectedLike(const SketchModel& model, const BodyItem& item) {
  for (const ProjectionData& p : model.projections) {
    if (p.stale) {
      continue;
    }
    if (const ProjectedItem found = matchIn(model, p, item); !found.empty()) {
      return found;
    }
  }
  return std::nullopt;
}

QString coincidentPoint(const SketchModel& model, const QString& point) {
  for (const ConstraintData& k : model.constraints) {
    if (k.type != QStringLiteral("coincident") || !k.refs.contains(point)) {
      continue;
    }
    for (const QString& other : k.refs) {
      if (other != point && pointOf(model, other) != nullptr) {
        return other;
      }
    }
  }
  return QString();
}

} // namespace mitcad::sketch
