// SPDX-License-Identifier: MIT
// Data exchange tests: round trips through STEP, IGES, STL and OBJ.
// Usage: test_geometry_io <directory for the test files>

#include "mitcad/io/exchange.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <map>
#include <sstream>
#include <string>
#include <utility>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_io.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool near(double a, double b, double tolerance) { return std::abs(a - b) <= tolerance; }

bool relative_near(double a, double b, double tolerance) {
  return std::abs(a - b) <= tolerance * std::max(1.0, std::abs(b));
}

using mitcad::io::Body;
using mitcad::io::Color;

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

double area(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

struct Box {
  double min[3];
  double max[3];
};

Box bbox(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, true, false);
  Box result{};
  box.Get(result.min[0], result.min[1], result.min[2], result.max[0], result.max[1], result.max[2]);
  return result;
}

bool same_box(const Box& a, const Box& b, double tolerance) {
  for (int i = 0; i < 3; ++i) {
    if (!near(a.min[i], b.min[i], tolerance) || !near(a.max[i], b.max[i], tolerance)) {
      std::fprintf(stderr, "bbox axis %d: [%g, %g] vs [%g, %g]\n", i, a.min[i], a.max[i], b.min[i],
                   b.max[i]);
      return false;
    }
  }
  return true;
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// A 60 x 40 x 20 block with all edges rounded by 3 mm.
TopoDS_Shape filleted_block() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(60.0, 40.0, 20.0).Shape();
  BRepFilletAPI_MakeFillet fillet(box);
  for (TopExp_Explorer it(box, TopAbs_EDGE); it.More(); it.Next()) {
    fillet.Add(3.0, TopoDS::Edge(it.Current()));
  }
  return fillet.Shape();
}

// A 50 x 30 x 20 block at (100, 0, 0) with a 12 mm hole through it.
TopoDS_Shape holed_block() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(100.0, 0.0, 0.0), 50.0, 30.0, 20.0).Shape();
  const TopoDS_Shape hole =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(125.0, 15.0, -1.0), gp::DZ()), 6.0, 22.0).Shape();
  return BRepAlgoAPI_Cut(box, hole).Shape();
}

std::vector<Body> sample_bodies() {
  return {Body{"Filleted block", filleted_block(), Color{0.8, 0.1, 0.1}, {}},
          Body{"Holed block \xC3\xA4", holed_block(), Color{0.2, 0.4, 0.9}, {}}};
}

bool same_color(const std::optional<Color>& a, const std::optional<Color>& b, double tolerance) {
  if (!a || !b) {
    return !a && !b;
  }
  return near(a->r, b->r, tolerance) && near(a->g, b->g, tolerance) && near(a->b, b->b, tolerance);
}

// Exact B-rep round trip: same volume, area and bounding box.
void check_same_solid(const TopoDS_Shape& original, const TopoDS_Shape& read) {
  CHECK(count(read, TopAbs_SOLID) == 1);
  CHECK(relative_near(volume(read), volume(original), 1e-6));
  CHECK(relative_near(area(read), area(original), 1e-6));
  CHECK(same_box(bbox(read), bbox(original), 1e-4));
}

std::string read_text(const std::string& path) {
  std::ifstream file(path, std::ios::binary);
  return std::string(std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>());
}

// Largest coordinate of the CARTESIAN_POINTs of a STEP file.
double max_step_coordinate(const std::string& path) {
  const std::string text = read_text(path);
  double largest = -1e300;
  std::size_t at = 0;
  while ((at = text.find("CARTESIAN_POINT(", at)) != std::string::npos) {
    // CARTESIAN_POINT('name',(x,y,z))
    const std::size_t open = text.find("(", text.find(",", at));
    const std::size_t close = text.find(")", open);
    std::string numbers = text.substr(open + 1, close - open - 1);
    std::replace(numbers.begin(), numbers.end(), ',', ' ');
    std::istringstream stream(numbers);
    double value = 0.0;
    while (stream >> value) {
      largest = std::max(largest, value);
    }
    at = close;
  }
  return largest;
}

