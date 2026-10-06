// SPDX-License-Identifier: MIT
// Fillets and chamfers by edge name, the names of their faces, and names
// that survive dimension changes and booleans.

#include <algorithm>
#include <cmath>
#include <utility>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>

#include "../src/face_select.hpp"
#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

ShapePtr block(double width = 60) { return extrude("F2", {rectangle(1, 0, 0, width, 40)}, 0, 20); }

// The vertical edge at corner (width, 0) and the top edge along y = 0.
std::string corner() { return edge(side("F2", 1, 0), side("F2", 1, 1)); }
std::string top_front() { return edge(end_cap("F2", 1), side("F2", 1, 0)); }

// True when the shape has a sharp vertical edge standing at (x, y).
bool sharp_vertical_edge_at(const Shape& shape, double x, double y) {
  for (int i = 0; i < shape.edge_count(); ++i) {
    const TopoDS_Edge& e = shape.edge(i);
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    if (near(a.X(), x) && near(a.Y(), y) && near(b.X(), x) && near(b.Y(), y) &&
        std::abs(a.Z() - b.Z()) > 1) {
      return true;
    }
  }
  return false;
}

void test_fillet_volume_and_names() {
  const double r = 3;
  const ShapePtr rounded = fillet("F5", *block(), {corner()}, r);
  CHECK(near(volume(*rounded), 60.0 * 40.0 * 20.0 - fillet_loss(r) * 20.0));
  CHECK(!sharp_vertical_edge_at(*rounded, 60, 0));
  const std::string face = "F5:fillet(" + corner() + ")";
  CHECK(faces_named(*rounded, face) == 1);
  // The neighbours keep their names; the fillet face meets them in new,
  // named edges.
  CHECK(faces_named(*rounded, side("F2", 1, 0)) == 1);
  CHECK(rounded->find_edges(edge(face, side("F2", 1, 0))).size() == 1);
  CHECK(rounded->find_edges(edge(face, end_cap("F2", 1))).size() == 1);
  CHECK(rounded->find_edges(corner()).empty());
  CHECK(edge_names_unique(*rounded));

  CHECK(throws_with([&] { fillet("F5", *block(), {edge(side("F2", 1, 0), side("F2", 1, 2))}, 1); },
                    "the body has no edge E{F2:side(c1[c4,c2])|F2:side(c3[c2,c4])}"));
  CHECK(throws_with([&] { fillet("F5", *block(), std::vector<std::string>{}, 1); }, "no edges"));
  CHECK(throws_with([&] { fillet("F5", *block(), {corner()}, 0); }, "radius must be greater"));
  CHECK(throws_with([&] { fillet("F5", *block(), {top_front()}, 30); }, "fillet failed"));
}

void test_fillet_corner_blends_are_named() {
  // Rounding all twelve edges blends each corner where three meet.
  const ShapePtr body = block();
  std::vector<std::string> all;
  for (int i = 0; i < body->edge_count(); ++i) {
    all.push_back(body->edge_name(i));
  }
  const ShapePtr rounded = fillet("F5", *body, all, 2);
  int fillets = 0;
  int corners = 0;
  for (int i = 0; i < rounded->face_count(); ++i) {
    for (const std::string& name : rounded->face_names(i)) {
      fillets += name.rfind("F5:fillet(", 0) == 0 ? 1 : 0;
      corners += name.rfind("F5:corner(V{", 0) == 0 ? 1 : 0;
    }
  }
  CHECK(fillets == 12);
  CHECK(corners == 8);
  CHECK(edge_names_unique(*rounded));
}

void test_fillet_follows_a_dimension_change() {
  const double r = 2;
  for (const double width : {60.0, 80.0}) {
    const ShapePtr rounded = fillet("F5", *block(width), {corner()}, r);
    CHECK(near(volume(*rounded), width * 40.0 * 20.0 - fillet_loss(r) * 20.0));
    CHECK(!sharp_vertical_edge_at(*rounded, width, 0));
    CHECK(sharp_vertical_edge_at(*rounded, 0, 0));
  }
}

void test_several_fillets_on_one_body() {
  // The second fillet rounds an edge the first one created.
  const ShapePtr first = fillet("F5", *block(), {corner()}, 3);
  const std::string new_edge = edge("F5:fillet(" + corner() + ")", end_cap("F2", 1));
  const ShapePtr second = fillet("F6", *first, {new_edge, top_front()}, 1);
  CHECK(volume(*second) < volume(*first));
  CHECK(faces_named(*second, "F6:fillet(" + new_edge + ")") == 1);
  CHECK(faces_named(*second, "F5:fillet(" + corner() + ")") == 1);
  CHECK(edge_names_unique(*second));
}

