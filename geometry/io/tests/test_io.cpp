// SPDX-License-Identifier: MIT
// Data exchange tests: round trips through STEP, IGES, STL and OBJ.
// Usage: test_geometry_io <directory for the test files>

#include "mitcad/io/exchange.hpp"
#include "mitcad/io/face_mesh.hpp"
#include "mitcad/io/mesh_compare.hpp"
#include "mitcad/io/mesh_store.hpp"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <map>
#include <memory>
#include <new>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <thread>
#include <utility>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepMesh_Context.hxx>
#include <BRepMesh_FaceChecker.hxx>
#include <BRepMesh_MeshAlgoFactory.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRepBndLib.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <IMeshData_Edge.hxx>
#include <IMeshData_Face.hxx>
#include <IMeshData_Model.hxx>
#include <IMeshTools_MeshAlgo.hxx>
#include <IMeshTools_MeshAlgoFactory.hxx>
#include <IMeshTools_Parameters.hxx>
#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressScope.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_PolygonOnTriangulation.hxx>
#include <Poly_Triangle.hxx>
#include <Poly_Triangulation.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <vector>

// windows.h (which OCCT headers such as Message_ProgressIndicator.hxx may
// bring in on Windows) defines near and far as empty macros, which would
// break near() below.
#ifdef near
#undef near
#endif
#ifdef far
#undef far
#endif

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

// The deviation per millimetre of radius of a turn in 500 straight pieces:
// the bound of cylinders and cones for 20 000 triangles a face (mitcad#438).
const double kTurnDeviation = 1.0 - std::cos(3.14159265358979323846 / 500.0);

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
    mitcad::io::triangulate(block, mitcad::io::MeshOptions{0.0, 0.5, false, 0, false, false});
  } catch (const mitcad::io::Error&) {
    threw = true;
  }
  CHECK(threw);
}

// A hemispherical dome shell (mitcad#398): sphere faces of radii 4450 and
// 4550 mm times `scale`, and a flat ring.
TopoDS_Shape dome(double scale) {
  const double quarter = 2.0 * std::atan(1.0);
  const gp_Ax2 axis;
  const TopoDS_Shape outer = BRepPrimAPI_MakeSphere(axis, 4550.0 * scale, 0.0, quarter).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeSphere(axis, 4450.0 * scale, 0.0, quarter).Shape();
  return BRepAlgoAPI_Cut(outer, inner).Shape();
}

// Per face: its surface type and triangle count.
std::vector<std::pair<GeomAbs_SurfaceType, int>> face_triangles(const TopoDS_Shape& meshed) {
  std::vector<std::pair<GeomAbs_SurfaceType, int>> faces;
  for (TopExp_Explorer it(meshed, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    const auto triangulation = BRep_Tool::Triangulation(face, location);
    faces.emplace_back(BRepAdaptor_Surface(face).GetType(),
                       triangulation.IsNull() ? -1 : triangulation->NbTriangles());
  }
  return faces;
}

// Building-scale doubly curved faces (mitcad#398): the default export
// mesh gives each sphere face of a 9 m dome at most about the preset's
// triangles (with the absolute 0.03 mm, about 1.5 million in all), while
// a dome of 9 cm meshes exactly as without the limit. Counts, not times.
void test_building_scale_sphere(const std::string& dir) {
  using mitcad::io::MeshOptions;
  using mitcad::io::Refinement;
  const MeshOptions medium = MeshOptions::from(Refinement::Medium);
  CHECK(medium.face_triangles == 20000);
  const TopoDS_Shape big = dome(1.0);
  const TopoDS_Shape small = dome(0.01);
  CHECK(count(big, TopAbs_FACE) == 3);

  // The floor: none for the small dome's faces or the ring; a large
  // cylinder's side has the turn's (mitcad#438: 2e-5 of its radius), its
  // flat ends none.
  for (TopExp_Explorer it(small, TopAbs_FACE); it.More(); it.Next()) {
    CHECK(mitcad::io::face_deviation_floor(TopoDS::Face(it.Current()), 0.03, 20000) == 0.0);
  }
  for (TopExp_Explorer it(big, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const double floor = mitcad::io::face_deviation_floor(face, 0.03, 20000);
    if (BRepAdaptor_Surface(face).GetType() == GeomAbs_Sphere) {
      // 0.8 * 2 pi R / 20000: 1.13 mm for R 4.5 m.
      CHECK(floor > 1.0 && floor < 1.25);
    } else {
      CHECK(floor == 0.0);
    }
  }
  const TopoDS_Shape tank = BRepPrimAPI_MakeCylinder(5000.0, 20000.0).Shape();
  for (TopExp_Explorer it(tank, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const double floor = mitcad::io::face_deviation_floor(face, 0.03, 20000);
    if (BRepAdaptor_Surface(face).GetType() == GeomAbs_Cylinder) {
      CHECK(near(floor, 5000.0 * kTurnDeviation, 1e-6)); // 0.0987 mm
    } else {
      CHECK(floor == 0.0);
    }
  }

  // The default export mesh of the big dome: bounded per sphere face.
  std::size_t total = 0;
  for (const auto& [type, triangles] : face_triangles(mitcad::io::triangulate(big, medium))) {
    CHECK(triangles > 0);
    if (type == GeomAbs_Sphere) {
      std::printf("Dome sphere face: %d triangles\n", triangles);
      CHECK(triangles > 5000 && triangles <= 30000);
    }
    total += static_cast<std::size_t>(std::max(triangles, 0));
  }
  CHECK(total <= 70000);
  // An STL file of it with the default options: a few megabytes.
  const std::string path = dir + "/dome.stl";
  mitcad::io::write_stl(path, {big}, mitcad::io::StlWriteOptions{});
  const auto size = std::filesystem::file_size(path);
  std::printf("Dome STL: %llu bytes\n", static_cast<unsigned long long>(size));
  CHECK(size < 84 + 50 * 70000);

  // The small dome: the same triangles with the limit as without it.
  MeshOptions unlimited = medium;
  unlimited.face_triangles = 0;
  const auto limited = face_triangles(mitcad::io::triangulate(small, medium));
  const auto absolute = face_triangles(mitcad::io::triangulate(small, unlimited));
  CHECK(limited == absolute);

  // Meshed in place (the 3D view's meshes, made on the model's thread):
  // every face triangulated within the bound; meshing again keeps them.
  const TopoDS_Shape shown = dome(1.0);
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  CHECK(!mitcad::io::has_triangulation(shown));
  CHECK(mitcad::io::mesh_in_place(shown, view));
  CHECK(mitcad::io::has_triangulation(shown));
  std::vector<const Poly_Triangulation*> first;
  for (TopExp_Explorer it(shown, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    const auto triangulation = BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location);
    first.push_back(triangulation.get());
  }
  for (const auto& [type, triangles] : face_triangles(shown)) {
    if (type == GeomAbs_Sphere) {
      std::printf("Dome sphere face in place: %d triangles\n", triangles);
      CHECK(triangles > 5000 && triangles <= 30000);
    }
  }
  CHECK(mitcad::io::mesh_in_place(shown, view));
  std::size_t kept = 0;
  std::size_t index = 0;
  for (TopExp_Explorer it(shown, TopAbs_FACE); it.More(); it.Next(), ++index) {
    TopLoc_Location location;
    const auto triangulation = BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location);
    kept += triangulation.get() == first[index] ? 1 : 0;
  }
  CHECK(kept == first.size());
}

// The faces' triangulations, in the shape's order.
std::vector<const Poly_Triangulation*> triangulation_pointers(const TopoDS_Shape& shape) {
  std::vector<const Poly_Triangulation*> meshes;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    meshes.push_back(BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location).get());
  }
  return meshes;
}

// A cancel pressed before the meshing starts.
class StopAtOnce : public Message_ProgressIndicator {
public:
  bool UserBreak() override { return true; }
  void Show(const Message_ProgressScope&, const bool) override {}
};

