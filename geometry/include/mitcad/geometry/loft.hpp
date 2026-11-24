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
// (only first or last). A chain (mitcad#2345): curves joined end to end,
// open or closed, a surface loft's section; a loft of chains has only
// chains, all open or all closed, and is a surface (`solid` false), its
// faces side(<curve>) after the first chain's curves as loft_sheet's.
struct LoftSection {
  enum class Kind { Region, Face, Point, Chain };
  Kind kind = Kind::Region;
  Frame frame;   // Region
  Region region; //
  ShapePtr body; // Face: a face name reference on the body
  std::string face;
  gp_Pnt point; // Point
  Path chain;   // Chain
  // Chain: the normal of the plane its curves lie in (a planar sketch's),
  // none for curves in space (a 3D sketch's, a curved face's sketch).
  std::optional<gp_Dir> normal;
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
// - Perpendicular (mitcad#2345; a region, a face or a chain): at right
//   angles to a face the section lies on, the face `face` of `body`, or
//   without one the face the section lies on: a face section's own face,
//   a region's or a chain's plane. At every point of the section the
//   takeoff runs along the face's normal there, to the side of the next
//   section; at the section's corners as Direction's and Tangent's. On a
//   plane without a face it is Direction at angle 0 (the distance between
//   the sections' centres); on a face the mean distance between the
//   sections' corresponding vertices, as Tangent's. The section must lie on
//   the face within 0.01 mm; the surface meets the face at right angles
//   within 1e-6 rad all along the section (rails that leave the section
//   otherwise bend it, as they bend every condition).
struct LoftEnd {
  enum class Kind { Free, Direction, Tangent, Smooth, PointSharp, PointTangent, Perpendicular };
  Kind kind = Kind::Free;
  double angle = 0.0;
  double weight = 1.0;
  ShapePtr body; // Perpendicular: the body of `face`; none: the section's own face or plane
  std::string face;
};

// Faces: side(<key>) from the edges of the first section that is not a
// point (#k pieces), keyed by segment for a region and by edge name for a
// face; caps start(<key>) and end(<key>) keyed by the region or the face
// name. With merge_tangent a joined run of edges names its face after the
// run's first edge. A closed loft has no caps. Rails split the sections' edges where
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
  // Tangent Edges: Merge (mitcad#2162). Each section's runs of edges that
  // meet tangentially (within the model's angular tolerance, 1e-6 rad) are
  // joined into one edge each, by arc length within 1e-8 mm, so that the
  // side between them is one face, tangent-continuous across the former
  // joins; a section smooth all round becomes one closed edge, started at
  // the point matching the neighbouring section's start. Each joined edge
  // names its face after the first edge of its run (side(<key>); a closed
  // smooth one after its edge whose name sorts first). Edges
  // that meet at a corner stay as they are. False: every edge its own face.
  bool merge_tangent = false;
};

ShapePtr loft(const LoftSpec& spec);

} // namespace mitcad::geometry
