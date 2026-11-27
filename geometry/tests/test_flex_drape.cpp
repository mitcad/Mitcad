// SPDX-License-Identifier: MIT
// Flex drape (mitcad#1713): a Flex item let hang (flex_drape.hpp).
//
// - A hose 10 thick (bend radius 20), 300 long, pinned at both ends 200
//   apart either side of a lying cylinder of radius 30, let fall from above
//   it: it comes to rest draped over the cylinder, touching it, nowhere
//   nearer its axis than the cylinder's radius and the hose's, its ends
//   where they were pinned and its length within 1 % of 300.
// - A wire 2 thick pinned at one end only, let go lying level in the air:
//   it hangs straight down from the pinned end, its free end its length
//   below it.
// - A wire pinned at its middle point only: that point stays exactly where
//   it was, the two halves hang down from it.
// - Two items 300 long hung between pins 200 apart in the air, a limp wire
//   (bend radius 5) and a stiff hose (bend radius 90): the wire's bottom
//   bends about as tightly as a hanging chain's (a catenary's radius there,
//   about 64 mm), the hose's wider, no tighter than its bend radius.
// - The draped hose's nodes (flex_drape_nodes): far fewer than the chain's
//   points, its ends among them, some on the cylinder's side face, and the
//   route the router makes through them within the tolerance of the chain
//   (half the hose's resting distance, 2.5 mm), as flex_drape_nodes reports
//   and as a route through them measured here says.
// - A hose 10 thick (bend radius 20), 300 long, pinned at both ends 200
//   apart in the air and let go lying level to one side of them, stepped
//   1/30 s at a time as the view steps it: it swings down through the
//   middle (past it by more than 5 mm: a swing, not a fall through syrup;
//   there within 0.6 s), and its points' speed (from where they are step
//   to step) falls below 20 mm/s within 2 s and stays there; it rests
//   (the rest detection: still for 0.3 s, its velocities zero) within 3 s
//   and stays where it rests in the steps after; a grab wakes it, and a
//   damping outside 0 to 1 is refused.
// - A wire 5 thick (bend radius 4) hanging straight up from an end pinned
//   on a block's top face, its next point on the face too: no node within
//   three points (5 mm) of the pinned end, and the route through the nodes
//   as the item makes it (the end where it was attached) does not turn
//   back, 97.5 long, its solid the section's area along it within 2 %.
// - A wire 5 thick (bend radius 4) whose chain lies on a block's top, runs
//   down a bore 4.3 across in it and back up, on over the edge and down
//   the side: no node in the bore or its mouth (where the item cannot
//   lie), and the route through the nodes does not turn back, stays over
//   the bore's mouth and is swept smooth, the section's area along it
//   within 2 %.
// - The work of a step is in proportion to the time it simulates, not to
//   how often it is asked: 1/30 s is 12 substeps, 1/60 s 6, two steps of
//   1/240 s three between them (the rest of a substep carried over); one
//   pass over the obstacles a substep, each point and segment's middle
//   asking the obstacles' mesh at most once a substep, and a chain far
//   from the obstacles asking only in its first substep (and for its
//   contacts at the end).
//
// Tolerances: the chain keeps the section's resting distance off the
// cylinder within the deflection of its triangles (flex_drape_deflection,
// 0.25 mm for it) and 0.1 mm of the constraints' own give; its length
// within 1 %; pinned points exactly (1e-9 mm); the straight hanging wire
// within 1 % of its length sideways.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/flex.hpp"
#include "mitcad/geometry/flex_drape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// A cylinder of radius 30 lying along y (from -60 to 60), its axis at
// z = 0, its faces named F1:f1, F1:f2, ...
ShapePtr lying_cylinder() {
  const TopoDS_Shape body = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -60, 0), gp_Dir(0, 1, 0)), 30, 120).Shape();
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"F1:f" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(body, faces);
}

// Points `segment` apart along the polyline through `corners`.
std::vector<gp_Pnt> along(const std::vector<gp_Pnt>& corners, double segment) {
  std::vector<gp_Pnt> out{corners.front()};
  double carried = 0.0;
  for (std::size_t k = 0; k + 1 < corners.size(); ++k) {
    const gp_Vec d(corners[k], corners[k + 1]);
    const double l = d.Magnitude();
    double at = segment - carried;
    while (at <= l + 1e-9) {
      out.push_back(corners[k].Translated(d * (at / l)));
      at += segment;
    }
    carried = l - (at - segment);
  }
  return out;
}