// The view's meshes of several bodies at once (mitcad#438): one meshing of
// all their faces gives each face the triangulation that meshing the bodies
// one by one gives it (the face bound included), a body's copy in another
// placement shares its faces' triangulations, a mesh body is left as it
// is, triangulations already made are kept, and a stopped range stops it.
// Counts, not times.
void test_mesh_all_in_place() {
  using mitcad::io::MeshOptions;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  const auto bodies = [] {
    return std::vector<TopoDS_Shape>{dome(1.0), dome(0.01), filleted_block(), holed_block()};
  };
  const std::vector<TopoDS_Shape> alone = bodies();
  for (const TopoDS_Shape& body : alone) {
    CHECK(mitcad::io::mesh_in_place(body, view));
  }

  std::vector<TopoDS_Shape> together = bodies();
  gp_Trsf aside;
  aside.SetTranslation(gp_Vec(0.0, 200.0, 0.0));
  const TopoDS_Shape copy = together[2].Moved(TopLoc_Location(aside));
  together.push_back(copy);
  const TopoDS_Shape plain = mitcad::io::triangulate(holed_block(), MeshOptions{});
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation> facets =
      BRep_Tool::Triangulation(TopoDS::Face(TopExp_Explorer(plain, TopAbs_FACE).Current()), location);
  const TopoDS_Shape mesh = mitcad::io::mesh_body(facets);
  together.push_back(mesh);
  CHECK(mitcad::io::mesh_all_in_place(together, view) == together.size());
  for (std::size_t i = 0; i < alone.size(); ++i) {
    CHECK(mitcad::io::has_triangulation(together[i]));
    CHECK(face_triangles(together[i]) == face_triangles(alone[i]));
  }
  for (const auto& [type, triangles] : face_triangles(together[0])) {
    if (type == GeomAbs_Sphere) {
      std::printf("Dome sphere face meshed with the others: %d triangles\n", triangles);
      CHECK(triangles > 5000 && triangles <= 30000);
    }
  }
  CHECK(mitcad::io::has_triangulation(copy));
  CHECK(triangulation_pointers(copy) == triangulation_pointers(together[2]));
  CHECK(triangulation_pointers(mesh) == std::vector<const Poly_Triangulation*>{facets.get()});

  // Meshed already: kept, and counted with the one meshed now.
  const auto first = triangulation_pointers(together[0]);
  const TopoDS_Shape fresh = filleted_block();
  CHECK(mitcad::io::mesh_all_in_place({together[0], fresh}, view) == 2);
  CHECK(triangulation_pointers(together[0]) == first);
  CHECK(face_triangles(fresh) == face_triangles(alone[2]));

  // Stopped: nothing meshed, nothing counted.
  const occ::handle<StopAtOnce> stop = new StopAtOnce();
  const std::vector<TopoDS_Shape> stopped{dome(1.0), filleted_block()};
  CHECK(mitcad::io::mesh_all_in_place(stopped, view, stop->Start()) == 0);
  CHECK(!mitcad::io::has_triangulation(stopped[0]));
  CHECK(!mitcad::io::has_triangulation(stopped[1]));

  // Nothing to mesh.
  CHECK(mitcad::io::mesh_all_in_place({}, view) == 0);
  CHECK(mitcad::io::mesh_all_in_place({TopoDS_Shape()}, view) == 0);
}

// A 60 x 40 x 5 mm plate with 24 through holes of 4 mm: two planes with
// many hole wires (mitcad#635).
TopoDS_Shape perforated_plate() {
  BRep_Builder builder;
  TopoDS_Compound holes;
  builder.MakeCompound(holes);
  for (int x = 0; x < 6; ++x) {
    for (int y = 0; y < 4; ++y) {
      builder.Add(holes, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5.0 + 10.0 * x, 5.0 + 10.0 * y, -1.0), gp::DZ()),
                                                  2.0, 7.0)
                             .Shape());
    }
  }
  return BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(60.0, 40.0, 5.0).Shape(), holes).Shape();
}

// What a meshing reports (mitcad#635): every face meshed the first time,
// kept the second; planes through Delabella (MeshOptions::fast_planes)
// have the same nodes and as many triangles as by default, faces and
// shared edges agreeing.
void test_mesh_report_and_fast_planes() {
  using mitcad::io::MeshOptions;
  using mitcad::io::MeshReport;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  const TopoDS_Shape plate = perforated_plate();
  MeshReport first;
  CHECK(mitcad::io::mesh_in_place(plate, view, Message_ProgressRange(), &first));
  CHECK(first.faces == 30);
  CHECK(first.meshed == 30);
  CHECK(first.kept == 0);
  CHECK(first.failed == 0);
  CHECK(first.total > 0.0 && first.triangulate > 0.0);
  CHECK(!first.slowest.empty() && first.slowest.front().surface == "plane" &&
        first.slowest.front().wires == 25);
  CHECK(first.line().find("faces=30 kept=0 meshed=30") == 0);
  MeshReport again;
  CHECK(mitcad::io::mesh_in_place(plate, view, Message_ProgressRange(), &again));
  CHECK(again.kept == 30 && again.meshed == 0);

  MeshOptions fast = view;
  fast.fast_planes = true;
  const TopoDS_Shape quick = perforated_plate();
  MeshReport report;
  CHECK(mitcad::io::mesh_in_place(quick, fast, Message_ProgressRange(), &report));
  CHECK(report.meshed == 30 && report.failed == 0);
  const mitcad::io::MeshStats slow = mitcad::io::mesh_stats(plate);
  const mitcad::io::MeshStats delabella = mitcad::io::mesh_stats(quick);
  std::printf("Perforated plate: %zu triangles, %zu nodes by default; %zu, %zu with fast planes\n", slow.triangles,
              slow.nodes, delabella.triangles, delabella.nodes);
  CHECK(delabella.nodes == slow.nodes);
  CHECK(delabella.triangles == slow.triangles);
  CHECK(face_triangles(quick) == face_triangles(plate));
}

// The perforated plate meshed by default and with planes through Delabella,
// compared face by face (mitcad#644): every face triangulated both ways
// and timed, the plate itself left without triangulations, planes with as
// many triangles and the same area both ways (the same boundary points),
// each face's area near its own and its deviation within its tolerance.
// A face measured by itself: a square of 10 mm in two triangles, then one
// of them turned over.
void test_compare_meshing() {
  using mitcad::io::MeshOptions;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  MeshOptions fast = view;
  fast.fast_planes = true;
  const TopoDS_Shape plate = perforated_plate();
  const mitcad::io::MeshComparison comparison = mitcad::io::compare_meshing({plate, TopoDS_Shape()}, view, fast);
  CHECK(!mitcad::io::has_triangulation(plate));
  CHECK(comparison.faces.size() == 30);
  CHECK(comparison.surfaceless == 0);
  CHECK(comparison.a.meshed == 30 && comparison.b.meshed == 30);
  CHECK(comparison.a.every_face_time.empty() && comparison.b.every_face_time.empty()); // let go
  int planes = 0;
  for (const mitcad::io::FaceMeshComparison& face : comparison.faces) {
    CHECK(face.body == 0);
    CHECK(face.a.triangulated && face.b.triangulated);
    CHECK(face.ms_a >= 0.0 && face.ms_b >= 0.0);
    CHECK(face.tolerance >= 0.01);
    CHECK(face.a.deviation <= face.tolerance && face.b.deviation <= face.tolerance);
    CHECK(face.a.folded == 0 && face.b.folded == 0);
    CHECK(relative_near(face.a.area, face.area, 1e-3));
    if (face.surface == "plane") {
      ++planes;
      CHECK(face.a.triangles == face.b.triangles);
      CHECK(relative_near(face.a.area, face.b.area, 1e-9));
      CHECK(face.a.deviation < 1e-9);
    } else {
      CHECK(face.surface == "cylinder");
      CHECK(face.a.deviation > 0.0);
    }
  }
  CHECK(planes == 6);

  BRepBuilderAPI_MakePolygon square(gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0), gp_Pnt(10, 10, 0), gp_Pnt(0, 10, 0), true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(gp_Pln(gp::XOY()), square.Wire(), true).Face();
  CHECK(!mitcad::io::face_mesh_quality(face).triangulated);
  occ::handle<Poly_Triangulation> mesh = new Poly_Triangulation(4, 2, true);
  const double corners[4][2] = {{0, 0}, {10, 0}, {10, 10}, {0, 10}};
  for (int i = 0; i < 4; ++i) {
    mesh->SetNode(i + 1, gp_Pnt(corners[i][0], corners[i][1], 0.0));
    mesh->SetUVNode(i + 1, gp_Pnt2d(corners[i][0], corners[i][1]));
  }
  mesh->SetTriangle(1, Poly_Triangle(1, 2, 3));
  mesh->SetTriangle(2, Poly_Triangle(1, 3, 4));
  BRep_Builder().UpdateFace(face, mesh);
  mitcad::io::FaceMeshQuality quality = mitcad::io::face_mesh_quality(face);
  CHECK(quality.triangulated && quality.triangles == 2 && quality.nodes == 4);
  CHECK(near(quality.area, 100.0, 1e-9));
  CHECK(quality.deviation < 1e-12 && quality.folded == 0);
  // Turned over on the first instead of beside it: a fold, and half the
  // face missing.
  mesh->SetNode(4, gp_Pnt(8, 2, 0));
  mesh->SetUVNode(4, gp_Pnt2d(8, 2));
  mesh->SetTriangle(2, Poly_Triangle(1, 4, 2));
  quality = mitcad::io::face_mesh_quality(face);
  CHECK(quality.folded == 1);
  CHECK(near(quality.area, 60.0, 1e-9));
}

