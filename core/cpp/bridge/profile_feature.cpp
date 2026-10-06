// SPDX-License-Identifier: MIT
#include "bridge/profile_feature.hpp"

#include <array>
#include <optional>
#include <stdexcept>
#include <string>

#include "bridge/profile.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/hole.hpp"
#include "mitcad/geometry/reference.hpp"
#include "mitcad/geometry/revolve.hpp"
#include "mitcad/geometry/thread.hpp"
#include "mitcad_bridge/kernel/profile_feature.h"

namespace mitcad::bridge {
namespace {

gp_Pnt point(const std::array<double, 3>& p) { return gp_Pnt(p[0], p[1], p[2]); }
gp_Dir direction(const std::array<double, 3>& d) { return gp_Dir(d[0], d[1], d[2]); }

std::array<double, 3> array(const gp_XYZ& xyz) { return {xyz.X(), xyz.Y(), xyz.Z()}; }

geometry::ShapePtr body_at(const ShapeList& bodies, std::size_t index) {
  if (index >= bodies.size() || !bodies.at(index)) {
    throw std::invalid_argument("a target's body is missing");
  }
  return bodies.at(index);
}

// None for an unset target.
std::optional<geometry::Target> target(const TargetInput& input, const ShapeList& bodies) {
  geometry::Target result;
  result.offset = input.offset;
  switch (input.kind) {
  case TargetKind::Unset:
    return std::nullopt;
  case TargetKind::Plane:
    result.kind = geometry::Target::Kind::Plane;
    result.plane = gp_Pln(point(input.plane_origin), direction(input.plane_normal));
    break;
  case TargetKind::Face:
    result.kind = geometry::Target::Kind::Face;
    result.body = body_at(bodies, input.body);
    result.face = std::string(input.face);
    result.extend = input.extend;
    break;
  case TargetKind::Body:
    result.kind = geometry::Target::Kind::Body;
    result.body = body_at(bodies, input.body);
    result.through = input.through;
    break;
  default:
    throw std::invalid_argument("unknown target kind");
  }
  return result;
}

geometry::ExtrudeSide side(const ExtrudeSideInput& input, const ShapeList& bodies) {
  geometry::ExtrudeSide result;
  result.target = target(input.target, bodies);
  result.distance = input.distance;
  result.taper = input.taper;
  if (input.thin) {
    geometry::ThinWall wall;
    wall.location = input.wall_location == 0   ? geometry::WallLocation::Side1
                    : input.wall_location == 2 ? geometry::WallLocation::Side2
                                               : geometry::WallLocation::Center;
    wall.thickness = input.wall_thickness;
    result.thin = wall;
  }
  return result;
}

AxisOutput axis_output(const gp_Ax1& axis) {
  return AxisOutput{array(axis.Location().XYZ()), array(axis.Direction().XYZ())};
}

} // namespace

std::shared_ptr<geometry::Shape> extrude_feature(rust::Str feature, const Frame& frame,
                                                 rust::Slice<const Region> regions,
                                                 const ExtrudeFeatureInput& input,
                                                 const ShapeList& bodies) {
  geometry::ExtrudeFeatureSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.regions = to_geometry(regions);
  spec.direction = direction(input.direction);
  spec.start_offset = input.start_offset;
  spec.start = target(input.start, bodies);
  spec.side1 = side(input.side1, bodies);
  if (input.two_sides) {
    spec.side2 = side(input.side2, bodies);
  }
  return geometry::extrude_feature(spec);
}

std::shared_ptr<geometry::Shape> revolve(rust::Str feature, const Frame& frame,
                                         rust::Slice<const Region> regions, const RevolveInput& input,
                                         const ShapeList& bodies) {
  geometry::RevolveSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.regions = to_geometry(regions);
  spec.axis = gp_Ax1(point(input.axis_origin), direction(input.axis_direction));
  spec.angle1 = input.angle1;
  if (input.two_sides) {
    spec.angle2 = input.angle2;
  }
  spec.target = target(input.target, bodies);
  return geometry::revolve(spec);
}

std::shared_ptr<geometry::Shape> hole_tool(rust::Str feature, const HoleInput& input,
                                           const ShapeList& bodies) {
  geometry::HoleSpec spec;
  spec.feature = std::string(feature);
  if (input.origins.size() != input.directions.size() || input.origins.size() % 3 != 0) {
    throw std::invalid_argument("the hole positions are not points and directions");
  }
  for (std::size_t i = 0; i + 2 < input.origins.size(); i += 3) {
    spec.positions.emplace_back(gp_Pnt(input.origins[i], input.origins[i + 1], input.origins[i + 2]),
                                gp_Dir(input.directions[i], input.directions[i + 1],
                                       input.directions[i + 2]));
  }
  spec.diameter = input.diameter;
  spec.type = input.shape == 1   ? geometry::HoleType::Counterbore
              : input.shape == 2 ? geometry::HoleType::Countersink
              : input.shape == 3 ? geometry::HoleType::Counterdrill
                                 : geometry::HoleType::Simple;
  spec.taper = input.taper;
  spec.counterbore_diameter = input.counterbore_diameter;
  spec.counterbore_depth = input.counterbore_depth;
  spec.countersink_diameter = input.countersink_diameter;
  spec.countersink_angle = input.countersink_angle;
  spec.flat = input.flat;
  if (!input.flat) {
    spec.tip_angle = input.tip_angle;
  }
  spec.depth = input.depth;
  switch (input.extent) {
  case 1:
    spec.extent = geometry::HoleExtent::ThroughAll;
    for (std::size_t i = 0; i < input.through; ++i) {
      spec.bodies.push_back(body_at(bodies, i));
    }
    break;
  case 2:
    spec.extent = geometry::HoleExtent::Target;
    spec.target = target(input.target, bodies);
    break;
  default:
    spec.extent = geometry::HoleExtent::Distance;
    break;
  }
  return geometry::hole_tool(spec);
}

std::shared_ptr<geometry::Shape> modeled_thread(rust::Str feature, const geometry::Shape& body,
                                                const ThreadInput& input) {
  geometry::ThreadSpec spec;
  spec.feature = std::string(feature);
  for (const rust::String& face : input.faces) {
    spec.faces.emplace_back(face);
  }
  spec.pitch = input.pitch;
  spec.depth = input.depth;
  spec.right_handed = input.right_handed;
  spec.full_length = input.full_length;
  spec.length = input.length;
  spec.offset = input.offset;
  spec.high_end = input.high_end;
  return geometry::modeled_thread(body, spec);
}

PlaneOutput face_plane(const geometry::Shape& shape, rust::Str face) {
  const gp_Pln plane = geometry::face_plane(shape, std::string(face));
  return PlaneOutput{array(plane.Location().XYZ()), array(plane.Axis().Direction().XYZ())};
}

CylinderOutput face_cylinder(const geometry::Shape& shape, rust::Str face) {
  const geometry::CylinderFace cylinder = geometry::face_cylinder(shape, std::string(face));
  return CylinderOutput{axis_output(cylinder.axis), cylinder.radius, cylinder.length, cylinder.internal};
}

} // namespace mitcad::bridge
