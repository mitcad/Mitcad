// SPDX-License-Identifier: MIT
// Fillets and chamfers that take a face away (mitcad#1106). A step 2 high
// whose top edge is bevelled or rounded by 2, a wall 2 thick whose inner top
// edge is bevelled by 2, and a tray whose walls are 2 thick and 2 high, its
// inner top edges bevelled or rounded by 2: each is built at its size, the
// faces it takes away gone, the dressup meeting the faces beyond at one edge
// each, its volume the exact shape's and no note; a later rounding of 1.5
// along the edge where the bevel meets the outer wall builds, named also at
// the tray's corners, and the tray's four outer top edges rounded together
// build too, mitred at the corners (mitcad#1118). The tray's
// corners, where two bevels take both walls' inner faces away and meet at
// the floor's corner, are the case behind the patch 1100 of the OCCT port
// (third_party/vcpkg-ports): checked with OCCT's dressups alone too. A
// blind hole's rim bevelled or rounded as deep as the hole is built at its
// size too, the hole's wall gone, with no note or info: the case behind the
// port's patch 1110 (mitcad#1117), checked with OCCT's dressups alone too.
// A wedge of 45 degrees whose knife edge is rounded so that the rounding
// takes a face 2.4 times its radius wide away leaves no strip (mitcad#1150).
// The feet of an octagon prism standing on a disc with its corners on the
// disc's rim, chamfered as wide as the segments of the disc's top beside
// them (and a little wider or narrower), take those segments away, cut by
// the rim (mitcad#1783). A block stepped in its section, its edges beside
// the side faces rounded as large as those faces are high or deep, so that
// each rounding ends exactly on a side face's far edge, takes the side
// faces away at the exact size, its volume exact and no note (GitHub #353).
// A bore's top rim chamfered from one end wall of a shelf standing out from
// the bore's wall to the other ends on the end walls' planes (GitHub #353's
// owner case of mitcad#1819). A wall's top rounded along both its edges
// with two radii that together are wider than the top: the smaller ball
// rides tangent on the larger rounding and the top goes (mitcad#1926).
// The same wall's top, a strip 2 wide closed off at its end by a face at 45
// degrees, rounded along the outer wall with 3: the strip goes, the ball
// rolling on the inner wall's edge, cut by the slope (mitcad#1926).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "../src/dressup_strips.hpp"
#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-10, true);
  return props.Mass();
}

double volume_of(const Shape& shape) { return volume_of(shape.occt()); }

// The name of the edge whose middle is at p (empty when none is).
std::string edge_at(const Shape& shape, const gp_Pnt& p) {
  for (int i = 0; i < shape.edge_count(); ++i) {
    if (BRep_Tool::Degenerated(shape.edge(i))) {
      continue;
    }
    const BRepAdaptor_Curve curve(shape.edge(i));
    if (curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p) < 1e-6) {
      return shape.edge_name(i);
    }
  }
  return {};
}

// The narrowest face's width: twice its area over its boundary's length.
double narrowest_face(const Shape& shape) {
  double narrowest = 1e300;
  for (int f = 0; f < shape.face_count(); ++f) {
    GProp_GProps area;
    BRepGProp::SurfaceProperties(shape.face(f), area);
    double boundary = 0.0;
    for (TopExp_Explorer ex(shape.face(f), TopAbs_EDGE); ex.More(); ex.Next()) {
      if (!BRep_Tool::Degenerated(TopoDS::Edge(ex.Current()))) {
        GProp_GProps length;
        BRepGProp::LinearProperties(ex.Current(), length);
        boundary += length.Mass();
      }
    }
    narrowest = std::min(narrowest, 2.0 * area.Mass() / boundary);
  }
  return narrowest;
}

bool valid(const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); }

// Whether every face has a name (printing those that have none).
bool all_faces_named(const Shape& shape) {
  bool all = true;
  for (int f = 0; f < shape.face_count(); ++f) {
    if (shape.face_names(f).empty()) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(shape.face(f), props);
      const gp_Pnt c = props.CentreOfMass();
      std::fprintf(stderr, "face %d (area %g, centre %g %g %g) has no name\n", f, props.Mass(), c.X(), c.Y(), c.Z());
      all = false;
    }
  }
  return all;
}

// Which path built a rounding of one edge, printed where its naming fails:
// OCCT's fillet alone (its result valid), OCCT's fillet repaired by shape
// healing (its result invalid), or another (OCCT's fillet fails: a retry,
// the mitred build, the cut-outs). Run on a copy of the body.
void report_rounding_path(const Shape& body, const std::string& edge_name, double radius) {
  const std::vector<int> found = body.find_edges(edge_name);
  if (found.size() != 1) {
    std::fprintf(stderr, "rounding path: edge %s found %zu times\n", edge_name.c_str(), found.size());
    return;
  }
  try {
    OCC_CATCH_SIGNALS
    BRepBuilderAPI_Copy copy(body.occt());
    const TopoDS_Edge edge = TopoDS::Edge(copy.ModifiedShape(body.edge(found.front())));
    BRepFilletAPI_MakeFillet maker(copy.Shape());
    maker.Add(radius, edge);
    maker.Build();
    if (!maker.IsDone()) {
      std::fprintf(stderr, "rounding path: OCCT's fillet fails (a retry, the mitred build or the cut-outs)\n");
      return;
    }
    int generated = 0;
    for (const TopoDS_Shape& image : maker.Generated(edge)) {
      generated += image.ShapeType() == TopAbs_FACE ? 1 : 0;
    }
    int faces = 0;
    for (TopExp_Explorer it(maker.Shape(), TopAbs_FACE); it.More(); it.Next()) {
      ++faces;
    }
    std::fprintf(stderr, "rounding path: OCCT's fillet %s; %d faces, %d of them generated by the edge\n",
                 BRepCheck_Analyzer(maker.Shape()).IsValid() ? "alone (valid)" : "+ healed (invalid)", faces,
                 generated);
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "rounding path: OCCT's fillet throws: %s\n", e.what());
  }
}

// The faces holding the name or one of its pieces (name#k).
int faces_holding(const Shape& shape, const std::string& name) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    count += face_matches(shape.face_names(f), name) ? 1 : 0;
  }
  return count;
}

// A block 30 x 20 x 5 with a step 2 high: x = 10 to 30 cut down to z = 3.
ShapePtr step() {
  const ShapePtr base = extrude("F1", {rectangle(1, 0, 0, 30, 20)}, 0, 5);
  const ShapePtr cut = extrude("F2", {rectangle(5, 10, -1, 30, 22)}, 3, 8);
  return boolean(BooleanOp::Cut, {base.get()}, *cut).pieces.at(0).shape;
}

// A block 30 x 20 x 7 cut back to a wall 2 thick at x = 0 to 2 above
// z = 3 (an L in section along y): its top face 2 wide, its inner face 4
// high.
ShapePtr wall() {
  const ShapePtr base = extrude("F1", {rectangle(1, 0, 0, 30, 20)}, 0, 7);
  const ShapePtr cut = extrude("F2", {rectangle(5, 2, -1, 30, 22)}, 3, 8);
  return boolean(BooleanOp::Cut, {base.get()}, *cut).pieces.at(0).shape;
}

// A block 30 x 20 x 7 with a pocket 26 x 16 from z = 5: a tray whose walls
// are 2 thick and 2 high.
ShapePtr tray() {
  const ShapePtr base = extrude("F1", {rectangle(1, 0, 0, 30, 20)}, 0, 7);
  const ShapePtr cut = extrude("F2", {rectangle(5, 2, 2, 26, 16)}, 5, 8);
  return boolean(BooleanOp::Cut, {base.get()}, *cut).pieces.at(0).shape;
}