void test_step(const std::string& dir) {
  const std::vector<Body> bodies = sample_bodies();
  for (const auto schema : {mitcad::io::StepSchema::AP214, mitcad::io::StepSchema::AP242}) {
    const bool ap242 = schema == mitcad::io::StepSchema::AP242;
    const std::string path = dir + (ap242 ? "/bodies-ap242.step" : "/bodies-ap214.stp");
    mitcad::io::write_step(path, bodies, {schema, mitcad::io::LengthUnit::Millimeter});
    const std::string text = read_text(path);
    CHECK(text.find(ap242 ? "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING" : "AUTOMOTIVE_DESIGN") !=
          std::string::npos);

    const std::vector<Body> read = mitcad::io::read_step(path);
    CHECK(read.size() == 2);
    if (read.size() != 2) {
      continue;
    }
    for (std::size_t i = 0; i < 2; ++i) {
      CHECK(read[i].name == bodies[i].name);
      CHECK(same_color(read[i].color, bodies[i].color, 1e-3));
      check_same_solid(bodies[i].shape, read[i].shape);
    }
  }
}

void test_step_units(const std::string& dir) {
  // A box from the origin to (1, 2, 3) inches.
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(25.4, 50.8, 76.2).Shape();
  const std::string inch_path = dir + "/box-inch.step";
  const std::string mm_path = dir + "/box-mm.step";
  mitcad::io::write_step(inch_path, {Body{"Box", box, std::nullopt, {}}},
                         {mitcad::io::StepSchema::AP214, mitcad::io::LengthUnit::Inch});
  mitcad::io::write_step(mm_path, {Body{"Box", box, std::nullopt, {}}});
  // The file stores inches, and the import converts them back.
  CHECK(read_text(inch_path).find("INCH") != std::string::npos);
  CHECK(near(max_step_coordinate(inch_path), 3.0, 1e-9));
  CHECK(near(max_step_coordinate(mm_path), 76.2, 1e-9));
  for (const std::string& path : {inch_path, mm_path}) {
    const std::vector<Body> read = mitcad::io::read_step(path);
    CHECK(read.size() == 1);
    if (!read.empty()) {
      CHECK(read[0].name == "Box");
      CHECK(!read[0].color);
      check_same_solid(box, read[0].shape);
    }
  }
}

void test_step_multi_solid_part() {
  // One part with two solids comes back as two bodies.
  BRep_Builder builder;
  TopoDS_Compound pair;
  builder.MakeCompound(pair);
  builder.Add(pair, BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape());
  builder.Add(pair, BRepPrimAPI_MakeBox(gp_Pnt(20.0, 0.0, 0.0), 10.0, 10.0, 5.0).Shape());
  const std::string path = std::filesystem::temp_directory_path().string() + "/mitcad-io-pair.step";
  mitcad::io::write_step(path, {Body{"Pair", pair, Color{0.0, 1.0, 0.0}, {}}});
  const std::vector<Body> read = mitcad::io::read_step(path);
  std::filesystem::remove(path);
  CHECK(read.size() == 2);
  if (read.size() == 2) {
    CHECK(read[0].name == "Pair");
    CHECK(read[1].name == "Pair (2)");
    CHECK(near(volume(read[0].shape), 1000.0, 1e-6));
    CHECK(near(volume(read[1].shape), 500.0, 1e-6));
    CHECK(same_color(read[1].color, Color{0.0, 1.0, 0.0}, 1e-3));
  }
}

void test_iges(const std::string& dir) {
  const std::vector<Body> bodies = sample_bodies();
  for (const bool solids : {true, false}) {
    const std::string path = dir + (solids ? "/bodies-solids.igs" : "/bodies-faces.iges");
    mitcad::io::write_iges(path, bodies, {mitcad::io::LengthUnit::Millimeter, solids});
    std::vector<Body> read = mitcad::io::read_iges(path);
    CHECK(read.size() == 2);
    if (read.size() != 2) {
      continue;
    }
    // Sewn bodies need not keep the file order.
    std::sort(read.begin(), read.end(), [](const Body& a, const Body& b) {
      return bbox(a.shape).min[0] < bbox(b.shape).min[0];
    });
    for (std::size_t i = 0; i < 2; ++i) {
      check_same_solid(bodies[i].shape, read[i].shape);
      CHECK(same_color(read[i].color, bodies[i].color, 1e-2));
    }
    // IGES names are ASCII.
    CHECK(read[0].name == "Filleted block");
    CHECK(read[1].name == "Holed block _");
  }
  // Inches: the reader converts to millimetres.
  const std::string inch_path = dir + "/bodies-inch.igs";
  mitcad::io::write_iges(inch_path, {bodies[1]}, {mitcad::io::LengthUnit::Inch, true});
  const std::vector<Body> read = mitcad::io::read_iges(inch_path);
  CHECK(read.size() == 1);
  if (!read.empty()) {
    check_same_solid(bodies[1].shape, read[0].shape);
  }
}

