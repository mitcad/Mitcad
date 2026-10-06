// SPDX-License-Identifier: MIT
// Operations leave their inputs as they are (T0e): B-rep fingerprints, the
// input check, and the operations that wrote into their inputs' sub-shapes
// in place (the face merge after booleans, sections, shape healing).

#include <exception>

#include <BRep_Builder.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_Plane.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "../src/history.hpp"
#include "check.hpp"
#include "mitcad/analysis/compare.hpp"
#include "mitcad/analysis/interference.hpp"
#include "mitcad/analysis/section.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// Sketch1's 60 x 40 rectangle (c1..c4) extruded 20 mm by F2.
ShapePtr block() { return extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20); }

// A p-curve on a plane that none of the edge's faces lies on, as OCCT's
// face merge (ShapeUpgrade_UnifySameDomain) gave the edges of the faces it
// merged.
void add_foreign_pcurve(const TopoDS_Edge& edge) {
  const occ::handle<Geom_Plane> plane = new Geom_Plane(gp_Pln(gp_Pnt(0, 0, 100), gp_Dir(0, 0, 1)));
  const occ::handle<Geom2d_Line> line = new Geom2d_Line(gp_Pnt2d(0, 0), gp_Dir2d(1, 0));
  BRep_Builder().UpdateEdge(edge, line, plane, TopLoc_Location(), 1.0e-7);
}

// The side of a cylinder extruded from circle c<curve>.
std::string side_of_circle(const std::string& feature, int curve) {
  return face_name(feature, "side", "c" + std::to_string(curve));
}

BooleanResult run(BooleanOp op, const std::vector<ShapePtr>& targets, const ShapePtr& tool) {
  std::vector<const Shape*> raw;
  for (const ShapePtr& target : targets) {
    raw.push_back(target.get());
  }
  return boolean(op, raw, *tool);
}

ShapePtr joined(const ShapePtr& target, const ShapePtr& tool) {
  const BooleanResult result = run(BooleanOp::Join, {target}, tool);
  CHECK(result.pieces.size() == 1);
  return result.pieces.empty() ? nullptr : result.pieces.front().shape;
}

// Runs `op` and checks that it leaves each input as it was.
template <class Op>
void check_kept(const char* what, const std::vector<ShapePtr>& inputs, Op&& op) {
  std::vector<BrepFingerprint> before;
  for (const ShapePtr& input : inputs) {
    before.push_back(fingerprint(input->occt()));
  }
  try {
    op();
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: unexpected error: %s\n", what, error.what());
    ++failures;
  }
  for (std::size_t i = 0; i < inputs.size(); ++i) {
    const BrepFingerprint after = fingerprint(inputs[i]->occt());
    if (!after.same_brep(before[i])) {
      std::fprintf(stderr, "%s changed input %zu: %s\n", what, i,
                   after.changes_since(before[i]).c_str());
      ++failures;
    }
  }
}

bool same_counts(const BrepFingerprint& a, const BrepFingerprint& b) {
  return a.shapes == b.shapes && a.surfaces == b.surfaces && a.curves == b.curves &&
         a.pcurves == b.pcurves && a.continuities == b.continuities && a.meshes == b.meshes;
}

void test_fingerprint() {
  const ShapePtr box = block();
  const BrepFingerprint before = fingerprint(box->occt());
  CHECK(fingerprint(box->occt()) == before);
  // OCCT's prism shares the bottom's face, wire, edges and vertices with
  // the top (moved by a location), and edges on planes need no p-curves.
  CHECK(before.shapes == 1 + 1 + 5 + 5 + 8 + 4);
  CHECK(before.surfaces == 5);
  CHECK(before.curves == 8);
  CHECK(before.pcurves == 0);
  CHECK(before.meshes == 0);
  CHECK(before.changes_since(before).empty());
  // The same block again: the same counts, other geometry handles.
  const BrepFingerprint other = fingerprint(block()->occt());
  CHECK(same_counts(other, before));
  CHECK(other.geometry != before.geometry);

  add_foreign_pcurve(box->edge(0));
  const BrepFingerprint after = fingerprint(box->occt());
  CHECK(after != before);
  CHECK(after.pcurves == before.pcurves + 1);
  CHECK(after.surfaces == before.surfaces + 1);
  CHECK(after.topology == before.topology);
  CHECK(after.changes_since(before) == "surfaces 5 -> 6, p-curves 0 -> 1, geometry");

  BRep_Builder().UpdateVertex(box->vertex(0), 1.0e-3);
  const BrepFingerprint wider = fingerprint(box->occt());
  CHECK(wider.changes_since(after) == "tolerances");
  CHECK(!wider.same_brep(after));
  // The bookkeeping flags alone are the same B-rep.
  BrepFingerprint flagged = wider;
  flagged.bookkeeping += 1;
  flagged.flagged[0] += 1;
  CHECK(flagged != wider);
  CHECK(flagged.same_brep(wider));
  CHECK(flagged.changes_since(wider) == "flags (free " + std::to_string(wider.flagged[0]) +
                                            " -> " + std::to_string(wider.flagged[0] + 1) + ")");
}

