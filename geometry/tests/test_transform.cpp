// SPDX-License-Identifier: MIT
// Moved, mirrored and scaled copies and the geometry of their faces,
// unions, the tools of faces and primitives.

#include <cmath>

#include <BRepGProp.hxx>
#include <BRep_Builder.hxx>
#include <GProp_GProps.hxx>
#include <Poly_Triangulation.hxx>
#include <TopoDS_Face.hxx>

#include "check.hpp"
#include "mitcad/geometry/import.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// Sketch1's 60 x 40 rectangle (c1..c4) extruded 20 mm by F2.
ShapePtr block() { return extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20); }

Affine translation(double x, double y, double z) {
  Affine map;
  map.translation = {x, y, z};
  return map;
}

Affine rotation_z(double angle) {
  Affine map;
  map.linear = {{{std::cos(angle), -std::sin(angle), 0.0},
                 {std::sin(angle), std::cos(angle), 0.0},
                 {0.0, 0.0, 1.0}}};
  return map;
}

bool box_is(const Shape& shape, double x0, double y0, double z0, double x1, double y1, double z1) {
  const BoundingBox box = bounding_box(shape);
  const bool ok = !box.empty && near(box.min.X(), x0, 1e-7) && near(box.min.Y(), y0, 1e-7) &&
                  near(box.min.Z(), z0, 1e-7) && near(box.max.X(), x1, 1e-7) &&
                  near(box.max.Y(), y1, 1e-7) && near(box.max.Z(), z1, 1e-7);
  if (!ok) {
    std::fprintf(stderr, "box %g %g %g .. %g %g %g\n", box.min.X(), box.min.Y(), box.min.Z(),
                 box.max.X(), box.max.Y(), box.max.Z());
  }
  return ok;
}

BooleanResult run(BooleanOp op, const ShapePtr& target, const ShapePtr& tool) {
  return boolean(op, {target.get()}, *tool);
}

void test_moves_keep_or_rename_faces() {
  const ShapePtr original = block();
  const ShapePtr moved = transform_shape(*original, translation(10, 5, 0));
  CHECK(near(volume(*moved), 48000.0));
  CHECK(box_is(*moved, 10, 5, 0, 70, 45, 20));
  CHECK(faces_named(*moved, end_cap("F2", 1)) == 1);
  CHECK(moved->find_edges(edge(side("F2", 1, 0), side("F2", 1, 1))).size() == 1);
  // The original is untouched.
  CHECK(box_is(*original, 0, 0, 0, 60, 40, 20));

  const ShapePtr turned = transform_shape(*original, rotation_z(kPi / 4), "F9:inst1");
  CHECK(near(volume(*turned), 48000.0));
  const double s = std::sqrt(0.5);
  CHECK(box_is(*turned, -40 * s, 0, 0, 60 * s, 100 * s, 20));
  CHECK(faces_named(*turned, "F9:inst1(" + end_cap("F2", 1) + ")") == 1);
  CHECK(faces_named(*turned, end_cap("F2", 1)) == 0);
  CHECK(edge_names_unique(*turned));
}

