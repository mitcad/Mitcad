// SPDX-License-Identifier: MIT
// Patch, Stitch and Thicken (mitcad#456): sketch regions patched into
// surface bodies, an open box of five patches stitched into one surface,
// its opening patched and the lid stitched on into a solid, a loop of a
// solid's named edges patched, a fill through a point, faces sewn only
// within the tolerance, and surfaces thickened to one side, the other and
// both, with the names of every face; the opening also as its free edges.
// Every operation leaves its inputs as they are (the input check is on).

#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <BRepBndLib.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "check.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/surfaces.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

double area(const g::Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape.occt(), props);
  return props.Mass();
}

double shape_volume(const g::Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.Mass();
}

bool solid(const g::Shape& shape) { return TopExp_Explorer(shape.occt(), TopAbs_SOLID).More(); }

// The z range of the shape's box.
std::pair<double, double> z_range(const g::Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape.occt(), box, false, false);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  return {z0, z1};
}

int named(const g::Shape& shape, const std::string& name) {
  return static_cast<int>(shape.find_faces(name).size());
}

// Every face's names, for a failed check.
void print_names(const g::Shape& shape) {
  for (int i = 0; i < shape.face_count(); ++i) {
    std::string names;
    for (const std::string& name : shape.face_names(i)) {
      names += " " + name;
    }
    std::fprintf(stderr, "  face %d:%s\n", i, names.c_str());
  }
}

g::Frame frame(gp_Pnt origin, gp_Dir x, gp_Dir y) {
  g::Frame f;
  f.origin = origin;
  f.x_axis = x;
  f.y_axis = y;
  return f;
}

g::Region region(const std::string& name, double x, double y, double width, double height) {
  g::Region r = g::rectangle_region(x, y, width, height);
  r.name = name;
  return r;
}

g::ShapePtr patch(const std::string& name, const g::Frame& f, double width, double height) {
  const std::vector<g::ShapePtr> patches = g::patch_regions("F2", f, {region(name, 0, 0, width, height)});
  CHECK(patches.size() == 1);
  return patches.front();
}

// The bottom and the four walls of a 20 mm cube open at the top (z = 20),
// each its own surface body.
std::vector<g::ShapePtr> open_box() {
  const gp_Dir x(1, 0, 0), y(0, 1, 0), z(0, 0, 1);
  return {patch("bottom", frame(gp_Pnt(0, 0, 0), x, y), 20, 20),
          patch("x0", frame(gp_Pnt(0, 0, 0), y, z), 20, 20),
          patch("x20", frame(gp_Pnt(20, 0, 0), y, z), 20, 20),
          patch("y0", frame(gp_Pnt(0, 0, 0), x, z), 20, 20),
          patch("y20", frame(gp_Pnt(0, 20, 0), x, z), 20, 20)};
}

std::vector<const g::Shape*> pointers(const std::vector<g::ShapePtr>& shapes) {
  std::vector<const g::Shape*> result;
  for (const auto& s : shapes) {
    result.push_back(s.get());
  }
  return result;
}

void test_regions_are_patched_into_surface_bodies() {
  const std::vector<g::ShapePtr> patches = g::patch_regions(
      "F2", g::Frame{}, {region("r{c1}", 0, 0, 20, 10), region("r{c5}", 40, 0, 10, 10)});
  CHECK(patches.size() == 2);
  CHECK(!solid(*patches[0]));
  CHECK_NEAR(area(*patches[0]), 200.0);
  CHECK_NEAR(area(*patches[1]), 100.0);
  CHECK(named(*patches[0], "F2:patch(r{c1})") == 1);
  CHECK(named(*patches[1], "F2:patch(r{c5})") == 1);
  // One closed opening, its outline.
  const std::vector<g::OpenBoundary> openings = g::open_boundaries(*patches[0]);
  CHECK(openings.size() == 1);
  if (!openings.empty()) {
    CHECK(openings[0].closed);
    CHECK(openings[0].faces == g::NameList{"F2:patch(r{c1})"});
    CHECK(openings[0].points.size() >= 4 * 40);
    for (const gp_Pnt& p : openings[0].points) {
      const bool on_outline = std::abs(p.X()) < 1e-9 || std::abs(p.X() - 20) < 1e-9 ||
                              std::abs(p.Y()) < 1e-9 || std::abs(p.Y() - 10) < 1e-9;
      CHECK(on_outline && std::abs(p.Z()) < 1e-9);
    }
  }
}

