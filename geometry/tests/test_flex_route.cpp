// SPDX-License-Identifier: MIT
// Flex routes (mitcad#1713): the route of a flexible item through its nodes
// past the bodies, and the solid along it.
//
// - A hose of diameter 10 (bend radius 20) from the top of a 100 mm block
//   to the face beside it: it runs over the edge the two faces share, in
//   the plane through its nodes square to that edge, outside the block and
//   within its bend radius.
// - Between opposite faces of the block it goes round: about 200 mm, not
//   the 100 mm through.
// - Between two points in the air either side of the block it goes round
//   the block.
// - Along a cylinder of radius 30 a hose of diameter 6 follows the
//   cylinder: its points lie the hose's radius off it, its length is the
//   helix's between its nodes.
// - A flat cable 12 x 2 along the cylinder keeps its flat side on it, and
//   a half turn at a node shows in its frames: before the node `down`
//   points to the cylinder, after it away, and the route is lifted off
//   the cylinder by the turned section's reach, without problems. A turn
//   the route is too short for at the item's twist limit is returned with
//   its place.
// - A hose in a slot narrower than itself: the route comes back, with the
//   places where it reaches into the body or bends too tightly.
// - The solids: a hose's volume is its section's area times the route's
//   length, a hollow one's its wall's, a flat cable's (with rounded
//   corners; with the half turn too) its section's.
// - A wire off a plane (from a pipe's end to a block's top): its solid,
//   along the route's frames. On over the block's edge to its side (the
//   owner's case): outside the block, within its bend radius, without
//   problems, its solid and its hollow solid whole.
// - The ports: a route's ends, and a pipe's end face, from which a route
//   leaves along the pipe.
// - A flat cable onto the design of app.ui-windows-flex, its third node on
//   a wire's surface, the last back at the first or at the wire's end port;
//   and the solid of one of its routes, turned over its edge at a node,
//   whose curve swings round that turn between its samples.
// - A flat cable 10 x 1 that does not bend over its edge, half round a drum
//   of radius 15 with a half turn at its middle node (the app's screenshots,
//   mitcad#1713): the turn spread evenly about the node (no step), lifted
//   off the drum by no more than the turned section's reach, flat on the
//   drum before and after, its solid whole; standing on its edge round the
//   drum it bends over its edge, which is its own warning
//   (EdgeBendTooTight), not one about the flat side.
// - A route that turns back on itself (the owner's case: a node added at
//   the route's own start, beside a first node on a profile's end edge):
//   its solid whole, without the turn, the section's area along the route.
// - A route that runs 60 mm down and 60 mm back up beside itself: its
//   solid along the whole route (in straight pieces), not a stub of it.
// - Every route here is at most 1.5 times as long as the straight lines
//   through its nodes, or the way round a body between them (kLongest).
//
// Tolerances. The nodes' points and the directions at ports are exact
// (1e-6 mm). A route keeps half its height off the bodies, and a reach into
// one above 1e-3 mm is a problem (mitcad#1713): off a planar face its
// points are compared within 1e-3 mm; on the cylinder within 0.2 mm below
// and 0.5 mm above the hose's radius (twice the deflection of the triangles
// the route is kept off, and the pass's own deviation beside a bend); bend
// radii within 15 %; lengths against bounds that hold for any such route;
// volumes within 0.2 % (round) and 1 % (flat).

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <Poly_Triangulation.hxx>
#include <TopLoc_Location.hxx>
#include <GProp_GProps.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/flex_route.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;
using routing::build_solid;
using routing::end_port;
using routing::FlexLimits;
using routing::FlexNode;
using routing::FlexPort;
using routing::FlexProblem;
using routing::FlexProblemKind;
using routing::FlexRoute;
using routing::FlexRouter;
using routing::FlexSample;
using routing::FlexSection;
using routing::FlexSectionKind;

TopoDS_Shape block() { return BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(100, 100, 100)).Shape(); }

// The face of the shape whose centre is nearest to the point.
TopoDS_Face face_at(const TopoDS_Shape& shape, const gp_Pnt& point) {
  TopoDS_Face best;
  double nearest = 1e100;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f.Current(), props);
    const double distance = props.CentreOfMass().Distance(point);
    if (distance < nearest) {
      nearest = distance;
      best = TopoDS::Face(f.Current());
    }
  }
  return best;
}

FlexNode on_face(const gp_Pnt& point, const TopoDS_Face& face) {
  FlexNode node;
  node.point = point;
  node.body = 0;
  node.face = face;
  return node;
}

FlexNode in_space(const gp_Pnt& point) {
  FlexNode node;
  node.point = point;
  return node;
}

FlexSection hose(double diameter) {
  FlexSection section;
  section.width = diameter;
  return section;
}

FlexSection flat_cable() {
  FlexSection section;
  section.kind = FlexSectionKind::Rectangular;
  section.width = 12;
  section.height = 2;
  return section;
}

const FlexProblem* problem_of(const FlexRoute& route, FlexProblemKind kind) {
  for (const FlexProblem& problem : route.problems) {
    if (problem.kind == kind) {
      return &problem;
    }
  }
  return nullptr;
}

void print_route(const char* what, const FlexRoute& route) {
  std::printf("%s: length %.6f, %d points, %d problem(s)\n", what, route.length,
              static_cast<int>(route.samples.size()), static_cast<int>(route.problems.size()));
  for (const FlexProblem& problem : route.problems) {
    for (const FlexProblem::Place& place : problem.places) {
      std::printf("  problem %d from %.3f to %.3f\n", static_cast<int>(problem.kind), place.from, place.to);
    }
  }
}

// How far outside the block [0, 100]^3 the point lies (0 inside it).
double off_block(const gp_Pnt& p) {
  double sum = 0;
  for (const double c : {p.X(), p.Y(), p.Z()}) {
    const double d = std::max({-c, 0.0, c - 100.0});
    sum += d * d;
  }
  return std::sqrt(sum);
}

double least_off_block(const FlexRoute& route) {
  double least = 1e100;
  for (const FlexSample& sample : route.samples) {
    least = std::min(least, off_block(sample.point));
  }
  return least;
}

// The tightest bend's radius, by the circles through each point and its
// neighbours two points away.
double tightest_bend(const FlexRoute& route) {
  double tightest = 1e100;
  for (std::size_t i = 2; i + 2 < route.samples.size(); ++i) {
    const gp_Vec ab(route.samples[i - 2].point, route.samples[i].point);
    const gp_Vec bc(route.samples[i].point, route.samples[i + 2].point);
    const gp_Vec ac(route.samples[i - 2].point, route.samples[i + 2].point);
    const double twice = ab.Crossed(bc).Magnitude();
    if (twice > 1e-12) {
      tightest = std::min(tightest, ab.Magnitude() * bc.Magnitude() * ac.Magnitude() / (2 * twice));
    }
  }
  return tightest;
}

// The guard against absurd routes (mitcad#1713: 882 mm for a route of 147,
// 32 923 mm for one of 282): a route is at most 1.5 times as long as the
// shortest way it could take, the straight lines between its nodes' points
// or, where a body lies between them, the way round it (`round`, the
// geodesic at the section's half height off the bodies). What the bend
// radius adds to that is within the factor in every case here.
constexpr double kLongest = 1.5;

double node_chords(const FlexRoute& route) {
  double sum = 0;
  const FlexSample* before = nullptr;
  for (const FlexSample& sample : route.samples) {
    if (sample.node >= 0) {
      if (before != nullptr) {
        sum += before->point.Distance(sample.point);
      }
      before = &sample;
    }
  }
  return sum;
}

void check_not_long(const char* what, const FlexRoute& route, double round = 0) {
  const double shortest = std::max(node_chords(route), round);
  std::printf("%s: %.4f times the shortest way, %.3f\n", what, route.length / shortest, shortest);
  CHECK(route.length <= kLongest * shortest);
}

// (Integrated to 1e-6: the default integration of the swept B-spline faces
// of a flat item is off by about 1 %.)
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-6);
  return props.Mass();
}

FlexLimits bend(double radius) {
  FlexLimits limits;
  limits.bend_radius = radius;
  return limits;
}

FlexRoute over_the_edge(const FlexRouter& router, const TopoDS_Shape& body) {
  const TopoDS_Face top = face_at(body, gp_Pnt(50, 50, 100));
  const TopoDS_Face side = face_at(body, gp_Pnt(100, 50, 50));
  return router.route({on_face(gp_Pnt(50, 50, 100), top), on_face(gp_Pnt(100, 50, 50), side)}, hose(10), bend(20));
}

void test_over_the_edge() {
  const TopoDS_Shape body = block();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const FlexRoute route = over_the_edge(router, body);
  print_route("over the edge", route);
  CHECK(route.problems.empty());
  CHECK(route.samples.size() >= 2);
  // The nodes: the hose's radius off their faces.
  CHECK(route.samples.front().point.Distance(gp_Pnt(50, 50, 105)) < 1e-6);
  CHECK(route.samples.back().point.Distance(gp_Pnt(105, 50, 50)) < 1e-6);
  CHECK(route.samples.front().on_surface);
  CHECK(route.node_at.size() == 2);
  CHECK(std::abs(route.node_at.front()) < 1e-6);
  CHECK(std::abs(route.node_at.back() - route.length) < 1e-6);
  // Over the shared edge, in the plane y = 50, outside the block.
  double least_edge = 1e100;
  double off_plane = 0;
  for (const FlexSample& sample : route.samples) {
    least_edge = std::min(least_edge, std::hypot(sample.point.X() - 100, sample.point.Z() - 100));
    off_plane = std::max(off_plane, std::abs(sample.point.Y() - 50));
  }
  const double least = least_off_block(route);
  const double tightest = tightest_bend(route);
  std::printf("over the edge: %.4f off the block, %.4f off the edge, %.4f off the plane, tightest bend %.3f\n",
              least, least_edge, off_plane, tightest);
  CHECK(least >= 5 - 1e-3);
  CHECK(least_edge >= 5 - 1e-3 && least_edge <= 5 + 3);
  CHECK(off_plane < 0.05);
  CHECK(tightest >= 0.85 * 20);
  // No shorter than the way round the edge at the hose's radius (45 + 45
  // and a quarter of a circle of radius 5), and not much longer.
  CHECK(route.length > 97 && route.length < 115);
  check_not_long("over the edge", route, 90 + 2.5 * kPi);
}