void test_fillet_after_booleans() {
  // The rim of a hole cut through the block, then a join on top.
  const ShapePtr drill = extrude("F4", {circle(5, 30, 20, 5)}, 0, 30);
  const ShapePtr holed = boolean(BooleanOp::Cut, {block().get()}, *drill).pieces.at(0).shape;
  const std::string rim = edge(end_cap("F2", 1), "F4:side(c5)");
  const ShapePtr boss = extrude("F6", {rectangle(9, 0, 0, 10, 10)}, 0, 30);
  const ShapePtr joined = boolean(BooleanOp::Join, {holed.get()}, *boss).pieces.at(0).shape;
  const double r = 1;
  const ShapePtr rounded = fillet("F7", *joined, {rim}, r);
  // A rim fillet on a hole of radius R removes about 2 pi (R + r) per unit
  // length of the loss of a straight edge (Pappus); allow for the curvature.
  const double expected = 2 * kPi * (5 + r * (1 - 2 / (3 * (4 - kPi)) * 1)) * fillet_loss(r);
  CHECK(near(volume(*joined) - volume(*rounded), expected, 0.05));
  CHECK(faces_named(*rounded, "F7:fillet(" + rim + ")") == 1);
}

void test_chamfers() {
  const ShapePtr body = block();
  const double d = 2;
  ChamferSpec equal;
  equal.distance = d;
  const ShapePtr bevelled = chamfer("F5", *body, {top_front()}, equal);
  CHECK(near(volume(*bevelled), 60.0 * 40.0 * 20.0 - d * d / 2 * 60.0));
  CHECK(faces_named(*bevelled, "F5:chamfer(" + top_front() + ")") == 1);
  CHECK(edge_names_unique(*bevelled));

  // Two distances: the first on the first face of the name. In
  // E{F2:end(...)|F2:side(...)} that is the top, so 1 mm is cut off the top
  // and 3 mm down the front face.
  ChamferSpec two;
  two.type = ChamferType::TwoDistances;
  two.distance = 1;
  two.distance2 = 3;
  const ShapePtr uneven = chamfer("F5", *body, {top_front()}, two);
  CHECK(near(volume(*uneven), 60.0 * 40.0 * 20.0 - 1.0 * 3.0 / 2 * 60.0));
  const auto lowest_front_point = [](const Shape& shape) {
    double lowest = 1e9;
    for (int i = 0; i < shape.vertex_count(); ++i) {
      const gp_Pnt p = BRep_Tool::Pnt(shape.vertex(i));
      if (near(p.Y(), 0) && p.Z() > 1) {
        lowest = std::min(lowest, p.Z());
      }
    }
    return lowest;
  };
  CHECK(near(lowest_front_point(*uneven), 17));
  two.flip = true;
  CHECK(near(lowest_front_point(*chamfer("F5", *body, {top_front()}, two)), 19));

  // Distance and angle: 2 mm on the top face at 30 degrees.
  ChamferSpec angled;
  angled.type = ChamferType::DistanceAngle;
  angled.distance = 2;
  angled.angle = kPi / 6;
  const ShapePtr sloped = chamfer("F5", *body, {top_front()}, angled);
  CHECK(near(volume(*sloped), 60.0 * 40.0 * 20.0 - 2.0 * 2.0 * std::tan(kPi / 6) / 2 * 60.0));

  angled.angle = kPi / 2;
  CHECK(throws_with([&] { chamfer("F5", *body, {top_front()}, angled); }, "between 0 and 90"));
  CHECK(throws_with([&] { chamfer("F5", *body, {top_front()}, ChamferSpec()); }, "distance must be"));
}

// The block's corner edge i (between side i - 1 and side i).
std::string corner_edge(int i) { return edge(side("F2", 1, (i + 3) % 4), side("F2", 1, i)); }

void test_fillet_edge_sets() {
  // T0 fil_two_edge_sets: 5 mm at (0, 0) and 2 mm at (60, 40) in one feature.
  FilletSet five;
  five.edges = {corner_edge(0)};
  five.radius = 5;
  FilletSet two;
  two.edges = {corner_edge(2)};
  two.radius = 2;
  const ShapePtr rounded = fillet("F5", *block(), {five, two});
  CHECK(near(volume(*rounded), 48000.0 - (fillet_loss(5) + fillet_loss(2)) * 20.0));
  CHECK(faces_named(*rounded, "F5:fillet(" + corner_edge(0) + ")") == 1);
  CHECK(faces_named(*rounded, "F5:fillet(" + corner_edge(2) + ")") == 1);
  CHECK(edge_names_unique(*rounded));
  CHECK(throws_with([&] { fillet("F5", *block(), {five, five}); }, "is in two edge sets"));

  // T0 fil_multi_edge: four vertical edges in one set.
  FilletSet all;
  all.edges = {corner_edge(0), corner_edge(1), corner_edge(2), corner_edge(3)};
  all.radius = 5;
  CHECK(near(volume(*fillet("F5", *block(), {all})), 48000.0 - 4 * fillet_loss(5) * 20.0));
}