// The copies operations work on hold what their inputs hold: the same
// counts, structure, tolerances and flags (BRepBuilderAPI_Copy stored
// p-curves for edges on planes, and a boolean on such copies left an
// invalid face where the same boolean on the inputs did not).
void test_input_copy() {
  const ShapePtr lower = extrude("F2", {circle(1, 0, 0, 10)}, 0, 20);
  const ShapePtr upper = extrude("F4", {circle(2, 0, 0, 10)}, 20, 40);
  const std::string bottom = face_name("F2", "start", "r{c1}");
  const ShapePtr rounded =
      fillet("F6", *joined(lower, upper), {edge(bottom, side_of_circle("F2", 1))}, 2);
  for (const ShapePtr& shape : {block(), lower, rounded}) {
    const BrepFingerprint original = fingerprint(shape->occt());
    const detail::InputCopy copy(shape->occt());
    const BrepFingerprint copied = fingerprint(copy.shape());
    CHECK(same_counts(copied, original));
    CHECK(copied.topology == original.topology);
    CHECK(copied.tolerances == original.tolerances);
    CHECK(copied.flags == original.flags);
    CHECK(copied.geometry != original.geometry); // its own curves on surfaces
    CHECK(copy.of(shape->face(0)).IsSame(TopExp_Explorer(copy.shape(), TopAbs_FACE).Current()));
    if (!same_counts(copied, original)) {
      std::fprintf(stderr, "the copy: %s\n", copied.changes_since(original).c_str());
    }
  }
  // Copied together, shapes keep the sub-shapes they share.
  const detail::InputCopy both(std::vector<TopoDS_Shape>{lower->occt(), lower->face(0)});
  bool shared = false;
  for (TopExp_Explorer it(both.shape(0), TopAbs_FACE); it.More(); it.Next()) {
    shared = shared || it.Current().IsSame(both.shape(1));
  }
  CHECK(shared);
  CHECK(!both.shape(1).IsSame(lower->face(0)));
}

// The check mode: test_main turns it on for the whole run and wants no
// change reported at the end, so this runs first and clears what it made.
void test_input_check_reports() {
  CHECK(input_checks_enabled());
  clear_input_changes();
  const ShapePtr box = block();
  {
    const CheckedOperation operation("test change");
    const TopoDS_Edge& edge = box->edge(0);
    add_foreign_pcurve(edge);
  }
  std::vector<std::string> changes = input_changes();
  CHECK(changes.size() == 1);
  if (!changes.empty()) {
    CHECK(changes[0].find("test change changed an input, a shape of 6 faces (F2:") == 0);
    CHECK(changes[0].find(": surfaces 5 -> 6, p-curves 0 -> 1, geometry") != std::string::npos);
  }
  // Read outside an operation and changed there: the next operation tells.
  clear_input_changes();
  add_foreign_pcurve(box->edge(1));
  { const CheckedOperation operation("test read"); }
  changes = input_changes();
  CHECK(changes.size() == 1);
  if (!changes.empty()) {
    CHECK(changes[0].find("changed outside checked operations (after test change, before test "
                          "read): surfaces") != std::string::npos);
  }
  // Nothing changed: nothing reported.
  clear_input_changes();
  {
    const CheckedOperation operation("test nothing");
    CHECK(box->occt().ShapeType() == TopAbs_SOLID);
  }
  CHECK(input_changes().empty());
  clear_input_changes();
}

