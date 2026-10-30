// SPDX-License-Identifier: MIT
// Join, Cut and Intersect with participant bodies: pieces, their sources and
// the face names they carry.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Circle.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>

#include "../src/history.hpp"
#include "check.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/path_sweep.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// Sketch1's 60 x 40 rectangle (c1..c4) extruded 20 mm by F2.
ShapePtr block() { return extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20); }

BooleanResult run(BooleanOp op, const std::vector<ShapePtr>& targets, const ShapePtr& tool) {
  std::vector<const Shape*> raw;
  for (const ShapePtr& target : targets) {
    raw.push_back(target.get());
  }
  return boolean(op, raw, *tool);
}

void test_join_keeps_names_and_names_new_edges() {
  // A 20 x 10 boss (c5..c8) from the sketch plane through the block's top.
  const ShapePtr boss = extrude("F4", {rectangle(5, 10, 10, 20, 10)}, 0, 30);
  const BooleanResult result = run(BooleanOp::Join, {block()}, boss);
  CHECK(result.touched == std::vector<bool>{true});
  CHECK(result.pieces.size() == 1);
  if (result.pieces.size() != 1) {
    return;
  }
  const Shape& joined = *result.pieces[0].shape;
  CHECK(result.pieces[0].sources == std::vector<std::size_t>{0});
  CHECK(near(volume(joined), 60.0 * 40.0 * 20.0 + 20.0 * 10.0 * 10.0));
  // The block's faces and the boss's sides and top keep their names. The
  // boss's base lies on the block's bottom face, which carries both names.
  for (int i = 0; i < 4; ++i) {
    CHECK(faces_named(joined, side("F2", 1, i)) == 1);
    CHECK(faces_named(joined, side("F4", 5, i)) == 1);
  }
  CHECK(faces_named(joined, end_cap("F2", 1)) == 1);
  CHECK(faces_named(joined, end_cap("F4", 5)) == 1);
  CHECK(joined.find_faces(start_cap("F4", 5)).size() == 1);
  CHECK(joined.find_faces(start_cap("F4", 5)) == joined.find_faces(start_cap("F2", 1)));
  CHECK(edge_names_unique(joined));
  // The edges where the boss meets the block's top have names now, so they
  // can be referenced (V0 could not).
  for (int i = 0; i < 4; ++i) {
    CHECK(joined.find_edges(edge(end_cap("F2", 1), side("F4", 5, i))).size() == 1);
  }
}

void test_join_merges_flush_faces() {
  const ShapePtr extension = extrude("F4", {rectangle(5, 60, 0, 20, 40)}, 0, 20);
  const BooleanResult result = run(BooleanOp::Join, {block()}, extension);
  CHECK(result.pieces.size() == 1);
  const Shape& joined = *result.pieces.at(0).shape;
  CHECK(joined.face_count() == 6);
  // The coplanar top faces became one face with both names.
  CHECK(joined.find_faces(end_cap("F2", 1)) == joined.find_faces(end_cap("F4", 5)));
  CHECK(joined.find_faces(end_cap("F2", 1)).size() == 1);
  CHECK(joined.find_faces(side("F2", 1, 1)).empty()); // the shared wall at x = 60
  CHECK(edge_names_unique(joined));
}

// A pillar (a circle of radius 10 extruded from 0 to 10, F2) and on it a
// ring turned about its axis (the rectangle 8..10 x 10..12, F4) whose
// profile lies on either side of the axis: the ring's seam faces the
// pillar's or lies opposite it. The join has one outer cylinder from 0 to
// 12, as the stored bodies of designs do: the face merge after the boolean
// failed to build the merged face's seam where the two seams lay half a
// turn apart (OCCT port patch 0220, mitcad#284).
void test_join_merges_coaxial_cylinders_whatever_their_seams() {
  for (const double side_of_axis : {1.0, -1.0}) {
    const ShapePtr pillar = extrude("F2", {circle(1, 0, 0, 10)}, 0, 10);
    mitcad::geometry::RevolveSpec spec;
    spec.feature = "F4";
    spec.frame.x_axis = gp_Dir(side_of_axis, 0, 0);
    spec.frame.y_axis = gp_Dir(0, 0, 1);
    spec.regions = {rectangle(2, 8, 10, 2, 2)};
    spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
    spec.angle1 = 2.0 * kPi;
    const ShapePtr ring = revolve(spec);
    const BooleanResult result = run(BooleanOp::Join, {pillar}, ring);
    CHECK(result.pieces.size() == 1);
    if (result.pieces.size() != 1) {
      continue;
    }
    const Shape& joined = *result.pieces[0].shape;
    CHECK(near(volume(joined), kPi * 100.0 * 10.0 + kPi * (100.0 - 64.0) * 2.0));
    // The bottom, the outer cylinder, the ring's top and inner wall, the
    // pillar's top inside the ring.
    CHECK(joined.face_count() == 5);
    const std::string pillar_wall = mitcad::geometry::face_name("F2", "side", "c1");
    const std::string ring_wall = side("F4", 2, 1); // x = 10 in the profile
    CHECK(faces_named(joined, pillar_wall) == 1);
    CHECK(joined.find_faces(pillar_wall) == joined.find_faces(ring_wall));
    CHECK(edge_names_unique(joined));
  }
}

void test_slivers_are_no_pieces() {
  // A result of the block and a flat solid beside it: two faces of a strip
  // 1.5 mm long, back to back (no volume), as a join of walls that continue
  // the body's walls left between coinciding faces. The strip is no piece;
  // a sliver alone stays the result.
  const ShapePtr body = block();
  const TopoDS_Face strip =
      BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, -1, 0), gp_Dir(0, 1, 0)), 0.0, 1e-6, 0.0, 1.5).Face();
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  builder.Add(shell, strip);
  builder.Add(shell, strip.Reversed());
  TopoDS_Solid flat;
  builder.MakeSolid(flat);
  builder.Add(flat, shell);
  TopoDS_Compound both;
  builder.MakeCompound(both);
  builder.Add(both, body->occt());
  builder.Add(both, flat);
  const std::vector<detail::FaceNamer::Piece> pieces = detail::FaceNamer(both).pieces();
  CHECK(pieces.size() == 1);
  CHECK(!pieces.empty() && near(volume(*pieces.at(0).shape), 60.0 * 40.0 * 20.0));
  CHECK(detail::FaceNamer(flat).pieces().size() == 1);
}

