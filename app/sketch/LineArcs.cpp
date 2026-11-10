// SPDX-License-Identifier: MIT
#include "LineArcs.hpp"

#include <algorithm>
#include <cmath>

namespace mitcad::sketch {

std::optional<Curve2> tangentArc(const V2& start, const V2& direction, const V2& end) {
  const V2 t = unit(direction);
  const V2 n = perp(t);
  const V2 d = end - start;
  const double side = dot(n, d);
  if (std::abs(side) < 1e-9 * std::max(1.0, norm(d))) {
    return std::nullopt;
  }
  const double k = dot(d, d) / (2.0 * side);
  const V2 center = start + n * k;
  const double radius = std::abs(k);
  const double a0 = angleOf(start - center);
  const double a1 = angleOf(end - center);
  // Leaving along t: counter-clockwise when the centre is on the left.
  return k > 0 ? Curve2::arc(center, radius, a0, a1) : Curve2::arc(center, radius, a1, a0);
}

LineArcKind lineArcKind(const V2& leaving, const V2& drag, bool normalAllowed) {
  if (!normalAllowed || norm(drag) <= 0.0) {
    return LineArcKind::Tangent;
  }
  const double along = dot(unit(leaving), unit(drag));
  return along >= std::cos(kLineArcTangentCone) ? LineArcKind::Tangent : LineArcKind::Normal;
}

V2 lineArcDirection(LineArcKind kind, const V2& leaving, const V2& start, const V2& toward) {
  const V2 t = unit(leaving);
  if (kind == LineArcKind::Tangent) {
    return t;
  }
  const V2 across = perp(t);
  return dot(toward - start, across) < 0.0 ? -across : across;
}

std::optional<Curve2> lineArc(LineArcKind kind, const V2& start, const V2& leaving, const V2& end) {
  return lineArc(kind, start, leaving, end, end);
}

std::optional<Curve2> lineArc(LineArcKind kind, const V2& start, const V2& leaving, const V2& end,
                              const V2& toward) {
  return tangentArc(start, lineArcDirection(kind, leaving, start, toward), end);
}

V2 lineArcEnd(const V2& start, const V2& direction, const V2& cursor, std::optional<double> radius,
              std::optional<double> sweepAngle) {
  const V2 t = unit(direction);
  const V2 n = perp(t);
  // The centre's side: counter-clockwise round it when on the left.
  const double side = dot(cursor - start, n) < 0.0 ? -1.0 : 1.0;
  if (radius && *radius > 0.0) {
    const V2 center = start + n * (side * *radius);
    if (sweepAngle) {
      const double a = angleOf(start - center) + side * *sweepAngle;
      return center + polar(a, *radius);
    }
    const V2 out = cursor - center;
    return center + (norm(out) > 0.0 ? unit(out) : t) * *radius;
  }
  if (sweepAngle) {
    // The chord leaves at half the sweep from the tangent.
    const V2 chord = polar(angleOf(t) + side * *sweepAngle / 2.0);
    return start + chord * std::max(0.0, dot(cursor - start, chord));
  }
  return cursor;
}

} // namespace mitcad::sketch