void test_mirrors_and_scales() {
  Affine mirror;
  mirror.linear = {{{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  const ShapePtr mirrored = transform_shape(*block(), mirror, "F9:inst1");
  CHECK(near(volume(*mirrored), 48000.0));
  CHECK(box_is(*mirrored, -60, 0, 0, 0, 40, 20));
  CHECK(mirrored->face_count() == 6);
  // The mirrored top still faces up, the mirrored side at x = -60 faces -X
  // (the planes' positions are left-handed after a mirror).
  const SurfaceDescription top =
      face_geometry(*mirrored, "F9:inst1(" + end_cap("F2", 1) + ")");
  CHECK(top.type == "plane");
  CHECK(top.axis.IsEqual(gp_Dir(0, 0, 1), 1e-9));
  const SurfaceDescription far = face_geometry(*mirrored, "F9:inst1(" + side("F2", 1, 1) + ")");
  CHECK(far.axis.IsEqual(gp_Dir(-1, 0, 0), 1e-9));
  CHECK(far.origin.Distance(gp_Pnt(-60, 0, 0)) < 1e-9);

  const ShapePtr cube = extrude("F2", {rectangle(1, 0, 0, 10, 10)}, 0, 10);
  Affine twice;
  twice.linear = {{{2, 0, 0}, {0, 2, 0}, {0, 0, 2}}};
  const ShapePtr doubled = transform_shape(*cube, twice);
  CHECK(near(volume(*doubled), 8000.0));
  CHECK(face_infos(*doubled).front().surface == "plane");

  Affine uneven;
  uneven.linear = {{{2, 0, 0}, {0, 1, 0}, {0, 0, 0.5}}};
  const ShapePtr flattened = transform_shape(*cube, uneven);
  CHECK(near(volume(*flattened), 1000.0, 1e-6));
  CHECK(box_is(*flattened, 0, 0, 0, 20, 10, 5));
  CHECK(faces_named(*flattened, end_cap("F2", 1)) == 1);
  // A non-uniform scale turns the faces into B-splines (planes included).
  CHECK(face_infos(*flattened).front().surface == "bspline");
  const ShapePtr peg = extrude("F2", {circle(1, 0, 0, 5)}, 0, 10);
  const ShapePtr oval = transform_shape(*peg, uneven);
  // The geometry is exact. OCCT's default volume integration of curved
  // B-spline faces is not (0.9 % here); the volumes Mitcad reports are.
  GProp_GProps props;
  BRepGProp::VolumeProperties(oval->occt(), props);
  CHECK(!near(props.Mass(), kPi * 25 * 10 * 2 * 0.5, 1e-3));
  CHECK_NEAR(volume(*oval), kPi * 25 * 10 * 2 * 0.5);
  // The body's area is the sum of its faces' areas.
  double faces = 0.0;
  for (const FaceInfo& face : face_infos(*oval)) {
    faces += face.area;
  }
  CHECK_NEAR_TOL(mass_properties(*oval).area, faces, 1e-9);
  CHECK(faces_named(*oval, mitcad::geometry::face_name("F2", "side", "c1")) == 1);

  Affine flat;
  flat.linear = {{{1, 0, 0}, {0, 1, 0}, {0, 0, 0}}};
  CHECK(throws_with([&] { transform_shape(*cube, flat); }, "flattens"));
}

// A mesh body as STL import makes it: one face without a surface carrying
// the 12 triangles of the box 0..60 x 0..40 x 0..20 (counter-clockwise seen
// from outside), named F7:import(0).
ShapePtr mesh_block() {
  occ::handle<Poly_Triangulation> mesh = new Poly_Triangulation(8, 12, false);
  // Node 1 + (x ? 1 : 0) + (y ? 2 : 0) + (z ? 4 : 0).
  for (int i = 0; i < 8; ++i) {
    mesh->SetNode(i + 1, gp_Pnt((i & 1) ? 60 : 0, (i & 2) ? 40 : 0, (i & 4) ? 20 : 0));
  }
  const int triangles[12][3] = {{1, 3, 4}, {1, 4, 2}, {5, 6, 8}, {5, 8, 7}, {1, 2, 6}, {1, 6, 5},
                                {3, 7, 8}, {3, 8, 4}, {1, 5, 7}, {1, 7, 3}, {2, 4, 8}, {2, 8, 6}};
  for (int t = 0; t < 12; ++t) {
    mesh->SetTriangle(t + 1, Poly_Triangle(triangles[t][0], triangles[t][1], triangles[t][2]));
  }
  TopoDS_Face face;
  BRep_Builder().MakeFace(face, mesh);
  return import_body(face, "F7", 0);
}

// A mesh body in a moved, turned, mirrored or scaled occurrence (and in a
// pattern or a Move): its triangles move with it, and OCCT's checker, which
// rejects a face without a surface (BRepCheck_NoSurface), is not asked
// about them (mitcad#20).
void test_mesh_bodies_move() {
  const std::string name = mitcad::geometry::face_name("F7", "import", "0");
  const ShapePtr original = mesh_block();
  CHECK(body_kind(original->occt()) == BodyKind::Mesh);
  CHECK_NEAR(volume(*original), 48000.0);

  const ShapePtr moved = transform_shape(*original, translation(10, 5, 0));
  CHECK(body_kind(moved->occt()) == BodyKind::Mesh);
  CHECK_NEAR(volume(*moved), 48000.0);
  CHECK(box_is(*moved, 10, 5, 0, 70, 45, 20));
  CHECK(faces_named(*moved, name) == 1);
  CHECK(box_is(*original, 0, 0, 0, 60, 40, 20));

  Affine placed = rotation_z(kPi / 2);
  placed.translation = {100, 0, 0};
  const ShapePtr turned = transform_shape(*original, placed, "F9:inst1");
  CHECK_NEAR(volume(*turned), 48000.0);
  CHECK(box_is(*turned, 60, 0, 0, 100, 60, 20));
  CHECK(faces_named(*turned, "F9:inst1(" + name + ")") == 1);
  const MassProperties mass = mass_properties(*turned);
  CHECK(mass.center.Distance(gp_Pnt(80, 30, 10)) < 1e-9);

  // A mirror keeps the triangles facing out: the volume stays positive.
  Affine mirror;
  mirror.linear = {{{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  const ShapePtr mirrored = transform_shape(*original, mirror);
  CHECK_NEAR(volume(*mirrored), 48000.0);
  CHECK(box_is(*mirrored, -60, 0, 0, 0, 40, 20));

  Affine uneven;
  uneven.linear = {{{2, 0, 0}, {0, 1, 0}, {0, 0, 0.5}}};
  const ShapePtr scaled = transform_shape(*original, uneven);
  CHECK(body_kind(scaled->occt()) == BodyKind::Mesh);
  CHECK_NEAR(volume(*scaled), 48000.0);
  CHECK(box_is(*scaled, 0, 0, 0, 120, 40, 10));
  CHECK(faces_named(*scaled, name) == 1);

  // Next to a solid in a compound: both move, and the solid is checked.
  const ShapePtr both = compound({original.get(), block().get()});
  const ShapePtr far = transform_shape(*both, translation(0, 0, 50));
  CHECK(box_is(*far, 0, 0, 50, 60, 40, 70));
  CHECK_NEAR(volume(*far), 2 * 48000.0);
}

void test_unions_keep_names() {
  // The reference model comb_join: A (x 0..40) and B (x 20..60), 20 mm cubes overlapping.
  const ShapePtr a = extrude("F2", {rectangle(1, 0, 0, 40, 20)}, 0, 20);
  const ShapePtr b = extrude("F4", {rectangle(5, 20, 0, 40, 20)}, 0, 20);
  const ShapePtr joined = unite({a.get(), b.get()});
  CHECK(near(volume(*joined), 60.0 * 20 * 20));
  CHECK(joined->face_count() == 6);
  CHECK(joined->find_faces(end_cap("F2", 1)) == joined->find_faces(end_cap("F4", 5)));
  CHECK(faces_named(*joined, side("F4", 5, 1)) == 1);
  CHECK(edge_names_unique(*joined));

  const ShapePtr far = extrude("F4", {rectangle(5, 100, 0, 10, 10)}, 0, 10);
  const ShapePtr both = unite({a.get(), far.get()});
  CHECK(solids(*both).size() == 2);
  CHECK(near(volume(*both), 40.0 * 20 * 20 + 1000.0));
  CHECK(unite({a.get()})->face_count() == 6);
}

void test_tools_of_faces() {
  // A plate with a hole cut through it: the hole's wall bounds empty space.
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 5);
  const ShapePtr drill = extrude("F4", {circle(5, 10, 10, 2.5)}, -1, 6);
  const BooleanResult holed = run(BooleanOp::Cut, plate, drill);
  CHECK(holed.pieces.size() == 1);
  const ShapePtr with_hole = holed.pieces.at(0).shape;
  const std::string wall = mitcad::geometry::face_name("F4", "side", "c5");
  const FaceTool hole = face_tool(*with_hole, {wall});
  CHECK(!hole.material);
  CHECK(near(volume(*hole.tool), kPi * 2.5 * 2.5 * 5, 1e-6));
  CHECK(faces_named(*hole.tool, wall) == 1);
  CHECK(hole.tool->face_count() == 3);
  // Patterned 20 mm along x, it cuts a second hole.
  const ShapePtr copy = transform_shape(*hole.tool, translation(20, 0, 0), "F9:inst1");
  const BooleanResult twice = run(BooleanOp::Cut, with_hole, copy);
  CHECK(near(volume(*twice.pieces.at(0).shape), 60.0 * 40 * 5 - 2 * kPi * 2.5 * 2.5 * 5, 1e-6));
  CHECK(faces_named(*twice.pieces.at(0).shape, "F9:inst1(" + wall + ")") == 1);

  // A boss on the plate: its side and top bound material.
  const ShapePtr peg = extrude("F6", {circle(9, 40, 20, 5)}, 0, 15);
  const BooleanResult bossed = run(BooleanOp::Join, plate, peg);
  const ShapePtr with_boss = bossed.pieces.at(0).shape;
  const std::string boss_side = mitcad::geometry::face_name("F6", "side", "c9");
  const std::string boss_top = mitcad::geometry::face_name("F6", "end", "r{c9}");
  const FaceTool boss = face_tool(*with_boss, {boss_side, boss_top});
  CHECK(boss.material);
  CHECK(near(volume(*boss.tool), kPi * 25 * 10, 1e-6));
  CHECK(faces_named(*boss.tool, boss_top) == 1);

  CHECK(throws_with([&] { face_tool(*with_boss, {"F6:side(c8)"}); }, "no face F6:side(c8)"));
}

void test_primitives_name_their_faces() {
  PrimitiveSpec box;
  box.feature = "F3";
  box.a = 30;
  box.b = 20;
  box.c = 10;
  box.frame.origin = gp_Pnt(5, 5, 0);
  const ShapePtr made = primitive(box);
  CHECK(near(volume(*made), 6000.0));
  CHECK(box_is(*made, 5, 5, 0, 35, 25, 10));
  for (const char* role : {"bottom", "top", "side0", "side1", "side2", "side3"}) {
    CHECK(faces_named(*made, mitcad::geometry::face_name("F3", role)) == 1);
  }
  const SurfaceDescription right = face_geometry(*made, "F3:side1");
  CHECK(right.axis.IsEqual(gp_Dir(1, 0, 0), 1e-9));
  CHECK(std::abs(right.origin.X() - 35) < 1e-9);
  CHECK(edge_names_unique(*made));

  // On a frame whose normal is +Y (the XZ sketch plane).
  PrimitiveSpec cylinder;
  cylinder.feature = "F4";
  cylinder.kind = PrimitiveKind::Cylinder;
  cylinder.a = 10;
  cylinder.c = 30;
  cylinder.frame.y_axis = gp_Dir(0, 0, -1);
  const ShapePtr can = primitive(cylinder);
  CHECK(near(volume(*can), kPi * 100 * 30, 1e-6));
  CHECK(box_is(*can, -10, 0, -10, 10, 30, 10));
  CHECK(face_geometry(*can, "F4:top").origin.Distance(gp_Pnt(0, 30, 0)) < 1e-9);
  CHECK(faces_named(*can, "F4:side0") == 1);

  PrimitiveSpec sphere;
  sphere.feature = "F5";
  sphere.kind = PrimitiveKind::Sphere;
  sphere.a = 15;
  CHECK(near(volume(*primitive(sphere)), 4.0 / 3.0 * kPi * 15 * 15 * 15, 1e-6));

  PrimitiveSpec torus;
  torus.feature = "F6";
  torus.kind = PrimitiveKind::Torus;
  torus.a = 20;
  torus.b = 5;
  const ShapePtr ring = primitive(torus);
  CHECK(near(volume(*ring), 2 * kPi * kPi * 20 * 25, 1e-6));
  CHECK(faces_named(*ring, "F6:side0") == 1);
  torus.b = 25;
  CHECK(throws_with([&] { primitive(torus); }, "too large"));
}

} // namespace

void transform_tests() {
  test_moves_keep_or_rename_faces();
  test_mirrors_and_scales();
  test_mesh_bodies_move();
  test_unions_keep_names();
  test_tools_of_faces();
  test_primitives_name_their_faces();
}

} // namespace test
