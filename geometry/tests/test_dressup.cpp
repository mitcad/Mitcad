// SPDX-License-Identifier: MIT
// Fillets and chamfers by edge name, the names of their faces, and names
// that survive dimension changes and booleans.

#include <algorithm>
#include <cmath>
#include <utility>

#include <cstdint>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_List.hxx>
#include <Poly_Triangulation.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec2d.hxx>

#include "../src/face_select.hpp"
#include "../src/history.hpp"
#include "../src/util.hpp"
#include "mitcad/geometry/brep_import.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/naming.hpp"
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
  // Wider than the side (20) and the top (40) together: no rolling ball fits.
  CHECK(throws_with([&] { fillet("F5", *block(), {top_front()}, 60); }, "fillet failed"));
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

// A chamfer the geometry kernel cannot build says what it was asked for
// (mitcad#1185): its edge, and the faces its chain ends against, with the
// kernel's own message kept and the message's start as before (the .f3d
// import tells a failure of the kernel by it). The block is 20 high and 40
// deep: a bevel of 100 down its front face and across its top would take
// all of it away, which cannot be built; the error says the bevel is
// wider than the faces beside the edge (mitcad#1232; one of 30 is built
// past the front face, cut by the bottom).
void test_chamfer_failure_names_its_edges_and_ends() {
  ChamferSpec huge;
  huge.distance = 100;
  std::string message;
  try {
    chamfer("F5", *block(), {top_front()}, huge);
  } catch (const std::invalid_argument& error) {
    message = std::string("invalid: ") + error.what();
  } catch (const std::runtime_error& error) {
    message = error.what();
  }
  std::printf("test_chamfer_failure_names_its_edges_and_ends: %s\n", message.c_str());
  CHECK(message.rfind("chamfer", 0) == 0);
  CHECK(message.find("edge " + top_front()) != std::string::npos);
  // The front top edge ends at the block's corners, against its two ends.
  CHECK(message.find("its chain ending against faces") != std::string::npos);
  CHECK(message.find(side("F2", 1, 1)) != std::string::npos);
  CHECK(message.find(side("F2", 1, 3)) != std::string::npos);
  CHECK(message.find("is wider than face") != std::string::npos);
  // 30: the plane z = y - 10 takes away 200 mm2 down to the bottom and the
  // triangle of legs 20 above it, along 60.
  huge.distance = 30;
  CHECK(near(volume(*chamfer("F5", *block(), {top_front()}, huge)), 60.0 * 40.0 * 20.0 - (200.0 + 200.0) * 60.0));
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

void test_fillet_spline_radius() {
  // mitcad#464 (port patch 0920): the radius along the 60 mm top front edge
  // a cubic Bezier of control radii 2, 6, 3, 4 from x = 0, by the position
  // u = x / 60: r(u) = 2 (1-u)^3 + 18 u (1-u)^2 + 9 u^2 (1-u) + 4 u^3. Each
  // cross-section misses (1 - pi / 4) r^2 of the block's, so the volume
  // misses (1 - pi / 4) 60 times the integral of r^2, 100 / 7.
  const auto r = [](double u) {
    const double v = 1 - u;
    return 2 * v * v * v + 18 * u * v * v + 9 * u * u * v + 4 * u * u * u;
  };
  FilletSet spline;
  spline.edges = {top_front()};
  spline.size = FilletSize::Spline;
  spline.knots = {0, 0, 0, 0, 1, 1, 1, 1};
  spline.radii = {2, 6, 3, 4};
  spline.start_vertex =
      "V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 3) + "}";
  const ShapePtr rounded = fillet("F5", *block(), {spline});
  CHECK_NEAR_TOL(volume(*rounded), 48000.0 - (1 - kPi / 4) * 60 * 100 / 7.0, 1e-5);
  for (const double u : {0.1, 0.25, 0.5, 0.8}) {
    CHECK_NEAR_TOL(800 - area_across_x(*rounded, 60 * u), (1 - kPi / 4) * r(u) * r(u), 2e-3);
  }
  CHECK(faces_named(*rounded, "F5:fillet(" + top_front() + ")") == 1);
  // From the other end: r(u) at x = 60 (1 - u).
  spline.start_vertex =
      "V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 1) + "}";
  const ShapePtr reversed = fillet("F5", *block(), {spline});
  CHECK_NEAR_TOL(800 - area_across_x(*reversed, 60 * (1 - 0.1)), (1 - kPi / 4) * r(0.1) * r(0.1), 2e-3);

  // Two spans of degree 2 (knots 0, 0, 0, 0.5, 1, 1, 1; control radii 2, 4,
  // 6, 3): at the inner knot r = (4 + 6) / 2 = 5, at u = 0.25 (t = 1/2 of
  // the first span) 2 / 4 + 4 * 5 / 8 + 6 / 8 = 3.75.
  spline.knots = {0, 0, 0, 0.5, 1, 1, 1};
  spline.radii = {2, 4, 6, 3};
  spline.start_vertex =
      "V{" + end_cap("F2", 1) + "|" + side("F2", 1, 0) + "|" + side("F2", 1, 3) + "}";
  const ShapePtr spans = fillet("F5", *block(), {spline});
  CHECK_NEAR_TOL(800 - area_across_x(*spans, 30), (1 - kPi / 4) * 25, 2e-3);
  CHECK_NEAR_TOL(800 - area_across_x(*spans, 15), (1 - kPi / 4) * 3.75 * 3.75, 2e-3);

  // Along a chain of three edges (the top front edge, the arc of a vertical
  // 10 mm fillet at (60, 0), the top right edge): the radius runs along the
  // whole chain, here linear (degree 1) from 2 at (0, 0) to 4 at (60, 40),
  // 3 half along it.
  FilletSet vertical;
  vertical.edges = {corner_edge(1)};
  vertical.radius = 10;
  const ShapePtr corner_round = fillet("F4", *block(), {vertical});
  spline.knots = {0, 0, 1, 1};
  spline.radii = {2, 4};
  const ShapePtr along = fillet("F5", *corner_round, {spline});
  const double half = (50 + 10 * kPi / 2 + 30) / 2;
  CHECK_NEAR_TOL(800 - area_across_x(*along, half), (1 - kPi / 4) * 9, 1e-3);

  // Malformed splines are refused.
  spline.knots = {0, 0, 0.5, 1};
  CHECK(throws_with([&] { fillet("F5", *block(), {spline}); }, "knots of a spline radius"));
  spline.knots = {0, 0, 1, 1, 1};
  CHECK(throws_with([&] { fillet("F5", *block(), {spline}); }, "degree 1 to 3"));
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

