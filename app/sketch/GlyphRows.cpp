// SPDX-License-Identifier: MIT
#include "GlyphRows.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include <utility>

#include <QLineF>

namespace mitcad::sketch {
namespace {

double pointRectDistance(const QPointF& p, const QRectF& r) {
  const double dx = std::max({r.left() - p.x(), 0.0, p.x() - r.right()});
  const double dy = std::max({r.top() - p.y(), 0.0, p.y() - r.bottom()});
  return std::hypot(dx, dy);
}

double pointSegmentDistance(const QPointF& p, const QPointF& a, const QPointF& b) {
  const QPointF ab = b - a;
  const double length2 = QPointF::dotProduct(ab, ab);
  const double t = length2 > 0.0 ? std::clamp(QPointF::dotProduct(p - a, ab) / length2, 0.0, 1.0) : 0.0;
  const QPointF nearest = a + ab * t;
  return std::hypot(p.x() - nearest.x(), p.y() - nearest.y());
}

double segmentRectDistance(const QPointF& a, const QPointF& b, const QRectF& r) {
  const double fromEnds = std::min(pointRectDistance(a, r), pointRectDistance(b, r));
  if (fromEnds == 0.0) {
    return 0.0;
  }
  const QLineF segment(a, b);
  const QPointF corners[] = {r.topLeft(), r.topRight(), r.bottomRight(), r.bottomLeft()};
  double distance = fromEnds;
  for (std::size_t i = 0; i < 4; ++i) {
    const QLineF side(corners[i], corners[(i + 1) % 4]);
    if (segment.intersects(side, nullptr) == QLineF::BoundedIntersection) {
      return 0.0;
    }
    distance = std::min(distance, pointSegmentDistance(corners[i], a, b));
  }
  return distance;
}

// Whether two rects overlap or touch, also when one has no width or height
// (a single point's, a level line's bounds).
bool meet(const QRectF& a, const QRectF& b) {
  return a.left() <= b.right() && b.left() <= a.right() && a.top() <= b.bottom() && b.top() <= a.bottom();
}

} // namespace

GlyphCurve glyphCurve(std::vector<QPointF> outline) {
  GlyphCurve curve;
  if (!outline.empty()) {
    double left = outline.front().x();
    double right = left;
    double top = outline.front().y();
    double bottom = top;
    for (const QPointF& p : outline) {
      left = std::min(left, p.x());
      right = std::max(right, p.x());
      top = std::min(top, p.y());
      bottom = std::max(bottom, p.y());
    }
    curve.bounds = QRectF(QPointF(left, top), QPointF(right, bottom));
  }
  curve.outline = std::move(outline);
  return curve;
}

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

double rectDistance(const QRectF& rect, const std::vector<QPointF>& outline) {
  if (outline.empty()) {
    return std::numeric_limits<double>::infinity();
  }
  if (outline.size() == 1) {
    return pointRectDistance(outline.front(), rect);
  }
  double distance = std::numeric_limits<double>::infinity();
  for (std::size_t i = 1; i < outline.size() && distance > 0.0; ++i) {
    distance = std::min(distance, segmentRectDistance(outline[i - 1], outline[i], rect));
  }
  return distance;
}

std::optional<double> clearDistance(const QRectF& row, const QPointF& direction, const std::vector<QRectF>& boxes,
                                    const std::vector<GlyphCurve>& curves, double limit) {
  double distance = 0.0;
  std::size_t boxMoves = 0;
  // A curve moves the row a pixel at a time: at most `limit` such moves.
  const std::size_t curveSteps = std::isfinite(limit) ? static_cast<std::size_t>(limit) + 1 : 0;
  for (std::size_t steps = 0;; ++steps) {
    const QRectF at = row.translated(direction * distance);
    double move = 0.0;
    // Along a fixed direction, each move leaves at least one box behind. At
    // most one move per box is needed, even in a dense sketch.
    if (boxMoves <= boxes.size()) {
      for (const QRectF& box : boxes) {
        if (!at.intersects(box)) continue;
        double exit = std::numeric_limits<double>::infinity();
        if (direction.x() > 1e-9) exit = std::min(exit, (box.right() - at.left()) / direction.x());
        if (direction.x() < -1e-9) exit = std::min(exit, (box.left() - at.right()) / direction.x());
        if (direction.y() > 1e-9) exit = std::min(exit, (box.bottom() - at.top()) / direction.y());
        if (direction.y() < -1e-9) exit = std::min(exit, (box.top() - at.bottom()) / direction.y());
        move = std::max(move, exit + 1.0);
      }
    }
    if (move > 0.0) {
      ++boxMoves;
    } else {
      const bool nearCurve = std::any_of(curves.begin(), curves.end(), [&at](const GlyphCurve& curve) {
        return meet(curve.bounds, at.adjusted(-kGlyphCurveClearance, -kGlyphCurveClearance, kGlyphCurveClearance,
                                              kGlyphCurveClearance)) &&
               rectDistance(at, curve.outline) <= kGlyphCurveClearance;
      });
      if (!nearCurve) {
        return distance;
      }
      if (steps >= curveSteps + boxes.size() + 1) {
        return std::nullopt;
      }
      move = 1.0;
    }
    distance += move;
    if (distance > limit) {
      return std::nullopt;
    }
  }
}

QPointF glyphRowOffset(const QRectF& row, const QPointF& normal, const std::vector<QRectF>& texts,
                       const std::vector<QRectF>& rows, const std::vector<GlyphCurve>& curves,
                       const std::vector<QPointF>& own) {
  const QPointF n = normal.isNull() ? QPointF(0.0, -1.0) : normal;
  // Clear of the texts, the rows before it and the curves near enough to
  // meet within its moves.
  std::vector<QRectF> boxes = texts;
  boxes.insert(boxes.end(), rows.begin(), rows.end());
  const double margin = kGlyphCurveClearance + 1.0;
  const QRectF area = row.translated(n * kGlyphMoveLimit)
                          .united(row.translated(-n * kGlyphMoveLimit))
                          .adjusted(-margin, -margin, margin, margin);
  std::vector<GlyphCurve> nearby;
  for (const GlyphCurve& curve : curves) {
    if (!curve.outline.empty() && meet(curve.bounds, area)) {
      nearby.push_back(curve);
    }
  }
  const std::optional<double> forward = clearDistance(row, n, boxes, nearby, kGlyphMoveLimit);
  if (forward && *forward == 0.0) {
    return QPointF();
  }
  const std::optional<double> backward = clearDistance(row, -n, boxes, nearby, kGlyphMoveLimit);
  if (forward || backward) {
    const bool useForward = forward && (!backward || *forward <= *backward);
    return useForward ? n * *forward : -n * *backward;
  }
  // Neither side is clear near it: the texts alone, which a row always
  // clears; dimension positions and model constraints stay intact. A move
  // never puts the row over its own point or curve where the other side
  // does not: a symbol there would hide it and take the clicks meant for
  // it.
  const double infinity = std::numeric_limits<double>::infinity();
  const double ahead = clearDistance(row, n, texts, {}, infinity).value_or(0.0);
  const double behind = clearDistance(row, -n, texts, {}, infinity).value_or(0.0);
  if (ahead <= 0.0 && behind <= 0.0) {
    return QPointF();
  }
  const bool aheadOver = rectTouches(row.translated(n * ahead), own);
  const bool behindOver = rectTouches(row.translated(-n * behind), own);
  // The nearer side clear of it; when neither is, the original side.
  const bool useAhead = aheadOver == behindOver ? aheadOver || ahead <= behind : !aheadOver;
  return n * (useAhead ? ahead : -behind);
}

} // namespace mitcad::sketch
