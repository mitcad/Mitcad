// SPDX-License-Identifier: MIT
// Moved, mirrored and scaled copies and the geometry of their faces,
// unions, the tools of faces and primitives.

#include <cmath>
#include <string>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>

#include "check.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/contact.hpp"
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

// A sphere of radius r at `centre`, scaled along z by `z` about its centre.
ShapePtr flattened_sphere(const std::string& feature, const gp_Pnt& centre, double r, double z) {
  PrimitiveSpec sphere;
  sphere.feature = feature;
  sphere.kind = PrimitiveKind::Sphere;
  sphere.a = r;
  sphere.frame.origin = centre;
  Affine flat;
  flat.linear = {{{1, 0, 0}, {0, 1, 0}, {0, 0, z}}};
  flat.translation = {0, 0, centre.Z() * (1 - z)};
  return transform_shape(*primitive(sphere), flat);
}

// A sphere scaled along its axis (mitcad#1504): flattened by 0.1 and 0.15
// it came out of the B-spline conversion with an unorientable face; it is
// the exact ellipsoid of revolution, valid, of the sphere's volume times
// the scale, its face named as the sphere's. Elongated, its seam's circle
// would become an ellipse whose major axis moves the parameter, so it is
// still converted to a B-spline (and valid).
void test_flattened_spheres() {
  const gp_Pnt centre(40, -30, -100);
  const double r = 150;
  for (const double z : {0.1, 0.15, 0.2, 2.0}) {
    const ShapePtr flattened = flattened_sphere("F5", centre, r, z);
    CHECK(BRepCheck_Analyzer(flattened->occt()).IsValid());
    CHECK(flattened->face_count() == 1);
    const std::string surface = face_infos(*flattened).front().surface;
    const char* expected = z < 1.0 ? "revolution" : "bspline";
    if (surface != expected) {
      std::fprintf(stderr, "scale %g: the surface is %s, not %s\n", z, surface.c_str(), expected);
    }
    CHECK(surface == expected);
    const MassProperties mass = mass_properties(*flattened);
    CHECK_NEAR(mass.volume, 4.0 / 3.0 * kPi * r * r * r * z);
    CHECK(mass.center.Distance(centre) < 1e-6);
    CHECK(faces_named(*flattened, mitcad::geometry::face_name("F5", "side0")) == 1);
  }
}

// A plate joined with spheres flattened along z by 0.2 whose tops rise
// 3 mm above it and which overlap each other (mitcad#1504: 38 of them ended
// in BRepCheck_UnorientableShape after 2.5 GB): one valid solid. A Combine's
// Join unites the target with all its tools at once, as here (a compound of
// overlapping tools is no boolean tool: its solids interfere).
void test_plate_with_flattened_spheres() {
  const ShapePtr plate = extrude("F1", {rectangle(1, -300, -300, 600, 600)}, 0, 10);
  const double radii[] = {100, 137, 174, 110, 147, 120};
  std::vector<ShapePtr> caps;
  double caps_volume = 0.0;
  for (int i = 0; i < 6; ++i) {
    const double r = radii[i];
    const double z = 0.2;
    const gp_Pnt centre(-150 + 120 * (i % 3), -100 + 140 * (i / 3), 13.0 - z * r);
    caps.push_back(flattened_sphere("F" + std::to_string(2 + i), centre, r, z));
    caps_volume += 4.0 / 3.0 * kPi * r * r * r * z;
  }
  std::vector<const Shape*> all{plate.get()};
  for (const ShapePtr& cap : caps) {
    all.push_back(cap.get());
  }
  const ShapePtr joined = unite(all);
  const std::size_t pieces = solids(*joined).size();
  if (pieces != 1) {
    std::fprintf(stderr, "the plate and the flattened spheres joined into %zu solids\n", pieces);
  }
  CHECK(pieces == 1);
  if (pieces == 1) {
    const ShapePtr body = joined;
    CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
    const double v = volume(*body);
    CHECK(v > 600.0 * 600 * 10);
    CHECK(v < 600.0 * 600 * 10 + caps_volume);
  }
}