void test_triangulate_keeps_original() {
  const TopoDS_Shape block = filleted_block();
  const TopoDS_Shape mesh = mitcad::io::triangulate(block, mitcad::io::MeshOptions{});
  CHECK(mitcad::io::mesh_stats(block).triangles == 0);
  const mitcad::io::MeshStats stats = mitcad::io::mesh_stats(mesh);
  CHECK(stats.triangles > 100);
  CHECK(stats.faces == static_cast<std::size_t>(count(block, TopAbs_FACE)));
  CHECK(!mitcad::io::is_mesh(block));
  bool threw = false;
  try {
    mitcad::io::triangulate(block, mitcad::io::MeshOptions{0.0, 0.5, false});
  } catch (const mitcad::io::Error&) {
    threw = true;
  }
  CHECK(threw);
}

void test_stl(const std::string& dir) {
  const TopoDS_Shape block = filleted_block();
  const Box exact = bbox(block);
  std::size_t previous = 0;
  for (const auto refinement :
       {mitcad::io::Refinement::Low, mitcad::io::Refinement::Medium, mitcad::io::Refinement::High}) {
    const mitcad::io::MeshOptions mesh = mitcad::io::MeshOptions::from(refinement);
    const std::size_t triangles =
        mitcad::io::mesh_stats(mitcad::io::triangulate(block, mesh)).triangles;
    CHECK(triangles > previous);
    previous = triangles;

    for (const auto format : {mitcad::io::StlFormat::Binary, mitcad::io::StlFormat::Ascii}) {
      const bool ascii = format == mitcad::io::StlFormat::Ascii;
      const std::string path = dir + "/block-" + std::to_string(static_cast<int>(refinement)) +
                               (ascii ? "-ascii.stl" : "-binary.stl");
      mitcad::io::write_stl(path, {block}, {format, mesh});
      const std::string text = read_text(path);
      if (ascii) {
        CHECK(text.rfind("solid", 0) == 0);
      } else {
        // 80 byte header, triangle count, 50 bytes per triangle.
        CHECK(text.size() == 84 + 50 * triangles);
      }
      const Body read = mitcad::io::read_stl(path);
      CHECK(read.name == path.substr(dir.size() + 1, path.size() - dir.size() - 5));
      CHECK(mitcad::io::is_mesh(read.shape));
      // The reader merges coincident nodes and drops the triangles that
      // collapse (OCCT meshes degenerate fillet corners into slivers).
      const std::size_t read_triangles = mitcad::io::mesh_stats(read.shape).triangles;
      CHECK(read_triangles <= triangles && read_triangles * 100 >= triangles * 98);
      // Mesh vertices lie on the surface: the box is within the deflection.
      CHECK(same_box(bbox(read.shape), exact, mesh.linear_deflection + 1e-3));
      // A mesh body exports again as it is.
      const std::string again = dir + "/again.stl";
      mitcad::io::write_stl(again, {read.shape}, {format, mesh});
      CHECK(mitcad::io::mesh_stats(mitcad::io::read_stl(again).shape).triangles ==
            read_triangles);
    }
  }
  // Two shapes into one file; inches on import.
  const std::string path = dir + "/two.stl";
  const TopoDS_Shape holed = holed_block();
  mitcad::io::write_stl(path, {block, holed});
  const Body inches = mitcad::io::read_stl(path, {25.4});
  const Box box = bbox(inches.shape);
  CHECK(near(box.max[0], 150.0 * 25.4, 1e-3));
  CHECK(near(box.min[2], 0.0, 1e-3));
}

// Each edge of the mesh shared by two triangles that run along it in
// opposite directions: a closed surface facing one way.
bool closed(const mitcad::io::IndexedMesh& mesh) {
  std::map<std::pair<std::uint32_t, std::uint32_t>, int> edges;
  for (const auto& t : mesh.triangles) {
    for (int k = 0; k < 3; ++k) {
      ++edges[{t[k], t[(k + 1) % 3]}];
    }
  }
  for (const auto& [edge, uses] : edges) {
    const auto back = edges.find({edge.second, edge.first});
    if (uses != 1 || back == edges.end() || back->second != 1) {
      std::fprintf(stderr, "edge %u-%u: %d uses\n", edge.first, edge.second, uses);
      return false;
    }
  }
  return !mesh.triangles.empty();
}