void test_fillet_faces_select_their_edges() {
  // A cylinder r = 10, h = 20; the top cap selects its rim. The rounding's
  // cross-section has its centroid 2r / (3 (4 - pi)) from the fillet axis.
  const ShapePtr cylinder = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  FilletSet set;
  set.faces = {"F2:end(r{c5})"};
  set.radius = 2;
  const ShapePtr rounded = fillet("F5", *cylinder, {set});
  const double r = 2;
  const double ring = 2 * kPi * (10 - r + 2 * r / (3 * (4 - kPi))) * fillet_loss(r);
  CHECK(near(volume(*rounded), kPi * 100 * 20 - ring, 1e-6));
  CHECK(faces_named(*rounded, "F5:fillet(E{F2:end(r{c5})|F2:side(c5)})") == 1);
}

void test_fillet_tangent_chains() {
  // T0 fil_tangent_chain: after rounding the vertical edges by 5 mm, one top
  // edge with a tangent chain rounds the whole outline by 3 mm.
  FilletSet corners;
  corners.edges = {corner_edge(0), corner_edge(1), corner_edge(2), corner_edge(3)};
  corners.radius = 5;
  const ShapePtr first = fillet("F5", *block(), {corners});
  FilletSet top;
  top.edges = {top_front()};
  top.radius = 3;
  const ShapePtr second = fillet("F6", *first, {top});
  const double expected = 48000.0 - 4 * fillet_loss(5) * 20 - fillet_loss(3) * (2 * 50 + 2 * 30) -
                          4 * fillet_loss(3) * (kPi / 2) *
                              (5 - 3 * (5.0 / 6 - kPi / 4) / (1 - kPi / 4));
  CHECK(near(volume(*second), expected, 1e-6));
  // The edges the chain added name their faces too: the arcs at the corners.
  for (int i = 0; i < 4; ++i) {
    const std::string arc = edge(end_cap("F2", 1), "F5:fillet(" + corner_edge(i) + ")");
    CHECK(faces_named(*second, "F6:fillet(" + arc + ")") == 1);
  }
  CHECK(edge_names_unique(*second));
  // Without a tangent chain only the one edge would be rounded: unsupported.
  top.tangent_chain = false;
  CHECK(throws_with([&] { fillet("F6", *first, {top}); }, "unsupported: edge"));
  // On a block without tangent neighbours it makes no difference.
  CHECK(near(volume(*fillet("F6", *block(), {top})), 48000.0 - fillet_loss(3) * 60));
}

void test_fillet_chord_length_and_variable_radius() {
  // fil_chord (K33): a 5 mm chord across a right angle is r = 5 / sqrt(2).
  FilletSet chord;
  chord.edges = {corner_edge(0)};
  chord.size = FilletSize::ChordLength;
  chord.chord = 5;
  CHECK(near(volume(*fillet("F5", *block(), {chord})),
             48000.0 - fillet_loss(5 / std::sqrt(2.0)) * 20));

  // fil_variable (K32): 5 to 10 mm linearly along the 60 mm top front edge.
  FilletSet variable;
  variable.edges = {top_front()};
  variable.size = FilletSize::Variable;
  variable.radius = 5;
  variable.radius2 = 10;
  const double loss = (1 - kPi / 4) * 60 * (25 + 50 + 100) / 3.0;
  const ShapePtr sloped = fillet("F5", *block(), {variable});
  CHECK_NEAR_TOL(volume(*sloped), 48000.0 - loss, 2e-4);
  // The start radius at the vertex at x = 60: the front face then ends at
  // z = 15 there and at z = 10 at x = 0.
  const auto lowest_top_at = [](const Shape& shape, double x) {
    double lowest = 1e9;
    for (int i = 0; i < shape.vertex_count(); ++i) {
      const gp_Pnt p = BRep_Tool::Pnt(shape.vertex(i));
      if (near(p.X(), x) && near(p.Y(), 0) && p.Z() > 1) {
        lowest = std::min(lowest, p.Z());
      }
    }
    return lowest;
  };
  variable.start_vertex =
      "V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 1) + "}";
  const ShapePtr reversed = fillet("F5", *block(), {variable});
  CHECK_NEAR_TOL(volume(*reversed), 48000.0 - loss, 2e-4);
  CHECK(near(lowest_top_at(*reversed, 60), 15, 1e-4));
  CHECK(near(lowest_top_at(*reversed, 0), 10, 1e-4));
  variable.start_vertex =
      "V{" + start_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 1) + "}";
  CHECK(throws_with([&] { fillet("F5", *block(), {variable}); }, "is not an end"));
}

// The area of the cross-section x = const of a shape: its volume in a thin
// slab across x, divided by the slab's thickness.
double area_across_x(const Shape& shape, double x) {
  const double h = 0.02;
  const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(x - h / 2, -1000, -1000), h, 2000, 2000).Shape();
  // Non-destructive: the shape is a result the run's input check watches.
  BRepAlgoAPI_Common common;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(shape.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(slab);
  common.SetArguments(arguments);
  common.SetTools(tools);
  common.SetNonDestructive(true);
  common.Build();
  GProp_GProps props;
  BRepGProp::VolumeProperties(common.Shape(), props);
  return props.Mass() / h;
}

