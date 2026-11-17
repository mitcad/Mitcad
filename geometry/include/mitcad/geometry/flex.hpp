// SPDX-License-Identifier: MIT
#pragma once

// Flex (mitcad#1713): flexible items (wires, cables, hoses, flat cables,
// strips, belts) laid through the model. Three things are the geometry's:
// the route of an item through its nodes (flex_route), the solid of a
// section that is not round along a route (flex_solid; a round one is a
// pipe, path_sweep.hpp) and the end ports of pipes and Flex items
// (end_port). Lengths are millimetres, angles radians; the Rust side
// documents the semantics (core/model-base/src/flex.rs) and
// core/model-api/src/commands.md, "Flex", what the user gets. Nothing here
// changes the shapes it is given.
//
// Section coordinates: a section is drawn with its origin on the route, x
// its width (along the surface the item rests on) and y its height, away
// from that surface; the route's direction is x × y. Along a route the
// section's y direction is each sample's `normal` and its x direction
// `normal × tangent`.

#include <array>
#include <optional>
#include <string>
#include <vector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry {

// A node of a route: a point the route passes through.
struct FlexNode {
  gp_Pnt point;
  // The face it lies on: the index of the face's body in the obstacles
  // (-1: the node is in space), the face's name in that body (every piece
  // of the name) and the face's unit outward normal at the node.
  int obstacle = -1;
  std::string face;
  gp_Dir normal;
  // A port's direction (the cap's outward normal): the route leaves the
  // first node along it and reaches the last node against it.
  std::optional<gp_Dir> direction;
  // A port's section orientation: the section's x here, square to
  // `direction`.
  std::optional<gp_Dir> x_axis;
  // Radians the section is turned about the route here, right hand about
  // the route's direction, beyond what the route gives it.
  double twist = 0.0;
};

struct FlexRouteSpec {
  std::string feature;
  // Two or more, in order from the item's start to its end.
  std::vector<FlexNode> nodes;
  // How far the route is held from the surface it lies on: the distance
  // from the route to the section's resting side.
  double resting = 0.0;
  // How far the section reaches sideways from the route.
  double half_width = 0.0;
  // How far the section reaches from the route any way: the clearance of
  // a stretch through the air.
  double reach = 0.0;
  // The smallest radius of bending about the section's x (the one radius
  // of a round section); none: the item does not bend that way.
  std::optional<double> bend_radius;
  // The smallest radius of bending about the section's y; for a round
  // section the same as `bend_radius`.
  std::optional<double> bend_radius_edge;
  // The most the section may turn about the route, radians per
  // millimetre; none: no limit.
  std::optional<double> max_twist;
  // A quicker, rougher route: the route alone under the pointer
  // (routing::FlexRouter::route's `quick`).
  bool quick = false;
};

// A point of a route.
struct FlexSample {
  // Its length along the route from the start.
  double along = 0.0;
  gp_Pnt point;
  // The route's direction here.
  gp_Dir tangent;
  // The section's y direction here, square to the tangent.
  gp_Dir normal;
  // The node here, by its index in the spec's nodes; -1: none.
  int node = -1;
  // The face the route lies on here: the obstacle's index (-1: none) and
  // the face's name.
  int obstacle = -1;
  std::string face;
};

enum class FlexProblemKind {
  // The item passes through an obstacle, or through its own run.
  PassesThrough,
  // The route bends with a smaller radius than allowed.
  BendTooTight,
  // The section turns more per length than allowed.
  TwistOverLimit,
  // No route passes through nothing within the bend radius: the route
  // given is the nearest found.
  NoCleanRoute,
  // A section that is not round bends over its edge (about its y) with a
  // smaller radius than `bend_radius_edge` allows, or at all without it.
  // (`BendTooTight` is then about its x alone.)
  EdgeBendTooTight,
};

// A stretch of a route with something wrong.
struct FlexProblem {
  FlexProblemKind kind = FlexProblemKind::PassesThrough;
  // The stretch, lengths along the route.
  double from = 0.0;
  double to = 0.0;
  // The obstacle passed through, by its index; -1: none (or the route's
  // own run).
  int obstacle = -1;
  // Where it is, to paint: points, and boxes as {minimum, maximum}.
  std::vector<gp_Pnt> points;
  std::vector<std::array<gp_Pnt, 2>> boxes;
};

struct FlexRoute {
  // The centre line: curves end to end in order, each running along the
  // route from the first node to the last, tangent to each other where
  // they meet (lines, circle arcs and B-splines; no helix).
  std::vector<ModelCurve> curve;
  // Points along it in order: the first at the first node (`along` 0),
  // the last at the last node (`along` the route's length), one at every
  // node (`node`), and between them close enough that the polyline
  // through them stays within a hundredth of `reach` of the curve.
  std::vector<FlexSample> samples;
  std::vector<FlexProblem> problems;
};