double chain_length(const std::vector<gp_Pnt>& points) {
  double length = 0.0;
  for (std::size_t i = 0; i + 1 < points.size(); ++i) {
    length += points[i].Distance(points[i + 1]);
  }
  return length;
}

// Lets the chain fall until it rests (at most `most` seconds, a second at
// a time); the last step.
FlexDrapeResult settle(const std::vector<ShapePtr>& obstacles, FlexDrapeSpec spec, double most = 30.0) {
  FlexDrapeResult result;
  double time = 0.0;
  do {
    spec.seconds = 1.0;
    result = flex_drape_step(obstacles, spec);
    spec.points = result.points;
    spec.velocities = result.velocities;
    spec.still = result.still;
    time += result.time;
  } while (!result.at_rest && time < most);
  std::printf("  rests %s after %.2f s\n", result.at_rest ? "yes" : "no", time);
  return result;
}

// The radius of the circle through the chain's points `k` either side of
// its lowest one.
double bottom_radius(const std::vector<gp_Pnt>& points, int k) {
  int low = 0;
  for (int i = 0; i < static_cast<int>(points.size()); ++i) {
    if (points[static_cast<std::size_t>(i)].Z() < points[static_cast<std::size_t>(low)].Z()) {
      low = i;
    }
  }
  low = std::clamp(low, k, static_cast<int>(points.size()) - 1 - k);
  const gp_Pnt& a = points[static_cast<std::size_t>(low - k)];
  const gp_Pnt& b = points[static_cast<std::size_t>(low)];
  const gp_Pnt& c = points[static_cast<std::size_t>(low + k)];
  const double ab = a.Distance(b);
  const double bc = b.Distance(c);
  const double ca = c.Distance(a);
  const double area2 = gp_Vec(a, b).Crossed(gp_Vec(a, c)).Magnitude();
  return area2 > 1e-12 ? ab * bc * ca / (2.0 * area2) : 1e100;
}

FlexDrapeSpec over_the_cylinder_spec() {
  FlexDrapeSpec spec;
  spec.resting = 5.0;
  spec.bend_radius = 20.0;
  spec.segment = flex_drape_segment(300.0, spec.resting, spec.bend_radius);
  // From an end, up over the cylinder and down to the other end: 300 long.
  const double rise = std::sqrt(150.0 * 150.0 - 100.0 * 100.0);
  spec.points = along({gp_Pnt(-100, 0, 40), gp_Pnt(0, 0, 40 + rise), gp_Pnt(100, 0, 40)}, spec.segment);
  spec.pinned.assign(spec.points.size(), 0);
  spec.pinned.front() = 1;
  spec.pinned.back() = 1;
  return spec;
}

FlexDrapeResult draped;
std::vector<ShapePtr> cylinder_obstacles;

void test_over_a_cylinder() {
  cylinder_obstacles = {lying_cylinder()};
  const FlexDrapeSpec spec = over_the_cylinder_spec();
  const double length = chain_length(spec.points);
  draped = settle(cylinder_obstacles, spec);
  CHECK(draped.at_rest);
  double nearest = 1e100;
  int touching = 0;
  for (std::size_t i = 0; i < draped.points.size(); ++i) {
    const gp_Pnt& p = draped.points[i];
    if (std::abs(p.Y()) < 55.0) {
      nearest = std::min(nearest, std::hypot(p.X(), p.Z()));
    }
    touching += draped.contacts[i].obstacle == 0 ? 1 : 0;
  }
  const double now = chain_length(draped.points);
  std::printf("over a cylinder: %d points, %d touching it, nearest %.4f from the axis, length %.3f of %.3f\n",
              static_cast<int>(draped.points.size()), touching, nearest, now, length);
  CHECK(touching >= 3);
  CHECK(nearest >= 35.0 - flex_drape_deflection(5.0) - 0.1);
  CHECK(std::abs(now - length) <= 0.01 * length);
  CHECK(draped.points.front().Distance(spec.points.front()) < 1e-9);
  CHECK(draped.points.back().Distance(spec.points.back()) < 1e-9);
  // Draped: its top, above the cylinder, on it (35 above the axis, within
  // the triangles' deflection), not in the air above it.
  double top = -1e100;
  for (const gp_Pnt& p : draped.points) {
    if (std::abs(p.X()) <= 10.0) {
      top = std::max(top, p.Z());
    }
  }
  CHECK(top < 35.0 + 1.0);
  CHECK(draped.normals.size() == draped.points.size());
}

