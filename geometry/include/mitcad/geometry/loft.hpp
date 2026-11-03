// SPDX-License-Identifier: MIT
#pragma once

// Lofts (F3): solids through sections in order, smooth or ruled, open or
// closed, optionally along a centre line or through rails, with end
// conditions at the first and the last section. Lengths are millimetres,
// angles radians.

#include <optional>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// A sketch region (its outer loop; one with holes is refused), a face of a
// body (its outer loop, also of a face with holes, mitcad#1183) or a point
// (only first or last).
struct LoftSection {
  enum class Kind { Region, Face, Point };
  Kind kind = Kind::Region;
  Frame frame;   // Region
  Region region; //
  ShapePtr body; // Face: a face name reference on the body
  std::string face;
  gp_Pnt point; // Point
};

// How the loft leaves its first section or reaches its last one. The
// "takeoff" is the derivative of the surface across the section, pointing
// from the section into the loft; over the span to the next section it is
// `weight` times a length (the stored bodies of the reference models
// follow the same measure):
// - Direction (a sketch region): at `angle` (|angle| < pi/2) from the
//   region's normal, tilted out of the region for a positive angle; the
//   distance between the sections' centres.
// - Tangent, Smooth (a face of a body): continuing each face next to the
//   section across its edge, tangent (G1) or also with its curvature (G2);
//   the mean distance between the sections' corresponding vertices.
// - PointTangent (a point): a rounded tip whose tangent plane is at right
//   angles to the line from the point to the next section's centre; each
//   point of that section's distance from the line.
// - Free and PointSharp impose nothing; with two sections the far end of
//   a condition follows Mitcad's rule for it (loft_skin.cpp, commands.md).
struct LoftEnd {
  enum class Kind { Free, Direction, Tangent, Smooth, PointSharp, PointTangent };
  Kind kind = Kind::Free;
  double angle = 0.0;
  double weight = 1.0;
};

// Faces: side(<key>) from the edges of the first section that is not a
// point (#k pieces), keyed by segment for a region and by edge name for a
// face; caps start(<key>) and end(<key>) keyed by the region or the face
// name. A closed loft has no caps. Rails split the sections' edges where
// they meet them (their pieces #k); every rail must meet every section and
// run through them in order. End conditions and rails need an open, smooth
// loft without a centre line; rails need sections that are not points.
struct LoftSpec {
  std::string feature;
  std::vector<LoftSection> sections;
  bool ruled = false;
  bool closed = false;
  bool solid = true; // false: a surface through the sections, the side faces only (a shell)
  Path centerline;   // empty: none
  std::vector<Path> rails;
  LoftEnd start;
  LoftEnd end;
  // Along a centre line (mitcad#543): how the sections turn along it, as a
  // sweep's frame (path_sweep.hpp, SweepSpec): the Frenet frame, towards an
  // auxiliary curve (the fraction rule with `curvilinear`) or about a
  // constant binormal; none: the sections keep their angle to the centre
  // line. Framed lofts are approximated within 1e-7 mm.
  bool frenet = false;
  Path auxiliary;
  bool curvilinear = false;
  std::optional<gp_Dir> constant_binormal;
  // Along a centre line (mitcad#547): the sections do not turn, they only
  // move along it (a sweep's parallel orientation); not with a frame.
  bool parallel = false;
};

ShapePtr loft(const LoftSpec& spec);

} // namespace mitcad::geometry
