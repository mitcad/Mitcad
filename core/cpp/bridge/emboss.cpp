// SPDX-License-Identifier: MIT
#include "bridge/emboss.hpp"

#include <string>

#include "bridge/profile.hpp"
#include "mitcad/geometry/emboss.hpp"
#include "mitcad/geometry/reference.hpp"
#include "mitcad_bridge/kernel/emboss.h"

namespace mitcad::bridge {

std::shared_ptr<geometry::Shape> wrapped_emboss(rust::Str feature, const Frame& frame,
                                                rust::Slice<const Region> regions, const EmbossWrap& wrap) {
  geometry::EmbossWrapSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.regions = to_geometry(regions);
  spec.axis = gp_Ax1(gp_Pnt(wrap.axis_origin[0], wrap.axis_origin[1], wrap.axis_origin[2]),
                     gp_Dir(wrap.axis_direction[0], wrap.axis_direction[1], wrap.axis_direction[2]));
  spec.radius = wrap.radius;
  spec.to_radius = wrap.to_radius;
  spec.half_angle = wrap.half_angle;
  return geometry::wrapped_emboss(spec);
}

ConeOutput face_cone(const geometry::Shape& shape, rust::Str face) {
  const geometry::ConeFace cone = geometry::face_cone(shape, std::string(face));
  ConeOutput out{};
  out.apex = {cone.apex.X(), cone.apex.Y(), cone.apex.Z()};
  out.axis = {cone.axis.X(), cone.axis.Y(), cone.axis.Z()};
  out.half_angle = cone.half_angle;
  out.internal = cone.internal;
  return out;
}

std::shared_ptr<geometry::Shape> projected_emboss(rust::Str feature, const Frame& frame,
                                                  rust::Slice<const Region> regions, const geometry::Shape& body,
                                                  rust::Str face, const EmbossProjection& projection) {
  geometry::EmbossProjectSpec spec;
  spec.feature = std::string(feature);
  spec.frame = to_geometry(frame);
  spec.regions = to_geometry(regions);
  spec.face = std::string(face);
  spec.direction = gp_Vec(projection.direction[0], projection.direction[1], projection.direction[2]);
  spec.depth = projection.depth;
  return geometry::projected_emboss(body, spec);
}

} // namespace mitcad::bridge