void test_one_end_free() {
  FlexDrapeSpec spec;
  spec.resting = 1.0;
  spec.bend_radius = 5.0;
  spec.segment = flex_drape_segment(100.0, spec.resting, spec.bend_radius);
  spec.points = along({gp_Pnt(0, 0, 100), gp_Pnt(100, 0, 100)}, spec.segment);
  spec.pinned.assign(spec.points.size(), 0);
  spec.pinned.front() = 1;
  const double length = chain_length(spec.points);
  const FlexDrapeResult hung = settle({}, spec);
  double sideways = 0.0;
  for (const gp_Pnt& p : hung.points) {
    sideways = std::max(sideways, std::hypot(p.X(), p.Y()));
  }
  const gp_Pnt& end = hung.points.back();
  std::printf("one end free: the end at (%.3f, %.3f, %.3f), %.3f sideways at most\n", end.X(), end.Y(), end.Z(),
              sideways);
  CHECK(hung.at_rest);
  CHECK(sideways <= 0.01 * length);
  CHECK(std::abs(end.Z() - (100.0 - length)) <= 0.01 * length);
  CHECK(hung.points.front().Distance(gp_Pnt(0, 0, 100)) < 1e-9);
}

void test_pinned_middle() {
  FlexDrapeSpec spec;
  spec.resting = 1.0;
  spec.bend_radius = 5.0;
  spec.segment = flex_drape_segment(200.0, spec.resting, spec.bend_radius);
  spec.points = along({gp_Pnt(0, 0, 100), gp_Pnt(200, 0, 100)}, spec.segment);
  spec.pinned.assign(spec.points.size(), 0);
  const std::size_t middle = spec.points.size() / 2;
  spec.pinned[middle] = 1;
  const gp_Pnt held = spec.points[middle];
  const FlexDrapeResult hung = settle({}, spec);
  std::printf("pinned middle: the middle at (%.6f, %.6f, %.6f), the ends at z %.3f and %.3f\n", hung.points[middle].X(),
              hung.points[middle].Y(), hung.points[middle].Z(), hung.points.front().Z(), hung.points.back().Z());
  CHECK(hung.points[middle].Distance(held) < 1e-9);
  CHECK(hung.points.front().Z() < 100.0 - 50.0);
  CHECK(hung.points.back().Z() < 100.0 - 50.0);
}

// The radius of the bottom of an item 300 long hung between pins 200 apart.
double hung_bottom(double resting, double bend_radius) {
  FlexDrapeSpec spec;
  spec.resting = resting;
  spec.bend_radius = bend_radius;
  spec.segment = flex_drape_segment(300.0, resting, bend_radius);
  const double rise = std::sqrt(150.0 * 150.0 - 100.0 * 100.0);
  spec.points = along({gp_Pnt(-100, 0, 0), gp_Pnt(0, 0, -rise), gp_Pnt(100, 0, 0)}, spec.segment);
  spec.pinned.assign(spec.points.size(), 0);
  spec.pinned.front() = 1;
  spec.pinned.back() = 1;
  const FlexDrapeResult hung = settle({}, spec);
  CHECK(hung.at_rest);
  const int k = std::max(1, static_cast<int>(std::lround(15.0 / spec.segment)));
  return bottom_radius(hung.points, k);
}

void test_stiffer_hangs_wider() {
  const double limp = hung_bottom(1.0, 5.0);
  const double stiff = hung_bottom(5.0, 90.0);
  std::printf("stiffer hangs wider: the bottom's radius %.2f limp, %.2f stiff\n", limp, stiff);
  CHECK(limp > 45.0 && limp < 85.0);
  CHECK(stiff > limp * 1.2);
  CHECK(stiff > 90.0 * 0.9);
}

