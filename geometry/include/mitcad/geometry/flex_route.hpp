// SPDX-License-Identifier: MIT
#pragma once

// Flex (mitcad#1713): the route of a flexible item (a wire, a hose, a flat
// cable, a strip) through its nodes past the bodies of the design, and the
// solid along it. Lengths are millimetres, angles radians.
//
// A route runs through its nodes in order. Between two nodes that lie on
// faces of one body, or of bodies that touch, it follows the surfaces by the
// shortest way, held off them by half the section's height; otherwise it
// goes through the air round the bodies in its way. The whole route is then
// bent no tighter than the item allows (over a sharp edge it leaves the
// surfaces and rounds), and checked against the bodies themselves. What
// cannot be had (a gap too narrow, a bend too tight) never fails the call:
// the best route found comes back with its problems and their places.
//
// The section's frame along the route is its tangent, its `down` (the side
// that lies against a surface, square to the tangent) and across =
// tangent x down. In section coordinates u runs across and v away from the
// surface (against `down`), the route through the origin, so
// across x up = tangent.
//
// The routing works on triangles of the bodies (FlexRouterOptions). On a
// planar face the route lies at half its height; where a node's twist
// turns a section that is not round, at the turned section's reach towards
// the face (up to half its width standing on its edge). Along a curved face it lies
// between the deflection nearer and twice the deflection farther than that,
// and a section counts as reaching into a body only beyond twice the
// deflection (or 2 % of its smaller size). The bend limits are met by moving
// the route's points until they rest, to within about 15 %: a bend is a
// problem when it is tighter than that.

#include <array>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry::routing {

enum class FlexSectionKind { Round, Rectangular, Profile };

// The item's section. Round: `width` is the outer diameter. Rectangular:
// `width` across and `height` away from the surface, its corners rounded by
// `corner_radius`. Profile: the closed outline `profile` in section
// coordinates (its first loop; further loops are holes), with `width` and
// `height` the size of the box round it, which the routing and the checks
// use. A positive `wall` makes it hollow, the wall inside the outline.
struct FlexSection {
  FlexSectionKind kind = FlexSectionKind::Round;
  double width = 0.0;
  double height = 0.0;
  double corner_radius = 0.0;
  Region profile;
  double wall = 0.0;

  bool round() const { return kind == FlexSectionKind::Round; }
  // How far the route's curve stays from a surface it lies on.
  double half_height() const { return (round() ? width : height) / 2.0; }
};

// How the item bends. `bend_radius`: the least radius of the route's curve,
// for a section that is not round its bends over the flat side (towards or
// away from `down`); `edge_bend_radius`: its bends over the edge (across).
// 0: no limit. `flat_bend` or `edge_bend` false: it does not bend that way
// at all. `max_twist`: the most the section turns about the route per
// length (radians per millimetre; 0: no limit; not for a round section).
struct FlexLimits {
  double bend_radius = 0.0;
  bool flat_bend = true;
  double edge_bend_radius = 0.0;
  bool edge_bend = true;
  double max_twist = 0.0;
};

// A node of a route. On a face: `body` is the index of its body among the
// router's obstacles and `face` that body's face (null: the body's surface
// nearest to the point), `point` a point of the face. In space: `body` -1.
// `twist` turns the section about the route from this node on (right hand
// about the tangent), spread over a length round the node.
// The first and the last node may be a port: `direction` is the way the
// route runs there (leaving at the first node, arriving at the last), kept
// exactly, and `down` the section's orientation there (before the route's
// own twists). A port is left clear of the bodies along its direction: the
// route's first stretch from it, about its half height long, is not held
// off them nor checked against them (it starts at the end of a pipe).
struct FlexNode {
  gp_Pnt point;
  int body = -1;
  TopoDS_Face face;
  double twist = 0.0;
  std::optional<gp_Dir> direction;
  std::optional<gp_Dir> down;
};

// A point of the route: its place, the route's unit tangent, the section's
// `down`, the length of the route up to it, and whether the item lies on a
// surface there (`down` is then against the surface's normal, turned by the
// twists): `body` and `face` are then the obstacle and its face (the index
// in TopExp order) it lies on, else -1. `node`: the index of the node the
// point is at, -1 between nodes; every node has its point.
struct FlexSample {
  gp_Pnt point;
  gp_Dir tangent;
  gp_Dir down;
  double at = 0.0;
  bool on_surface = false;
  int node = -1;
  int body = -1;
  int face = -1;
};

enum class FlexProblemKind {
  PassesThrough, // the item's section reaches into a body
  BendTooTight,  // the route bends tighter than the limits allow (a section
                 // that is not round: over its flat side)
  TwistOverLimit, // the section turns faster than `max_twist`
  NoCleanRoute,  // no way round the bodies was found between two nodes
  EdgeBendTooTight, // a section that is not round bends over its edge
                    // tighter than `edge_bend_radius` (or at all, without
                    // `edge_bend`)
};

