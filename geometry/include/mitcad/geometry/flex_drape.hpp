// SPDX-License-Identifier: MIT
#pragma once

// Flex drape (mitcad#1713): a Flex item let hang. Its centre line is a chain
// of short segments that falls toward gravity, lies limp on what is below,
// wraps round curved faces and edges and never passes through a body, held
// where it is pinned; then the fewest nodes that make the router give the
// same route (flex.hpp's flex_route) where it came to rest. The model keeps
// the chain between the calls and calls flex_drape_step once per frame of
// the view (core/model-api/src/commands.md, "Hanging cables"). Nothing here
// changes the shapes it is given. Lengths are millimetres, times seconds.
//
// The chain is position-based dynamics in substeps of a fixed length
// (kDrapeSubstep): a step simulates as many of them as its time holds and
// carries the rest of a substep over to the next step, so that its work is
// in proportion to the time it simulates. Each substep moves the free
// points by their velocities and gravity, then holds them, a fixed number
// of times, to
// - their neighbours' distance (the segment's length: it does not stretch)
//   and no farther from the nearest pinned point on either side than the
//   chain between them is long (long range attachments);
// - straight, as stiffly as the item's bend radius says: its bending
//   length (where its own weight bends it as much as its stiffness holds
//   it straight) is kBendLength times the bend radius, so that a thick
//   hose bends more gently than a thin wire;
// - no tighter than the bend radius at all;
// - outside every obstacle by the section's resting distance (its points
//   and the middles of its segments), sliding on them with friction: once
//   a substep, and a point is asked of the obstacles only once it may have
//   come within that distance of them since it was asked last (the
//   clearance then found, less how far it moved);
// - apart from its own run by the section's thickness (it does not pass
//   through itself);
// - its pinned points where they are, and a grabbed point toward where the
//   pointer pulls it.
//
// It comes to rest as a real hose does, with a slight swing and no more:
// - the air holds every point back (velocity damping, as strong as the
//   spec's `damping` says), harder once the whole chain is slow, so that
//   the last small swings die out instead of going on for long;
// - the item's own material damps its short wiggles (the change of each
//   joint's bend, not the chain's swing as a whole);
// - a push off an obstacle gives no speed away from it (a hose does not
//   bounce off what it falls on);
// and it rests once its points have moved slower than a small share of its
// length per second for kDrapeRestSeconds (their mean speed over each frame,
// so that a tremble within a frame is no motion).

#include <optional>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/flex.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// What a point of the chain lies on: the obstacle (its index), the face's
// name there (empty when the face has none) and the surface's outward
// normal.
struct FlexDrapeContact {
  int obstacle = -1;
  std::string face;
  gp_Dir normal;
};

// How strongly the air holds a hanging item back, 0 to 1 (FlexDrapeSpec's
// `damping`): 0 none at all (it swings on until something stops it), 1
// about twice as strong as the default.
constexpr double kDrapeDefaultDamping = 0.5;

struct FlexDrapeSpec {
  // The chain's points in order, two or more, and their velocities (mm/s;
  // empty: at rest).
  std::vector<gp_Pnt> points;
  std::vector<gp_Vec> velocities;
  // Each point: it stays where it is (an end attached, a node at a clip).
  // Empty: none is pinned.
  std::vector<char> pinned;
  // The length of each segment between two neighbouring points.
  double segment = 0.0;
  // How far the centre line is held from a surface it lies on: the
  // distance from the route to the section's resting side.
  double resting = 0.0;
  // The smallest radius of the centre line; 0: limp, without a limit.
  double bend_radius = 0.0;
  // The unit direction gravity pulls in.
  gp_Dir gravity = gp_Dir(0.0, 0.0, -1.0);
  // The time to simulate (in substeps of kDrapeSubstep, measured for rest
  // in frames of kDrapeFrame); it stops early once the chain rests.
  double seconds = 1.0 / 30.0;
  // What the step before left of a substep (its result's `carry`), added
  // to `seconds`: 0 up to kDrapeSubstep.
  double carry = 0.0;
  // A point held by the pointer: its index, pulled toward `target` (as a
  // pinned point moved there, no faster than the section's size each
  // substep, and kept out of the obstacles).
  std::optional<int> grab;
  gp_Pnt target;
  // How long the chain has been still before this call (seconds): rest
  // carries over from call to call.
  double still = 0.0;
  // How strongly the air holds it back, 0 to 1 (kDrapeDefaultDamping).
  double damping = kDrapeDefaultDamping;
};

