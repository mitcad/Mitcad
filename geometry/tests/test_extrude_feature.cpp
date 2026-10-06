// SPDX-License-Identifier: MIT
// The extrude options: two sides, start offsets and planes, tapers,
// thin walls and extents up to planes, faces and bodies. Volumes follow
// the T0 reference models (ext_*).

#include <algorithm>
#include <cmath>
#include <utility>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

ExtrudeSide distance(double d, double taper = 0.0) {
  ExtrudeSide side;
  side.distance = d;
  side.taper = taper;
  return side;
}

ExtrudeFeatureSpec spec_of(std::vector<Region> regions, double length) {
  ExtrudeFeatureSpec spec;
  spec.feature = "F2";
  spec.regions = std::move(regions);
  spec.side1 = distance(length);
  return spec;
}

// The only face with the name lies in the plane z = `z`.
bool face_at_z(const Shape& shape, const std::string& name, double z) {
  const std::vector<int> found = shape.find_faces(name);
  if (found.size() != 1) {
    std::fprintf(stderr, "%zu faces named %s\n", found.size(), name.c_str());
    return false;
  }
  const BoundingBox box = bounding_box(Shape(shape.face(found[0])));
  return near(box.min.Z(), z) && near(box.max.Z(), z);
}

// Square frustum of base side a, top side b, height h.
double frustum(double a, double b, double h) { return h / 3.0 * (a * a + b * b + a * b); }

void test_two_sides_and_symmetric() {
  // T0 ext_symmetric_full: 20 mm in total, z -10..10. The far cap of side
  // one is the start (`startFaces` in .f3d).
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 60, 40)}, 10);
  spec.side2 = distance(10);
  const ShapePtr symmetric = extrude_feature(spec);
  CHECK(near(volume(*symmetric), 48000));
  CHECK(face_at_z(*symmetric, start_cap("F2", 1), 10));
  CHECK(face_at_z(*symmetric, end_cap("F2", 1), -10));
  CHECK(faces_named(*symmetric, side("F2", 1, 0)) == 1);
  CHECK(symmetric->face_count() == 6 && edge_names_unique(*symmetric));

  // T0 ext_two_sides: 15 mm along the normal, 5 mm against it.
  spec.side1 = distance(15);
  spec.side2 = distance(5);
  const ShapePtr two = extrude_feature(spec);
  CHECK(near(volume(*two), 48000));
  CHECK(near(bounding_box(*two).min.Z(), -5) && near(bounding_box(*two).max.Z(), 15));

  // Along the reversed direction side one goes down.
  spec.direction = gp_Dir(0, 0, -1);
  CHECK(face_at_z(*extrude_feature(spec), start_cap("F2", 1), -15));
}

void test_start() {
  // T0 ext_start_offset: 10 mm up, then 20 mm: z 10..30.
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 60, 40)}, 20);
  spec.start_offset = 10;
  const ShapePtr block = extrude_feature(spec);
  CHECK(near(volume(*block), 48000));
  CHECK(face_at_z(*block, start_cap("F2", 1), 10) && face_at_z(*block, end_cap("F2", 1), 30));

  // From a plane parallel to the sketch: the same; its normal's sense does
  // not matter.
  Target plane;
  plane.plane = gp_Pln(gp_Pnt(0, 0, 10), gp_Dir(0, 0, -1));
  spec.start_offset = 0;
  spec.start = plane;
  CHECK(face_at_z(*extrude_feature(spec), start_cap("F2", 1), 10));

  // From an oblique plane z = 5 + x/4: the slab between it and its copy
  // 20 mm higher.
  spec.start->plane = gp_Pln(gp_Pnt(0, 0, 5), gp_Dir(-0.25, 0, 1));
  const ShapePtr slanted = extrude_feature(spec);
  CHECK(near(volume(*slanted), 60 * 40 * 20));
  const BoundingBox box = bounding_box(*slanted);
  CHECK(near(box.min.Z(), 5) && near(box.max.Z(), 40));
  CHECK(faces_named(*slanted, start_cap("F2", 1)) == 1 && faces_named(*slanted, end_cap("F2", 1)) == 1);
  CHECK(faces_named(*slanted, side("F2", 1, 1)) == 1 && edge_names_unique(*slanted));
  spec.side1.taper = 0.1;
  CHECK(throws_with([&] { extrude_feature(spec); }, "parallel to the sketch"));
}

