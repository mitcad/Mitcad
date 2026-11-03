// SPDX-License-Identifier: MIT
// The extrude options: two sides, start offsets and planes, tapers,
// thin walls and extents up to planes, faces and bodies. Volumes follow
// the T0 reference models (ext_*).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <utility>

#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

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

// mitcad#765: a negative side ends on the other side's side; the solid lies
// between the two ends only, its cap there named as the short side's far
// cap, and the side holding it is tapered from the start as without the
// other's end (the short side's taper shapes nothing).
void test_negative_side() {
  // 50 along the normal and -30 against it: z 30..50.
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 60, 40)}, 50);
  spec.side2 = distance(-30);
  const ShapePtr up = extrude_feature(spec);
  CHECK(near(volume(*up), 60 * 40 * 20));
  CHECK(face_at_z(*up, start_cap("F2", 1), 50) && face_at_z(*up, end_cap("F2", 1), 30));
  CHECK(up->face_count() == 6 && edge_names_unique(*up));

  // Mirrored, -30 and 50: z -50..-30.
  spec.side1 = distance(-30);
  spec.side2 = distance(50);
  const ShapePtr down = extrude_feature(spec);
  CHECK(near(volume(*down), 60 * 40 * 20));
  CHECK(face_at_z(*down, start_cap("F2", 1), -30) && face_at_z(*down, end_cap("F2", 1), -50));

  // Tapered by -5 degrees on side one: the frustum of 0..50 less that of
  // 0..30, whatever side two's taper.
  const double angle = 5.0 * kPi / 180.0;
  const double k = std::tan(angle);
  spec = spec_of({rectangle(1, 0, 0, 40, 40)}, 50);
  spec.side1.taper = -angle;
  spec.side2 = distance(-30, 0.2);
  const ShapePtr tapered_part = extrude_feature(spec);
  CHECK(near(volume(*tapered_part), frustum(40, 40 - 100 * k, 50) - frustum(40, 40 - 60 * k, 30)));
  CHECK(face_at_z(*tapered_part, start_cap("F2", 1), 50) && face_at_z(*tapered_part, end_cap("F2", 1), 30));
  CHECK(BRepCheck_Analyzer(tapered_part->occt()).IsValid());

  // Ends that meet or cross are refused, as are two negative sides.
  spec = spec_of({rectangle(1, 0, 0, 60, 40)}, 50);
  spec.side2 = distance(-50);
  CHECK(throws_with([&] { extrude_feature(spec); }, "overlap"));
  spec.side1 = distance(-10);
  spec.side2 = distance(-5);
  CHECK(throws_with([&] { extrude_feature(spec); }, "both sides"));
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

// A taper that closes the profile partway (mitcad#1186): a C of lines, a
// 23 x 44 block less a 20 x 40 bite from its left side, which leaves
// strips 2 wide at the bottom and the top and 3 wide at the right. Leaning
// in by 45 degrees, the strips' walls move 1 mm in a millimetre: the bottom
// and top strips close 1 mm up, before the end of an extrusion 3 mm long.
// The error says so, naming the walls that meet; the same profile tapered
// less, or extruded less far, builds.
Region c_profile() {
  const double pts[8][2] = {{0, 0}, {23, 0}, {23, 44}, {0, 44}, {0, 42}, {20, 42}, {20, 2}, {0, 2}};
  Region region;
  Loop loop;
  std::string key = "r{";
  for (int i = 0; i < 8; ++i) {
    const auto curve = [](int k) { return "c" + std::to_string((k + 8) % 8 + 1); };
    Segment segment;
    segment.kind = SegmentKind::Line;
    segment.start = gp_Pnt2d(pts[i][0], pts[i][1]);
    segment.end = gp_Pnt2d(pts[(i + 1) % 8][0], pts[(i + 1) % 8][1]);
    segment.name = curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
    key += (i == 0 ? "" : ",") + segment.name;
    loop.segments.push_back(segment);
  }
  region.loops.push_back(loop);
  region.name = key + "}";
  return region;
}