void test_an_open_box_is_stitched_patched_and_closed() {
  const std::vector<g::ShapePtr> sides = open_box();
  const g::ShapePtr tray = g::stitch(pointers(sides), 0.0);
  CHECK(!solid(*tray));
  CHECK_NEAR(area(*tray), 5 * 400.0);
  CHECK(named(*tray, "F2:patch(bottom)") == 1);
  CHECK(named(*tray, "F2:patch(y20)") == 1);
  // The opening at the top runs along the four walls.
  const std::vector<g::OpenBoundary> openings = g::open_boundaries(*tray);
  CHECK(openings.size() == 1);
  if (openings.empty()) {
    return;
  }
  CHECK(openings[0].closed);
  CHECK((openings[0].faces ==
         g::NameList{"F2:patch(x0)", "F2:patch(x20)", "F2:patch(y0)", "F2:patch(y20)"}));
  for (const gp_Pnt& p : openings[0].points) {
    CHECK(std::abs(p.Z() - 20) < 1e-6);
  }
  // The same opening as its free edges (mitcad#1927: the view picks one of
  // them as the opening): the four top edges, each of one wall only.
  const std::vector<g::OpenBoundaryEdges> free = g::open_boundary_edges(*tray);
  CHECK(free.size() == 1);
  if (!free.empty()) {
    CHECK(free[0].faces == openings[0].faces);
    CHECK(free[0].edges.size() == 4);
    for (const int e : free[0].edges) {
      CHECK(tray->edge_name(e).empty());
      BRepAdaptor_Curve curve(tray->edge(e));
      CHECK(std::abs(curve.Value(curve.FirstParameter()).Z() - 20) < 1e-6);
      CHECK(std::abs(curve.Value(curve.LastParameter()).Z() - 20) < 1e-6);
    }
  }
  g::PatchLoop opening;
  opening.opening = 0;
  const g::ShapePtr lid = g::patch_loop("F7", *tray, opening);
  CHECK(!solid(*lid));
  CHECK_NEAR(area(*lid), 400.0);
  CHECK(named(*lid, "F7:patch") == 1);
  const auto [low, high] = z_range(*lid);
  CHECK(std::abs(low - 20) < 1e-6 && std::abs(high - 20) < 1e-6);
  // The box closes into a solid; every face keeps its name.
  const g::ShapePtr box = g::stitch({tray.get(), lid.get()}, 0.001);
  CHECK(solid(*box));
  CHECK_NEAR(shape_volume(*box), 8000.0);
  CHECK(named(*box, "F7:patch") == 1);
  CHECK(named(*box, "F2:patch(x0)") == 1);
  CHECK(g::open_boundaries(*box).empty());

  // A loop of the solid's named edges: the lid's again.
  g::PatchLoop edges;
  for (const char* wall : {"F2:patch(x0)", "F2:patch(x20)", "F2:patch(y0)", "F2:patch(y20)"}) {
    edges.edges.push_back(g::make_edge_name(wall, "F7:patch"));
  }
  const g::ShapePtr top = g::patch_loop("F9", *box, edges);
  CHECK_NEAR(area(*top), 400.0);
  CHECK(named(*top, "F9:patch") == 1);
  // An edge between two faces is only passed through.
  edges.continuity = g::Continuity::G1;
  CHECK(throws_with([&] { g::patch_loop("F9", *box, edges); }, "lies between two faces"));
  // Edges that do not close up.
  g::PatchLoop open;
  open.edges = {edges.edges[0], edges.edges[1]};
  CHECK(throws_with([&] { g::patch_loop("F9", *box, open); }, "do not close up"));
  // A solid is not thickened.
  CHECK(throws_with([&] { g::thicken("F9", *box, 1.0, false); }, "is a solid"));
}

