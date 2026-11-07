// SPDX-License-Identifier: MIT
// Intersection curves (mitcad#1405): a half circle on XZ swept along Y
// crossed by a slanted line on XY swept along Z gives the one half ellipse
// where the half cylinder meets the plane, from end to end of the arc; two
// circles of one radius on XZ and on XY give the two ellipses where the
// cylinders meet, which cross where the cylinders touch (a branching
// result the model refuses); parallel normals are refused.

#include <cmath>
#include <cstddef>
#include <cstdio>
#include <exception>
#include <vector>

#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/intersection_curve.hpp"

namespace {

using mitcad::geometry::intersection_curves;
using mitcad::geometry::ModelCurve;
using mitcad::geometry::ModelCurveKind;
using mitcad::geometry::Path;
using mitcad::geometry::PathCurve;
using test::kPi;

// The comparisons' tolerance (mm).
constexpr double kTolerance = 1e-6;

// A circle or an arc of radius `radius` about the origin, normal `normal`,
// starting along +X: from `first` to `last` radians, or whole.
PathCurve arc(const gp_Dir& normal, double radius, double first, double last, bool closed) {
  PathCurve piece;
  piece.name = "c1";
  ModelCurve& c = piece.curve;
  c.kind = ModelCurveKind::Conic;
  c.center = gp_Pnt(0.0, 0.0, 0.0);
  c.normal = normal;
  c.x_axis = gp_Dir(1.0, 0.0, 0.0);
  c.major = radius;
  c.minor = radius;
  c.first = first;
  c.last = last;
  c.closed = closed;
  return piece;
}

PathCurve line(const gp_Pnt& start, const gp_Pnt& end) {
  PathCurve piece;
  piece.name = "c1";
  piece.curve.kind = ModelCurveKind::Line;
  piece.curve.start = start;
  piece.curve.end = end;
  return piece;
}

// The point of a crossing piece at `fraction` (0 to 1) of its parameter
// range.
gp_Pnt point_at(const ModelCurve& c, double fraction) {
  switch (c.kind) {
  case ModelCurveKind::Line:
    return gp_Pnt(c.start.XYZ() + (c.end.XYZ() - c.start.XYZ()) * fraction);
  case ModelCurveKind::Conic: {
    const double t = c.first + (c.last - c.first) * fraction;
    const gp_Vec x(c.x_axis);
    const gp_Vec y = gp_Vec(c.normal).Crossed(x);
    return c.center.Translated(x * (c.major * std::cos(t)) + y * (c.minor * std::sin(t)));
  }
  case ModelCurveKind::BSpline: {
    // The full knot vector as distinct knots and multiplicities.
    std::vector<double> distinct;
    std::vector<int> mults;
    for (const double k : c.knots) {
      if (!distinct.empty() && std::abs(k - distinct.back()) <= 1e-12) {
        ++mults.back();
      } else {
        distinct.push_back(k);
        mults.push_back(1);
      }
    }
    NCollection_Array1<gp_Pnt> poles(1, static_cast<int>(c.poles.size()));
    for (std::size_t i = 0; i < c.poles.size(); ++i) {
      poles.SetValue(static_cast<int>(i) + 1, c.poles[i]);
    }
    NCollection_Array1<double> knots(1, static_cast<int>(distinct.size()));
    NCollection_Array1<int> multiplicities(1, static_cast<int>(distinct.size()));
    for (std::size_t i = 0; i < distinct.size(); ++i) {
      knots.SetValue(static_cast<int>(i) + 1, distinct[i]);
      multiplicities.SetValue(static_cast<int>(i) + 1, mults[i]);
    }
    occ::handle<Geom_BSplineCurve> spline;
    if (c.weights.empty()) {
      spline = new Geom_BSplineCurve(poles, knots, multiplicities, c.degree);
    } else {
      NCollection_Array1<double> weights(1, static_cast<int>(c.weights.size()));
      for (std::size_t i = 0; i < c.weights.size(); ++i) {
        weights.SetValue(static_cast<int>(i) + 1, c.weights[i]);
      }
      spline = new Geom_BSplineCurve(poles, weights, knots, multiplicities, c.degree);
    }
    const double t0 = spline->FirstParameter();
    const double t1 = spline->LastParameter();
    return spline->Value(t0 + (t1 - t0) * fraction);
  }
  default:
    return c.start;
  }
}

// Whether every piece lies on both surfaces at nine points along it.
template <class OnFirst, class OnSecond>
bool on_both(const std::vector<ModelCurve>& pieces, OnFirst on_first, OnSecond on_second) {
  for (const ModelCurve& c : pieces) {
    for (int i = 0; i <= 8; ++i) {
      const gp_Pnt p = point_at(c, i / 8.0);
      if (!on_first(p) || !on_second(p)) {
        std::fprintf(stderr, "  off a surface: (%.9g, %.9g, %.9g)\n", p.X(), p.Y(), p.Z());
        return false;
      }
    }
  }
  return true;
}

// How many ends of the pieces are at `p`.
int ends_at(const std::vector<ModelCurve>& pieces, const gp_Pnt& p) {
  int count = 0;
  for (const ModelCurve& c : pieces) {
    if (c.closed) {
      continue;
    }
    for (const double fraction : {0.0, 1.0}) {
      if (point_at(c, fraction).Distance(p) <= kTolerance) {
        ++count;
      }
    }
  }
  return count;
}

// The upper half of a circle of radius 20 on XZ (its normal -Y, so the arc
// from +X over +Z to -X) swept along Y, crossed by the line y = x + 10 on XY
// swept along Z: the half ellipse x^2 + z^2 = 400, y = x + 10, z >= 0 from
// (20, 30, 0) to (-20, -10, 0).
void test_arc_and_line() {
  const Path first{arc(gp_Dir(0.0, -1.0, 0.0), 20.0, 0.0, kPi, false)};
  const Path second{line(gp_Pnt(-30.0, -20.0, 0.0), gp_Pnt(30.0, 40.0, 0.0))};
  const std::vector<ModelCurve> pieces =
      intersection_curves(first, gp_Dir(0.0, 1.0, 0.0), second, gp_Dir(0.0, 0.0, 1.0));
  CHECK(!pieces.empty());
  CHECK(on_both(
      pieces,
      [](const gp_Pnt& p) {
        return std::abs(std::hypot(p.X(), p.Z()) - 20.0) <= kTolerance && p.Z() >= -kTolerance;
      },
      [](const gp_Pnt& p) { return std::abs(p.Y() - p.X() - 10.0) <= kTolerance; }));
  // One stretch from end to end of the arc: each end once, the pieces'
  // other ends paired.
  CHECK(ends_at(pieces, gp_Pnt(20.0, 30.0, 0.0)) == 1);
  CHECK(ends_at(pieces, gp_Pnt(-20.0, -10.0, 0.0)) == 1);
  // Through the top of the arc.
  bool top = false;
  for (const ModelCurve& c : pieces) {
    for (int i = 0; i <= 64; ++i) {
      top = top || point_at(c, i / 64.0).Distance(gp_Pnt(0.0, 10.0, 20.0)) <= 0.5;
    }
  }
  CHECK(top);
}

// Circles of radius 20 about the origin on XZ and on XY, swept along Y and
// along Z: the cylinders meet in the two ellipses y = z and y = -z, which
// cross at (20, 0, 0) and (-20, 0, 0), where the cylinders touch: more
// than two pieces end there, or the two ellipses come whole (closed pieces
// that cross), either way not one curve.
void test_two_cylinders_branch() {
  const Path first{arc(gp_Dir(0.0, 1.0, 0.0), 20.0, 0.0, 2.0 * kPi, true)};
  const Path second{arc(gp_Dir(0.0, 0.0, 1.0), 20.0, 0.0, 2.0 * kPi, true)};
  const std::vector<ModelCurve> pieces =
      intersection_curves(first, gp_Dir(0.0, 1.0, 0.0), second, gp_Dir(0.0, 0.0, 1.0));
  CHECK(!pieces.empty());
  CHECK(on_both(
      pieces, [](const gp_Pnt& p) { return std::abs(std::hypot(p.X(), p.Z()) - 20.0) <= kTolerance; },
      [](const gp_Pnt& p) { return std::abs(std::hypot(p.X(), p.Y()) - 20.0) <= kTolerance; }));
  int closed = 0;
  for (const ModelCurve& c : pieces) {
    closed += c.closed ? 1 : 0;
  }
  CHECK(ends_at(pieces, gp_Pnt(-20.0, 0.0, 0.0)) > 2 || closed >= 2);
}

// Sketches whose normals are parallel sweep their curves the same way: no
// curve, an error.
void test_parallel_normals_refused() {
  const Path first{line(gp_Pnt(0.0, 0.0, 0.0), gp_Pnt(10.0, 0.0, 0.0))};
  const Path second{line(gp_Pnt(5.0, -5.0, 3.0), gp_Pnt(5.0, 5.0, 3.0))};
  bool thrown = false;
  try {
    intersection_curves(first, gp_Dir(0.0, 0.0, 1.0), second, gp_Dir(0.0, 0.0, -1.0));
  } catch (const std::exception&) {
    thrown = true;
  }
  CHECK(thrown);
}

} // namespace

int main() {
  test::guarded("arc and line", test_arc_and_line);
  test::guarded("two cylinders branch", test_two_cylinders_branch);
  test::guarded("parallel normals refused", test_parallel_normals_refused);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("intersection curve: all checks passed\n");
  return 0;
}
