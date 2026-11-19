// SPDX-License-Identifier: MIT
#include "bridge/face_extrude.hpp"

#include <cmath>
#include <stdexcept>
#include <string>

#include <gp_Dir.hxx>

#include "bridge/profile_feature.hpp"
#include "mitcad/geometry/face_extrude.hpp"
#include "mitcad_bridge/kernel/face_extrude.h"

namespace mitcad::bridge {

std::shared_ptr<geometry::Shape> extrude_face(rust::Str feature, const geometry::Shape& body, rust::Str face,
                                              rust::Str key, const FaceExtrusion& extrusion,
                                              const TargetInput& target1, const TargetInput& target2,
                                              const ShapeList& bodies) {
  geometry::FaceExtrudeSpec spec;
  spec.feature = std::string(feature);
  spec.face = std::string(face);
  spec.key = std::string(key);
  spec.reversed = extrusion.reversed;
  spec.start_offset = extrusion.start;
  spec.distance1 = extrusion.side1;
  // A side up to a target (mitcad#2033) has no distance.
  spec.target1 = target_of(target1, bodies);
  spec.target2 = target_of(target2, bodies);
  if (extrusion.two_sides && !spec.target2) {
    spec.distance2 = extrusion.side2;
  }
  if (spec.target2 && !extrusion.two_sides) {
    throw std::invalid_argument("a target of side two without two sides");
  }
  // A direction of its own and the sides' tapers (mitcad#541).
  if (extrusion.has_direction) {
    const auto& d = extrusion.direction;
    if (!(std::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) > 1.0e-12)) {
      throw std::invalid_argument("the extrusion direction has no length");
    }
    spec.direction = gp_Dir(d[0], d[1], d[2]);
  }
  spec.taper1 = extrusion.taper1;
  spec.taper2 = extrusion.taper2;
  return geometry::extrude_face(body, spec);
}

} // namespace mitcad::bridge