// The area of the face named (or split from) `name`.
double named_face_area(const Shape& shape, const std::string& name) {
  double area = 0.0;
  for (int i = 0; i < shape.face_count(); ++i) {
    if (face_matches(shape.face_names(i), name)) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(shape.face(i), props);
      area += props.Mass();
    }
  }
  return area;
}

void test_angle_chamfer_measures_one_face_along_a_chain() {
  // The top outline of a block with its four corners rounded is one tangent
  // chain of eight edges. The roundings are the feature "F1", so their faces
  // sort before the top face: the arcs' names list the rounding first, the
  // lines' names the top face first. A distance-angle chamfer without a
  // reference face measures on the first face of the edge it is given, and
  // OCCT takes that face's role along the whole chain: the chain's first
  // listed edge decides, not the lowest index in the body.
  FilletSet corners;
  corners.edges = {corner_edge(0), corner_edge(1), corner_edge(2), corner_edge(3)};
  corners.radius = 5;
  const ShapePtr rounded = fillet("F1", *block(), {corners});
  const std::string line = top_front();
  const std::string arc = edge(end_cap("F2", 1), "F1:fillet(" + corner_edge(0) + ")");
  CHECK(rounded->find_edges(arc).size() == 1);
  const auto first_face = [&](const std::string& name) {
    const std::vector<int> found = rounded->find_edges(name);
    return found.size() == 1 ? rounded->edge_faces(found.front())[0] : -1;
  };
  // (The faces the two kinds of edges list first are of different roles.)
  CHECK(face_matches(rounded->face_names(first_face(line)), end_cap("F2", 1)));
  CHECK(!face_matches(rounded->face_names(first_face(arc)), end_cap("F2", 1)));

  const double outline = 2.0 * (60 + 40) - 8.0 * 5 + 2.0 * kPi * 5;
  const double top = 60.0 * 40.0 - (4 - kPi) * 25.0;
  const double angle = kPi / 6;
  const auto top_after = [&](double inset) { return top - outline * inset + kPi * inset * inset; };

  ChamferSet angled;
  angled.spec.type = ChamferType::DistanceAngle;
  angled.spec.distance = 2;
  angled.spec.angle = angle;
  // The consistent reference face: 2 mm on the top face.
  angled.edges = {line};
  angled.spec.reference_face = end_cap("F2", 1);
  const ShapePtr on_top = chamfer("F5", *rounded, {angled});
  CHECK_NEAR_TOL(named_face_area(*on_top, end_cap("F2", 1)), top_after(2.0), 1e-5);
  // The first listed edge a line: the top face again, whichever edge of the
  // chain has the lowest index.
  angled.spec.reference_face.clear();
  const ShapePtr from_line = chamfer("F5", *rounded, {angled});
  CHECK_NEAR_TOL(named_face_area(*from_line, end_cap("F2", 1)), top_after(2.0), 1e-5);
  CHECK(near(volume(*from_line), volume(*on_top), 1e-9));
  // The first listed edge an arc: its first face is the rounding, which the
  // whole chain measures on (the top face is cut back by the other leg).
  angled.edges = {arc};
  const ShapePtr from_arc = chamfer("F5", *rounded, {angled});
  CHECK_NEAR_TOL(named_face_area(*from_arc, end_cap("F2", 1)), top_after(2.0 * std::tan(angle)), 1e-5);
}

void test_dressups_that_take_a_face_away() {
  // A rib 2 mm wide: rounding both top edges by 1 mm (a full round) or
  // bevelling them by 1 mm leaves no top face. OCCT builds both at their
  // size since the port's patch 0950 (the two contacts along each other on
  // the top meet there), with no note of a size 0.1 % smaller.
  const std::vector<std::string> top = {edge(end_cap("F2", 1), side("F2", 1, 1)),
                                        edge(end_cap("F2", 1), side("F2", 1, 3))};
  const ShapePtr round = fillet("F5", *block(2), top, 1);
  CHECK(near(volume(*round), 2.0 * 40 * 20 - 2 * fillet_loss(1) * 40, 1e-6));
  CHECK(faces_named(*round, "F5:fillet(" + top[0] + ")") == 1);
  CHECK(round->notes().empty());
  CHECK(fillet("F5", *block(2), top, 0.5)->notes().empty());
  ChamferSet bevel;
  bevel.edges = top;
  bevel.spec.distance = 1;
  const ShapePtr bevelled = chamfer("F5", *block(2), {bevel});
  CHECK(near(volume(*bevelled), 2.0 * 40 * 20 - 2 * 0.5 * 40, 1e-6));
  CHECK(bevelled->notes().empty());
}

// A 50 x 100 rectangle extruded 10 high: its end side 0, 50 wide, between
// sides 1 and 3 (mitcad#821).
ShapePtr end_block() { return extrude("F1", {rectangle(1, 0, 0, 50, 100)}, 0, 10); }

// Whether every face of the shape has a name, an area and a display mesh,
// and every edge a length; what is missing is printed.
bool named_meshed_and_whole(const char* what, const Shape& shape) {
  bool ok = true;
  // (Meshed on a copy: the mesh stays on the faces it is made on.)
  const ShapePtr copy = mitcad::geometry::detail::working_copy(shape);
  BRepMesh_IncrementalMesh mesh(copy->occt(), 0.05, false, 0.5);
  for (int i = 0; i < copy->face_count(); ++i) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(copy->face(i), props);
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> triangles = BRep_Tool::Triangulation(copy->face(i), location);
    const bool named = !copy->face_names(i).empty();
    const bool meshed = !triangles.IsNull() && triangles->NbTriangles() > 0;
    if (!named || !(props.Mass() > 1e-6) || !meshed) {
      std::fprintf(stderr, "%s: face %d (%s), area %.9g, %s\n", what, i,
                   named ? copy->face_names(i).front().c_str() : "no name", props.Mass(),
                   meshed ? "meshed" : "no mesh");
      ok = false;
    }
  }
  for (int i = 0; i < copy->edge_count(); ++i) {
    if (BRep_Tool::Degenerated(copy->edge(i))) {
      continue;
    }
    GProp_GProps props;
    BRepGProp::LinearProperties(copy->edge(i), props);
    if (!(props.Mass() > 1e-6)) {
      std::fprintf(stderr, "%s: edge %d (%s) of length %.9g\n", what, i, copy->edge_name(i).c_str(), props.Mass());
      ok = false;
    }
  }
  return ok;
}