void test_join_with_several_participants() {
  const ShapePtr left = extrude("F2", {rectangle(1, 0, 0, 10, 10)}, 0, 10);
  const ShapePtr right = extrude("F3", {rectangle(5, 20, 0, 10, 10)}, 0, 10);
  const ShapePtr far = extrude("F4", {rectangle(9, 100, 0, 10, 10)}, 0, 10);
  // A bridge over the gap touches the left and right blocks only.
  const ShapePtr bridge = extrude("F6", {rectangle(13, 5, 0, 20, 10)}, 5, 15);
  const BooleanResult result = run(BooleanOp::Join, {left, far, right}, bridge);
  CHECK((result.touched == std::vector<bool>{true, false, true}));
  CHECK(result.pieces.size() == 1);
  CHECK((result.pieces.at(0).sources == std::vector<std::size_t>{0, 2}));

  // A tool that touches nothing joins nothing.
  const ShapePtr apart = extrude("F6", {rectangle(13, 50, 50, 5, 5)}, 0, 5);
  const BooleanResult none = run(BooleanOp::Join, {left, right}, apart);
  CHECK((none.touched == std::vector<bool>{false, false}) && none.pieces.empty());
}

void test_join_of_many_copies_finds_what_they_touch() {
  // Ten 2 mm cubes along x, as a pattern's copies (one shape of ten solids).
  std::vector<ShapePtr> cubes;
  std::vector<const Shape*> parts;
  for (int k = 0; k < 10; ++k) {
    cubes.push_back(extrude("F6", {rectangle(13, 10.0 * k, 0, 2, 2)}, 0, 2));
    parts.push_back(cubes.back().get());
  }
  const ShapePtr copies = unite(parts);
  // On a face of the fourth copy, on an edge of the sixth, a post 0.27 mm
  // from a corner of the eighth (their boxes overlap), a block 1.5e-6 mm
  // from the ninth, one inside the tenth, and one far from all.
  const ShapePtr face = extrude("F2", {rectangle(1, 32, 0, 5, 2)}, 0, 2);
  const ShapePtr edge = extrude("F3", {rectangle(5, 52, 2, 3, 3)}, 0, 2);
  const ShapePtr post = extrude("F4", {circle(9, 72.9, 2.9, 1)}, 0, 2);
  const ShapePtr gap = extrude("F5", {rectangle(10, 82.0000015, 0, 3, 2)}, 0, 2);
  const ShapePtr inside = extrude("F7", {rectangle(17, 90.5, 0.5, 1, 1)}, 0.5, 1.5);
  const ShapePtr far = extrude("F8", {rectangle(21, 0, 100, 2, 2)}, 0, 2);
  const BooleanResult result =
      run(BooleanOp::Join, {face, edge, post, gap, inside, far}, copies);
  CHECK((result.touched == std::vector<bool>{true, true, false, false, true, false}));
  // The swallowed block leaves no face of its own.
  std::vector<std::size_t> sources;
  for (const BooleanPiece& piece : result.pieces) {
    sources.insert(sources.end(), piece.sources.begin(), piece.sources.end());
  }
  std::sort(sources.begin(), sources.end());
  CHECK((sources == std::vector<std::size_t>{0, 1}));
}

