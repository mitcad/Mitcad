// SPDX-License-Identifier: MIT
#include "SketchGeometry.hpp"

#include <algorithm>

namespace mitcad::sketch {
namespace {

constexpr double kTiny = 1e-12;

// The point of a (rational) B-spline by de Boor's algorithm.
V2 deBoor(const Curve2& c, double t) {
  const int n = static_cast<int>(c.control.size());
  const int p = c.degree;
  const std::vector<double>& u = c.knots;
  if (n == 0) {
    return {};
  }
  if (n < p + 1 || static_cast<int>(u.size()) != n + p + 1) {
    return c.control.front();
  }
  int k = p;
  while (k < n - 1 && t >= u[static_cast<std::size_t>(k + 1)]) {
    ++k;
  }
  const bool rational = static_cast<int>(c.weights.size()) == n;
  struct H {
    double x, y, w;
  };
  std::vector<H> d(static_cast<std::size_t>(p + 1));
  for (int j = 0; j <= p; ++j) {
    const auto i = static_cast<std::size_t>(j + k - p);
    const double w = rational ? c.weights[i] : 1.0;
    d[static_cast<std::size_t>(j)] = {c.control[i].x * w, c.control[i].y * w, w};
  }
  for (int r = 1; r <= p; ++r) {
    for (int j = p; j >= r; --j) {
      const double lo = u[static_cast<std::size_t>(j + k - p)];
      const double hi = u[static_cast<std::size_t>(j + 1 + k - r)];
      const double alpha = hi - lo > kTiny ? (t - lo) / (hi - lo) : 0.0;
      H& cur = d[static_cast<std::size_t>(j)];
      const H& prev = d[static_cast<std::size_t>(j - 1)];
      cur = {(1.0 - alpha) * prev.x + alpha * cur.x, (1.0 - alpha) * prev.y + alpha * cur.y,
             (1.0 - alpha) * prev.w + alpha * cur.w};
    }
  }
  const H& r = d[static_cast<std::size_t>(p)];
  return r.w != 0.0 ? V2{r.x / r.w, r.y / r.w} : V2{r.x, r.y};
}

// Segment intersection of polylines' pieces p1-p2 and q1-q2.
std::optional<V2> segmentHit(const V2& p1, const V2& p2, const V2& q1, const V2& q2) {
  const V2 r = p2 - p1;
  const V2 s = q2 - q1;
  const double d = cross(r, s);
  if (std::abs(d) < kTiny) {
    return std::nullopt;
  }
  const double t = cross(q1 - p1, s) / d;
  const double u = cross(q1 - p1, r) / d;
  constexpr double e = 1e-9;
  if (t < -e || t > 1.0 + e || u < -e || u > 1.0 + e) {
    return std::nullopt;
  }
  return p1 + r * t;
}

// Where the infinite line through a, b meets a circle.
std::vector<V2> lineCircle(const V2& a, const V2& b, const V2& center, double radius) {
  const V2 d = b - a;
  const double len2 = dot(d, d);
  if (len2 < kTiny) {
    return {};
  }
  const double t = dot(center - a, d) / len2;
  const V2 foot = a + d * t;
  const double h = dist(foot, center);
  if (h > radius * (1.0 + 1e-9)) {
    return {};
  }
  const double half = std::sqrt(std::max(0.0, radius * radius - h * h));
  const V2 along = unit(d) * half;
  if (half < 1e-9) {
    return {foot};
  }
  return {foot - along, foot + along};
}

std::vector<V2> circleCircle(const V2& c1, double r1, const V2& c2, double r2) {
  const double d = dist(c1, c2);
  if (d < kTiny || d > r1 + r2 + 1e-9 || d < std::abs(r1 - r2) - 1e-9) {
    return {};
  }
  const double a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
  const double h = std::sqrt(std::max(0.0, r1 * r1 - a * a));
  const V2 u = (c2 - c1) / d;
  const V2 mid = c1 + u * a;
  if (h < 1e-9) {
    return {mid};
  }
  return {mid + perp(u) * h, mid - perp(u) * h};
}

bool isCircular(const Curve2& c) {
  return c.type == Curve2::Type::Circle || c.type == Curve2::Type::Arc;
}

// Whether a point found on a curve's carrier lies within the curve.
bool onCurve(const Curve2& c, const V2& p) {
  switch (c.type) {
  case Curve2::Type::Line: {
    const V2 d = c.b - c.a;
    const double len2 = dot(d, d);
    const double t = len2 > 0.0 ? dot(p - c.a, d) / len2 : 0.0;
    return t >= -1e-9 && t <= 1.0 + 1e-9;
  }
  case Curve2::Type::Arc:
    return c.contains(angleOf(p - c.center)) || dist(p, c.startPoint()) < 1e-7 ||
           dist(p, c.endPoint()) < 1e-7;
  default:
    return true;
  }
}

} // namespace

double normalizeAngle(double angle) {
  double a = std::fmod(angle, kTwoPi);
  if (a < 0.0) {
    a += kTwoPi;
  }
  return a;
}

double sweep(double start, double end) {
  const double s = normalizeAngle(end - start);
  return s <= 1e-12 ? kTwoPi : s;
}

Curve2 Curve2::line(const V2& a, const V2& b) {
  Curve2 c;
  c.type = Type::Line;
  c.a = a;
  c.b = b;
  return c;
}

Curve2 Curve2::circle(const V2& center, double radius) {
  Curve2 c;
  c.type = Type::Circle;
  c.center = center;
  c.radius = radius;
  return c;
}

Curve2 Curve2::arc(const V2& center, double radius, double start, double end) {
  Curve2 c;
  c.type = Type::Arc;
  c.center = center;
  c.radius = radius;
  c.start = start;
  c.end = end;
  return c;
}

double Curve2::t0() const {
  switch (type) {
  case Type::Line:
    return 0.0;
  case Type::Circle:
  case Type::Ellipse:
    return 0.0;
  case Type::Arc:
  case Type::EllipticalArc:
    return start;
  case Type::Spline:
    return knots.size() > static_cast<std::size_t>(degree) ? knots[static_cast<std::size_t>(degree)]
                                                             : 0.0;
  }
  return 0.0;
}

double Curve2::t1() const {
  switch (type) {
  case Type::Line:
    return 1.0;
  case Type::Circle:
  case Type::Ellipse:
    return kTwoPi;
  case Type::Arc:
  case Type::EllipticalArc:
    return start + sweep(start, end);
  case Type::Spline: {
    const std::size_t n = control.size();
    return knots.size() > n && n > 0 ? knots[n] : 1.0;
  }
  }
  return 1.0;
}

V2 Curve2::at(double t) const {
  switch (type) {
  case Type::Line:
    return a + (b - a) * t;
  case Type::Circle:
  case Type::Arc:
    return center + polar(t, radius);
  case Type::Ellipse:
  case Type::EllipticalArc: {
    const V2 local{radius * std::cos(t), minor * std::sin(t)};
    const double c = std::cos(rotation);
    const double s = std::sin(rotation);
    return center + V2{local.x * c - local.y * s, local.x * s + local.y * c};
  }
  case Type::Spline:
    return deBoor(*this, std::clamp(t, t0(), t1()));
  }
  return {};
}

V2 Curve2::tangent(double t) const {
  switch (type) {
  case Type::Line:
    return b - a;
  case Type::Circle:
  case Type::Arc:
    return perp(polar(t, radius));
  case Type::Ellipse:
  case Type::EllipticalArc: {
    const V2 local{-radius * std::sin(t), minor * std::cos(t)};
    const double c = std::cos(rotation);
    const double s = std::sin(rotation);
    return {local.x * c - local.y * s, local.x * s + local.y * c};
  }
  case Type::Spline: {
    const double h = (t1() - t0()) * 1e-4;
    const double lo = std::max(t0(), t - h);
    const double hi = std::min(t1(), t + h);
    return hi > lo ? (at(hi) - at(lo)) / (hi - lo) : V2{1.0, 0.0};
  }
  }
  return {1.0, 0.0};
}

std::vector<V2> Curve2::sample(double step) const {
  step = std::max(step, 1e-6);
  int count = 2;
  switch (type) {
  case Type::Line:
    return {a, b};
  case Type::Circle:
  case Type::Arc:
  case Type::Ellipse:
  case Type::EllipticalArc: {
    const double length = (t1() - t0()) * std::max(radius, minor);
    count = std::clamp(static_cast<int>(length / step), 16, 720);
    break;
  }
  case Type::Spline:
    count = std::clamp(static_cast<int>(control.size()) * 24, 48, 960);
    break;
  }
  std::vector<V2> points;
  points.reserve(static_cast<std::size_t>(count) + 1);
  for (int i = 0; i <= count; ++i) {
    points.push_back(at(t0() + (t1() - t0()) * i / count));
  }
  return points;
}

V2 Curve2::nearest(const V2& p, double* tOut) const {
  double t = t0();
  switch (type) {
  case Type::Line: {
    const V2 d = b - a;
    const double len2 = dot(d, d);
    t = len2 > 0.0 ? std::clamp(dot(p - a, d) / len2, 0.0, 1.0) : 0.0;
    break;
  }
  case Type::Circle:
    t = normalizeAngle(angleOf(p - center));
    break;
  case Type::Arc: {
    const double angle = angleOf(p - center);
    if (contains(angle)) {
      t = start + normalizeAngle(angle - start);
    } else {
      t = dist(p, startPoint()) <= dist(p, endPoint()) ? t0() : t1();
    }
    break;
  }
  default: {
    // Dense samples, then a refinement around the best one.
    const int count = type == Type::Spline ? 400 : 360;
    const double lo = t0();
    const double hi = t1();
    double best = lo;
    double bestDistance = dist(at(lo), p);
    for (int i = 1; i <= count; ++i) {
      const double s = lo + (hi - lo) * i / count;
      const double d = dist(at(s), p);
      if (d < bestDistance) {
        bestDistance = d;
        best = s;
      }
    }
    double left = std::max(lo, best - (hi - lo) / count);
    double right = std::min(hi, best + (hi - lo) / count);
    for (int i = 0; i < 40; ++i) {
      const double m1 = left + (right - left) / 3.0;
      const double m2 = right - (right - left) / 3.0;
      if (dist(at(m1), p) < dist(at(m2), p)) {
        right = m2;
      } else {
        left = m1;
      }
    }
    t = (left + right) / 2.0;
    break;
  }
  }
  if (tOut != nullptr) {
    *tOut = t;
  }
  return at(t);
}

double Curve2::parameterOf(const V2& p) const {
  double t = 0.0;
  nearest(p, &t);
  return t;
}

bool Curve2::contains(double t) const {
  switch (type) {
  case Type::Arc:
  case Type::EllipticalArc:
    return normalizeAngle(t - start) <= sweep(start, end) + 1e-9;
  case Type::Line:
  case Type::Spline:
    return t >= t0() - 1e-9 && t <= t1() + 1e-9;
  default:
    return true;
  }
}

std::vector<V2> intersections(const Curve2& a, const Curve2& b) {
  std::vector<V2> hits;
  using T = Curve2::Type;
  if (a.type == T::Line && b.type == T::Line) {
    if (const auto hit = segmentHit(a.a, a.b, b.a, b.b)) {
      hits.push_back(*hit);
    }
    return hits;
  }
  if (a.type == T::Line && isCircular(b)) {
    for (const V2& p : lineCircle(a.a, a.b, b.center, b.radius)) {
      if (onCurve(a, p) && onCurve(b, p)) {
        hits.push_back(p);
      }
    }
    return hits;
  }
  if (isCircular(a) && b.type == T::Line) {
    return intersections(b, a);
  }
  if (isCircular(a) && isCircular(b)) {
    for (const V2& p : circleCircle(a.center, a.radius, b.center, b.radius)) {
      if (onCurve(a, p) && onCurve(b, p)) {
        hits.push_back(p);
      }
    }
    return hits;
  }
  // Ellipses and splines: their polylines, fine enough for snapping.
  const std::vector<V2> pa = a.sample(0.5);
  const std::vector<V2> pb = b.sample(0.5);
  for (std::size_t i = 1; i < pa.size(); ++i) {
    for (std::size_t j = 1; j < pb.size(); ++j) {
      if (const auto hit = segmentHit(pa[i - 1], pa[i], pb[j - 1], pb[j])) {
        const bool known = std::any_of(hits.begin(), hits.end(),
                                       [&](const V2& h) { return dist(h, *hit) < 1e-6; });
        if (!known) {
          hits.push_back(*hit);
        }
      }
    }
  }
  return hits;
}

std::optional<V2> circumcenter(const V2& a, const V2& b, const V2& c) {
  const V2 ab = b - a;
  const V2 ac = c - a;
  const double d = 2.0 * cross(ab, ac);
  if (std::abs(d) <= 1e-12 * std::max(1e-300, norm(ab) * norm(ac))) {
    return std::nullopt;
  }
  const double ab2 = dot(ab, ab);
  const double ac2 = dot(ac, ac);
  return a + V2{(ac.y * ab2 - ab.y * ac2) / d, (ab.x * ac2 - ac.x * ab2) / d};
}

std::vector<V2> fitSplinePreview(const std::vector<V2>& points, int samplesPerSpan) {
  if (points.size() < 3) {
    return points;
  }
  // Catmull-Rom spans with end tangents mirrored: close to the model's
  // cubic through the points, which is what the preview needs.
  std::vector<V2> result;
  const std::size_t n = points.size();
  for (std::size_t i = 0; i + 1 < n; ++i) {
    const V2 p0 = i == 0 ? points[0] * 2.0 - points[1] : points[i - 1];
    const V2 p1 = points[i];
    const V2 p2 = points[i + 1];
    const V2 p3 = i + 2 < n ? points[i + 2] : points[n - 1] * 2.0 - points[n - 2];
    for (int s = 0; s < samplesPerSpan; ++s) {
      const double t = static_cast<double>(s) / samplesPerSpan;
      const double t2 = t * t;
      const double t3 = t2 * t;
      result.push_back((p1 * 2.0 + (p2 - p0) * t + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * t2 +
                        (p1 * 3.0 - p0 - p2 * 3.0 + p3) * t3) *
                       0.5);
    }
  }
  result.push_back(points.back());
  return result;
}

std::vector<V2> controlSplinePreview(const std::vector<V2>& control, int degree, int samples) {
  const int n = static_cast<int>(control.size());
  if (n < 2) {
    return control;
  }
  Curve2 c;
  c.type = Curve2::Type::Spline;
  c.degree = std::min(degree, n - 1);
  c.control = control;
  const int p = c.degree;
  const int inner = n - p - 1;
  c.knots.assign(static_cast<std::size_t>(p + 1), 0.0);
  for (int i = 1; i <= inner; ++i) {
    c.knots.push_back(static_cast<double>(i) / (inner + 1));
  }
  c.knots.insert(c.knots.end(), static_cast<std::size_t>(p + 1), 1.0);
  std::vector<V2> result;
  for (int i = 0; i <= samples; ++i) {
    result.push_back(c.at(static_cast<double>(i) / samples));
  }
  return result;
}

} // namespace mitcad::sketch