// A stand-in for Delabella's planes (use_fast_plane_algorithms_for_tests,
// mitcad#644): a plane left without a triangulation, as Delabella left a
// regular quarter disc, or the default insertion's triangulation with one
// triangle more reaching 60 mm past the face, as Delabella covered a sliver
// many times over.
enum class Spoil { Nothing, Outside };

class SpoiledPlane : public IMeshTools_MeshAlgo {
public:
  SpoiledPlane(Spoil spoil, occ::handle<IMeshTools_MeshAlgo> inner) : m_spoil(spoil), m_inner(std::move(inner)) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    if (m_spoil == Spoil::Nothing) {
      return;
    }
    m_inner->Perform(face, parameters, range);
    const TopoDS_Face& shape = face->GetFace();
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> made = BRep_Tool::Triangulation(shape, location);
    if (made.IsNull() || made->NbNodes() < 2 || !made->HasUVNodes()) {
      return;
    }
    const int nodes = made->NbNodes();
    const int triangles = made->NbTriangles();
    occ::handle<Poly_Triangulation> wider = new Poly_Triangulation(nodes + 1, triangles + 1, true);
    for (int i = 1; i <= nodes; ++i) {
      wider->SetNode(i, made->Node(i));
      wider->SetUVNode(i, made->UVNode(i));
    }
    for (int i = 1; i <= triangles; ++i) {
      wider->SetTriangle(i, made->Triangle(i));
    }
    const gp_Pnt2d outside(made->UVNode(1).X() + 37.0, made->UVNode(1).Y() + 49.0);
    wider->SetUVNode(nodes + 1, outside);
    wider->SetNode(nodes + 1, BRepAdaptor_Surface(shape, false).Value(outside.X(), outside.Y()));
    wider->SetTriangle(triangles + 1, Poly_Triangle(1, 2, nodes + 1));
    wider->Deflection(made->Deflection());
    BRep_Builder().UpdateFace(shape, wider);
  }

private:
  Spoil m_spoil;
  occ::handle<IMeshTools_MeshAlgo> m_inner;
};

class SpoiledPlanes : public IMeshTools_MeshAlgoFactory {
public:
  explicit SpoiledPlanes(Spoil spoil) : m_spoil(spoil) {}

  occ::handle<IMeshTools_MeshAlgo> GetAlgo(const GeomAbs_SurfaceType type,
                                           const IMeshTools_Parameters& parameters) const override {
    return new SpoiledPlane(m_spoil, BRepMesh_MeshAlgoFactory().GetAlgo(type, parameters));
  }

private:
  Spoil m_spoil;
};

// A lens 20 mm long and 0.1 mm thick between two arcs of 1 000 mm radius:
// a sliver of a plane.
TopoDS_Shape plane_sliver() {
  const gp_Pnt a(0.0, 0.0, 0.0);
  const gp_Pnt b(20.0, 0.0, 0.0);
  const double radius = (100.0 + 0.05 * 0.05) / (2.0 * 0.05);
  const gp_Circ below(gp_Ax2(gp_Pnt(10.0, radius - 0.05, 0.0), gp::DZ()), radius);
  const gp_Circ above(gp_Ax2(gp_Pnt(10.0, 0.05 - radius, 0.0), gp::DZ()), radius);
  // Counter-clockwise round each centre: a to b along the bottom, b to a
  // along the top.
  const TopoDS_Edge bottom = BRepBuilderAPI_MakeEdge(below, a, b).Edge();
  const TopoDS_Edge top = BRepBuilderAPI_MakeEdge(above, b, a).Edge();
  return BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(bottom, top).Wire(), true).Face();
}

// A quarter disc of 5 mm radius: two lines and an arc.
TopoDS_Shape quarter_disc() {
  const gp_Pnt centre(0.0, 0.0, 0.0);
  const gp_Pnt x(5.0, 0.0, 0.0);
  const gp_Pnt y(0.0, 5.0, 0.0);
  const TopoDS_Edge arc = BRepBuilderAPI_MakeEdge(gp_Circ(gp::XOY(), 5.0), x, y).Edge();
  const TopoDS_Wire wire =
      BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(centre, x).Edge(), arc, BRepBuilderAPI_MakeEdge(y, centre).Edge())
          .Wire();
  return BRepBuilderAPI_MakeFace(wire, true).Face();
}