// The route of a Flex item through the nodes in order: the shortest one
// that passes through no obstacle and stays within the bend radii.
//
// - Between neighbouring nodes that both lie on faces of obstacles that
//   touch (or of one obstacle) the route runs along the surfaces, held
//   `resting` away from them, crossing from face to face and from body to
//   body where they touch. Over a sharp edge it rounds with the bend radius
//   and leaves the surface for that stretch, as a real cable does.
// - Any other stretch (a node in space, nodes on bodies that do not touch)
//   is a smooth curve that passes around the obstacles in its way, `reach`
//   clear of them.
// - The whole route keeps to the bend radii: `bend_radius` where it bends
//   about the section's x, `bend_radius_edge` where it bends about the
//   section's y; without a radius the route does not bend that way at all
//   (it goes the longer way round, or the stretch is a problem).
// - Nothing is passed through: the obstacles, and the route's own earlier
//   run (a section's reach apart; touching is allowed).
// - At a node with a `direction` the route's tangent is that direction at
//   the first node and its reverse at the last.
//
// Orientation, each sample's `normal` (the section's y): the surface's
// outward normal where the route lies on a face; through the air from the
// orientation at one end of the stretch to that at the other with the
// least twist; each node's `twist` added from that node on; at a node
// with an `x_axis` the section's x is that. A twist per length over
// `max_twist` is a problem.
//
// A route that cannot be clean is not an error: the best one found is
// returned with its problems (where it passes through something, bends
// too tightly or twists too much; NoCleanRoute when no clean route
// exists), each with the stretch and places to paint. Throws only for
// input that gives no route at all: fewer than two nodes, nodes all at
// one point, an obstacle index or a face that does not resolve.
// Deterministic: the same input gives the same route on every platform
// to 1e-6 mm.
FlexRoute flex_route(const std::vector<ShapePtr>& obstacles, const FlexRouteSpec& spec);

// The solid of a section swept along a route.
struct FlexSolidSpec {
  std::string feature;
  // The route's centre line (FlexRoute::curve), named.
  Path path;
  // The route's samples (FlexRoute::samples): at each the section's
  // origin is the sample's point, its y the sample's normal and its x
  // normal × tangent; between samples it turns evenly.
  std::vector<FlexSample> samples;
  // A circle of the diameter `width` when `round`; a rectangle `width`
  // (along x) by `height` (along y) centred on the route, its corners
  // rounded by `corner_radius` (0: sharp); or, when neither, the one
  // region of `outline` in section coordinates where it is drawn (its
  // first loop the outside, the others holes).
  bool round = false;
  bool rectangular = true;
  double width = 0.0;
  double height = 0.0;
  double corner_radius = 0.0;
  std::vector<Region> outline;
  // Makes the section hollow, the wall inside it.
  std::optional<double> wall;
};

// The solid of `spec`. Faces: caps `start` and `end`. Of a circle side0
// (inner0 of a hollow one). Of a rectangle
// `side0` (the resting side, at -y), `side1` (at +x), `side2` and `side3`,
// counter-clockwise about the route's direction, and `corner0` to
// `corner3` after each when the corners are rounded; `inner<i>` likewise
// for a hollow one. Of an outline `side<i>` in the order of the outer
// loop's segments and `inner<i>` in the order of the inner loops'
// segments. Where the section passes through itself along the route (a
// bend tighter than the section reaches) the solid is built in pieces
// fused into one, as a pipe is (kFoldsResolved). Throws when no solid can
// be built.
ShapePtr flex_solid(const FlexSolidSpec& spec);

// An end of a pipe or of a Flex item where another item begins or ends.
struct FlexPort {
  // The middle of the cap's outer boundary (so also of a hollow one).
  gp_Pnt position;
  // The cap's outward normal: an item that begins here leaves along it.
  gp_Dir direction;
  // The section's x at the cap, square to `direction`: the direction of
  // the cap's longest straight edge; for a cap without one (a round cap)
  // the model's x axis made square to `direction`, or its y axis when the
  // direction is along x: the same on every evaluation.
  gp_Dir x_axis;
  // The cap's extent along `x_axis` and across it.
  double width = 0.0;
  double height = 0.0;
};

// The end port at the cap face `cap` of `shape` (`F5:start`, `F5:end`;
// every piece of the name, as one face). Throws when the shape has no
// such face or the face is not planar.
FlexPort end_port(const Shape& shape, const std::string& cap);

} // namespace mitcad::geometry