void test_opposite_faces() {
  const TopoDS_Shape body = block();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const FlexRoute route = router.route({on_face(gp_Pnt(0, 50, 50), face_at(body, gp_Pnt(0, 50, 50))),
                                        on_face(gp_Pnt(100, 50, 50), face_at(body, gp_Pnt(100, 50, 50)))},
                                       hose(10), bend(20));
  print_route("opposite faces", route);
  CHECK(problem_of(route, FlexProblemKind::PassesThrough) == nullptr);
  CHECK(problem_of(route, FlexProblemKind::NoCleanRoute) == nullptr);
  CHECK(route.samples.front().point.Distance(gp_Pnt(-5, 50, 50)) < 1e-6);
  CHECK(route.samples.back().point.Distance(gp_Pnt(105, 50, 50)) < 1e-6);
  const double least = least_off_block(route);
  std::printf("opposite faces: %.4f off the block, tightest bend %.3f\n", least, tightest_bend(route));
  CHECK(least >= 5 - 1e-3);
  // Round the block: 50 + 100 + 50 along its faces.
  CHECK(route.length > 190 && route.length < 240);
  // (Round two edges at the hose's radius: 50 + 100 + 50 and two quarters.)
  check_not_long("opposite faces", route, 200 + 5 * kPi);
}

void test_through_the_air() {
  const TopoDS_Shape body = block();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const FlexRoute route =
      router.route({in_space(gp_Pnt(-50, 50, 50)), in_space(gp_Pnt(150, 50, 50))}, hose(10), bend(20));
  print_route("through the air", route);
  CHECK(problem_of(route, FlexProblemKind::PassesThrough) == nullptr);
  CHECK(problem_of(route, FlexProblemKind::NoCleanRoute) == nullptr);
  CHECK(route.samples.front().point.Distance(gp_Pnt(-50, 50, 50)) < 1e-6);
  CHECK(route.samples.back().point.Distance(gp_Pnt(150, 50, 50)) < 1e-6);
  CHECK(!route.samples[route.samples.size() / 2].on_surface);
  const double least = least_off_block(route);
  std::printf("through the air: %.4f off the block, tightest bend %.3f\n", least, tightest_bend(route));
  CHECK(least >= 5 - 1e-3);
  // Round one side of the block: two legs of about 70 mm and the side.
  CHECK(route.length > 220 && route.length < 290);
  // (To the corners of the block grown by the hose's radius and along it.)
  check_not_long("through the air", route, 2 * std::hypot(45.0, 55.0) + 100);
  // Without the block: straight.
  const FlexRouter empty(std::vector<TopoDS_Shape>{});
  const FlexRoute straight =
      empty.route({in_space(gp_Pnt(-50, 50, 50)), in_space(gp_Pnt(150, 50, 50))}, hose(10), bend(20));
  CHECK(straight.problems.empty());
  CHECK_NEAR(straight.length, 200.0);
}

TopoDS_Shape cylinder() { return BRepPrimAPI_MakeCylinder(30, 100).Shape(); }

TopoDS_Face cylinder_side(const TopoDS_Shape& body) {
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cylinder) {
      return TopoDS::Face(f.Current());
    }
  }
  return TopoDS_Face();
}

gp_Pnt on_cylinder(double angle, double z) { return gp_Pnt(30 * std::cos(angle), 30 * std::sin(angle), z); }

void test_along_a_cylinder() {
  const TopoDS_Shape body = cylinder();
  const TopoDS_Face side = cylinder_side(body);
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const FlexRoute route =
      router.route({on_face(on_cylinder(0, 30), side), on_face(on_cylinder(2 * kPi / 3, 70), side)}, hose(6), bend(10));
  print_route("along a cylinder", route);
  CHECK(route.problems.empty());
  double nearest = 1e100;
  double farthest = 0;
  int lying = 0;
  for (const FlexSample& sample : route.samples) {
    const double radius = std::hypot(sample.point.X(), sample.point.Y());
    nearest = std::min(nearest, radius);
    farthest = std::max(farthest, radius);
    lying += sample.on_surface ? 1 : 0;
  }
  // The helix at the hose's radius off the cylinder between the nodes.
  const double helix = std::hypot(33 * 2 * kPi / 3, 40.0);
  std::printf("along a cylinder: %.4f to %.4f from the axis, %d of %d points on it, the helix %.4f\n", nearest,
              farthest, lying, static_cast<int>(route.samples.size()), helix);
  CHECK(nearest >= 33 - 0.2);
  CHECK(farthest <= 33 + 0.5);
  CHECK(lying * 10 >= static_cast<int>(route.samples.size()) * 9);
  CHECK(std::abs(route.length - helix) < 1.5);
  check_not_long("along a cylinder", route, helix);
}

// A flat cable through three points of one helix on the cylinder, the
// middle one's twist as given.
FlexRoute cable_on_cylinder(const FlexRouter& router, const TopoDS_Face& side, double twist, double max_twist) {
  std::vector<FlexNode> nodes{on_face(on_cylinder(0, 20), side), on_face(on_cylinder(kPi / 3, 50), side),
                              on_face(on_cylinder(2 * kPi / 3, 80), side)};
  nodes[1].twist = twist;
  FlexLimits limits;
  limits.bend_radius = 5;
  limits.edge_bend_radius = 50;
  limits.max_twist = max_twist;
  return router.route(nodes, flat_cable(), limits);
}

// `down` along the way out from the cylinder's axis at the point: -1 with
// the flat side on the cylinder.
double down_outwards(const FlexSample& sample) {
  const gp_Vec out = gp_Vec(sample.point.X(), sample.point.Y(), 0).Normalized();
  return gp_Vec(sample.down).Dot(out);
}

void test_flat_cable() {
  const TopoDS_Shape body = cylinder();
  const TopoDS_Face side = cylinder_side(body);
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const FlexRoute lying = cable_on_cylinder(router, side, 0, 0.3);
  print_route("flat cable", lying);
  CHECK(lying.problems.empty());
  CHECK(lying.node_at.size() == 3);
  CHECK(lying.samples.front().node == 0 && lying.samples.back().node == 2);
  for (const FlexSample& sample : lying.samples) {
    CHECK(sample.node != 1 || std::abs(sample.at - lying.node_at[1]) < 1e-9);
    CHECK(!sample.on_surface || (sample.body == 0 && sample.face >= 0));
  }
  double least = 1;
  for (const FlexSample& sample : lying.samples) {
    least = std::min(least, -down_outwards(sample));
    CHECK(std::abs(gp_Vec(sample.down).Dot(gp_Vec(sample.tangent))) < 1e-6);
  }
  std::printf("flat cable: down against the cylinder's normal by at least %.6f\n", least);
  CHECK(least > 0.98);
  // (The helix through the nodes at the cable's half height off the
  // cylinder.)
  const double helix = 2 * std::hypot(31 * kPi / 3, 30.0);
  check_not_long("flat cable", lying, helix);

  // Half a turn at the middle node, over 1.5 pi / 0.3 = 15.7 mm about it.
  const FlexRoute turned = cable_on_cylinder(router, side, kPi, 0.3);
  print_route("flat cable, half a turn", turned);
  // (Half way through the turn the cable stands on its edge: the route is
  // lifted off the cylinder by the section's reach there, half its width,
  // and rises to it and comes down within its flat bend radius before and
  // after the turn: tilted by that rise up to about 12 mm from the node.)
  CHECK(turned.problems.empty());
  const double middle = turned.node_at[1];
  double highest = 0;
  for (const FlexSample& sample : turned.samples) {
    highest = std::max(highest, std::hypot(sample.point.X(), sample.point.Y()) - 30);
  }
  std::printf("flat cable, half a turn: lifted up to %.4f off the cylinder\n", highest);
  CHECK(highest >= 6 - 0.25);
  double before = 1;
  double after = 1;
  for (const FlexSample& sample : turned.samples) {
    if (sample.at < middle - 15) {
      before = std::min(before, -down_outwards(sample));
    } else if (sample.at > middle + 15) {
      after = std::min(after, down_outwards(sample));
    }
  }
  std::printf("flat cable, half a turn: down to the cylinder by %.6f before, away from it by %.6f after\n", before,
              after);
  CHECK(before > 0.98);
  CHECK(after > 0.98);
  check_not_long("flat cable, half a turn", turned, helix);

  // The half turn at a hundredth of a radian per millimetre takes 471 mm:
  // the route is 90 mm long.
  const FlexRoute steep = cable_on_cylinder(router, side, kPi, 0.01);
  print_route("flat cable, a turn too steep", steep);
  const FlexProblem* twist = problem_of(steep, FlexProblemKind::TwistOverLimit);
  CHECK(twist != nullptr);
  CHECK(twist == nullptr || !twist->places.empty());
  CHECK(steep.samples.size() >= 2);
  check_not_long("flat cable, a turn too steep", steep, helix);
}