void test_taper() {
  // T0 ext_taper: 40 x 40, 10 high, -10 degrees: the sides lean in.
  const double angle = 10.0 * kPi / 180.0;
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 40, 40)}, 10);
  spec.side1.taper = -angle;
  const ShapePtr in = extrude_feature(spec);
  CHECK(near(volume(*in), frustum(40, 40 - 20 * std::tan(angle), 10)));
  CHECK(near(volume(*in), 14630.8, 1e-5));
  CHECK(faces_named(*in, side("F2", 1, 0)) == 1 && in->face_count() == 6);
  CHECK(face_at_z(*in, end_cap("F2", 1), 10) && edge_names_unique(*in));
  spec.side1.taper = angle;
  CHECK(near(volume(*extrude_feature(spec)), frustum(40, 40 + 20 * std::tan(angle), 10)));

  // The hole shrinks as the outside grows.
  const double five = 5.0 * kPi / 180.0;
  const double k = std::tan(five);
  Region holed = rectangle(1, 0, 0, 40, 40);
  holed.loops.push_back(circle(5, 20, 20, 5).loops[0]);
  spec.regions = {holed};
  spec.side1.taper = five;
  const double outer = (std::pow(40 + 20 * k, 3) - std::pow(40, 3)) / (6 * k);
  const double hole = kPi * (std::pow(5, 3) - std::pow(5 - 10 * k, 3)) / (3 * k);
  const ShapePtr holed_block = extrude_feature(spec);
  CHECK(near(volume(*holed_block), outer - hole));
  CHECK(faces_named(*holed_block, "F2:side(c5)") == 1);

  // Symmetric: both halves narrow away from the sketch plane; each side
  // face is two pieces.
  spec = spec_of({rectangle(1, 0, 0, 40, 40)}, 10);
  spec.side1.taper = -angle;
  spec.side2 = distance(10, -angle);
  const ShapePtr both = extrude_feature(spec);
  CHECK(near(volume(*both), 2 * frustum(40, 40 - 20 * std::tan(angle), 10)));
  CHECK(faces_named(*both, side("F2", 1, 0) + "#0") == 1 && faces_named(*both, side("F2", 1, 0) + "#1") == 1);
  CHECK(both->find_faces(side("F2", 1, 0)).size() == 2);
  CHECK(face_at_z(*both, start_cap("F2", 1), 10) && face_at_z(*both, end_cap("F2", 1), -10));
  CHECK(edge_names_unique(*both));

  spec.side1.taper = 1.5;
  CHECK(throws_with([&] { extrude_feature(spec); }, "between -85 and 85"));

  // A taper that shrinks an arc to nothing closes the profile (OCCT's
  // draft would intersect the cones past their apex, endlessly at times):
  // a circle of radius 5 leaning in by 20 degrees over 20 mm, and a hole
  // of radius 5 that a widening taper closes.
  const double twenty = 20.0 * kPi / 180.0;
  spec = spec_of({circle(1, 0, 0, 5)}, 20);
  spec.side1.taper = -twenty;
  CHECK(throws_with([&] { extrude_feature(spec); }, "closes the profile"));
  spec.side1.distance = 10;
  CHECK(near(volume(*extrude_feature(spec)), kPi * 10 / 3 * (25 + 5 * (5 - 10 * std::tan(twenty)) +
                                                              std::pow(5 - 10 * std::tan(twenty), 2))));
  Region ring = rectangle(1, -20, -20, 40, 40);
  ring.loops.push_back(circle(5, 0, 0, 5).loops[0]);
  spec = spec_of({ring}, 20);
  spec.side1.taper = twenty;
  CHECK(throws_with([&] { extrude_feature(spec); }, "closes the profile"));
}