// A problem of a route and where it is: stretches of the route by length
// along it (`from` to `to`), each with a point in it (the worst one) and,
// for a section that reaches into a body, that obstacle (-1: the item's
// own run, or none).
struct FlexProblem {
  struct Place {
    double from = 0.0;
    double to = 0.0;
    gp_Pnt point;
    int body = -1;
  };
  FlexProblemKind kind = FlexProblemKind::NoCleanRoute;
  std::vector<Place> places;
};

// A route: its curve (a B-spline, as a path of pipe() and sweep() takes
// it), points along it (at even steps of its parameter between the nodes,
// one at each node) with the section's
// frame, its length, the length along it at each node, and its problems.
struct FlexRoute {
  ModelCurve curve;
  std::vector<FlexSample> samples;
  double length = 0.0;
  std::vector<double> node_at;
  std::vector<FlexProblem> problems;
  // The work done, for tests that bound it (a clock does not hold under
  // load, mitcad#1970): the points the passes that bend the route within
  // its limits and off the bodies went over, summed over the passes of
  // each run.
  std::int64_t relax_work = 0;
};

// `deflection`, `angle`: the mesh of the bodies (the linear deflection in
// millimetres and the angular one). `relax_iterations`: the most rounds of
// the pass that bends the route within its limits and off the bodies, in
// each of its two runs; fewer for a quicker, rougher route while a node is
// dragged.
struct FlexRouterOptions {
  double deflection = 0.1;
  double angle = 0.2;
  int relax_iterations = 400;
};

// The router of one set of obstacles. Made once per set and kept by the
// caller while the set stays: it keeps the bodies' triangles (meshed on
// copies; the bodies are left as they are), the tree that finds the
// triangle nearest to a point, the graph the surface paths are searched in
// and where the bodies touch each other. route() only reads them: it
// may be called for every move of a dragged node.
// Routes of one router may be worked out on several threads at once.
class FlexRouter {
public:
  explicit FlexRouter(const std::vector<TopoDS_Shape>& obstacles, const FlexRouterOptions& options = {});
  ~FlexRouter();
  FlexRouter(FlexRouter&&) noexcept;
  FlexRouter& operator=(FlexRouter&&) noexcept;

  // The route through the nodes (two or more, not all at one place). Its
  // own run is in its way too: two stretches of it may lie on each other,
  // not pass through each other. Of a section that is not round, a stretch
  // along the surfaces leaves its first node the way that turns least
  // from where the stretch before arrives (weighted by the edgewise bend
  // radius; above all else when the item does not bend over its edge).
  // Throws only for input that is no route at all (fewer than two nodes, a
  // section without a size). `quick`: a quicker, rougher route, as a
  // pointer's preview shows it while it moves (mitcad#1713): its points
  // only at the coarser steps, the bends' sweeps fewer; its checks as any
  // route's.
  FlexRoute route(const std::vector<FlexNode>& nodes, const FlexSection& section, const FlexLimits& limits,
                  bool quick = false) const;

private:
  struct Impl;
  std::unique_ptr<Impl> m_impl;
};

// The solid of the section along the route, the section turning with the
// route's frames: a round one along a route in one plane as pipe() builds
// it, the others (and a round one along any other route) as sweep() builds
// them along the route with the frames as its auxiliary curve.
// Faces: the caps `start` and `end`; of a round section side0 (inner0 of a
// hollow one); of a rectangle side0 (the resting side), side1 (at +u),
// side2 and side3, counter-clockwise about the route, corner0 to corner3
// after each when the corners are rounded, and of a hollow one inner0 to
// inner3 with its rounded corners inner4 to inner7; of a profile side<i>
// in the order of its outer loop's segments and inner<i> in the order of
// the other loops' (with a wall: side<i> by the edges of the walled face).
// Where the route bends tighter than the section allows, the solid is built
// in pieces as pipes and sweeps are (mitcad#1697). Never a solid along a
// part of the route (mitcad#1713): a pipe or sweep with less than half the
// section's area along its route is tried another way, and a route that
// runs back along itself for more than a blip is built in straight pieces
// along all of it.
ShapePtr build_solid(const std::string& feature, const FlexRoute& route, const FlexSection& section);
// The same along a path given as curves end to end, with the frames of the
// samples (their points, tangents and `down`).
ShapePtr build_solid(const std::string& feature, const Path& path, const std::vector<FlexSample>& samples,
                     const FlexSection& section);

// An end of a pipe or of a Flex item, to start or end a route at: its
// place (the centre of the end), the way out of the item there, and the
// section's orientation.
struct FlexPort {
  gp_Pnt position;
  gp_Dir direction;
  gp_Dir down;
};

// The port at a route's start or end.
FlexPort end_port(const FlexRoute& route, bool at_start);

// The port of a planar end face of a body (a pipe's `start` or `end`, a
// Flex item's): the face's centre, its normal out of the body, and `down`
// square to the face's longest straight edge, towards it (any direction
// square to the normal for a face without straight edges, as a round
// one's). Throws when the body has no such face or it is not planar.
FlexPort end_port(const Shape& body, const std::string& face);

} // namespace mitcad::geometry::routing