void test_narrow_slot() {
  // A slot 6 mm wide and 20 deep along y in the top of a block; a hose of
  // diameter 10 from the slot's floor to the top beside the slot.
  const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(100, 100, 50)).Shape();
  const TopoDS_Shape slot = BRepPrimAPI_MakeBox(gp_Pnt(47, 0, 30), gp_Pnt(53, 100, 50)).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(slab, slot).Shape();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  FlexNode low = in_space(gp_Pnt(50, 50, 30));
  low.body = 0; // the body's surface nearest to the point
  FlexNode top = in_space(gp_Pnt(80, 50, 50));
  top.body = 0;
  const FlexRoute route = router.route({low, top}, hose(10), bend(20));
  print_route("narrow slot", route);
  CHECK(route.samples.size() >= 2);
  CHECK(route.length > 30);
  const FlexProblem* through = problem_of(route, FlexProblemKind::PassesThrough);
  const FlexProblem* tight = problem_of(route, FlexProblemKind::BendTooTight);
  CHECK(through != nullptr || tight != nullptr);
  // The hose does not fit where it starts.
  CHECK(through != nullptr && !through->places.empty() && through->places.front().from < 1.0);
  for (const FlexProblem& problem : route.problems) {
    for (const FlexProblem::Place& place : problem.places) {
      CHECK(place.from <= place.to && place.from >= -1e-9 && place.to <= route.length + 1e-9);
    }
  }
  // (Up out of the slot, 20, and across the top to the node, 30.)
  check_not_long("narrow slot", route, 50);
}

int solids_of(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer e(shape, TopAbs_SOLID); e.More(); e.Next()) {
    ++count;
  }
  return count;
}

// What `make` builds, with the time it took.
template <class F>
ShapePtr timed(const char* what, F&& make) {
  const auto started = std::chrono::steady_clock::now();
  ShapePtr made = make();
  std::printf("%s: built in %.3f s\n", what,
              std::chrono::duration<double>(std::chrono::steady_clock::now() - started).count());
  return made;
}

void check_solid(const char* what, const ShapePtr& solid, double expected, double relative) {
  const double volume = volume_of(solid->occt());
  const bool valid = BRepCheck_Analyzer(solid->occt()).IsValid();
  std::printf("%s: %d solid(s), valid %d, volume %.6f, the section's area times the length %.6f\n", what,
              solids_of(solid->occt()), valid ? 1 : 0, volume, expected);
  CHECK(solids_of(solid->occt()) == 1);
  CHECK(valid);
  CHECK_NEAR_TOL(volume, expected, relative);
}

void test_solids() {
  const TopoDS_Shape body = block();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const FlexRoute route = over_the_edge(router, body);
  const ShapePtr solid = build_solid("F5", route, hose(10));
  check_solid("hose", solid, kPi * 25 * route.length, 2e-3);
  CHECK(!solid->find_faces("F5:start").empty() && !solid->find_faces("F5:end").empty());
  FlexSection hollow = hose(10);
  hollow.wall = 1;
  check_solid("hollow hose", build_solid("F5", route, hollow), kPi * (25 - 16) * route.length, 2e-3);

  const TopoDS_Shape drum = cylinder();
  const TopoDS_Face side = cylinder_side(drum);
  const FlexRouter drum_router(std::vector<TopoDS_Shape>{drum});
  const FlexRoute lying = cable_on_cylinder(drum_router, side, 0, 0.3);
  check_solid("flat cable", build_solid("F6", lying, flat_cable()), 24 * lying.length, 1e-2);
  FlexSection rounded = flat_cable();
  rounded.corner_radius = 0.5;
  check_solid("flat cable, rounded corners", build_solid("F6", lying, rounded),
              (24 - (4 - kPi) * 0.25) * lying.length, 1e-2);
  FlexSection tube = flat_cable();
  tube.height = 4;
  tube.wall = 0.5;
  // (The same route: a section 4 high lies 1 mm into the cylinder, which
  // the solid does not mind.)
  const ShapePtr flat_tube = timed("flat tube", [&] { return build_solid("F6", lying, tube); });
  check_solid("flat tube", flat_tube, (12 * 4 - 11 * 3) * lying.length, 1e-2);
  const FlexRoute turned = cable_on_cylinder(drum_router, side, kPi, 0.3);
  check_solid("flat cable, half a turn", build_solid("F6", turned, flat_cable()), 24 * turned.length, 1e-2);
  // The faces' names; a profile (the rectangle 12 x 3 as an outline), and
  // with a wall of 0.5 its face less the outline drawn in by the wall.
  const ShapePtr cable = build_solid("F6", lying, flat_cable());
  CHECK(!cable->find_faces("F6:start").empty() && !cable->find_faces("F6:end").empty());
  for (const char* name : {"F6:side0", "F6:side1", "F6:side2", "F6:side3"}) {
    CHECK(!cable->find_faces(name).empty());
  }
  CHECK(!build_solid("F6", lying, rounded)->find_faces("F6:corner0").empty());
  CHECK(!flat_tube->find_faces("F6:inner0").empty());
  FlexSection outline;
  outline.kind = FlexSectionKind::Profile;
  outline.profile = rectangle_region(-6, -1.5, 12, 3);
  outline.width = 12;
  outline.height = 3;
  const ShapePtr strip = build_solid("F7", lying, outline);
  check_solid("outline", strip, 36 * lying.length, 1e-2);
  CHECK(!strip->find_faces("F7:side0").empty() && !strip->find_faces("F7:start").empty());
  outline.wall = 0.5;
  check_solid("hollow outline", timed("hollow outline", [&] { return build_solid("F7", lying, outline); }),
              (36 - 11 * 2) * lying.length, 1e-2);
}

// A hose whose last stretch crosses its first: A to B along x, on to C
// beside the middle of that stretch and across it to D. The item may lie
// on itself, not pass through itself: where it crosses, its two stretches
// are its diameter apart (within 0.5 mm, the pass's own deviation).
void test_own_run() {
  const FlexRouter empty(std::vector<TopoDS_Shape>{});
  const FlexRoute route = empty.route({in_space(gp_Pnt(0, 0, 0)), in_space(gp_Pnt(100, 0, 0)),
                                       in_space(gp_Pnt(50, 50, 2)), in_space(gp_Pnt(50, -50, 2))},
                                      hose(10), bend(15));
  print_route("own run", route);
  CHECK(problem_of(route, FlexProblemKind::PassesThrough) == nullptr);
  double least = 1e100;
  for (const FlexSample& a : route.samples) {
    for (const FlexSample& b : route.samples) {
      if (b.at - a.at > 40) {
        least = std::min(least, a.point.Distance(b.point));
      }
    }
  }
  std::printf("own run: its stretches %.4f apart where they cross\n", least);
  CHECK(least >= 10 - 0.5);
  CHECK(least <= 10 + 3);
  check_not_long("own run", route);
  int at_nodes = 0;
  for (const FlexSample& sample : route.samples) {
    at_nodes += sample.node >= 0 ? 1 : 0;
  }
  CHECK(at_nodes == 4);
}

// How tightly a flat item bends over its edge at each sample, as 1 / the
// radius: the turn of its tangent about its `down` (square to its flat
// side) between the samples two before and two after, over the length
// between them. (Not by the circle through the points, as the route's
// check measures it.)
std::vector<double> edgewise_of(const FlexRoute& route) {
  const std::size_t count = route.samples.size();
  std::vector<double> out(count, 0.0);
  for (std::size_t i = 2; i + 2 < count; ++i) {
    const gp_Vec down(route.samples[i].down);
    const gp_Vec before(route.samples[i - 2].tangent);
    const gp_Vec after(route.samples[i + 2].tangent);
    const double length = route.samples[i + 2].at - route.samples[i - 2].at;
    if (length > 1e-9) {
      out[i] = std::abs(std::atan2(before.Crossed(after).Dot(down), before.Dot(after))) / length;
    }
  }
  return out;
}

// Whether a place of the problem covers the length along the route.
bool covers(const FlexProblem& problem, double at, double slack) {
  for (const FlexProblem::Place& place : problem.places) {
    if (at >= place.from - slack && at <= place.to + slack) {
      return true;
    }
  }
  return false;
}

