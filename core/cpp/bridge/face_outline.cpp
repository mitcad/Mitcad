// SPDX-License-Identifier: MIT
#include "bridge/face_outline.hpp"

#include <array>
#include <cstdint>
#include <stdexcept>
#include <string>

#include <gp_Dir.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/face_outline.hpp"
#include "mitcad_bridge/kernel/face_outline.h"

namespace mitcad::bridge {
namespace {

std::array<double, 3> xyz(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

std::array<double, 3> xyz(const gp_Dir& d) { return {d.X(), d.Y(), d.Z()}; }

// The kinds as face_outline.rs reads them: 0 a point, 1 a line, 2 a conic,
// 3 a B-spline.
OutlineCurve convert(const geometry::ModelCurve& c) {
  OutlineCurve out{};
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

} // namespace

rust::Vec<OutlineCurve> face_outline(const geometry::Shape& shape, rust::Str face, std::array<double, 3> direction) {
  const gp_Vec along(direction[0], direction[1], direction[2]);
  if (!(along.Magnitude() > 1e-12)) {
    throw std::invalid_argument("a face's outline needs the direction it is seen along");
  }
  rust::Vec<OutlineCurve> result;
  for (const geometry::ModelCurve& c : geometry::face_outline(shape, std::string(face), gp_Dir(along))) {
    result.push_back(convert(c));
  }
  return result;
}

} // namespace mitcad::bridge
