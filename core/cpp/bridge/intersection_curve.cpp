// SPDX-License-Identifier: MIT
#include "bridge/intersection_curve.hpp"

#include <array>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Vec.hxx>

#include "bridge/sweeps.hpp"
#include "mitcad/geometry/intersection_curve.hpp"
#include "mitcad_bridge/kernel/intersection_curve.h"

namespace mitcad::bridge {
namespace {

std::array<double, 3> xyz(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

std::array<double, 3> xyz(const gp_Dir& d) { return {d.X(), d.Y(), d.Z()}; }

// The kinds as intersection_curve.rs reads them: 0 a point, 1 a line, 2 a
// conic, 3 a B-spline.
CrossingCurve convert(const geometry::ModelCurve& c) {
  CrossingCurve out{};
  switch (c.kind) {
  case geometry::ModelCurveKind::Point:
    out.kind = 0;
    break;
  case geometry::ModelCurveKind::Line:
    out.kind = 1;
    break;
  case geometry::ModelCurveKind::Conic:
    out.kind = 2;
    break;
  case geometry::ModelCurveKind::BSpline:
    out.kind = 3;
    break;
  case geometry::ModelCurveKind::Helix:
    throw std::invalid_argument("a helix is not an edge's curve");
  }
  out.start = xyz(c.start);
  out.end = xyz(c.end);
  out.center = xyz(c.center);
  out.normal = xyz(c.normal);
  out.x_axis = xyz(c.x_axis);
  out.major = c.major;
  out.minor = c.minor;
  out.first = c.first;
  out.last = c.last;
  out.closed = c.closed;
  out.degree = static_cast<std::uint32_t>(c.degree);
  for (const gp_Pnt& p : c.poles) {
    out.poles.push_back(p.X());
    out.poles.push_back(p.Y());
    out.poles.push_back(p.Z());
  }
  for (const double w : c.weights) {
    out.weights.push_back(w);
  }
  for (const double k : c.knots) {
    out.knots.push_back(k);
  }
  return out;
}

geometry::Path chain(rust::Slice<const SweepCurve> curves) {
  geometry::Path path;
  for (const SweepCurve& curve : curves) {
    path.push_back(path_curve_of(curve));
  }
  return path;
}

gp_Dir direction(const std::array<double, 3>& d, const char* which) {
  const gp_Vec along(d[0], d[1], d[2]);
  if (!(along.Magnitude() > 1e-12)) {
    throw std::invalid_argument(std::string("an intersection curve needs the ") + which + " sketch's normal");
  }
  return gp_Dir(along);
}

} // namespace

rust::Vec<CrossingCurve> intersection_curves(rust::Slice<const SweepCurve> first,
                                             std::array<double, 3> first_direction,
                                             rust::Slice<const SweepCurve> second,
                                             std::array<double, 3> second_direction) {
  const std::vector<geometry::ModelCurve> curves =
      geometry::intersection_curves(chain(first), direction(first_direction, "first"), chain(second),
                                    direction(second_direction, "second"));
  rust::Vec<CrossingCurve> result;
  for (const geometry::ModelCurve& c : curves) {
    result.push_back(convert(c));
  }
  return result;
}

} // namespace mitcad::bridge