void test_thin() {
  // T0 ext_thin_closed: a 2 mm wall centred on a 40 x 20 outline, 10 high.
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 40, 20)}, 10);
  spec.side1.thin = ThinWall{WallLocation::Center, 2};
  const ShapePtr frame = extrude_feature(spec);
  CHECK(near(volume(*frame), (42 * 22 - 38 * 18) * 10));
  CHECK(frame->face_count() == 10);
  for (int i = 0; i < 4; ++i) {
    CHECK(faces_named(*frame, "F2:outer(" + rectangle_segment(1, i) + ")") == 1);
    CHECK(faces_named(*frame, "F2:inner(" + rectangle_segment(1, i) + ")") == 1);
  }
  CHECK(faces_named(*frame, start_cap("F2", 1)) == 1 && faces_named(*frame, end_cap("F2", 1)) == 1);
  CHECK(edge_names_unique(*frame));
  // Side 1 is outside the region, side 2 inside.
  spec.side1.thin->location = WallLocation::Side1;
  CHECK(near(volume(*extrude_feature(spec)), (44 * 24 - 40 * 20) * 10));
  spec.side1.thin->location = WallLocation::Side2;
  CHECK(near(volume(*extrude_feature(spec)), (40 * 20 - 36 * 16) * 10));

  // A disc with a hole: a ring on each loop; side 1 goes into the hole.
  Region ring = circle(1, 0, 0, 20);
  ring.loops.push_back(circle(2, 0, 0, 10).loops[0]);
  spec = spec_of({ring}, 10);
  spec.side1.thin = ThinWall{WallLocation::Side1, 2};
  CHECK(near(volume(*extrude_feature(spec)), kPi * ((22 * 22 - 20 * 20) + (10 * 10 - 8 * 8)) * 10));

  // Different walls on the two sides meet in a step at the sketch plane.
  spec = spec_of({rectangle(1, 0, 0, 40, 20)}, 10);
  spec.side1.thin = ThinWall{WallLocation::Center, 2};
  spec.side2 = distance(5);
  spec.side2->thin = ThinWall{WallLocation::Side2, 4};
  const ShapePtr stepped = extrude_feature(spec);
  CHECK(near(volume(*stepped), (42 * 22 - 38 * 18) * 10 + (40 * 20 - 32 * 12) * 5));
  CHECK(!stepped->find_faces("F2:mid(" + rectangle_region_name(1) + ")").empty());
  spec.side2->thin.reset();
  CHECK(throws_with([&] { extrude_feature(spec); }, "both sides of a thin extrusion"));
}

void test_to_planes() {
  // T0 ext_to_object_plane: up to a plane 25 mm above the sketch.
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 60, 40)}, 0);
  Target plane;
  plane.plane = gp_Pln(gp_Pnt(0, 0, 25), gp_Dir(0, 0, 1));
  spec.side1.target = plane;
  const ShapePtr block = extrude_feature(spec);
  CHECK(near(volume(*block), 60000));
  CHECK(face_at_z(*block, end_cap("F2", 1), 25) && face_at_z(*block, start_cap("F2", 1), 0));
  CHECK(block->face_count() == 6 && edge_names_unique(*block));
  // A positive offset makes it longer.
  spec.side1.target->offset = 2;
  CHECK(near(volume(*extrude_feature(spec)), 60 * 40 * 27));
  // An oblique plane z = 20 + x/4: the end follows it.
  spec.side1.target->offset = 0;
  spec.side1.target->plane = gp_Pln(gp_Pnt(0, 0, 20), gp_Dir(-0.25, 0, 1));
  CHECK(near(volume(*extrude_feature(spec)), 60 * 40 * 27.5));
  // Behind the start, or along the plane.
  spec.side1.target->plane = gp_Pln(gp_Pnt(0, 0, -5), gp_Dir(0, 0, 1));
  CHECK(throws_with([&] { extrude_feature(spec); }, "behind the start"));
  spec.side1.target->plane = gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0));
  CHECK(throws_with([&] { extrude_feature(spec); }, "parallel to the target plane"));
  // Two sides: up to the plane, and 5 mm down.
  spec.side1.target->plane = gp_Pln(gp_Pnt(0, 0, 25), gp_Dir(0, 0, 1));
  spec.side2 = distance(5);
  CHECK(near(volume(*extrude_feature(spec)), 60 * 40 * 30));
}