// The tray's inner top edges.
std::vector<std::string> tray_edges(const Shape& tray) {
  return {edge_at(tray, gp_Pnt(2, 10, 7)), edge_at(tray, gp_Pnt(28, 10, 7)), edge_at(tray, gp_Pnt(15, 2, 7)),
          edge_at(tray, gp_Pnt(15, 18, 7))};
}

void test_step() {
  const ShapePtr body = step();
  const std::string top = edge_at(*body, gp_Pnt(10, 10, 5));
  CHECK(!top.empty());
  const double block = 10.0 * 20 * 5 + 20.0 * 20 * 3;
  ChamferSet bevel;
  bevel.edges = {top};
  bevel.spec.distance = 2;
  const ShapePtr bevelled = chamfer("F3", *body, {bevel});
  CHECK(valid(*bevelled));
  CHECK(narrowest_face(*bevelled) > 0.01);
  CHECK_NEAR_TOL(volume_of(*bevelled), block - 0.5 * 2 * 2 * 20, 1e-6);
  CHECK(bevelled->notes().empty() && bevelled->infos().empty());
  const ShapePtr rounded = fillet("F3", *body, {top}, 2);
  CHECK(valid(*rounded));
  CHECK(narrowest_face(*rounded) > 0.01);
  CHECK_NEAR_TOL(volume_of(*rounded), block - fillet_loss(2) * 20, 1e-6);
  CHECK(rounded->notes().empty() && rounded->infos().empty());
}

void test_wall_then_a_rounding_along_the_outer_wall() {
  const ShapePtr body = wall();
  ChamferSet bevel;
  bevel.edges = {edge_at(*body, gp_Pnt(2, 10, 7))};
  bevel.spec.distance = 2;
  const ShapePtr bevelled = chamfer("F3", *body, {bevel});
  CHECK(valid(*bevelled));
  CHECK(narrowest_face(*bevelled) > 0.01);
  CHECK_NEAR_TOL(volume_of(*bevelled), 30.0 * 20 * 3 + 2.0 * 20 * 4 - 0.5 * 2 * 2 * 20, 1e-6);
  CHECK(bevelled->notes().empty() && bevelled->infos().empty());
  // The bevel meets the outer wall at one edge, which keeps the name of the
  // top face's edge there: the bevel's face holds the top face's names.
  const std::string outer = edge(end_cap("F1", 1), side("F1", 1, 3));
  CHECK(edge_at(*bevelled, gp_Pnt(0, 10, 7)) == outer);
  // The bevel leaves a wedge of 45 degrees there, too narrow for the ball:
  // it rolls on the bevel's far edge at (2, 5), the bevel's face gone, its
  // axis 1.5 from the outer wall and from that edge, at z = 5 - sqrt(2).
  // What it takes away of the wedge: the wedge above the axis less the
  // cylinder's part between the outer wall and that edge.
  const ShapePtr rounded = fillet("F4", *bevelled, {outer}, 1.5);
  CHECK(valid(*rounded));
  CHECK(faces_named(*rounded, "F4:fillet(" + outer + ")") == 1);
  const double r = 1.5;
  const double axis = 5 - std::sqrt(2.0);
  const auto F = [r](double u) { return 0.5 * u * std::sqrt(r * r - u * u) + 0.5 * r * r * std::asin(u / r); };
  const double wedge = 2.0 * (7 - axis) - 2.0;
  const double under = F(0.5) - F(-r);
  CHECK_NEAR_TOL(volume_of(*rounded), volume_of(*bevelled) - 20.0 * (wedge - under), 1e-6);
}

void test_tray() {
  const ShapePtr body = tray();
  const std::vector<std::string> edges = tray_edges(*body);
  for (const std::string& e : edges) {
    CHECK(!e.empty());
  }
  // The walls' volume less the bevels': each a prism of 2 mm2 along its
  // centroid's path, 2/3 outside the pocket.
  const double walls = 30.0 * 20 * 5 + (30.0 * 20 - 26.0 * 16) * 2;
  const double path = 2.0 * (26 + 16) + 8.0 * 2 / 3;
  ChamferSet bevel;
  bevel.edges = edges;
  bevel.spec.distance = 2;
  const ShapePtr bevelled = chamfer("F3", *body, {bevel});
  CHECK(valid(*bevelled));
  CHECK(narrowest_face(*bevelled) > 0.01);
  CHECK_NEAR_TOL(volume_of(*bevelled), walls - 2.0 * path, 1e-6);
  CHECK(bevelled->notes().empty() && bevelled->infos().empty());
  CHECK(faces_named(*bevelled, "F3:chamfer(" + edges[0] + ")") == 1);
  // A rounding of 1.5 where a bevel meets the outer wall, its ball rolling
  // on the bevel's far edge as on the wall, here ending at the corners
  // where the bevels meet: it takes away less than along the whole side
  // (20 mm of the wall's 2.33 mm2). The top face meets the edges of all
  // four bevels, each the inner face of one: the bevel's face holds that
  // inner face's names (as name_consumed_faces gives them), and the edge
  // is named after it and the outer wall.
  const std::string outer = edge_at(*bevelled, gp_Pnt(0, 10, 7));
  CHECK(outer == edge(side("F1", 1, 3), side("F2", 5, 3)));
  const ShapePtr rounded = fillet("F4", *bevelled, {outer}, 1.5);
  CHECK(valid(*rounded));
  const double taken = volume_of(*bevelled) - volume_of(*rounded);
  CHECK(taken > 0.5 * 20 * 2.33 && taken < 20 * 2.33);
  CHECK(rounded->notes().empty() && rounded->infos().empty());
  // Named like any rounding, also where OCCT's result at the corners needed
  // the repair (mitcad#1118): every face named, the rounding's faces after
  // the edge.
  const bool all_named = all_faces_named(*rounded);
  const bool fillet_named = faces_holding(*rounded, "F4:fillet(" + outer + ")") >= 1;
  CHECK(all_named);
  CHECK(fillet_named);
  if (!all_named || !fillet_named) {
    report_rounding_path(*bevelled, outer, 1.5);
  }
  // Rounded by 2: each rounding's section 4 - pi mm2, its centroid
  // 2 (10 - 3 pi) / (3 (4 - pi)) outside the pocket.
  FilletSet round;
  round.edges = edges;
  round.radius = 2;
  const ShapePtr full = fillet("F3", *body, {round});
  CHECK(valid(*full));
  CHECK(narrowest_face(*full) > 0.01);
  const double centroid = 2.0 * (10 - 3 * kPi) / (3 * (4 - kPi));
  CHECK_NEAR_TOL(volume_of(*full), walls - fillet_loss(2) * (2.0 * (26 + 16) + 8.0 * centroid), 1e-6);
  CHECK(full->notes().empty() && full->infos().empty());
}