void test_end_rounded_to_a_half_round_in_one_fillet() {
  // mitcad#821: both vertical edges of the block's 50 wide end rounded with
  // 25 in one fillet. The two roundings take the end face away and meet on
  // one line in its middle (a half round). The top and the bottom keep their
  // names, once each, and are meshed, as where the edges are rounded in two
  // fillets one after the other, or with 24.9, which leaves a strip of the
  // end face.
  const std::string left = edge(side("F1", 1, 0), side("F1", 1, 3));
  const std::string right = edge(side("F1", 1, 0), side("F1", 1, 1));
  const ShapePtr one = fillet("F3", *end_block(), {left, right}, 25.0);
  const ShapePtr two = fillet("F4", *fillet("F3", *end_block(), {left}, 25.0), {right}, 25.0);
  const ShapePtr apart = fillet("F3", *end_block(), {left, right}, 24.9);
  const std::vector<std::pair<const char*, ShapePtr>> cases = {{"one fillet", one}, {"two fillets", two},
                                                               {"radius 24.9", apart}};
  for (const auto& [what, rounded] : cases) {
    CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
    CHECK(rounded->notes().empty());
    CHECK(named_meshed_and_whole(what, *rounded));
    CHECK(faces_named(*rounded, end_cap("F1", 1)) == 1);
    CHECK(faces_named(*rounded, start_cap("F1", 1)) == 1);
    CHECK(edge_names_unique(*rounded));
    // (The caps hold their own names only.)
    for (int i = 0; i < rounded->face_count(); ++i) {
      const NameList& names = rounded->face_names(i);
      if (has_name(names, end_cap("F1", 1)) || has_name(names, start_cap("F1", 1))) {
        CHECK(names.size() == 1);
      }
    }
  }
  const double cap = 50.0 * 100.0 - 2.0 * fillet_loss(25.0);
  for (const ShapePtr& rounded : {one, two}) {
    CHECK_NEAR_TOL(named_face_area(*rounded, end_cap("F1", 1)), cap, 1e-6);
    CHECK_NEAR_TOL(named_face_area(*rounded, start_cap("F1", 1)), cap, 1e-6);
    CHECK_NEAR_TOL(volume(*rounded), cap * 10.0, 1e-6);
  }
  CHECK_NEAR_TOL(named_face_area(*apart, end_cap("F1", 1)), 50.0 * 100.0 - 2.0 * fillet_loss(24.9), 1e-6);
  // No end face left: four sides less the end, two caps, two roundings; the
  // two fillets give the same faces, 24.9 the strip besides.
  CHECK(one->face_count() == 7);
  CHECK(two->face_count() == one->face_count());
  CHECK(apart->face_count() == 8);
  CHECK(faces_named(*one, "F3:fillet(" + left + ")") == 1);
  CHECK(faces_named(*one, "F3:fillet(" + right + ")") == 1);
}

// Faces named `name` or one of its split pieces.
int faces_matching(const Shape& shape, const std::string& name) {
  int count = 0;
  for (int i = 0; i < shape.face_count(); ++i) {
    count += face_matches(shape.face_names(i), name) ? 1 : 0;
  }
  return count;
}

void test_dressups_of_circles_built_as_rings() {
  // A hole 0.5 mm from the block's side: bevelling or rounding its rim by
  // 1 mm runs over the side face. OCCT's chamfer runs on over it and the
  // side cuts its cone (the port's patch 0072); where OCCT cannot, the
  // section revolved about the hole's axis is cut away instead.
  const ShapePtr drill = extrude("F4", {circle(5, 5.5, 20, 5)}, -1, 21);
  const ShapePtr holed = boolean(BooleanOp::Cut, {block().get()}, *drill).pieces.at(0).shape;
  const std::string rim = edge(end_cap("F2", 1), "F4:side(c5)");
  ChamferSet bevel;
  bevel.edges = {rim};
  bevel.spec.distance = 1;
  const ShapePtr bevelled = chamfer("F5", *holed, {bevel});
  CHECK(mitcad::geometry::detail::is_valid(bevelled->occt()));
  CHECK(faces_matching(*bevelled, "F5:chamfer(" + rim + ")") >= 1);
  CHECK(bevelled->notes().empty());
  // What it takes away: the block's part of the cone the bevel bounds.
  const TopoDS_Shape cone =
      BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(5.5, 20, 19), gp_Dir(0, 0, 1)), 5, 6, 1).Shape();
  // (On a copy: the boolean adds p-curves to its arguments.)
  GProp_GProps props;
  BRepGProp::VolumeProperties(BRepAlgoAPI_Common(BRepBuilderAPI_Copy(holed->occt()).Shape(), cone).Shape(),
                              props);
  const double bevel_loss = volume(*holed) - volume(*bevelled);
  CHECK(near(bevel_loss, props.Mass(), 1e-6));
  // A rounding takes less than the bevel of its size.
  const ShapePtr rounded = fillet("F5", *holed, {rim}, 1);
  CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
  CHECK(faces_matching(*rounded, "F5:fillet(" + rim + ")") >= 1);
  const double round_loss = volume(*holed) - volume(*rounded);
  CHECK(round_loss > 0.3 * bevel_loss && round_loss < bevel_loss);
  // A boss 0.5 mm from the side: the rounding at its foot (concave) is
  // joined to it, over the edge of the top face.
  const ShapePtr post = extrude("F4", {circle(5, 5.5, 20, 5)}, 20, 30);
  const ShapePtr bossed = boolean(BooleanOp::Join, {block().get()}, *post).pieces.at(0).shape;
  const std::string foot = edge(end_cap("F2", 1), "F4:side(c5)");
  const ShapePtr footed = fillet("F5", *bossed, {foot}, 1);
  CHECK(mitcad::geometry::detail::is_valid(footed->occt()));
  CHECK(faces_matching(*footed, "F5:fillet(" + foot + ")") >= 1);
  CHECK(volume(*footed) > volume(*bossed));
  // Other edges keep OCCT's failure.
  CHECK(throws_with([&] { fillet("F5", *block(), {top_front()}, 60); }, "fillet failed"));
}