struct FlexDrapeResult {
  std::vector<gp_Pnt> points;
  std::vector<gp_Vec> velocities;
  // The section's y at each point (square to the chain): the surface's
  // normal where it lies on one, turned evenly between, so that a flat
  // section keeps its flat side against what it lies on.
  std::vector<gp_Dir> normals;
  // What each point lies on; obstacle -1 for a point in the air.
  std::vector<FlexDrapeContact> contacts;
  // The seconds the chain has now been still (none of its points faster
  // than kDrapeRestSpeed times its length per second over a frame), and
  // whether that is kDrapeRestSeconds: it rests (its velocities then
  // zero).
  double still = 0.0;
  bool at_rest = false;
  // The seconds simulated (whole substeps), and what is left of a substep
  // for the next step (FlexDrapeSpec's `carry`; 0 once it rests).
  double time = 0.0;
  double carry = 0.0;
  // The work done, for tests that bound it: the substeps, the passes over
  // the obstacles (one a substep) and the nearest-point questions asked of
  // them (the contacts' at the end included).
  int substeps = 0;
  int collisions = 0;
  int queries = 0;
};

// The substep (seconds), the frame the rest is measured over (a whole
// number of substeps) and the rounds of the constraints in each substep.
constexpr double kDrapeSubstep = 1.0 / 360.0;
constexpr int kDrapeFrameSubsteps = 12;
constexpr double kDrapeFrame = 1.0 / 30.0;
constexpr int kDrapeIterations = 3;
// At the end of each substep, in each of kDrapeIterations rounds: sweeps of
// the bend radius's limit at most (kDrapeRoundSweeps; the lengths' passes
// after each undo what more would do), and passes of the segments'
// lengths; then the limit once more, kDrapeLimitSweeps at most.
constexpr int kDrapeLimitSweeps = 32;
constexpr int kDrapeRoundSweeps = 8;
constexpr int kDrapeLengthPasses = 4;
// How far beyond the resting distance a point asks for the obstacles, in
// resting distances: what it finds clear there it need not ask again until
// it has moved that far.
constexpr double kDrapeClearance = 4.0;
// The bending length over the bend radius (see above).
constexpr double kDrapeBendLength = 2.0;
// Gravity, mm/s²: a hanging item moves as it would.
constexpr double kDrapeGravity = 9810.0;
// The air's damping at `damping` 1, per second: a point's velocity falls by
// e^-(damping × rate) each second. kDrapeAirDamping always; and, while
// no point of the chain is faster than kDrapeSettleSpeeds times its rest
// speed (below) and nothing holds it, up to kDrapeSettleDamping more, the
// slower it is the more (the default's 6 per second while it swings: a
// slight swing, a slight overshoot; up to 24 per second as it settles).
constexpr double kDrapeAirDamping = 12.0;
constexpr double kDrapeSettleDamping = 36.0;
constexpr double kDrapeSettleSpeeds = 30.0;
// The item's own damping of its short wiggles, per second: the change of
// each joint's bend falls by about e^-rate each second (the chain's swing
// as a whole bends its joints little and keeps its speed).
constexpr double kDrapeShapeDamping = 60.0;
// Friction on what the chain lies on.
constexpr double kDrapeFriction = 0.5;
// At rest: its points' mean speed over each frame below this share of the
// chain's length per second (at least kDrapeRestFloor mm/s) for
// kDrapeRestSeconds.
constexpr double kDrapeRestSpeed = 0.005;
constexpr double kDrapeRestFloor = 0.5;
constexpr double kDrapeRestSeconds = 0.3;