void test_nodes_of_the_drape() {
  if (draped.points.empty()) {
    CHECK(false);
    return;
  }
  FlexDrapeNodesSpec spec;
  spec.points = draped.points;
  spec.contacts = draped.contacts;
  spec.keep = {0, static_cast<int>(draped.points.size()) - 1};
  spec.resting = 5.0;
  spec.half_width = 5.0;
  spec.reach = 5.0;
  spec.bend_radius = 20.0;
  spec.bend_radius_edge = 20.0;
  spec.tolerance = 2.5;
  const FlexDrapeNodes nodes = flex_drape_nodes(cylinder_obstacles, spec);
  int on_faces = 0;
  for (const int i : nodes.nodes) {
    on_faces += draped.contacts[static_cast<std::size_t>(i)].face.empty() ? 0 : 1;
  }
  std::printf("nodes of the drape: %d of %d points, %d on faces, the route %.4f from the chain at most\n",
              static_cast<int>(nodes.nodes.size()), static_cast<int>(draped.points.size()), on_faces, nodes.deviation);
  CHECK(!nodes.nodes.empty() && nodes.nodes.front() == 0);
  CHECK(nodes.nodes.back() == static_cast<int>(draped.points.size()) - 1);
  CHECK(static_cast<int>(nodes.nodes.size()) * 2 < static_cast<int>(draped.points.size()));
  CHECK(on_faces >= 2);
  CHECK(nodes.deviation >= 0.0 && nodes.deviation <= spec.tolerance);
  // The route through them, measured here: every point of the chain
  // within the tolerance of the route's polyline.
  FlexRouteSpec route;
  route.feature = "F9";
  route.resting = 5.0;
  route.bend_radius = 20.0;
  route.bend_radius_edge = 20.0;
  for (const int i : nodes.nodes) {
    const FlexDrapeContact& c = draped.contacts[static_cast<std::size_t>(i)];
    FlexNode node;
    node.point = draped.points[static_cast<std::size_t>(i)];
    if (!c.face.empty() && i != 0 && i != static_cast<int>(draped.points.size()) - 1) {
      node.obstacle = c.obstacle;
      node.face = c.face;
      node.normal = c.normal;
      node.point = node.point.Translated(-gp_Vec(c.normal) * 5.0);
    }
    route.nodes.push_back(node);
  }
  const FlexRoute routed = flex_route(cylinder_obstacles, route);
  double farthest = 0.0;
  for (const gp_Pnt& p : draped.points) {
    double best = 1e100;
    for (std::size_t k = 0; k + 1 < routed.samples.size(); ++k) {
      const gp_Pnt& a = routed.samples[k].point;
      const gp_Vec ab(a, routed.samples[k + 1].point);
      const double l2 = ab.SquareMagnitude();
      const double t = l2 > 1e-18 ? std::clamp(gp_Vec(a, p).Dot(ab) / l2, 0.0, 1.0) : 0.0;
      best = std::min(best, p.Distance(a.Translated(ab * t)));
    }
    farthest = std::max(farthest, best);
  }
  std::printf("nodes of the drape: measured %.4f from the route at most\n", farthest);
  CHECK(farthest <= spec.tolerance + 1e-6);
}

// The hose of the swing: 300 long, pinned at both ends 200 apart, lying
// level to one side of them (+y).
FlexDrapeSpec swing_spec() {
  FlexDrapeSpec spec;
  spec.resting = 5.0;
  spec.bend_radius = 20.0;
  spec.segment = flex_drape_segment(300.0, spec.resting, spec.bend_radius);
  const double aside = std::sqrt(150.0 * 150.0 - 100.0 * 100.0);
  spec.points = along({gp_Pnt(-100, 0, 0), gp_Pnt(0, aside, 0), gp_Pnt(100, 0, 0)}, spec.segment);
  spec.pinned.assign(spec.points.size(), 0);
  spec.pinned.front() = 1;
  spec.pinned.back() = 1;
  return spec;
}