void test_join_with_a_plate_of_many_holes() {
  // A 60 x 40 plate 2 mm thick with 40 x 26 holes: the faces near a tool
  // have over a thousand edges, so the tool's vertices are tried before the
  // faces are measured.
  Region plate = rectangle(1, 0, 0, 60, 40);
  int curve = 5;
  for (int i = 0; i < 40; ++i) {
    for (int j = 0; j < 26; ++j) {
      plate.loops.push_back(circle(curve++, 0.75 + 1.5 * i, 0.75 + 1.5 * j, 0.4).loops.at(0));
    }
  }
  const ShapePtr holed = extrude("F2", {plate}, 0, 2);
  // A small block above the plate between two posts on it, touching neither.
  const ShapePtr above = extrude("F3", {rectangle(1045, 2.8, 1.3, 0.4, 0.4)}, 3, 4);
  const auto posts = [](double from) {
    return extrude("F4", {rectangle(1049, 1.3, 1.3, 0.4, 0.4), rectangle(1053, 4.3, 1.3, 0.4, 0.4)},
                   from, 5);
  };
  // Standing on the plate between its holes: joined to it.
  const BooleanResult standing = run(BooleanOp::Join, {holed, above}, posts(2));
  CHECK((standing.touched == std::vector<bool>{true, false}));
  CHECK(standing.pieces.size() == 1);
  if (standing.pieces.size() == 1) {
    CHECK((standing.pieces[0].sources == std::vector<std::size_t>{0}));
    CHECK(near(volume(*standing.pieces[0].shape), volume(*holed) + 2 * 0.4 * 0.4 * 3));
  }
  // Sunk into the plate: joined too.
  CHECK((run(BooleanOp::Join, {holed, above}, posts(1)).touched == std::vector<bool>{true, false}));
  // A thousandth of a millimetre above it: nothing touched.
  const BooleanResult hovering = run(BooleanOp::Join, {holed, above}, posts(2.001));
  CHECK((hovering.touched == std::vector<bool>{false, false}) && hovering.pieces.empty());
  // A nanometre and a half above it (the noise of the geometry a file stores): it
  // still joins, as one solid with the plate.
  const BooleanResult nearly = run(BooleanOp::Join, {holed, above}, posts(2.0000015));
  CHECK((nearly.touched == std::vector<bool>{true, false}));
  CHECK(nearly.pieces.size() == 1);
  if (nearly.pieces.size() == 1) {
    CHECK((nearly.pieces[0].sources == std::vector<std::size_t>{0}));
    CHECK(near(volume(*nearly.pieces[0].shape), volume(*holed) + 2 * 0.4 * 0.4 * 3));
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

ShapePtr extrude_on(const std::string& feature, const Frame& frame, std::vector<Region> regions, double start,
                    double end) {
  ExtrudeSpec spec;
  spec.feature = feature;
  spec.frame = frame;
  spec.direction = frame.normal();
  spec.regions = std::move(regions);
  spec.start = start;
  spec.end = end;
  return mitcad::geometry::extrude(spec);
}

// A join within a tolerance (mitcad#298), on one target.
BooleanResult run_within(const ShapePtr& target, const ShapePtr& tool, double within) {
  const std::vector<const Shape*> raw{target.get()};
  return boolean(BooleanOp::Join, raw, *tool, within);
}

void test_join_within_a_tolerance_reaches_a_tool_micrometres_away() {
  // A pin 2.6 micrometres above the block's top: apart for an exact join,
  // joined within 0.01 mm (as a body the import kept within its tolerance
  // joins the tool the file joined to it).
  const ShapePtr body = block();
  const ShapePtr pin = extrude("F4", {circle(5, 30, 20, 2.5)}, 20.0026, 21.5);
  const BooleanResult exact = run(BooleanOp::Join, {body}, pin);
  CHECK((exact.touched == std::vector<bool>{false}) && exact.pieces.empty());
  const BooleanResult within = run_within(body, pin, 0.01);
  CHECK((within.touched == std::vector<bool>{true}));
  CHECK(within.pieces.size() == 1);
  if (within.pieces.size() == 1) {
    CHECK((within.pieces[0].sources == std::vector<std::size_t>{0}));
    CHECK_NEAR_TOL(volume(*within.pieces[0].shape), volume(*body) + volume(*pin), 1e-5);
    CHECK(faces_named(*within.pieces[0].shape, "F4:side(c5)") == 1);
  }
  // Farther away than the tolerance: still apart.
  const ShapePtr far = extrude("F4", {circle(5, 30, 20, 2.5)}, 20.02, 21.5);
  const BooleanResult apart = run_within(body, far, 0.01);
  CHECK((apart.touched == std::vector<bool>{false}) && apart.pieces.empty());
  // Touching exactly: the exact join's result.
  const ShapePtr standing = extrude("F4", {circle(5, 30, 20, 2.5)}, 20, 21.5);
  const BooleanResult on = run_within(body, standing, 0.01);
  CHECK(on.pieces.size() == 1);
  CHECK(!on.pieces.empty() &&
        near(volume(*on.pieces[0].shape), volume(*run(BooleanOp::Join, {body}, standing).pieces.at(0).shape)));
}

// Half of a tee of two pipes of the same outer radius (13, wall 3), cut at
// the plane y = 0 through the main pipe's axis (z): the branch pipe (along
// -y, at z = 50) meets the main one where their outer cylinders are
// tangent, at (-13, 0, 50) on the cut face's edge. With a gusset plate
// 1.5 mm thick on the cut face, against the main pipe's outer line x = -13,
// `offset` off the face and its normal turned by `tilt` (the noise of a
// body that matches a file's within a micrometre).
std::pair<ShapePtr, ShapePtr> half_tee_and_plate(double offset, double tilt) {
  const ShapePtr main_outer = test::extrude("F1", {circle(1, 0, 0, 13)}, 0, 100);
  Frame xz;
  xz.origin = gp_Pnt(0, 0, 0);
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  const ShapePtr branch_outer = extrude_on("F2", xz, {circle(2, 0, 50, 13)}, 0, 50);
  const ShapePtr main_inner = test::extrude("F3", {circle(3, 0, 0, 10)}, 0, 100);
  const ShapePtr branch_inner = extrude_on("F4", xz, {circle(4, 0, 50, 10)}, 0, 50);
  const ShapePtr half = test::extrude("F6", {rectangle(8, -20, 0, 40, 20)}, -1, 101);
  ShapePtr tee = run(BooleanOp::Cut, {main_outer}, half).pieces.at(0).shape;
  tee = run(BooleanOp::Join, {tee}, branch_outer).pieces.at(0).shape;
  tee = run(BooleanOp::Cut, {tee}, main_inner).pieces.at(0).shape;
  tee = run(BooleanOp::Cut, {tee}, branch_inner).pieces.at(0).shape;
  Frame side;
  side.origin = gp_Pnt(-13, offset, 0);
  side.x_axis = gp_Dir(tilt, 1, 0);
  side.y_axis = gp_Dir(0, 0, 1);
  const ShapePtr plate =
      extrude_on("F5", side, {polygon(5, {gp_Pnt2d(0, 0), gp_Pnt2d(50, 50), gp_Pnt2d(0, 100)})}, 0, 1.5);
  return {tee, plate};
}

void test_join_within_a_tolerance_on_a_near_coincident_face() {
  // The plate half a micrometre into the cut face, turned by 4e-9: the
  // exact join's result has an invalid face (a self-intersecting wire at
  // the tangent point); within 0.01 mm the join is retried with the
  // smallest fuzzy value that gives a valid result.
  const auto [tee, plate] = half_tee_and_plate(-5e-7, 3.9e-9);
  const BooleanResult within = run_within(tee, plate, 0.01);
  CHECK((within.touched == std::vector<bool>{true}));
  CHECK(within.pieces.size() == 1);
  if (within.pieces.size() == 1) {
    CHECK_NEAR_TOL(volume(*within.pieces[0].shape), volume(*tee) + 2500.0 * 1.5, 1e-6);
  }
  // Half a micrometre off the face (within the contact distance, but not
  // on the face for the exact boolean): the exact join leaves the plate a
  // solid of its own beside the tee; within the tolerance it joins it.
  const auto [tee2, apart] = half_tee_and_plate(5e-7, 0.0);
  const BooleanResult stray = run(BooleanOp::Join, {tee2}, apart);
  const BooleanResult joined = run_within(tee2, apart, 0.01);
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() == 1) {
    CHECK((joined.pieces[0].sources == std::vector<std::size_t>{0}));
    CHECK_NEAR_TOL(volume(*joined.pieces[0].shape), volume(*tee2) + 2500.0 * 1.5, 1e-6);
  }
  // Where the exact join merges them, the join within the tolerance is it.
  if (stray.pieces.size() == 1) {
    CHECK(near(volume(*stray.pieces[0].shape), volume(*joined.pieces.at(0).shape)));
  }
}

// A block 40 x 20 x 20 with half a cylinder (radius 12.5, axis z through
// the middle of its front side y = 0) taken out of it, its two rims rounded
// with radius 1, and the tool that fills the cavity again: half a cylinder
// `dr` larger in radius, as high as the block (it meets the roundings
// tangentially).
std::pair<ShapePtr, ShapePtr> rounded_cavity_and_filler(double dr) {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-20, 0, 0), gp_Pnt(20, 20, 20)).Shape();
  const TopoDS_Shape hole = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 12.5, 22).Shape();
  const TopoDS_Shape cavity = BRepAlgoAPI_Cut(box, hole).Shape();
  BRepFilletAPI_MakeFillet rims(cavity);
  for (TopExp_Explorer it(cavity, TopAbs_EDGE); it.More(); it.Next()) {
    if (BRepAdaptor_Curve(TopoDS::Edge(it.Current())).GetType() == GeomAbs_Circle) {
      rims.Add(1.0, TopoDS::Edge(it.Current()));
    }
  }
  rims.Build();
  const TopoDS_Shape filler_cylinder =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 12.5 + dr, 20).Shape();
  const TopoDS_Shape filler =
      BRepAlgoAPI_Common(filler_cylinder, BRepPrimAPI_MakeBox(gp_Pnt(-20, 0, -5), gp_Pnt(20, 20, 25)).Shape())
          .Shape();
  const auto named = [](const TopoDS_Shape& shape, const std::string& feature) {
    std::vector<Shape::NamedFace> names;
    int i = 0;
    for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
      names.push_back({it.Current(), {feature + ":import(" + std::to_string(i++) + ")"}});
    }
    return std::make_shared<Shape>(shape, names);
  };
  CHECK(rims.IsDone());
  return {named(rims.IsDone() ? rims.Shape() : cavity, "F1"), named(filler, "F2")};
}