// ShapeUpgrade_UnifySameDomain, which merges the coplanar faces a boolean
// leaves (SimplifyResult), gives the edges of the faces it merges p-curves
// on the merged face's surface in place, also in OCCT's safe input mode:
// those edges were the cached inputs' own.
void test_joins_leave_inputs() {
  const ShapePtr base = block();
  // Flush with the block's top, bottom and back: faces to merge.
  const ShapePtr extension = extrude("F4", {rectangle(5, 60, 0, 20, 40)}, 0, 20);
  check_kept("a join merging faces", {base, extension},
             [&] { CHECK(joined(base, extension)->face_count() == 6); });
  const ShapePtr boss = extrude("F6", {rectangle(9, 10, 10, 20, 10)}, 0, 30);
  check_kept("a join", {base, boss}, [&] { joined(base, boss); });
  // A cut flush with the block's front leaves a step whose faces merge.
  const ShapePtr slot = extrude("F8", {rectangle(13, 0, 0, 60, 10)}, 10, 30);
  check_kept("a cut", {base, slot}, [&] { run(BooleanOp::Cut, {base}, slot); });
  check_kept("an intersection", {base, boss}, [&] { run(BooleanOp::Intersect, {base}, boss); });
  const ShapePtr second = extrude("F10", {rectangle(17, 80, 0, 20, 40)}, 0, 20);
  check_kept("a union", {base, extension, second}, [&] {
    unite({base.get(), extension.get(), second.get()});
  });
  // Two cylinders one on the other: their sides merge into one face, on
  // the lower one's surface, whose p-curves the upper one's edges need.
  const ShapePtr lower = extrude("F12", {circle(21, 0, 0, 10)}, 0, 20);
  const ShapePtr upper = extrude("F14", {circle(22, 0, 0, 10)}, 20, 40);
  check_kept("a join merging cylinders", {lower, upper},
             [&] { CHECK(joined(lower, upper)->face_count() == 3); });
  // The same with a step: the sides of a wider ring flush with a cylinder.
  const ShapePtr ring = extrude("F16", {circle(23, 0, 0, 10)}, 40, 45);
  check_kept("a join of three cylinders", {lower, upper, ring}, [&] {
    const ShapePtr two = joined(lower, upper);
    joined(two, ring);
  });
}

// BRepAlgoAPI_Section in its default (destructive) mode may give the
// sections' faces p-curves and tolerances in place.
void test_sections_leave_inputs() {
  const ShapePtr base = block();
  const Tool at20 = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  check_kept("a split of faces", {base}, [&] { split_faces("F7", *base, {end_cap("F2", 1)}, at20); });
  check_kept("a split of the body", {base}, [&] { split_body("F7", *base, at20); });
  // A setback corner cuts the roundings across with sections.
  FilletSet three;
  three.edges = {edge(side("F2", 1, 0), side("F2", 1, 1)),
                 edge(end_cap("F2", 1), side("F2", 1, 0)),
                 edge(end_cap("F2", 1), side("F2", 1, 1))};
  three.radius = 2;
  check_kept("a setback corner", {base}, [&] { fillet("F5", *base, {three}, false); });
}

// The analysis library's booleans ran destructive: comparing a body with
// another nearly the same (the .f3d import's final comparison with the stored
// bodies) gave its edges p-curves on the other's surfaces.
void test_analyses_leave_inputs() {
  const ShapePtr base = block();
  // Nearly the base: a block 0.01 mm longer, made apart.
  const ShapePtr longer = extrude("F4", {rectangle(5, 0, 0, 60.01, 40)}, 0, 20);
  const ShapePtr boss = extrude("F6", {rectangle(9, 10, 10, 20, 10)}, 0, 30);
  check_kept("a comparison", {base, longer}, [&] {
    mitcad::analysis::CompareOptions options;
    options.samples = 200;
    mitcad::analysis::compare(base->occt(), longer->occt(), options);
  });
  check_kept("interference", {base, boss},
             [&] { mitcad::analysis::interferences({base->occt(), boss->occt()}); });
  check_kept("a section", {base}, [&] {
    mitcad::analysis::section(base->occt(), mitcad::analysis::Plane{{0, 0, 10}, {0, 0, 1}});
  });
  check_kept("a clip", {base}, [&] {
    mitcad::analysis::clip(base->occt(), mitcad::analysis::Plane{{0, 0, 10}, {0, 0, 1}});
  });
}

// OCCT's offsets and lofts widened the tolerances of their input's edges.
void test_offsets_and_lofts_leave_inputs() {
  const ShapePtr base = block();
  ShellSpec spec;
  spec.inside = 2;
  spec.faces = {end_cap("F2", 1)};
  check_kept("a shell", {base}, [&] { shell("F5", *base, spec); });
  spec.faces.clear();
  spec.outside = 1;
  check_kept("a closed shell", {base}, [&] { shell("F5", *base, spec); });
  // A loft from the block's top to a smaller square above it.
  LoftSpec loft_spec;
  loft_spec.feature = "F5";
  LoftSection top;
  top.kind = LoftSection::Kind::Face;
  top.body = base;
  top.face = end_cap("F2", 1);
  LoftSection square;
  square.kind = LoftSection::Kind::Region;
  square.frame.origin = gp_Pnt(0, 0, 40);
  square.region = rectangle(21, 10, 10, 40, 20);
  loft_spec.sections = {top, square};
  check_kept("a loft from a face", {base}, [&] { loft(loft_spec); });
}