void test_settles_soon() {
  FlexDrapeSpec spec = swing_spec();
  const std::size_t middle = spec.points.size() / 2;
  constexpr double kStep = 1.0 / 30.0;
  double time = 0.0;
  double through = -1.0; // when the middle first swung past the pins' line
  double farthest = 0.0; // how far past it (-y)
  double lastFast = 0.0; // the last step with a point faster than 20 mm/s
  double rested = -1.0;
  std::vector<gp_Pnt> restedAt;
  double drift = 0.0; // how far a point moved after it rested
  FlexDrapeResult result;
  for (int k = 0; k < 120; ++k) {
    spec.seconds = kStep;
    result = flex_drape_step({}, spec);
    time += result.time;
    double fastest = 0.0;
    for (std::size_t i = 0; i < result.points.size(); ++i) {
      fastest = std::max(fastest, result.points[i].Distance(spec.points[i]) / result.time);
    }
    if (fastest >= 20.0) {
      lastFast = time;
    }
    const double y = result.points[middle].Y();
    if (y < 0.0 && through < 0.0) {
      through = time;
    }
    farthest = std::max(farthest, -y);
    if (result.at_rest && rested < 0.0) {
      rested = time;
      restedAt = result.points;
      CHECK(result.still >= kDrapeRestSeconds - 1e-9);
      for (const gp_Vec& v : result.velocities) {
        CHECK(v.Magnitude() == 0.0);
      }
    } else if (rested >= 0.0) {
      for (std::size_t i = 0; i < result.points.size(); ++i) {
        drift = std::max(drift, result.points[i].Distance(restedAt[i]));
      }
    }
    spec.points = result.points;
    spec.velocities = result.velocities;
    spec.still = result.still;
  }
  std::printf("settles soon: through the middle after %.2f s, %.2f past it; slower than 20 mm/s from %.2f s; rests "
              "after %.2f s, moves %.4f after\n",
              through, farthest, lastFast, rested, drift);
  CHECK(through >= 0.0 && through <= 0.6);
  CHECK(farthest > 5.0);
  CHECK(lastFast <= 2.0);
  CHECK(rested >= 0.0 && rested <= 3.0);
  CHECK(drift < 0.5);

  // A grab wakes it.
  FlexDrapeSpec pulled = spec;
  pulled.grab = static_cast<int>(middle);
  pulled.target = spec.points[middle].Translated(gp_Vec(0, 0, 20));
  const FlexDrapeResult woken = flex_drape_step({}, pulled);
  CHECK(!woken.at_rest);
  CHECK(woken.still == 0.0);

  // A damping outside 0 to 1 is no damping.
  FlexDrapeSpec wrong = spec;
  wrong.damping = 1.5;
  bool refused = false;
  try {
    flex_drape_step({}, wrong);
  } catch (const std::invalid_argument&) {
    refused = true;
  }
  CHECK(refused);
}

// A block 200 x 20 x 20 (its top at z = 20), its faces named F2:f1, ...;
// the name of its top face in `top`.
ShapePtr flat_block(std::string& top) {
  const TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(0, -10, 0), gp_Pnt(200, 10, 20)).Shape();
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const std::string name = "F2:f" + std::to_string(++i);
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f.Current(), props);
    if (std::abs(props.CentreOfMass().Z() - 20.0) < 1e-9) {
      top = name;
    }
    faces.push_back({f.Current(), {name}});
  }
  return std::make_shared<Shape>(body, faces);
}