// Every plane of a meshed shape triangulated, its triangles covering the
// area its boundary's polygons enclose (io::plane_mesh_area), that area
// within `tolerance` of the face's own (relative); how many planes.
int planes_fit(const TopoDS_Shape& meshed, double tolerance) {
  int planes = 0;
  for (TopExp_Explorer it(meshed, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (BRepAdaptor_Surface(face, false).GetType() != GeomAbs_Plane) {
      continue;
    }
    ++planes;
    const mitcad::io::PlaneMeshArea area = mitcad::io::plane_mesh_area(face);
    GProp_GProps properties;
    BRepGProp::SurfaceProperties(face, properties);
    std::printf("Plane of %.4f mm²: boundary polygon %.4f mm², triangles %.4f mm²\n", properties.Mass(),
                area.boundary, area.triangles);
    CHECK(area.boundary > 0.0);
    CHECK(mitcad::io::plane_area_fits(area.triangles, area.boundary));
    CHECK(relative_near(area.boundary, properties.Mass(), tolerance));
  }
  return planes;
}

// The final display tier's planes (MeshOptions::check_planes, mitcad#644):
// through Delabella, a plane without a triangulation or with its area off
// its boundary's triangulated again by the default insertion. A perforated
// plate, a sliver and a quarter disc get triangulations covering their
// boundaries; the plate the default's triangles. With a stand-in for
// Delabella that leaves the planes without triangulations, or reaches past
// them, every plane is made again and gets the default's triangulation,
// counted in the report; the interactive tier (fast_planes unchecked)
// keeps the stand-in's.
void test_checked_planes() {
  using mitcad::io::MeshOptions;
  using mitcad::io::MeshReport;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  MeshOptions fast = view;
  fast.fast_planes = true;
  MeshOptions checked = fast;
  checked.check_planes = true;

  CHECK(mitcad::io::plane_area_fits(100.0, 100.09));
  CHECK(!mitcad::io::plane_area_fits(100.0, 100.2));
  CHECK(mitcad::io::plane_area_fits(0.0185, 0.018)); // 0.001 mm² at least
  CHECK(!mitcad::io::plane_area_fits(0.026, 0.018));
  CHECK(!mitcad::io::plane_area_fits(11.769, 0.018));
  CHECK(mitcad::io::plane_mesh_area(TopoDS::Face(quarter_disc())).boundary < 0.0); // not meshed

  const TopoDS_Shape reference = perforated_plate();
  CHECK(mitcad::io::mesh_in_place(reference, view));

  // Delabella, checked.
  const TopoDS_Shape plate = perforated_plate();
  MeshReport report;
  CHECK(mitcad::io::mesh_in_place(plate, checked, Message_ProgressRange(), &report));
  CHECK(report.meshed == 30 && report.failed == 0);
  CHECK(report.planes_redone == 0);
  CHECK(report.line().find("planes redone") == std::string::npos);
  CHECK(face_triangles(plate) == face_triangles(reference));
  CHECK(planes_fit(plate, 1e-2) == 6);
  // The sliver's arcs, 0.05 mm high, are polygons of a few points.
  const std::pair<TopoDS_Shape, double> shapes[] = {{plane_sliver(), 0.3}, {quarter_disc(), 1e-2}};
  for (const auto& [face, tolerance] : shapes) {
    CHECK(mitcad::io::mesh_in_place(face, checked));
    CHECK(mitcad::io::has_triangulation(face));
    CHECK(planes_fit(face, tolerance) == 1);
  }

  for (const Spoil spoil : {Spoil::Nothing, Spoil::Outside}) {
    mitcad::io::use_fast_plane_algorithms_for_tests(new SpoiledPlanes(spoil));
    // Checked: every plane made again by the default.
    const TopoDS_Shape redone = perforated_plate();
    MeshReport again;
    CHECK(mitcad::io::mesh_in_place(redone, checked, Message_ProgressRange(), &again));
    std::printf("Stand-in planes %s, checked: %s\n", spoil == Spoil::Nothing ? "missing" : "too large",
                again.line().c_str());
    CHECK(again.meshed == 30 && again.failed == 0);
    CHECK(again.planes_redone == 6);
    CHECK(again.line().find("; planes redone 6") != std::string::npos);
    CHECK(face_triangles(redone) == face_triangles(reference));
    CHECK(planes_fit(redone, 1e-2) == 6);
    const std::pair<TopoDS_Shape, double> spoilt[] = {{plane_sliver(), 0.3}, {quarter_disc(), 1e-2}};
    for (const auto& [face, tolerance] : spoilt) {
      MeshReport alone;
      CHECK(mitcad::io::mesh_in_place(face, checked, Message_ProgressRange(), &alone));
      CHECK(alone.planes_redone == 1);
      CHECK(mitcad::io::has_triangulation(face));
      CHECK(planes_fit(face, tolerance) == 1);
    }

    // Unchecked (the interactive tier): the stand-in's planes as they are.
    const TopoDS_Shape kept = perforated_plate();
    MeshReport unchecked;
    CHECK(mitcad::io::mesh_in_place(kept, fast, Message_ProgressRange(), &unchecked));
    CHECK(unchecked.planes_redone == 0);
    int spoiled = 0;
    for (TopExp_Explorer it(kept, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      if (BRepAdaptor_Surface(face, false).GetType() != GeomAbs_Plane) {
        continue;
      }
      TopLoc_Location location;
      if (spoil == Spoil::Nothing) {
        spoiled += BRep_Tool::Triangulation(face, location).IsNull() ? 1 : 0;
      } else {
        const mitcad::io::PlaneMeshArea area = mitcad::io::plane_mesh_area(face);
        spoiled += area.boundary > 0.0 && !mitcad::io::plane_area_fits(area.triangles, area.boundary) ? 1 : 0;
      }
    }
    CHECK(spoiled == 6);
  }
  mitcad::io::use_fast_plane_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());
}

// A face's triangulation given to the same face of a copy (mitcad#635):
// every face of a copy that shares the surfaces and curves takes its
// donor's, each edge with its polygons (a prism's top edges, its base's
// moved up, included), the meshing that follows keeps them all; a body
// made anew (its own surfaces and curves) takes none, and neither does a
// face that has a triangulation of its own.
void test_reuse_triangulations() {
  using mitcad::io::MeshOptions;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  BRepBuilderAPI_MakePolygon square(gp_Pnt(0, 0, 0), gp_Pnt(30, 0, 0), gp_Pnt(30, 20, 0), gp_Pnt(0, 20, 0), true);
  BRepBuilderAPI_MakeFace base(square.Wire(), true);
  BRepBuilderAPI_MakePolygon slot(gp_Pnt(5, 5, 0), gp_Pnt(5, 15, 0), gp_Pnt(25, 15, 0), gp_Pnt(25, 5, 0), true);
  base.Add(slot.Wire());
  const TopoDS_Shape prism = BRepPrimAPI_MakePrism(base.Face(), gp_Vec(0, 0, 5)).Shape();
  for (const TopoDS_Shape& donor : {perforated_plate(), prism}) {
    CHECK(mitcad::io::mesh_in_place(donor, view));
    int faces = 0;
    for (TopExp_Explorer it(donor, TopAbs_FACE); it.More(); it.Next()) {
      ++faces;
    }
    const TopoDS_Shape copy = BRepBuilderAPI_Copy(donor, false, false).Shape();
    CHECK(mitcad::io::mesh_stats(copy).triangles == 0);
    const std::size_t given = mitcad::io::reuse_triangulations(copy, {donor});
    std::printf("Reused %zu of %d faces\n", given, faces);
    CHECK(given == static_cast<std::size_t>(faces));
    CHECK(BRepTools::Triangulation(copy, 0.01, true));
    CHECK(mitcad::io::mesh_stats(copy).triangles == mitcad::io::mesh_stats(donor).triangles);
    mitcad::io::MeshReport report;
    CHECK(mitcad::io::mesh_in_place(copy, view, Message_ProgressRange(), &report));
    CHECK(report.kept == static_cast<std::size_t>(faces) && report.meshed == 0);
    CHECK(mitcad::io::reuse_triangulations(copy, {donor}) == 0);
  }
  const TopoDS_Shape plate = perforated_plate();
  const TopoDS_Shape anew = perforated_plate();
  CHECK(mitcad::io::mesh_in_place(plate, view));
  CHECK(mitcad::io::reuse_triangulations(anew, {plate}) == 0);
  CHECK(mitcad::io::reuse_triangulations(anew, {}) == 0);
}

// How many of the shape's edges have a polygon on each face's
// triangulation they bound (a seam's two count once).
int edge_polygons(const TopoDS_Shape& shape) {
  int found = 0;
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face& face = TopoDS::Face(faces.Current());
    TopLoc_Location location;
    const auto triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      continue;
    }
    for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
      if (!BRep_Tool::PolygonOnTriangulation(TopoDS::Edge(edges.Current()), triangulation, location).IsNull()) {
        ++found;
      }
    }
  }
  return found;
}