// A flat cable that does not bend over its edge, on the top of the block
// from A to B along y and on to C on the face at x = 100: the short way
// turns a quarter turn in the top face at B; it goes on over the far edge
// (y = 100) and round instead. A hose takes the short way.
//
// No route reaches C from B without bending over the edge (the faces it can
// reach along its flat side from B heading +y, crossing only edges along x,
// are the four round x = 40): the cable turns on its edge after B (mitcad#1713,
// the many places of BendTooTight it showed; EdgeBendTooTight's since the
// bends over the edge are their own problem). They are real, not of a wrong
// axis or of frames before a twist: on the top face `down` is the face's
// normal, so all the route's bending in that face is over the edge, and the
// cable's own turn of its tangent about `down` agrees with the places: each
// place bends over the edge, the turn at B is one of them, and where none
// is, it hardly bends that way. (The places other than B's, of radii from
// 300 to 1000 mm, are the pass's slight bends of a cable that may not bend
// over its edge at all, judged against 100 times its size, 1200 mm.)
//
// With a bend radius over the edge of 50 mm and room for it (three nodes on
// the top whose stretches turn by 30 degrees at the middle one), the cable
// turns within it: no BendTooTight nor EdgeBendTooTight, and its turn about `down` nowhere
// tighter than 50 mm (within the check's 15 %).
void test_edgewise() {
  const TopoDS_Shape body = block();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const TopoDS_Face top = face_at(body, gp_Pnt(50, 50, 100));
  const TopoDS_Face side = face_at(body, gp_Pnt(100, 50, 50));
  const std::vector<FlexNode> nodes{on_face(gp_Pnt(40, 20, 100), top), on_face(gp_Pnt(40, 70, 100), top),
                                    on_face(gp_Pnt(100, 70, 50), side)};
  const auto farthest_y = [](const FlexRoute& route) {
    double farthest = -1e100;
    for (const FlexSample& sample : route.samples) {
      farthest = std::max(farthest, sample.point.Y());
    }
    return farthest;
  };
  const FlexRoute plain = router.route(nodes, hose(4), bend(8));
  print_route("edgewise, a hose", plain);
  FlexLimits limits;
  limits.bend_radius = 5;
  limits.edge_bend = false;
  const FlexRoute flat = router.route(nodes, flat_cable(), limits);
  print_route("edgewise, a flat cable", flat);
  std::printf("edgewise: the hose reaches y = %.3f, the flat cable y = %.3f\n", farthest_y(plain), farthest_y(flat));
  CHECK(farthest_y(plain) < 85);
  CHECK(farthest_y(flat) > 100.5);
  CHECK(flat.samples.size() >= 2);
  // (Along the top and over the edge at x = 100 down the side: 50 + 60 +
  // 50; the flat cable round the far edge too, within the factor of that.)
  check_not_long("edgewise, a hose", plain, 160);
  check_not_long("edgewise, a flat cable", flat, 160);

  // The places over the edge, against the cable's own turn about `down`.
  // (`down` the top face's normal from A to B, and on to the turn after B.)
  for (const FlexSample& sample : flat.samples) {
    if (sample.at < flat.node_at[1]) {
      CHECK(gp_Vec(sample.down).Dot(gp_Vec(0, 0, -1)) > 1 - 1e-6);
    }
  }
  const FlexProblem* tight = problem_of(flat, FlexProblemKind::EdgeBendTooTight);
  CHECK(tight != nullptr);
  if (tight != nullptr) {
    const std::vector<double> edgewise = edgewise_of(flat);
    // (The cable may not bend over its edge: no tighter than 1200 mm, and
    // a problem from 1200 / 1.15.)
    const double limit = 1.0 / (1200.0 / 1.15);
    double at_b = 0;
    for (std::size_t i = 0; i < flat.samples.size(); ++i) {
      if (std::abs(flat.samples[i].at - flat.node_at[1]) < 3) {
        at_b = std::max(at_b, edgewise[i]);
      }
    }
    std::printf("edgewise: the flat cable turns over its edge at B to a radius of %.2f mm\n", 1.0 / at_b);
    CHECK(at_b > 10 * limit);
    CHECK(covers(*tight, flat.node_at[1], 3.0));
    int unplaced = 0;
    int bent = 0;
    for (const FlexProblem::Place& place : tight->places) {
      double most = 0;
      for (std::size_t i = 0; i < flat.samples.size(); ++i) {
        if (flat.samples[i].at >= place.from - 1.0 && flat.samples[i].at <= place.to + 1.0) {
          most = std::max(most, edgewise[i]);
        }
      }
      bent += most > 0.5 * limit ? 1 : 0;
    }
    for (std::size_t i = 0; i < flat.samples.size(); ++i) {
      unplaced += edgewise[i] > 3 * limit && !covers(*tight, flat.samples[i].at, 1.0) ? 1 : 0;
    }
    std::printf("edgewise: %d of %d places bend over the edge, %d points that bend so are in none\n", bent,
                static_cast<int>(tight->places.size()), unplaced);
    CHECK(bent == static_cast<int>(tight->places.size()));
    CHECK(unplaced == 0);
  }

  // Room for a bend over the edge of 50 mm.
  const std::vector<FlexNode> roomy{on_face(gp_Pnt(10, 10, 100), top), on_face(gp_Pnt(30, 50, 100), top),
                                    on_face(gp_Pnt(90, 90, 100), top)};
  FlexLimits edge;
  edge.bend_radius = 5;
  edge.edge_bend_radius = 50;
  const FlexRoute turning = router.route(roomy, flat_cable(), edge);
  print_route("edgewise, room for the bend", turning);
  const std::vector<double> turns = edgewise_of(turning);
  const double tightest = 1.0 / std::max(*std::max_element(turns.begin(), turns.end()), 1e-12);
  std::printf("edgewise, room for the bend: tightest over the edge %.3f\n", tightest);
  CHECK(problem_of(turning, FlexProblemKind::BendTooTight) == nullptr);
  CHECK(problem_of(turning, FlexProblemKind::EdgeBendTooTight) == nullptr);
  CHECK(tightest >= 50 / 1.15);
  check_not_long("edgewise, room for the bend", turning);
}

void test_ports() {
  // A route's ends.
  const TopoDS_Shape body = block();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const FlexRoute route = over_the_edge(router, body);
  const FlexPort start = end_port(route, true);
  const FlexPort end = end_port(route, false);
  CHECK(start.position.Distance(gp_Pnt(50, 50, 105)) < 1e-6);
  CHECK(end.position.Distance(gp_Pnt(105, 50, 50)) < 1e-6);
  CHECK(gp_Vec(start.direction).Dot(gp_Vec(route.samples.front().tangent)) < -1 + 1e-9);
  CHECK(gp_Vec(end.direction).Dot(gp_Vec(route.samples.back().tangent)) > 1 - 1e-9);
  // On the top face `down` is against the face's normal.
  CHECK(gp_Vec(start.down).Dot(gp_Vec(0, 0, -1)) > 1 - 1e-6);

  // A pipe's end: its centre, out of the pipe.
  ModelCurve line;
  line.kind = ModelCurveKind::Line;
  line.start = gp_Pnt(0, 0, 0);
  line.end = gp_Pnt(50, 0, 0);
  PipeSpec spec;
  spec.feature = "F9";
  spec.path = {{"c1", line}};
  spec.size = 10;
  const ShapePtr tube = pipe(spec);
  const FlexPort port = end_port(*tube, "F9:end");
  CHECK(port.position.Distance(gp_Pnt(50, 0, 0)) < 1e-6);
  CHECK(gp_Vec(port.direction).Dot(gp_Vec(1, 0, 0)) > 1 - 1e-9);
  CHECK(std::abs(gp_Vec(port.down).Dot(gp_Vec(port.direction))) < 1e-9);
  const FlexPort other = end_port(*tube, "F9:start");
  CHECK(other.position.Distance(gp_Pnt(0, 0, 0)) < 1e-6);
  CHECK(gp_Vec(other.direction).Dot(gp_Vec(-1, 0, 0)) > 1 - 1e-9);
  CHECK(throws_with([&] { end_port(*tube, "F9:nothing"); }, "the body has no such face"));
  // A square pipe's end: `down` square to a side.
  spec.section = PipeSection::Square;
  const ShapePtr bar = pipe(spec);
  const FlexPort square = end_port(*bar, "F9:end");
  const gp_Vec down(square.down);
  CHECK(std::abs(down.Dot(gp_Vec(1, 0, 0))) < 1e-9);
  CHECK(std::max(std::abs(down.Y()), std::abs(down.Z())) > 1 - 1e-9);

  // A hose from the pipe's end, the pipe in its way: it leaves along the
  // pipe and reaches into nothing.
  const FlexRouter beside(std::vector<TopoDS_Shape>{tube->occt()});
  FlexNode from = in_space(port.position);
  from.direction = port.direction;
  from.down = port.down;
  const FlexRoute leaving = beside.route({from, in_space(gp_Pnt(100, 40, 0))}, hose(10), bend(20));
  print_route("from a pipe's end", leaving);
  CHECK(problem_of(leaving, FlexProblemKind::PassesThrough) == nullptr);
  CHECK(leaving.samples.front().point.Distance(gp_Pnt(50, 0, 0)) < 1e-6);
  CHECK(gp_Vec(leaving.samples.front().tangent).Dot(gp_Vec(1, 0, 0)) > 1 - 1e-6);
  CHECK(leaving.samples.back().point.Distance(gp_Pnt(100, 40, 0)) < 1e-6);
  std::printf("from a pipe's end: tightest bend %.3f\n", tightest_bend(leaving));
  CHECK(tightest_bend(leaving) >= 0.85 * 20);
  check_not_long("from a pipe's end", leaving);
}

// A wire of 1 mm from the top end of an upright pipe to the top of a block
// beside it: a route that is not in one plane. Its solid comes along the
// route's frames (pipe()'s own frame took minutes on such a spline), whole,
// with its section's volume and its faces' names.
void test_wire_off_a_plane() {
  ModelCurve line;
  line.kind = ModelCurveKind::Line;
  line.start = gp_Pnt(-40, 0, 0);
  line.end = gp_Pnt(-40, 0, 30);
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = {{"c1", line}};
  spec.size = 8;
  const ShapePtr tube = pipe(spec);
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, 0), gp_Pnt(40, 20, 20)).Shape();
  const FlexRouter router(std::vector<TopoDS_Shape>{tube->occt(), box});
  const FlexPort port = end_port(*tube, "F2:end");
  FlexNode from = in_space(port.position);
  from.direction = port.direction;
  from.down = port.down;
  FlexNode to = on_face(gp_Pnt(20.27, -0.27, 20), face_at(box, gp_Pnt(20, 0, 20)));
  to.body = 1;
  const FlexRoute route = router.route({from, to}, hose(1), bend(4));
  print_route("a wire off a plane", route);
  CHECK(route.samples.back().point.Distance(gp_Pnt(20.27, -0.27, 20.5)) < 1e-6);
  check_not_long("a wire off a plane", route);
  const ShapePtr wire = build_solid("F6", route, hose(1));
  check_solid("a wire off a plane", wire, kPi * 0.25 * route.length, 1e-2);
  for (const char* name : {"F6:start", "F6:end", "F6:side0"}) {
    CHECK(!wire->find_faces(name).empty());
  }
}

