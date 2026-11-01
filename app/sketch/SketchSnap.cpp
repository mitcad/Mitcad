// SPDX-License-Identifier: MIT
#include "SketchSnap.hpp"

#include <algorithm>
#include <cmath>
#include <limits>

#include <QRegularExpression>
#include <QStringList>

namespace mitcad::sketch {
namespace {

// Screen distances, logical pixels.
constexpr double kPointPixels = 10.0;
constexpr double kSpecialPixels = 8.0; // midpoints, quadrants, intersections
constexpr double kCurvePixels = 6.0;
// Curves this near the cursor are tried for intersections.
constexpr double kNearPixels = 40.0;
// Body geometry is this much further than it is, so that the sketch's own
// geometry wins ties (mitcad#258).
constexpr double kBodyTiePixels = 1.0;
// So are the sketch's axes (mitcad#948): a sketch curve along an axis wins.
constexpr double kAxisTiePixels = 1.0;

double screenDistance(const QPointF& a, const QPointF& b) { return std::hypot(a.x() - b.x(), a.y() - b.y()); }

struct Candidate {
  int rank = std::numeric_limits<int>::max(); // lower wins
  double distance = std::numeric_limits<double>::max();
  Snap snap;

  void offer(int r, double d, const Snap& s) {
    if (r < rank || (r == rank && d < distance)) {
      rank = r;
      distance = d;
      snap = s;
    }
  }
};

} // namespace

QString Snap::describe() const {
  switch (kind) {
  case Kind::None:
    return QStringLiteral("free");
  case Kind::Grid:
    return QStringLiteral("grid");
  case Kind::Point:
    return QStringLiteral("point %1").arg(point);
  case Kind::Center:
    return QStringLiteral("center %1").arg(point);
  case Kind::Origin:
    return QStringLiteral("origin");
  case Kind::Midpoint:
    return QStringLiteral("midpoint of %1").arg(curve);
  case Kind::Quadrant:
    return QStringLiteral("quadrant of %1").arg(curve);
  case Kind::Intersection:
    return QStringLiteral("intersection of %1 and %2").arg(curve, curve2);
  case Kind::OnCurve:
    if (!axis.isEmpty()) {
      return QStringLiteral("on the %1 axis (%2)").arg(axis, curve);
    }
    return QStringLiteral("on %1").arg(curve);
  case Kind::BodyVertex:
    return QStringLiteral("body vertex %1").arg(bodyItem.source);
  case Kind::BodyMidpoint:
    return QStringLiteral("midpoint of body edge %1").arg(bodyItem.source);
  }
  return QString();
}

Snap snap(const SnapSettings& settings, const V2& raw, const QPointF& screen) {
  const SketchModel& model = *settings.model;
  const auto& toScreen = settings.toScreen;
  Candidate best;

  // Points: line ends, centres, free points.
  bool originPoint = false;
  for (const PointData& p : model.points) {
    if (settings.skipPoints.contains(p.id) || !p.axis.isEmpty()) {
      continue;
    }
    if (p.fixed && norm(p.at) < 1e-9) {
      originPoint = true;
    }
    const double d = screenDistance(toScreen(p.at), screen);
    if (d > kPointPixels) {
      continue;
    }
    Snap s;
    s.kind = Snap::Kind::Point;
    s.at = p.at;
    s.point = p.id;
    for (const CurveData& c : model.curves) {
      if (c.center == p.id && c.start != p.id) {
        s.kind = Snap::Kind::Center;
        s.curve = c.id;
        break;
      }
    }
    best.offer(0, d, s);
  }
  if (!originPoint) {
    const double d = screenDistance(toScreen(V2{}), screen);
    if (d <= kPointPixels) {
      Snap s;
      s.kind = Snap::Kind::Origin;
      best.offer(0, d, s);
    }
  }

  // Midpoints, quadrants, curves near the cursor.
  std::vector<const CurveData*> nearby;
  for (const CurveData& c : model.curves) {
    if (settings.skipCurves.contains(c.id) || !c.axis.isEmpty()) {
      continue;
    }
    const V2 onCurve = c.curve.nearest(raw);
    const double d = screenDistance(toScreen(onCurve), screen);
    if (d <= kNearPixels) {
      nearby.push_back(&c);
    }
    if (c.isLine() || c.isArc()) {
      const V2 middle = c.curve.middle();
      const double dm = screenDistance(toScreen(middle), screen);
      if (dm <= kSpecialPixels) {
        Snap s;
        s.kind = Snap::Kind::Midpoint;
        s.at = middle;
        s.curve = c.id;
        best.offer(1, dm, s);
      }
    }
    if (c.isCircular() || c.isEllipse()) {
      for (int q = 0; q < 4; ++q) {
        const double t = q * kPi / 2.0;
        if (!c.curve.contains(t) && !c.curve.contains(t + kTwoPi)) {
          continue;
        }
        const V2 at = c.curve.at(t);
        const double dq = screenDistance(toScreen(at), screen);
        if (dq <= kSpecialPixels) {
          Snap s;
          s.kind = Snap::Kind::Quadrant;
          s.at = at;
          s.curve = c.id;
          best.offer(1, dq, s);
        }
      }
    }
    if (d <= kCurvePixels) {
      Snap s;
      s.kind = Snap::Kind::OnCurve;
      s.at = onCurve;
      s.curve = c.id;
      best.offer(2, d, s);
    }
  }
  // Intersections of the curves near the cursor, away from shared points.
  for (std::size_t i = 0; i < nearby.size(); ++i) {
    for (std::size_t j = i + 1; j < nearby.size(); ++j) {
      for (const V2& hit : intersections(nearby[i]->curve, nearby[j]->curve)) {
        const double d = screenDistance(toScreen(hit), screen);
        if (d > kSpecialPixels) {
          continue;
        }
        bool atPoint = false;
        for (const PointData& p : model.points) {
          atPoint = atPoint || dist(p.at, hit) < 1e-6;
        }
        if (atPoint) {
          continue; // a point snap there wins
        }
        Snap s;
        s.kind = Snap::Kind::Intersection;
        s.at = hit;
        s.curve = nearby[i]->id;
        s.curve2 = nearby[j]->id;
        best.offer(1, d, s);
      }
    }
  }

  // The x and y axes as lines across the view (mitcad#948), as curves to
  // snap onto, a little further than they are: a sketch curve along an
  // axis wins, and so do points, the origin and the special points.
  // Along the axis the point keeps to the grid while grid snapping is on,
  // as it would off the axis.
  if (settings.axes) {
    for (int i = 0; i < 2; ++i) {
      const bool alongX = i == 0;
      const QString axis = alongX ? QStringLiteral("x") : QStringLiteral("y");
      const QString line = model.axisLine(axis);
      const QString id = line.isEmpty() ? axisPick(axis) : line;
      if (settings.skipCurves.contains(id)) {
        continue;
      }
      const V2 foot = alongX ? V2{raw.x, 0.0} : V2{0.0, raw.y};
      const double d = screenDistance(toScreen(foot), screen);
      if (d > kCurvePixels) {
        continue;
      }
      double along = alongX ? raw.x : raw.y;
      if (settings.grid && settings.gridStep > 0.0) {
        along = std::round(along / settings.gridStep) * settings.gridStep;
      }
      along += 0.0; // no -0 in logs and commands
      Snap s;
      s.kind = Snap::Kind::OnCurve;
      s.at = alongX ? V2{along, 0.0} : V2{0.0, along};
      s.curve = id;
      s.axis = axis;
      best.offer(2, d + kAxisTiePixels, s);
    }
  }

  // Body vertices as points and the middles of body lines and arcs as
  // midpoints (mitcad#258), a little further than they are: the sketch's
  // own point or midpoint at the same place wins.
  if (settings.body != nullptr) {
    for (const BodyItem& item : settings.body->items()) {
      // Only what lies in the plane: the items out of it (mitcad#866) are
      // the Dimension tool's.
      if (!item.inPlane) {
        continue;
      }
      if (item.isVertex()) {
        const double d = screenDistance(toScreen(item.at), screen);
        if (d <= kPointPixels) {
          Snap s;
          s.kind = Snap::Kind::BodyVertex;
          s.at = item.at;
          s.bodyItem = item;
          best.offer(0, d + kBodyTiePixels, s);
        }
      } else if (item.hasMiddle()) {
        const V2 middle = item.curve.middle();
        const double d = screenDistance(toScreen(middle), screen);
        if (d <= kSpecialPixels) {
          Snap s;
          s.kind = Snap::Kind::BodyMidpoint;
          s.at = middle;
          s.bodyItem = item;
          best.offer(1, d + kBodyTiePixels, s);
        }
      }
    }
  }

  Snap result = best.snap;
  if (best.rank == std::numeric_limits<int>::max()) {
    result = Snap();
    result.at = raw;
    if (settings.grid && settings.gridStep > 0.0) {
      result.kind = Snap::Kind::Grid;
      result.at = {std::round(raw.x / settings.gridStep) * settings.gridStep,
                   std::round(raw.y / settings.gridStep) * settings.gridStep};
      // No -0 in logs and commands.
      result.at.x += 0.0;
      result.at.y += 0.0;
    }
  }
  result.screen = screen;
  return result;
}

QString axesAt(const SketchModel& model, const Snap& snap) {
  switch (snap.kind) {
  case Snap::Kind::Origin:
    return QStringLiteral("xy");
  case Snap::Kind::OnCurve:
    return snap.axis;
  case Snap::Kind::Point:
  case Snap::Kind::Center:
    return model.pointAxes(snap.point);
  default:
    return QString();
  }
}

std::optional<QJsonObject> axisHold(const QString& point, const Snap& snap,
                                    const std::function<QString(const QString&)>& entity) {
  if (snap.kind != Snap::Kind::OnCurve || snap.axis.isEmpty() || point.isEmpty()) {
    return std::nullopt;
  }
  const QString line = entity(snap.curve);
  if (line.isEmpty()) {
    return std::nullopt;
  }
  return QJsonObject{{QStringLiteral("type"), QStringLiteral("coincident")},
                     {QStringLiteral("point"), point},
                     {QStringLiteral("entity"), line}};
}

// ---------------------------------------------------------------------------
// What a drawing tool adds automatically (mitcad#1024)

namespace {

// How near an end must lie to a curve, and how nearly the two must run the
// same way there (the sine of the angle between them), to be tried as a
// tangent: the model's own check (no point moves more than 1e-9 mm) has
// the last word.
constexpr double kTouchOn = 1e-6;
constexpr double kTouchSine = 1e-6;
// Coordinates this close are the same for an axis alignment.
constexpr double kAligned = 1e-9;

// The model's curve `id`, looked up here: test_app links SketchSnap.cpp
// without SketchModel.cpp (SketchModel::curve).
const CurveData* curveOf(const SketchModel& model, const QString& id) {
  for (const CurveData& c : model.curves) {
    if (c.id == id) {
      return &c;
    }
  }
  return nullptr;
}

QJsonObject relation(const QString& type, const QString& a, const QString& b) {
  return {{QStringLiteral("type"), type}, {QStringLiteral("a"), a}, {QStringLiteral("b"), b}};
}

} // namespace

QString axisAlignment(const V2& a, const V2& b) {
  const V2 d = b - a;
  if (norm(d) <= kAligned) {
    return QString();
  }
  if (std::abs(d.y) <= kAligned) {
    return QStringLiteral("horizontal");
  }
  if (std::abs(d.x) <= kAligned) {
    return QStringLiteral("vertical");
  }
  return QString();
}

std::vector<QJsonObject> quadrantHold(const SketchModel& model, const QString& point, const Snap& snap) {
  std::vector<QJsonObject> hold;
  if (snap.kind != Snap::Kind::Quadrant || point.isEmpty() || snap.curve.isEmpty()) {
    return hold;
  }
  hold.push_back({{QStringLiteral("type"), QStringLiteral("coincident")},
                  {QStringLiteral("point"), point},
                  {QStringLiteral("entity"), snap.curve}});
  const CurveData* curve = curveOf(model, snap.curve);
  if (curve == nullptr || curve->center.isEmpty() || curve->center == point) {
    return hold;
  }
  // Level or plumb with the centre: exactly so for a circle or an arc; an
  // ellipse turned off the axes has its quadrants elsewhere.
  const V2 d = snap.at - curve->curve.center;
  const double size = std::max(1.0, norm(d));
  if (std::abs(d.y) <= kAligned * size) {
    hold.push_back(relation(QStringLiteral("horizontal_points"), point, curve->center));
  } else if (std::abs(d.x) <= kAligned * size) {
    hold.push_back(relation(QStringLiteral("vertical_points"), point, curve->center));
  }
  return hold;
}

std::optional<Snap> tangentTouch(const SketchModel& model, const V2& from, const V2& cursor, double tolerance,
                                 const QSet<QString>& skip) {
  std::optional<Snap> best;
  double bestDistance = tolerance;
  const double sides[] = {-1.0, 1.0};
  for (const CurveData& c : model.curves) {
    if (!c.isCircular() || !c.axis.isEmpty() || skip.contains(c.id)) {
      continue;
    }
    const V2 offset = from - c.curve.center;
    const double d = norm(offset);
    const double r = c.curve.radius;
    if (r <= 0.0 || d <= r * (1.0 + 1e-9)) {
      continue; // on or inside: no line from there touches it
    }
    const double spread = std::acos(r / d);
    const double toward = angleOf(offset);
    for (const double side : sides) {
      const double angle = toward + side * spread;
      if (!c.curve.contains(angle)) {
        continue;
      }
      const V2 touch = c.curve.center + polar(angle, r);
      const double off = dist(touch, cursor);
      if (off <= bestDistance) {
        bestDistance = off;
        Snap s;
        s.kind = Snap::Kind::OnCurve;
        s.at = touch;
        s.curve = c.id;
        best = s;
      }
    }
  }
  return best;
}

std::vector<QJsonObject> tangentCandidates(const SketchModel& model, const QString& curve) {
  std::vector<QJsonObject> candidates;
  const CurveData* made = curveOf(model, curve);
  if (made == nullptr || !(made->isLine() || made->isArc())) {
    return candidates;
  }
  // Curves already tangent to it.
  QSet<QString> taken;
  for (const ConstraintData& k : model.constraints) {
    if (k.type == QStringLiteral("tangent") && k.refs.contains(curve)) {
      for (const QString& ref : k.refs) {
        taken.insert(ref);
      }
    }
  }
  const double ends[] = {made->curve.t0(), made->curve.t1()};
  for (const double t : ends) {
    const V2 p = made->curve.at(t);
    const V2 direction = unit(made->curve.tangent(t));
    for (const CurveData& other : model.curves) {
      if (other.id == curve || !other.axis.isEmpty() || taken.contains(other.id)) {
        continue;
      }
      const bool round = other.isCircular();
      if (!round && !(other.isLine() && made->isArc())) {
        continue;
      }
      // An arc on the same circle touches nothing.
      if (round && made->isArc() && dist(other.curve.center, made->curve.center) <= kTouchOn) {
        continue;
      }
      double at = 0.0;
      const V2 foot = other.curve.nearest(p, &at);
      if (dist(foot, p) > kTouchOn) {
        continue;
      }
      const V2 along = unit(other.curve.tangent(at));
      if (std::abs(cross(direction, along)) > kTouchSine) {
        continue;
      }
      candidates.push_back(relation(QStringLiteral("tangent"), other.id, curve));
      taken.insert(other.id);
    }
  }
  return candidates;
}

QString constraintText(const QJsonObject& constraint) {
  const QString type = constraint.value(QStringLiteral("type")).toString();
  if (type == QStringLiteral("coincident")) {
    return QStringLiteral("coincident %1 on %2")
        .arg(constraint.value(QStringLiteral("point")).toString(),
             constraint.value(QStringLiteral("entity")).toString());
  }
  QStringList parts{type};
  const char* const keys[] = {"point", "line", "curve", "a", "b", "entity"};
  for (const char* key : keys) {
    const QJsonValue value = constraint.value(QLatin1String(key));
    if (value.isString()) {
      parts << value.toString();
    }
  }
  return parts.join(QLatin1Char(' '));
}

QString autoConstraintReason(const QString& error) {
  static const QRegularExpression reason(QStringLiteral("auto constraint(?: k\\d+)?: (.+)$"),
                                         QRegularExpression::DotMatchesEverythingOption);
  const QRegularExpressionMatch match = reason.match(error);
  return match.hasMatch() ? match.captured(1).trimmed() : error.trimmed();
}

} // namespace mitcad::sketch
