// SPDX-License-Identifier: MIT
#pragma once

// Plane geometry of sketch mode in sketch coordinates (millimetres,
// radians): what the view needs to snap, infer, preview and hit-test
// without asking the model. The model stays the authority on the solved
// geometry; these curves are built from its `sketch` query.

#include <cmath>
#include <optional>
#include <vector>

namespace mitcad::sketch {

constexpr double kPi = 3.14159265358979323846;
constexpr double kTwoPi = 2.0 * kPi;

struct V2 {
  double x = 0.0;
  double y = 0.0;

  V2 operator+(const V2& o) const { return {x + o.x, y + o.y}; }
  V2 operator-(const V2& o) const { return {x - o.x, y - o.y}; }
  V2 operator*(double s) const { return {x * s, y * s}; }
  V2 operator/(double s) const { return {x / s, y / s}; }
  V2 operator-() const { return {-x, -y}; }
  bool operator==(const V2& o) const { return x == o.x && y == o.y; }
  bool operator!=(const V2& o) const { return !(*this == o); }
};

inline double dot(const V2& a, const V2& b) { return a.x * b.x + a.y * b.y; }
inline double cross(const V2& a, const V2& b) { return a.x * b.y - a.y * b.x; }
inline double norm(const V2& a) { return std::hypot(a.x, a.y); }
inline double dist(const V2& a, const V2& b) { return norm(a - b); }
inline V2 perp(const V2& a) { return {-a.y, a.x}; }
inline V2 unit(const V2& a) {
  const double n = norm(a);
  return n > 0.0 ? a / n : V2{1.0, 0.0};
}
inline V2 polar(double angle, double radius = 1.0) {
  return {std::cos(angle) * radius, std::sin(angle) * radius};
}
inline double angleOf(const V2& a) { return std::atan2(a.y, a.x); }
// An angle in [0, 2π).
double normalizeAngle(double angle);
// The counter-clockwise sweep from `start` to `end`, in (0, 2π].
double sweep(double start, double end);

// A curve of a sketch. Arcs and elliptical arcs run counter-clockwise from
// `start` to `end` (angles; ellipse parameters for elliptical arcs).
struct Curve2 {
  enum class Type { Line, Circle, Arc, Ellipse, EllipticalArc, Spline };

  Type type = Type::Line;
  V2 a, b;                // Line: start and end
  V2 center;              // circles, arcs, ellipses
  double radius = 0.0;    // circles and arcs; an ellipse's major radius
  double minor = 0.0;     // ellipses
  double rotation = 0.0;  // ellipses: the major axis from the sketch x axis
  double start = 0.0;     // arcs
  double end = 0.0;       //
  int degree = 3;         // Spline
  std::vector<V2> control;
  std::vector<double> weights; // empty: non-rational
  std::vector<double> knots;   // the full knot vector

  static Curve2 line(const V2& a, const V2& b);
  static Curve2 circle(const V2& center, double radius);
  static Curve2 arc(const V2& center, double radius, double start, double end);

  bool isClosed() const { return type == Type::Circle || type == Type::Ellipse; }
  bool isOpen() const { return !isClosed(); }
  // The parameter range: [0, 1] for lines and splines, angles otherwise.
  double t0() const;
  double t1() const;
  V2 at(double t) const;
  // The direction of travel at t (not normalized).
  V2 tangent(double t) const;
  V2 startPoint() const { return at(t0()); }
  V2 endPoint() const { return at(t1()); }
  // The middle of a line or an arc.
  V2 middle() const { return at((t0() + t1()) / 2.0); }
  // Points along the curve, no further apart than about `step` mm (and at
  // least a few).
  std::vector<V2> sample(double step) const;
  // The nearest point of the curve to p, with its parameter.
  V2 nearest(const V2& p, double* t = nullptr) const;
  double distanceTo(const V2& p) const { return dist(nearest(p), p); }
  // The parameter of a point on (or near) the curve.
  double parameterOf(const V2& p) const;
  // A closed curve's parameters wrap around.
  bool contains(double t) const;
};

// Where two curves cross (or touch), within the curves' extents.
std::vector<V2> intersections(const Curve2& a, const Curve2& b);

// The centre of the circle through three points; none when they are on a
// line.
std::optional<V2> circumcenter(const V2& a, const V2& b, const V2& c);

// A cubic B-spline through points (chord-length parameters, natural ends),
// as Bezier control points: for previews of fit-point splines.
std::vector<V2> fitSplinePreview(const std::vector<V2>& points, int samplesPerSpan = 16);
// A clamped uniform B-spline of control points, sampled.
std::vector<V2> controlSplinePreview(const std::vector<V2>& control, int degree,
                                     int samples = 96);

} // namespace mitcad::sketch