void test_fillet_mid_radii() {
  // K32 with a mid radius: 5 mm at x = 0, 6 mm a quarter along, 10 mm at
  // x = 60 on the top front edge. The radius follows a smooth law through
  // them, so the cross-section x = 15 misses (1 - pi / 4) 6^2 of the block's.
  FilletSet variable;
  variable.edges = {top_front()};
  variable.size = FilletSize::Variable;
  variable.radius = 5;
  variable.radius2 = 10;
  variable.mid = {{0.25, 6.0}};
  variable.start_vertex =
      "V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 3) + "}";
  const ShapePtr rounded = fillet("F5", *block(), {variable});
  CHECK_NEAR_TOL(800 - area_across_x(*rounded, 15), (1 - kPi / 4) * 36, 1e-4);
  CHECK(faces_named(*rounded, "F5:fillet(" + top_front() + ")") == 1);
  // From the other end the mid radius is at x = 45.
  variable.start_vertex =
      "V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 1) + "}";
  const ShapePtr reversed = fillet("F5", *block(), {variable});
  CHECK_NEAR_TOL(800 - area_across_x(*reversed, 45), (1 - kPi / 4) * 36, 1e-4);
  variable.mid = {{0.5, 6.0}, {0.25, 7.0}};
  CHECK(throws_with([&] { fillet("F5", *block(), {variable}); }, "must increase"));

  // Along a chain of three edges: the top front edge (50 mm), the arc of a
  // vertical 10 mm fillet at (60, 0) and the top right edge (30 mm). The
  // radii run along the whole chain: 2 mm at (0, 0), 5 mm half along it
  // (x = 47.85 on the first edge), 4 mm at (60, 40).
  FilletSet vertical;
  vertical.edges = {corner_edge(1)};
  vertical.radius = 10;
  const ShapePtr corner_round = fillet("F4", *block(), {vertical});
  FilletSet chain;
  chain.edges = {top_front()};
  chain.size = FilletSize::Variable;
  chain.radius = 2;
  chain.radius2 = 4;
  chain.mid = {{0.5, 5.0}};
  chain.start_vertex =
      "V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 3) + "}";
  const ShapePtr along = fillet("F5", *corner_round, {chain});
  const double half = (50 + 10 * kPi / 2 + 30) / 2;
  CHECK_NEAR_TOL(800 - area_across_x(*along, half), (1 - kPi / 4) * 25, 1e-3);
  CHECK_NEAR_TOL(800 - area_across_x(*along, 0.05), (1 - kPi / 4) * 4, 5e-3);
}

void test_chamfer_miter_corners() {
  // The three edges at the corner (60, 0, 20), 2 mm. With miter corners
  // the bevels run on until they meet: the removed prisms 2 * 2 / 2 * 120
  // overlap pairwise in pyramids of 8 / 3 and all three in 2 (K90).
  ChamferSet three;
  three.edges = {corner_edge(1), top_front(), edge(end_cap("F2", 1), side("F2", 1, 1))};
  three.spec.distance = 2;
  const ShapePtr mitered = chamfer("F5", *block(), {three}, ChamferCorner::Miter);
  CHECK_NEAR(volume(*mitered), 48000.0 - (240 - 3 * 8.0 / 3 + 2));
  for (const std::string& e : three.edges) {
    CHECK(faces_named(*mitered, "F5:chamfer(" + e + ")") == 1);
  }
  CHECK(mitered->face_count() == 9); // no face of the corner
  CHECK(edge_names_unique(*mitered));
  CHECK(mitered->notes().empty());
  // OCCT's own corner cuts the corner off too.
  const ShapePtr patched = chamfer("F5", *block(), {three}, ChamferCorner::Chamfer);
  CHECK(volume(*patched) < volume(*mitered) - 0.01);

  // cha_top_loop: only two bevelled edges meet at each vertex, so the miter
  // corner type changes nothing, and the result says so.
  ChamferSet loop;
  loop.faces = {end_cap("F2", 1)};
  loop.spec.distance = 2;
  const ShapePtr top = chamfer("F5", *block(), {loop}, ChamferCorner::Miter);
  CHECK_NEAR(volume(*top), 48000.0 - (2.0 * 200 - 4 * 8.0 / 3));
  CHECK(top->notes().size() == 1);

  // Concave: a notch [0, 20] x [0, 20] x [10, 20] cut from the block's
  // corner; its three inner edges at (20, 20, 10), 2 mm. The wedges added
  // along them (lengths 20, 20, 10) overlap as the convex prisms do.
  const ShapePtr notch = extrude("F3", {rectangle(5, 0, 0, 20, 20)}, 10, 20);
  const ShapePtr notched = boolean(BooleanOp::Cut, {block().get()}, *notch).pieces.at(0).shape;
  const std::string floor = start_cap("F3", 5);
  const std::string wall_x = side("F3", 5, 1);
  const std::string wall_y = side("F3", 5, 2);
  ChamferSet inner;
  inner.edges = {edge(floor, wall_x), edge(floor, wall_y), edge(wall_x, wall_y)};
  inner.spec.distance = 2;
  const ShapePtr filled = chamfer("F6", *notched, {inner}, ChamferCorner::Miter);
  CHECK_NEAR(volume(*filled), 44000.0 + (2.0 * 50 - 3 * 8.0 / 3 + 2));
  CHECK(edge_names_unique(*filled));
  // Where the notch's vertical edge meets the top, convex edges meet it.
  ChamferSet mixed;
  mixed.edges = {edge(wall_x, wall_y), edge(end_cap("F2", 1), wall_x), edge(end_cap("F2", 1), wall_y)};
  mixed.spec.distance = 1;
  CHECK(throws_with([&] { chamfer("F6", *notched, {mixed}, ChamferCorner::Miter); },
                    "unsupported: a miter corner where convex and concave"));
}

