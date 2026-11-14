// SPDX-License-Identifier: MIT
#include "bridge/sweeps.hpp"

#include <array>
#include <cstdint>
#include <iterator>
#include <stdexcept>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Vec.hxx>

#include "bridge/profile.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/rib.hpp"
#include "mitcad_bridge/kernel/sweeps.h"

namespace mitcad::bridge {
namespace {

gp_Pnt point(const std::array<double, 3>& p) { return gp_Pnt(p[0], p[1], p[2]); }
gp_Dir direction(const std::array<double, 3>& d) { return gp_Dir(d[0], d[1], d[2]); }

geometry::PathCurve path_curve(const SweepCurve& input) {
  geometry::PathCurve piece;
  piece.name = std::string(input.name);
  geometry::ModelCurve& curve = piece.curve;
  switch (input.kind) {
  case 0:
    curve.kind = geometry::ModelCurveKind::Line;
    curve.start = point(input.start);
    curve.end = point(input.end);
    break;
  case 1:
    curve.kind = geometry::ModelCurveKind::Conic;
    curve.center = point(input.center);
    curve.normal = direction(input.normal);
    curve.x_axis = direction(input.x_axis);
    curve.major = input.major;
    curve.minor = input.minor;
    curve.first = input.first;
    curve.last = input.last;
    curve.closed = input.closed;
    break;
  case 2:
    curve.kind = geometry::ModelCurveKind::BSpline;
    curve.degree = static_cast<int>(input.degree);
    for (std::size_t i = 0; i + 2 < input.poles.size(); i += 3) {
      curve.poles.emplace_back(input.poles[i], input.poles[i + 1], input.poles[i + 2]);
    }
    curve.weights.assign(input.weights.begin(), input.weights.end());
    curve.knots.assign(input.knots.begin(), input.knots.end());
    break;
  case 3:
    // A helix (mitcad#385): the radius as major, the pitch as minor.
    curve.kind = geometry::ModelCurveKind::Helix;
    curve.center = point(input.center);
    curve.normal = direction(input.normal);
    curve.x_axis = direction(input.x_axis);
    curve.major = input.major;
    curve.minor = input.minor;
    curve.first = input.first;
    curve.last = input.last;
    break;
  default:
    throw std::invalid_argument("unknown path curve kind");
  }
  return piece;
}

geometry::Path path(const rust::Vec<SweepCurve>& curves) {
  geometry::Path result;
  for (const SweepCurve& curve : curves) {
    result.push_back(path_curve(curve));
  }
  return result;
}

geometry::LoftEnd loft_end(std::uint8_t kind, double angle, double weight) {
  using Kind = geometry::LoftEnd::Kind;
  static constexpr Kind kinds[] = {Kind::Free,       Kind::Direction, Kind::Tangent,
                                   Kind::Smooth,     Kind::PointSharp, Kind::PointTangent};
  if (static_cast<std::size_t>(kind) >= std::size(kinds)) {
    throw std::invalid_argument("unknown loft end condition");
  }
  geometry::LoftEnd end;
  end.kind = kinds[kind];
  end.angle = angle;
  end.weight = weight;
  return end;
}

geometry::ShapePtr body_at(const ShapeList& bodies, std::size_t index) {
  if (index >= bodies.size() || !bodies.at(index)) {
    throw std::invalid_argument("a loft section's body is missing");
  }
  return bodies.at(index);
}

} // namespace

// A path curve of the geometry library (thin extrusions along curves, mitcad#463).
geometry::PathCurve path_curve_of(const SweepCurve& input) { return path_curve(input); }

std::shared_ptr<geometry::Shape> sweep_solid(rust::Str feature, const Frame& frame,
                                             rust::Slice<const Region> regions, const SweepInput& input,
                                             const ShapeList& bodies) {
  geometry::SweepSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.regions = to_geometry(regions);
  spec.path = path(input.path);
  spec.extent1 = input.extent1;
  spec.extent2 = input.extent2;
  spec.orientation =
      input.parallel ? geometry::SweepOrientation::Parallel : geometry::SweepOrientation::Perpendicular;
  spec.twist = input.twist;
  spec.taper = input.taper;
  spec.rail = path(input.rail);
  // A second rail and an end scale (mitcad#464).
  spec.rail2 = path(input.rail2);
  spec.scale = input.end_scale;
  // The rails' points (mitcad#469).
  spec.rail_point = input.rail_plane ? geometry::RailPoint::Plane : geometry::RailPoint::Fraction;
  spec.scaling = input.scaling == 1   ? geometry::ProfileScaling::Stretch
                 : input.scaling == 2 ? geometry::ProfileScaling::None
                                      : geometry::ProfileScaling::Scale;
  // A fixed binormal (mitcad#397).
  if (input.fixed_binormal) {
    const gp_Vec binormal(input.binormal[0], input.binormal[1], input.binormal[2]);
    if (binormal.Magnitude() < 1.0e-12) {
      throw std::invalid_argument("the binormal has no length");
    }
    spec.binormal = gp_Dir(binormal);
  }
  // Guide surfaces (mitcad#157).
  if (input.guide_faces.size() != input.guide_bodies.size()) {
    throw std::invalid_argument("a guide surface's body is missing");
  }
  for (std::size_t i = 0; i < input.guide_faces.size(); ++i) {
    const std::size_t body = input.guide_bodies[i];
    if (body >= bodies.size() || !bodies.at(body)) {
      throw std::invalid_argument("a guide surface's body is missing");
    }
    spec.guide.push_back({bodies.at(body), std::string(input.guide_faces[i])});
  }
  // A Frenet or auxiliary frame (mitcad#539).
  spec.frenet = input.frenet;
  spec.auxiliary = path(input.auxiliary);
  spec.curvilinear = input.curvilinear;
  if (input.constant_binormal) {
    const gp_Vec binormal(input.constant_direction[0], input.constant_direction[1], input.constant_direction[2]);
    if (binormal.Magnitude() < 1.0e-12) {
      throw std::invalid_argument("the constant binormal has no length");
    }
    spec.constant_binormal = gp_Dir(binormal);
  }
  // A body face as the profile (mitcad#543).
  if (input.has_profile_face) {
    if (input.profile_body >= bodies.size() || !bodies.at(input.profile_body)) {
      throw std::invalid_argument("the profile face's body is missing");
    }
    spec.profile_body = bodies.at(input.profile_body);
    spec.profile_face = std::string(input.profile_face);
    spec.profile_key = std::string(input.profile_key);
  }
  return geometry::sweep(spec);
}

std::shared_ptr<geometry::Shape> loft_solid(rust::Str feature, const LoftInput& input,
                                            rust::Slice<const Frame> frames,
                                            rust::Slice<const Region> regions, const ShapeList& bodies) {
  geometry::LoftSpec spec;
  spec.feature = std::string(feature);
  const std::vector<geometry::Region> converted = to_geometry(regions);
  for (const LoftSectionInput& section : input.sections) {
    geometry::LoftSection result;
    switch (section.kind) {
    case 0:
      if (section.index >= converted.size() || section.index >= frames.size()) {
        throw std::invalid_argument("a loft section's region is missing");
      }
      result.kind = geometry::LoftSection::Kind::Region;
      result.frame = to_geometry(frames[section.index]);
      result.region = converted[section.index];
      break;
    case 1:
      result.kind = geometry::LoftSection::Kind::Face;
      result.body = body_at(bodies, section.index);
      result.face = std::string(section.face);
      break;
    case 2:
      result.kind = geometry::LoftSection::Kind::Point;
      result.point = point(section.point);
      break;
    default:
      throw std::invalid_argument("unknown loft section kind");
    }
    spec.sections.push_back(std::move(result));
  }
  spec.centerline = path(input.centerline);
  spec.ruled = input.ruled;
  spec.closed = input.closed;
  spec.solid = input.solid;
  // A frame along the centre line (mitcad#543).
  spec.frenet = input.frenet;
  spec.auxiliary = path(input.auxiliary);
  spec.curvilinear = input.curvilinear;
  if (input.constant_binormal) {
    const gp_Vec binormal(input.constant_direction[0], input.constant_direction[1], input.constant_direction[2]);
    if (binormal.Magnitude() < 1.0e-12) {
      throw std::invalid_argument("the constant binormal has no length");
    }
    spec.constant_binormal = gp_Dir(binormal);
  }
  // Parallel along the centre line (mitcad#547).
  spec.parallel = input.parallel;
  std::size_t at = 0;
  for (const std::size_t count : input.rails) {
    if (at + count > input.rail_curves.size()) {
      throw std::invalid_argument("a loft rail's curves are missing");
    }
    geometry::Path rail;
    for (std::size_t i = at; i < at + count; ++i) {
      rail.push_back(path_curve(input.rail_curves[i]));
    }
    spec.rails.push_back(std::move(rail));
    at += count;
  }
  spec.start = loft_end(input.start_kind, input.start_angle, input.start_weight);
  spec.end = loft_end(input.end_kind, input.end_angle, input.end_weight);
  return geometry::loft(spec);
}

std::shared_ptr<geometry::Shape> pipe_solid(rust::Str feature, const PipeInput& input) {
  geometry::PipeSpec spec;
  spec.feature = std::string(feature);
  spec.path = path(input.path);
  spec.extent1 = input.extent1;
  spec.extent2 = input.extent2;
  spec.section = input.section == 1   ? geometry::PipeSection::Square
                 : input.section == 2 ? geometry::PipeSection::Triangular
                                      : geometry::PipeSection::Circular;
  spec.size = input.size;
  spec.thickness = input.thickness;
  spec.corners = input.corners == 1 ? geometry::PipeCorners::Rounded : geometry::PipeCorners::Mitered;
  return geometry::pipe(spec);
}

std::shared_ptr<geometry::Shape> coil_solid(rust::Str feature, const Frame& frame, const CoilInput& input) {
  geometry::CoilSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.diameter = input.diameter;
  spec.revolutions = input.revolutions;
  spec.pitch = input.pitch;
  spec.angle = input.angle;
  spec.spiral = input.spiral;
  spec.clockwise = input.clockwise;
  switch (input.section) {
  case 1:
    spec.section = geometry::CoilSection::Square;
    break;
  case 2:
    spec.section = geometry::CoilSection::TriangularExternal;
    break;
  case 3:
    spec.section = geometry::CoilSection::TriangularInternal;
    break;
  default:
    spec.section = geometry::CoilSection::Circular;
  }
  spec.position = input.position == 0   ? geometry::CoilPosition::Inside
                  : input.position == 2 ? geometry::CoilPosition::Outside
                                        : geometry::CoilPosition::OnCenter;
  spec.size = input.size;
  return geometry::coil(spec);
}

std::shared_ptr<geometry::Shape> rib_solid(rust::Str feature, const Frame& frame, const RibInput& input,
                                           const ShapeList& bodies) {
  geometry::RibSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.web = input.web;
  std::size_t next = 0;
  for (const std::size_t count : input.chains) {
    geometry::Path chain;
    for (std::size_t i = 0; i < count && next < input.curves.size(); ++i, ++next) {
      chain.push_back(path_curve(input.curves[next]));
    }
    spec.chains.push_back(std::move(chain));
  }
  spec.thickness = input.thickness;
  spec.location = input.location == 1   ? geometry::ThicknessLocation::Side1
                  : input.location == 2 ? geometry::ThicknessLocation::Side2
                                        : geometry::ThicknessLocation::Symmetric;
  if (input.has_depth) {
    spec.depth = input.depth;
  }
  spec.flip = input.flip;
  spec.extend_ends = input.extend_ends;
  spec.end_offset = input.end_offset;
  if (input.has_direction) {
    spec.direction = direction(input.direction);
  }
  for (std::size_t i = 0; i < bodies.size(); ++i) {
    spec.bodies.push_back(body_at(bodies, i));
  }
  return geometry::rib(spec);
}

std::shared_ptr<geometry::Shape> helix_solid(rust::Str feature, const Frame& frame,
                                             rust::Slice<const Region> regions, const HelixInput& input,
                                             const ShapeList& bodies) {
  geometry::HelixSweepSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.regions = to_geometry(regions);
  spec.axis = gp_Ax1(point(input.origin), direction(input.direction));
  spec.pitch = input.pitch;
  spec.revolutions = input.revolutions;
  spec.left_handed = input.left_handed;
  spec.growth = input.growth;
  spec.flip = input.flip;
  spec.freecad = input.freecad;
  spec.start_flat = input.ends[0];
  spec.start_transition = input.ends[1];
  spec.end_transition = input.ends[2];
  spec.end_flat = input.ends[3];
  spec.piece_shift = input.piece_shift;
  for (std::size_t i = 0; i < bodies.size(); ++i) {
    spec.bodies.push_back(body_at(bodies, i));
  }
  return geometry::helix_sweep(spec);
}

} // namespace mitcad::bridge