// A block with a post on top, the post's top rim given a p-curve on its top
// face 0.01 mm off its curve: an input that OCCT's checker rejects there.
ShapePtr block_with_faulty_post() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 60, 40, 20).Shape();
  const TopoDS_Shape post = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, 20, 20), gp_Dir(0, 0, 1)), 5, 5).Shape();
  const TopoDS_Shape body = BRepBuilderAPI_Copy(BRepAlgoAPI_Fuse(box, post).Shape()).Shape();
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane || std::abs(surface.Plane().Location().Z() - 25) > 1e-6) {
      continue;
    }
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& rim = TopoDS::Edge(e.Current());
      double first = 0;
      double last = 0;
      const occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(rim, face, first, last);
      const occ::handle<Geom2d_Curve> moved = occ::down_cast<Geom2d_Curve>(curve->Translated(gp_Vec2d(0.01, 0)));
      BRep_Builder().UpdateEdge(rim, moved, face, BRep_Tool::Tolerance(rim));
    }
  }
  return import_body(body, "F1", 0);
}

// The edge of a body between the points (x, y, any z) at both ends.
std::string vertical_edge_at(const Shape& body, double x, double y) {
  for (int i = 0; i < body.edge_count(); ++i) {
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(body.edge(i)));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(body.edge(i)));
    if (near(a.X(), x) && near(a.Y(), y) && near(b.X(), x) && near(b.Y(), y)) {
      return body.edge_name(i);
    }
  }
  return {};
}

void test_dressups_of_faulty_inputs() {
  // Stored bodies of imported designs are at times invalid in places
  // (mitcad#107). A rounding away from the fault is built and keeps it as
  // it was; it is not refused, nor the rest of the body healed.
  const ShapePtr body = block_with_faulty_post();
  CHECK(!mitcad::geometry::detail::is_valid(body->occt()));
  const std::string corner = vertical_edge_at(*body, 60, 0);
  CHECK(!corner.empty());
  const ShapePtr rounded = fillet("F5", *body, {corner}, 3);
  CHECK_NEAR(volume(*rounded), volume(*body) - fillet_loss(3) * 20);
  CHECK(faces_named(*rounded, "F5:fillet(" + corner + ")") == 1);
  CHECK(rounded->notes().empty());
  CHECK(!mitcad::geometry::detail::is_valid(rounded->occt()));
  // A fault next to the rounding is not taken over: the post's foot
  // rounded beside its faulty top comes out healed.
  std::string foot;
  for (int i = 0; i < body->edge_count(); ++i) {
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(body->edge(i)));
    BRepAdaptor_Curve curve(body->edge(i));
    if (curve.GetType() == GeomAbs_Circle && near(a.Z(), 20)) {
      foot = body->edge_name(i);
    }
  }
  CHECK(!foot.empty());
  const ShapePtr footed = fillet("F5", *body, {foot}, 1);
  CHECK(mitcad::geometry::detail::is_valid(footed->occt()));
  CHECK(volume(*footed) > volume(*body));
}

// A cone frustum (radii 3 at z = 0 and 1 at z = 2) as the .f3d import
// builds it from its stored B-rep, the top circle in two arcs whose curve
// runs clockwise seen from above, against the side's axis.
ShapePtr imported_frustum() {
  mitcad::brep::Body b;
  const double r0 = 3;
  const double r1 = 1;
  const double h = 2;
  const auto add = [&](int kind, const std::vector<double>& reals) {
    mitcad::brep::Geometry g;
    g.kind = kind;
    g.real_offset = static_cast<std::uint32_t>(b.reals.size());
    g.real_count = static_cast<std::uint32_t>(reals.size());
    b.reals.insert(b.reals.end(), reals.begin(), reals.end());
    return g;
  };
  const double slant = std::sqrt((r0 - r1) * (r0 - r1) + h * h);
  b.vertices = {r1, 0, h, 1e-4, -r1, 0, h, 1e-4, r0, 0, 0, 1e-4};
  b.curves.push_back(add(1, {0, 0, h, 0, 0, -1, 1, 0, 0, r1, r1}));                  // top circle
  b.curves.push_back(add(1, {0, 0, 0, 0, 0, 1, 1, 0, 0, r0, r0}));                   // bottom circle
  b.curves.push_back(add(0, {r0, 0, 0, (r1 - r0) / slant, 0, h / slant}));           // seam
  b.surfaces.push_back(add(1, {0, 0, 0, 0, 0, 1, 1, 0, 0, r0, -std::atan((r0 - r1) / h)}));
  b.surfaces.push_back(add(0, {0, 0, h, 0, 0, 1, 1, 0, 0}));
  b.surfaces.push_back(add(0, {0, 0, 0, 0, 0, -1, 1, 0, 0}));
  b.edges.push_back({0, 0, 1, 0.0, kPi, 1e-4});
  b.edges.push_back({0, 1, 0, kPi, 2 * kPi, 1e-4});
  b.edges.push_back({1, 2, 2, 0.0, 2 * kPi, 1e-4});
  b.edges.push_back({2, 2, 0, 0.0, slant, 1e-4});
  const auto loop = [&](const std::vector<mitcad::brep::Coedge>& coedges) {
    b.loops.push_back({static_cast<std::uint32_t>(b.coedges.size()), static_cast<std::uint32_t>(coedges.size())});
    b.coedges.insert(b.coedges.end(), coedges.begin(), coedges.end());
  };
  loop({{2, true}, {3, true}, {0, true}, {1, true}, {3, false}}); // side
  loop({{1, false}, {0, false}});                                // top
  loop({{2, false}});                                            // bottom
  for (std::uint32_t f = 0; f < 3; ++f) {
    mitcad::brep::Face face;
    face.surface = f;
    face.first_loop = f;
    face.loop_count = 1;
    b.faces.push_back(face);
    b.shell_faces.push_back(f);
  }
  b.shells.push_back({0, 0, 3, true});
  b.lump_count = 1;
  const mitcad::brep::BuildResult built = mitcad::brep::build_body(b);
  CHECK(built.report.built && built.report.valid);
  return import_body(built.shape, "F1", 0);
}

