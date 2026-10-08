// SPDX-License-Identifier: MIT
// Join, Cut and Intersect with participant bodies: pieces, their sources and
// the face names they carry.

#include "check.hpp"

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

} // namespace

void boolean_tests() {
  test_join_keeps_names_and_names_new_edges();
  test_join_merges_flush_faces();
  test_join_with_several_participants();
  test_join_of_many_copies_finds_what_they_touch();
  test_join_with_a_plate_of_many_holes();
  test_cut_hole_names_the_walls();
  test_cut_splitting_a_face_numbers_the_pieces();
  test_cut_splitting_the_body();
  test_cut_misses_or_removes();
  test_cut_changes_by_more_than_a_billionth();
  test_intersect();
}

} // namespace test
