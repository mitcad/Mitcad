// SPDX-License-Identifier: MIT
// The tangent chains of a body's edges and faces (mitcad#290), and that a
// fillet, chamfer and shell with a tangent chain take exactly the chain.

#include <algorithm>
#include <cmath>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/tangent_chain.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// A 60 x 40 x 20 block (F2) with lines c1..c4.
ShapePtr block() { return extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20); }

// The vertical edge between side i - 1 and side i, and the top edge of side i.
std::string vertical(int i) { return edge(side("F2", 1, (i + 3) % 4), side("F2", 1, i)); }
std::string top(int i) { return edge(end_cap("F2", 1), side("F2", 1, i)); }

// The chain that holds the item, or just the item.
std::vector<int> chain_of(const std::vector<std::vector<int>>& chains, int item) {
  for (const std::vector<int>& chain : chains) {
    if (std::find(chain.begin(), chain.end(), item) != chain.end()) {
      return chain;
    }
  }
  return {item};
}

int edge_index(const Shape& body, const std::string& name) {
  const std::vector<int> found = body.find_edges(name);
  CHECK(found.size() == 1);
  return found.empty() ? -1 : found.front();
}

// The number of faces with a name that starts with the prefix.
int faces_starting(const Shape& shape, const std::string& prefix) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    const NameList& names = shape.face_names(f);
    if (std::any_of(names.begin(), names.end(),
                    [&](const std::string& name) { return name.rfind(prefix, 0) == 0; })) {
      ++count;
    }
  }
  return count;
}

// A fillet and a chamfer of the one edge with a tangent chain round and
// bevel every edge of its chain and no other.
void check_dressups_take_the_chain(const Shape& body, const std::string& seed) {
  const std::vector<int> chain = chain_of(edge_tangent_chains(body), edge_index(body, seed));
  FilletSet rounding;
  rounding.edges = {seed};
  rounding.radius = 1;
  const ShapePtr rounded = fillet("F9", body, std::vector<FilletSet>{rounding});
  CHECK(rounded->notes().empty());
  CHECK(faces_starting(*rounded, "F9:fillet(") == static_cast<int>(chain.size()));
  ChamferSet bevel;
  bevel.edges = {seed};
  bevel.spec.distance = 1;
  const ShapePtr bevelled = chamfer("F9", body, std::vector<ChamferSet>{bevel});
  CHECK(bevelled->notes().empty());
  CHECK(faces_starting(*bevelled, "F9:chamfer(") == static_cast<int>(chain.size()));
  for (const int e : chain) {
    CHECK(faces_named(*rounded, "F9:fillet(" + body.edge_name(e) + ")") == 1);
    CHECK(faces_named(*bevelled, "F9:chamfer(" + body.edge_name(e) + ")") == 1);
  }
}

void test_sharp_box_has_no_chains() {
  const ShapePtr box = block();
  CHECK(edge_tangent_chains(*box).empty());
  CHECK(face_tangent_chains(*box).empty());
  check_dressups_take_the_chain(*box, top(0));
}

void test_rounded_corner_joins_its_edges() {
  // One vertical edge rounded: the top edges on both sides of it continue
  // tangentially into the rounding's top edge; the box's sharp corners
  // stop the chain.
  const ShapePtr box = block();
  const ShapePtr rounded = fillet("F3", *box, {vertical(1)}, 5);
  const std::vector<std::vector<int>> chains = edge_tangent_chains(*rounded);
  CHECK(chains.size() == 2); // along the top and along the bottom
  const std::vector<int> along = chain_of(chains, edge_index(*rounded, top(0)));
  CHECK(along.size() == 3);
  CHECK(std::count(along.begin(), along.end(), edge_index(*rounded, top(1))) == 1);
  CHECK(chain_of(chains, edge_index(*rounded, top(2))).size() == 1);
  CHECK(chain_of(chains, edge_index(*rounded, vertical(2))).size() == 1);
  check_dressups_take_the_chain(*rounded, top(0));
  check_dressups_take_the_chain(*rounded, top(2));

  // The faces: the rounding and the two sides it joins.
  const std::vector<std::vector<int>> faces = face_tangent_chains(*rounded);
  CHECK(faces.size() == 1);
  const std::vector<int> sides = chain_of(faces, rounded->find_faces(side("F2", 1, 0)).front());
  CHECK(sides.size() == 3);
  CHECK(std::count(sides.begin(), sides.end(), rounded->find_faces("F3:fillet(" + vertical(1) + ")").front()) ==
        1);
  // A shell that removes one of them with a tangent chain removes all three.
  ShellSpec shell;
  shell.faces = {side("F2", 1, 0)};
  shell.inside = 2;
  const ShapePtr hollow = mitcad::geometry::shell("F9", *rounded, shell);
  CHECK(faces_named(*hollow, side("F2", 1, 1)) == 0);
  CHECK(faces_named(*hollow, side("F2", 1, 2)) == 1);
}

