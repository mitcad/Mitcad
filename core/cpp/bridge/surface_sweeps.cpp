// SPDX-License-Identifier: MIT
#include "bridge/surface_sweeps.hpp"

#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "bridge/sweeps.hpp"
#include "mitcad/geometry/surface_sweeps.hpp"
#include "mitcad_bridge/kernel/surface_sweeps.h"

namespace mitcad::bridge {
namespace {

std::vector<geometry::Path> chains_of(rust::Slice<const SweepCurve> curves, rust::Slice<const std::size_t> chains) {
  std::vector<geometry::Path> result;
  std::size_t next = 0;
  for (const std::size_t count : chains) {
    if (count > curves.size() - next) {
      throw std::invalid_argument("a chain of curves runs past the curves given");
    }
    geometry::Path chain;
    for (std::size_t i = 0; i < count; ++i, ++next) {
      chain.push_back(path_curve_of(curves[next]));
    }
    result.push_back(std::move(chain));
  }
  return result;
}

geometry::Path path_of(rust::Slice<const SweepCurve> curves) {
  geometry::Path path;
  for (const SweepCurve& curve : curves) {
    path.push_back(path_curve_of(curve));
  }
  return path;
}

// Throws unless there are `count` values.
void need(rust::Slice<const double> values, std::size_t count, const char* what) {
  if (values.size() < count) {
    throw std::invalid_argument(std::string(what) + " needs " + std::to_string(count) + " values");
  }
}

gp_Pnt point(rust::Slice<const double> v, std::size_t at = 0) { return gp_Pnt(v[at], v[at + 1], v[at + 2]); }

gp_Dir direction(rust::Slice<const double> v, std::size_t at, const char* what) {
  const gp_Vec vector(v[at], v[at + 1], v[at + 2]);
  if (vector.Magnitude() < 1.0e-12) {
    throw std::invalid_argument(std::string(what) + " has no length");
  }
  return gp_Dir(vector);
}

} // namespace

std::shared_ptr<geometry::Shape> open_faces(const geometry::Shape& shape, rust::Str feature,
                                            rust::Slice<const rust::String> roles) {
  std::vector<std::string> names;
  for (const rust::String& role : roles) {
    names.emplace_back(role);
  }
  return geometry::open_faces(shape, std::string(feature), names);
}

std::shared_ptr<geometry::Shape> extrude_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                               rust::Slice<const std::size_t> chains,
                                               rust::Slice<const double> shift,
                                               rust::Slice<const double> direction_, double one, double two) {
  need(shift, 3, "the shift");
  need(direction_, 3, "the direction");
  return geometry::extrude_sheet(std::string(feature), chains_of(curves, chains),
                                 gp_Vec(shift[0], shift[1], shift[2]), direction(direction_, 0, "the direction"),
                                 one, two);
}

std::shared_ptr<geometry::Shape> revolve_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                               rust::Slice<const std::size_t> chains,
                                               rust::Slice<const double> origin,
                                               rust::Slice<const double> direction_, double angle1,
                                               double angle2) {
  need(origin, 3, "the axis");
  need(direction_, 3, "the axis");
  return geometry::revolve_sheet(std::string(feature), chains_of(curves, chains),
                                 gp_Ax1(point(origin), direction(direction_, 0, "the axis")), angle1, angle2);
}

std::shared_ptr<geometry::Shape> sweep_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                             rust::Slice<const std::size_t> chains,
                                             rust::Slice<const SweepCurve> path, rust::Slice<const double> plane,
                                             rust::Slice<const double> extents, bool parallel) {
  need(plane, 6, "the profile plane");
  need(extents, 2, "the extents");
  return geometry::sweep_sheet(std::string(feature), chains_of(curves, chains), path_of(path),
                               gp_Pln(point(plane), direction(plane, 3, "the profile plane")), extents[0],
                               extents[1],
                               parallel ? geometry::SweepOrientation::Parallel
                                        : geometry::SweepOrientation::Perpendicular);
}

std::shared_ptr<geometry::Shape> loft_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                            rust::Slice<const std::size_t> chains, bool ruled, bool closed) {
  return geometry::loft_sheet(std::string(feature), chains_of(curves, chains), ruled, closed);
}

std::shared_ptr<geometry::Shape> helix_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                             rust::Slice<const std::size_t> chains, rust::Slice<const double> axis,
                                             double pitch, double revolutions, bool left_handed) {
  need(axis, 6, "the axis");
  return geometry::helix_sheet(std::string(feature), chains_of(curves, chains),
                               gp_Ax1(point(axis), direction(axis, 3, "the axis")), pitch, revolutions, left_handed);
}

} // namespace mitcad::bridge