void test_join_within_a_tolerance_rebuilds_lost_solids() {
  // The filler as large as the cavity: the exact join fills it, leaving
  // the grooves at the rims.
  const auto [exact_block, exact_filler] = rounded_cavity_and_filler(0.0);
  const BooleanResult exact = run(BooleanOp::Join, {exact_block}, exact_filler);
  CHECK(exact.pieces.size() == 1);
  const double filled = exact.pieces.empty() ? 0.0 : volume(*exact.pieces[0].shape);
  CHECK(filled > 16000.0 - 20.0 && filled < 16000.0);
  // The filler 0.1 nm thinner than the cavity: OCCT's exact join builds no
  // solid at all (it cannot orient the faces and leaves them unused); half
  // a micrometre thinner, it leaves the filler's faces loose beside the
  // block. Within 0.01 mm the join is retried with the smallest fuzzy value
  // that builds solids of every face and the target (mitcad#235: an
  // extrusion against a stored body's rounded cylindrical wall).
  for (const double dr : {-1e-7, -5e-7}) {
    const auto [block_with_cavity, filler] = rounded_cavity_and_filler(dr);
    const BooleanResult within = run_within(block_with_cavity, filler, 0.01);
    CHECK((within.touched == std::vector<bool>{true}));
    CHECK(within.pieces.size() == 1);
    if (within.pieces.size() == 1) {
      CHECK((within.pieces[0].sources == std::vector<std::size_t>{0}));
      CHECK_NEAR_TOL(volume(*within.pieces[0].shape), filled, 1e-6);
      CHECK(BRepCheck_Analyzer(within.pieces[0].shape->occt()).IsValid());
    }
  }
}

// The edges of a shape shorter than 1e-5 mm (degenerate ones aside).
int zero_length_edges(const Shape& shape) {
  int count = 0;
  for (int i = 0; i < shape.edge_count(); ++i) {
    const TopoDS_Edge& e = shape.edge(i);
    if (!BRep_Tool::Degenerated(e)) {
      BRepAdaptor_Curve curve(e);
      count += GCPnts_AbscissaPoint::Length(curve) < 1e-5 ? 1 : 0;
    }
  }
  return count;
}

// A ring between radii 12 and 15 about the z axis that runs into a wall
// (y >= 12) tangentially, as a boss of a design whose sketch put the end
// of the line along the wall `delta` short of the point where its circle
// touches the wall (the solver's noise along a tangent): the wall with the
// ring's first quarter (x, y >= 0) joined to it, which ends exactly at the
// tangent point (0, 12), and the rest of the ring (from the wall round to
// the x axis) as the tool, all 10 high.
struct TangentBoss {
  ShapePtr target;
  ShapePtr tool;
};