// Display triangulations kept in the result store (mitcad#858): the bytes
// of a meshed shape give a copy of it without triangulations the same
// triangles and edge polygons, which a meshing then keeps (nothing meshed
// again); a second time nothing; a shape of other faces, or bytes cut
// short, nothing at all (an error). A face met again in another placement
// is written once.
void test_display_mesh_bytes() {
  using mitcad::io::MeshOptions;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  BRep_Builder builder;
  TopoDS_Compound twice;
  builder.MakeCompound(twice);
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
  gp_Trsf aside;
  aside.SetTranslation(gp_Vec(50.0, 0.0, 0.0));
  builder.Add(twice, cylinder);
  builder.Add(twice, cylinder.Moved(TopLoc_Location(aside)));
  for (const TopoDS_Shape& meshed : {perforated_plate(), holed_block(), filleted_block(), TopoDS_Shape(twice)}) {
    CHECK(mitcad::io::mesh_in_place(meshed, view));
    const std::string bytes = mitcad::io::triangulation_bytes(meshed);
    // Taken for writing later (mitcad#880): the same bytes, also once the
    // shape's faces are meshed again (other triangulations, other edge
    // polygons) and after the shape is gone.
    {
      std::shared_ptr<const mitcad::io::TriangulationCapture> taken;
      std::string own;
      {
        const TopoDS_Shape held = BRepBuilderAPI_Copy(meshed, false, false).Shape();
        CHECK(mitcad::io::mesh_in_place(held, view));
        own = mitcad::io::triangulation_bytes(held);
        taken = mitcad::io::capture_triangulations(held);
        CHECK(mitcad::io::triangulation_bytes(*taken) == own);
        BRepTools::Clean(held);
        const MeshOptions coarse{0.5, 20.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
        CHECK(mitcad::io::mesh_in_place(held, coarse));
        CHECK(mitcad::io::triangulation_bytes(held) != own);
      }
      CHECK(!own.empty() && mitcad::io::triangulation_bytes(*taken) == own);
    }
    const TopoDS_Shape copy = BRepBuilderAPI_Copy(meshed, false, false).Shape();
    CHECK(mitcad::io::mesh_stats(copy).triangles == 0);
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
    TopExp::MapShapes(copy, TopAbs_FACE, faces);
    // Every face counts once given one, also one in another placement that
    // shares its TShape with an earlier face or (in the copy) is a copy of it.
    const std::size_t given = mitcad::io::restore_triangulations(copy, bytes);
    std::printf("Display meshes: %zu bytes gave %zu of %d faces\n", bytes.size(), given, faces.Extent());
    CHECK(given == static_cast<std::size_t>(faces.Extent()));
    CHECK(mitcad::io::has_triangulation(copy));
    CHECK(mitcad::io::mesh_stats(copy).triangles == mitcad::io::mesh_stats(meshed).triangles);
    CHECK(mitcad::io::mesh_stats(copy).nodes == mitcad::io::mesh_stats(meshed).nodes);
    CHECK(edge_polygons(copy) == edge_polygons(meshed));
    CHECK(BRepTools::Triangulation(copy, 0.01, true));
    mitcad::io::MeshReport report;
    CHECK(mitcad::io::mesh_in_place(copy, view, Message_ProgressRange(), &report));
    CHECK(report.meshed == 0);
    CHECK(mitcad::io::restore_triangulations(copy, bytes) == 0);
    // The same bytes, cut short: nothing written.
    const TopoDS_Shape again = BRepBuilderAPI_Copy(meshed, false, false).Shape();
    bool refused = false;
    try {
      mitcad::io::restore_triangulations(again, std::string_view(bytes).substr(0, bytes.size() - 5));
    } catch (const std::runtime_error&) {
      refused = true;
    }
    CHECK(refused);
    CHECK(mitcad::io::mesh_stats(again).triangles == 0);
    // From a capture of a meshed copy instead of bytes (mitcad#881): the
    // body takes the copy's triangulations and edge polygons, the same
    // handles, as if it had been meshed itself.
    {
      const TopoDS_Shape body = BRepBuilderAPI_Copy(meshed, false, false).Shape();
      const TopoDS_Shape meshedCopy = BRepBuilderAPI_Copy(body, false, false).Shape();
      CHECK(mitcad::io::mesh_in_place(meshedCopy, view));
      const auto capture = mitcad::io::capture_triangulations(meshedCopy);
      CHECK(mitcad::io::restore_triangulations(body, *capture) == static_cast<std::size_t>(count(body, TopAbs_FACE)));
      CHECK(mitcad::io::has_triangulation(body));
      CHECK(mitcad::io::mesh_stats(body).triangles == mitcad::io::mesh_stats(meshed).triangles);
      CHECK(edge_polygons(body) == edge_polygons(meshed));
      TopLoc_Location here;
      TopLoc_Location there;
      CHECK(BRep_Tool::Triangulation(TopoDS::Face(TopExp_Explorer(body, TopAbs_FACE).Current()), here) ==
            BRep_Tool::Triangulation(TopoDS::Face(TopExp_Explorer(meshedCopy, TopAbs_FACE).Current()), there));
      CHECK(mitcad::io::restore_triangulations(body, *capture) == 0);
    }
  }
  // The cylinder twice again, sharing its TShape as the meshed compound
  // does: the repeated faces take the triangulations through it.
  {
    const TopoDS_Shape fresh = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
    TopoDS_Compound shared;
    builder.MakeCompound(shared);
    builder.Add(shared, fresh);
    builder.Add(shared, fresh.Moved(TopLoc_Location(aside)));
    const std::size_t given = mitcad::io::restore_triangulations(shared, mitcad::io::triangulation_bytes(twice));
    CHECK(given == static_cast<std::size_t>(count(shared, TopAbs_FACE)));
    CHECK(mitcad::io::has_triangulation(shared));
    CHECK(mitcad::io::mesh_stats(shared).triangles == mitcad::io::mesh_stats(twice).triangles);
  }
  // Bytes of another shape: nothing written.
  const TopoDS_Shape block = holed_block();
  CHECK(mitcad::io::mesh_in_place(block, view));
  const TopoDS_Shape other = filleted_block();
  bool refused = false;
  try {
    mitcad::io::restore_triangulations(other, mitcad::io::triangulation_bytes(block));
  } catch (const std::runtime_error&) {
    refused = true;
  }
  CHECK(refused);
  CHECK(mitcad::io::mesh_stats(other).triangles == 0);
  // So too a capture of another shape (mitcad#881).
  refused = false;
  try {
    mitcad::io::restore_triangulations(other, *mitcad::io::capture_triangulations(block));
  } catch (const std::runtime_error&) {
    refused = true;
  }
  CHECK(refused);
  CHECK(mitcad::io::mesh_stats(other).triangles == 0);
}

// The most points a circle edge of the shape's cylinder faces has in their
// triangulations (0 without one).
int circle_points(const TopoDS_Shape& meshed) {
  int most = 0;
  for (TopExp_Explorer faces(meshed, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face& face = TopoDS::Face(faces.Current());
    if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Cylinder) {
      continue;
    }
    TopLoc_Location location;
    const auto triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      continue;
    }
    for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
      if (BRepAdaptor_Curve(edge).GetType() != GeomAbs_Circle) {
        continue;
      }
      const auto polygon = BRep_Tool::PolygonOnTriangulation(edge, triangulation, location);
      if (!polygon.IsNull()) {
        most = std::max(most, polygon->NbNodes());
      }
    }
  }
  return most;
}

// A stand-in for a face whose triangulation does not end (use_face_
// algorithms_for_tests, mitcad#846): a cylinder's face loops until it is
// released, never testing the stop, and then leaves a triangulation of the
// default insertion in the face it was given; other faces are triangulated
// by the default insertion and counted.
struct EndlessState {
  std::atomic<bool> entered{false};
  std::atomic<bool> release{false};
  std::atomic<bool> returned{false};
  std::atomic<int> others{0};
};

class EndlessFace : public IMeshTools_MeshAlgo {
public:
  EndlessFace(EndlessState& state, occ::handle<IMeshTools_MeshAlgo> inner, bool endless)
      : m_state(state), m_inner(std::move(inner)), m_endless(endless) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    if (!m_endless) {
      m_inner->Perform(face, parameters, range);
      ++m_state.others;
      return;
    }
    m_state.entered = true;
    while (!m_state.release) {
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    m_inner->Perform(face, parameters, Message_ProgressRange());
    m_state.returned = true;
  }

private:
  EndlessState& m_state;
  occ::handle<IMeshTools_MeshAlgo> m_inner;
  bool m_endless;
};

class EndlessCylinders : public IMeshTools_MeshAlgoFactory {
public:
  explicit EndlessCylinders(EndlessState& state) : m_state(state) {}

  occ::handle<IMeshTools_MeshAlgo> GetAlgo(const GeomAbs_SurfaceType type,
                                           const IMeshTools_Parameters& parameters) const override {
    return new EndlessFace(m_state, BRepMesh_MeshAlgoFactory().GetAlgo(type, parameters),
                           type == GeomAbs_Cylinder);
  }

private:
  EndlessState& m_state;
};

// Stops once the endless face is under way and the other faces are done.
class StopWhenStuck : public Message_ProgressIndicator {
public:
  StopWhenStuck(const EndlessState& state, int others) : m_state(state), m_others(others) {}
  bool UserBreak() override { return m_state.entered && m_state.others >= m_others; }
  void Show(const Message_ProgressScope&, const bool) override {}

private:
  const EndlessState& m_state;
  int m_others;
};