// A route's curve as OCCT's B-spline (its knots given one by one).
occ::handle<Geom_BSplineCurve> bspline_of(const ModelCurve& curve) {
  std::vector<double> knots;
  std::vector<int> multiplicities;
  for (const double knot : curve.knots) {
    if (!knots.empty() && std::abs(knot - knots.back()) < 1e-12) {
      ++multiplicities.back();
    } else {
      knots.push_back(knot);
      multiplicities.push_back(1);
    }
  }
  NCollection_Array1<gp_Pnt> poles(1, static_cast<int>(curve.poles.size()));
  for (std::size_t i = 0; i < curve.poles.size(); ++i) {
    poles.SetValue(static_cast<int>(i) + 1, curve.poles[i]);
  }
  NCollection_Array1<double> values(1, static_cast<int>(knots.size()));
  NCollection_Array1<int> counts(1, static_cast<int>(knots.size()));
  for (std::size_t i = 0; i < knots.size(); ++i) {
    values.SetValue(static_cast<int>(i) + 1, knots[i]);
    counts.SetValue(static_cast<int>(i) + 1, multiplicities[i]);
  }
  return new Geom_BSplineCurve(poles, values, counts, curve.degree);
}

// How far outside the box [lo, hi] the point lies (0 inside it).
double off_box(const gp_Pnt& p, const gp_Pnt& lo, const gp_Pnt& hi) {
  const double dx = std::max({lo.X() - p.X(), 0.0, p.X() - hi.X()});
  const double dy = std::max({lo.Y() - p.Y(), 0.0, p.Y() - hi.Y()});
  const double dz = std::max({lo.Z() - p.Z(), 0.0, p.Z() - hi.Z()});
  return std::sqrt(dx * dx + dy * dy + dz * dz);
}

// The owner's main case (app.ui-windows-flex): the wire off a plane goes
// on from the block's top over its edge to a node on its side at x = 40.
// Everywhere, over the edge too, its centre is its radius off the block
// (within 1e-3 mm: it passed 0.415 mm from the edge, 0.085 mm into the
// block, and no problem was reported, mitcad#1713), it bends within its
// bend radius, and its solid is built.
void test_wire_over_an_edge() {
  ModelCurve line;
  line.kind = ModelCurveKind::Line;
  line.start = gp_Pnt(-40, 0, 0);
  line.end = gp_Pnt(-40, 0, 30);
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = {{"c1", line}};
  spec.size = 8;
  const ShapePtr tube = pipe(spec);
  line.start = gp_Pnt(80, 0, 0);
  line.end = gp_Pnt(80, 0, 30);
  spec.feature = "F5";
  spec.path = {{"c1", line}};
  const ShapePtr other = pipe(spec);
  const gp_Pnt lo(0, -20, 0);
  const gp_Pnt hi(40, 20, 20);
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(lo, hi).Shape();
  const FlexRouter router(std::vector<TopoDS_Shape>{tube->occt(), box, other->occt()});
  const FlexPort port = end_port(*tube, "F2:end");
  FlexNode from = in_space(port.position);
  from.direction = port.direction;
  from.down = port.down;
  FlexNode top = on_face(gp_Pnt(20.27, -0.27, 20), face_at(box, gp_Pnt(20, 0, 20)));
  top.body = 1;
  FlexNode side = on_face(gp_Pnt(40, 0, 10), face_at(box, gp_Pnt(40, 0, 10)));
  side.body = 1;
  const auto started = std::chrono::steady_clock::now();
  const FlexRoute route = router.route({from, top, side}, hose(1), bend(4));
  std::printf("a wire over an edge: routed in %.3f s\n",
              std::chrono::duration<double>(std::chrono::steady_clock::now() - started).count());
  print_route("a wire over an edge", route);
  CHECK(route.problems.empty());
  CHECK(route.samples.back().point.Distance(gp_Pnt(40.5, 0, 10)) < 1e-6);
  double least = 1e100;
  gp_Pnt nearest;
  for (const FlexSample& sample : route.samples) {
    if (off_box(sample.point, lo, hi) < least) {
      least = off_box(sample.point, lo, hi);
      nearest = sample.point;
    }
  }
  // (And between the samples: the route's curve itself, at ten times as
  // many points.)
  double between = 1e100;
  const occ::handle<Geom_BSplineCurve> spline = bspline_of(route.curve);
  const int steps = 10 * static_cast<int>(route.samples.size());
  for (int i = 0; i <= steps; ++i) {
    const double span = spline->LastParameter() - spline->FirstParameter();
    const double u = spline->FirstParameter() + span * i / static_cast<double>(steps);
    between = std::min(between, off_box(spline->Value(u), lo, hi));
  }
  const double tightest = tightest_bend(route);
  std::printf("a wire over an edge: %.4f off the block (at %.3f %.3f %.3f), %.4f along its curve, tightest bend %.4f\n",
              least, nearest.X(), nearest.Y(), nearest.Z(), between, tightest);
  CHECK(least >= 0.5 - 1e-3);
  CHECK(between >= 0.5 - 1e-3);
  CHECK(problem_of(route, FlexProblemKind::PassesThrough) == nullptr);
  CHECK(tightest >= 0.85 * 4);
  check_not_long("a wire over an edge", route);
  // (Swept along the route's frames: a spline of many spans, whose swept
  // surface lay up to 0.9 mm off it, 1 % of its volume, before the sweep
  // was let approximate its frames in more pieces.)
  const ShapePtr wire = timed("a wire over an edge", [&] { return build_solid("F6", route, hose(1)); });
  check_solid("a wire over an edge", wire, kPi * 0.25 * route.length, 2e-3);
  // Hollow along the same route (not in one plane): a whole solid with its
  // wall's volume and its bore's faces (it was an empty shape).
  FlexSection hollow = hose(1);
  hollow.wall = 0.2;
  const ShapePtr bored = timed("a hollow wire over an edge", [&] { return build_solid("F6", route, hollow); });
  check_solid("a hollow wire over an edge", bored, kPi * (0.25 - 0.09) * route.length, 2e-3);
  for (const char* name : {"F6:start", "F6:end", "F6:side0", "F6:inner0"}) {
    CHECK(!bored->find_faces(name).empty());
  }
}

// A hose of diameter 6 (bend radius 10) from a node in the air down to a
// node on a block's top and up to a node in the air beyond it (the design
// of app.ui-windows-flex-edit): a route without problems.
void test_hose_over_a_top() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(20, 20, 0), gp_Pnt(60, 50, 20)).Shape();
  const FlexRouter router(std::vector<TopoDS_Shape>{box});
  FlexNode top = on_face(gp_Pnt(40, 35, 20), face_at(box, gp_Pnt(40, 35, 20)));
  const FlexRoute route =
      router.route({in_space(gp_Pnt(-10, 35, 30)), top, in_space(gp_Pnt(90, 35, 30))}, hose(6), bend(10));
  print_route("a hose over a top", route);
  CHECK(route.problems.empty());
  std::printf("a hose over a top: tightest bend %.4f\n", tightest_bend(route));
  check_not_long("a hose over a top", route);
}

// The point of the route nearest to `near`.
gp_Pnt route_point_near(const FlexRoute& route, const gp_Pnt& near) {
  gp_Pnt best = route.samples.front().point;
  for (const FlexSample& sample : route.samples) {
    if (sample.point.Distance(near) < best.Distance(near)) {
      best = sample.point;
    }
  }
  return best;
}