void test_fillets_of_circles_in_pieces() {
  // OCCT's fillet fails on the top rim of the imported frustum, a whole
  // circle in two arcs (mitcad#107); it rounds the rim once its pieces are
  // merged into one edge, and both pieces name the rounding.
  const ShapePtr frustum = imported_frustum();
  const std::vector<std::string> rim = {edge("F1:import(0)", "F1:import(1)") + "#0",
                                        edge("F1:import(0)", "F1:import(1)") + "#1"};
  CHECK(frustum->find_edges(rim[0]).size() == 1 && frustum->find_edges(rim[1]).size() == 1);
  {
    const ShapePtr copy = mitcad::geometry::detail::working_copy(*frustum);
    BRepFilletAPI_MakeFillet plain(copy->occt());
    for (const std::string& name : rim) {
      const int e = copy->find_edges(name).front();
      if (plain.Contour(copy->edge(e)) == 0) {
        plain.Add(0.5, copy->edge(e));
      }
    }
    plain.Build();
    CHECK(!plain.IsDone()); // why the pieces are merged; if OCCT learns it, this test can go
  }
  const ShapePtr rounded = fillet("F5", *frustum, rim, 0.5);
  CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
  CHECK(faces_matching(*rounded, "F5:fillet(" + rim[0] + ")") >= 1);
  CHECK(faces_matching(*rounded, "F5:fillet(" + rim[1] + ")") >= 1);
  // The same rounding as on OCCT's own frustum with a whole top circle.
  const TopoDS_Shape cone = BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 3, 1, 2).Shape();
  const ShapePtr whole = import_body(cone, "F1", 0);
  std::string top;
  for (int i = 0; i < whole->edge_count(); ++i) {
    BRepAdaptor_Curve curve(whole->edge(i));
    if (curve.GetType() == GeomAbs_Circle && near(curve.Value(curve.FirstParameter()).Z(), 2)) {
      top = whole->edge_name(i);
    }
  }
  CHECK_NEAR(volume(*rounded), volume(*fillet("F5", *whole, {top}, 0.5)));
}

void test_fillets_of_knife_edges() {
  // A disk with a cove round its rim tangent to both faces: the edges where
  // the cove meets them are knife edges (the faces' normals opposite).
  // OCCT's default tolerances find no start for the rolling ball there; it
  // builds the rounding with tighter ones (mitcad#107).
  const TopoDS_Shape disk = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 20, 3).Shape();
  const TopoDS_Shape cove = BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0, 0, 1.5), gp_Dir(0, 0, 1)), 20, 1.5).Shape();
  const ShapePtr body = import_body(BRepAlgoAPI_Cut(disk, cove).Shape(), "F1", 0);
  // The upper knife edge.
  int index = -1;
  for (int i = 0; i < body->edge_count(); ++i) {
    const std::vector<int> faces = mitcad::geometry::detail::faces_at_edge(*body, i);
    BRepAdaptor_Curve curve(body->edge(i));
    if (faces.size() == 2 && near(curve.Value(curve.FirstParameter()).Z(), 3) &&
        mitcad::geometry::detail::normal_angle(body->edge(i), body->face(faces[0]), body->face(faces[1])) > 3) {
      index = i;
    }
  }
  CHECK(index >= 0);
  const std::string knife = body->edge_name(index);
  {
    const ShapePtr copy = mitcad::geometry::detail::working_copy(*body);
    BRepFilletAPI_MakeFillet plain(copy->occt());
    plain.Add(0.5, copy->edge(index));
    plain.Build();
    CHECK(!plain.IsDone()); // why the parameters are tightened
  }
  const ShapePtr rounded = fillet("F5", *body, {knife}, 0.5);
  CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
  CHECK(faces_named(*rounded, "F5:fillet(" + knife + ")") == 1);
  CHECK(volume(*rounded) < volume(*body));
}

// A block 5 x 10 x 10 and, beside it, a wedge as high whose front face
// slopes forward (from y = 0 at the top to y = -3 at the bottom): the top
// front edges of the two run on in line, but the faces below them meet at
// an angle and two more sharp edges meet at the vertex between them.
ShapePtr block_and_sloped_wedge() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(5, 10, 10)).Shape();
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(5, -3, 0), gp_Pnt(5, 10, 0), gp_Pnt(5, 10, 10),
                                     gp_Pnt(5, 0, 10), true);
  const TopoDS_Shape wedge =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire()).Face(), gp_Vec(5, 0, 0)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(box, wedge).Shape(), true, true, true);
  unify.Build();
  return import_body(unify.Shape(), "F1", 0);
}

// The edge from (x0, 0, 10) to (x1, 0, 10); -1 for none.
int top_front_edge(const Shape& body, double x0, double x1) {
  for (int i = 0; i < body.edge_count(); ++i) {
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(body.edge(i)));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(body.edge(i)));
    const auto at = [](const gp_Pnt& p, double x) {
      return near(p.X(), x) && std::abs(p.Y()) < 1e-7 && near(p.Z(), 10);
    };
    if ((at(a, x0) && at(b, x1)) || (at(a, x1) && at(b, x0))) {
      return i;
    }
  }
  return -1;
}