TangentBoss tangent_boss(double delta) {
  const auto line = [](int curve, gp_Pnt2d a, gp_Pnt2d b) {
    Segment s;
    s.name = "c" + std::to_string(curve);
    s.start = a;
    s.end = b;
    return s;
  };
  const auto arc = [](int curve, double radius, double from, double to) {
    Segment s;
    s.name = "c" + std::to_string(curve);
    s.kind = SegmentKind::Arc;
    s.center = gp_Pnt2d(0, 0);
    s.radius = radius;
    s.start_angle = from;
    s.end_angle = to;
    return s;
  };
  const auto region = [](std::vector<Segment> segments) {
    Region r;
    r.name = "r{";
    for (const Segment& s : segments) {
      r.name += (r.name.size() > 2 ? "," : "") + s.name;
    }
    r.name += "}";
    Loop loop;
    loop.segments = std::move(segments);
    r.loops.push_back(std::move(loop));
    return r;
  };
  const ShapePtr wall = extrude("F1", {rectangle(1, -20, 12, 40, 18)}, 0, 10);
  const ShapePtr quarter = extrude("F2",
                                   {region({line(5, {12, 0}, {15, 0}), arc(6, 15, 0, kPi / 2),
                                            line(7, {0, 15}, {0, 12}), arc(8, 12, 0, kPi / 2)})},
                                   0, 10);
  TangentBoss boss;
  boss.target = run(BooleanOp::Join, {wall}, quarter).pieces.at(0).shape;
  const double touch = kPi / 2 + delta / 12;
  const double rim = kPi - std::asin(12.0 / 15.0);
  boss.tool = extrude("F3",
                      {region({line(9, {15 * std::cos(rim), 12}, {12 * std::cos(touch), 12 * std::sin(touch)}),
                               arc(10, 12, touch, 2 * kPi), line(11, {12, 0}, {15, 0}), arc(12, 15, rim, 2 * kPi)})},
                      0, 10);
  return boss;
}

void test_join_within_a_tolerance_of_a_boss_touching_a_wall_tangentially() {
  // Exact, the join keeps the tool's end 1.17 micrometres from the
  // target's tangent edge: a strip of the wall that wide is left between
  // the two parts of the ring's inner cylinder, its ends edges of zero
  // length (on which a chamfer of the ring failed, mitcad#326). Within the
  // import's tolerance the join merges them: the body of an exact end.
  const TangentBoss exact_end = tangent_boss(0.0);
  const BooleanResult reference = run_within(exact_end.target, exact_end.tool, 0.01);
  CHECK(reference.pieces.size() == 1);
  const TangentBoss noisy = tangent_boss(1.17e-6);
  const BooleanResult exact = run(BooleanOp::Join, {noisy.target}, noisy.tool);
  CHECK(exact.pieces.size() == 1);
  if (exact.pieces.size() == 1) {
    // (The strip the exact join leaves: this test's premise.)
    CHECK(zero_length_edges(*exact.pieces[0].shape) > 0);
  }
  const BooleanResult within = run_within(noisy.target, noisy.tool, 0.01);
  CHECK((within.touched == std::vector<bool>{true}));
  CHECK(within.pieces.size() == 1);
  if (within.pieces.size() == 1 && reference.pieces.size() == 1) {
    const Shape& joined = *within.pieces[0].shape;
    CHECK((within.pieces[0].sources == std::vector<std::size_t>{0}));
    CHECK(zero_length_edges(joined) == 0);
    CHECK(joined.face_count() == reference.pieces[0].shape->face_count());
    CHECK(near(volume(joined), volume(*reference.pieces[0].shape)));
    CHECK(faces_named(joined, "F3:side(c10)") == 1);
    CHECK(edge_names_unique(joined));
  }
}

// A tube boss on a plate (along -x: the plate x 0..2, radius 20, the tube
// x 2..12, radius 6.5) with a bore of radius 4 through both, made by two
// cuts whose cylinders' seams lie `first` and `second` degrees about the
// axis from +y, merged into one face: its seam steps between them at x = 2.
ShapePtr bored_boss(double first, double second) {
  const gp_Dir along(-1, 0, 0);
  const auto seam = [](double degrees) {
    const double a = degrees * M_PI / 180.0;
    return gp_Dir(0, std::cos(a), std::sin(a));
  };
  const TopoDS_Shape plate = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(2, 0, 0), along), 20, 2).Shape();
  const TopoDS_Shape tube = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(12, 0, 0), along), 6.5, 10).Shape();
  const TopoDS_Shape low = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(2, 0, 0), along, seam(first)), 4, 3).Shape();
  const TopoDS_Shape high = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(13, 0, 0), along, seam(second)), 4, 11).Shape();
  TopoDS_Shape boss = BRepAlgoAPI_Fuse(plate, tube).Shape();
  boss = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(boss, low).Shape(), high).Shape();
  ShapeUpgrade_UnifySameDomain unify(boss, true, true, true);
  unify.Build();
  std::vector<Shape::NamedFace> names;
  int i = 0;
  for (TopExp_Explorer it(unify.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    names.push_back({it.Current(), {"F1:import(" + std::to_string(i++) + ")"}});
  }
  return std::make_shared<Shape>(unify.Shape(), names);
}

void test_cut_of_a_thread_in_a_bore_whose_seam_steps() {
  // A thread cut into the bore: a circle of radius 1 on the bore's surface
  // (in the plane y = 0, 13 from the plate's face), 12 turns of pitch 3 along
  // -x, its pieces' seams shifted by a sixth of a turn. Where the bore's
  // seam steps a quarter turn, OCCT's cut removed nothing; the bore rebuilt
  // with one straight seam takes the thread (mitcad#235).
  Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  HelixSweepSpec spec;
  spec.feature = "F2";
  spec.frame = xz;
  spec.regions = {circle(1, 13, 4, 1)};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(-1, 0, 0));
  spec.pitch = 3;
  spec.revolutions = 12;
  spec.piece_shift = 1.0 / 6.0;
  const ShapePtr thread = helix_sweep(spec);
  const ShapePtr straight = bored_boss(0, 0);
  const BooleanResult reference = run(BooleanOp::Cut, {straight}, thread);
  CHECK(reference.pieces.size() == 1);
  const double removed = reference.pieces.empty() ? 0.0 : volume(*straight) - volume(*reference.pieces[0].shape);
  // Half the thread's section along the 4 turns in the bore.
  CHECK(removed > 150.0 && removed < 200.0);
  for (const auto& [first, second] : {std::pair{0.0, 90.0}, std::pair{180.0, 270.0}}) {
    const ShapePtr stepping = bored_boss(first, second);
    const BooleanResult cut = run(BooleanOp::Cut, {stepping}, thread);
    CHECK((cut.touched == std::vector<bool>{true}));
    CHECK(cut.pieces.size() == 1);
    if (cut.pieces.size() == 1) {
      CHECK_NEAR_TOL(volume(*stepping) - volume(*cut.pieces[0].shape), removed, 1e-4);
      CHECK(BRepCheck_Analyzer(cut.pieces[0].shape->occt()).IsValid());
    }
  }
}