// A meshing stopped while a face's triangulation does not end (mitcad#846):
// it returns as stopped within a few seconds (kMeshAbandonSeconds after the
// stop), the face counted as abandoned, the other faces keeping their
// triangulations; the abandoned face, once it ends, writes nothing into the
// shape. Then meshings go on as before, in parallel, and a meshing without
// a stop is not affected by the stand-in's absence.
void test_abandoned_face() {
  const mitcad::io::MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  EndlessState state;
  mitcad::io::use_face_algorithms_for_tests(new EndlessCylinders(state));
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
  const occ::handle<StopWhenStuck> stop = new StopWhenStuck(state, 2);
  mitcad::io::MeshReport report;
  const std::size_t abandonedBefore = mitcad::io::abandoned_faces();
  const auto start = std::chrono::steady_clock::now();
  const bool done = mitcad::io::mesh_in_place(cylinder, view, stop->Start(), &report);
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("Endless face stopped: %.2f s; %s\n", seconds, report.line().c_str());
  CHECK(!done);
  CHECK(state.entered);
  CHECK(seconds < mitcad::io::kMeshAbandonSeconds + 3.0);
  CHECK(report.abandoned == 1);
  CHECK(report.line().find("; abandoned 1") != std::string::npos);
  // Its worker counts in the process's abandoned faces until it ends
  // (mitcad#936).
  CHECK(mitcad::io::abandoned_faces() == abandonedBefore + 1);
  int planes = 0;
  TopoDS_Face round;
  for (TopExp_Explorer it(cylinder, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    if (BRepAdaptor_Surface(face, false).GetType() == GeomAbs_Cylinder) {
      round = face;
      CHECK(BRep_Tool::Triangulation(face, location).IsNull());
    } else {
      planes += BRep_Tool::Triangulation(face, location).IsNull() ? 0 : 1;
    }
  }
  CHECK(planes == 2);
  mitcad::io::use_face_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());

  // The abandoned face ends: its triangulation stays its private copy's.
  state.release = true;
  for (int i = 0; i < 2000 && !state.returned; ++i) {
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  CHECK(state.returned);
  for (int i = 0; i < 2000 && mitcad::io::abandoned_faces() != abandonedBefore; ++i) {
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  CHECK(mitcad::io::abandoned_faces() == abandonedBefore);
  std::this_thread::sleep_for(std::chrono::milliseconds(100));
  TopLoc_Location location;
  CHECK(!round.IsNull() && BRep_Tool::Triangulation(round, location).IsNull());

  // Meshings go on: with a range that is never stopped, and without one.
  const TopoDS_Shape again = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
  const occ::handle<StopWhenStuck> never = new StopWhenStuck(state, 1000);
  CHECK(mitcad::io::mesh_in_place(again, view, never->Start()));
  CHECK(mitcad::io::has_triangulation(again));
  const TopoDS_Shape plain = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
  mitcad::io::MeshReport plainReport;
  CHECK(mitcad::io::mesh_in_place(plain, view, Message_ProgressRange(), &plainReport));
  CHECK(mitcad::io::has_triangulation(plain));
  CHECK(plainReport.abandoned == 0 && plainReport.meshed == 3);
  CHECK(face_triangles(plain) == face_triangles(again));
}

// A stand-in for a face's triangulation that throws what OCCT's own
// failures are not (use_face_algorithms_for_tests, mitcad#936): a
// std::runtime_error, or std::bad_alloc as a plain allocation that fails.
class ThrowingFace : public IMeshTools_MeshAlgo {
public:
  explicit ThrowingFace(bool allocation) : m_allocation(allocation) {}

  void Perform(const IMeshData::IFaceHandle&, const IMeshTools_Parameters&, const Message_ProgressRange&) override {
    if (m_allocation) {
      throw std::bad_alloc();
    }
    throw std::runtime_error("a face's triangulation threw");
  }

private:
  bool m_allocation;
};

// A cylinder's face through ThrowingFace, the others through the default
// insertion.
class ThrowingCylinders : public IMeshTools_MeshAlgoFactory {
public:
  explicit ThrowingCylinders(bool allocation) : m_allocation(allocation) {}

  occ::handle<IMeshTools_MeshAlgo> GetAlgo(const GeomAbs_SurfaceType type,
                                           const IMeshTools_Parameters& parameters) const override {
    if (type == GeomAbs_Cylinder) {
      return new ThrowingFace(m_allocation);
    }
    return BRepMesh_MeshAlgoFactory().GetAlgo(type, parameters);
  }

private:
  bool m_allocation;
};

// A range that is never stopped: the meshing's own workers (mesh_in_place).
class NeverStops : public Message_ProgressIndicator {
public:
  bool UserBreak() override { return false; }
  void Show(const Message_ProgressScope&, const bool) override {}
};

// How many of a cylinder's planes have a triangulation; `roundMeshed`:
// whether its round face has one.
int meshed_planes(const TopoDS_Shape& cylinder, bool& roundMeshed) {
  int planes = 0;
  roundMeshed = false;
  for (TopExp_Explorer it(cylinder, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    const bool meshed = !BRep_Tool::Triangulation(face, location).IsNull();
    if (BRepAdaptor_Surface(face, false).GetType() == GeomAbs_Cylinder) {
      roundMeshed = meshed;
    } else {
      planes += meshed ? 1 : 0;
    }
  }
  return planes;
}

// A face whose triangulation throws something other than OCCT's failures
// (mitcad#936): with a range that can be stopped (the meshing's own
// detached workers, where an escaping exception ended the process through
// std::terminate) and without one (OCCT's pool, which threw it again on
// the meshing's thread), the meshing returns with that face failed and
// without a triangulation, the other faces meshed, the process alive. An
// allocation that fails leaves the face failed too, and the meshing then
// throws "out of memory", the other faces meshed.
void test_throwing_face() {
  const mitcad::io::MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  const bool ranges[] = {true, false};
  for (const bool active : ranges) {
    const occ::handle<NeverStops> never = new NeverStops();
    mitcad::io::use_face_algorithms_for_tests(new ThrowingCylinders(false));
    const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
    mitcad::io::MeshReport report;
    bool done = false;
    bool threw = false;
    try {
      done = mitcad::io::mesh_in_place(cylinder, view, active ? never->Start() : Message_ProgressRange(), &report);
    } catch (const std::exception& error) {
      threw = true;
      std::printf("Throwing face (%s): unexpected %s\n", active ? "own workers" : "OCCT's pool", error.what());
    }
    mitcad::io::use_face_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());
    std::printf("Throwing face (%s): %s\n", active ? "own workers" : "OCCT's pool", report.line().c_str());
    CHECK(!threw);
    CHECK(done);
    CHECK(report.faces == 3 && report.failed == 1 && report.meshed == 2 && report.abandoned == 0);
    bool roundMeshed = true;
    CHECK(meshed_planes(cylinder, roundMeshed) == 2);
    CHECK(!roundMeshed);
  }
  for (const bool active : ranges) {
    const occ::handle<NeverStops> never = new NeverStops();
    mitcad::io::use_face_algorithms_for_tests(new ThrowingCylinders(true));
    const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
    std::string message;
    try {
      mitcad::io::mesh_in_place(cylinder, view, active ? never->Start() : Message_ProgressRange());
    } catch (const mitcad::io::Error& error) {
      message = error.what();
    } catch (const std::exception& error) {
      message = std::string("unexpected ") + error.what();
    }
    mitcad::io::use_face_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());
    std::printf("Face out of memory (%s): %s\n", active ? "own workers" : "OCCT's pool", message.c_str());
    CHECK(message == "triangulation: out of memory");
    bool roundMeshed = true;
    CHECK(meshed_planes(cylinder, roundMeshed) == 2);
    CHECK(!roundMeshed);
  }
  // Meshings go on as before.
  const TopoDS_Shape plain = BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape();
  CHECK(mitcad::io::mesh_in_place(plain, view));
  CHECK(mitcad::io::has_triangulation(plain));
}

// A square plate of 1 650 mm, 125 to 1 775 mm from the origin on both axes,
// less a disc of `radius` about the origin: a concave arc cut from its
// corner (mitcad#846).
TopoDS_Shape corner_cut_square(double radius) {
  const double low = 125.0;
  const double high = 1775.0;
  const double along = std::sqrt(radius * radius - low * low);
  const gp_Pnt arc_end(low, along, 0.0);
  const gp_Pnt arc_start(along, low, 0.0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(arc_end, gp_Pnt(low, high, 0.0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(low, high, 0.0), gp_Pnt(high, high, 0.0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(high, high, 0.0), gp_Pnt(high, low, 0.0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(high, low, 0.0), arc_start).Edge());
  const gp_Circ circle(gp_Ax2(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), radius);
  wire.Add(BRepBuilderAPI_MakeEdge(circle, arc_start, arc_end).Edge());
  return BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
}

// Planes whose boundary is nearly cocircular, through Delabella (OCCT port
// patch 1070, mitcad#846): a square less a disc about its corner, the arc in
// 150 to 300 points at the view's deviation. Delabella's rounded in-circle
// signs contradicted each other on such arcs, and its hull walk crashed or
// never ended: with a disc of 1 200 mm (a building's plate) on Windows,
// and with the other radii here on Linux too (a crash, then two endless
// loops). Each plane meshes in both of the view's Delabella tiers, the
// interactive one (unchecked) and the checked one, its triangles covering
// the area its boundary encloses, none made again by the default insertion.
void test_nearly_cocircular_planes() {
  using mitcad::io::MeshOptions;
  using mitcad::io::MeshReport;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  MeshOptions preview = view;
  preview.fast_planes = true;
  MeshOptions checked = preview;
  checked.check_planes = true;
  // 400 mm + 5 mm steps + 0.37 mm: the radii of a sweep that failed.
  const double radii[] = {1200.0, 400.0 + 163 * 5.0 + 0.37, 400.0 + 194 * 5.0 + 0.37, 400.0 + 227 * 5.0 + 0.37};
  for (const double radius : radii) {
    for (const MeshOptions& options : {preview, checked}) {
      const TopoDS_Shape plate = corner_cut_square(radius);
      MeshReport report;
      CHECK(mitcad::io::mesh_in_place(plate, options, Message_ProgressRange(), &report));
      std::printf("Corner cut by a disc of %.2f mm, %s: %s\n", radius,
                  options.check_planes ? "checked" : "interactive", report.line().c_str());
      CHECK(report.meshed == 1 && report.failed == 0);
      CHECK(report.planes_redone == 0);
      CHECK(planes_fit(plate, 1e-4) == 1);
    }
  }
}

// The search of BRepMesh's healer for crossing segments of a plane's wires
// (BRepMesh_FaceChecker, OCCT port patch 1081, mitcad#884) no longer
// searches wires whose boxes are apart against each other; the crossings
// it finds are the same. A 100 mm square with 64 holes apart from each
// other and from it: no crossings. With two more holes that overlap each
// other: their two edges cross, and no others. (The overlapping holes'
// centres are offset in both directions: with them on one line 2 mm apart,
// both polygons have a point where the circles cross, which is a touch and
// not a crossing.)
void test_face_checker_wire_boxes() {
  const auto circle = [](double x, double y) {
    const TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(x, y, 0.0), gp::DZ()), 2.0)).Edge();
    return BRepBuilderAPI_MakeWire(edge).Wire();
  };
  for (const bool overlapping : {false, true}) {
    BRepBuilderAPI_MakeFace maker(gp_Pln(gp::XOY()), 0.0, 100.0, 0.0, 100.0);
    for (int i = 0; i < 8; ++i) {
      for (int j = 0; j < 8; ++j) {
        maker.Add(TopoDS::Wire(circle(10.0 + 10.0 * i, 10.0 + 10.0 * j).Reversed()));
      }
    }
    std::vector<TopoDS_Edge> crossing;
    if (overlapping) {
      for (const gp_Pnt2d& centre : {gp_Pnt2d(45.0, 95.0), gp_Pnt2d(47.3, 95.4)}) {
        const TopoDS_Wire hole = circle(centre.X(), centre.Y());
        crossing.push_back(TopoDS::Edge(TopExp_Explorer(hole, TopAbs_EDGE).Current()));
        maker.Add(TopoDS::Wire(hole.Reversed()));
      }
    }
    const occ::handle<BRepMesh_Context> context = new BRepMesh_Context();
    context->SetShape(maker.Face());
    context->ChangeParameters().Deflection = 0.01;
    context->ChangeParameters().Angle = 5.0 * std::atan(1.0) / 45.0;
    context->ChangeParameters().InParallel = false;
    // What BRepMesh_IncrementalMesh fills in for those left unset.
    context->ChangeParameters().DeflectionInterior = 0.01;
    context->ChangeParameters().AngleInterior = 10.0 * std::atan(1.0) / 45.0;
    context->ChangeParameters().MinSize = 0.001;
    CHECK(context->BuildModel());
    CHECK(context->DiscretizeEdges());
    const occ::handle<IMeshData_Model>& model = context->GetModel();
    CHECK(model->FacesNb() == 1);
    CHECK(model->GetFace(0)->WiresNb() == (overlapping ? 67 : 65));
    BRepMesh_FaceChecker checker(model->GetFace(0), context->GetParameters());
    CHECK(checker.Perform() == !overlapping);
    const occ::handle<IMeshData::MapOfIEdgePtr>& found = checker.GetIntersectingEdges();
    CHECK(found->Extent() == static_cast<int>(crossing.size()));
    for (IMeshData::MapOfIEdgePtr::Iterator it(*found); it.More(); it.Next()) {
      const TopoDS_Edge& edge = it.Key()->GetEdge();
      CHECK(std::any_of(crossing.begin(), crossing.end(),
                        [&edge](const TopoDS_Edge& hole) { return hole.IsSame(edge); }));
    }
  }
}

// Building-scale cylinders and cones (mitcad#438): at the view's absolute
// 0.01 mm a turn round a radius of metres took thousands of pieces, which
// their faces' triangles follow (BRepMesh puts no points inside a
// cylinder, and a cone's grid steps round it as finely). With the face
// bound, the deviation of a turn in 500 pieces (2e-5 of the radius; about
// 700 points on a circle, BRepMesh cutting edges at half their deviation)
// for the faces and the edges that bound them; a cylinder of centimetres, and
// a small bore through the large one with its edges, mesh as without the
// bound. Meshed in place again, the triangulations are kept. Counts, not
// times.
void test_building_scale_round_faces() {
  using mitcad::io::MeshOptions;
  const MeshOptions view{0.01, 5.0 * std::atan(1.0) / 45.0, false, 20000, false, false};
  MeshOptions unlimited = view;
  unlimited.face_triangles = 0;

  // The floors: the turn's for the side of a tank of 5 m radius and for a
  // cone of 6.8 m radius at its base, none for flat ends or a small can.
  const TopoDS_Shape tank = BRepPrimAPI_MakeCylinder(5000.0, 20000.0).Shape();
  const TopoDS_Shape roof = BRepPrimAPI_MakeCone(6800.0, 0.0, 5000.0).Shape();
  const TopoDS_Shape can = BRepPrimAPI_MakeCylinder(50.0, 100.0).Shape();
  int round = 0;
  for (const TopoDS_Shape& shape : {tank, roof, can}) {
    for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      const double floor = mitcad::io::face_deviation_floor(face, 0.01, 20000);
      const GeomAbs_SurfaceType type = BRepAdaptor_Surface(face).GetType();
      if (shape.IsSame(tank) && type == GeomAbs_Cylinder) {
        CHECK(near(floor, 5000.0 * kTurnDeviation, 1e-6)); // 0.0987 mm
        ++round;
      } else if (shape.IsSame(roof) && type == GeomAbs_Cone) {
        CHECK(near(floor, 6800.0 * kTurnDeviation, 1e-6)); // 0.134 mm
        ++round;
      } else {
        CHECK(floor == 0.0);
      }
    }
  }
  CHECK(round == 2);

  // The tank and the cone in the view: far fewer triangles on the round
  // faces and on the flat ends, whose triangles are their edges' too.
  const auto tank_bounded = face_triangles(mitcad::io::triangulate(tank, view));
  const auto tank_plain = face_triangles(mitcad::io::triangulate(tank, unlimited));
  CHECK(tank_bounded.size() == tank_plain.size());
  for (std::size_t i = 0; i < tank_bounded.size() && i < tank_plain.size(); ++i) {
    std::printf("Tank face %zu: %d triangles, %d without the bound\n", i, tank_bounded[i].second,
                tank_plain[i].second);
    CHECK(tank_bounded[i].second > 0);
    CHECK(tank_bounded[i].second * 2 < tank_plain[i].second);
    if (tank_bounded[i].first == GeomAbs_Cylinder) {
      CHECK(tank_bounded[i].second <= 3000);
    }
  }
  const int tank_turn = circle_points(mitcad::io::triangulate(tank, view));
  const int tank_plain_turn = circle_points(mitcad::io::triangulate(tank, unlimited));
  std::printf("Tank's top circle: %d points, %d without the bound\n", tank_turn, tank_plain_turn);
  CHECK(tank_turn > 400 && tank_turn <= 1200);
  CHECK(tank_plain_turn >= 1500);
  const auto roof_bounded = face_triangles(mitcad::io::triangulate(roof, view));
  const auto roof_plain = face_triangles(mitcad::io::triangulate(roof, unlimited));
  CHECK(roof_bounded.size() == roof_plain.size());
  for (std::size_t i = 0; i < roof_bounded.size() && i < roof_plain.size(); ++i) {
    std::printf("Roof face %zu: %d triangles, %d without the bound\n", i, roof_bounded[i].second,
                roof_plain[i].second);
    CHECK(roof_bounded[i].second > 0);
    if (roof_bounded[i].first == GeomAbs_Cone) {
      CHECK(roof_bounded[i].second <= 40000);
      CHECK(roof_bounded[i].second * 3 < roof_plain[i].second);
    }
  }

  // Small ones: as without the bound, also a bore of 50 mm through the
  // tank, whose edges lie on the large cylinder too.
  CHECK(face_triangles(mitcad::io::triangulate(can, view)) ==
        face_triangles(mitcad::io::triangulate(can, unlimited)));
  const TopoDS_Shape bore =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-6000.0, 0.0, 10000.0), gp::DX()), 50.0, 12000.0).Shape();
  const TopoDS_Shape drilled = BRepAlgoAPI_Cut(tank, bore).Shape();
  const auto bores = [](const TopoDS_Shape& meshed) {
    std::vector<int> counts;
    for (TopExp_Explorer it(meshed, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      const BRepAdaptor_Surface surface(face);
      if (surface.GetType() == GeomAbs_Cylinder && surface.Cylinder().Radius() < 100.0) {
        TopLoc_Location location;
        const auto triangulation = BRep_Tool::Triangulation(face, location);
        counts.push_back(triangulation.IsNull() ? -1 : triangulation->NbTriangles());
      }
    }
    return counts;
  };
  const std::vector<int> bore_bounded = bores(mitcad::io::triangulate(drilled, view));
  CHECK(!bore_bounded.empty());
  CHECK(bore_bounded == bores(mitcad::io::triangulate(drilled, unlimited)));

  // Meshed in place twice (the view's meshes on the model's thread, then
  // the view's own check): the coarser edges meet the bound again, and
  // every triangulation is kept.
  const std::vector<TopoDS_Shape> shown{BRepPrimAPI_MakeCylinder(5000.0, 20000.0).Shape(),
                                        BRepPrimAPI_MakeCone(6800.0, 0.0, 5000.0).Shape(),
                                        BRepAlgoAPI_Cut(tank, bore).Shape()};
  for (const TopoDS_Shape& made : shown) {
    CHECK(mitcad::io::mesh_in_place(made, view));
    CHECK(mitcad::io::has_triangulation(made));
    const auto first = triangulation_pointers(made);
    CHECK(mitcad::io::mesh_in_place(made, view));
    CHECK(triangulation_pointers(made) == first);
  }
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

// The shaded meshes of USDZ export (mitcad#123): a block per face in the
// shapes' face order, unit normals out of the body that agree with the
// winding of each triangle that has an area (a sphere's poles have
// collapsed ones), the surface's own normals on curved faces; a mesh body
// faceted.
void test_face_meshes(const std::string& dir) {
  using mitcad::io::FaceMeshes;
  using mitcad::io::MeshOptions;
  using mitcad::io::Refinement;
  const auto consistent = [](const FaceMeshes& mesh) {
    bool ok = mesh.vertices.size() == mesh.normals.size();
    std::size_t total = 0;
    for (const std::uint32_t count : mesh.face_triangles) {
      total += count;
    }
    ok = ok && total == mesh.triangles.size();
    for (const auto& n : mesh.normals) {
      ok = ok && std::abs(std::sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]) - 1.0) < 1e-9;
    }
    for (const auto& t : mesh.triangles) {
      const auto& a = mesh.vertices[t[0]];
      const auto& b = mesh.vertices[t[1]];
      const auto& c = mesh.vertices[t[2]];
      const gp_Vec facet = gp_Vec(gp_Pnt(a[0], a[1], a[2]), gp_Pnt(b[0], b[1], b[2]))
                               .Crossed(gp_Vec(gp_Pnt(a[0], a[1], a[2]), gp_Pnt(c[0], c[1], c[2])));
      const double length = facet.Magnitude();
      for (const std::uint32_t corner : t) {
        const auto& n = mesh.normals[corner];
        if (length > 1e-12 && facet.Dot(gp_Vec(n[0], n[1], n[2])) <= 0.0) {
          std::fprintf(stderr, "face meshes: a normal %g %g %g against its triangle's %g %g %g\n", n[0], n[1], n[2],
                       facet.X() / length, facet.Y() / length, facet.Z() / length);
          ok = false;
        }
      }
    }
    return ok;
  };
  const MeshOptions options = MeshOptions::from(Refinement::Medium);
  // A box: six faces of two triangles and four vertices each, normals along
  // the axes.
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(10.0, 20.0, 30.0).Shape();
  const FaceMeshes boxed = mitcad::io::face_meshes(box, options);
  CHECK(boxed.face_triangles == std::vector<std::uint32_t>(6, 2));
  CHECK(boxed.vertices.size() == 24);
  CHECK(consistent(boxed));
  for (const auto& n : boxed.normals) {
    CHECK(std::abs(std::abs(n[0]) + std::abs(n[1]) + std::abs(n[2]) - 1.0) < 1e-12);
  }
  // A sphere: the surface's normals, outwards.
  const gp_Pnt center(0.0, 0.0, 50.0);
  const TopoDS_Shape sphere = BRepPrimAPI_MakeSphere(center, 15.0).Shape();
  const FaceMeshes round = mitcad::io::face_meshes(sphere, options);
  CHECK(consistent(round));
  double worst = 1.0;
  for (std::size_t i = 0; i < round.vertices.size(); ++i) {
    const auto& p = round.vertices[i];
    const auto& n = round.normals[i];
    const gp_Vec radial = gp_Vec(center, gp_Pnt(p[0], p[1], p[2])).Normalized();
    worst = std::min(worst, radial.Dot(gp_Vec(n[0], n[1], n[2])));
  }
  std::fprintf(stderr, "face meshes: sphere %zu triangles, worst normal cosine %.9f\n", round.triangles.size(),
               worst);
  CHECK(worst > 0.999);
  // Reversed faces (the hole's), fillets: every face, in the shape's order.
  for (const TopoDS_Shape& shape : {holed_block(), filleted_block()}) {
    const FaceMeshes mesh = mitcad::io::face_meshes(shape, options);
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    CHECK(mesh.face_triangles.size() == static_cast<std::size_t>(faces.Extent()));
    CHECK(consistent(mesh));
    CHECK(mitcad::io::mesh_stats(shape).triangles == 0);
  }
  // A mesh body: faceted, three vertices per triangle.
  const std::string path = dir + "/faces.stl";
  mitcad::io::write_stl(path, {box}, {mitcad::io::StlFormat::Binary, options});
  const FaceMeshes faceted = mitcad::io::face_meshes(mitcad::io::read_stl(path).shape, options);
  CHECK(faceted.face_triangles == std::vector<std::uint32_t>{12});
  CHECK(faceted.vertices.size() == 36);
  CHECK(consistent(faceted));
}