// The tray's walls bevelled by 2 all across, then all four outer top edges
// rounded by 1.5 (mitcad#1118). Each rounding takes its bevel away, its
// ball rolling on the bevel's lower edge (the pocket's rim) as along the
// straight wall: a cylinder of radius r = 1.5 whose axis is 1.5 inside the
// outer wall at z = a = 5 - sqrt(2), from its contact with the outer wall
// at z = a to the rim. At the corners, where OCCT cannot join two
// roundings at a vertex where two more sharp edges meet, the two cylinders
// meet on the plane halfway between the sides (a mitre).
//
// The volume: below z = a the whole block, 30 x 20 x a; above it the
// pocket's floor region, 26 x 16 x (5 - a), and the walls, whose material
// at distance t from the outer wall reaches h(t) = sqrt(r^2 - (t - 1.5)^2)
// above a. The mitres cut the walls into four trapezoids, two 20 - 2t long
// at t and two 30 - 2t: the walls hold the integral of h(t) (100 - 8t)
// from t = 0 to 2, that is 100 I0 - 8 I1 with (u = t - 1.5)
//   I0 = integral of sqrt(r^2 - u^2) from -1.5 to 0.5 = F(0.5) - F(-1.5),
//   F(u) = u sqrt(r^2 - u^2) / 2 + r^2 asin(u / r) / 2,
//   I1 = integral of (u + 1.5) sqrt(r^2 - u^2)
//      = -(r^2 - 0.5^2)^(3/2) / 3 + 1.5 I0.
void test_tray_rounded_all_round() {
  const ShapePtr body = tray();
  ChamferSet bevel;
  bevel.edges = tray_edges(*body);
  bevel.spec.distance = 2;
  const ShapePtr bevelled = chamfer("F3", *body, {bevel});
  CHECK(valid(*bevelled));
  const std::vector<std::string> outer = {edge_at(*bevelled, gp_Pnt(0, 10, 7)), edge_at(*bevelled, gp_Pnt(30, 10, 7)),
                                          edge_at(*bevelled, gp_Pnt(15, 0, 7)), edge_at(*bevelled, gp_Pnt(15, 20, 7))};
  for (const std::string& e : outer) {
    CHECK(!e.empty());
  }
  FilletSet round;
  round.edges = outer;
  round.radius = 1.5;
  const ShapePtr rounded = fillet("F4", *bevelled, {round});
  CHECK(valid(*rounded));
  CHECK(all_faces_named(*rounded));
  CHECK(edge_names_unique(*rounded));
  for (const std::string& e : outer) {
    CHECK(faces_named(*rounded, "F4:fillet(" + e + ")") == 1);
  }
  // The bevels are gone.
  for (const std::string& e : bevel.edges) {
    CHECK(faces_holding(*rounded, "F3:chamfer(" + e + ")") == 0);
  }
  CHECK(rounded->notes().empty() && rounded->infos().empty());
  const double r = 1.5;
  const double a = 5 - std::sqrt(2.0);
  const auto F = [r](double u) { return 0.5 * u * std::sqrt(r * r - u * u) + 0.5 * r * r * std::asin(u / r); };
  const double i0 = F(0.5) - F(-r);
  const double i1 = -std::pow(r * r - 0.25, 1.5) / 3 + 1.5 * i0;
  const double exact = 30.0 * 20 * a + 26.0 * 16 * (5 - a) + 100 * i0 - 8 * i1;
  CHECK_NEAR_TOL(volume_of(*rounded), exact, 1e-6);
}

// The tray's corner with OCCT's dressups alone: two adjacent inner top
// edges, each bevel or rounding taking its wall's inner face away. The
// corner failed (TopOpeBRepDS_DataStructure::Point) before the patch 1100.
void test_tray_corner_with_occt_alone() {
  const ShapePtr body = tray();
  const TopoDS_Edge a = body->edge(body->find_edges(edge_at(*body, gp_Pnt(2, 10, 7))).at(0));
  const TopoDS_Edge b = body->edge(body->find_edges(edge_at(*body, gp_Pnt(15, 2, 7))).at(0));
  for (int kind = 0; kind < 2; ++kind) {
    bool done = false;
    try {
      OCC_CATCH_SIGNALS
      if (kind == 0) {
        BRepFilletAPI_MakeChamfer maker(body->occt());
        maker.Add(2.0, a);
        maker.Add(2.0, b);
        maker.Build();
        done = maker.IsDone() && BRepCheck_Analyzer(maker.Shape()).IsValid();
      } else {
        BRepFilletAPI_MakeFillet maker(body->occt());
        maker.Add(2.0, a);
        maker.Add(2.0, b);
        maker.Build();
        done = maker.IsDone() && BRepCheck_Analyzer(maker.Shape()).IsValid();
      }
    } catch (const Standard_Failure& e) {
      std::fprintf(stderr, "tray corner: %s\n", e.what());
    }
    if (!done) {
      std::fprintf(stderr, "tray corner: the %s fails\n", kind == 0 ? "chamfer" : "fillet");
    }
    CHECK(done);
  }
}

// A plate 40 x 40 x 3 with a blind hole of radius 5, 2 deep.
ShapePtr blind_hole() {
  const ShapePtr plate = extrude("F1", {rectangle(1, -20, -20, 40, 40)}, 0, 3);
  const ShapePtr drill = extrude("F2", {circle(5, 0, 0, 5)}, 1, 4);
  return boolean(BooleanOp::Cut, {plate.get()}, *drill).pieces.at(0).shape;
}

// The blind hole's rim (the circle's middle, half a turn from where it
// starts).
std::string hole_rim(const Shape& body) { return edge_at(body, gp_Pnt(-5, 0, 3)); }

// What the rim's bevel of 2 takes away: a ring of 2 mm2 at 5 + 2/3 from the
// axis; the rounding of 2: 4 - pi mm2 at 5 + 2 (10 - 3 pi) / (3 (4 - pi)).
double hole_rim_ring(bool bevel) {
  return bevel ? 2.0 * 2 * kPi * (5 + 2.0 / 3)
               : fillet_loss(2) * 2 * kPi * (5 + 2.0 * (10 - 3 * kPi) / (3 * (4 - kPi)));
}

// The blind hole's rim bevelled or rounded by 2, which takes the hole's wall
// away, its contact on the wall along the wall's bottom edge: at its size,
// the volume the exact shape's, no note or info. (Before the port's patch
// 1110 OCCT built it only 0.1 % smaller: the strip that left was removed
// and an info said so.)
void test_hole_rim_at_its_size() {
  const ShapePtr body = blind_hole();
  const std::string rim = hole_rim(*body);
  CHECK(!rim.empty());
  const double holed = volume_of(*body);
  const std::string wall = "F2:side(c5)";
  CHECK(faces_named(*body, wall) == 1);
  for (int kind = 0; kind < 2; ++kind) {
    ShapePtr out;
    if (kind == 0) {
      ChamferSet bevel;
      bevel.edges = {rim};
      bevel.spec.distance = 2;
      out = chamfer("F3", *body, {bevel});
    } else {
      out = fillet("F3", *body, {rim}, 2);
    }
    const std::string what = kind == 0 ? "chamfer" : "fillet";
    CHECK(valid(*out));
    CHECK(narrowest_face(*out) > 0.01);
    CHECK(out->notes().empty());
    CHECK(out->infos().empty());
    for (const std::string& info : out->infos()) {
      std::fprintf(stderr, "hole rim %s: %s\n", what.c_str(), info.c_str());
    }
    CHECK_NEAR_TOL(volume_of(*out), holed - hole_rim_ring(kind == 0), 1e-6);
    // The dressup's face holds the names of the hole's wall it took away.
    const std::string face = "F3:" + what + "(" + rim + ")";
    CHECK(faces_named(*out, face) == 1);
    for (int f = 0; f < out->face_count(); ++f) {
      if (has_name(out->face_names(f), face)) {
        CHECK(has_name(out->face_names(f), wall));
      }
    }
  }
}

// The blind hole's rim with OCCT's dressups alone: the chamfer and the
// fillet of 2 build, valid, with the exact volume. The contact on the wall
// lies on its bottom edge, a closed edge of the cylinder; both ends of its
// one piece are snapped onto the seam's vertex, and the first one got the
// second one's parameter (no piece: the walk after failed) before the
// patch 1110.
void test_hole_rim_with_occt_alone() {
  const ShapePtr body = blind_hole();
  const std::vector<int> found = body->find_edges(hole_rim(*body));
  CHECK(found.size() == 1);
  if (found.size() != 1) {
    return;
  }
  const TopoDS_Edge rim = body->edge(found.front());
  const double holed = volume_of(*body);
  for (int kind = 0; kind < 2; ++kind) {
    bool done = false;
    double volume = 0.0;
    try {
      OCC_CATCH_SIGNALS
      if (kind == 0) {
        BRepFilletAPI_MakeChamfer maker(body->occt());
        maker.Add(2.0, rim);
        maker.Build();
        done = maker.IsDone() && BRepCheck_Analyzer(maker.Shape()).IsValid();
        volume = done ? volume_of(maker.Shape()) : 0.0;
      } else {
        BRepFilletAPI_MakeFillet maker(body->occt());
        maker.Add(2.0, rim);
        maker.Build();
        done = maker.IsDone() && BRepCheck_Analyzer(maker.Shape()).IsValid();
        volume = done ? volume_of(maker.Shape()) : 0.0;
      }
    } catch (const Standard_Failure& e) {
      std::fprintf(stderr, "hole rim: %s\n", e.what());
    }
    if (!done) {
      std::fprintf(stderr, "hole rim: the %s fails\n", kind == 0 ? "chamfer" : "fillet");
    }
    CHECK(done);
    if (done) {
      CHECK_NEAR_TOL(volume, holed - hole_rim_ring(kind == 0), 1e-6);
    }
  }
}