// A flat cable 10 x 2 (bend radius 4, not over its edge) from the block's
// top onto two places on a swept wire of 1 mm beside it (a design
// app.ui-windows-flex made after its second item: the pipes, the block, the
// wire over the block's top and side, and the wire from the block's side
// through two points in the air). A route under the pointer: it took 21 s
// on the build VM (mitcad#1713), its bends swept on to the last pass; here
// its work is bounded, whatever it finds. (Not its time: 5 s failed in a
// Linux debug build, 7 s unloaded and more under a check's load,
// mitcad#1970.)
void test_onto_a_swept_wire() {
  ModelCurve line;
  line.kind = ModelCurveKind::Line;
  line.start = gp_Pnt(-40, 0, 0);
  line.end = gp_Pnt(-40, 0, 30);
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = {{"c1", line}};
  spec.size = 8;
  const ShapePtr tube = pipe(spec);
  line.start = gp_Pnt(80, 0, 0);
  line.end = gp_Pnt(80, 0, 30);
  spec.feature = "F5";
  spec.path = {{"c1", line}};
  const ShapePtr other = pipe(spec);
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, 0), gp_Pnt(40, 20, 20)).Shape();
  const TopoDS_Face top_face = face_at(box, gp_Pnt(20, 0, 20));
  const FlexRouter first(std::vector<TopoDS_Shape>{tube->occt(), box, other->occt()});
  const FlexPort port = end_port(*tube, "F2:end");
  FlexNode from = in_space(port.position);
  from.direction = port.direction;
  from.down = port.down;
  FlexNode top = on_face(gp_Pnt(20.27, -0.27, 20), top_face);
  top.body = 1;
  FlexNode front = on_face(gp_Pnt(19.84, -20, 9.92), face_at(box, gp_Pnt(20, -20, 10)));
  front.body = 1;
  const ShapePtr flex1 = build_solid("F6", first.route({from, top, front}, hose(1), bend(4)), hose(1));
  FlexNode side = on_face(gp_Pnt(40, 0.16, 9.92), face_at(box, gp_Pnt(40, 0, 10)));
  side.body = 1;
  const FlexRoute second = first.route(
      {side, in_space(gp_Pnt(65.76, -82.30, 9.92)), in_space(gp_Pnt(101.47, -54.59, 9.92))}, hose(1), bend(4));
  const ShapePtr flex2 = build_solid("F7", second, hose(1));
  const std::vector<int> wire_faces = flex2->find_faces("F7:side0");
  CHECK(wire_faces.size() == 1);
  if (wire_faces.size() != 1) {
    return;
  }
  const TopoDS_Face wire_face = flex2->face(wire_faces.front());

  const auto started = std::chrono::steady_clock::now();
  const FlexRouter router(
      std::vector<TopoDS_Shape>{tube->occt(), box, other->occt(), flex1->occt(), flex2->occt()});
  const double meshed = std::chrono::duration<double>(std::chrono::steady_clock::now() - started).count();
  FlexNode on_top = on_face(gp_Pnt(28, 8, 20), top_face);
  on_top.body = 1;
  FlexNode on_wire1 = on_face(route_point_near(second, gp_Pnt(65.76, -82.30, 9.92)).Translated(gp_Vec(0, 0, 0.5)),
                              wire_face);
  on_wire1.body = 4;
  FlexNode on_wire2 = on_face(route_point_near(second, gp_Pnt(101.47, -54.59, 9.92)).Translated(gp_Vec(0, 0, 0.5)),
                              wire_face);
  on_wire2.body = 4;
  FlexSection cable = flat_cable();
  cable.width = 10;
  FlexLimits limits = bend(4);
  limits.flat_bend = true;
  limits.edge_bend = false;
  const auto routing = std::chrono::steady_clock::now();
  const FlexRoute two = router.route({on_top, on_wire1}, cable, limits);
  const double two_took = std::chrono::duration<double>(std::chrono::steady_clock::now() - routing).count();
  const auto routing3 = std::chrono::steady_clock::now();
  const FlexRoute three = router.route({on_top, on_wire1, on_wire2}, cable, limits);
  const double three_took = std::chrono::duration<double>(std::chrono::steady_clock::now() - routing3).count();
  print_route("onto a swept wire, two nodes", two);
  print_route("onto a swept wire, three nodes", three);
  std::printf("onto a swept wire: meshed in %.3f s, routed in %.3f s (two nodes) and %.3f s (three), work %lld and "
              "%lld\n",
              meshed, two_took, three_took, static_cast<long long>(two.relax_work),
              static_cast<long long>(three.relax_work));
  // The quick routes the pointer's preview takes (mitcad#1713): within a
  // few per cent of the length (0.33 s and 0.56 s on the build VM, where the
  // full routes took 0.57 s and 1.42 s).
  for (const FlexRoute* full : {&two, &three}) {
    const auto quick_started = std::chrono::steady_clock::now();
    const FlexRoute quick = full == &two ? router.route({on_top, on_wire1}, cable, limits, true)
                                         : router.route({on_top, on_wire1, on_wire2}, cable, limits, true);
    const double quick_took = std::chrono::duration<double>(std::chrono::steady_clock::now() - quick_started).count();
    std::printf("onto a swept wire, quick, %zu nodes: routed in %.3f s, work %lld, length %.6f (the full route's "
                "%.6f)\n",
                full == &two ? std::size_t{2} : std::size_t{3}, quick_took, static_cast<long long>(quick.relax_work),
                quick.length, full->length);
    CHECK(quick.samples.size() >= 2);
    CHECK_NEAR_TOL(quick.length, full->length, 5e-2);
    CHECK(quick.relax_work > 0 && quick.relax_work < full->relax_work);
    CHECK(quick.relax_work <= 100000);
  }
  CHECK(two.samples.size() >= 2 && three.samples.size() >= 2);
  // The work: about twice what the routes take (69 400 and 133 500 points
  // passed over in a Linux build, the quick ones 27 400 and 39 500); bends
  // that sweep on to the last pass take over a million.
  CHECK(two.relax_work > 0 && two.relax_work <= 150000);
  CHECK(three.relax_work > 0 && three.relax_work <= 270000);

  // The cable's solid along those routes, as the item's preview and the
  // item build it (mitcad#1713: "the profile could not be swept along the
  // path"), also with its first node turned a quarter. Where the route
  // turns over the cable's edge round the wire (0.25 mm at the second
  // node, a problem the route reports) the solid is eased to the bend the
  // cable can take: its volume is then up to a few per cent less than the
  // section's area times the route's length, never more.
  const double area = cable.width * cable.height;
  check_solid("onto a swept wire, two nodes, the solid",
              timed("onto a swept wire, two nodes, the solid", [&] { return build_solid("F8", two, cable); }),
              area * two.length, 2e-2);
  const auto check_eased = [area](const char* what, const FlexRoute& route, const ShapePtr& solid, double least) {
    const double volume = volume_of(solid->occt());
    const double full = area * route.length;
    const bool valid = BRepCheck_Analyzer(solid->occt()).IsValid();
    std::printf("%s: %d solid(s), valid %d, volume %.6f, %.4f of the section's area times the length %.6f\n", what,
                solids_of(solid->occt()), valid ? 1 : 0, volume, volume / full, full);
    CHECK(solids_of(solid->occt()) == 1);
    CHECK(valid);
    CHECK(volume >= least * full && volume <= 1.002 * full);
  };
  check_eased("onto a swept wire, three nodes, the solid", three,
              timed("onto a swept wire, three nodes, the solid", [&] { return build_solid("F8", three, cable); }),
              0.95);
  check_not_long("onto a swept wire, two nodes", two);
  check_not_long("onto a swept wire, three nodes", three);
  // The first node turned a quarter: the cable stands on its edge from
  // there on, and is kept off the bodies by half its width. (It ran away to
  // 882 mm, and its solid, eased round the bends that made, had 0.85 of
  // the section's area times that length, mitcad#1713.)
  on_top.twist = kPi / 2;
  const FlexRoute turned = router.route({on_top, on_wire1, on_wire2}, cable, limits);
  print_route("onto a swept wire, the first node turned a quarter", turned);
  check_not_long("onto a swept wire, the first node turned a quarter", turned);
  check_solid("onto a swept wire, the first node turned a quarter, the solid",
              timed("onto a swept wire, turned, the solid", [&] { return build_solid("F8", turned, cable); }),
              area * turned.length, 1e-2);
}


// The design of app.ui-windows-flex as its script lays it, the nodes where
// its clicks landed (in either layout): the pipes and the block, Flex1
// (a wire of 1 mm from Pipe1's end over the block's top to its front) and
// Flex2 (a wire from the block's side through two points in the air), and a
// flat cable 10 x 2 (bend radius 4, not over its edge) laid from the top
// onto Flex2. Its third node is where a click landed on the seam of Flex2's
// side: a node on an edge, which the model gives the router as a point, on
// the wire's surface. Under the pointer the routes back to the first node
// and on to Flex2's end port came out 32 923 and 3 942 mm (16 259 and
// 3 976 mm in the app), the full one back to the first node 8 811 mm
// (mitcad#1713): a node 5 mm above the rim of the wire's end was taken for
// inside the wire and pushed its neighbour 40 mm away a pass, and the curve
// was lifted again and again beside the node on the wire until it rang.
// Each route is within kLongest of the straight lines through its nodes.
FlexNode ui_on(const gp_Pnt& point, const TopoDS_Face& face) {
  FlexNode node = on_face(point, face);
  node.body = 1;
  return node;
}

// Where the clicks of app.ui-windows-flex landed in one layout.
struct UiClicks {
  const char* layout;
  gp_Pnt flex1_top;
  gp_Pnt flex1_front;
  gp_Pnt flex2_side;
  gp_Pnt flex2_air;
  gp_Pnt flex2_end;
  gp_Pnt start;
  gp_Pnt air;
  gp_Pnt on_wire;
};

void ui_design(const UiClicks& clicks) {
  ModelCurve line;
  line.kind = ModelCurveKind::Line;
  line.start = gp_Pnt(-40, 0, 0);
  line.end = gp_Pnt(-40, 0, 30);
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = {{"c1", line}};
  spec.size = 8;
  const ShapePtr tube = pipe(spec);
  line.start = gp_Pnt(80, 0, 0);
  line.end = gp_Pnt(80, 0, 30);
  spec.feature = "F5";
  spec.path = {{"c1", line}};
  const ShapePtr other = pipe(spec);
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, 0), gp_Pnt(40, 20, 20)).Shape();
  const TopoDS_Face top = face_at(box, gp_Pnt(20, 0, 20));
  const FlexRouter three(std::vector<TopoDS_Shape>{tube->occt(), box, other->occt()});
  const FlexPort pipe_end = end_port(*tube, "F2:end");
  FlexNode from = in_space(pipe_end.position);
  from.direction = pipe_end.direction;
  from.down = pipe_end.down;
  const FlexNode front = ui_on(clicks.flex1_front, face_at(box, gp_Pnt(20, -20, 10)));
  const FlexRoute first = three.route({from, ui_on(clicks.flex1_top, top), front}, hose(1), bend(4));
  const FlexRoute second = three.route({ui_on(clicks.flex2_side, face_at(box, gp_Pnt(40, 0, 10))),
                                        in_space(clicks.flex2_air), in_space(clicks.flex2_end)},
                                       hose(1), bend(4));
  const std::string name = std::string("the ui design, ") + clicks.layout;
  print_route((name + ", Flex1").c_str(), first);
  print_route((name + ", Flex2").c_str(), second);
  const FlexRouter router(std::vector<TopoDS_Shape>{tube->occt(), box, other->occt(),
                                                    build_solid("F6", first, hose(1))->occt(),
                                                    build_solid("F7", second, hose(1))->occt()});
  FlexSection cable = flat_cable();
  cable.width = 10;
  FlexLimits limits = bend(4);
  limits.flat_bend = true;
  limits.edge_bend = false;
  const FlexNode start = ui_on(clicks.start, top);
  const FlexNode air = in_space(clicks.air);
  const FlexNode on_wire = in_space(clicks.on_wire);
  // (Flex2's end port as the last node: the route arrives against its
  // direction.)
  const FlexPort end = end_port(second, false);
  FlexNode port = in_space(end.position);
  port.direction = gp_Dir(gp_Vec(end.direction).Reversed());
  port.down = end.down;
  FlexNode turned = start;
  turned.twist = kPi / 2;

  // Each route, and the solid along a full one that ends elsewhere than it
  // starts (the item's preview).
  const auto routed = [&](const char* what, const std::vector<FlexNode>& nodes, bool quick) {
    const std::string named = name + ", " + what;
    const auto started = std::chrono::steady_clock::now();
    const FlexRoute route = router.route(nodes, cable, limits, quick);
    std::printf("%s: routed in %.3f s\n", named.c_str(),
                std::chrono::duration<double>(std::chrono::steady_clock::now() - started).count());
    print_route(named.c_str(), route);
    CHECK(route.samples.size() >= 2);
    check_not_long(named.c_str(), route);
    if (!quick && nodes.front().point.Distance(nodes.back().point) > 1e-6) {
      const ShapePtr solid = timed((named + ", the solid").c_str(), [&] { return build_solid("F8", route, cable); });
      const double volume = volume_of(solid->occt());
      const double full = (cable.round() ? kPi * 0.25 * cable.width * cable.width : cable.width * cable.height) *
                          route.length;
      std::printf("%s, the solid: %d solid(s), %.4f of the section's area times the length\n", named.c_str(),
                  solids_of(solid->occt()), volume / full);
      CHECK(solids_of(solid->occt()) == 1);
    }
  };
  routed("three nodes", {start, air, on_wire}, false);
  routed("the first node turned a quarter", {turned, air, on_wire}, false);
  routed("back to the first node", {start, air, on_wire, start}, true);
  routed("back to the first node, full", {start, air, on_wire, start}, false);
  routed("on to Flex2's end port", {start, air, on_wire, port}, true);
  routed("onto Flex2's end port", {start, air, port}, true);
  cable = hose(1);
  limits = bend(4);
  routed("a wire back to the first node", {start, air, on_wire, start}, true);
}