void test_dressups_leave_inputs() {
  const ShapePtr base = block();
  const std::string corner = edge(side("F2", 1, 0), side("F2", 1, 1));
  check_kept("a fillet", {base}, [&] { fillet("F5", *base, {corner}, 3); });
  ChamferSpec spec;
  spec.distance = 2;
  check_kept("a chamfer", {base}, [&] { chamfer("F5", *base, {corner}, spec); });
  ChamferSet two;
  two.edges = {corner, edge(end_cap("F2", 1), side("F2", 1, 0))};
  two.spec = spec;
  check_kept("a blend chamfer corner", {base},
             [&] { chamfer("F5", *base, {two}, ChamferCorner::Blend); });
}

// The same join computed again after other operations ran on its inputs
// (as an import tries several definitions before the one it keeps) gives
// the same B-rep: before T0e, the face merges of the joins tried first left
// p-curves and surfaces on the block's edges, which the second join then
// carried, and on a corpus design the merge of that second join broke
// (BRepCheck_UnorientableShape) where a fresh one did not.
void test_join_same_after_other_operations() {
  const ShapePtr base = block();
  const ShapePtr extension = extrude("F4", {rectangle(5, 60, 0, 20, 40)}, 0, 20);
  const BrepFingerprint fresh = fingerprint(joined(base, extension)->occt());
  // Tries on the same block: joins flush with its other sides, a cut.
  joined(base, extrude("F6", {rectangle(9, -20, 0, 20, 40)}, 0, 20));
  joined(base, extrude("F8", {rectangle(13, 0, 40, 60, 20)}, 0, 20));
  joined(base, extrude("F10", {rectangle(17, 0, -10, 60, 10)}, 0, 20));
  run(BooleanOp::Cut, {base}, extrude("F12", {rectangle(21, 0, 0, 60, 10)}, 10, 30));
  const BrepFingerprint again = fingerprint(joined(base, extension)->occt());
  CHECK(same_counts(again, fresh));
  if (!same_counts(again, fresh)) {
    std::fprintf(stderr, "the join again: %s\n", again.changes_since(fresh).c_str());
  }

  // The same on curved faces: a cylinder joined with one on top of it,
  // after joins with others above and below it that merge their sides.
  const ShapePtr rod = extrude("F14", {circle(25, 0, 0, 10)}, 0, 20);
  const ShapePtr top = extrude("F16", {circle(26, 0, 0, 10)}, 20, 30);
  const BrepFingerprint fresh_rod = fingerprint(joined(rod, top)->occt());
  joined(rod, extrude("F18", {circle(27, 0, 0, 10)}, -10, 0));
  joined(rod, extrude("F20", {circle(28, 0, 0, 10)}, 20, 25));
  split_faces("F22", *rod, {side_of_circle("F14", 25)},
              Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 10), gp_Dir(0, 0, 1))));
  const BrepFingerprint again_rod = fingerprint(joined(rod, top)->occt());
  CHECK(same_counts(again_rod, fresh_rod));
  if (!same_counts(again_rod, fresh_rod)) {
    std::fprintf(stderr, "the cylinders' join again: %s\n",
                 again_rod.changes_since(fresh_rod).c_str());
  }
}

} // namespace

void input_check_self_tests() {
  guarded("test_fingerprint", test_fingerprint);
  guarded("test_input_check_reports", test_input_check_reports);
}

void input_check_tests() {
  guarded("test_input_copy", test_input_copy);
  guarded("test_joins_leave_inputs", test_joins_leave_inputs);
  guarded("test_sections_leave_inputs", test_sections_leave_inputs);
  guarded("test_dressups_leave_inputs", test_dressups_leave_inputs);
  guarded("test_analyses_leave_inputs", test_analyses_leave_inputs);
  guarded("test_offsets_and_lofts_leave_inputs", test_offsets_and_lofts_leave_inputs);
  guarded("test_join_same_after_other_operations", test_join_same_after_other_operations);
}

} // namespace test