// The case of mitcad#1493 and mitcad#1529: a 1000 x 1000 x 10 plate joined
// at once with 38 overlapping spheres of radius 100-199 flattened along z
// by 0.2, their tops 3 mm above it (the layout of the MCP reproduction
// cli.mcp_memory_boolean). As B-splines this union took 2.3-2.5 GiB and
// ended in BRepCheck_UnorientableShape; as ellipsoids of revolution
// (mitcad#1504) it is one valid closed solid, built in about 2 s with a
// peak of 1.8 GB on 32 threads.
void test_plate_with_38_flattened_spheres() {
  const ShapePtr plate = extrude("F1", {rectangle(1, -500, -500, 1000, 1000)}, 0, 10);
  std::vector<ShapePtr> caps;
  double caps_volume = 0.0;
  for (int k = 0; k < 38; ++k) {
    const int row = k / 7;
    const int column = k % 7;
    const double r = 100 + (37 * k) % 100;
    const double z = 0.2;
    const gp_Pnt centre(-420 + 150 * column + 40 * (row % 2), -430 + 130 * row, 13.0 - z * r);
    caps.push_back(flattened_sphere("F" + std::to_string(2 + k), centre, r, z));
    caps_volume += 4.0 / 3.0 * kPi * r * r * r * z;
  }
  std::vector<const Shape*> all{plate.get()};
  for (const ShapePtr& cap : caps) {
    all.push_back(cap.get());
  }
  const ShapePtr joined = unite(all);
  const std::size_t pieces = solids(*joined).size();
  if (pieces != 1) {
    std::fprintf(stderr, "the plate and the 38 flattened spheres joined into %zu solids\n", pieces);
  }
  CHECK(pieces == 1);
  if (pieces == 1) {
    CHECK(BRepCheck_Analyzer(joined->occt()).IsValid());
    int shells = 0;
    for (TopExp_Explorer it(joined->occt(), TopAbs_SHELL); it.More(); it.Next()) {
      ++shells;
      CHECK(BRep_Tool::IsClosed(it.Current()));
    }
    CHECK(shells == 1);
    // More than the plate, less than the plate and every sphere whole; and
    // the volume this union had when it was first built (mitcad#1529), to
    // 0.01 %.
    const double v = volume(*joined);
    CHECK(v > 1000.0 * 1000 * 10);
    CHECK(v < 1000.0 * 1000 * 10 + caps_volume);
    CHECK(std::abs(v - 63745105.55) < 1e-4 * v);
  }
}

// Each face's triangulation, in the order the faces are explored (null
// where a face has none).
std::vector<const Poly_Triangulation*> triangulations(const TopoDS_Shape& shape) {
  std::vector<const Poly_Triangulation*> found;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    found.push_back(BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location).get());
  }
  return found;
}

