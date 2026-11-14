// SPDX-License-Identifier: MIT
#include "bridge/intersection_curve.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
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

rust::Vec<CrossingCurve> converted(const std::vector<geometry::ModelCurve>& curves) {
  rust::Vec<CrossingCurve> result;
  for (const geometry::ModelCurve& c : curves) {
    result.push_back(convert(c));
  }
  return result;
}

gp_Dir unit_of(const std::array<double, 3>& d, const char* what) {
  const gp_Vec along(d[0], d[1], d[2]);
  if (!(along.Magnitude() > 1e-12)) {
    throw std::invalid_argument(std::string("the helix's ") + what + " has no length");
  }
  return gp_Dir(along);
}

// One side of a section: shape i with the next counts[i] face names.
std::vector<geometry::SectionPart> section_side(const ShapeList& shapes, rust::Slice<const rust::String> faces,
                                                rust::Slice<const std::size_t> counts) {
  if (counts.size() != shapes.size()) {
    throw std::invalid_argument("a section needs a count of faces for each shape");
  }
  std::vector<geometry::SectionPart> side;
  std::size_t next = 0;
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    geometry::SectionPart part;
    part.shape = shapes.at(i);
    if (!part.shape) {
      throw std::invalid_argument("a body of the section has no shape");
    }
    if (counts[i] > faces.size() - next) {
      throw std::invalid_argument("a section's face names run past those given");
    }
    for (std::size_t k = 0; k < counts[i]; ++k, ++next) {
      part.faces.emplace_back(faces[next]);
    }
    side.push_back(std::move(part));
  }
  return side;
}

} // namespace

rust::Vec<CrossingCurve> intersection_curves(rust::Slice<const SweepCurve> first,
                                             std::array<double, 3> first_direction,
                                             rust::Slice<const SweepCurve> second,
                                             std::array<double, 3> second_direction) {
  const std::vector<geometry::ModelCurve> curves =
      geometry::intersection_curves(chain(first), direction(first_direction, "first"), chain(second),
                                    direction(second_direction, "second"));
  return converted(curves);
}

rust::Vec<CrossingCurve> helix_curve(const HelixCurveInput& input) {
  geometry::HelixCurve helix;
  helix.origin = gp_Pnt(input.origin[0], input.origin[1], input.origin[2]);
  helix.direction = unit_of(input.direction, "axis");
  helix.x_axis = unit_of(input.x_axis, "start direction");
  helix.radius = input.radius;
  helix.pitch = input.pitch;
  helix.revolutions = input.revolutions;
  helix.left_handed = input.left_handed;
  helix.growth = input.growth;
  return converted(geometry::helix_curve(helix));
}

rust::Vec<CrossingCurve> shape_section(const ShapeList& first, rust::Slice<const rust::String> first_faces,
                                       rust::Slice<const std::size_t> first_counts, const ShapeList& second,
                                       rust::Slice<const rust::String> second_faces,
                                       rust::Slice<const std::size_t> second_counts) {
  return converted(geometry::shape_section(section_side(first, first_faces, first_counts),
                                           section_side(second, second_faces, second_counts)));
}

} // namespace mitcad::bridge