void test_to_faces_and_bodies() {
  // T0 ext_to_object_face: a 20 mm circle on a plane 40 mm up extruded down
  // to the block's top.
  const ShapePtr base = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  ExtrudeFeatureSpec post;
  post.feature = "F4";
  post.frame.origin = gp_Pnt(0, 0, 40);
  post.regions = {circle(5, 30, 20, 10)};
  post.direction = gp_Dir(0, 0, -1);
  Target top;
  top.kind = Target::Kind::Face;
  top.body = base;
  top.face = end_cap("F2", 1);
  post.side1.target = top;
  const ShapePtr to_face = extrude_feature(post);
  CHECK(near(volume(*to_face), kPi * 100 * 20));
  CHECK(face_at_z(*to_face, "F4:end(r{c5})", 20) && face_at_z(*to_face, "F4:start(r{c5})", 40));
  CHECK(to_face->face_count() == 3 && edge_names_unique(*to_face));
  // Not extended (the face and its neighbours), with an offset.
  post.side1.target->extend = false;
  post.side1.target->offset = 2;
  CHECK(near(bounding_box(*extrude_feature(post)).min.Z(), 18));
  // A body: to its first face, or through it.
  Target body;
  body.kind = Target::Kind::Body;
  body.body = base;
  post.side1.target = body;
  CHECK(near(bounding_box(*extrude_feature(post)).min.Z(), 20));
  post.side1.target->through = true;
  const ShapePtr through = extrude_feature(post);
  CHECK(near(volume(*through), kPi * 100 * 40) && near(bounding_box(*through).min.Z(), 0));
  CHECK(faces_named(*through, "F4:end(r{c5})") == 1);
  // Half of the circle passes the block's edge: not everywhere ended.
  post.side1.target->through = false;
  post.regions = {circle(5, 60, 20, 10)};
  CHECK(throws_with([&] { extrude_feature(post); }, "does not cut across the whole profile"));
  // Behind the start.
  post.regions = {circle(5, 30, 20, 10)};
  post.direction = gp_Dir(0, 0, 1);
  CHECK(throws_with([&] { extrude_feature(post); }, "behind the start"));

  // Up to a curved face: a 4 x 4 bar along -x to a cylinder of radius 10
  // standing on the origin.
  const ShapePtr pillar = extrude("F6", {circle(9, 0, 0, 10)}, 0, 20);
  ExtrudeFeatureSpec bar;
  bar.feature = "F7";
  bar.frame.origin = gp_Pnt(30, 0, 0);
  bar.frame.x_axis = gp_Dir(0, 1, 0);
  bar.frame.y_axis = gp_Dir(0, 0, 1);
  bar.regions = {rectangle(1, -2, 8, 4, 4)};
  bar.direction = gp_Dir(-1, 0, 0);
  Target side_face;
  side_face.kind = Target::Kind::Face;
  side_face.body = pillar;
  side_face.face = "F6:side(c9)";
  bar.side1.target = side_face;
  const double chord = 2 * std::sqrt(96.0) + 100 * std::asin(0.2);
  CHECK(near(volume(*extrude_feature(bar)), 4 * (4 * 30 - chord), 1e-6));
}

void test_clockwise_loops() {
  // Loops may run either way round: a clockwise outline with a
  // counter-clockwise hole extrudes like the other way round.
  Region region = rectangle(1, 0, 0, 40, 20);
  std::vector<Segment>& lines = region.loops[0].segments;
  std::reverse(lines.begin(), lines.end());
  for (Segment& line : lines) {
    std::swap(line.start, line.end);
  }
  Region hole = rectangle(5, 10, 5, 10, 10);
  region.loops.push_back(hole.loops[0]);
  const ShapePtr block = extrude_feature(spec_of({region}, 10));
  CHECK(near(volume(*block), (40 * 20 - 10 * 10) * 10));
  CHECK(faces_named(*block, side("F2", 1, 0)) == 1 && faces_named(*block, side("F2", 5, 0)) == 1);
}

void test_several_regions() {
  // Touching regions with a taper still fuse into one solid.
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 10, 10), rectangle(5, 10, 0, 10, 10)}, 5);
  spec.side1.taper = -0.05;
  const ShapePtr joined = extrude_feature(spec);
  CHECK(solids(*joined).size() == 1);
  spec.side1.taper = 0.0;
  spec.side1.distance = 0.0;
  CHECK(throws_with([&] { extrude_feature(spec); }, "greater than zero"));
  spec.regions.clear();
  CHECK(throws_with([&] { extrude_feature(spec); }, "no profiles"));
}

} // namespace

void extrude_feature_tests() {
  guarded("test_two_sides_and_symmetric", test_two_sides_and_symmetric);
  guarded("test_start", test_start);
  guarded("test_taper", test_taper);
  guarded("test_thin", test_thin);
  guarded("test_to_planes", test_to_planes);
  guarded("test_to_faces_and_bodies", test_to_faces_and_bodies);
  guarded("test_clockwise_loops", test_clockwise_loops);
  guarded("test_several_regions", test_several_regions);
}

} // namespace test
