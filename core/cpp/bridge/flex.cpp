// SPDX-License-Identifier: MIT
#include "bridge/flex.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "bridge/profile.hpp"
#include "bridge/sweeps.hpp"
#include "mitcad/geometry/flex.hpp"
#include "mitcad_bridge/kernel/flex.h"

namespace mitcad::bridge {
namespace {

gp_Pnt point(const std::array<double, 3>& p) { return gp_Pnt(p[0], p[1], p[2]); }

std::array<double, 3> xyz(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

std::array<double, 3> xyz(const gp_Dir& d) { return {d.X(), d.Y(), d.Z()}; }

// A unit direction; an error that names `what` for a vector without
// length.
gp_Dir direction(const std::array<double, 3>& d, const char* what) {
  const gp_Vec along(d[0], d[1], d[2]);
  if (!(along.Magnitude() > 1e-12)) {
    throw std::invalid_argument(std::string("a flex item needs ") + what);
  }
  return gp_Dir(along);
}

// A limit as flex.rs gives it: a value above zero, none otherwise.
std::optional<double> limit(double value) {
  if (value > 0.0) {
    return value;
  }
  return std::nullopt;
}

// The kinds as flex.rs reads them: 0 a point, 1 a line, 2 a conic, 3 a
// B-spline.
FlexCurve convert(const geometry::ModelCurve& c) {
  FlexCurve out{};
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
    throw std::invalid_argument("a helix is not a piece of a flex item's route");
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

FlexSampleData convert(const geometry::FlexSample& sample) {
  FlexSampleData out{};
  out.along = sample.along;
  out.point = xyz(sample.point);
  out.tangent = xyz(sample.tangent);
  out.normal = xyz(sample.normal);
  out.node = static_cast<std::int32_t>(sample.node);
  out.obstacle = static_cast<std::int32_t>(sample.obstacle);
  out.face = rust::String(sample.face);
  return out;
}

geometry::FlexSample sample_of(const FlexSampleData& sample) {
  geometry::FlexSample out;
  out.along = sample.along;
  out.point = point(sample.point);
  out.tangent = direction(sample.tangent, "the direction of each sample of its route");
  out.normal = direction(sample.normal, "the orientation of each sample of its route");
  out.node = static_cast<int>(sample.node);
  out.obstacle = static_cast<int>(sample.obstacle);
  out.face = std::string(sample.face);
  return out;
}

// The kinds as flex.rs reads them: 0 passes through, 1 a bend too tight,
// 2 a twist over the limit, 3 no clean route, 4 a bend over the edge too
// tight.
FlexProblemData convert(const geometry::FlexProblem& problem) {
  FlexProblemData out{};
  switch (problem.kind) {
  case geometry::FlexProblemKind::PassesThrough:
    out.kind = 0;
    break;
  case geometry::FlexProblemKind::BendTooTight:
    out.kind = 1;
    break;
  case geometry::FlexProblemKind::TwistOverLimit:
    out.kind = 2;
    break;
  case geometry::FlexProblemKind::NoCleanRoute:
    out.kind = 3;
    break;
  case geometry::FlexProblemKind::EdgeBendTooTight:
    out.kind = 4;
    break;
  }
  out.from = problem.from;
  out.to = problem.to;
  out.obstacle = static_cast<std::int32_t>(problem.obstacle);
  for (const gp_Pnt& p : problem.points) {
    out.points.push_back(p.X());
    out.points.push_back(p.Y());
    out.points.push_back(p.Z());
  }
  for (const std::array<gp_Pnt, 2>& box : problem.boxes) {
    for (const gp_Pnt& corner : box) {
      out.boxes.push_back(corner.X());
      out.boxes.push_back(corner.Y());
      out.boxes.push_back(corner.Z());
    }
  }
  return out;
}

} // namespace

FlexRouteData flex_route(rust::Str feature, const ShapeList& obstacles, const FlexRouteInput& input) {
  geometry::FlexRouteSpec spec;
  spec.feature = std::string(feature);
  for (const FlexNodeInput& node : input.nodes) {
    geometry::FlexNode item;
    item.point = point(node.point);
    if (node.obstacle >= 0) {
      const std::size_t body = static_cast<std::size_t>(node.obstacle);
      if (body >= obstacles.size() || !obstacles.at(body)) {
        throw std::invalid_argument("the body of a flex item's node is missing");
      }
      item.obstacle = static_cast<int>(node.obstacle);
      item.face = std::string(node.face);
      item.normal = direction(node.normal, "the normal of the face each node lies on");
    }
    if (node.has_direction) {
      item.direction = direction(node.direction, "the direction of the port it begins or ends at");
    }
    if (node.has_x_axis) {
      item.x_axis = direction(node.x_axis, "the orientation of the port it begins or ends at");
    }
    item.twist = node.twist;
    spec.nodes.push_back(std::move(item));
  }
  spec.resting = input.resting;
  spec.half_width = input.half_width;
  spec.reach = input.reach;
  spec.bend_radius = limit(input.bend_radius);
  spec.bend_radius_edge = limit(input.bend_radius_edge);
  spec.max_twist = limit(input.max_twist);
  spec.quick = input.quick;
  const geometry::FlexRoute route = geometry::flex_route(obstacles.items(), spec);
  FlexRouteData out{};
  for (const geometry::ModelCurve& curve : route.curve) {
    out.curve.push_back(convert(curve));
  }
  for (const geometry::FlexSample& sample : route.samples) {
    out.samples.push_back(convert(sample));
  }
  for (const geometry::FlexProblem& problem : route.problems) {
    out.problems.push_back(convert(problem));
  }
  return out;
}

std::shared_ptr<geometry::Shape> flex_solid(rust::Str feature, rust::Slice<const SweepCurve> path,
                                            rust::Slice<const Region> outline, const FlexSolidInput& input) {
  geometry::FlexSolidSpec spec;
  spec.feature = std::string(feature);
  for (const SweepCurve& curve : path) {
    spec.path.push_back(path_curve_of(curve));
  }
  for (const FlexSampleData& sample : input.samples) {
    spec.samples.push_back(sample_of(sample));
  }
  spec.round = input.round;
  spec.rectangular = input.rectangular;
  spec.width = input.width;
  spec.height = input.height;
  spec.corner_radius = input.corner_radius;
  spec.outline = to_geometry(outline);
  spec.wall = limit(input.wall);
  return geometry::flex_solid(spec);
}

FlexPortData flex_end_port(const geometry::Shape& shape, rust::Str cap) {
  const geometry::FlexPort port = geometry::end_port(shape, std::string(cap));
  FlexPortData out{};
  out.position = xyz(port.position);
  out.direction = xyz(port.direction);
  out.x_axis = xyz(port.x_axis);
  out.size = {port.width, port.height};
  return out;
}

} // namespace mitcad::bridge