// Volumes integrated adaptively: the blend surfaces are B-splines of high
// degree, which volume()'s fixed Gauss points integrate only roughly.
double accurate_volume(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props, 1.0e-9);
  return props.Mass();
}

// The cross-section a G2 fillet with contact distance e takes at a right
// angle (Mitcad's quintic: inner poles 0.35 w and 0.7 w of the way to the
// corner): the area between the corner and the curve, and the distance of
// its centroid from the corner along the first face.
std::pair<double, double> g2_section(double e, double weight) {
  const double near_pole = 0.35 * weight;
  const double far_pole = std::min(0.7 * weight, 0.95);
  const double u[6] = {e, (1 - near_pole) * e, (1 - far_pole) * e, 0, 0, 0};
  const double v[6] = {0, 0, 0, (1 - far_pole) * e, (1 - near_pole) * e, e};
  const double binomial[6] = {1, 5, 10, 10, 5, 1};
  const auto at = [&](const double* c, double t) {
    double value = 0;
    for (int k = 0; k < 6; ++k) {
      value += binomial[k] * std::pow(t, k) * std::pow(1 - t, 5 - k) * c[k];
    }
    return value;
  };
  // The derivative: 5 times the quartic of the poles' differences.
  const auto slope = [](const double* c, double t) {
    const double binomial4[5] = {1, 4, 6, 4, 1};
    double d = 0;
    for (int k = 0; k < 5; ++k) {
      d += 5 * binomial4[k] * std::pow(t, k) * std::pow(1 - t, 4 - k) * (c[k + 1] - c[k]);
    }
    return d;
  };
  // Gauss-Legendre with 5 points integrates the degree 9 integrands exactly.
  const double x[5] = {-0.9061798459386640, -0.5384693101056831, 0.0, 0.5384693101056831,
                       0.9061798459386640};
  const double w[5] = {0.2369268850561891, 0.4786286704993665, 0.5688888888888889, 0.4786286704993665,
                       0.2369268850561891};
  double area = 0;
  double moment = 0;
  for (int i = 0; i < 5; ++i) {
    const double t = (x[i] + 1) / 2;
    const double uu = at(u, t);
    const double vv = at(v, t);
    area += w[i] / 2 * 0.5 * (uu * slope(v, t) - vv * slope(u, t));
    moment += w[i] / 2 * 0.5 * uu * uu * slope(v, t);
  }
  return {area, moment / area};
}

// The lowest point of the shape on the front face (y = 0) above z = 1.
double lowest_front(const Shape& shape) {
  double lowest = 1e9;
  for (int i = 0; i < shape.vertex_count(); ++i) {
    const gp_Pnt p = BRep_Tool::Pnt(shape.vertex(i));
    if (near(p.Y(), 0) && p.Z() > 1) {
      lowest = std::min(lowest, p.Z());
    }
  }
  return lowest;
}

void test_asymmetric_fillets() {
  // fil_asymmetric (K88): 3 mm on the top face (the reference), 6 mm down
  // the front face along the 60 mm top front edge: a quarter ellipse.
  FilletSet set;
  set.edges = {top_front()};
  set.size = FilletSize::Asymmetric;
  set.radius = 3;
  set.radius2 = 6;
  set.reference_face = end_cap("F2", 1);
  const ShapePtr rounded = fillet("F5", *block(), {set});
  CHECK_NEAR_TOL(accurate_volume(*rounded), 48000.0 - (1 - kPi / 4) * 3 * 6 * 60, 1e-7);
  CHECK(near(lowest_front(*rounded), 14));
  const std::string face = "F5:fillet(" + top_front() + ")";
  CHECK(faces_named(*rounded, face) == 1);
  CHECK(rounded->face_count() == 7);
  CHECK(edge_names_unique(*rounded));
  // Tangent to both faces along the contact lines.
  for (const std::string& side_face : {end_cap("F2", 1), side("F2", 1, 0)}) {
    const std::vector<int> contact = rounded->find_edges(edge(face, side_face));
    CHECK(contact.size() == 1);
    if (contact.size() == 1) {
      const auto faces = rounded->edge_faces(contact.front());
      CHECK(detail::normal_angle(rounded->edge(contact.front()), rounded->face(faces[0]),
                                 rounded->face(faces[1])) < 1e-6);
    }
  }
  // Flipped: 6 mm on the top face.
  set.flip = true;
  CHECK(near(lowest_front(*fillet("F5", *block(), {set})), 17));
  // Without a reference face the first distance is on the first face of
  // the edge's name, the top.
  set.flip = false;
  set.reference_face.clear();
  CHECK(near(lowest_front(*fillet("F5", *block(), {set})), 14));
}