// A region of straight lines c<first>.. through the points (closed).
Region polygon(int first, const std::vector<gp_Pnt2d>& points) {
  Region region;
  Loop loop;
  std::string name = "r{";
  for (std::size_t i = 0; i < points.size(); ++i) {
    Segment segment;
    segment.name = "c" + std::to_string(first + static_cast<int>(i));
    segment.start = points[i];
    segment.end = points[(i + 1) % points.size()];
    loop.segments.push_back(segment);
    name += (i == 0 ? "" : ",") + segment.name;
  }
  region.loops.push_back(loop);
  region.name = name + "}";
  return region;
}

// A wedge of 45 degrees along z (mitcad#1150): its knife edge on the z axis
// between the floor y = 0 and a face 2 wide at 45 degrees to it, which meets
// a wall x = sqrt(2) beyond; 30 x 10 in section, 10 high. Its rounding of
// 2 tan(22.5 degrees) touches both faces 2 from the edge, so it takes the
// narrow face away: its contact is 2.4 times its radius, and the strip a
// retry 0.1 % smaller leaves (1e-3 of the contact) is removed as well, no
// face of the result narrower than 0.01.
void test_wedge_of_45_degrees() {
  const double a = std::sqrt(2.0);
  const ShapePtr body =
      extrude("F1", {polygon(1, {gp_Pnt2d(0, 0), gp_Pnt2d(30, 0), gp_Pnt2d(30, 10), gp_Pnt2d(a, 10), gp_Pnt2d(a, a)})},
              0, 10);
  const std::string knife = edge_at(*body, gp_Pnt(0, 0, 5));
  CHECK(!knife.empty());
  const double r = 2.0 * (std::sqrt(2.0) - 1.0);
  const ShapePtr rounded = fillet("F2", *body, {knife}, r);
  CHECK(valid(*rounded));
  CHECK(narrowest_face(*rounded) > 0.01);
  for (const std::string& info : rounded->infos()) {
    std::fprintf(stderr, "wedge of 45 degrees: %s\n", info.c_str());
  }
  // The section less the rounding's: r^2 / tan(22.5 degrees) less the
  // sector of 135 degrees, along 10 (within the retry's 0.1 %).
  const double section = 301.0 - 10.0 * a;
  const double taken = r * r * (1.0 / std::tan(kPi / 8) - 3.0 * kPi / 8);
  CHECK_NEAR_TOL(volume_of(*rounded), 10.0 * (section - taken), 1e-4);
}

// A loop of straight lines through the points, named <prefix><first>..
Loop polygon_loop(const std::string& prefix, int first, const std::vector<gp_Pnt2d>& points) {
  Loop loop;
  for (std::size_t i = 0; i < points.size(); ++i) {
    Segment segment;
    segment.name = prefix + std::to_string(first + static_cast<int>(i));
    segment.start = points[i];
    segment.end = points[(i + 1) % points.size()];
    loop.segments.push_back(segment);
  }
  return loop;
}

// The removal of the strip a dressup built 0.1 % smaller leaves
// (detail::without_strips, mitcad#1106) on a body made to look like one:
// the input's top face c4 of a prism 5 high is 6 wide; in the "smaller"
// body its face of the same name is 2 µm wide, between two faces whose names
// the input has not (the dressup's). With 4 faces beside it, it is removed:
// the faces beside it meet, one face fewer, the volume within 1e-4.
void test_strip_removed() {
  const ShapePtr input = extrude(
      "F1", {polygon(1, {gp_Pnt2d(0, 0), gp_Pnt2d(20, 0), gp_Pnt2d(20, 10), gp_Pnt2d(13, 15), gp_Pnt2d(7, 15),
                         gp_Pnt2d(0, 10)})},
      0, 5);
  Region narrow = polygon(11, {gp_Pnt2d(0, 0), gp_Pnt2d(20, 0), gp_Pnt2d(20, 10), gp_Pnt2d(10.001, 15),
                               gp_Pnt2d(9.999, 15), gp_Pnt2d(0, 10)});
  narrow.loops[0].segments[3].name = "c4";
  const ShapePtr smaller = extrude("F1", {narrow}, 0, 5);
  CHECK(input->find_faces(face_name("F1", "side", "c4")).size() == 1);
  CHECK(smaller->find_faces(face_name("F1", "side", "c4")).size() == 1);
  CHECK(detail::strips_to_remove(*smaller, *input, 2.0).faces == smaller->find_faces(face_name("F1", "side", "c4")));
  const ShapePtr out = detail::without_strips(*smaller, *input, 2.0);
  CHECK(out != nullptr);
  if (out) {
    CHECK(valid(*out));
    CHECK(out->face_count() == smaller->face_count() - 1);
    CHECK_NEAR_TOL(volume_of(*out), volume_of(*smaller), 1e-4 * volume_of(*smaller));
  }
}

// A strip with more faces beside it than kMostFacesBesideStrips stays
// (mitcad#1337): OCCT's defeaturing would grow every one of them by the
// strip's extent and intersect each with each (a strip along a rounding of
// 44 edges round a circle, 88 faces beside it, took more than 300 s and
// 5 GB). Here the caps of a tube 2 µm thick, both named as the wide caps of
// the input's block: between two squares 8 faces lie beside each cap and
// both are removed; between two 12-gons 24 do, and neither is.
void test_strip_with_many_faces_beside_it_stays() {
  const Region square = polygon(1, {gp_Pnt2d(-30, -30), gp_Pnt2d(30, -30), gp_Pnt2d(30, 30), gp_Pnt2d(-30, 30)});
  const ShapePtr input = extrude("F1", {square}, 0, 5);
  const auto tube = [&square](int corners) {
    std::vector<gp_Pnt2d> outer;
    std::vector<gp_Pnt2d> inner;
    for (int i = 0; i < corners; ++i) {
      const double a = 2.0 * kPi * i / corners;
      outer.emplace_back(20.0 * std::cos(a), 20.0 * std::sin(a));
      inner.emplace_back(19.998 * std::cos(a), 19.998 * std::sin(a));
    }
    std::reverse(inner.begin(), inner.end());
    Region region;
    region.name = square.name;
    region.loops = {polygon_loop("c", 101, outer), polygon_loop("c", 201, inner)};
    return extrude("F1", {region}, 0, 5);
  };
  const ShapePtr few = tube(4);
  CHECK(few->face_count() == 10);
  CHECK(detail::strips_to_remove(*few, *input, 2.0).faces.size() == 2);
  const ShapePtr many = tube(12);
  CHECK(many->face_count() == 26);
  CHECK(detail::kMostFacesBesideStrips < 24);
  CHECK(detail::strips_to_remove(*many, *input, 2.0).faces.empty());
  CHECK(detail::without_strips(*many, *input, 2.0) == nullptr);
}

// A disc of radius 31.5, 3 thick, with a regular octagon prism on it whose
// corners lie on the disc's rim (mitcad#1783): the disc's top face is left
// as eight segments between the octagon's sides and the rim, each
// R (1 - cos 22.5 degrees) wide at its side's middle and ending in a point
// at the side's ends.
constexpr double kDiscRadius = 31.5;