// A Move of a body with curved faces (mitcad#838): move_shape turns and
// moves it as a location on the same B-rep, so every face keeps the
// triangulation it has (the same one, nothing meshed again, also when
// meshed again with the same options) and its names, the body is where
// the motion puts it, and the input is as it was: no location, its faces'
// own triangulations and its place. A mirror still moves a copy (without
// the meshes), and a mesh body still moves with its triangles.
void test_moves_keep_meshes() {
  // A peg of radius 5, 10 high: a cylinder and two caps.
  // Meshed as the view meshes a body's display: outside any operation, so
  // the input check (MITCAD_CHECK_INPUTS) would report the shape changed;
  // the peg is made and meshed while the check is off, and so is not
  // checked.
  const bool checking = mitcad::geometry::input_checks_enabled();
  mitcad::geometry::enable_input_checks(false);
  const ShapePtr original = extrude("F2", {circle(1, 0, 0, 5)}, 0, 10);
  BRepMesh_IncrementalMesh(original->occt(), 0.01, false, 0.5, false);
  mitcad::geometry::enable_input_checks(checking);
  const std::vector<const Poly_Triangulation*> meshes = triangulations(original->occt());
  CHECK(meshes.size() >= 3);
  for (const Poly_Triangulation* mesh : meshes) {
    CHECK(mesh != nullptr);
  }
  const TopLoc_Location placed = original->occt().Location();
  std::vector<TopLoc_Location> face_places;
  for (TopExp_Explorer it(original->occt(), TopAbs_FACE); it.More(); it.Next()) {
    face_places.push_back(it.Current().Location());
  }
  // 30 degrees about X, then 20 along X.
  Affine turn;
  const double c = std::cos(kPi / 6);
  const double s = std::sin(kPi / 6);
  turn.linear = {{{1, 0, 0}, {0, c, -s}, {0, s, c}}};
  turn.translation = {20, 0, 0};
  const ShapePtr moved = move_shape(*original, turn);
  CHECK(triangulations(moved->occt()) == meshes);
  CHECK(!moved->occt().Location().IsEqual(placed));
  CHECK(moved->face_count() == original->face_count());
  for (int i = 0; i < original->face_count() && i < moved->face_count(); ++i) {
    CHECK(moved->face_names(i) == original->face_names(i));
  }
  // Meshed again as the view meshes: the triangulations are kept.
  BRepMesh_IncrementalMesh(moved->occt(), 0.01, false, 0.5, false);
  CHECK(triangulations(moved->occt()) == meshes);
  const MassProperties at = mass_properties(*moved);
  CHECK_NEAR(at.volume, kPi * 25 * 10);
  CHECK(at.center.Distance(gp_Pnt(20, -5 * s, 5 * c)) < 1e-6);
  // The input is untouched.
  CHECK(original->occt().Location().IsEqual(placed));
  std::size_t face = 0;
  for (TopExp_Explorer it(original->occt(), TopAbs_FACE); it.More(); it.Next(), ++face) {
    CHECK(face < face_places.size() && it.Current().Location().IsEqual(face_places[face]));
  }
  CHECK(triangulations(original->occt()) == meshes);
  CHECK(mass_properties(*original).center.Distance(gp_Pnt(0, 0, 5)) < 1e-6);

  // A copy's faces renamed, sharing the meshes as well.
  const ShapePtr copy = move_shape(*original, turn, "F9:inst1");
  CHECK(triangulations(copy->occt()) == meshes);
  for (int i = 0; i < original->face_count() && i < copy->face_count(); ++i) {
    for (const std::string& name : original->face_names(i)) {
      CHECK(faces_named(*copy, "F9:inst1(" + name + ")") == 1);
    }
  }

  // A mirror is a copy, without the meshes.
  Affine mirror;
  mirror.linear = {{{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  const ShapePtr mirrored = move_shape(*original, mirror);
  CHECK_NEAR(volume(*mirrored), kPi * 25 * 10);
  for (const Poly_Triangulation* mesh : triangulations(mirrored->occt())) {
    CHECK(mesh == nullptr);
  }
  CHECK(triangulations(original->occt()) == meshes);
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

  // A Move's motion (move_shape, mitcad#838) moves its triangles too.
  const ShapePtr shifted = move_shape(*original, placed);
  CHECK(body_kind(shifted->occt()) == BodyKind::Mesh);
  CHECK(box_is(*shifted, 60, 0, 0, 100, 60, 20));
  CHECK(box_is(*original, 0, 0, 0, 60, 40, 20));
}

// A body OCCT's checker finds invalid, as some imported bodies are (one
// solid of two shells' faces in one shell, which is not connected): a
// rigid motion moves it as it is (mitcad#537: Reorient Design's Move of such
// a body failed); a general affine map still checks its result.
void test_invalid_bodies_move() {
  const ShapePtr first = block();
  const ShapePtr second = transform_shape(*first, translation(100, 0, 0), "F9:inst1");
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  std::vector<Shape::NamedFace> faces;
  for (const ShapePtr& part : {first, second}) {
    for (int i = 0; i < part->face_count(); ++i) {
      builder.Add(shell, part->face(i));
      faces.push_back({part->face(i), part->face_names(i)});
    }
  }
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  builder.Add(solid, shell);
  const auto invalid = std::make_shared<Shape>(solid, faces);
  CHECK(!BRepCheck_Analyzer(invalid->occt()).IsValid());

  Affine quarter;
  quarter.linear = {{{0, -1, 0}, {1, 0, 0}, {0, 0, 1}}};
  const ShapePtr turned = transform_shape(*invalid, quarter);
  CHECK(box_is(*turned, -40, 0, 0, 0, 160, 20));
  CHECK(faces_named(*turned, end_cap("F2", 1)) == 1);
  CHECK(faces_named(*turned, "F9:inst1(" + end_cap("F2", 1) + ")") == 1);
  const ShapePtr moved = transform_shape(*invalid, translation(0, 0, 10));
  CHECK(box_is(*moved, 0, 0, 10, 160, 40, 30));

  Affine uneven;
  uneven.linear = {{{2, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  CHECK(throws_with([&] { transform_shape(*invalid, uneven); }, "the transform produced an invalid shape"));
}

// A mesh body in the surface operations (mitcad#399): the booleans,
// unions, dressups and face operations refuse it before OCCT's algorithms
// get it (their face intersection read its null surface, mitcad#392);
// a tool's contacts take it as apart, and its face's geometry is "mesh".
void test_mesh_bodies_refused() {
  const ShapePtr mesh = mesh_block();
  const ShapePtr solid = block();
  const char* refused = "a mesh body (faces without surfaces) cannot be used";
  CHECK(throws_with([&] { boolean(BooleanOp::Join, {solid.get()}, *mesh); }, refused));
  CHECK(throws_with([&] { boolean(BooleanOp::Cut, {mesh.get()}, *solid); }, refused));
  CHECK(throws_with([&] { unite({solid.get(), mesh.get()}); }, refused));
  CHECK(throws_with([&] { fillet("F9", *mesh, std::vector<std::string>{"E{a|b}"}, 1.0); }, refused));
  ShellSpec shell;
  shell.inside = 1.0;
  CHECK(throws_with([&] { ::mitcad::geometry::shell("F9", *mesh, shell); }, refused));
  CHECK(throws_with([&] { split_body("F9", *mesh, Tool::of_plane(gp_Pln(gp_Pnt(10, 0, 0), gp_Dir(1, 0, 0))), true); },
                    refused));
  CHECK(throws_with([&] { split_body("F9", *solid, Tool::of_body(mesh), true); }, refused));
  const std::vector<ToolContact> contacts = tool_contacts(*solid, {mesh.get()});
  CHECK(contacts.size() == 1 && contacts[0] == ToolContact::Apart);
  CHECK(face_geometry(*mesh, mitcad::geometry::face_name("F7", "import", "0")).type == "mesh");
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

  // The walls of two separate holes: a solid of each (mitcad#719).
  const ShapePtr second = extrude("F5", {circle(7, 40, 10, 2)}, -1, 6);
  const ShapePtr two_holes = run(BooleanOp::Cut, with_hole, second).pieces.at(0).shape;
  const std::string other_wall = mitcad::geometry::face_name("F5", "side", "c7");
  const FaceTool holes = face_tool(*two_holes, {wall, other_wall});
  CHECK(!holes.material);
  CHECK(solids(*holes.tool).size() == 2);
  CHECK(near(volume(*holes.tool), (kPi * 2.5 * 2.5 + kPi * 2 * 2) * 5, 1e-6));
  CHECK(faces_named(*holes.tool, other_wall) == 1);

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

// A body adopted from a shape (mitcad#244) is the body its B-rep data reads
// as: the same faces in the same order with the same names, sharing the
// shape's B-rep, with what was measured of the shape that does not depend
// on names.
void test_adopted_bodies_are_their_data_read() {
  const ShapePtr block = extrude("F2", {rectangle(1, -10, 5, 60, 40), circle(5, 80, 0, 5)}, 0, 20);
  const MassProperties props = mass_properties(*block);
  bounding_box(*block);
  block->set_checked_valid(true);
  CHECK(!edge_middles(*block).empty());

  const std::string data = brep_data(block->occt());
  const ShapePtr read = import_body(read_brep_data(data.data(), data.size()), "F9", 3);
  const ShapePtr adopted = adopt_body(*block, "F9", 3);
  CHECK(adopted->occt().IsEqual(block->occt()));
  CHECK(!read->occt().IsSame(block->occt()));
  CHECK(adopted->face_count() == read->face_count());
  for (int i = 0; i < read->face_count(); ++i) {
    CHECK(adopted->face_names(i) == read->face_names(i));
  }
  CHECK(adopted->edge_count() == read->edge_count());
  for (int i = 0; i < read->edge_count(); ++i) {
    CHECK(adopted->edge_name(i) == read->edge_name(i));
  }
  CHECK(adopted->face_names(0).front() == face_name("F9", "import", "3"));
  // Measures carried over; edge middles carry names and are made anew.
  CHECK(adopted->measured() == block->measured());
  CHECK(adopted->bounds() == block->bounds());
  CHECK(adopted->checked_valid() == std::optional<bool>(true));
  CHECK(!read->checked_valid().has_value());
  CHECK(adopted->kept_edge_middles() == nullptr);
  CHECK(near(mass_properties(*read).volume, props.volume, 1e-9));
  const std::vector<EdgeMiddle> middles = edge_middles(*adopted);
  const std::vector<EdgeMiddle> read_middles = edge_middles(*read);
  CHECK(middles.size() == read_middles.size());
  for (std::size_t i = 0; i < middles.size() && i < read_middles.size(); ++i) {
    CHECK(middles[i].name == read_middles[i].name);
  }
}

} // namespace

void transform_tests() {
  test_moves_keep_or_rename_faces();
  test_mirrors_and_scales();
  test_flattened_spheres();
  test_plate_with_flattened_spheres();
  test_plate_with_38_flattened_spheres();
  test_mesh_bodies_move();
  test_moves_keep_meshes();
  test_invalid_bodies_move();
  test_mesh_bodies_refused();
  test_unions_keep_names();
  test_tools_of_faces();
  test_primitives_name_their_faces();
  test_adopted_bodies_are_their_data_read();
}

} // namespace test
