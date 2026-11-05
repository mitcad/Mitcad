// SPDX-License-Identifier: MIT
// A chamfer of a concave edge wider than the narrow face beside it
// (mitcad#1260): an octagon (flats at +-40 mm, 10 mm high, its vertical
// corners rounded R2) standing on a disc R44 x 2 mm, the octagon's foot (the
// concave edge loop between its sides and the disc's top) chamfered with an
// equal distance along its tangent chain. The disc's top ring is 4 mm wide
// at the flats and 0.87 mm at the corners, so the chamfer runs over the
// ring there. The chamfer adds material: where it runs past the ring's far
// edge it is cut by the face beyond, the disc's rim cylinder, never left
// hanging out past the rim. At 4 mm the chamfer's contact meets the rim
// exactly at the flats' middles: it fails with a message that says where.
// The check of mitcad#1253 warns of such a chamfer hanging out past the rim
// (made by hand here: the chamfer on a larger disc, the disc cut back).
//
// The expected numbers are those of the reference results (MITCAD_GEOMETRY_CASES,
// case `chamfer-overflow-concave`, STEP files; built-in when it is not set).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <filesystem>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "../src/dressup_check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/io/step.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kFlat = 40.0;       // the octagon's flats' distance from the axis
constexpr double kCornerRadius = 2.0; // its vertical corners' rounding
constexpr double kDiscRadius = 44.0;
constexpr double kDiscTop = 2.0;
constexpr double kTop = 12.0;

double volume_of(const TopoDS_Shape& shape) { return mitcad::analysis::physical_properties(shape).volume; }

// The body with its faces named N1, N2, ... in TopExp order.
ShapePtr named(const TopoDS_Shape& body) {
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"N" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(body, faces);
}

// The octagon on the disc (of radius 44, or `radius`).
TopoDS_Shape octagon_on_disc(double radius = kDiscRadius) {
  BRepBuilderAPI_MakePolygon polygon;
  const double circumradius = kFlat / std::cos(kPi / 8);
  for (int k = 0; k < 8; ++k) {
    const double a = kPi / 8 + k * kPi / 4;
    polygon.Add(gp_Pnt(circumradius * std::cos(a), circumradius * std::sin(a), kDiscTop));
  }
  polygon.Close();
  const TopoDS_Face base = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  const TopoDS_Shape prism = BRepPrimAPI_MakePrism(base, gp_Vec(0, 0, kTop - kDiscTop)).Shape();
  BRepFilletAPI_MakeFillet corners(prism);
  for (TopExp_Explorer ex(prism, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.Z() - b.Z()) > 1.0) {
      corners.Add(kCornerRadius, edge);
    }
  }
  const TopoDS_Shape octagon = corners.Shape();
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(radius, kDiscTop).Shape();
  return BRepAlgoAPI_Fuse(disc, octagon).Shape();
}

// The faces on the disc's top plane facing up: the ring's pieces.
int ring_pieces(const TopoDS_Shape& shape) {
  int pieces = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    gp_Dir normal = plane.Axis().Direction();
    if (face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    if (std::abs(plane.Location().Z() - kDiscTop) < 1e-6 && normal.Z() > 1.0 - 1e-9 &&
        std::abs(plane.Axis().Direction().Z()) > 1.0 - 1e-9) {
      ++pieces;
    }
  }
  return pieces;
}

// How far from the axis the shape's vertices reach.
double vertex_radius(const TopoDS_Shape& shape) {
  double largest = 0.0;
  for (TopExp_Explorer ex(shape, TopAbs_VERTEX); ex.More(); ex.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
    largest = std::max(largest, std::hypot(p.X(), p.Y()));
  }
  return largest;
}

// How far from the axis points along the shape's edges reach (their
// curves: an intersection's approximation lies within its tolerance).
double largest_radius(const TopoDS_Shape& shape) {
  double largest = 0.0;
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      continue;
    }
    for (int k = 0; k <= 16; ++k) {
      const gp_Pnt p = curve->Value(first + (last - first) * k / 16);
      largest = std::max(largest, std::hypot(p.X(), p.Y()));
    }
  }
  return largest;
}

// A reference result's numbers.
struct Reference {
  double size;
  const char* file;
  double volume;
  int pieces; // the disc's top ring's pieces
};