ShapePtr octagon_on_disc() {
  const ShapePtr disc = extrude("F1", {circle(1, 0, 0, kDiscRadius)}, 0, 3);
  std::vector<gp_Pnt2d> corners;
  for (int i = 0; i < 8; ++i) {
    const double a = kPi / 4 * i;
    corners.emplace_back(kDiscRadius * std::cos(a), kDiscRadius * std::sin(a));
  }
  const ShapePtr prism = extrude("F2", {polygon(11, corners)}, 3, 20);
  return boolean(BooleanOp::Join, {disc.get()}, *prism).pieces.at(0).shape;
}

// What the chamfer of `d` of one octagon side's foot adds: the space under
// its plane over the segment between the side and the rim, the plane d high
// at the side and meeting the floor d out: the integral over the segment
// (x from the side, a = R cos 22.5 degrees, out to a + d or the rim) of
// (d + a - x) 2 sqrt(R^2 - x^2).
double octagon_side_chamfer(double d) {
  const double r = kDiscRadius;
  const double a = r * std::cos(kPi / 8);
  const double b = std::min(r, a + d);
  const auto root = [r](double x) { return std::sqrt(std::max(0.0, r * r - x * x)); };
  // Antiderivatives of sqrt(R^2 - x^2) and x sqrt(R^2 - x^2).
  const auto plain = [&](double x) { return 0.5 * (x * root(x) + r * r * std::asin(std::min(1.0, x / r))); };
  const auto moment = [&](double x) { return -std::pow(root(x), 3) / 3.0; };
  return 2.0 * ((d + a) * (plain(b) - plain(a)) - (moment(b) - moment(a)));
}

// The faces of the shape on the plane z = 3 facing up (the disc's top).
int faces_on_disc_top(const Shape& shape) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    const BRepAdaptor_Surface surface(shape.face(f));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    GProp_GProps props;
    BRepGProp::SurfaceProperties(shape.face(f), props);
    if (std::abs(plane.Axis().Direction().Z()) > 1.0 - 1e-9 && std::abs(props.CentreOfMass().Z() - 3.0) < 1e-6) {
      ++count;
    }
  }
  return count;
}

// The smallest face's area.
double smallest_face(const Shape& shape) {
  double smallest = 1e300;
  for (int f = 0; f < shape.face_count(); ++f) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(shape.face(f), props);
    smallest = std::min(smallest, props.Mass());
  }
  return smallest;
}

// The octagon's feet chamfered (mitcad#1783): a chamfer as wide as the
// segments at their middles, 1e-3 wider and 1e-3 narrower, and one clearly
// wider takes them away, cut by the disc's rim, with nothing of the disc's
// top face left and no sliver: valid, at its size, with no note or info,
// the volume the exact shape's (1e-3 narrower: less a sliver of about
// 3e-4 mm2 by the middle of each side, taken away with the face). One
// narrower than the segments (2) leaves their middles, each named after its
// segment, the chamfer cut by the rim near the corners. Each foot's chamfer
// face is named after the foot as the input names it. Before, every one
// of these failed or was built 0.1 % smaller, crumpled at the corners.
void test_octagon_on_disc() {
  const ShapePtr body = octagon_on_disc();
  CHECK(valid(*body));
  CHECK(faces_on_disc_top(*body) == 8);
  const double ring = kDiscRadius * (1.0 - std::cos(kPi / 8));
  std::vector<std::string> feet;
  std::vector<std::string> segments;
  for (int i = 0; i < 8; ++i) {
    const double a = kPi / 4 * (i + 0.5);
    const double m = kDiscRadius * std::cos(kPi / 8);
    feet.push_back(edge_at(*body, gp_Pnt(m * std::cos(a), m * std::sin(a), 3)));
    CHECK(!feet.back().empty());
    // The segment's name: the foot's other face but the octagon's side.
    for (int f = 0; f < body->face_count(); ++f) {
      const BRepAdaptor_Surface surface(body->face(f));
      GProp_GProps props;
      BRepGProp::SurfaceProperties(body->face(f), props);
      const gp_Pnt c = props.CentreOfMass();
      if (surface.GetType() == GeomAbs_Plane && std::abs(c.Z() - 3.0) < 1e-6 &&
          std::abs(std::atan2(c.Y(), c.X()) - std::remainder(a, 2 * kPi)) < 1e-6) {
        segments.push_back(body->face_names(f).front());
      }
    }
  }
  CHECK(segments.size() == 8);
  const double before = volume_of(*body);
  for (const double d : {ring, ring + 1e-3, ring - 1e-3, 2.4}) {
    ChamferSet bevel;
    bevel.edges = feet;
    bevel.spec.distance = d;
    const ShapePtr out = chamfer("F3", *body, {bevel});
    CHECK(valid(*out));
    CHECK(faces_on_disc_top(*out) == 0);
    CHECK(smallest_face(*out) > 1e-3);
    CHECK(out->face_count() == 19);
    CHECK(out->notes().empty());
    CHECK(out->infos().empty());
    CHECK_NEAR_TOL(volume_of(*out), before + 8 * octagon_side_chamfer(d), 1e-9);
    // Each foot's chamfer face, named after the foot (the edge's name in
    // the input), the segments gone (where a sliver was taken away, its
    // names held by the chamfer's face beside it, as after the 0.1 %
    // smaller retry).
    for (std::size_t i = 0; i < feet.size() && i < segments.size(); ++i) {
      CHECK(faces_named(*out, "F3:chamfer(" + feet[i] + ")") == 1);
      CHECK(faces_holding(*out, segments[i]) == (d < ring ? 1 : 0));
    }
  }
  ChamferSet narrower;
  narrower.edges = feet;
  narrower.spec.distance = 2;
  const ShapePtr out = chamfer("F3", *body, {narrower});
  CHECK(valid(*out));
  CHECK(faces_on_disc_top(*out) == 8);
  CHECK(smallest_face(*out) > 1e-3);
  CHECK(out->notes().empty());
  CHECK(out->infos().empty());
  CHECK_NEAR_TOL(volume_of(*out), before + 8 * octagon_side_chamfer(2), 1e-9);
  for (std::size_t i = 0; i < feet.size() && i < segments.size(); ++i) {
    CHECK(faces_named(*out, "F3:chamfer(" + feet[i] + ")") == 1);
    CHECK(faces_named(*out, segments[i]) == 1);
  }
}

// A block 10 x 10 in plan stepped in its section along y: 5 high for
// y = 0 to 5, 10 high for y = 5 to 10 (GitHub #353's design). Each of its
// side faces (x = 0, x = 10) is an L; the lower top and the step's wall
// are 10 wide.
ShapePtr stepped_block() {
  const ShapePtr low = extrude("F1", {rectangle(1, 0, 0, 10, 5)}, 0, 5);
  const ShapePtr high = extrude("F2", {rectangle(5, 0, 5, 10, 5)}, 0, 10);
  return boolean(BooleanOp::Join, {low.get()}, *high).pieces.at(0).shape;
}

// The stepped block's edges (by their middles) rounded with 5, as large as
// the side faces are high below the lower top's edges and deep beside the
// step's edges: each rounding ends exactly on its side face's far edge (the
// bottom's or the back's edge), the side face taken away there. The
// geometry kernel cannot build that at the exact size; it is built as a
// narrower face is (exactly_wide, GitHub #353), not 0.1 % smaller with a
// sliver of the side face left: valid, no face under 0.01 mm2, no note and
// no information, the volume exact.
void rounded_to_the_far_edges(const char* what, const std::vector<gp_Pnt>& middles, double expected) {
  const ShapePtr body = stepped_block();
  CHECK_NEAR_TOL(volume_of(*body), 750.0, 1e-9);
  FilletSet round;
  for (const gp_Pnt& m : middles) {
    round.edges.push_back(edge_at(*body, m));
    CHECK(!round.edges.back().empty());
  }
  round.radius = 5;
  const ShapePtr rounded = fillet("F3", *body, {round});
  CHECK(valid(*rounded));
  CHECK(narrowest_face(*rounded) > 0.01);
  CHECK_NEAR_TOL(volume_of(*rounded), expected, 1e-6);
  CHECK(rounded->notes().empty() && rounded->infos().empty());
  CHECK(all_faces_named(*rounded));
  if (!rounded->notes().empty() || !rounded->infos().empty() || !near(volume_of(*rounded), expected, 1e-6)) {
    std::fprintf(stderr, "%s: volume %.9g, expected %.9g\n", what, volume_of(*rounded), expected);
  }
}