void test_own_tangent_chains() {
  // mitcad#224: OCCT does not follow the chain from the block's top front
  // edge into the wedge's (a third sharp edge meets the vertex); Mitcad's
  // own chain does.
  const ShapePtr body = block_and_sloped_wedge();
  const int left = top_front_edge(*body, 0, 5);
  const int right = top_front_edge(*body, 5, 10);
  CHECK(left >= 0 && right >= 0);
  if (left < 0 || right < 0) {
    return;
  }
  std::vector<int> owner(static_cast<std::size_t>(body->edge_count()), -1);
  owner[static_cast<std::size_t>(left)] = 0;
  // Not for a set that does not follow chains.
  CHECK(detail::follow_tangent_chains(*body, owner, {0}) == 0);
  CHECK(owner[static_cast<std::size_t>(right)] == -1);
  CHECK(detail::follow_tangent_chains(*body, owner, {1}) == 1);
  CHECK(owner[static_cast<std::size_t>(right)] == 0);
  // The block's edges at right angles are no continuation.
  const ShapePtr plain = block();
  std::vector<int> alone(static_cast<std::size_t>(plain->edge_count()), -1);
  alone[static_cast<std::size_t>(plain->find_edges(top_front()).front())] = 0;
  CHECK(detail::follow_tangent_chains(*plain, alone, {1}) == 0);

  // A fillet and a chamfer of the one edge round and bevel both.
  const std::string first = body->edge_name(left);
  const std::string second = body->edge_name(right);
  FilletSet set;
  set.edges = {first};
  set.radius = 1;
  const ShapePtr rounded = fillet("F5", *body, {set});
  CHECK(faces_named(*rounded, "F5:fillet(" + first + ")") == 1);
  CHECK(faces_named(*rounded, "F5:fillet(" + second + ")") == 1);
  ChamferSpec spec;
  spec.distance = 1;
  const ShapePtr bevelled = chamfer("F5", *body, {first}, spec);
  CHECK(faces_named(*bevelled, "F5:chamfer(" + first + ")") == 1);
  // Where OCCT cannot build the chain it does not follow itself (it does
  // not bevel these two contours meeting end to end), the kernel's chain
  // alone, with a note.
  CHECK(faces_named(*bevelled, "F5:chamfer(" + second + ")") == 1 || !bevelled->notes().empty());
  // Without a tangent chain the one edge only.
  set.tangent_chain = false;
  const ShapePtr one = fillet("F5", *body, {set});
  CHECK(faces_named(*one, "F5:fillet(" + first + ")") == 1);
  CHECK(faces_named(*one, "F5:fillet(" + second + ")") == 0);
}

// A shaft of radius 2.4 from z = 2.5 with a flange of two 45 degree
// chamfers up to radius 2.9 and back, cut by the plane y = -1: with `band`
// a cylindrical band that high between the chamfers, with `ring` a flat
// ring that wide under the lower one (stored bodies hold such strips of
// half a micrometre where the design has one edge, mitcad#299).
ShapePtr flanged_shaft(double band, double ring) {
  const double r0 = 2.4, r1 = 2.9, z0 = 9.5;
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(0, 0, 2.5));
  profile.Add(gp_Pnt(r0, 0, 2.5));
  profile.Add(gp_Pnt(r0, 0, z0));
  if (ring > 0) {
    profile.Add(gp_Pnt(r0 + ring, 0, z0));
  }
  const double z1 = z0 + (r1 - r0 - ring);
  profile.Add(gp_Pnt(r1, 0, z1));
  if (band > 0) {
    profile.Add(gp_Pnt(r1, 0, z1 + band));
  }
  const double z2 = z1 + band + (r1 - r0);
  profile.Add(gp_Pnt(r0, 0, z2));
  profile.Add(gp_Pnt(0, 0, z2));
  profile.Close();
  const TopoDS_Shape turned = BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(profile.Wire(), true).Face(),
                                                    gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)))
                                  .Shape();
  const TopoDS_Shape cutter = BRepPrimAPI_MakeBox(gp_Pnt(-5, -1, 0), gp_Pnt(5, 5, 15)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(turned, cutter).Shape(), true, true, true);
  unify.Build();
  return import_body(unify.Shape(), "F1", 0);
}

// The edges of the flanged shaft's cut face above its foot, longer than
// 0.01: the shaft's sides, the chamfers' and the top's.
std::vector<int> flange_outline(const Shape& body) {
  std::vector<int> edges;
  for (int i = 0; i < body.edge_count(); ++i) {
    BRepAdaptor_Curve curve(body.edge(i));
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (near(a.Y(), -1) && near(b.Y(), -1) && near(m.Y(), -1) && a.Distance(b) > 0.01 &&
        (a.Z() > 2.6 || b.Z() > 2.6)) {
      edges.push_back(i);
    }
  }
  return edges;
}

// The areas of a shape's faces, smallest first.
std::vector<double> sorted_areas(const Shape& shape) {
  std::vector<double> areas;
  for (int f = 0; f < shape.face_count(); ++f) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(shape.face(f), props);
    areas.push_back(props.Mass());
  }
  std::sort(areas.begin(), areas.end());
  return areas;
}

void test_fillets_at_strips_within_the_tolerance() {
  // mitcad#299: the roundings of the chamfers' edges on the cut face end at
  // the two vertices of an edge of half a micrometre, the band's, and OCCT's
  // fillet fails there; within the import's tolerance the band is the edge
  // where the chamfers meet, and the roundings meet in one mitre there as
  // on the body without the band. Likewise the ring under the lower chamfer.
  const ShapePtr sharp = flanged_shaft(0, 0);
  const std::vector<int> sharp_edges = flange_outline(*sharp);
  CHECK(sharp_edges.size() == 7);
  std::vector<std::string> sharp_names;
  for (int e : sharp_edges) {
    sharp_names.push_back(sharp->edge_name(e));
  }
  const ShapePtr expected = fillet("F5", *sharp, sharp_names, 0.2);
  const double expected_change = accurate_volume(*expected) - accurate_volume(*sharp);
  for (const auto& [band, ring] : {std::pair{5e-4, 0.0}, std::pair{0.0, 5e-4}, std::pair{5e-4, 5e-4}}) {
    const ShapePtr body = flanged_shaft(band, ring);
    const std::vector<int> outline = flange_outline(*body);
    CHECK(outline.size() == 7);
    std::vector<std::string> names;
    for (int e : outline) {
      names.push_back(body->edge_name(e));
    }
    if (band > 0) {
      const ShapePtr copy = mitcad::geometry::detail::working_copy(*body);
      BRepFilletAPI_MakeFillet plain(copy->occt());
      for (int e : outline) {
        plain.Add(0.2, copy->edge(e));
      }
      plain.Build();
      CHECK(!plain.IsDone()); // why the band is collapsed; if OCCT learns it, this check can go
    }
    const ShapePtr rounded = fillet("F5", *body, names, 0.2);
    CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
    CHECK(rounded->notes().empty());
    for (const std::string& name : names) {
      CHECK(faces_named(*rounded, "F5:fillet(" + name + ")") == 1);
    }
    // The faces of the body without the strips: each rounding one face,
    // two meeting in a mitre at each chamfer's end, the material taken away
    // within the strips' width.
    CHECK(rounded->face_count() == expected->face_count());
    if (rounded->face_count() == expected->face_count()) {
      const std::vector<double> got = sorted_areas(*rounded);
      const std::vector<double> want = sorted_areas(*expected);
      for (std::size_t i = 0; i < got.size(); ++i) {
        CHECK_NEAR_TOL(got[i], want[i], 1e-3);
      }
    }
    CHECK_NEAR_TOL(accurate_volume(*rounded) - accurate_volume(*body), expected_change, 1e-3);
  }
}