// The folder of geometry cases kept outside the repository (empty when
// MITCAD_GEOMETRY_CASES is not set).
std::string geometry_cases_root() {
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

// With MITCAD_TEST_BREP_DIR set to a folder, the results are written there
// as BRep files (to look at them).
void dump(const TopoDS_Shape& shape, const std::string& name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  std::string folder;
  if (_dupenv_s(&value, &length, "MITCAD_TEST_BREP_DIR") == 0 && value != nullptr) {
    folder = value;
    std::free(value);
  }
#else
  const char* value = std::getenv("MITCAD_TEST_BREP_DIR");
  const std::string folder = value != nullptr ? std::string(value) : std::string();
#endif
  if (!folder.empty()) {
    BRepTools::Write(shape, (std::filesystem::path(folder) / (name + ".brep")).string().c_str());
    // (And the part about the corner at -22.5 degrees, to look at closely.)
    const double a = -kPi / 8;
    const double r = kDiscRadius - 1.0;
    const gp_Pnt low(r * std::cos(a) - 8.0, r * std::sin(a) - 8.0, 0.0);
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(low, 16.0, 16.0, 8.0).Shape();
    BRepTools::Write(BRepAlgoAPI_Common(shape, box).Shape(),
                     (std::filesystem::path(folder) / (name + "-corner.brep")).string().c_str());
  }
}

std::string cases_folder() {
  const std::string root = geometry_cases_root();
  if (root.empty()) {
    return {};
  }
  const std::filesystem::path folder = std::filesystem::path(root) / "chamfer-overflow-concave";
  return std::filesystem::is_directory(folder) ? folder.string() : std::string();
}

// The reference's volume and ring pieces from its STEP file where the
// cases' folder has it (else the built-in numbers).
Reference read_reference(Reference built_in, const std::string& folder) {
  if (folder.empty()) {
    return built_in;
  }
  const std::filesystem::path path = std::filesystem::path(folder) / built_in.file;
  if (!std::filesystem::exists(path)) {
    return built_in;
  }
  const auto bodies = mitcad::io::read_step(path.string());
  CHECK(bodies.size() == 1);
  if (bodies.size() != 1) {
    return built_in;
  }
  Reference out = built_in;
  out.volume = volume_of(bodies.front().shape);
  out.pieces = ring_pieces(bodies.front().shape);
  CHECK(vertex_radius(bodies.front().shape) <= kDiscRadius + 1e-6);
  std::printf("reference %s: vertices %.9f, edges %.9f from the axis at most\n", built_in.file,
              vertex_radius(bodies.front().shape), largest_radius(bodies.front().shape));
  std::printf("reference %s: volume %.3f, ring pieces %d\n", built_in.file, out.volume, out.pieces);
  return out;
}

ChamferSet foot_chamfer(const Shape& body, double size) {
  ChamferSet set;
  for (int i = 0; i < body.edge_count(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(body.edge(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (!curve.IsNull() && curve->Value(0.5 * (first + last)).Distance(gp_Pnt(kFlat, 0, kDiscTop)) < 1e-6) {
      set.edges.push_back(body.edge_name(i));
    }
  }
  CHECK(set.edges.size() == 1);
  set.spec.type = ChamferType::EqualDistance;
  set.spec.distance = size;
  set.tangent_chain = true;
  return set;
}

void test_octagon_foot() {
  const ShapePtr body = named(octagon_on_disc());
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  CHECK(body->face_count() == 20);
  const std::string folder = cases_folder();
  if (folder.empty()) {
    std::printf("octagon foot: MITCAD_GEOMETRY_CASES has no chamfer-overflow-concave, built-in numbers\n");
  }
  const Reference none = read_reference({0.0, "octagon-on-disc-0mm.step", 65176.698, 1}, folder);
  CHECK_NEAR_TOL(volume_of(body->occt()), none.volume, 1e-6);
  CHECK(ring_pieces(body->occt()) == 1);
  const std::vector<Reference> references = {
      {1.0, "octagon-on-disc-chamfer-1mm.step", 65309.892, 8},
      {2.0, "octagon-on-disc-chamfer-2mm.step", 65696.720, 8},
      {3.0, "octagon-on-disc-chamfer-3mm.step", 66292.217, 8},
      {3.9, "octagon-on-disc-chamfer-3_9mm.step", 66955.211, 8},
  };
  for (const Reference& built_in : references) {
    const Reference reference = read_reference(built_in, folder);
    ShapePtr result;
    try {
      result = chamfer("C", *body, {foot_chamfer(*body, reference.size)});
    } catch (const std::exception& error) {
      std::fprintf(stderr, "octagon foot %g mm: %s\n", reference.size, error.what());
      CHECK(false);
      continue;
    }
    dump(result->occt(), "octagon-foot-" + std::to_string(static_cast<int>(std::lround(reference.size * 10))));
    const double volume = volume_of(result->occt());
    const double radius = vertex_radius(result->occt());
    const double along_edges = largest_radius(result->occt());
    const int pieces = ring_pieces(result->occt());
    std::printf("octagon foot %g mm: valid %d, volume %.3f (reference %.3f, %+.4f %%), faces %d, "
                "vertices %.9f and edges %.9f from the axis at most, ring pieces %d (reference %d)\n",
                reference.size, BRepCheck_Analyzer(result->occt()).IsValid() ? 1 : 0, volume, reference.volume,
                100.0 * (volume - reference.volume) / reference.volume, result->face_count(), radius, along_edges,
                pieces, reference.pieces);
    for (const std::string& note : result->notes()) {
      std::printf("  note: %s\n", note.c_str());
    }
    CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
    CHECK_NEAR_TOL(volume, reference.volume, 1e-3);
    CHECK(radius <= kDiscRadius + 1e-6);
    CHECK(along_edges <= kDiscRadius + 1e-5);
    CHECK(pieces == reference.pieces);
    CHECK(result->notes().empty());
  }
  // The exact tangent case: the chamfer's contact meets the rim at the
  // flats' middles. A message that names the edge and the faces.
  try {
    const ShapePtr result = chamfer("C", *body, {foot_chamfer(*body, 4.0)});
    std::fprintf(stderr, "octagon foot 4 mm: built (volume %.3f, largest radius %.6f)\n", volume_of(result->occt()),
                 largest_radius(result->occt()));
    CHECK(false);
  } catch (const std::exception& error) {
    const std::string message = error.what();
    std::printf("octagon foot 4 mm: %s\n", message.c_str());
    CHECK(message.find("edge E{") != std::string::npos);
    CHECK(message.find("reaches exactly to the far edge of face N") != std::string::npos);
    CHECK(message.find("at (44, 0, 2)") != std::string::npos);
  }
}

// Whether the face lies on the surface: its vertices and the middle of its
// parameters within 1e-7.
bool lies_on(const TopoDS_Face& face, const TopoDS_Face& other) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(other);
  std::vector<gp_Pnt> points;
  for (TopExp_Explorer v(face, TopAbs_VERTEX); v.More(); v.Next()) {
    points.push_back(BRep_Tool::Pnt(TopoDS::Vertex(v.Current())));
  }
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  points.push_back(BRepAdaptor_Surface(face).Value(0.5 * (u0 + u1), 0.5 * (v0 + v1)));
  for (const gp_Pnt& p : points) {
    GeomAPI_ProjectPointOnSurf on(p, surface);
    if (on.NbPoints() == 0 || on.LowerDistance() > 1e-7) {
      return false;
    }
  }
  return true;
}

// The check of #1253 on a chamfer hanging out past the disc's rim, as one
// that overflowed the ring was built before (mitcad#1260): the octagon's
// foot chamfered 3 on a disc of 50, the disc then cut back to 44 under it.
// Its faces are named after those of the chamfer on the larger disc or of
// the body on the disc of 44 that they lie on.
void test_warning_on_an_eave() {
  using detail::DressupSide;
  const ShapePtr input = named(octagon_on_disc());
  const ShapePtr wide = named(octagon_on_disc(50.0));
  const ShapePtr chamfered = chamfer("C", *wide, {foot_chamfer(*wide, 3.0)});
  CHECK(chamfered->notes().empty());
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 60.0, 3.0).Shape();
  const TopoDS_Shape inner =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -2), gp_Dir(0, 0, 1)), kDiscRadius, 5.0).Shape();
  const TopoDS_Shape annulus = BRepAlgoAPI_Cut(outer, inner).Shape();
  const TopoDS_Shape eave = BRepAlgoAPI_Cut(chamfered->occt(), annulus).Shape();
  dump(eave, "eave-30");
  CHECK(BRepCheck_Analyzer(eave).IsValid());
  CHECK(vertex_radius(eave) > kDiscRadius + 1.0);
  std::vector<Shape::NamedFace> faces;
  int unnamed = 0;
  for (TopExp_Explorer f(eave, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    NameList names;
    for (const Shape* from : {chamfered.get(), input.get()}) {
      for (int k = 0; k < from->face_count() && names.empty(); ++k) {
        if (lies_on(face, from->face(k))) {
          names = from->face_names(k);
        }
      }
    }
    if (names.empty()) {
      names = {"X" + std::to_string(++unnamed)};
    }
    faces.push_back({face, names});
  }
  const ShapePtr result = std::make_shared<Shape>(eave, faces);
  // (The chamfer's faces name edges of the body on the disc of 44.)
  int edges = 0;
  for (int f = 0; f < result->face_count(); ++f) {
    const std::string& name = result->face_names(f).front();
    if (name.rfind("C:chamfer(", 0) == 0) {
      CHECK(input->find_edges(name.substr(10, name.find(')') - 10)).size() == 1);
      ++edges;
    }
  }
  CHECK(edges >= 8);
  const std::string warning = detail::dressup_warning(*input, *result, "C", "chamfer", DressupSide::Outside, 3.0);
  std::printf("eave: %s\n", warning.c_str());
  CHECK(warning.find("mm past the body's original extent, where a chamfer of concave edges only fills the corner") !=
        std::string::npos);
  CHECK(warning.find("is turned over on its surface (it faces into the body)") != std::string::npos);
  // The chamfer as built on the disc of 44: none.
  const ShapePtr built = chamfer("C", *input, {foot_chamfer(*input, 3.0)});
  CHECK(detail::dressup_warning(*input, *built, "C", "chamfer", DressupSide::Outside, 3.0).empty());
  // On the disc of 50, nothing hangs out.
  CHECK(detail::dressup_warning(*wide, *chamfered, "C", "chamfer", DressupSide::Outside, 3.0).empty());
}

} // namespace
} // namespace test

int main() {
  test::guarded("octagon foot", test::test_octagon_foot);
  test::guarded("warning on an eave", test::test_warning_on_an_eave);
  if (test::failures == 0) {
    std::puts("test_chamfer_overflow_concave: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