void test_cut_hole_names_the_walls() {
  const ShapePtr drill = extrude("F4", {circle(5, 30, 20, 5)}, 0, 30);
  const BooleanResult result = run(BooleanOp::Cut, {block()}, drill);
  CHECK(result.touched == std::vector<bool>{true});
  CHECK(result.pieces.size() == 1);
  const Shape& holed = *result.pieces.at(0).shape;
  CHECK(near(volume(holed), 60.0 * 40.0 * 20.0 - kPi * 25.0 * 20.0));
  CHECK(faces_named(holed, "F4:side(c5)") == 1);
  CHECK(holed.find_faces("F4:end(r{c5})").empty());
  CHECK(edge_names_unique(holed));
  // The rims of the hole.
  CHECK(holed.find_edges(edge(end_cap("F2", 1), "F4:side(c5)")).size() == 1);
  CHECK(holed.find_edges(edge(start_cap("F2", 1), "F4:side(c5)")).size() == 1);
}

void test_cut_splitting_a_face_numbers_the_pieces() {
  // A slot across the whole top, 5 mm deep: the top face splits in two, the
  // front and back faces stay whole (with a notch).
  const ShapePtr slot = extrude("F4", {rectangle(5, 25, -5, 10, 50)}, 15, 25);
  const BooleanResult result = run(BooleanOp::Cut, {block()}, slot);
  CHECK(result.pieces.size() == 1);
  const Shape& slotted = *result.pieces.at(0).shape;
  const std::string top = end_cap("F2", 1);
  CHECK(slotted.find_faces(top).size() == 2);
  CHECK(faces_named(slotted, top + "#0") == 1 && faces_named(slotted, top + "#1") == 1);
  // Pieces are numbered in geometric order: #0 is the one at smaller x.
  const std::vector<int> first = slotted.find_faces(top + "#0");
  CHECK(first.size() == 1 && near(bounding_box(Shape(slotted.face(first.at(0)))).max.X(), 25));
  CHECK(faces_named(slotted, side("F2", 1, 0)) == 1);
  CHECK(edge_names_unique(slotted));
  // An edge between the top and the front face exists on both sides of the
  // slot: the reference without "#k" resolves to both, numbered ones to one.
  const std::string top_front = edge(top, side("F2", 1, 0));
  CHECK(slotted.find_edges(top_front).size() == 2);
  CHECK(slotted.find_edges(edge(top + "#1", side("F2", 1, 0))).size() == 1);
}

void test_cut_splitting_the_body() {
  // A slot through the whole height splits the block into two pieces.
  const ShapePtr saw = extrude("F4", {rectangle(5, 25, -5, 10, 50)}, -1, 21);
  const BooleanResult result = run(BooleanOp::Cut, {block()}, saw);
  CHECK(result.touched == std::vector<bool>{true});
  CHECK(result.pieces.size() == 2);
  if (result.pieces.size() == 2) {
    for (const BooleanPiece& piece : result.pieces) {
      CHECK(piece.sources == std::vector<std::size_t>{0});
    }
    // Geometric order: the left piece first.
    CHECK(near(volume(*result.pieces[0].shape), 25.0 * 40.0 * 20.0));
    CHECK(near(volume(*result.pieces[1].shape), 25.0 * 40.0 * 20.0));
    CHECK(bounding_box(*result.pieces[0].shape).max.X() < 30);
    // The split faces are numbered across both pieces.
    const std::string top = end_cap("F2", 1);
    CHECK(faces_named(*result.pieces[0].shape, top + "#0") == 1);
    CHECK(faces_named(*result.pieces[1].shape, top + "#1") == 1);
    CHECK(faces_named(*result.pieces[0].shape, side("F2", 1, 3)) == 1);
    CHECK(faces_named(*result.pieces[1].shape, side("F2", 1, 1)) == 1);
  }
}

void test_cut_misses_or_removes() {
  const ShapePtr body = block();
  const ShapePtr far = extrude("F4", {rectangle(5, 100, 0, 10, 10)}, 0, 20);
  const BooleanResult missed = run(BooleanOp::Cut, {body}, far);
  CHECK(missed.touched == std::vector<bool>{false} && missed.pieces.empty());
  // Touching a face removes nothing either.
  const ShapePtr flush = extrude("F4", {rectangle(5, 60, 0, 10, 40)}, 0, 20);
  CHECK(run(BooleanOp::Cut, {body}, flush).touched == std::vector<bool>{false});
  // A tool in the body's box that passes it by (in the notch of an L):
  // the cut leaves it as it was, unchecked (mitcad#181).
  const ShapePtr notch = extrude("F4", {rectangle(5, 40, 20, 30, 30)}, -1, 30);
  const BooleanResult cut_notch = run(BooleanOp::Cut, {body}, notch);
  CHECK(cut_notch.touched == std::vector<bool>{true} && cut_notch.pieces.size() == 1);
  if (cut_notch.pieces.size() == 1) {
    const ShapePtr l_shape = cut_notch.pieces[0].shape;
    const ShapePtr inside = extrude("F6", {rectangle(9, 50, 30, 5, 5)}, 5, 10);
    const BooleanResult passed = run(BooleanOp::Cut, {l_shape}, inside);
    CHECK(passed.touched == std::vector<bool>{false} && passed.pieces.empty());
    CHECK(near(volume(*l_shape), 60.0 * 40.0 * 20.0 - 20.0 * 20.0 * 20.0));
  }
  const ShapePtr all = extrude("F4", {rectangle(5, -10, -10, 80, 60)}, -1, 30);
  const BooleanResult removed = run(BooleanOp::Cut, {body}, all);
  CHECK(removed.touched == std::vector<bool>{true} && removed.pieces.empty());
  // The inputs are cached model results and stay untouched.
  CHECK(near(volume(*body), 60.0 * 40.0 * 20.0));
  CHECK(body->face_count() == 6);
}