void test_convex_and_concave_roundings_overlapping_on_a_step() {
  // mitcad#235: a step 0.5 high on the block's top (a 60 x 20 x 0.5 box
  // joined on its front half), both edges of the riser rounded 0.3. The
  // roundings overlap on the riser (0.6 > 0.5) and OCCT's fillet fails; the
  // convex rounding is built first, then the concave one, whose ball rolls
  // on the convex rounding: an S-shaped section, the riser gone.
  const ShapePtr step = extrude("F3", {rectangle(5, 0, 0, 60, 20)}, 20, 20.5);
  const ShapePtr body = boolean(BooleanOp::Join, {block().get()}, *step).pieces.at(0).shape;
  const std::string top = edge(end_cap("F3", 5), side("F3", 5, 2));
  const std::string foot = edge(end_cap("F2", 1), side("F3", 5, 2));
  {
    const ShapePtr copy = mitcad::geometry::detail::working_copy(*body);
    BRepFilletAPI_MakeFillet plain(copy->occt());
    plain.Add(0.3, copy->edge(copy->find_edges(top).at(0)));
    plain.Add(0.3, copy->edge(copy->find_edges(foot).at(0)));
    plain.Build();
    CHECK(!plain.IsDone()); // why the roundings are built in order; if OCCT learns it, this check can go
  }
  const ShapePtr rounded = fillet("F5", *body, {top, foot}, 0.3);
  CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
  CHECK(faces_named(*rounded, "F5:fillet(" + top + ")") == 1);
  CHECK(faces_named(*rounded, "F5:fillet(" + foot + ")") == 1);
  CHECK(faces_named(*rounded, side("F3", 5, 2)) == 0);
  // The section, the riser at y = 0 and its foot at z = 0: the convex arc
  // about (-r, h - r), the concave one about (y2, r) at 2r from it, the
  // arcs meeting half-way between their centres.
  const double r = 0.3, h = 0.5;
  const double y2 = -r + std::sqrt(4 * r * r - (2 * r - h) * (2 * r - h));
  const double yt = (y2 - r) / 2;
  const auto segment = [r](double u) { return (u * std::sqrt(r * r - u * u) + r * r * std::asin(u / r)) / 2; };
  const double change = (h - r) * (yt + r) + segment(yt + r) + r * (y2 - yt) + segment(yt - y2) - r * h;
  CHECK_NEAR_TOL(accurate_volume(*rounded), 48000.0 + 60 * 20 * h + change * 60, 1e-6);
}

// A ring wall between radii 112 and 115 with a window from z = -20 to -5
// between two pillars whose ends are full rounds (half cylinders of radius
// 1.5 round the wall's middle, tangent to both walls); with `halves`, each
// end is two faces of that cylinder meeting at its tip. The faces are named
// W<k>.
ShapePtr window_wall(bool halves) {
  const double r1 = 112, r2 = 115, rm = 113.5, rc = 1.5;
  const double phi = std::atan2(16.5, std::sqrt(rm * rm - 16.5 * 16.5));
  const gp_Ax2 axis(gp_Pnt(0, 0, -30), gp_Dir(0, 0, 1));
  TopoDS_Shape ring = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(axis, r2, 30).Shape(),
                                      BRepPrimAPI_MakeCylinder(axis, r1, 30).Shape())
                          .Shape();
  const gp_Ax2 at(gp_Pnt(0, 0, -20), gp_Dir(0, 0, 1), gp_Dir(std::sin(phi), std::cos(phi), 0));
  TopoDS_Shape window = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(at, r2 + 1, 15, 2 * phi).Shape(),
                                        BRepPrimAPI_MakeCylinder(at, r1 - 1, 15, 2 * phi).Shape())
                            .Shape();
  for (const int s : {-1, 1}) {
    const gp_Pnt centre(rm * std::sin(s * phi), rm * std::cos(phi), -20);
    const gp_Dir towards(-s, 0, 0);
    if (!halves) {
      // (Its seam inside the pillar.)
      const gp_Ax2 whole(centre, gp_Dir(0, 0, 1), towards.Reversed());
      window = BRepAlgoAPI_Cut(window, BRepPrimAPI_MakeCylinder(whole, rc, 15).Shape()).Shape();
      continue;
    }
    for (const gp_Dir& start : {towards, towards.Reversed()}) {
      const gp_Ax2 half(centre, gp_Dir(0, 0, 1), start);
      window = BRepAlgoAPI_Cut(window, BRepPrimAPI_MakeCylinder(half, rc, 15, kPi).Shape()).Shape();
    }
  }
  // With halves, the window cut in three pieces at x = +-15, which leaves
  // the floor's and the ceiling's edges with the walls in three pieces, as
  // in the stored design (with them whole, OCCT builds the rounding since
  // the port's patch 0950).
  TopoDS_Shape cut = ring;
  if (halves) {
    const TopoDS_Shape middle = BRepPrimAPI_MakeBox(gp_Pnt(-15, 0, -40), gp_Pnt(15, 200, 10)).Shape();
    cut = BRepAlgoAPI_Cut(cut, BRepAlgoAPI_Common(window, middle).Shape()).Shape();
    cut = BRepAlgoAPI_Cut(cut, BRepAlgoAPI_Cut(window, middle).Shape()).Shape();
  } else {
    cut = BRepAlgoAPI_Cut(ring, window).Shape();
  }
  // The cut's faces of one surface joined, the halves and the walls kept
  // apart at x = +-15.
  ShapeUpgrade_UnifySameDomain keep(cut, true, true, true);
  for (TopExp_Explorer ex(cut, TopAbs_EDGE); ex.More(); ex.Next()) {
    BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
    if (c.GetType() == GeomAbs_Line && std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9 &&
        std::abs(std::abs(a.X()) - 15.0) < 1e-6) {
      keep.KeepShape(ex.Current());
    }
  }
  keep.Build();
  std::vector<Shape::NamedFace> faces;
  int k = 0;
  for (TopExp_Explorer ex(keep.Shape(), TopAbs_FACE); ex.More(); ex.Next(), ++k) {
    faces.push_back({ex.Current(), {"W" + std::to_string(k)}});
  }
  return std::make_shared<Shape>(keep.Shape(), faces);
}