double mesh_volume(const mitcad::io::IndexedMesh& mesh) {
  double six = 0.0;
  for (const auto& t : mesh.triangles) {
    const auto& a = mesh.vertices[t[0]];
    const auto& b = mesh.vertices[t[1]];
    const auto& c = mesh.vertices[t[2]];
    six += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) +
           a[2] * (b[0] * c[1] - b[1] * c[0]);
  }
  return six / 6.0;
}

// The meshes of 3MF export (mitcad#13): closed, facing outwards, the
// volume within the deviation (times the area) of the body's, the body
// itself untouched; a mesh body's own triangles.
void test_indexed_mesh(const std::string& dir) {
  using mitcad::io::MeshOptions;
  using mitcad::io::Refinement;
  const TopoDS_Shape sphere = BRepPrimAPI_MakeSphere(gp_Pnt(0.0, 0.0, 50.0), 15.0).Shape();
  const TopoDS_Shape shapes[] = {BRepPrimAPI_MakeBox(10.0, 20.0, 30.0).Shape(), filleted_block(), holed_block(),
                                 sphere};
  for (const TopoDS_Shape& shape : shapes) {
    for (const Refinement refinement : {Refinement::Low, Refinement::High}) {
      const MeshOptions options = MeshOptions::from(refinement);
      const mitcad::io::IndexedMesh mesh = mitcad::io::indexed_mesh(shape, options);
      CHECK(closed(mesh));
      const double exact = volume(shape);
      const double meshed = mesh_volume(mesh);
      std::fprintf(stderr, "indexed mesh: %zu triangles, volume %.4f of %.4f\n", mesh.triangles.size(), meshed,
                   exact);
      CHECK(meshed > 0.0);
      CHECK(near(meshed, exact, area(shape) * options.linear_deflection));
      CHECK(mitcad::io::mesh_stats(shape).triangles == 0);
    }
  }
  // Finer is closer.
  const TopoDS_Shape block = filleted_block();
  const double exact = volume(block);
  CHECK(std::abs(mesh_volume(mitcad::io::indexed_mesh(block, MeshOptions::from(Refinement::High))) - exact) <
        std::abs(mesh_volume(mitcad::io::indexed_mesh(block, MeshOptions::from(Refinement::Low))) - exact));
  // A mesh body (an STL read back) gives its own triangles.
  const std::string path = dir + "/indexed.stl";
  mitcad::io::write_stl(path, {block}, {mitcad::io::StlFormat::Binary, MeshOptions::from(Refinement::Medium)});
  const Body read = mitcad::io::read_stl(path);
  const mitcad::io::IndexedMesh mesh = mitcad::io::indexed_mesh(read.shape, MeshOptions::from(Refinement::High));
  CHECK(mesh.triangles.size() == mitcad::io::mesh_stats(read.shape).triangles);
  CHECK(closed(mesh));
  CHECK(near(mesh_volume(mesh), exact, area(block) * 0.03));
}

void test_obj(const std::string& dir) {
  const std::vector<Body> bodies = sample_bodies();
  const std::string path = dir + "/bodies.obj";
  const mitcad::io::MeshOptions mesh = mitcad::io::MeshOptions::from(mitcad::io::Refinement::Low);
  mitcad::io::write_obj(path, bodies, {mesh});
  CHECK(std::filesystem::exists(dir + "/bodies.mtl"));
  const std::vector<Body> read = mitcad::io::read_obj(path);
  CHECK(read.size() == 2);
  if (read.size() == 2) {
    for (std::size_t i = 0; i < 2; ++i) {
      CHECK(read[i].name == bodies[i].name);
      CHECK(mitcad::io::is_mesh(read[i].shape));
      CHECK(mitcad::io::mesh_stats(read[i].shape).triangles ==
            mitcad::io::mesh_stats(mitcad::io::triangulate(bodies[i].shape, mesh)).triangles);
      CHECK(same_box(bbox(read[i].shape), bbox(bodies[i].shape), mesh.linear_deflection + 1e-3));
      CHECK(same_color(read[i].color, bodies[i].color, 1e-3));
    }
  }
  const std::vector<Body> scaled = mitcad::io::read_obj(path, {10.0});
  CHECK(scaled.size() == 2 && near(bbox(scaled[1].shape).max[0], 1500.0, 1e-2));
}

bool box_near(const Box& a, const Box& b, double tolerance) {
  for (int i = 0; i < 3; ++i) {
    if (!near(a.min[i], b.min[i], tolerance) || !near(a.max[i], b.max[i], tolerance)) {
      return false;
    }
  }
  return true;
}