void test_stepped_block_rounded_to_the_far_edges() {
  // The region of the step's foot (x 0 to 10, y 5 to 10, z 0 to 5) where
  // the roundings of a lower top's edge (a cylinder along y) and of the
  // step's edge above it (along z) meet: they run on into it and meet as a
  // mitre, the side face there gone too; what they leave of it is the union
  // of the two half cylinders, 125 pi - 500/3 (their common part is 500/3).
  const double foot = 250.0 - (125.0 * kPi - 500.0 / 3.0);
  const gp_Pnt low_top_right(10, 2.5, 5);
  const gp_Pnt low_top_left(0, 2.5, 5);
  const gp_Pnt step_right(10, 5, 7.5);
  const gp_Pnt step_left(0, 5, 7.5);
  const gp_Pnt front_left(0, 0, 2.5);
  rounded_to_the_far_edges("a lower top's edge", {low_top_right}, 750.0 - 5 * fillet_loss(5));
  // (The two roundings also meet on the lower top: a full round.)
  rounded_to_the_far_edges("both lower top's edges", {low_top_right, low_top_left}, 750.0 - 10 * fillet_loss(5));
  rounded_to_the_far_edges("a lower top's and the step's edge", {low_top_right, step_right},
                           750.0 - 10 * fillet_loss(5) - foot / 2);
  // GitHub #353's first design: all four, the body 250 pi - 500/3.
  rounded_to_the_far_edges("both sides", {low_top_right, low_top_left, step_right, step_left},
                           750.0 - 20 * fillet_loss(5) - foot);
  // The front's and the step's upright edges on one side: chains whose ends'
  // faces (the lower top's plane) hold the space past the side face's far
  // edge on opposite sides, each clipped by its own ends (fill_overflow).
  rounded_to_the_far_edges("the front's and the step's upright edges", {front_left, step_left},
                           750.0 - 10 * fillet_loss(5));
  // GitHub #353's second design: every edge round the side faces but their
  // bottom and back edges, so that the chains end on those far edges (the
  // front's upright edge on the bottom's, the upper top's edge on the
  // back's): the space past them is filled too (fill_overflow's
  // `past_ends`). The front half (y 0 to 5, z 0 to 5) and the upper part
  // (y 5 to 10, z 5 to 10) are each the common part of two half cylinders
  // (on one side the quarter of it, 500/6, and the other side whole), the
  // step's foot as above.
  const gp_Pnt front_right(10, 0, 2.5);
  const gp_Pnt high_top_left(0, 7.5, 10);
  const gp_Pnt high_top_right(10, 7.5, 10);
  rounded_to_the_far_edges("one side all round", {front_left, low_top_left, step_left, high_top_left},
                           375.0 + 250.0 / 3.0 + 62.5 * kPi);
  rounded_to_the_far_edges("both sides all round",
                           {front_left, low_top_left, step_left, high_top_left, front_right, low_top_right,
                            step_right, high_top_right},
                           500.0 / 3.0 + 125.0 * kPi);
}

// The volume `shape` has in common with `tool`, on a copy of the shape (a
// boolean adds curves to its inputs, which the input checks would take for
// a change of the body).
double common_volume(const Shape& shape, const TopoDS_Shape& tool) {
  return volume_of(BRepAlgoAPI_Common(BRepBuilderAPI_Copy(shape.occt()).Shape(), tool).Shape());
}

// A cube of 10 rounded on all its edges by `radius` at (or within 2e-4 of)
// half its side (GitHub #353's third design at 5 mm): the corners' balls at
// both ends of each edge meet in its middle, every face taken away; the
// geometry kernel's fillet never returned there and a cancel did not reach
// it. Refused before the kernel (corners_meet), it is built 0.1 % smaller,
// with the warning that says so, at once: the volume of the cube rounded
// by 0.999 `radius`, 1000 - 12 (10 - 2r) r^2 (1 - pi/4) - 8 r^3 (1 - pi/6).
void test_cube_rounded_at_half_its_side(double radius) {
  const ShapePtr cube = extrude("F1", {rectangle(1, 0, 0, 10, 10)}, 0, 10);
  FilletSet round;
  for (int e = 0; e < cube->edge_count(); ++e) {
    round.edges.push_back(cube->edge_name(e));
  }
  CHECK(round.edges.size() == 12);
  round.radius = radius;
  const ShapePtr rounded = fillet("F2", *cube, {round});
  CHECK(valid(*rounded));
  CHECK(!rounded->notes().empty());
  const double r = radius * (1.0 - 1.0e-3);
  const double expected = 1000.0 - 12 * (10 - 2 * r) * fillet_loss(r) - 8 * r * r * r * (1 - kPi / 6);
  CHECK_NEAR_TOL(volume_of(*rounded), expected, 1e-6);
}

// GitHub #375: the stepped block rounded by 5 on both sides (as GitHub
// #353's first design), then its back's bottom edge (y = 10, z = 0)
// chamfered or rounded by 5. Its faces there were slivers of the 0.1 %
// smaller rounding, which the bevel left untrimmed; now the rounding takes
// the side faces away at its size and the bevel cuts the bent half round
// cleanly: valid, no face under 0.01 mm2, no note, and the volume the
// rounded block's less its common part with the bevel's wedge (the
// triangle y - 5 > z) or with the rounding's corner (the square less the
// circle about y = 5, z = 5), along x.
void test_stepped_block_rounded_then_its_back_edge_dressed() {
  const ShapePtr body = stepped_block();
  FilletSet round;
  for (const gp_Pnt& m : {gp_Pnt(10, 2.5, 5), gp_Pnt(0, 2.5, 5), gp_Pnt(10, 5, 7.5), gp_Pnt(0, 5, 7.5)}) {
    round.edges.push_back(edge_at(*body, m));
  }
  round.radius = 5;
  const ShapePtr rounded = fillet("F3", *body, {round});
  CHECK_NEAR_TOL(volume_of(*rounded), 250.0 * kPi - 500.0 / 3.0, 1e-6);
  const std::string back = edge_at(*rounded, gp_Pnt(5, 10, 0));
  CHECK(!back.empty());
  // (The triangle y - 5 > z, 0 < z < 5, along x past the body.)
  BRepBuilderAPI_MakePolygon triangle(gp_Pnt(-5, 5, 0), gp_Pnt(-5, 10, 0), gp_Pnt(-5, 10, 5), true);
  const TopoDS_Shape wedge =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(triangle.Wire()).Face(), gp_Vec(20, 0, 0)).Shape();
  ChamferSet bevel;
  bevel.edges = {back};
  bevel.spec.distance = 5;
  const ShapePtr bevelled = chamfer("F4", *rounded, {bevel});
  CHECK(valid(*bevelled));
  CHECK(narrowest_face(*bevelled) > 0.01);
  CHECK(bevelled->notes().empty() && bevelled->infos().empty());
  CHECK_NEAR_TOL(volume_of(*bevelled),
                 volume_of(*rounded) - common_volume(*rounded, wedge), 1e-6);
  const ShapePtr round_back = fillet("F4", *rounded, {back}, 5);
  CHECK(valid(*round_back));
  CHECK(narrowest_face(*round_back) > 0.01);
  CHECK(round_back->notes().empty() && round_back->infos().empty());
  const TopoDS_Shape square = BRepPrimAPI_MakeBox(gp_Pnt(-5, 5, 0), gp_Pnt(15, 10, 5)).Shape();
  const TopoDS_Shape ball =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-5, 5, 5), gp_Dir(1, 0, 0)), 5, 20).Shape();
  const TopoDS_Shape corner = BRepAlgoAPI_Cut(square, ball).Shape();
  CHECK_NEAR_TOL(volume_of(*round_back),
                 volume_of(*rounded) - common_volume(*rounded, corner), 1e-6);
}