void test_g2_fillets() {
  // fil_g2 (K87): 3 mm along the top front edge, weight 1. It touches the
  // faces 3 mm from the edge, as the circular one would, and removes less.
  FilletSet set;
  set.edges = {top_front()};
  set.radius = 3;
  set.curvature = true;
  const ShapePtr rounded = fillet("F5", *block(), {set});
  const auto [area, centroid] = g2_section(3, 1);
  CHECK(area < fillet_loss(3));
  CHECK_NEAR_TOL(accurate_volume(*rounded), 48000.0 - area * 60, 1e-7);
  CHECK(near(lowest_front(*rounded), 17));
  CHECK(faces_named(*rounded, "F5:fillet(" + top_front() + ")") == 1);
  CHECK(edge_names_unique(*rounded));
  // A larger weight keeps the curve closer to the corner.
  set.weight = 2;
  CHECK_NEAR_TOL(accurate_volume(*fillet("F5", *block(), {set})), 48000.0 - g2_section(3, 2).first * 60,
                 1e-7);
  set.weight = 1;

  // A concave edge: the riser of an L (a 60 x 20 x 20 box joined on the
  // block's front half), its foot rounded 2 mm: material is added.
  const ShapePtr riser = extrude("F3", {rectangle(5, 0, 0, 60, 20)}, 20, 40);
  const ShapePtr ell = boolean(BooleanOp::Join, {block().get()}, *riser).pieces.at(0).shape;
  FilletSet foot;
  foot.edges = {edge(end_cap("F2", 1), side("F3", 5, 2))};
  foot.radius = 2;
  foot.curvature = true;
  CHECK_NEAR_TOL(accurate_volume(*fillet("F4", *ell, {foot})), 72000.0 + g2_section(2, 1).first * 60, 1e-7);

  // A closed chain: the top outline of a block whose vertical edges are
  // rounded 5 mm. Along the arcs the cross-section turns about their axes
  // (Pappus), the centroid 5 - c from the axis.
  FilletSet corners;
  corners.edges = {corner_edge(0), corner_edge(1), corner_edge(2), corner_edge(3)};
  corners.radius = 5;
  const ShapePtr outline = fillet("F4", *block(), {corners});
  FilletSet top;
  top.edges = {top_front()};
  top.radius = 2;
  top.curvature = true;
  const ShapePtr smooth = fillet("F5", *outline, {top});
  const auto [ring, offset] = g2_section(2, 1);
  const double expected = 48000.0 - 4 * fillet_loss(5) * 20 - ring * (2 * 50 + 2 * 30) -
                          4 * ring * (kPi / 2) * (5 - offset);
  CHECK_NEAR_TOL(accurate_volume(*smooth), expected, 1e-6);
  CHECK(edge_names_unique(*smooth));

  // One closed edge: the rim of a cylinder r = 10, h = 20, its top selecting
  // it. The cross-section turns about the axis, its centroid 10 - c out.
  const ShapePtr cylinder = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  FilletSet rim;
  rim.faces = {"F2:end(r{c5})"};
  rim.radius = 2;
  rim.curvature = true;
  const ShapePtr rounded_rim = fillet("F5", *cylinder, {rim});
  CHECK_NEAR_TOL(accurate_volume(*rounded_rim), kPi * 100 * 20 - ring * 2 * kPi * (10 - offset), 1e-7);
  CHECK(faces_named(*rounded_rim, "F5:fillet(E{F2:end(r{c5})|F2:side(c5)})") == 1);
  CHECK(edge_names_unique(*rounded_rim));
  // Asymmetric, 1 mm on the top (the first face of the edge's name) and 3
  // mm down the side: the quarter ellipse's complement has its centroid
  // (5/6 - pi/4) / (1 - pi/4) mm in from the rim.
  FilletSet uneven;
  uneven.faces = rim.faces;
  uneven.size = FilletSize::Asymmetric;
  uneven.radius = 1;
  uneven.radius2 = 3;
  const double inward = (5.0 / 6 - kPi / 4) / (1 - kPi / 4);
  CHECK_NEAR_TOL(accurate_volume(*fillet("F5", *cylinder, {uneven})),
                 kPi * 100 * 20 - (1 - kPi / 4) * 3 * 2 * kPi * (10 - inward), 1e-7);

  // With a rolling ball set elsewhere (the vertical edge at (60, 40)).
  FilletSet vertical;
  vertical.edges = {corner_edge(2)};
  vertical.radius = 5;
  CHECK_NEAR_TOL(accurate_volume(*fillet("F5", *block(), {vertical, set})),
                 48000.0 - fillet_loss(5) * 20 - area * 60, 1e-7);

  // Chains that meet at a corner, and variable radii, are unsupported.
  FilletSet three = set;
  three.edges = {corner_edge(1), top_front(), edge(end_cap("F2", 1), side("F2", 1, 1))};
  CHECK(throws_with([&] { fillet("F5", *block(), {three}); }, "unsupported: asymmetric and curvature"));
  FilletSet variable = set;
  variable.size = FilletSize::Variable;
  variable.radius2 = 4;
  CHECK(throws_with([&] { fillet("F5", *block(), {variable}); }, "unsupported: curvature continuous fillets "
                                                                 "with a variable radius"));
  (void)centroid;
}