// The body read back whose box is `box`, or null.
const Body* with_box(const std::vector<Body>& read, const Box& box, double tolerance) {
  const auto found = std::find_if(read.begin(), read.end(),
                                  [&](const Body& body) { return box_near(bbox(body.shape), box, tolerance); });
  return found == read.end() ? nullptr : &*found;
}

// Placements (mitcad#19): a 20 x 10 x 5 pin turned a quarter about z and
// moved 100 mm along x, and moved 50 mm along y; a block once as it is.
// Every format has the copies where the placements put them; STEP as an
// assembly with the pin's part placed twice.
void test_placements(const std::string& dir) {
  using mitcad::io::Placement;
  Placement turned;
  turned.rotation = {{{0.0, -1.0, 0.0}, {1.0, 0.0, 0.0}, {0.0, 0.0, 1.0}}};
  turned.translation = {100.0, 0.0, 0.0};
  turned.name = "Pin:1";
  Placement moved;
  moved.translation = {0.0, 50.0, 0.0};
  moved.name = "Pin:2";
  CHECK(!turned.is_identity() && Placement{}.is_identity());
  const TopoDS_Shape pin = BRepPrimAPI_MakeBox(20.0, 10.0, 5.0).Shape();
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-50.0, -50.0, 0.0), 10.0, 10.0, 10.0).Shape();
  const std::vector<Body> bodies = {Body{"Pin", pin, Color{0.1, 0.6, 0.2}, {turned, moved}},
                                    Body{"Block", block, std::nullopt, {}}};
  const Box first{{90.0, 0.0, 0.0}, {100.0, 20.0, 5.0}};
  const Box second{{0.0, 50.0, 0.0}, {20.0, 60.0, 5.0}};
  const Box own = bbox(block);
  // The copies in the order of the bodies and placements, as named.
  const std::vector<Body> placed = mitcad::io::placed_bodies(bodies);
  CHECK(placed.size() == 3);
  if (placed.size() == 3) {
    CHECK(placed[0].name == "Pin (Pin:1)" && placed[1].name == "Pin (Pin:2)" && placed[2].name == "Block");
    CHECK(box_near(bbox(placed[0].shape), first, 1e-6) && box_near(bbox(placed[1].shape), second, 1e-6));
    CHECK(placed[2].shape.IsSame(block) && placed[0].placements.empty());
    CHECK(same_color(placed[1].color, bodies[0].color, 0.0));
  }
  // The pin itself does not move.
  CHECK(box_near(bbox(pin), Box{{0.0, 0.0, 0.0}, {20.0, 10.0, 5.0}}, 1e-6));

  // STEP: an assembly named after the file, the pin's part once with an
  // instance per placement; read back, a body per instance.
  const std::string step = dir + "/placed.step";
  mitcad::io::write_step(step, bodies);
  const std::string text = read_text(step);
  const auto occurrences = [&text](const std::string& what) {
    std::size_t n = 0;
    for (std::size_t at = text.find(what); at != std::string::npos; at = text.find(what, at + 1)) {
      ++n;
    }
    return n;
  };
  CHECK(occurrences("PRODUCT('Pin'") == 1);
  CHECK(occurrences("PRODUCT('placed'") == 1);
  CHECK(occurrences("NEXT_ASSEMBLY_USAGE_OCCURRENCE") == 3);
  CHECK(occurrences("'Pin:1'") >= 1 && occurrences("'Pin:2'") >= 1);
  const std::vector<Body> from_step = mitcad::io::read_step(step);
  CHECK(from_step.size() == 3);
  for (const Box& box : {first, second}) {
    const Body* found = with_box(from_step, box, 1e-6);
    CHECK(found != nullptr && found->name == "Pin" && same_color(found->color, bodies[0].color, 1e-3));
  }
  const Body* found = with_box(from_step, own, 1e-6);
  CHECK(found != nullptr && found->name == "Block");

  // IGES and OBJ: a moved copy per placement, named with it.
  const std::string iges = dir + "/placed.igs";
  mitcad::io::write_iges(iges, bodies);
  const std::string obj = dir + "/placed.obj";
  const mitcad::io::MeshOptions mesh = mitcad::io::MeshOptions::from(mitcad::io::Refinement::Low);
  mitcad::io::write_obj(obj, bodies, {mesh});
  for (const std::vector<Body>& read : {mitcad::io::read_iges(iges), mitcad::io::read_obj(obj)}) {
    CHECK(read.size() == 3);
    const Body* a = with_box(read, first, 1e-3);
    const Body* b = with_box(read, second, 1e-3);
    const Body* c = with_box(read, own, 1e-3);
    CHECK(a != nullptr && a->name == "Pin (Pin:1)" && same_color(a->color, bodies[0].color, 1e-2));
    CHECK(b != nullptr && b->name == "Pin (Pin:2)");
    CHECK(c != nullptr && c->name == "Block");
  }

  // BRep and STL (by extension): the copies in one shape.
  const std::string brep = dir + "/placed.brep";
  mitcad::io::write_file(brep, bodies);
  const std::vector<Body> from_brep = mitcad::io::read_file(brep);
  CHECK(from_brep.size() == 1 && count(from_brep[0].shape, TopAbs_SOLID) == 3);
  std::vector<Body> solids;
  for (TopExp_Explorer it(from_brep[0].shape, TopAbs_SOLID); it.More(); it.Next()) {
    solids.push_back(Body{"", it.Current(), std::nullopt, {}});
  }
  CHECK(with_box(solids, first, 1e-6) && with_box(solids, second, 1e-6) && with_box(solids, own, 1e-6));
  const std::string stl = dir + "/placed.stl";
  mitcad::io::write_file(stl, bodies);
  CHECK(box_near(bbox(mitcad::io::read_stl(stl).shape), Box{{-50.0, -50.0, 0.0}, {100.0, 60.0, 10.0}}, 1e-3));

  // A mesh body moves too (OBJ, BRep), by its location.
  const std::string pin_stl = dir + "/pin.stl";
  mitcad::io::write_stl(pin_stl, {pin});
  const Body scan{"Scan", mitcad::io::read_stl(pin_stl).shape, std::nullopt, {turned}};
  const std::string mesh_obj = dir + "/scan.obj";
  mitcad::io::write_obj(mesh_obj, {scan}, {mesh});
  const std::vector<Body> scan_obj = mitcad::io::read_obj(mesh_obj);
  CHECK(scan_obj.size() == 1 && scan_obj[0].name == "Scan" && box_near(bbox(scan_obj[0].shape), first, 1e-3));
  const std::string mesh_brep = dir + "/scan.brep";
  mitcad::io::write_brep(mesh_brep, {scan});
  const std::vector<Body> scan_brep = mitcad::io::read_file(mesh_brep);
  CHECK(scan_brep.size() == 1 && mitcad::io::is_mesh(scan_brep[0].shape) &&
        box_near(bbox(scan_brep[0].shape), first, 1e-3));
}

