// SPDX-License-Identifier: MIT
#pragma once

// Sweeps along paths (F3): sketch profiles swept along a path (sweep),
// pipes and coils, as solids with named faces. Lofts are in
// loft.hpp, ribs and webs in rib.hpp. Lengths are millimetres, angles
// radians; the Rust side documents the semantics (core/model/src/sweeps.rs).

#include <string>
#include <vector>

#include <gp_Ax1.hxx>

#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry {

// A curve of a path and its name (a sketch curve "c3" or an edge name).
struct PathCurve {
  std::string name;
  ModelCurve curve;
};

// Curves end to end in order, each running along the path.
using Path = std::vector<PathCurve>;

enum class SweepOrientation { Perpendicular, Parallel };
enum class ProfileScaling { Scale, Stretch, None };

// The regions swept along the path from where its plane meets the path:
// `extent1` of the path's length after that point and `extent2` of the
// length before it (fractions of those parts, of the whole length on a
// closed path). Twist (radians over the swept length, right hand about the
// path) and taper (the profile's farthest point from the path moves out
// by s tan(taper) at distance s) are ignored with a guide rail, which turns
// the profile towards it and sizes it by its distance from the path, the
// rail point at the same fraction of its length.
//
// Faces: side(<segment>) from the profile segments (#k pieces), and with
// caps start(<region>) and end(<region>): start at the profile when it is
// at an end of the swept part, else at the end of side one.
struct SweepSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  Path path;
  double extent1 = 1.0;
  double extent2 = 1.0;
  SweepOrientation orientation = SweepOrientation::Perpendicular;
  double twist = 0.0;
  double taper = 0.0;
  Path rail; // empty: no guide rail
  ProfileScaling scaling = ProfileScaling::Scale;
};

ShapePtr sweep(const SweepSpec& spec);

enum class PipeSection { Circular, Square, Triangular };

// The section at right angles to the path at its start (a circle of
// diameter `size`, or a square or an equilateral triangle in a circle of
// that diameter), swept `extent1` of the path's length, and on a closed
// path `extent2` of it backwards. A positive `thickness` makes it hollow,
// the wall inside the section. Faces side<i>, inner<i>, start and end.
struct PipeSpec {
  std::string feature;
  Path path;
  double extent1 = 1.0;
  double extent2 = 0.0;
  PipeSection section = PipeSection::Circular;
  double size = 0.0;
  double thickness = 0.0;
};

ShapePtr pipe(const PipeSpec& spec);

enum class CoilSection { Circular, Square, TriangularExternal, TriangularInternal };
enum class CoilPosition { Inside, OnCenter, Outside };

// A helix about the frame's normal through its origin from the frame's x
// axis, counter-clockwise about the normal unless `clockwise`,
// `revolutions` turns rising `pitch` per turn and widening by `angle` (a
// cone), or with `spiral` a flat spiral growing `pitch` in radius per turn;
// the section at right angles to it, positioned on the diameter. Faces
// side<i>, start and end.
struct CoilSpec {
  std::string feature;
  Frame frame;
  double diameter = 0.0;
  double revolutions = 0.0;
  double pitch = 0.0;
  double angle = 0.0;
  bool spiral = false;
  bool clockwise = false;
  CoilSection section = CoilSection::Circular;
  CoilPosition position = CoilPosition::OnCenter;
  double size = 0.0;
};

ShapePtr coil(const CoilSpec& spec);

// FreeCAD's helices (mitcad#4): the regions turned about `axis` while
// they move along its direction, `revolutions` turns rising `pitch` each,
// right-handed about the direction unless `left_handed` (a screw motion:
// every section in a plane through the axis is the profile turned there).
// Faces side(<segment>), start(<region>) at the profile and end(<region>).
struct HelixSweepSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  gp_Ax1 axis;
  double pitch = 0.0;
  double revolutions = 0.0;
  bool left_handed = false;
};

ShapePtr helix_sweep(const HelixSweepSpec& spec);

} // namespace mitcad::geometry