void test_taper_closing_the_profile_partway() {
  const double angle = 45.0 * kPi / 180.0;
  ExtrudeFeatureSpec spec = spec_of({c_profile()}, 3);
  spec.side1.taper = -angle;
  std::string message;
  try {
    extrude_feature(spec);
  } catch (const std::exception& error) {
    message = error.what();
  }
  std::printf("test_taper_closing_the_profile_partway: %s\n", message.c_str());
  // As before, the start the .f3d import knows a kernel failure by.
  CHECK(message.rfind("the taper failed", 0) == 0);
  CHECK(message.find("leaning by 45 degrees") != std::string::npos);
  // The bottom strip's walls (c1 at y = 0, c7 at y = 2) and the top
  // strip's (c3 at y = 44, c5 at y = 42), 2 apart, meet 1 mm up.
  CHECK(message.find("F2:side(c1[c8,c2]) and F2:side(c7[c6,c8])") != std::string::npos);
  CHECK(message.find("F2:side(c5[c4,c6])") != std::string::npos);
  CHECK(message.find("2 mm apart in the profile, meet 1 mm from the sketch plane") != std::string::npos);
  CHECK(message.find("before the extrusion's end 3 mm from it: the taper closes the profile there") !=
        std::string::npos);
  // Nearby profiles that do not close build: 15 degrees over 3 mm (the
  // walls move 0.8 mm), and 45 degrees over 0.9 mm.
  spec.side1.taper = -15.0 * kPi / 180.0;
  const ShapePtr gentle = extrude_feature(spec);
  CHECK(BRepCheck_Analyzer(gentle->occt()).IsValid());
  CHECK(volume(*gentle) > 0.0);
  spec.side1.taper = -angle;
  spec.side1.distance = 0.9;
  const ShapePtr short_one = extrude_feature(spec);
  CHECK(BRepCheck_Analyzer(short_one->occt()).IsValid());
  CHECK(volume(*short_one) > 0.0);
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

  // A taper that shrinks a circle to nothing before the end of the
  // extrusion ends in the apex of the cone: a circle of radius 5 leaning in
  // by 20 degrees over 20 mm (OCCT's draft would intersect the cone past
  // its apex, endlessly at times). A hole of radius 5 that a widening taper
  // closes ends in its cone's apex (mitcad#235): the square's frustum less
  // the cone.
  const double twenty = 20.0 * kPi / 180.0;
  spec = spec_of({circle(1, 0, 0, 5)}, 20);
  spec.side1.taper = -twenty;
  const ShapePtr cone = extrude_feature(spec);
  const double apex = 5.0 / std::tan(twenty);
  CHECK(near(volume(*cone), kPi * 25 * apex / 3.0, 1e-6));
  CHECK(cone->face_count() == 2 && faces_named(*cone, "F2:side(c1)") == 1);
  CHECK(faces_named(*cone, "F2:start(r{c1})") == 1 && near(bounding_box(*cone).max.Z(), apex, 1e-6));
  spec.side1.distance = 10;
  CHECK(near(volume(*extrude_feature(spec)), kPi * 10 / 3 * (25 + 5 * (5 - 10 * std::tan(twenty)) +
                                                              std::pow(5 - 10 * std::tan(twenty), 2))));
  Region ring = rectangle(1, -20, -20, 40, 40);
  ring.loops.push_back(circle(5, 0, 0, 5).loops[0]);
  spec = spec_of({ring}, 20);
  spec.side1.taper = twenty;
  const ShapePtr closed_hole = extrude_feature(spec);
  const double grown = 40.0 + 40.0 * std::tan(twenty);
  CHECK(near(volume(*closed_hole), frustum(40, grown, 20) - kPi * 25 * apex / 3.0, 1e-6));
  CHECK(BRepCheck_Analyzer(closed_hole->occt()).IsValid());
  CHECK(faces_named(*closed_hole, "F2:side(c5)") == 1 && edge_names_unique(*closed_hole));
}