// The largest angle between the normals of a face and its neighbours along
// their common edges, at the edges' middles.
double largest_kink(const Shape& shape, int face) {
  double largest = 0;
  for (TopExp_Explorer it(shape.face(face), TopAbs_EDGE); it.More(); it.Next()) {
    const int e = shape.edge_index(it.Current());
    const auto faces = shape.edge_faces(e);
    if (faces[0] >= 0 && faces[1] >= 0 && faces[0] != faces[1]) {
      largest = std::max(largest, detail::normal_angle(shape.edge(e), shape.face(faces[0]), shape.face(faces[1])));
    }
  }
  return largest;
}

void test_fillet_corner_types() {
  // K31: three 2 mm fillets at the corner (60, 0, 20). Rolling ball: the
  // prisms (1 - pi/4) r^2 along the 120 mm of edges but the r long ends in
  // the corner's cube, where an eighth of the ball stays: r^3 (1 - pi/6).
  FilletSet three;
  three.edges = {corner_edge(1), top_front(), edge(end_cap("F2", 1), side("F2", 1, 1))};
  three.radius = 2;
  const ShapePtr rolled = fillet("F5", *block(), {three}, true);
  const double rolling = 48000.0 - (fillet_loss(2) * (120 - 3 * 2) + 8 * (1 - kPi / 6));
  CHECK_NEAR(volume(*rolled), rolling);
  // Setback (K89): the roundings stop 3 mm (1.5 r) from the vertex and a
  // patch tangent to them and to the faces closes the corner.
  const ShapePtr setback = fillet("F5", *block(), {three}, false);
  const std::string corner = "F5:corner(V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" +
                             side("F2", 1, 1) + "})";
  CHECK(faces_named(*setback, corner) == 1);
  for (const std::string& e : three.edges) {
    CHECK(faces_named(*setback, "F5:fillet(" + e + ")") == 1);
  }
  // (Mitcad's own corner, measured: a little more material than the ball.)
  CHECK_NEAR_TOL(accurate_volume(*setback), 47899.467, 1e-6);
  CHECK(std::abs(accurate_volume(*setback) - rolling) < 2.0);
  CHECK(edge_names_unique(*setback));
  for (int f = 0; f < setback->face_count(); ++f) {
    if (has_name(setback->face_names(f), corner)) {
      CHECK(largest_kink(*setback, f) < 2e-2);
    }
  }
  // Without corners the corner type changes nothing.
  FilletSet one;
  one.edges = {corner_edge(1)};
  one.radius = 2;
  CHECK(near(volume(*fillet("F5", *block(), {one}, false)), 48000.0 - fillet_loss(2) * 20));

  // Chamfer blend corners (K90): the same corner bevelled 2 mm, the bevels
  // stopped 3 mm from the vertex and a patch between them.
  ChamferSet bevels;
  bevels.edges = three.edges;
  bevels.spec.distance = 2;
  const ShapePtr blended = chamfer("F5", *block(), {bevels}, ChamferCorner::Blend);
  CHECK(faces_named(*blended, corner) == 1);
  // (Measured; OCCT's own corner: 47765.333, the miter 47766.)
  CHECK_NEAR_TOL(accurate_volume(*blended), 47769.27, 1e-6);
  CHECK(std::abs(accurate_volume(*blended) - volume(*chamfer("F5", *block(), {bevels}))) < 5.0);
  CHECK(edge_names_unique(*blended));
}