// A tube 10 high round a bore of radius 26, its outer radius `outer`, with
// a shelf standing out from the bore's wall flush with the top: a block
// from x = -8 to 8, y = 24.1 to 27.5 (into the wall), joined. The bore's top
// rim runs round from one end wall of the shelf to the other; each end wall
// is 0.64 wide where the rim meets it, on the bore's side of the wall.
ShapePtr tube_with_shelf(double outer) {
  const ShapePtr tube = extrude("F1", {circle(1, 0, 0, outer)}, 0, 10);
  const ShapePtr bore = extrude("F2", {circle(2, 0, 0, 26)}, -1, 11);
  const ShapePtr cut = boolean(BooleanOp::Cut, {tube.get()}, *bore).pieces.at(0).shape;
  const ShapePtr shelf = extrude("F3", {rectangle(5, -8, 24.1, 16, 3.4)}, 0, 10);
  return boolean(BooleanOp::Join, {cut.get()}, *shelf).pieces.at(0).shape;
}

// The bore's top rim: the arc of radius 26 at z = 10.
std::string bore_rim(const Shape& body) {
  for (int i = 0; i < body.edge_count(); ++i) {
    const BRepAdaptor_Curve curve(body.edge(i));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - 26) < 1e-6 &&
        std::abs(curve.Circle().Location().Z() - 10) < 1e-6) {
      return body.edge_name(i);
    }
  }
  return {};
}

// The bore's top rim chamfered by `size` (equal distances), its chain
// ending against the shelf's end walls (GitHub #353, mitcad#1819). The
// geometry kernel finds no face to end the bevel on there, at any size: the
// end wall lies on the bore's side of the wall, the bevel in the wall. It
// is ended on the end walls' planes, as a modeller would, the end walls
// grown out along their planes to meet it (ended_on_end_planes); where the
// top beside the rim is narrower than the bevel (outer radius 28.28: the
// ring 2.28 wide) the bevel also takes the top away there and meets the
// outer wall. The reference: the tube less the cone's ring (the triangle
// of the bevel's section revolved) outside the shelf's slab (|x| < 8 on
// the shelf's side).
void test_bore_rim_chamfered_against_a_shelf(double outer, double size) {
  const ShapePtr body = tube_with_shelf(outer);
  const std::string rim = bore_rim(*body);
  CHECK(!rim.empty());
  ChamferSet bevel;
  bevel.edges = {rim};
  bevel.spec.distance = size;
  const ShapePtr bevelled = chamfer("F4", *body, {bevel});
  CHECK(valid(*bevelled));
  CHECK(narrowest_face(*bevelled) > 0.01);
  CHECK(bevelled->notes().empty() && bevelled->infos().empty());
  CHECK(faces_holding(*bevelled, "F4:chamfer(" + rim + ")") >= 1);
  BRepBuilderAPI_MakePolygon section(gp_Pnt(26, 0, 10), gp_Pnt(26 + size, 0, 10), gp_Pnt(26, 0, 10 - size), true);
  const TopoDS_Shape ring =
      BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(section.Wire()).Face(), gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)))
          .Shape();
  const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(-8, 0, -1), gp_Pnt(8, 40, 11)).Shape();
  const TopoDS_Shape bevel_ring = BRepAlgoAPI_Cut(ring, slab).Shape();
  const double expected = volume_of(*body) - common_volume(*body, bevel_ring);
  CHECK_NEAR_TOL(volume_of(*bevelled), expected, 1e-6);
  if (!near(volume_of(*bevelled), expected, 1e-6)) {
    std::fprintf(stderr, "rim against a shelf (outer %g, size %g): volume %.9g, expected %.9g\n", outer, size,
                 volume_of(*bevelled), expected);
  }
}

void test_bore_rim_chamfered_against_a_shelf() {
  test_bore_rim_chamfered_against_a_shelf(30, 1);
  test_bore_rim_chamfered_against_a_shelf(30, 2.7);
  test_bore_rim_chamfered_against_a_shelf(28.28, 2.7);
}

// The same rim rounded by `radius`: ended on the end walls' planes alike.
// The reference: the tube less the ring the rounding takes off (the
// section's square less the ball's circle, revolved) outside the slab.
void test_bore_rim_rounded_against_a_shelf(double radius) {
  const ShapePtr body = tube_with_shelf(30);
  const std::string rim = bore_rim(*body);
  CHECK(!rim.empty());
  const ShapePtr rounded = fillet("F4", *body, {rim}, radius);
  CHECK(valid(*rounded));
  CHECK(narrowest_face(*rounded) > 0.01);
  CHECK(rounded->notes().empty() && rounded->infos().empty());
  CHECK(faces_holding(*rounded, "F4:fillet(" + rim + ")") >= 1);
  const gp_Ax1 axis(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  BRepBuilderAPI_MakePolygon square(gp_Pnt(26, 0, 10), gp_Pnt(26 + radius, 0, 10), gp_Pnt(26 + radius, 0, 10 - radius),
                                    gp_Pnt(26, 0, 10 - radius), true);
  const TopoDS_Shape block = BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(square.Wire()).Face(), axis).Shape();
  const TopoDS_Shape ball =
      BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0, 0, 10 - radius), gp_Dir(0, 0, 1)), 26 + radius, radius).Shape();
  const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(-8, 0, -1), gp_Pnt(8, 40, 11)).Shape();
  const TopoDS_Shape loss = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(block, ball).Shape(), slab).Shape();
  const double expected = volume_of(*body) - common_volume(*body, loss);
  CHECK_NEAR_TOL(volume_of(*rounded), expected, 1e-6);
  if (!near(volume_of(*rounded), expected, 1e-6)) {
    std::fprintf(stderr, "rim rounded against a shelf (radius %g): volume %.9g, expected %.9g\n", radius,
                 volume_of(*rounded), expected);
  }
}

} // namespace