void test_cut_changes_by_more_than_a_billionth() {
  // A body changes when its volume changes by more than 1e-9 of it
  // (48 000 mm3 here): a pocket of 1e-4 mm3 does, one of 2e-5 mm3 does not.
  const ShapePtr deep = extrude("F4", {rectangle(5, 30, 20, 0.1, 0.1)}, 19.99, 21);
  CHECK(run(BooleanOp::Cut, {block()}, deep).touched == std::vector<bool>{true});
  const ShapePtr shallow = extrude("F4", {rectangle(5, 30, 20, 0.1, 0.1)}, 19.998, 21);
  CHECK(run(BooleanOp::Cut, {block()}, shallow).touched == std::vector<bool>{false});
}

void test_intersect() {
  const ShapePtr post = extrude("F4", {circle(5, 30, 20, 5)}, 10, 30);
  const BooleanResult result = run(BooleanOp::Intersect, {block()}, post);
  CHECK(result.touched == std::vector<bool>{true});
  CHECK(result.pieces.size() == 1);
  const Shape& common = *result.pieces.at(0).shape;
  CHECK(near(volume(common), kPi * 25.0 * 10.0));
  CHECK(faces_named(common, "F4:side(c5)") == 1);
  CHECK(faces_named(common, "F4:start(r{c5})") == 1);
  CHECK(faces_named(common, end_cap("F2", 1)) == 1);
  // A tool around the whole body leaves it as it is.
  const ShapePtr around = extrude("F4", {rectangle(5, -10, -10, 80, 60)}, -1, 30);
  CHECK(run(BooleanOp::Intersect, {block()}, around).touched == std::vector<bool>{false});
}

// A face split by curves that leave a vertex tangentially to each other
// (OCCT port patch 0360, mitcad#235).
struct TangentSplit {
  int faces = 0;
  bool valid = false;
  double smallest = 0;   // the sliver's area as split
  double expected = 0;   // the sliver's area from its own wire
  double total = 0;
};

// A square split by a circle of radius 1.5 and a curve inside it that
// leaves the circle's point (1.5, 0) tangentially (a B-spline through the
// points of a circle of radius `inner` touching the circle there, up to
// `sweep` radians round it, its parameters from 0 to `range`), and runs on
// in a line to the circle at 120 degrees. The two vertices have
// `tolerance`.
TangentSplit split_by_tangent_curves(double inner, double sweep, double range, double tolerance) {
  TangentSplit out;
  const gp_Pln plane(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  const TopoDS_Face square = BRepBuilderAPI_MakeFace(plane, -2, 2, -2, 2).Face();
  const occ::handle<Geom_Circle> outer = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 1.5);
  const int count = 24;
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, count);
  occ::handle<NCollection_HArray1<double>> parameters = new NCollection_HArray1<double>(1, count);
  for (int i = 0; i < count; ++i) {
    const double angle = sweep * i / (count - 1);
    points->SetValue(i + 1, gp_Pnt(1.5 - inner + inner * std::cos(angle), inner * std::sin(angle), 0));
    parameters->SetValue(i + 1, range * i / (count - 1));
  }
  GeomAPI_Interpolate interpolate(points, parameters, false, 1e-9);
  interpolate.Perform();
  if (!interpolate.IsDone()) {
    return out;
  }
  const gp_Pnt end = points->Value(count);
  const gp_Pnt meet(1.5 * std::cos(2 * kPi / 3), 1.5 * std::sin(2 * kPi / 3), 0);
  BRep_Builder builder;
  const TopoDS_Vertex touch = BRepBuilderAPI_MakeVertex(gp_Pnt(1.5, 0, 0)).Vertex();
  const TopoDS_Vertex corner = BRepBuilderAPI_MakeVertex(end).Vertex();
  const TopoDS_Vertex on_circle = BRepBuilderAPI_MakeVertex(meet).Vertex();
  builder.UpdateVertex(touch, tolerance);
  builder.UpdateVertex(on_circle, tolerance);
  const TopoDS_Edge circle_edge =
      BRepBuilderAPI_MakeEdge(outer, touch, touch, 0, 2 * kPi).Edge();
  const TopoDS_Edge curve_edge = BRepBuilderAPI_MakeEdge(interpolate.Curve(), touch, corner).Edge();
  const TopoDS_Edge line_edge = BRepBuilderAPI_MakeEdge(corner, on_circle).Edge();
  const TopoDS_Edge arc_edge = BRepBuilderAPI_MakeEdge(outer, touch, on_circle, 0, 2 * kPi / 3).Edge();
  GProp_GProps sliver;
  BRepGProp::SurfaceProperties(
      BRepBuilderAPI_MakeFace(plane, BRepBuilderAPI_MakeWire(curve_edge, line_edge, arc_edge).Wire()).Face(), sliver);
  out.expected = std::abs(sliver.Mass());
  BRepAlgoAPI_Splitter splitter;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(square);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(circle_edge);
  tools.Append(curve_edge);
  tools.Append(line_edge);
  splitter.SetArguments(arguments);
  splitter.SetTools(tools);
  splitter.Build();
  if (!splitter.IsDone() || splitter.HasErrors()) {
    return out;
  }
  out.valid = true;
  out.smallest = 1e9;
  for (TopExp_Explorer it(splitter.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(it.Current(), props);
    ++out.faces;
    out.total += props.Mass();
    out.smallest = std::min(out.smallest, props.Mass());
    out.valid = out.valid && BRepCheck_Analyzer(it.Current()).IsValid();
  }
  return out;
}

