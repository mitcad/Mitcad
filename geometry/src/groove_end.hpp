// SPDX-License-Identifier: MIT
#pragma once

// The end of a groove rounded as a ball mill ends it (mitcad#2154). A
// groove is a tube cut into the body (a circle swept along a line or an
// arc: its floor a cylinder or a torus, the body's material outside it),
// ending in a flat end cap across it; the edge between the end cap and the
// floor is an arc of the tube's circle there. Where the groove is shallow
// (its circle's centre above the body), a rounding of that edge wider than
// the end cap is high takes the end cap away (mitcad#1819's rule): built
// on the extended body, the end cap continued along its plane to the whole
// disc and the floor along its surface to the whole tube, so that the edge
// is the whole circle and the rounding closes on itself; what it adds is
// cut by the faces beyond the end cap (the body's top), which grow out to
// meet it. Below the circle's radius Rc the rounding is the geometry
// kernel's on that extended body (a torus piece for a straight groove); at
// Rc the ball's centre does not move along the edge, which the kernel's
// rolling ball cannot build: the rounding is the sphere of radius Rc on
// the tube's axis (or its spine, along an arc), Rc from the end cap's
// plane; above Rc no ball of that radius fits in the groove. Internal to
// the geometry library.

#include <optional>
#include <string>

#include <gp_Ax1.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// How far a radius may be from the groove's circle's and count as equal
// (mm, the ticket's 1e-6).
inline constexpr double kGrooveRadiusTolerance = 1.0e-6;

// The end of a groove at an edge of the body.
struct GrooveEnd {
  // The edge's circle: centred on the tube's axis (spine), the tube's
  // radius.
  gp_Circ circle;
  // The end cap's outward normal: from it into the groove.
  gp_Dir into;
  int cap = -1;   // the end cap (a plane), by index in the body
  int floor = -1; // the floor (a cylinder or a torus)
  // Along an arc: the floor a torus about `axis`, the circle's centre
  // `spine` from it.
  bool bent = false;
  gp_Ax1 axis;
  double spine = 0.0;
};

// Whether the edge (by index) of `body` ends a groove: a circle (or an arc
// of it) between a planar face across the tube (its normal along the
// circle's axis) and a cylinder (its axis through the circle's centre, of
// the circle's radius) or a torus (its axis in the plane, the circle's
// centre on its spine, its minor radius the circle's), the floor's material
// outside the tube and the floor on the end cap's outer side; the end cap
// all inside the circle (not a pocket's floor with a concave corner arc,
// mitcad#1368), cut across by planar faces beyond it (not a whole disc: a
// blind hole's bottom), the circle's centre on the outer side of one of
// their planes (a shallow groove). None where it is not.
std::optional<GrooveEnd> groove_end(const Shape& body, int edge);

// The rounding of radius `radius` (up to the groove's radius, within
// kGrooveRadiusTolerance) of the groove end `end` at edge `edge` of `body`,
// built as the header says: the body with what the rounding adds on the
// extended body, on the material side of the planar faces beyond the end
// cap (the faces sharing an edge with it but the floor). Its new faces are
// named <feature>:fillet(<edge>), the rest keep their names (the faces
// beyond grow over what is added). Null where a face beyond is not a plane,
// the geometry kernel cannot round the extended body, what is added runs
// into the body's material (the groove ends within the rounding's reach) or
// the result is not one valid closed solid; throws only for cancelling.
ShapePtr rounded_groove_end(const std::string& feature, const Shape& body, int edge, const GrooveEnd& end,
                            double radius);

// The error of a rounding of radius `radius` wider than the groove `end`
// at edge `edge` ends (no ball of that radius fits in it): in words for the
// person, the edge, the radii and the floor for agents, the edge as its
// place, after "fillet failed; ".
std::string groove_too_narrow(const Shape& body, int edge, const GrooveEnd& end, double radius);

} // namespace mitcad::geometry::detail