void test_taper_closing_ring() {
  // A ring whose walls a taper leans into each other (mitcad#235): radii
  // 1.5 and 2.5, leaning in by 50 degrees over 2 mm. The outline narrows and
  // the hole widens until they meet at (2.5 - 1.5) / (2 tan 50) from the
  // start, in a ridge at the middle radius: the ring of a triangle's
  // section, a cone less a cone.
  const double fifty = 50.0 * kPi / 180.0;
  const double t = std::tan(fifty);
  Region ring = circle(1, 0, 0, 2.5);
  ring.loops.push_back(circle(2, 0, 0, 1.5).loops[0]);
  ExtrudeFeatureSpec spec = spec_of({ring}, 2);
  spec.side1.taper = -fifty;
  const ShapePtr ridge = extrude_feature(spec);
  CHECK(BRepCheck_Analyzer(ridge->occt()).IsValid());
  const double meet = 1.0 / (2.0 * t);
  // Pappus: the triangle's area times the path of its centroid.
  const double triangle = 0.5 * 1.0 * meet;
  CHECK(near(volume(*ridge), 2.0 * kPi * 2.0 * triangle, 1e-6));
  CHECK(near(bounding_box(*ridge).max.Z(), meet, 1e-6));
  CHECK(faces_named(*ridge, "F2:side(c1)") == 1 && faces_named(*ridge, "F2:side(c2)") == 1);
  CHECK(faces_named(*ridge, start_cap("F2", 1)) <= 1 && edge_names_unique(*ridge));
  // Not closing within the length: the draft, as before.
  spec.side1.distance = 0.2;
  const ShapePtr open = extrude_feature(spec);
  const double r1 = 2.5 - 0.2 * t;
  const double r2 = 1.5 + 0.2 * t;
  const double frustum_volume = [](double a, double b, double h) {
    return kPi * h / 3.0 * (a * a + a * b + b * b);
  }(2.5, r1, 0.2) - [](double a, double b, double h) {
    return kPi * h / 3.0 * (a * a + a * b + b * b);
  }(1.5, r2, 0.2);
  CHECK(near(volume(*open), frustum_volume, 1e-6));
}

// A regular hexagon of circumradius r about (0, 0), lines c1..c6.
Region hexagon(double r) {
  Region region;
  region.name = "r{c1[c6,c2],c2[c1,c3],c3[c2,c4],c4[c3,c5],c5[c4,c6],c6[c5,c1]}";
  region.loops.emplace_back();
  for (int i = 0; i < 6; ++i) {
    const double a = kPi / 3.0 * i;
    const double b = kPi / 3.0 * (i + 1);
    Segment line;
    line.name = "c" + std::to_string(i + 1);
    line.kind = SegmentKind::Line;
    line.start = gp_Pnt2d(r * std::cos(a), r * std::sin(a));
    line.end = gp_Pnt2d(r * std::cos(b), r * std::sin(b));
    region.loops.front().segments.push_back(line);
  }
  return region;
}