#include "flex_turned_over_edge.inc"

// The solid along a route whose curve swings round a sharp turn at a node
// between its samples (flex_turned_over_edge.inc: a flat cable that may not
// bend over its edge turned so at a node in the air, 0.4 mm round): along a
// spline through the samples, whole, with the section's area times the
// length within 1 % (mitcad#1713: "the profile could not be swept along the
// path", in app.ui-windows-flex's Floating layout).
void test_turned_over_its_edge() {
  FlexRoute route;
  route.curve.kind = ModelCurveKind::BSpline;
  route.curve.degree = kTurnedOverEdgeDegree;
  for (const auto& pole : kTurnedOverEdgePoles) {
    route.curve.poles.push_back(gp_Pnt(pole[0], pole[1], pole[2]));
  }
  for (const double knot : kTurnedOverEdgeKnots) {
    route.curve.knots.push_back(knot);
  }
  for (const auto& s : kTurnedOverEdgeSamples) {
    FlexSample sample;
    sample.point = gp_Pnt(s[0], s[1], s[2]);
    sample.tangent = gp_Dir(s[3], s[4], s[5]);
    sample.down = gp_Dir(s[6], s[7], s[8]);
    sample.at = s[9];
    route.samples.push_back(sample);
  }
  route.length = route.samples.back().at;
  std::printf("turned over its edge: tightest bend %.4f\n", tightest_bend(route));
  FlexSection cable = flat_cable();
  cable.width = 10;
  check_solid("turned over its edge",
              timed("turned over its edge", [&] { return build_solid("F8", route, cable); }), 20 * route.length, 1e-2);
}

void test_the_ui_design() {
  // (As the app logs them.)
  ui_design(UiClicks{"floating", gp_Pnt(20.33274831, -0.3327502169, 20), gp_Pnt(19.98268784, -20, 9.99134392),
                     gp_Pnt(40, 0.01730834554, 9.991345827), gp_Pnt(55.81311694, -80.85206692, 9.991345827),
                     gp_Pnt(103.7870476, -42.21263587, 9.991345827), gp_Pnt(11.9983737, 8.001624396, 20),
                     gp_Pnt(47.70526699, -104.3099754, 20), gp_Pnt(57.84505463, -80.61762238, 9.991345406)});
  ui_design(UiClicks{"docked", gp_Pnt(20.24353425, -0.2435361615, 20), gp_Pnt(19.96928844, -20, 9.984644222),
                     gp_Pnt(40, 0.03070774078, 9.98464613), gp_Pnt(41.85854817, -42.53541165, 9.98464613),
                     gp_Pnt(60.97180416, -29.01799547, 9.98464613), gp_Pnt(12.12956653, 7.870431563, 20),
                     gp_Pnt(44.2182448, -56.84891888, 20), gp_Pnt(43.58076096, -42.53222275, 9.984645844)});
}

// The flat cable of the app's screenshots (tools/ui-scripts/flex-shots-edit.json):
// 10 x 1, bend radius 5 and none over its edge, half round a drum of radius 15
// with a half turn at its middle node and no twist limit.
void test_half_turn_round_a_drum() {
  const TopoDS_Shape drum = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(40, -40, 0), gp_Dir(0, 0, 1)), 15, 40).Shape();
  const TopoDS_Face side = cylinder_side(drum);
  const FlexRouter router(std::vector<TopoDS_Shape>{drum});
  std::vector<FlexNode> nodes{on_face(gp_Pnt(25, -40, 8), side), on_face(gp_Pnt(40, -55, 20), side),
                              on_face(gp_Pnt(55, -40, 32), side)};
  nodes[1].twist = kPi;
  FlexSection section;
  section.kind = FlexSectionKind::Rectangular;
  section.width = 10;
  section.height = 1;
  FlexLimits limits;
  limits.bend_radius = 5;
  limits.edge_bend = false;
  const FlexRoute route = router.route(nodes, section, limits);
  print_route("half a turn round a drum", route);
  CHECK(route.node_at.size() == 3);
  const double middle = route.node_at.size() == 3 ? route.node_at[1] : 0;
  // The way out from the drum's axis at a point, and how far off the drum.
  const auto out_of = [](const gp_Pnt& p) { return gp_Vec(p.X() - 40, p.Y() + 40, 0); };
  double highest = 0;
  double steepest = 0;
  double before = 1;
  double after = 1;
  double at_node = 1;
  for (std::size_t i = 0; i < route.samples.size(); ++i) {
    const FlexSample& s = route.samples[i];
    const gp_Vec out = out_of(s.point);
    highest = std::max(highest, out.Magnitude() - 15);
    const double facing = gp_Vec(s.down).Dot(out.Normalized());
    if (s.at < middle - 21) {
      before = std::min(before, -facing);
    } else if (s.at > middle + 21) {
      after = std::min(after, facing);
    }
    if (s.node == 1) {
      at_node = std::abs(facing);
    }
    // The section's own turn about the route from the point before: its
    // `down` carried along the turn of the tangent alone, against its own.
    if (i > 0) {
      const FlexSample& b = route.samples[i - 1];
      const double step = s.at - b.at;
      gp_Vec carried(b.down);
      const gp_Vec axis = gp_Vec(b.tangent).Crossed(gp_Vec(s.tangent));
      if (axis.Magnitude() > 1e-12) {
        carried.Rotate(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(axis)), gp_Vec(b.tangent).Angle(gp_Vec(s.tangent)));
      }
      const double turn = std::abs(carried.AngleWithRef(gp_Vec(s.down), gp_Vec(s.tangent)));
      steepest = std::max(steepest, step > 1e-9 ? turn / step : 0);
    }
  }
  std::printf("half a turn round a drum: lifted up to %.4f, the section turns about the route up to %.4f rad/mm; "
              "down to the drum by %.6f before, away by %.6f after, %.4f at the node\n",
              highest, steepest, before, after, at_node);
  // Lifted by no more than the turned section's reach (its half diagonal),
  // within twice the deflection of the triangles it is kept off.
  CHECK(highest <= std::hypot(10.0, 1.0) / 2 + 2 * 0.1 + 0.05);
  // The turn spread evenly over its length about the node: no step. Half a
  // turn over 4 x 10 mm turns at most 1.5 pi / 40 = 0.118 rad/mm, the
  // drum's own normal about a route on it at most 1 / (2 x 15.5) = 0.032
  // more; a fifth of that over it.
  CHECK(steepest <= 0.2);
  // Flat on the drum before the turn, the other side down after it, on
  // its edge at the node.
  CHECK(before > 0.95);
  CHECK(after > 0.95);
  CHECK(at_node < 0.1);
  // Standing on its edge round the drum it bends over its edge, which a
  // flat cable without a radius over its edge does not: that warning
  // alone, about the turn, none about its flat side.
  CHECK(problem_of(route, FlexProblemKind::BendTooTight) == nullptr);
  const FlexProblem* edgewise = problem_of(route, FlexProblemKind::EdgeBendTooTight);
  CHECK(edgewise != nullptr);
  if (edgewise != nullptr) {
    for (const FlexProblem::Place& place : edgewise->places) {
      CHECK(place.from >= middle - 21 && place.to <= middle + 21);
    }
  }
  check_not_long("half a turn round a drum", route, 2 * std::hypot(15.5 * kPi / 2, 12.0));
  const ShapePtr solid = build_solid("F4", route, section);
  const TopoDS_Shape& shape = solid->occt();
  BRepMesh_IncrementalMesh(shape, 0.05, false, 0.1);
  double farthest = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(TopoDS::Face(f.Current()), location);
    if (mesh.IsNull()) {
      continue;
    }
    for (int i = 1; i <= mesh->NbNodes(); ++i) {
      const gp_Pnt p = mesh->Node(i).Transformed(location.Transformation());
      farthest = std::max(farthest, std::hypot(p.X() - 40, p.Y() + 40) - 15);
    }
  }
  const bool valid = BRepCheck_Analyzer(shape).IsValid();
  std::printf("half a turn round a drum: solid valid %d, volume %.3f of %.3f, farthest %.4f off the drum\n",
              valid ? 1 : 0, volume_of(shape), 10 * route.length, farthest);
  // The solid as the route: whole, its section's area along it, no part
  // farther off the drum than the section standing on its edge reaches
  // (the mesh's deflection beside).
  CHECK(valid);
  CHECK(std::abs(volume_of(shape) - 10 * route.length) <= 1e-2 * 10 * route.length);
  CHECK(farthest <= highest + std::hypot(10.0, 1.0) / 2 + 0.05);
}