void test_a_fill_passes_through_its_points() {
  const g::ShapePtr tray = g::stitch(pointers(open_box()), 0.0);
  g::PatchLoop dome;
  dome.opening = 0;
  dome.points = {gp_Pnt(10, 10, 24)};
  const g::ShapePtr lid = g::patch_loop("F7", *tray, dome);
  CHECK(named(*lid, "F7:patch") == 1);
  TopExp_Explorer it(lid->occt(), TopAbs_FACE);
  CHECK(it.More());
  if (!it.More()) {
    return;
  }
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(TopoDS::Face(it.Current()));
  GeomAPI_ProjectPointOnSurf projection(gp_Pnt(10, 10, 24), surface);
  CHECK(projection.NbPoints() > 0 && projection.LowerDistance() < 1e-2);
  const auto [low, high] = z_range(*lid);
  CHECK(low > 20 - 1e-3 && high > 23.5);
}

void test_faces_are_sewn_within_the_tolerance() {
  // Two plates 0.01 apart.
  const g::ShapePtr a = patch("a", g::Frame{}, 10, 10);
  const g::ShapePtr b = patch("b", frame(gp_Pnt(10.01, 0, 0), gp_Dir(1, 0, 0), gp_Dir(0, 1, 0)), 10, 10);
  CHECK(g::open_boundaries(*g::stitch({a.get(), b.get()}, 0.001)).size() == 2);
  const g::ShapePtr sewn = g::stitch({a.get(), b.get()}, 0.05);
  CHECK(g::open_boundaries(*sewn).size() == 1);
  CHECK(named(*sewn, "F2:patch(a)") == 1 && named(*sewn, "F2:patch(b)") == 1);
}

void test_surfaces_are_thickened() {
  const g::ShapePtr plate = patch("r", g::Frame{}, 20, 10);
  const std::string face = "F2:patch(r)";
  struct Case {
    double thickness;
    bool symmetric;
    double low, high;
  };
  for (const Case c : {Case{2, false, 0, 2}, Case{-2, false, -2, 0}, Case{3, true, -1.5, 1.5}}) {
    const g::ShapePtr thick = g::thicken("F3", *plate, c.thickness, c.symmetric);
    CHECK(solid(*thick));
    CHECK_NEAR(shape_volume(*thick), 200.0 * std::abs(c.thickness));
    const auto [low, high] = z_range(*thick);
    CHECK(std::abs(low - c.low) < 1e-6 && std::abs(high - c.high) < 1e-6);
    CHECK(named(*thick, "F3:front(" + face + ")") == 1);
    CHECK(named(*thick, "F3:back(" + face + ")") == 1);
    CHECK(named(*thick, "F3:wall(" + face + ")") == 4);
    if (named(*thick, "F3:wall(" + face + ")") != 4) {
      print_names(*thick);
    }
    // The front is the side the face's normal (+z) points to.
    const std::vector<int> front = thick->find_faces("F3:front(" + face + ")");
    if (!front.empty()) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(thick->face(front.front()), props);
      CHECK(std::abs(props.CentreOfMass().Z() - c.high) < 1e-6);
    }
  }
  // The open box's surface (five faces) thickened outwards... or inwards:
  // its faces' normals are their sketches', so only the volume is fixed.
  const g::ShapePtr tray = g::stitch(pointers(open_box()), 0.0);
  const g::ShapePtr walls = g::thicken("F4", *tray, 1.0, true);
  CHECK(solid(*walls));
  CHECK(shape_volume(*walls) > 5 * 400.0 * 0.9);
  CHECK(throws_with([&] { g::thicken("F3", *plate, 0.0, false); }, "must not be zero"));
}

} // namespace
} // namespace test

int main() {
  mitcad::geometry::enable_input_checks(true);
  test::guarded("test_regions_are_patched_into_surface_bodies", test::test_regions_are_patched_into_surface_bodies);
  test::guarded("test_an_open_box_is_stitched_patched_and_closed",
                test::test_an_open_box_is_stitched_patched_and_closed);
  test::guarded("test_a_fill_passes_through_its_points", test::test_a_fill_passes_through_its_points);
  test::guarded("test_faces_are_sewn_within_the_tolerance", test::test_faces_are_sewn_within_the_tolerance);
  test::guarded("test_surfaces_are_thickened", test::test_surfaces_are_thickened);
  for (const std::string& change : mitcad::geometry::input_changes()) {
    std::fprintf(stderr, "an input changed: %s\n", change.c_str());
    ++test::failures;
  }
  if (test::failures == 0) {
    std::puts("test_surfaces: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
