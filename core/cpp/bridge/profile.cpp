// SPDX-License-Identifier: MIT
#include "bridge/profile.hpp"

#include <array>
#include <cstdint>
#include <stdexcept>
#include <string>

#include "mitcad_bridge/kernel/profile.h"

namespace mitcad::bridge {
namespace {

gp_Pnt2d point(const std::array<double, 2>& p) { return gp_Pnt2d(p[0], p[1]); }

gp_Dir direction(const std::array<double, 3>& d) { return gp_Dir(d[0], d[1], d[2]); }

geometry::Segment to_geometry(const Segment& s) {
  geometry::Segment segment;
  segment.name = std::string(s.name);
  switch (s.kind) {
  case SegmentKind::Line:
    segment.kind = geometry::SegmentKind::Line;
    break;
  case SegmentKind::Arc:
    segment.kind = geometry::SegmentKind::Arc;
    break;
  case SegmentKind::Circle:
    segment.kind = geometry::SegmentKind::Circle;
    break;
  case SegmentKind::Ellipse:
    segment.kind = geometry::SegmentKind::Ellipse;
    break;
  case SegmentKind::EllipseArc:
    segment.kind = geometry::SegmentKind::EllipseArc;
    break;
  case SegmentKind::BSpline:
    segment.kind = geometry::SegmentKind::BSpline;
    break;
  default:
    throw std::invalid_argument("unknown segment kind");
  }
  segment.start = point(s.start);
  segment.end = point(s.end);
  segment.center = point(s.center);
  segment.radius = s.radius;
  segment.minor_radius = s.minor_radius;
  segment.rotation = s.rotation;
  segment.start_angle = s.start_angle;
  segment.end_angle = s.end_angle;
  segment.degree = static_cast<int>(s.degree);
  for (std::size_t i = 0; i + 1 < s.poles.size(); i += 2) {
    segment.poles.emplace_back(s.poles[i], s.poles[i + 1]);
  }
  segment.weights.assign(s.weights.begin(), s.weights.end());
  segment.knots.assign(s.knots.begin(), s.knots.end());
  for (const std::uint32_t m : s.multiplicities) {
    segment.multiplicities.push_back(static_cast<int>(m));
  }
  segment.periodic = s.periodic;
  return segment;
}

} // namespace

geometry::Frame to_geometry(const Frame& frame) {
  geometry::Frame result;
  result.origin = gp_Pnt(frame.origin[0], frame.origin[1], frame.origin[2]);
  result.x_axis = direction(frame.x_axis);
  result.y_axis = direction(frame.y_axis);
  return result;
}

std::vector<geometry::Region> to_geometry(rust::Slice<const Region> regions) {
  std::vector<geometry::Region> result;
  for (const Region& r : regions) {
    geometry::Region region;
    region.name = std::string(r.name);
    for (const Loop& l : r.loops) {
      geometry::Loop loop;
      for (const Segment& s : l.segments) {
        loop.segments.push_back(to_geometry(s));
      }
      region.loops.push_back(std::move(loop));
    }
    result.push_back(std::move(region));
  }
  return result;
}

std::shared_ptr<geometry::Shape> profile_shape(const Frame& frame, rust::Slice<const Region> regions) {
  return geometry::profile_shape(to_geometry(frame), to_geometry(regions));
}

} // namespace mitcad::bridge
