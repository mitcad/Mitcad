// SPDX-License-Identifier: MIT
#pragma once

// Sweeps along paths (F3): sketch profiles swept along a path (sweep),
// pipes and coils, as solids with named faces. Lofts are in
// loft.hpp, ribs and webs in rib.hpp. Lengths are millimetres, angles
// radians; the Rust side documents the semantics (core/model-base/src/sweeps.rs).

#include <optional>
#include <string>
#include <vector>

#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>

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
// Where a guide rail's point is for a place along the path (mitcad#469):
// at the same fraction of the rail's length as the place is of the path's
// (`Fraction`), or where the rail crosses the plane square to the path
// there (`Plane`).
enum class RailPoint { Fraction, Plane };

// The regions swept along the path from where its plane meets the path:
// `extent1` of the path's length after that point and `extent2` of the
// length before it (fractions of those parts, of the whole length on a
// closed path). Twist (radians over the swept length, right hand about the
// path) and taper (the profile's farthest point from the path moves out
// by s tan(taper) at distance s) are ignored with a guide rail, which turns
// the profile towards it and sizes it by its distance from the path, the
// rail point chosen by `rail_point`.
//
// The plane rule (mitcad#469): at each place the rail's point is where it
// crosses the plane square to the path. At the profile it is the crossing
// nearest to the path; from there, place by place, the next crossing
// onwards along the rail in the way the rail runs with the path (back
// along it before the profile), so a rail that crosses a plane more than
// once is followed in order. An end of the rail within 1e-3 mm (or a
// millionth of the path's length) of a plane counts as crossing it; a rail
// that does not reach a plane of the swept part is an error.
//
// Faces: side(<segment>) from the profile segments (#k pieces), and with
// caps start(<region>) and end(<region>): start at the profile when it is
// at an end of the swept part, else at the end of side one. With the
// parallel orientation a path turning back through the profile's plane is
// swept in pieces that are fused; the copy where it turns is the face
// turn(<region>).
//
// Guide surfaces (mitcad#157), instead of a guide rail: faces of bodies
// the path lies on. The profile keeps its place in the frame of the path's
// tangent and the surface's normal where the path runs (the normal of the
// face each path curve lies on), so it turns with the surface; twist and
// taper are ignored. Each path curve must lie on one of the faces (within
// 1e-4 mm), and the orientation must be perpendicular.
struct GuideFace {
  ShapePtr body;
  std::string face; // a face name; every piece of a split face
};

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
  // A second guide rail (mitcad#464; empty: none): the profile moves with
  // the first rail's point, turns with the line across to the second and
  // sizes (`scaling`) by its length, the rails' points chosen by
  // `rail_point` (with the fraction rule taken into the plane square to the
  // path).
  Path rail2;
  // The rails' points (mitcad#469); the fraction rule by default, as
  // designs saved before the plane rule have it.
  RailPoint rail_point = RailPoint::Fraction;
  // The profile's size at the ends of the swept part (mitcad#464), from 1
  // at the profile in proportion to the length run, about the path; 0.01
  // to 100, not with guide rails or guide surfaces.
  double scale = 1.0;
  ProfileScaling scaling = ProfileScaling::Scale;
  std::vector<GuideFace> guide; // empty: no guide surface
  // A fixed binormal direction (mitcad#397): the profile keeps its angles
  // to it and to the path's tangent (its frame the tangent, binormal x
  // tangent, and the binormal made square to the tangent), so a profile
  // level at the start stays level along a sloped path with the binormal
  // up. Perpendicular orientation only, without twist, taper, a guide rail
  // or guide surfaces, along a path without corners that never runs along
  // the binormal. None: the profile keeps its angle to the path without
  // twisting about it.
  std::optional<gp_Dir> binormal;
  // How a plain perpendicular sweep's profile turns instead (mitcad#539):
  // with `frenet` the path's Frenet frame (its tangent, principal normal
  // and binormal; the profile turns with the path's curvature), or along a
  // non-empty `auxiliary` curve the frame whose normal points from the path
  // to the curve: to its point at the same fraction of its length with
  // `curvilinear` (the frame's tangent then square to that normal), else
  // where it crosses the plane square to the path. Not with twist, taper,
  // an end scale, guide rails, guide surfaces, a fixed binormal or parallel
  // orientation (`unsupported: …`).
  bool frenet = false;
  Path auxiliary;
  bool curvilinear = false;
  // Or about a constant binormal (the pipe shell's constant binormal): the
  // frame keeps this binormal, its normal binormal x tangent and its tangent
  // the path's made square to the binormal, so the profile turns only about
  // it. (`binormal` above keeps the path's tangent instead.)
  std::optional<gp_Dir> constant_binormal;
  // A planar face of a body as the profile instead of `regions`
  // (mitcad#543): the face `profile_face` (one face, planar) of
  // `profile_body`, its bounded area with its holes; `frame` is then the
  // face's plane (its value is not used). The body is left as it is. Its
  // side faces are named side(<edge>) by the edges' names in the body (an
  // edge without one by its index among the face's edges), its caps
  // start(<profile_key>) and end(<profile_key>). Not with twist, taper, an
  // end scale, guide rails or guide surfaces.
  ShapePtr profile_body;
  std::string profile_face;
  std::string profile_key;
};

