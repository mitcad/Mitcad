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
    return QStringLiteral("on %1").arg(curve);
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
    if (settings.skipPoints.contains(p.id)) {
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
    if (settings.skipCurves.contains(c.id)) {
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

} // namespace mitcad::sketch