void test_taper_closing_profile() {
  // A taper that leans the plane sides in until they meet ends the solid
  // there: a hexagon with sides leaning in by 85 degrees closes into a
  // pyramid whose apex is at (apothem / tan 85) from the start, well within
  // the 10 mm asked for. OCCT's draft cannot build it.
  const double eighty_five = 85.0 * kPi / 180.0;
  const double apothem = 1.25;
  const double r = apothem / std::cos(kPi / 6.0);
  const double depth = apothem / std::tan(eighty_five);
  ExtrudeFeatureSpec spec = spec_of({hexagon(r)}, 10);
  spec.side1.taper = -eighty_five;
  const ShapePtr pyramid = extrude_feature(spec);
  const double hexagon_area = 3.0 * std::sqrt(3.0) / 2.0 * r * r;
  CHECK(near(volume(*pyramid), hexagon_area * depth / 3.0, 1e-6));
  CHECK(pyramid->face_count() == 7);
  for (int i = 0; i < 6; ++i) {
    CHECK(faces_named(*pyramid, "F2:side(c" + std::to_string(i + 1) + ")") == 1);
  }
  CHECK(faces_named(*pyramid, "F2:start(" + hexagon(r).name + ")") == 1);
  CHECK(faces_named(*pyramid, "F2:end(" + hexagon(r).name + ")") == 0);
  CHECK(edge_names_unique(*pyramid));
  const BoundingBox box = bounding_box(*pyramid);
  CHECK(near(box.max.Z(), depth, 1e-6) && near(box.min.Z(), 0.0));

  // A rectangle closes into a roof with a ridge: 40 x 20, leaning in by 45
  // degrees, closes after 10 mm into a ridge of 20 mm.
  spec = spec_of({rectangle(1, 0, 0, 40, 20)}, 15);
  spec.side1.taper = -kPi / 4.0;
  const ShapePtr roof = extrude_feature(spec);
  CHECK(near(volume(*roof), 8000.0 - 6000.0 + 4000.0 / 3.0, 1e-6));
  CHECK(roof->face_count() == 5 && edge_names_unique(*roof));
  CHECK(near(bounding_box(*roof).max.Z(), 10.0, 1e-6));

  // Not closing within the length: the ordinary draft, with its far cap.
  spec.side1.distance = 8;
  const ShapePtr frustum_solid = extrude_feature(spec);
  CHECK(frustum_solid->face_count() == 6 && face_at_z(*frustum_solid, end_cap("F2", 1), 8));
}

void test_thin() {
  // T0 ext_thin_closed: a 2 mm wall centred on a 40 x 20 outline, 10 high.
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 40, 20)}, 10);
  spec.side1.thin = ThinWall{WallLocation::Center, 2, 0};
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
  // Both sides: 2 outside, 1 inside (mitcad#463).
  spec.side1.thin = ThinWall{WallLocation::Both, 2, 1};
  CHECK(near(volume(*extrude_feature(spec)), (44 * 24 - 38 * 18) * 10));

  // A disc with a hole: a ring on each loop; side 1 goes into the hole.
  Region ring = circle(1, 0, 0, 20);
  ring.loops.push_back(circle(2, 0, 0, 10).loops[0]);
  spec = spec_of({ring}, 10);
  spec.side1.thin = ThinWall{WallLocation::Side1, 2, 0};
  CHECK(near(volume(*extrude_feature(spec)), kPi * ((22 * 22 - 20 * 20) + (10 * 10 - 8 * 8)) * 10));

  // Different walls on the two sides meet in a step at the sketch plane.
  spec = spec_of({rectangle(1, 0, 0, 40, 20)}, 10);
  spec.side1.thin = ThinWall{WallLocation::Center, 2, 0};
  spec.side2 = distance(5);
  spec.side2->thin = ThinWall{WallLocation::Side2, 4, 0};
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

