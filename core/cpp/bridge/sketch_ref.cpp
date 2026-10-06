// SPDX-License-Identifier: MIT
#include "bridge/sketch_ref.hpp"

#include <array>
#include <cstdint>
#include <string>

#include "mitcad/geometry/sketch_ref.hpp"
#include "mitcad_bridge/kernel/sketch_ref.h"

namespace mitcad::bridge {
namespace {

std::array<double, 3> xyz(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

std::array<double, 3> xyz(const gp_Dir& d) { return {d.X(), d.Y(), d.Z()}; }

ModelCurve convert(const geometry::ModelCurve& c) {
  ModelCurve out{};
  switch (c.kind) {
  case geometry::ModelCurveKind::Point:
    out.kind = ModelCurveKind::Point;
    break;
  case geometry::ModelCurveKind::Line:
    out.kind = ModelCurveKind::Line;
    break;
  case geometry::ModelCurveKind::Conic:
    out.kind = ModelCurveKind::Conic;
    break;
  case geometry::ModelCurveKind::BSpline:
    out.kind = ModelCurveKind::BSpline;
    break;
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

rust::Vec<ModelCurve> model_curves(const geometry::Shape& shape, rust::Str name) {
  rust::Vec<ModelCurve> result;
  for (const geometry::ModelCurve& c : geometry::curves_of(shape, std::string(name))) {
    result.push_back(convert(c));
  }
  return result;
}

IndexedElement indexed_element(const geometry::Shape& shape, std::uint8_t kind, std::size_t index) {
  const geometry::IndexedElement element =
      geometry::indexed_element(shape, static_cast<char>(kind), static_cast<int>(index));
  IndexedElement out{};
  out.found = element.found;
  out.name = rust::String(element.name);
  for (const geometry::ModelCurve& c : element.curves) {
    out.curves.push_back(convert(c));
  }
  return out;
}

} // namespace mitcad::bridge