void test_nodes_beside_a_pinned_end() {
  // The owner's case (mitcad#1713): a wire 5 thick (bend radius 4) pinned
  // on a block's top face, where its route was attached, hangs straight up
  // from it; its next point, 2 mm along, lies on the face too. That point
  // was a node beside the pinned end, which the item's route lifts off the
  // face as it was: the route ran down to the one and back up to the
  // other, and most of the item's solid was lost on Keep Shape. No node
  // now within the item's thickness and bend radius of the end; the route
  // as the item makes it (its end on the face where it was attached) does
  // not turn back, and its solid is whole.
  std::string top;
  const std::vector<ShapePtr> obstacles{flat_block(top)};
  CHECK(!top.empty());
  FlexDrapeNodesSpec spec;
  for (int k = 0; k <= 49; ++k) {
    spec.points.emplace_back(100, 0, 120 - 2.0 * k); // down to z = 22
  }
  spec.points.emplace_back(100, 0, 20); // the pinned end, on the face
  const int last = static_cast<int>(spec.points.size()) - 1;
  for (const gp_Pnt& p : spec.points) {
    FlexDrapeContact contact;
    if (p.Z() - 20.0 <= 2.5 + 0.02) {
      contact.obstacle = 0;
      contact.face = top;
      contact.normal = gp_Dir(0, 0, 1);
    }
    spec.contacts.push_back(contact);
  }
  spec.keep = {0, last};
  spec.resting = 2.5;
  spec.half_width = 2.5;
  spec.reach = 2.5;
  spec.bend_radius = 4.0;
  spec.bend_radius_edge = 4.0;
  spec.tolerance = 1.25;
  const FlexDrapeNodes nodes = flex_drape_nodes(obstacles, spec);
  std::printf("nodes beside a pinned end: %d nodes, the last two at points %d and %d, deviation %.4f\n",
              static_cast<int>(nodes.nodes.size()), nodes.nodes.size() >= 2 ? nodes.nodes[nodes.nodes.size() - 2] : -1,
              nodes.nodes.empty() ? -1 : nodes.nodes.back(), nodes.deviation);
  CHECK(!nodes.nodes.empty() && nodes.nodes.front() == 0 && nodes.nodes.back() == last);
  // None within 5 mm (three points) of the pinned end.
  for (const int i : nodes.nodes) {
    CHECK(i == last || i <= last - 3);
  }
  CHECK(nodes.deviation >= 0.0 && nodes.deviation <= spec.tolerance);

  // The route the item makes through them: the pinned end on the face
  // where it was attached, the others as the lock keeps them.
  FlexRouteSpec route;
  route.feature = "F9";
  route.resting = 2.5;
  route.bend_radius = 4.0;
  route.bend_radius_edge = 4.0;
  for (const int i : nodes.nodes) {
    const FlexDrapeContact& c = spec.contacts[static_cast<std::size_t>(i)];
    FlexNode node;
    node.point = spec.points[static_cast<std::size_t>(i)];
    if (!c.face.empty()) {
      node.obstacle = c.obstacle;
      node.face = c.face;
      node.normal = c.normal;
      if (i != last) {
        node.point = node.point.Translated(-gp_Vec(c.normal) * 2.5);
      }
    }
    route.nodes.push_back(node);
  }
  const FlexRoute routed = flex_route(obstacles, route);
  CHECK(routed.samples.size() >= 2);
  if (routed.samples.size() < 2) {
    return;
  }
  // No turn back: each step of the samples on along the one before.
  int back = 0;
  gp_Vec before;
  for (std::size_t k = 0; k + 1 < routed.samples.size(); ++k) {
    const gp_Vec step(routed.samples[k].point, routed.samples[k + 1].point);
    if (step.Magnitude() < 1e-7) {
      continue;
    }
    if (before.Magnitude() > 0.0 && before.Dot(step) <= 0.0) {
      ++back;
    }
    before = step;
  }
  const double length = routed.samples.back().along;
  FlexSolidSpec solid;
  solid.feature = "F9";
  for (std::size_t k = 0; k < routed.curve.size(); ++k) {
    solid.path.push_back(PathCurve{"F9:route" + std::to_string(k + 1), routed.curve[k]});
  }
  solid.samples = routed.samples;
  solid.round = true;
  solid.rectangular = false;
  solid.width = 5.0;
  const ShapePtr made = flex_solid(solid);
  GProp_GProps props;
  BRepGProp::VolumeProperties(made->occt(), props);
  const double expected = kPi * 2.5 * 2.5 * length;
  std::printf("nodes beside a pinned end: the route %.3f long, %d turn(s) back, its solid %.3f of %.3f\n", length, back,
              props.Mass(), expected);
  CHECK(back == 0);
  CHECK(std::abs(length - 97.5) <= 0.5);
  CHECK(std::abs(props.Mass() - expected) <= 0.02 * expected);
}

// A block 40 x 40 x 20 (its top at z = 20) with a bore 4.3 across through
// it, upright at x = y = 0, its faces named F3:f1, ...
ShapePtr bored_block() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, 0), gp_Pnt(20, 20, 20)).Shape();
  const TopoDS_Shape bore = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 2.15, 22).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(box, bore).Shape();
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"F3:f" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(body, faces);
}