void test_to_body_from_inside() {
  // A stepped body: the block z = 0..20 with a step x = 30..60 up to z = 30.
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const ShapePtr step = extrude("F3", {rectangle(5, 30, 0, 30, 40)}, 20, 30);
  const ShapePtr body = boolean(BooleanOp::Join, {block.get()}, *step).pieces.at(0).shape;
  Target first;
  first.kind = Target::Kind::Body;
  first.body = body;

  // A profile inside the body ends at the first face beyond it: a circle
  // at z = 10 up to the top at z = 20.
  ExtrudeFeatureSpec inside;
  inside.feature = "F4";
  inside.frame.origin = gp_Pnt(0, 0, 10);
  inside.regions = {circle(9, 15, 20, 5)};
  inside.side1.target = first;
  const ShapePtr post = extrude_feature(inside);
  CHECK(near(volume(*post), kPi * 25 * 10));
  CHECK(face_at_z(*post, "F4:end(r{c9})", 20) && face_at_z(*post, "F4:start(r{c9})", 10));

  // Partly inside: a 20 x 20 square at z = 25 across the step's side, down.
  // Outside the step it ends on the lower top (z = 20), inside it at the
  // bottom (z = 0).
  ExtrudeFeatureSpec across;
  across.feature = "F6";
  across.frame.origin = gp_Pnt(0, 0, 25);
  across.regions = {rectangle(10, 20, 10, 20, 20)};
  across.direction = gp_Dir(0, 0, -1);
  across.side1.target = first;
  const ShapePtr partly = extrude_feature(across);
  CHECK(near(volume(*partly), 10 * 20 * 5 + 10 * 20 * 25));
  CHECK(near(bounding_box(*partly).min.Z(), 0) && near(bounding_box(*partly).max.Z(), 25));

  // Through the body from there: each part runs to the body's far side, the
  // part outside the step from above it and the part inside the step from
  // within it, both to z = 0.
  ExtrudeFeatureSpec through_spec = across;
  Target through_body = first;
  through_body.through = true;
  through_spec.side1.target = through_body;
  const ShapePtr through_all_parts = extrude_feature(through_spec);
  CHECK(near(volume(*through_all_parts), 20 * 20 * 25));
  CHECK(near(bounding_box(*through_all_parts).min.Z(), 0));

  // On a face: the same square on the step's top (z = 30), down: 10 mm to
  // the lower top beside the step, 30 mm through the step.
  across.frame.origin = gp_Pnt(0, 0, 30);
  CHECK(near(volume(*extrude_feature(across)), 10 * 20 * 10 + 10 * 20 * 30));

  // Up to a curved face that crosses the profile: a 4 x 4 bar along -x from
  // x = 9.9 to a cylinder of radius 10 on the z axis. Its middle (|y| < a)
  // starts inside and runs to the far side, the rest to the near side.
  const ShapePtr pillar = extrude("F7", {circle(9, 0, 0, 10)}, 0, 20);
  ExtrudeFeatureSpec bar;
  bar.feature = "F8";
  bar.frame.origin = gp_Pnt(9.9, 0, 0);
  bar.frame.x_axis = gp_Dir(0, 1, 0);
  bar.frame.y_axis = gp_Dir(0, 0, 1);
  bar.regions = {rectangle(1, -2, 8, 4, 4)};
  bar.direction = gp_Dir(-1, 0, 0);
  Target wall;
  wall.kind = Target::Kind::Face;
  wall.body = pillar;
  wall.face = "F7:side(c9)";
  bar.side1.target = wall;
  // The integral of sqrt(100 - y^2) from 0 to y.
  const auto arc = [](double y) { return y / 2 * std::sqrt(100 - y * y) + 50 * std::asin(y / 10); };
  const double a = std::sqrt(100 - 9.9 * 9.9);
  CHECK(near(volume(*extrude_feature(bar)), 4 * (9.9 * 4 + 4 * arc(a) - 2 * arc(2)), 1e-6));

  // A plane that crosses the profile still leaves no end.
  Target slanted;
  slanted.plane = gp_Pln(gp_Pnt(30, 0, 25), gp_Dir(1, 0, 1));
  across.side1.target = slanted;
  CHECK(throws_with([&] { extrude_feature(across); }, "crosses the profile"));
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

// A circle whose one arc starts and ends where it touches another
// profile's circle (a region `c2[c1,c1]`): its parameters run past a full
// turn from there.
Region touching_circle(int curve, double cx, double cy, double radius, double at) {
  Segment arc;
  arc.name = "c" + std::to_string(curve) + "[c1,c1]";
  arc.kind = SegmentKind::Arc;
  arc.center = gp_Pnt2d(cx, cy);
  arc.radius = radius;
  arc.start_angle = at;
  arc.end_angle = at;
  Region region;
  region.name = "r{" + arc.name + "}";
  region.loops.push_back(Loop{{arc}});
  return region;
}

void test_circles_touching_at_a_point() {
  // A circle of 5 mm and one of 3 mm touching it from outside at (5, 0),
  // whose arc starts there (angle pi, and the same past 2 pi).
  for (const double at : {kPi, 3.0 * kPi}) {
    ExtrudeFeatureSpec spec = spec_of({circle(1, 0, 0, 5), touching_circle(2, 8, 0, 3, at)}, 10);
    const ShapePtr shape = extrude_feature(spec);
    CHECK(BRepCheck_Analyzer(shape->occt()).IsValid());
    CHECK(near(volume(*shape), kPi * (25.0 + 9.0) * 10.0, 1e-6));
    spec.side1.taper = 0.05;
    const ShapePtr tapered = extrude_feature(spec);
    CHECK(BRepCheck_Analyzer(tapered->occt()).IsValid());
  }
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

// Thin walls along chains of sketch curves (mitcad#463).

PathCurve chain_line(const std::string& name, double x0, double y0, double x1, double y1) {
  PathCurve piece;
  piece.name = name;
  piece.curve.kind = ModelCurveKind::Line;
  piece.curve.start = gp_Pnt(x0, y0, 0);
  piece.curve.end = gp_Pnt(x1, y1, 0);
  return piece;
}

// An arc of `radius` about the origin from angle `first` to `last`,
// counter-clockwise about +z.
PathCurve chain_arc(const std::string& name, double radius, double first, double last) {
  PathCurve piece;
  piece.name = name;
  piece.curve.kind = ModelCurveKind::Conic;
  piece.curve.center = gp_Pnt(0, 0, 0);
  piece.curve.normal = gp_Dir(0, 0, 1);
  piece.curve.x_axis = gp_Dir(1, 0, 0);
  piece.curve.major = piece.curve.minor = radius;
  piece.curve.first = first;
  piece.curve.last = last;
  return piece;
}

ExtrudeFeatureSpec along(std::vector<Path> chains, WallLocation location, double thickness) {
  ExtrudeFeatureSpec spec = spec_of({}, 10);
  for (std::size_t i = 0; i < chains.size(); ++i) {
    std::string key = "r{";
    for (const PathCurve& piece : chains[i]) {
      key += (key.size() > 2 ? "," : "") + piece.name;
    }
    spec.chain_keys.push_back(key + "}");
  }
  spec.chains = std::move(chains);
  spec.side1.thin = ThinWall{location, thickness, 0};
  return spec;
}

void test_thin_open_chains() {
  // An L bending left: (0,0) to (40,0) to (40,30). Side 1 is to its right,
  // round the outside of the corner, which the walls meet in a mitre.
  const Path ell{chain_line("c1", 0, 0, 40, 0), chain_line("c2", 40, 0, 40, 30)};
  ExtrudeFeatureSpec spec = along({ell}, WallLocation::Side1, 2);
  const ShapePtr outside = extrude_feature(spec);
  CHECK(near(volume(*outside), (2 * 40 + 2 * 30 + 2 * 2) * 10));
  for (const char* face : {"F2:outer(c1)", "F2:outer(c2)", "F2:inner(c1)", "F2:inner(c2)",
                           "F2:tip0(c1)", "F2:tip1(c2)", "F2:start(r{c1,c2})", "F2:end(r{c1,c2})"}) {
    CHECK(faces_named(*outside, face) == 1);
  }
  CHECK(outside->face_count() == 8 && edge_names_unique(*outside));
  CHECK(BRepCheck_Analyzer(outside->occt()).IsValid());
  // Side 2, inside the corner; centred, half each way.
  spec.side1.thin->location = WallLocation::Side2;
  CHECK(near(volume(*extrude_feature(spec)), (2 * 40 + 2 * 30 - 2 * 2) * 10));
  spec.side1.thin->location = WallLocation::Center;
  const ShapePtr centred = extrude_feature(spec);
  CHECK(near(volume(*centred), (2 * 40 + 2 * 30) * 10));
  const BoundingBox box = bounding_box(*centred);
  CHECK(near(box.min.X(), 0) && near(box.max.X(), 41) && near(box.min.Y(), -1) && near(box.max.Y(), 30));
  // Both sides by their own thicknesses: 2 to the right, 1 to the left.
  spec.side1.thin = ThinWall{WallLocation::Both, 2, 1};
  CHECK(near(volume(*extrude_feature(spec)), (2 * 40 + 2 * 30 + 2 * 2 + 1 * 40 + 1 * 30 - 1 * 1) * 10));
  spec.side1.thin->thickness2 = 0.0;
  CHECK(throws_with([&] { extrude_feature(spec); }, "thickness on side 2"));
  spec.side1.thin = ThinWall{WallLocation::Center, 2, 0};
  // A taper widens the wall away from the start.
  spec.side1.taper = 0.05;
  const ShapePtr tapered = extrude_feature(spec);
  CHECK(volume(*tapered) > (2 * 40 + 2 * 30) * 10 && BRepCheck_Analyzer(tapered->occt()).IsValid());
  spec.side1.taper = 0.0;

  // A straight line: side 2 to its left.
  spec = along({{chain_line("c1", 0, 0, 10, 0)}}, WallLocation::Side2, 1);
  const BoundingBox left = bounding_box(*extrude_feature(spec));
  CHECK(near(volume(*extrude_feature(spec)), 10 * 1 * 10));
  CHECK(near(left.min.Y(), 0) && near(left.max.Y(), 1));

  // A half circle of radius 10, centred: a half ring 9 to 11.
  spec = along({{chain_arc("c1", 10, 0, kPi)}}, WallLocation::Center, 2);
  CHECK(near(volume(*extrude_feature(spec)), kPi / 2 * (11 * 11 - 9 * 9) * 10));
  // Thicker inside than its radius: no wall.
  spec.side1.thin = ThinWall{WallLocation::Side2, 12, 0};
  CHECK(throws_with([&] { extrude_feature(spec); }, "thin wall"));

  // A closed chain: side 1 outside the area it encloses.
  const Path square{chain_line("c1", 0, 0, 40, 0), chain_line("c2", 40, 0, 40, 20),
                    chain_line("c3", 40, 20, 0, 20), chain_line("c4", 0, 20, 0, 0)};
  spec = along({square}, WallLocation::Side1, 2);
  const ShapePtr frame = extrude_feature(spec);
  CHECK(near(volume(*frame), (44 * 24 - 40 * 20) * 10));
  CHECK(faces_named(*frame, "F2:outer(c3)") == 1 && faces_named(*frame, "F2:start(r{c1,c2,c3,c4})") == 1);

  // Chains that meet in a T make one solid; two sides with their own walls.
  spec = along({{chain_line("c1", 0, 0, 40, 0)}, {chain_line("c2", 20, 0, 20, 20)}}, WallLocation::Center, 2);
  const ShapePtr tee = extrude_feature(spec);
  CHECK(solids(*tee).size() == 1);
  CHECK(near(volume(*tee), (40 * 2 + 2 * 19) * 10));
  spec.side2 = distance(5);
  spec.side2->thin = ThinWall{WallLocation::Center, 4, 0};
  CHECK(near(volume(*extrude_feature(spec)), (40 * 2 + 2 * 19) * 10 + (40 * 4 + 4 * 18) * 5));

  // Curves only with walls, each chain with its key.
  spec = along({ell}, WallLocation::Center, 2);
  spec.side1.thin.reset();
  CHECK(throws_with([&] { extrude_feature(spec); }, "only a thin extrusion"));
  spec = along({ell}, WallLocation::Center, 2);
  spec.chain_keys.clear();
  CHECK(throws_with([&] { extrude_feature(spec); }, "its key"));
}

} // namespace

// Along a direction off the sketch normal (mitcad#539): a 20 x 10 rectangle
// 10 along (1, 0, 2), its far cap at the height h = 10 cos (20 / sqrt 5)
// moved 10 / sqrt 5 along x. Without a taper a slanted prism of the
// profile's area times h. With a taper of 5 degrees the profile grows in
// its plane by t tan 5 at t along the direction (each side face ruled from
// its edge to the edge's offset moved along the direction): a prismatoid
// of rectangles grown evenly with the height, whose faces stay planes. A
// circle grows into an oblique cone whose sections are circles: the volume
// of the upright frustum of the same radii and height.
void test_oblique() {
  const double root5 = std::sqrt(5.0);
  const double h = 20 / root5;
  const double five = 5.0 * kPi / 180.0;
  const double grow = 10 * std::tan(five);
  ExtrudeFeatureSpec spec = spec_of({rectangle(1, 0, 0, 20, 10)}, 10);
  spec.direction = gp_Dir(1, 0, 2);
  const ShapePtr slanted = extrude_feature(spec);
  CHECK_NEAR(volume(*slanted), 200 * h);
  CHECK(face_at_z(*slanted, end_cap("F2", 1), h));

  spec.side1.taper = five;
  const ShapePtr tapered = extrude_feature(spec);
  const auto area = [](double g) { return (20 + 2 * g) * (10 + 2 * g); };
  CHECK_NEAR(volume(*tapered), h / 6 * (area(0) + 4 * area(grow / 2) + area(grow)));
  CHECK(face_at_z(*tapered, end_cap("F2", 1), h));
  const std::vector<int> far = tapered->find_faces(end_cap("F2", 1));
  CHECK(far.size() == 1);
  if (far.size() == 1) {
    const BoundingBox cap = bounding_box(Shape(tapered->face(far[0])));
    CHECK(near(cap.min.X(), 10 / root5 - grow) && near(cap.max.X(), 20 + 10 / root5 + grow));
    CHECK(near(cap.min.Y(), -grow) && near(cap.max.Y(), 10 + grow));
  }
  CHECK(faces_named(*tapered, side("F2", 1, 0)) == 1 && tapered->face_count() == 6);
  CHECK(edge_names_unique(*tapered));
  for (int i = 0; i < tapered->face_count(); ++i) {
    CHECK(BRepAdaptor_Surface(tapered->face(i)).GetType() == GeomAbs_Plane);
  }

  spec.regions = {circle(1, 0, 0, 5)};
  const double r1 = 5 + grow;
  const ShapePtr cone = extrude_feature(spec);
  CHECK_NEAR(volume(*cone), kPi * h / 3 * (25 + 5 * r1 + r1 * r1));
  CHECK(faces_named(*cone, "F2:side(c1)") == 1 && faces_named(*cone, "F2:end(r{c1})") == 1);
  CHECK(BRepCheck_Analyzer(cone->occt()).IsValid());

  // Not up to an object.
  Target plane;
  plane.plane = gp_Pln(gp_Pnt(0, 0, 8), gp_Dir(0, 0, 1));
  spec.side1.target = plane;
  CHECK(throws_with([&] { extrude_feature(spec); }, "unsupported: a taper up to an object"));
}

void extrude_feature_tests() {
  guarded("test_two_sides_and_symmetric", test_two_sides_and_symmetric);
  // Along a direction off the sketch normal (mitcad#539).
  guarded("test_oblique", test_oblique);
  guarded("test_start", test_start);
  guarded("test_taper", test_taper);
  guarded("test_taper_closing_the_profile_partway", test_taper_closing_the_profile_partway);
  guarded("test_taper_closing_profile", test_taper_closing_profile);
  guarded("test_taper_closing_ring", test_taper_closing_ring);
  guarded("test_thin", test_thin);
  guarded("test_to_planes", test_to_planes);
  guarded("test_to_faces_and_bodies", test_to_faces_and_bodies);
  guarded("test_to_body_from_inside", test_to_body_from_inside);
  guarded("test_clockwise_loops", test_clockwise_loops);
  guarded("test_circles_touching_at_a_point", test_circles_touching_at_a_point);
  guarded("test_several_regions", test_several_regions);
  guarded("test_thin_open_chains", test_thin_open_chains);
  // A negative side of two (mitcad#765).
  guarded("test_negative_side", test_negative_side);
}

} // namespace test