void test_rounded_box_top_loop_closes() {
  // Every vertical edge rounded: the top outline is one closed chain of
  // eight edges, and so is the bottom one.
  const ShapePtr box = block();
  const ShapePtr rounded = fillet("F3", *box, {vertical(0), vertical(1), vertical(2), vertical(3)}, 5);
  const std::vector<std::vector<int>> chains = edge_tangent_chains(*rounded);
  CHECK(chains.size() == 2);
  for (int i = 0; i < 4; ++i) {
    CHECK(chain_of(chains, edge_index(*rounded, top(i))).size() == 8);
  }
  CHECK(face_tangent_chains(*rounded).size() == 1);
  CHECK(face_tangent_chains(*rounded).front().size() == 8);
  check_dressups_take_the_chain(*rounded, top(3));
}

// A bar bent by 30 degrees at a mitre, its front top edge rounded with 2:
// in a section across it (y, z) the bottom S (z = -1) from the back wall
// y = -6 to the rounding, the rounding about (0, 0) up to the top
// z = `top`, the top back to the back wall. With the top at z = 2 the
// rounding is tangent to it; lower, the top cuts the rounding off at a
// crease. The bar runs along x up to the mitre and on along the turned
// direction; the two roundings meet in the mitre plane along a crease from
// S up to the top, where they are tangent when the top is tangent to them.
ShapePtr bent_rounded_bar(double top) {
  constexpr double kRadius = 2.0;
  const double front = std::sqrt(kRadius * kRadius - 1.0);
  const double up = std::sqrt(kRadius * kRadius - top * top);
  const gp_Pnt a(-10, -6, -1);
  const gp_Pnt b(-10, front, -1);
  const gp_Pnt c(-10, up, top);
  const gp_Pnt d(-10, -6, top);
  // A point of the rounding between b and c.
  const double half = 0.5 * (std::atan2(-1.0, front) + std::atan2(top, up));
  const gp_Pnt m(-10, kRadius * std::cos(half), kRadius * std::sin(half));
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(a, b).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(b, m, c).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(c, d).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(d, a).Edge());
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape straight = BRepPrimAPI_MakePrism(section, gp_Vec(20, 0, 0)).Shape();
  constexpr double kTurn = 0.52359877559829887; // 30 degrees
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), kTurn);
  const TopoDS_Shape turned = BRepBuilderAPI_Transform(straight, turn, true).Shape();
  // The mitre: the plane across the mean of the two directions.
  const gp_Dir across(1.0 + std::cos(kTurn), std::sin(kTurn), 0.0);
  const TopoDS_Face mitre = BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, 0, 0), across)).Face();
  const TopoDS_Shape before =
      BRepAlgoAPI_Common(straight, BRepPrimAPI_MakeHalfSpace(mitre, gp_Pnt(-1, 0, 0)).Solid()).Shape();
  const TopoDS_Shape after =
      BRepAlgoAPI_Common(turned, BRepPrimAPI_MakeHalfSpace(mitre, gp_Pnt(1, 1, 0)).Solid()).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(before, after).Shape(), true, true, true);
  unify.Build();
  return import_body(unify.Shape(), "F1", 0);
}

// The edge of the body whose middle (by parameter) lies nearest to p.
int edge_near(const Shape& body, const gp_Pnt& p) {
  int best = -1;
  double nearest = 1e100;
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    const double distance = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p);
    if (distance < nearest) {
      nearest = distance;
      best = e;
    }
  }
  return best;
}

void test_chain_runs_on_across_a_fading_crease() {
  // mitcad#1435: the bottom's edges with the two roundings meet at the
  // mitre at 30 degrees, where the roundings meet along a crease. Where
  // the crease fades out (the roundings tangent to each other at its top
  // end), one chain takes both; where it stays a crease up to the top, the
  // chain stops at the turn as before.
  const double front = std::sqrt(3.0);
  const gp_Pnt mine(-5, front, -1);
  const double turn = 0.52359877559829887; // 30 degrees
  const gp_Pnt theirs(5 * std::cos(turn) - front * std::sin(turn), 5 * std::sin(turn) + front * std::cos(turn), -1);
  for (const double top : {2.0, 1.5}) {
    const ShapePtr bar = bent_rounded_bar(top);
    const int e = edge_near(*bar, mine);
    const int o = edge_near(*bar, theirs);
    CHECK(e >= 0 && o >= 0 && e != o);
    const std::vector<int> chain = chain_of(edge_tangent_chains(*bar), e);
    const bool both = std::find(chain.begin(), chain.end(), o) != chain.end();
    CHECK(both == (top == 2.0));
    if (top == 2.0) {
      CHECK(chain.size() == 2);
      check_dressups_take_the_chain(*bar, bar->edge_name(e));
    }
  }
}

} // namespace

void tangent_chain_tests() {
  guarded("test_sharp_box_has_no_chains", test_sharp_box_has_no_chains);
  guarded("test_rounded_corner_joins_its_edges", test_rounded_corner_joins_its_edges);
  guarded("test_rounded_box_top_loop_closes", test_rounded_box_top_loop_closes);
  guarded("test_chain_runs_on_across_a_fading_crease", test_chain_runs_on_across_a_fading_crease);
}

} // namespace test