void test_dispatch(const std::string& dir) {
  using mitcad::io::Format;
  CHECK(mitcad::io::format_of("a/b.STEP") == Format::Step);
  CHECK(mitcad::io::format_of("x.igs") == Format::Iges);
  CHECK(mitcad::io::format_of("x.Stl") == Format::Stl);
  CHECK(mitcad::io::format_of("x.obj") == Format::Obj);
  CHECK(mitcad::io::format_of("x.brep") == Format::Brep);
  CHECK(!mitcad::io::format_of("x.dwg"));
  CHECK(!mitcad::io::format_of("dir.v2/file"));

  const std::vector<Body> bodies = sample_bodies();
  const std::string path = dir + "/bodies.brep";
  mitcad::io::write_file(path, bodies);
  const std::vector<Body> read = mitcad::io::read_file(path);
  CHECK(read.size() == 1 && count(read[0].shape, TopAbs_SOLID) == 2);
  bool threw = false;
  try {
    mitcad::io::read_file(dir + "/missing.step");
  } catch (const mitcad::io::Error&) {
    threw = true;
  }
  CHECK(threw);
  threw = false;
  try {
    mitcad::io::write_file(dir + "/x.dwg", bodies);
  } catch (const mitcad::io::Error&) {
    threw = true;
  }
  CHECK(threw);
}

} // namespace

int main(int argc, char** argv) {
  const std::string dir =
      argc > 1 ? argv[1]
               : (std::filesystem::temp_directory_path() / "mitcad-test-geometry-io").string();
  std::filesystem::create_directories(dir);
  mitcad::io::silence_occt_messages();
  try {
    test_step(dir);
    test_step_units(dir);
    test_step_multi_solid_part();
    test_iges(dir);
    test_triangulate_keeps_original();
    test_stl(dir);
    test_indexed_mesh(dir);
    test_obj(dir);
    test_placements(dir);
    test_dispatch(dir);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "unexpected exception: %s\n", error.what());
    return 1;
  }
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("all geometry io tests passed\n");
  return 0;
}