void test_nodes_past_a_narrow_bore() {
  // The owner's case (mitcad#1713): a wire 5 thick (bend radius 4) pinned
  // beside the mouth of a bore 4.3 across hung down into the bore and out
  // again; a node kept in the bore made the item's route dip into it and
  // come back, 0.3 mm round, and the solid along that was the cable in a
  // few short pieces. Here its chain lies on the block's top, runs down
  // the bore 10 mm and back up, on along the top, over the edge and down
  // the side into the air. No node lies in the bore or its mouth now; the
  // route the item makes through the nodes does not turn back, and its
  // solid is the section's area along it within 2 %.
  const std::vector<ShapePtr> obstacles{bored_block()};
  std::vector<gp_Pnt> corners{gp_Pnt(-8, 0, 22.5),   gp_Pnt(-0.4, 0, 22.5), gp_Pnt(-0.4, 0, 10),
                              gp_Pnt(0.4, 0, 10),    gp_Pnt(0.4, 0, 22.5),  gp_Pnt(20, 0, 22.5)};
  for (const double degrees : {67.5, 45.0, 22.5}) {
    const double a = degrees * kPi / 180.0;
    corners.emplace_back(20 + 2.5 * std::cos(a), 0, 20 + 2.5 * std::sin(a));
  }
  corners.emplace_back(22.5, 0, 20);
  corners.emplace_back(22.5, 0, -30);
  // What each point lies on, as the hanging item's step gives it (no time
  // simulated, every point held).
  FlexDrapeSpec chain;
  chain.points = along(corners, 2.0);
  chain.pinned.assign(chain.points.size(), 1);
  chain.segment = 2.0;
  chain.resting = 2.5;
  chain.bend_radius = 4.0;
  chain.seconds = 0.0;
  const FlexDrapeResult lying = flex_drape_step(obstacles, chain);
  CHECK(lying.substeps == 0 && lying.contacts.size() == chain.points.size());

  FlexDrapeNodesSpec spec;
  spec.points = chain.points;
  spec.contacts = lying.contacts;
  const int last = static_cast<int>(spec.points.size()) - 1;
  spec.keep = {0, last};
  spec.resting = 2.5;
  spec.half_width = 2.5;
  spec.reach = 2.5;
  spec.bend_radius = 4.0;
  spec.bend_radius_edge = 4.0;
  spec.tolerance = 1.25;
  const FlexDrapeNodes nodes = flex_drape_nodes(obstacles, spec);
  int in_bore = 0;
  for (const int i : nodes.nodes) {
    const gp_Pnt& p = spec.points[static_cast<std::size_t>(i)];
    if (std::abs(p.X()) < 2.15 && p.Z() < 21.0) {
      ++in_bore;
    }
  }
  std::printf("nodes past a narrow bore: %d nodes of %d points, %d in the bore, deviation %.3f\n",
              static_cast<int>(nodes.nodes.size()), last + 1, in_bore, nodes.deviation);
  CHECK(!nodes.nodes.empty() && nodes.nodes.front() == 0 && nodes.nodes.back() == last);
  CHECK(in_bore == 0);

  // The route the item makes through them, the nodes on faces as the lock
  // keeps them (at the surface below the centre line).
  FlexRouteSpec route;
  route.feature = "F9";
  route.resting = 2.5;
  route.bend_radius = 4.0;
  route.bend_radius_edge = 4.0;
  for (const int i : nodes.nodes) {
    const FlexDrapeContact& c = spec.contacts[static_cast<std::size_t>(i)];
    FlexNode node;
    node.point = spec.points[static_cast<std::size_t>(i)];
    if (c.obstacle >= 0 && !c.face.empty()) {
      node.obstacle = c.obstacle;
      node.face = c.face;
      node.normal = c.normal;
      node.point = node.point.Translated(-gp_Vec(c.normal) * 2.5);
    }
    route.nodes.push_back(node);
  }
  const FlexRoute routed = flex_route(obstacles, route);
  CHECK(routed.samples.size() >= 2);
  if (routed.samples.size() < 2) {
    return;
  }
  int back = 0;
  gp_Vec before;
  for (std::size_t k = 0; k + 1 < routed.samples.size(); ++k) {
    const gp_Vec step(routed.samples[k].point, routed.samples[k + 1].point);
    if (step.Magnitude() < 1e-7) {
      continue;
    }
    if (before.Magnitude() > 0.0 && before.Dot(step) <= 0.0) {
      ++back;
    }
    before = step;
  }
  double lowest = 1e100;
  for (const FlexSample& sample : routed.samples) {
    if (std::abs(sample.point.X()) < 2.15) {
      lowest = std::min(lowest, sample.point.Z());
    }
  }
  const double length = routed.samples.back().along;
  FlexSolidSpec solid;
  solid.feature = "F9";
  for (std::size_t k = 0; k < routed.curve.size(); ++k) {
    solid.path.push_back(PathCurve{"F9:route" + std::to_string(k + 1), routed.curve[k]});
  }
  solid.samples = routed.samples;
  solid.round = true;
  solid.rectangular = false;
  solid.width = 5.0;
  const ShapePtr made = flex_solid(solid);
  GProp_GProps props;
  BRepGProp::VolumeProperties(made->occt(), props, 1.0e-6);
  const double expected = kPi * 2.5 * 2.5 * length;
  std::printf("nodes past a narrow bore: the route %.3f long, %d turn(s) back, lowest over the bore %.3f, its "
              "solid %.3f of %.3f\n",
              length, back, lowest, props.Mass(), expected);
  CHECK(back == 0);
  // Over the bore's mouth, not down in it.
  CHECK(lowest >= 20.0);
  // Swept smooth (not in straight pieces, which carry a note), whole.
  CHECK(made->notes().empty());
  CHECK(std::abs(props.Mass() - expected) <= 0.02 * expected);
}

