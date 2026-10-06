// SPDX-License-Identifier: MIT
#pragma once

// What a face operation works against: a plane, a face of a body, a whole
// body or sketch curves (drafts, replace face, split body and split face).

#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

struct Tool {
  enum class Kind { Plane, Face, Body, Curves };
  Kind kind = Kind::Plane;
  // Plane: the plane; its normal orders the pieces of a split body.
  gp_Pln plane;
  // Face: the body and the face's name on it (all pieces of a split face);
  // Body: the body.
  ShapePtr shape;
  std::string face;
  // Curves: the curves (c<n>) of the sketch regions, swept along
  // `direction` both ways through the body.
  Frame frame;
  std::vector<Region> regions;
  std::vector<std::string> curves;
  gp_Dir direction{0.0, 0.0, 1.0};

  static Tool of_plane(const gp_Pln& plane);
  static Tool of_face(ShapePtr shape, std::string face);
  static Tool of_body(ShapePtr shape);
};

} // namespace mitcad::geometry