// A sample of a hand-made route: its point, way and `down` (square to it).
FlexSample sample_at(const gp_Pnt& point, const gp_Vec& tangent, const gp_Vec& toward) {
  FlexSample sample;
  sample.point = point;
  sample.tangent = gp_Dir(tangent);
  const gp_Vec t(sample.tangent);
  sample.down = gp_Dir(toward - t * toward.Dot(t));
  return sample;
}

void test_turned_back() {
  // The owner's case (mitcad#1713): a round hose 5 mm across from a node on
  // the end edge of a 20 x 20 profile, 500 mm long, along its side; a free
  // node added at the route's own start, 2.5 mm off the end face where the
  // hose's centre is, after the first node. The route ran 0.15 mm out of
  // that point and back to it, and the item was lost: "the profile could
  // not be swept along the path". Its solid now: whole, its section's area
  // along the route.
  const TopoDS_Shape profile = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, 0), gp_Pnt(10, 10, 500)).Shape();
  const FlexRouter router(std::vector<TopoDS_Shape>{profile});
  FlexNode on_edge;
  on_edge.point = gp_Pnt(10, -5.89108148228172, 500);
  on_edge.body = 0;
  const FlexNode added = in_space(gp_Pnt(10, -5.89108148228172, 502.5));
  const FlexNode on_side = on_face(gp_Pnt(10, 5.83341744909643, 370.3507569222421),
                                   face_at(profile, gp_Pnt(10, 0, 250)));
  const FlexRoute route = router.route({on_edge, added, on_side}, hose(5), bend(4));
  print_route("turned back at the start", route);
  CHECK(route.samples.size() >= 2);
  if (route.samples.size() >= 2) {
    check_solid("turned back at the start, the solid",
                timed("turned back at the start", [&] { return build_solid("F2", route, hose(5)); }),
                kPi * 2.5 * 2.5 * route.length, 3e-2);
  }

  // The same turn made by hand, whatever the route makes of it: from the
  // start 0.15 mm out and back to the start, then 60 mm straight down. The
  // solid leaves the turn out: whole, 60 mm of the section.
  const gp_Pnt first(10, -5.89108148228172, 502.5);
  const gp_Pnt out(first.X() - 0.1416, first.Y() - 0.0301, first.Z() + 0.0063);
  const gp_Vec x(1, 0, 0);
  std::vector<FlexSample> samples{sample_at(first, gp_Vec(first, out), gp_Vec(0, 0, 1)),
                                  sample_at(out, gp_Vec(out, first), gp_Vec(0, 0, 1)),
                                  sample_at(first, gp_Vec(0, 0, -1), -x)};
  for (int k = 1; k <= 200; ++k) {
    samples.push_back(sample_at(gp_Pnt(first.X(), first.Y(), first.Z() - 0.3 * k), gp_Vec(0, 0, -1), -x));
  }
  double at = 0;
  for (std::size_t i = 1; i < samples.size(); ++i) {
    at += samples[i].point.Distance(samples[i - 1].point);
    samples[i].at = at;
  }
  // (The path as the route's points joined by lines, the turn in it.)
  Path path;
  for (std::size_t i = 1; i < samples.size(); ++i) {
    if (samples[i].point.Distance(samples[i - 1].point) < 1e-9) {
      continue;
    }
    ModelCurve line;
    line.kind = ModelCurveKind::Line;
    line.start = samples[i - 1].point;
    line.end = samples[i].point;
    path.push_back(PathCurve{"route", line});
  }
  check_solid("turned back by hand, the solid",
              timed("turned back by hand", [&] { return build_solid("F2", path, samples, hose(5)); }),
              kPi * 2.5 * 2.5 * 60.0, 1e-2);
}

void test_doubled_back() {
  // A route that runs 60 mm down, steps 3 mm aside and runs 60 mm back up
  // beside itself (a hose 5 across: a turn no section is swept round).
  // Without its turns each point of the way back took away one of the way
  // down, and the solid was a stub of a few millimetres (mitcad#1713: a
  // locked hanging item showed only a small part of its body). Now the
  // whole route in straight pieces: at least the section's area along it.
  const gp_Vec x(1, 0, 0);
  std::vector<FlexSample> samples;
  for (int k = 0; k <= 200; ++k) {
    samples.push_back(sample_at(gp_Pnt(0, 0, 100 - 0.3 * k), gp_Vec(0, 0, -1), x));
  }
  for (int k = 0; k <= 200; ++k) {
    samples.push_back(sample_at(gp_Pnt(3, 0, 40 + 0.3 * k), gp_Vec(0, 0, 1), x));
  }
  double at = 0;
  for (std::size_t i = 1; i < samples.size(); ++i) {
    at += samples[i].point.Distance(samples[i - 1].point);
    samples[i].at = at;
  }
  Path path;
  for (std::size_t i = 1; i < samples.size(); ++i) {
    ModelCurve line;
    line.kind = ModelCurveKind::Line;
    line.start = samples[i - 1].point;
    line.end = samples[i].point;
    path.push_back(PathCurve{"route", line});
  }
  const ShapePtr solid = timed("doubled back", [&] { return build_solid("F2", path, samples, hose(5)); });
  const double volume = volume_of(solid->occt());
  const double whole = kPi * 2.5 * 2.5 * at;
  std::printf("doubled back: the route %.3f long, its solid %.3f of %.3f\n", at, volume, whole);
  CHECK(std::abs(at - 123.0) < 1e-6);
  CHECK(volume >= 0.98 * whole);
}

void test_into_a_slot_mouth() {
  // The owner's case (mitcad#1713): a wire 5 mm across, bend radius 4, along
  // the top of a 20 x 20 profile with a T-slot, a node moved onto the far
  // wall of the slot's mouth (5.2 mm wide, the wire fits). The shortest way
  // along the surfaces runs down the near wall, round the slot's cavity and
  // up the far wall; the route followed it into the cavity, a dip below the
  // node held there by the pull to the surfaces, and in the pointer's
  // quicker route a knot bent to 1.4 mm. Now straight across the mouth: never
  // below the mouth into the cavity, the full route within its bend radius.
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(0, -10, 0), gp_Pnt(200, 10, 20)).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-1, -2.6, 15), gp_Pnt(201, 2.6, 21)).Shape()).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-1, -6, 8), gp_Pnt(201, 6, 15)).Shape()).Shape();
  const FlexRouter router(std::vector<TopoDS_Shape>{body});
  const TopoDS_Face top = face_at(body, gp_Pnt(100, -6.3, 20));
  const TopoDS_Face far_wall = face_at(body, gp_Pnt(100, 2.6, 17.5));
  const std::vector<FlexNode> nodes{on_face(gp_Pnt(40, -6.3, 20), top), on_face(gp_Pnt(120, 2.6, 18), far_wall),
                                    on_face(gp_Pnt(180, -6.3, 20), top)};
  for (const bool quick : {false, true}) {
    const char* what = quick ? "into a slot's mouth, quick" : "into a slot's mouth";
    const FlexRoute route = router.route(nodes, hose(5), bend(4), quick);
    print_route(what, route);
    CHECK(route.samples.size() >= 2);
    double lowest = 1e100;
    for (const FlexSample& sample : route.samples) {
      lowest = std::min(lowest, sample.point.Z());
    }
    std::printf("%s: lowest %.4f (the cavity below 15), tightest bend %.3f\n", what, lowest, tightest_bend(route));
    CHECK(lowest >= 15.0);
    CHECK(problem_of(route, FlexProblemKind::PassesThrough) == nullptr);
    if (!quick) {
      CHECK(problem_of(route, FlexProblemKind::BendTooTight) == nullptr);
    }
    check_not_long(what, route);
  }
}

} // namespace
} // namespace test

int main() {
  // (Each line as it comes: a test that is stopped has said how far it got.)
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  test::guarded("over the edge", test::test_over_the_edge);
  test::guarded("opposite faces", test::test_opposite_faces);
  test::guarded("through the air", test::test_through_the_air);
  test::guarded("along a cylinder", test::test_along_a_cylinder);
  test::guarded("flat cable", test::test_flat_cable);
  test::guarded("narrow slot", test::test_narrow_slot);
  test::guarded("solids", test::test_solids);
  test::guarded("own run", test::test_own_run);
  test::guarded("edgewise", test::test_edgewise);
  test::guarded("ports", test::test_ports);
  test::guarded("a wire off a plane", test::test_wire_off_a_plane);
  test::guarded("a wire over an edge", test::test_wire_over_an_edge);
  test::guarded("a hose over a top", test::test_hose_over_a_top);
  test::guarded("onto a swept wire", test::test_onto_a_swept_wire);
  test::guarded("the ui design", test::test_the_ui_design);
  test::guarded("turned over its edge", test::test_turned_over_its_edge);
  test::guarded("half a turn round a drum", test::test_half_turn_round_a_drum);
  test::guarded("turned back", test::test_turned_back);
  test::guarded("doubled back", test::test_doubled_back);
  test::guarded("into a slot's mouth", test::test_into_a_slot_mouth);
  if (test::failures == 0) {
    std::puts("test_flex_route: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
