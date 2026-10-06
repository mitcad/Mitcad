// SPDX-License-Identifier: MIT
#pragma once

// Geometry of named faces and edges for the features that refer to them:
// the plane a hole starts on, the axis of a revolution, the cylinder of a
// thread. All throw std::invalid_argument when the name resolves to nothing
// or to geometry of the wrong kind.

#include <string>

#include <gp_Ax1.hxx>
#include <gp_Pln.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The plane of a planar face (all pieces of a split one), through the
// middle of the face, its normal pointing out of the material.
gp_Pln face_plane(const Shape& shape, const std::string& face);

// A cylindrical face (all pieces of a split one).
struct CylinderFace {
  // On the cylinder's axis, at the face's low end along the axis of the
  // cylinder's surface; the direction is that axis.
  gp_Ax1 axis;
  double radius = 0.0;
  // The face's extent along the axis, from `axis`'s origin.
  double length = 0.0;
  // The material lies outside the cylinder (a hole's wall).
  bool internal = false;
};

CylinderFace face_cylinder(const Shape& shape, const std::string& face);

} // namespace mitcad::geometry