// The curve follows a circle 0.01 and 0.001 smaller for 1.5 radians, with
// ten units of parameter over its 2.2 mm, and the vertices have the
// tolerance of a thread's section edges (5e-6 mm). Each
// curve's direction at the vertex was measured at a step of its own from it
// (the circle's from its curvature and the vertex's tolerance, the curve's
// from its parametrisation): the circle's farther away, where it had turned
// more than the curve though the curve turns faster. Their order about the
// vertex came out the wrong way round, and the sliver between them as a
// face with the disc's boundary too, of negative area (a part's counterbore
// through an internal thread whose crest lies on the hole's circle left the
// body empty). The pieces are the square outside the circle, the disc less
// the sliver and the sliver.
void test_split_by_curves_tangent_at_a_vertex() {
  for (const double inner : {1.49, 1.499}) {
    const TangentSplit split = split_by_tangent_curves(inner, 1.5, 10, 5e-6);
    if (split.faces != 3 || !split.valid || !near(split.smallest, split.expected, 1e-5) ||
        !near(split.total, 16, 1e-6)) {
      std::fprintf(stderr, "split by tangent curves, radius %g: %d faces, %s, sliver %.9g (%.9g), total %.9g\n", inner,
                   split.faces, split.valid ? "valid" : "invalid", split.smallest, split.expected, split.total);
      ++failures;
    }
  }
}

// The folder of geometry cases kept outside the repository
// (MITCAD_GEOMETRY_CASES: bodies cut from reported models, which are not
// committed here); empty when it is not set.
std::string geometry_cases() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, "MITCAD_GEOMETRY_CASES") == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv("MITCAD_GEOMETRY_CASES");
  return value != nullptr ? std::string(value) : std::string();
#endif
}

// A body of a geometry case read from its B-rep file, its faces named as
// imported by `feature`; null when the file cannot be read.
ShapePtr case_body(const std::string& path, const std::string& feature) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    return nullptr;
  }
  std::ostringstream data;
  data << in.rdbuf();
  const std::string bytes = data.str();
  return import_body(read_brep_data(bytes.data(), bytes.size()), feature, 0);
}

void test_join_that_loses_volume_is_retried() {
  // (mitcad#964) A reported model's pad, flush with two walls of its body
  // that meet tangentially (the rim and a concave wall, merged faces of
  // the body's earlier joins and cuts), over a pocket below it: OCCT's
  // exact union takes the pad's overlap with the body away. The join
  // finds that its result lost volume and is retried with a fuzzy value
  // of 1e-5 mm: the pad adds what FreeCAD's adds, 134.754 mm3. The two
  // bodies are the case `caliper-pad028` of MITCAD_GEOMETRY_CASES
  // (`body.brep`, the body before the pad; `pad.brep`, the pad's tool);
  // no synthetic layout reproduced OCCT's failure.
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_join_that_loses_volume_is_retried: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string folder = cases + "/caliper-pad028/";
  const ShapePtr body = case_body(folder + "body.brep", "F1");
  const ShapePtr pad = case_body(folder + "pad.brep", "F2");
  CHECK(body != nullptr && pad != nullptr);
  if (body == nullptr || pad == nullptr) {
    std::printf("test_join_that_loses_volume_is_retried: %sbody.brep or pad.brep not read\n", folder.c_str());
    return;
  }
  const double before = volume(*body);
  const double added = 134.75405463195966; // FreeCAD's result less its body before the pad
  // The test's premise: OCCT's exact union adds less than that (on the
  // inputs as they are: non-destructive, as the join's own).
  const double exact = [&] {
    BRepAlgoAPI_Fuse fuse;
    NCollection_List<TopoDS_Shape> arguments;
    NCollection_List<TopoDS_Shape> tools;
    arguments.Append(body->occt());
    tools.Append(pad->occt());
    fuse.SetArguments(arguments);
    fuse.SetTools(tools);
    fuse.SetNonDestructive(true);
    fuse.Build();
    GProp_GProps props;
    BRepGProp::VolumeProperties(fuse.Shape(), props);
    return props.Mass();
  }();
  CHECK(exact < before + added - 1.0);
  const BooleanResult joined = run(BooleanOp::Join, {body}, pad);
  CHECK((joined.touched == std::vector<bool>{true}));
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() == 1) {
    CHECK((joined.pieces[0].sources == std::vector<std::size_t>{0}));
    const double after = volume(*joined.pieces[0].shape);
    CHECK(after >= before);
    CHECK_NEAR_TOL(after - before, added, 1e-6);
    CHECK(BRepCheck_Analyzer(joined.pieces[0].shape->occt()).IsValid());
  }
}

} // namespace

void boolean_tests() {
  guarded("test_split_by_curves_tangent_at_a_vertex", test_split_by_curves_tangent_at_a_vertex);
  test_join_keeps_names_and_names_new_edges();
  test_join_merges_flush_faces();
  guarded("test_join_merges_coaxial_cylinders_whatever_their_seams",
          test_join_merges_coaxial_cylinders_whatever_their_seams);
  test_slivers_are_no_pieces();
  test_join_with_several_participants();
  test_join_of_many_copies_finds_what_they_touch();
  test_join_with_a_plate_of_many_holes();
  test_cut_hole_names_the_walls();
  test_cut_splitting_a_face_numbers_the_pieces();
  test_cut_splitting_the_body();
  test_cut_misses_or_removes();
  test_cut_changes_by_more_than_a_billionth();
  test_intersect();
  // Joins within a tolerance (mitcad#298).
  test_join_within_a_tolerance_reaches_a_tool_micrometres_away();
  test_join_within_a_tolerance_on_a_near_coincident_face();
  test_join_within_a_tolerance_rebuilds_lost_solids();
  test_cut_of_a_thread_in_a_bore_whose_seam_steps();
  test_join_within_a_tolerance_of_a_boss_touching_a_wall_tangentially();
  // A join that loses volume (mitcad#964), on a reported model's bodies.
  guarded("test_join_that_loses_volume_is_retried", test_join_that_loses_volume_is_retried);
}

} // namespace test