void test_chamfer_sets_and_reference_faces() {
  // T0 cha_top_loop: the four top edges, 2 mm; the corners overlap in
  // pyramids of 8 / 3.
  ChamferSet loop;
  loop.faces = {end_cap("F2", 1)};
  loop.spec.distance = 2;
  CHECK(near(volume(*chamfer("F5", *block(), {loop})), 48000.0 - (2.0 * 200 - 4 * 8.0 / 3)));

  // T0 cha_two_distances on the vertical edge at (0, 0): 5 mm on the face
  // x = 0 (side 3), 3 mm on y = 0.
  ChamferSet two;
  two.edges = {corner_edge(0)};
  two.spec.type = ChamferType::TwoDistances;
  two.spec.distance = 5;
  two.spec.distance2 = 3;
  two.spec.reference_face = side("F2", 1, 3);
  const auto corner_points = [](const Shape& shape) {
    double along_x = 0;
    double along_y = 0;
    for (int i = 0; i < shape.vertex_count(); ++i) {
      const gp_Pnt p = BRep_Tool::Pnt(shape.vertex(i));
      if (near(p.Z(), 0) && near(p.Y(), 0) && p.X() < 30) {
        along_x = p.X();
      }
      if (near(p.Z(), 0) && near(p.X(), 0) && p.Y() < 20) {
        along_y = p.Y();
      }
    }
    return std::make_pair(along_x, along_y);
  };
  const ShapePtr uneven = chamfer("F5", *block(), {two});
  CHECK(near(volume(*uneven), 48000.0 - 5 * 3 / 2.0 * 20));
  const auto [x1, y1] = corner_points(*uneven);
  CHECK(near(x1, 3) && near(y1, 5));
  two.spec.flip = true;
  const auto [x2, y2] = corner_points(*chamfer("F5", *block(), {two}));
  CHECK(near(x2, 5) && near(y2, 3));
  two.spec.reference_face = end_cap("F2", 1);
  CHECK(throws_with([&] { chamfer("F5", *block(), {two}); }, "does not border face"));

  // T0 cha_distance_angle: 5 mm and 30 degrees, the other leg 5 tan 30.
  ChamferSet angled;
  angled.edges = {corner_edge(0)};
  angled.spec.type = ChamferType::DistanceAngle;
  angled.spec.distance = 5;
  angled.spec.angle = kPi / 6;
  angled.spec.reference_face = side("F2", 1, 0);
  CHECK(near(volume(*chamfer("F5", *block(), {angled})),
             48000.0 - 5 * 5 * std::tan(kPi / 6) / 2 * 20));

  // A chamfer along a rounded outline follows the tangent chain.
  FilletSet corners;
  corners.edges = {corner_edge(0), corner_edge(1), corner_edge(2), corner_edge(3)};
  corners.radius = 5;
  const ShapePtr rounded = fillet("F4", *block(), {corners});
  ChamferSet rim;
  rim.edges = {top_front()};
  rim.spec.distance = 1;
  CHECK(chamfer("F5", *rounded, {rim})->face_count() == rounded->face_count() + 8);
  rim.tangent_chain = false;
  CHECK(throws_with([&] { chamfer("F5", *rounded, {rim}); }, "unsupported:"));
}

void test_dressups_that_take_a_face_away() {
  // A rib 2 mm wide: rounding both top edges by 1 mm (a full round) or
  // bevelling them by 1 mm leaves no top face, which OCCT cannot build;
  // the sizes are retried 1e-3 smaller, leaving a strip that narrow.
  const std::vector<std::string> top = {edge(end_cap("F2", 1), side("F2", 1, 1)),
                                        edge(end_cap("F2", 1), side("F2", 1, 3))};
  const ShapePtr round = fillet("F5", *block(2), top, 1);
  CHECK(near(volume(*round), 2.0 * 40 * 20 - 2 * fillet_loss(1) * 40, 1e-4));
  CHECK(faces_named(*round, "F5:fillet(" + top[0] + ")") == 1);
  // The result says so, for the model's warning (P9).
  CHECK(round->notes().size() == 1);
  CHECK(round->notes().front().find("0.1 % smaller") != std::string::npos);
  CHECK(fillet("F5", *block(2), top, 0.5)->notes().empty());
  ChamferSet bevel;
  bevel.edges = top;
  bevel.spec.distance = 1;
  const ShapePtr bevelled = chamfer("F5", *block(2), {bevel});
  CHECK(near(volume(*bevelled), 2.0 * 40 * 20 - 2 * 0.5 * 40, 1e-4));
  CHECK(bevelled->notes().size() == 1);
}

} // namespace

void dressup_tests() {
  test_fillet_volume_and_names();
  test_fillet_corner_blends_are_named();
  test_fillet_follows_a_dimension_change();
  test_several_fillets_on_one_body();
  test_fillet_after_booleans();
  test_chamfers();
  guarded("test_fillet_edge_sets", test_fillet_edge_sets);
  guarded("test_fillet_faces_select_their_edges", test_fillet_faces_select_their_edges);
  guarded("test_fillet_tangent_chains", test_fillet_tangent_chains);
  guarded("test_fillet_chord_length_and_variable_radius", test_fillet_chord_length_and_variable_radius);
  guarded("test_fillet_mid_radii", test_fillet_mid_radii);
  guarded("test_chamfer_miter_corners", test_chamfer_miter_corners);
  guarded("test_asymmetric_fillets", test_asymmetric_fillets);
  guarded("test_g2_fillets", test_g2_fillets);
  guarded("test_fillet_corner_types", test_fillet_corner_types);
  guarded("test_chamfer_sets_and_reference_faces", test_chamfer_sets_and_reference_faces);
  guarded("test_dressups_that_take_a_face_away", test_dressups_that_take_a_face_away);
}

} // namespace test