ShapePtr sweep(const SweepSpec& spec);

enum class PipeSection { Circular, Square, Triangular };

// The joints at a pipe's sharp corners (mitcad#1448): the sides run on to
// the corner's bisecting plane (`Mitered`), or the section turns about the
// corner (`Rounded`, OCCT's round corner).
enum class PipeCorners { Mitered, Rounded };

// The section at right angles to the path at its start (a circle of
// diameter `size`, or a square or an equilateral triangle in a circle of
// that diameter), swept `extent1` of the path's length, and on a closed
// path `extent2` of it backwards. A positive `thickness` makes it hollow,
// the wall inside the section. `corners` joins it at the path's sharp
// corners (see `detail::pipe_shell` for where that cannot be built).
// Faces side<i>, inner<i>, start and end.
struct PipeSpec {
  std::string feature;
  Path path;
  double extent1 = 1.0;
  double extent2 = 0.0;
  PipeSection section = PipeSection::Circular;
  double size = 0.0;
  double thickness = 0.0;
  PipeCorners corners = PipeCorners::Mitered;
};

ShapePtr pipe(const PipeSpec& spec);

enum class CoilSection { Circular, Square, TriangularExternal, TriangularInternal };
enum class CoilPosition { Inside, OnCenter, Outside };

// A helix about the frame's normal through its origin from the frame's x
// axis, counter-clockwise about the normal unless `clockwise`,
// `revolutions` turns rising `pitch` per turn and widening by `angle` (a
// cone), or with `spiral` a flat spiral growing `pitch` in radius per turn;
// the section in the plane through the axis (it moves by a screw motion),
// its radial extent on the diameter (centred on it, or touching it from
// inside or outside). `size` is the circle's diameter, the square's side
// or the triangle's height. Faces side<i>, start and end.
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
// A `growth` (outwards per turn, mitcad#59) widens the helix: the profiles
// move out as they turn. Mitcad's construction (mitcad#83) moves them by
// the screw motion and out along the direction from the axis towards their
// centre, in proportion to the turn, so every section in a plane through
// the axis is the profile turned there and moved out; it is the same for
// every profile position and growth sign. With `freecad` the profiles
// instead follow the Frenet frame of a helix of the same pitch and growth
// a hundred times as far from the axis, as FreeCAD builds its conical and
// growing helices (staying nearly in planes through the axis), for the
// FreeCAD import. `flip`: the travel `axis` is against the reference axis
// (FreeCAD's construction depends on it). `bodies`: the bodies the result
// is cut from, joined to or intersected with (mitcad#60, without a
// growth): where a side of a profile lies on one of their cylinders about
// the axis, the helix is built of faces of at most a third of a turn, the
// same shape, which the booleans with those bodies handle; a straight
// side as long as the pitch is also moved 0.1 um off the cylinder, into
// the void a cut's profile faces or the material a join's faces, so that
// the turns do not touch in an edge on it; beyond the bodies' faces on the
// cylinder (where the bodies reach) that sliver is cut back to it.
//
// Ends (mitcad#235; angles in radians, none by default): over a
// `*_transition` the pitch changes linearly between zero at the outer end
// and the full pitch, and over a `*_flat` the profiles turn without rising;
// the start's flat comes first, then its transition, the `revolutions` at
// the full pitch, the end's transition and flat. With ends the helix is
// built as Mitcad's widening one (not with `freecad`), and a growth follows
// the rise: the profiles move out by `growth` for every `pitch` they rise.
//
// `piece_shift` (turns, 0 by default; mitcad#235): above 0 (and below a
// third), the helix is swept in pieces of a third of a turn whose seams are
// shifted by that much (the first piece is `piece_shift` long), also where
// it would otherwise be one piece; the shape is the same. A boolean that
// fails where the pieces' seams meet seams of the bodies' faces about the
// axis can be retried with it (not with `freecad` and a growth).
struct HelixSweepSpec {
  std::string feature;
  Frame frame;
  std::vector<Region> regions;
  gp_Ax1 axis;
  double pitch = 0.0;
  double revolutions = 0.0;
  bool left_handed = false;
  double growth = 0.0;
  bool flip = false;
  bool freecad = false;
  std::vector<ShapePtr> bodies;
  double start_flat = 0.0;
  double start_transition = 0.0;
  double end_transition = 0.0;
  double end_flat = 0.0;
  double piece_shift = 0.0;
};

ShapePtr helix_sweep(const HelixSweepSpec& spec);

} // namespace mitcad::geometry
