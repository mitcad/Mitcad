// SPDX-License-Identifier: MIT
#include "SketchSnap.hpp"

#include <cmath>
#include <limits>

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
    return QStringLiteral("centre %1").arg(point);
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

} // namespace mitcad::sketch