// The wall's top (2 wide) rounded along both its edges in one fillet, with
// 1.5 along the outer wall (x = 0) and 0.8 along the inner (x = 2): the top
// is narrower than the two roundings together (mitcad#1926, port patch
// 1720). The ball of 0.8 rolls tangent on the rounding of 1.5, inside its
// circle, and on the inner wall; the top goes. The section above z = 3:
// the two arcs, the inner wall up to the small ball's contact and the outer
// wall up to the large one's. Valid, every face named, no top left, no
// note; whichever order the sets come in.
void test_wall_top_rounded_with_two_radii() {
  const ShapePtr body = wall();
  const double h = 7, w = 2, r2 = 1.5, r1 = 0.8;
  const std::string outer = edge_at(*body, gp_Pnt(0, 10, h));
  const std::string inner = edge_at(*body, gp_Pnt(w, 10, h));
  CHECK(!outer.empty() && !inner.empty());
  // (x, z) of the section; the small ball inside the large one's circle.
  const double c2x = r2, c2z = h - r2;
  const double dx = (w - r1) - c2x, dz = std::sqrt((r2 - r1) * (r2 - r1) - dx * dx);
  const double c1x = w - r1, c1z = c2z + dz;
  const double at = std::atan2(dz, dx);
  std::vector<std::pair<double, double>> section{{0, 3}, {w, 3}};
  const int n = 4000;
  for (int i = 0; i <= n; ++i) {
    const double a = at * i / n;
    section.emplace_back(c1x + r1 * std::cos(a), c1z + r1 * std::sin(a));
  }
  for (int i = 0; i <= n; ++i) {
    const double a = at + (kPi - at) * i / n;
    section.emplace_back(c2x + r2 * std::cos(a), c2z + r2 * std::sin(a));
  }
  double area = 0;
  for (std::size_t i = 0; i < section.size(); ++i) {
    const auto& p = section[i];
    const auto& q = section[(i + 1) % section.size()];
    area += p.first * q.second - q.first * p.second;
  }
  const double expected = 30.0 * 20 * 3 + 20.0 * 0.5 * std::abs(area);
  FilletSet large;
  large.edges = {outer};
  large.radius = r2;
  FilletSet small;
  small.edges = {inner};
  small.radius = r1;
  for (const bool large_first : {true, false}) {
    const ShapePtr rounded =
      fillet("F3", *body, large_first ? std::vector<FilletSet>{large, small} : std::vector<FilletSet>{small, large});
    CHECK(valid(*rounded));
    CHECK(all_faces_named(*rounded));
    CHECK(rounded->notes().empty());
    CHECK_NEAR_TOL(volume_of(*rounded), expected, 1e-6);
    // No plane face is left at the top's height.
    int tops = 0;
    for (int f = 0; f < rounded->face_count(); ++f) {
      const BRepAdaptor_Surface s(rounded->face(f));
      tops += s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - h) < 1e-9 &&
                  s.Plane().Axis().Direction().IsParallel(gp_Dir(0, 0, 1), 1e-9)
                ? 1
                : 0;
    }
    CHECK(tops == 0);
  }
}

// The wall's top, a strip 2 wide between the outer wall and the inner one
// (a parallel plane 2 away), closed off at its end by a face at 45 degrees
// (the end's top edge chamfered with 3), rounded along the outer wall with
// a radius wider than the strip, 3 and 2.5 (mitcad#1926): the rounding
// takes the strip away, its ball rolling on the inner wall's top edge (its
// axis r from the outer wall and from that edge, at z = 7 - sqrt(r^2 -
// (r - 2)^2)), and the face at 45 degrees cuts it at the end. The section:
// the arc from the inner wall's top to the outer wall; the face at 45
// degrees (z = 4 + y) cuts the wall above z = 4. Valid, every face named,
// no top left; the volume within a millionth.
// Until mitcad#1926 makes the geometry kernel build it, the fillet fails,
// in words, the edge and its faces painted, and never comes back as a valid
// body that is wrong: the kernel built roundings between 2 and 3 with the
// ball riding on the floor beyond the wall and the outer wall, the wall
// under the strip gone and the floor's edge rounded instead, and at 3 that
// came back from the 0.1 % smaller retry (mitcad#1953). Either outcome
// passes; a body with a wall missing does not.
void test_strip_between_a_parallel_plane_and_a_slope() {
  const ShapePtr body = wall();
  ChamferSet bevel;
  bevel.edges = {edge_at(*body, gp_Pnt(1, 0, 7))};
  bevel.spec.distance = 3;
  const ShapePtr sloped = chamfer("F3", *body, {bevel});
  CHECK(valid(*sloped));
  const std::string outer = edge_at(*sloped, gp_Pnt(0, 11.5, 7));
  CHECK(!outer.empty());
  for (const double r : {3.0, 2.5}) {
    const double w = 2, h = 7;
    const double cx = r, cz = h - std::sqrt(r * r - (cx - w) * (cx - w));
    const double a0 = std::atan2(h - cz, w - cx);
    // The section above z = 4 (the slope's foot) as a polygon: its area and
    // the integral of z - 4 over it give what the slope takes away.
    std::vector<std::pair<double, double>> upper{{0, 4}, {w, 4}};
    const int n = 4000;
    for (int i = 0; i <= n; ++i) {
      const double a = a0 + (kPi - a0) * i / n;
      upper.emplace_back(cx + r * std::cos(a), cz + r * std::sin(a));
    }
    double area = 0, moment = 0;
    for (std::size_t i = 0; i < upper.size(); ++i) {
      const auto& p = upper[i];
      const auto& q = upper[(i + 1) % upper.size()];
      const double cross = p.first * q.second - q.first * p.second;
      area += 0.5 * cross;
      moment += cross * (p.second + q.second) / 6.0;
    }
    area = std::abs(area);
    moment = std::abs(moment);
    const double expected = 30.0 * 20 * 3 + 20.0 * (w * 1 + area) - (moment - 4 * area);
    ShapePtr rounded;
    std::string error;
    try {
      rounded = fillet("F4", *sloped, {outer}, r);
    } catch (const std::runtime_error& failed) {
      error = failed.what();
    }
    if (!rounded) {
      // In words: no names, ids or coordinates in what a person reads; the
      // places say where.
      const std::string said = error_message(error);
      std::fprintf(stderr, "strip beside a slope, rounded with %g: %s\n", r, said.c_str());
      CHECK(said.find("wider than the face beside its edge") != std::string::npos ||
            said.find("too large for the face beside its edge") != std::string::npos);
      CHECK(error.find(kPlacedTail) != std::string::npos);
      const std::string message = error.substr(error.find("\"message\":"));
      const std::string plain = message.substr(0, message.find("\"detail\":"));
      CHECK(plain.find("E{") == std::string::npos && plain.find("F1:") == std::string::npos &&
            plain.find("F2:") == std::string::npos && plain.find('(') == std::string::npos);
      CHECK(error.find("\"kind\":\"edge\"") != std::string::npos);
      continue;
    }
    CHECK(valid(*rounded));
    CHECK(all_faces_named(*rounded));
    CHECK_NEAR_TOL(volume_of(*rounded), expected, 1e-6);
    int tops = 0;
    for (int f = 0; f < rounded->face_count(); ++f) {
      const BRepAdaptor_Surface s(rounded->face(f));
      tops += s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - h) < 1e-9 &&
                  s.Plane().Axis().Direction().IsParallel(gp_Dir(0, 0, 1), 1e-9)
                ? 1
                : 0;
    }
    CHECK(tops == 0);
  }
}

void dressup_takes_face_tests() {
  guarded("octagon on disc", test_octagon_on_disc);
  guarded("step", test_step);
  guarded("wall then a rounding along the outer wall", test_wall_then_a_rounding_along_the_outer_wall);
  guarded("tray", test_tray);
  guarded("tray rounded all round", test_tray_rounded_all_round);
  guarded("tray corner with OCCT alone", test_tray_corner_with_occt_alone);
  guarded("hole rim at its size", test_hole_rim_at_its_size);
  guarded("hole rim with OCCT alone", test_hole_rim_with_occt_alone);
  guarded("wedge of 45 degrees", test_wedge_of_45_degrees);
  guarded("strip removed", test_strip_removed);
  guarded("strip with many faces beside it stays", test_strip_with_many_faces_beside_it_stays);
  guarded("stepped block rounded to the far edges", test_stepped_block_rounded_to_the_far_edges);
  guarded("bore rim chamfered against a shelf", [] { test_bore_rim_chamfered_against_a_shelf(); });
  guarded("cube rounded at half its side", [] {
    test_cube_rounded_at_half_its_side(5);
    test_cube_rounded_at_half_its_side(4.9998);
  });
  guarded("stepped block rounded then its back edge dressed", test_stepped_block_rounded_then_its_back_edge_dressed);
  guarded("bore rim rounded against a shelf", [] {
    test_bore_rim_rounded_against_a_shelf(1);
    test_bore_rim_rounded_against_a_shelf(2.7);
  });
  guarded("wall top rounded with two radii", test_wall_top_rounded_with_two_radii);
  guarded("strip between a parallel plane and a slope", test_strip_between_a_parallel_plane_and_a_slope);
}

} // namespace test