void test_work_per_step() {
  // In the air: the substeps by the time, the rest carried over.
  FlexDrapeSpec spec = swing_spec();
  spec.seconds = 1.0 / 30.0;
  FlexDrapeResult step = flex_drape_step({}, spec);
  std::printf("work per step: 1/30 s in %d substeps, %d passes, %.6f s\n", step.substeps, step.collisions, step.time);
  CHECK(step.substeps == 12 && step.collisions == 12 && step.queries == 0);
  CHECK(std::abs(step.time - 1.0 / 30.0) < 1e-12 && step.carry < 1e-12);
  spec.seconds = 1.0 / 60.0;
  step = flex_drape_step({}, spec);
  CHECK(step.substeps == 6 && step.collisions == 6);
  spec.seconds = 1.0 / 240.0;
  const FlexDrapeResult first = flex_drape_step({}, spec);
  spec.points = first.points;
  spec.velocities = first.velocities;
  spec.still = first.still;
  spec.carry = first.carry;
  const FlexDrapeResult second = flex_drape_step({}, spec);
  std::printf("work per step: two of 1/240 s in %d and %d substeps, %.6f s carried after\n", first.substeps,
              second.substeps, second.carry);
  CHECK(first.substeps == 1 && std::abs(first.carry - (1.0 / 240.0 - kDrapeSubstep)) < 1e-12);
  CHECK(second.substeps == 2 && second.carry < 1e-9);
  bool refused = false;
  try {
    spec.carry = -1.0;
    flex_drape_step({}, spec);
  } catch (const std::invalid_argument&) {
    refused = true;
  }
  CHECK(refused);

  // Over the cylinder: one pass over the obstacles a substep, each probe
  // (a point or a segment's middle) asking at most once in it.
  if (cylinder_obstacles.empty()) {
    cylinder_obstacles = {lying_cylinder()};
  }
  FlexDrapeSpec over = over_the_cylinder_spec();
  over.seconds = 1.0 / 30.0;
  const int probes = 2 * static_cast<int>(over.points.size()) - 1;
  step = flex_drape_step(cylinder_obstacles, over);
  std::printf("work per step: over the cylinder %d substeps, %d passes, %d questions of %d probes\n", step.substeps,
              step.collisions, step.queries, probes);
  CHECK(step.substeps == 12 && step.collisions == 12);
  CHECK(step.queries <= (step.substeps + 1) * probes);

  // Far above it: the obstacles asked in the first substep and for the
  // contacts only, nothing being near.
  FlexDrapeSpec distant = over_the_cylinder_spec();
  for (gp_Pnt& p : distant.points) {
    p.Translate(gp_Vec(0, 0, 1000));
  }
  distant.seconds = 1.0 / 30.0;
  step = flex_drape_step(cylinder_obstacles, distant);
  std::printf("work per step: far from the cylinder %d questions of %d probes\n", step.queries, probes);
  CHECK(step.substeps == 12 && step.queries <= 2 * probes);
  CHECK(flex_drape_deflection(5.0) == 0.25 && flex_drape_deflection(0.5) == 0.1 && flex_drape_deflection(50.0) == 0.5);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(flex_drape) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  test::guarded("over a cylinder", test::test_over_a_cylinder);
  test::guarded("one end free", test::test_one_end_free);
  test::guarded("pinned middle", test::test_pinned_middle);
  test::guarded("stiffer hangs wider", test::test_stiffer_hangs_wider);
  test::guarded("nodes of the drape", test::test_nodes_of_the_drape);
  test::guarded("settles soon", test::test_settles_soon);
  test::guarded("nodes beside a pinned end", test::test_nodes_beside_a_pinned_end);
  test::guarded("nodes past a narrow bore", test::test_nodes_past_a_narrow_bore);
  test::guarded("work per step", test::test_work_per_step);
  if (test::failures == 0) {
    std::puts("test_flex_drape: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
