// SPDX-License-Identifier: MIT
#pragma once

// Curved sheet faces developed (mitcad#2435): the faces of a body on one
// cylinder or cone in the chart of the sheet panel they become, and how
// much a face bends two ways (which no sheet development lays flat).

#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sheet_domain.hpp"

namespace mitcad::geometry {

struct SheetFaceDevelopment {
  bool cone = false;
  // An oblique (eccentric) cone (mitcad#2491): the lines from `origin`
  // (the faces' own apex) through the circle about `centre` of
  // `circle_radius` in the plane of `radial_start` and
  // `angular_direction` (`axis` its normal); the angle runs round that
  // circle. Its chart is the development of the neutral surface (no cone:
  // the faces' cone moved along its normals) by its generators
  // (detail::ObliqueDevelopment, `sheet_oblique_cone.hpp`), the distance
  // along each from the apex kept.
  bool oblique = false;
  gp_Pnt centre;
  double circle_radius = 0;
  // An oblique cone's body is a plate behind the faces: every other face
  // lies on the surface `thickness` behind them (within 1e-4 mm, at a
  // grid of its points) or is a wall meeting both the faces and such a
  // face. (Cylinders and cones are checked by the model.)
  bool plate = false;
  // A cylinder: a point of its axis; a cone: the apex of the neutral cone
  // (the faces' cone moved along its normals to the neutral surface).
  gp_Pnt origin;
  // A cone's from its apex where the radius grows.
  gp_Dir axis{0, 0, 1};
  // Across the axis where the faces start (their least angle about it) and
  // the right-handed way round the axis from there.
  gp_Dir radial_start{1, 0, 0};
  gp_Dir angular_direction{0, 1, 0};
  double neutral_radius = 0;  // a cylinder's
  double half_angle = 0;      // a cone's
  // How far the faces run round the axis (radians, up to a full turn), and
  // whether they close on themselves there (a seam).
  double turn = 0;
  bool closed = false;
  // The faces' surface faces away from the material: the front is the
  // inner surface of the sheet.
  bool inner_is_front = false;
  // Each face's edges in the chart: a cylinder's (angle * neutral radius,
  // along the axis from the origin), a cone's development about its apex at
  // the neutral cone. A generator is a line, a circle about the axis a line
  // (a cylinder's) or an arc about the apex (a cone's), both from their
  // exact ends; other edges cubic B-splines through their exact
  // development within 1e-8 mm. An oblique cone's generators are lines,
  // its other edges B-splines. An edge that is no generator and no circle
  // about the axis (an oblique cut, an eccentric cone's end) whose face
  // across it is a plane meeting the generators is developed where that
  // plane meets the neutral surface (mitcad#2491: a mitre's cut is kept at
  // the neutral surface), each point moved along its generator; a
  // generator ending at such an edge ends there too. Edges shared by two
  // faces come once with each face.
  std::vector<std::vector<BoundedCurve2>> faces;
};

// The faces named in `faces` (every piece of each name), all on one
// cylinder or cone (stored so, or as another surface within 1e-6 mm of it,
// such as a spline or an extrusion of a circle; every piece within 1e-5 mm
// of the first one's surface), or on one oblique cone (a ruled surface
// whose generators meet at one apex over a circle, mitcad#2491), and
// joined at shared edges, developed at the neutral surface: `neutral_depth` (K t) from the inner
// surface of a sheet `thickness` thick whose front is the faces and whose
// material lies behind them. Faces of other surfaces or of several
// surfaces, faces not joined, faces running to a cone's apex or more than
// once round, and names of no face are errors.
SheetFaceDevelopment sheet_face_development(const Shape& shape, const std::vector<std::string>& faces,
                                            double neutral_depth, double thickness);

struct FaceBending {
  // The largest magnitude of the Gaussian curvature (1/mm^2) at a grid of
  // points inside the face (its every piece), and the diagonal of its box.
  double gaussian = 0;
  double size = 0;
};

FaceBending face_bending(const Shape& shape, const std::string& face);

} // namespace mitcad::geometry