void test_obj(const std::string& dir) {
  const std::vector<Body> bodies = sample_bodies();
  const std::string path = dir + "/bodies.obj";
  const mitcad::io::MeshOptions mesh = mitcad::io::MeshOptions::from(mitcad::io::Refinement::Low);
  mitcad::io::write_obj(path, bodies, {mesh, mitcad::io::LengthUnit::Millimeter});
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
  // In centimetres (mitcad#734): a tenth of the numbers, the same size read
  // back at 10 mm per unit.
  const std::string centimetres = dir + "/bodies-cm.obj";
  mitcad::io::write_obj(centimetres, bodies, {mesh, mitcad::io::LengthUnit::Centimeter});
  const std::vector<Body> plain = mitcad::io::read_obj(centimetres);
  const std::vector<Body> sized = mitcad::io::read_obj(centimetres, {10.0});
  CHECK(plain.size() == 2 && sized.size() == 2);
  if (plain.size() == 2 && sized.size() == 2 && read.size() == 2) {
    CHECK(near(bbox(plain[1].shape).max[0], bbox(read[1].shape).max[0] / 10.0, 1e-4));
    CHECK(same_box(bbox(sized[1].shape), bbox(read[1].shape), 1e-3));
  }
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
  mitcad::io::write_obj(obj, bodies, {mesh, mitcad::io::LengthUnit::Millimeter});
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
  mitcad::io::write_obj(mesh_obj, {scan}, {mesh, mitcad::io::LengthUnit::Millimeter});
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
    test_building_scale_sphere(dir);
    test_mesh_all_in_place();
    test_mesh_report_and_fast_planes();
    test_reuse_triangulations();
    test_display_mesh_bytes();
    test_compare_meshing();
    test_checked_planes();
    test_abandoned_face();
    test_throwing_face();
    test_nearly_cocircular_planes();
    test_face_checker_wire_boxes();
    test_building_scale_round_faces();
    test_stl(dir);
    test_indexed_mesh(dir);
    test_face_meshes(dir);
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