// The edges of the window's floor and ceiling with the walls.
std::vector<std::string> window_edges(const Shape& wall) {
  std::vector<std::string> names;
  for (int e = 0; e < wall.edge_count(); ++e) {
    BRepAdaptor_Curve c(wall.edge(e));
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    const double r = std::hypot(m.X(), m.Y());
    const bool sill = std::abs(m.Z() + 20) < 1e-6 || std::abs(m.Z() + 5) < 1e-6;
    if (sill && c.GetType() == GeomAbs_Circle && m.Y() > 100 &&
        (std::abs(r - 112) < 1e-6 || std::abs(r - 115) < 1e-6)) {
      names.push_back(wall.edge_name(e));
    }
  }
  return names;
}

void test_window_floor_rounded_up_to_pillar_ends_in_halves() {
  // mitcad#343: the floor of a window in a ring wall 3 thick rounded with
  // 1.5 on both edges, a torus that takes the floor away, ending at the
  // pillars' full-rounded ends. Where each end is two faces of its cylinder,
  // OCCT's corner cuts the rounding's end by one of them only and fails; the
  // faces of one surface at the rounded edges' vertices are joined first, as
  // the bodies stored in the files have them, and the rounding is the one
  // made where each end is one face.
  const ShapePtr whole = window_wall(false);
  const ShapePtr halved = window_wall(true);
  CHECK(halved->face_count() == whole->face_count() + 2);
  const std::vector<std::string> whole_edges = window_edges(*whole);
  const std::vector<std::string> halved_edges = window_edges(*halved);
  CHECK(whole_edges.size() == 4);
  CHECK(halved_edges.size() == 4);
  const ShapePtr expected = fillet("F9", *whole, whole_edges, 1.5);
  CHECK(mitcad::geometry::detail::is_valid(expected->occt()));
  const double expected_change = accurate_volume(*expected) - accurate_volume(*whole);
  {
    const ShapePtr copy = mitcad::geometry::detail::working_copy(*halved);
    BRepFilletAPI_MakeFillet plain(copy->occt());
    for (const std::string& name : halved_edges) {
      plain.Add(1.5, copy->edge(copy->find_edges(name).at(0)));
    }
    plain.Build();
    // (Why the halves are joined; if OCCT learns it, this check can go.)
    CHECK(!plain.IsDone() || !mitcad::geometry::detail::is_valid(plain.Shape()));
  }
  const ShapePtr rounded = fillet("F9", *halved, halved_edges, 1.5);
  CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
  CHECK_NEAR_TOL(accurate_volume(*rounded) - accurate_volume(*halved), expected_change, 1e-4);
  CHECK(rounded->face_count() == expected->face_count());
}

} // namespace

void dressup_tests() {
  test_fillet_volume_and_names();
  test_fillet_corner_blends_are_named();
  test_fillet_follows_a_dimension_change();
  test_several_fillets_on_one_body();
  test_fillet_after_booleans();
  test_chamfers();
  guarded("test_chamfer_failure_names_its_edges_and_ends", test_chamfer_failure_names_its_edges_and_ends);
  guarded("test_fillet_edge_sets", test_fillet_edge_sets);
  guarded("test_fillet_faces_select_their_edges", test_fillet_faces_select_their_edges);
  guarded("test_fillet_tangent_chains", test_fillet_tangent_chains);
  guarded("test_fillet_chord_length_and_variable_radius", test_fillet_chord_length_and_variable_radius);
  guarded("test_fillet_mid_radii", test_fillet_mid_radii);
  guarded("test_fillet_spline_radius", test_fillet_spline_radius);
  guarded("test_chamfer_miter_corners", test_chamfer_miter_corners);
  guarded("test_asymmetric_fillets", test_asymmetric_fillets);
  guarded("test_g2_fillets", test_g2_fillets);
  guarded("test_fillet_corner_types", test_fillet_corner_types);
  guarded("test_chamfer_sets_and_reference_faces", test_chamfer_sets_and_reference_faces);
  guarded("test_angle_chamfer_measures_one_face_along_a_chain", test_angle_chamfer_measures_one_face_along_a_chain);
  guarded("test_dressups_that_take_a_face_away", test_dressups_that_take_a_face_away);
  guarded("test_end_rounded_to_a_half_round_in_one_fillet", test_end_rounded_to_a_half_round_in_one_fillet);
  guarded("test_dressups_of_circles_built_as_rings", test_dressups_of_circles_built_as_rings);
  guarded("test_dressups_of_faulty_inputs", test_dressups_of_faulty_inputs);
  guarded("test_own_tangent_chains", test_own_tangent_chains);
  guarded("test_fillets_of_circles_in_pieces", test_fillets_of_circles_in_pieces);
  guarded("test_fillets_of_knife_edges", test_fillets_of_knife_edges);
  guarded("test_fillets_at_strips_within_the_tolerance", test_fillets_at_strips_within_the_tolerance);
  guarded("test_convex_and_concave_roundings_overlapping_on_a_step",
          test_convex_and_concave_roundings_overlapping_on_a_step);
  guarded("test_window_floor_rounded_up_to_pillar_ends_in_halves",
          test_window_floor_rounded_up_to_pillar_ends_in_halves);
}

} // namespace test