// The segment's length for an item: a fraction of its section and of its
// bend radius (a wire's segments are short, a stiff hose's longer), and no
// more segments than kDrapeMostSegments over `length`.
constexpr int kDrapeMostSegments = 400;
double flex_drape_segment(double length, double resting, double bend_radius);

// The linear deflection the obstacles are meshed with for an item of a
// resting distance (mm): a twentieth of it, from 0.1 to 0.5 mm (curved
// faces at most 0.2 radians a triangle besides). The chain rests that
// distance off the triangles, which lie up to the deflection inside a
// curved face.
double flex_drape_deflection(double resting);

// One step of the chain among `obstacles` (spec.seconds of it). Throws for
// input that is no chain (fewer than two points, a segment or a resting
// distance that is not above zero, lists of other lengths than the
// points', a damping outside 0 to 1, a carry below zero).
FlexDrapeResult flex_drape_step(const std::vector<ShapePtr>& obstacles, const FlexDrapeSpec& spec);

// The nodes of a hanging item where it rests.
struct FlexDrapeNodesSpec {
  // The chain and what each point lies on (flex_drape_step's).
  std::vector<gp_Pnt> points;
  std::vector<FlexDrapeContact> contacts;
  // The points that must be nodes (the pinned nodes, the ends), in order
  // of the chain; the first and the last point are nodes in any case. An
  // end with a port: its direction out of the item it belongs to and its
  // section's x (flex.hpp's FlexNode).
  std::vector<int> keep;
  std::optional<gp_Dir> start_direction;
  std::optional<gp_Dir> start_x_axis;
  std::optional<gp_Dir> end_direction;
  std::optional<gp_Dir> end_x_axis;
  // The item, as flex_route takes it.
  double resting = 0.0;
  double half_width = 0.0;
  double reach = 0.0;
  std::optional<double> bend_radius;
  std::optional<double> bend_radius_edge;
  std::optional<double> max_twist;
  // How far the route through the nodes may be from the chain.
  double tolerance = 0.0;
};

struct FlexDrapeNodes {
  // The points that are nodes, by their index in the chain, in order; a
  // node at a point that lies on a face is on that face.
  std::vector<int> nodes;
  // The farthest a point of the chain is from the route through them
  // (within `tolerance` unless kDrapeNodeRounds were not enough, or the
  // chain lies where the item cannot).
  double deviation = 0.0;
};

// Rounds of routing and adding a node where the route is farthest from the
// chain.
constexpr int kDrapeNodeRounds = 6;

// The fewest nodes through which flex_route gives the chain's shape: the
// points to keep, where the chain meets and leaves a surface (so that a
// stretch along the surfaces runs between nodes on faces and one through
// the air between others), and between them the points that hold it
// within the tolerance as a polyline; then routed, and where the chain is
// farther from the route than the tolerance, the farthest point of that
// stretch added, up to kDrapeNodeRounds times. No node but the points to
// keep where the item cannot lie (in a body, or nearer to one than the
// resting distance by more than a twentieth of it, at least 0.1 mm: a
// cable hung down a bore narrower than itself) or where it only touches a
// face over its edge (more than 30 degrees off the face's normal), which
// the route would not give back; the route finds its own way past them.
// No two nodes but the points to keep lie nearer along the chain than
// twice the resting distance or the bend radius, whichever is longer (a
// node right beside another made the route turn back between them). A node on a face is routed at the surface
// below the centre line, or at the body's nearest point where the chain is
// nearer to it (a pinned end on what it is attached to). Throws when no
// route can be made through the nodes.
FlexDrapeNodes flex_drape_nodes(const std::vector<ShapePtr>& obstacles, const FlexDrapeNodesSpec& spec);

} // namespace mitcad::geometry
