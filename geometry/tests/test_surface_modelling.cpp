// SPDX-License-Identifier: MIT
// Surface modelling (mitcad#455): surface bodies swept from sketch curves
// (straight, about an axis, along a path, through sections) and a solid
// extrusion's walls without its caps, with their areas and the names of
// their faces. Every operation leaves its inputs as they are (the input
// check is on).

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Pnt.hxx>
#include <BRepGProp.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pln.hxx>
// The vertices of an extended plane and their distances (mitcad#2056).
#include <BRep_Tool.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Vec.hxx>

// A loop through three sketches filled, measured against its curves
// (mitcad#2220).
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Curve2d.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepTools.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Circle.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Line.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeAnalysis_Curve.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt2d.hxx>

// A network whose curves run on past the outer crossing curves, and one
// whose curves do not all meet (mitcad#2333).
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <stdexcept>

#include "check.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/surface_edits.hpp"
#include "mitcad/geometry/surface_fills.hpp"
#include "mitcad/geometry/surface_offsets.hpp"
#include "mitcad/geometry/surface_sweeps.hpp"
#include "mitcad/geometry/surfaces.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

double area(const g::Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape.occt(), props);
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

g::PathCurve line(const std::string& name, gp_Pnt a, gp_Pnt b) {
  g::PathCurve piece;
  piece.name = name;
  piece.curve.kind = g::ModelCurveKind::Line;
  piece.curve.start = a;
  piece.curve.end = b;
  return piece;
}

// An L of two lines in the xy plane: (0, 0) to (10, 0) to (10, 5).
g::Path ell() { return {line("c1", gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0)), line("c2", gp_Pnt(10, 0, 0), gp_Pnt(10, 5, 0))}; }

void test_open_curves_are_extruded_into_a_surface() {
  // 8 up and 2 down from the plane moved 1 up.
  const g::ShapePtr sheet = g::extrude_sheet("F2", {ell()}, gp_Vec(0, 0, 1), gp_Dir(0, 0, 1), 8.0, 2.0);
  CHECK(!solid(*sheet));
  CHECK_NEAR(area(*sheet), 15.0 * 10.0);
  CHECK(sheet->face_count() == 2);
  CHECK(named(*sheet, "F2:side(c1)") == 1);
  CHECK(named(*sheet, "F2:side(c2)") == 1);
  if (named(*sheet, "F2:side(c1)") != 1) {
    print_names(*sheet);
  }
  CHECK(throws_with([] { g::extrude_sheet("F2", {ell()}, gp_Vec(), gp_Dir(0, 0, 1), 0.0, 0.0); },
                    "add up to more than zero"));
}

void test_curves_are_revolved_into_a_surface() {
  // A line 5 off the z axis, 10 long: a cylinder's wall, a quarter of it.
  const g::Path wall = {line("c3", gp_Pnt(5, 0, 0), gp_Pnt(5, 0, 10))};
  const gp_Ax1 axis(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  const g::ShapePtr full = g::revolve_sheet("F3", {wall}, axis, 2 * kPi, 0.0);
  CHECK_NEAR_TOL(area(*full), 2 * kPi * 5 * 10, 1e-6);
  CHECK(named(*full, "F3:side(c3)") == 1);
  const g::ShapePtr quarter = g::revolve_sheet("F3", {wall}, axis, kPi / 4, kPi / 4);
  CHECK_NEAR_TOL(area(*quarter), kPi / 2 * 5 * 10, 1e-6);
  CHECK(!solid(*quarter));
}

void test_curves_are_swept_along_a_path() {
  // The L swept 20 up along the z axis from its plane.
  const g::Path path = {line("c9", gp_Pnt(0, 0, 0), gp_Pnt(0, 0, 20))};
  const gp_Pln plane(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  const g::ShapePtr sheet =
      g::sweep_sheet("F4", {ell()}, path, plane, 1.0, 0.0, g::SweepOrientation::Perpendicular);
  CHECK_NEAR_TOL(area(*sheet), 15.0 * 20.0, 1e-5);
  CHECK(named(*sheet, "F4:side(c1)") == 1);
  CHECK(named(*sheet, "F4:side(c2)") == 1);
  // From the middle of a path through the plane, half each way.
  const g::Path through = {line("c9", gp_Pnt(0, 0, -10), gp_Pnt(0, 0, 10))};
  const g::ShapePtr both =
      g::sweep_sheet("F4", {ell()}, through, plane, 0.5, 0.5, g::SweepOrientation::Parallel);
  CHECK_NEAR_TOL(area(*both), 15.0 * 10.0, 1e-5);
  CHECK(named(*both, "F4:side(c1)") == 2);
}

void test_sections_of_curves_are_lofted() {
  // Two lines 10 apart: a ruled quad of 10 x 10.
  const g::Path low = {line("c1", gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0))};
  const g::Path high = {line("c5", gp_Pnt(0, 0, 10), gp_Pnt(10, 0, 10))};
  const g::ShapePtr sheet = g::loft_sheet("F5", {low, high}, true, false);
  CHECK_NEAR_TOL(area(*sheet), 100.0, 1e-6);
  CHECK(named(*sheet, "F5:side(c1)") == 1);
  CHECK(throws_with(
      [&] {
        g::loft_sheet("F5", {low, {line("c7", gp_Pnt(0, 0, 0), gp_Pnt(1, 0, 0)), line("c8", gp_Pnt(1, 0, 0), gp_Pnt(0, 1, 0)),
                              line("c9", gp_Pnt(0, 1, 0), gp_Pnt(0, 0, 0))}},
                      false, false);
      },
      "all open or all closed"));
}

void test_a_solid_extrusion_loses_its_caps() {
  g::ExtrudeSpec spec;
  spec.feature = "F2";
  g::Region r = g::rectangle_region(0, 0, 20, 10);
  r.name = "r1";
  for (std::size_t i = 0; i < r.loops.front().segments.size(); ++i) {
    r.loops.front().segments[i].name = "c" + std::to_string(i + 1);
  }
  spec.regions = {r};
  spec.start = 0.0;
  spec.end = 5.0;
  const g::ShapePtr box = g::extrude(spec);
  const g::ShapePtr walls = g::open_faces(*box, "F2", {"start", "end", "mid"});
  CHECK(!solid(*walls));
  CHECK(walls->face_count() == 4);
  CHECK_NEAR(area(*walls), 60.0 * 5.0);
  CHECK(named(*walls, "F2:start(r1)") == 0);
  CHECK(named(*walls, "F2:end(r1)") == 0);
  for (int i = 1; i <= 4; ++i) {
    CHECK(named(*walls, "F2:side(c" + std::to_string(i) + ")") == 1);
  }
  if (named(*walls, "F2:side(c1)") != 1) {
    print_names(*walls);
  }
  // Another feature's roles take nothing away.
  CHECK(g::open_faces(*box, "F3", {"start", "end"})->face_count() == 6);
}

// --- Offset Surface, Ruled Surface and Extend

// A 20 x 10 x 5 box F2 (r1, sides c1..c4), and its walls without caps.
g::ShapePtr box() {
  g::ExtrudeSpec spec;
  spec.feature = "F2";
  g::Region r = g::rectangle_region(0, 0, 20, 10);
  r.name = "r1";
  for (std::size_t i = 0; i < r.loops.front().segments.size(); ++i) {
    r.loops.front().segments[i].name = "c" + std::to_string(i + 1);
  }
  spec.regions = {r};
  spec.end = 5.0;
  return g::extrude(spec);
}

g::ShapePtr walls() { return g::open_faces(*box(), "F2", {"start", "end"}); }

// The index of the walls' opening at the top (z = 5).
int top_opening(const g::Shape& sheet) {
  const std::vector<g::OpenBoundary> openings = g::open_boundaries(sheet);
  for (std::size_t i = 0; i < openings.size(); ++i) {
    if (!openings[i].points.empty() && openings[i].points.front().Z() > 2.5) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

std::vector<std::string> top_edges() {
  std::vector<std::string> edges;
  for (int i = 1; i <= 4; ++i) {
    edges.push_back("E{F2:end(r1)|F2:side(c" + std::to_string(i) + ")}");
  }
  return edges;
}

void test_faces_are_offset_into_a_surface() {
  const g::ShapePtr solid_box = box();
  const g::ShapePtr up = g::offset_surface("F3", *solid_box, {"F2:end(r1)"}, 2.0, false);
  CHECK(!solid(*up));
  CHECK_NEAR(area(*up), 200.0);
  CHECK(named(*up, "F3:offset(F2:end(r1))") == 1);
  CHECK_NEAR(z_range(*up).first, 7.0);
  CHECK_NEAR(z_range(*g::offset_surface("F3", *solid_box, {"F2:end(r1)"}, -1.0, false)).first, 4.0);
  const g::ShapePtr copy = g::offset_surface("F3", *solid_box, {"F2:end(r1)"}, 0.0, false);
  CHECK_NEAR(area(*copy), 200.0);
  CHECK(named(*copy, "F3:offset(F2:end(r1))") == 1);
  // The walls 1 out: the corners' gaps closed, 2 (22 + 12) 5.
  const g::ShapePtr out = g::offset_surface("F3", *walls(), {}, 1.0, false);
  CHECK(out->face_count() == 4);
  CHECK_NEAR_TOL(area(*out), 340.0, 1e-3);
  CHECK(named(*out, "F3:offset(F2:side(c1))") == 1);
}

void test_strips_are_ruled_off_edges() {
  const g::ShapePtr solid_box = box();
  g::RuledSpec spec;
  spec.feature = "F3";
  spec.edges = top_edges();
  spec.face = "F2:end(r1)";
  spec.distance = 3.0;
  // Along the top's normal: walls 3 high above the box's top edges.
  const g::ShapePtr normal = g::ruled_surface(*solid_box, spec);
  CHECK(!solid(*normal));
  CHECK_NEAR_TOL(area(*normal), 60.0 * 3.0, 1e-6);
  CHECK(named(*normal, "F3:ruled(E{F2:end(r1)|F2:side(c1)})") == 1);
  // Continuing the top: a mitred frame 3 wide round it.
  spec.mode = g::RuledMode::Tangent;
  const g::ShapePtr mitred = g::ruled_surface(*solid_box, spec);
  CHECK_NEAR_TOL(area(*mitred), 26.0 * 16.0 - 200.0, 1e-6);
  CHECK(mitred->face_count() == 4);
  // Round corners: quarter discs of radius 3.
  spec.corners = g::RuledCorners::Round;
  const g::ShapePtr round = g::ruled_surface(*solid_box, spec);
  CHECK_NEAR_TOL(area(*round), 180.0 + 9.0 * kPi, 1e-5);
  CHECK(round->face_count() == 8);
  // At 90 degrees from the normal: the frame again.
  spec.corners = g::RuledCorners::Mitre;
  spec.mode = g::RuledMode::Angle;
  spec.angle = kPi / 2;
  CHECK_NEAR_TOL(area(*g::ruled_surface(*solid_box, spec)), 216.0, 1e-6);
  // The walls' free top edges, upwards.
  const g::ShapePtr sheet = walls();
  g::RuledSpec up;
  up.feature = "F3";
  up.openings = {{top_opening(*sheet), {}, {}}};
  up.distance = 2.0;
  up.mode = g::RuledMode::Tangent;
  const g::ShapePtr rim = g::ruled_surface(*sheet, up);
  CHECK_NEAR_TOL(area(*rim), 120.0, 1e-6);
  CHECK(named(*rim, "F3:ruled(F2:side(c1))") == 1);
  CHECK_NEAR(z_range(*rim).second, 7.0);
  // Edges apart make strips of their own (two sides of the top, 20 x 3 each).
  spec.edges = {top_edges()[0], top_edges()[2]};
  CHECK_NEAR_TOL(area(*g::ruled_surface(*solid_box, spec)), 120.0, 1e-6);
}

void test_surfaces_are_extended() {
  const g::ShapePtr sheet = walls();
  const int top = top_opening(*sheet);
  CHECK(top >= 0);
  const g::ShapePtr natural = g::extend_surface(*sheet, {}, {{top, {}, {}}}, 2.0, g::ExtendMode::Natural);
  CHECK(!solid(*natural));
  CHECK(natural->face_count() == 4);
  CHECK_NEAR_TOL(area(*natural), 60.0 * 7.0, 1e-6);
  CHECK(named(*natural, "F2:side(c1)") == 1);
  CHECK_NEAR(z_range(*natural).second, 7.0);
  // One wall only, tangent (a plane continues as itself).
  const g::ShapePtr one =
      g::extend_surface(*sheet, {}, {{top, {"F2:side(c1)"}, {}}}, 2.0, g::ExtendMode::Tangent);
  CHECK_NEAR_TOL(area(*one), 300.0 + 40.0, 1e-5);
  CHECK(throws_with([&] { g::extend_surface(*box(), top_edges(), {}, 2.0, g::ExtendMode::Natural); },
                    "solid"));
}

// A surface body of one planar face, a trapezoid cut at a slant inside its
// plane (y = 0): 40 long at z = 0, 35 at z = 10 (mitcad#1955, GitHub
// mitcad/Mitcad#407: a wall whose corner a cut took away).
g::ShapePtr trapezoid() {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(0, 0, 0), gp_Pnt(40, 0, 0), gp_Pnt(35, 0, 10), gp_Pnt(0, 0, 10), true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  builder.Add(shell, face);
  return std::make_shared<g::Shape>(shell, std::vector<g::Shape::NamedFace>{{face, {"F2:side(c1)"}}});
}

// Whether the shape has a vertex within 1e-6 mm of the point.
bool has_vertex(const g::Shape& shape, const gp_Pnt& point) {
  for (int i = 0; i < shape.vertex_count(); ++i) {
    if (BRep_Tool::Pnt(shape.vertex(i)).Distance(point) <= 1.0e-6) {
      return true;
    }
  }
  std::fprintf(stderr, "  no vertex at (%.9g, %.9g, %.9g); the vertices:\n", point.X(), point.Y(), point.Z());
  for (int i = 0; i < shape.vertex_count(); ++i) {
    const gp_Pnt p = BRep_Tool::Pnt(shape.vertex(i));
    std::fprintf(stderr, "    (%.9g, %.9g, %.9g)\n", p.X(), p.Y(), p.Z());
  }
  return false;
}

// The distance from the point to the line through a and b.
double from_line(const gp_Pnt& point, const gp_Pnt& a, const gp_Pnt& b) {
  return gp_Lin(a, gp_Dir(gp_Vec(a, b))).Distance(point);
}

// A surface body of one planar face in the plane y = 0 with the corners
// (x, z) given in turn, named F2:side(c1). The plane's own axes are turned
// 30 degrees from x and z, so that even a rectangle along x and z is
// trimmed inside the plane's parameter range (the extension's planar
// path, not the widened range of an untrimmed face).
g::ShapePtr plane_sheet(const std::vector<std::pair<double, double>>& corners) {
  BRepBuilderAPI_MakePolygon outline;
  for (const auto& corner : corners) {
    outline.Add(gp_Pnt(corner.first, 0, corner.second));
  }
  outline.Close();
  const gp_Pln plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, -1, 0), gp_Dir(std::sqrt(3.0) / 2, 0, 0.5)));
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(plane, outline.Wire(), true).Face();
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  builder.Add(shell, face);
  return std::make_shared<g::Shape>(shell, std::vector<g::Shape::NamedFace>{{face, {"F2:side(c1)"}}});
}

void test_a_trimmed_plane_is_extended_past_one_edge() {
  // mitcad#1955: a single free edge of a face trimmed inside its plane,
  // picked by a point on it, extends; the face stays one face. Its sides
  // continue the neighbouring edges (mitcad#2056).
  const g::ShapePtr sheet = trapezoid();
  CHECK_NEAR_TOL(area(*sheet), 375.0, 1e-6);
  const g::ShapePtr lower =
      g::extend_surface(*sheet, {}, {{0, {}, {gp_Pnt(20, 0, 0)}}}, 2.0, g::ExtendMode::Natural);
  CHECK(lower->face_count() == 1);
  CHECK(named(*lower, "F2:side(c1)") == 1);
  // The left edge continued square, the slanted edge continued along its
  // slope (1 across for 2 down): a strip 40 long at the top, 41 at z = -2.
  CHECK_NEAR_TOL(area(*lower), 375.0 + 81.0, 1e-6);
  CHECK_NEAR_TOL(z_range(*lower).first, -2.0, 1e-6);
  CHECK(lower->edge_count() == 4);
  CHECK(has_vertex(*lower, gp_Pnt(0, 0, -2)));
  CHECK(has_vertex(*lower, gp_Pnt(41, 0, -2)));
  // The slanted edge (any mode continues a plane): the top and bottom
  // edges continued by 2 / sin(63.4 degrees) = sqrt(5), the new end edge
  // parallel to the slanted one, 2 from it (GitHub mitcad/Mitcad#462).
  const g::ShapePtr slanted =
      g::extend_surface(*sheet, {}, {{0, {}, {gp_Pnt(37.5, 0, 5)}}}, 2.0, g::ExtendMode::Tangent);
  CHECK(slanted->face_count() == 1);
  CHECK(named(*slanted, "F2:side(c1)") == 1);
  CHECK(slanted->edge_count() == 4);
  CHECK_NEAR_TOL(area(*slanted), 375.0 + 2.0 * std::sqrt(125.0), 1e-6);
  const gp_Pnt bottom(40.0 + std::sqrt(5.0), 0, 0);
  const gp_Pnt top(35.0 + std::sqrt(5.0), 0, 10);
  CHECK(has_vertex(*slanted, bottom));
  CHECK(has_vertex(*slanted, top));
  CHECK(has_vertex(*slanted, gp_Pnt(0, 0, 0)));
  CHECK(has_vertex(*slanted, gp_Pnt(0, 0, 10)));
  CHECK_NEAR_TOL(from_line(bottom, gp_Pnt(40, 0, 0), gp_Pnt(35, 0, 10)), 2.0, 1e-6);
  CHECK_NEAR_TOL(from_line(top, gp_Pnt(40, 0, 0), gp_Pnt(35, 0, 10)), 2.0, 1e-6);
  // The input is left as it was.
  CHECK_NEAR_TOL(area(*sheet), 375.0, 1e-6);
  CHECK(sheet->edge_count() == 4);
}

void test_a_trimmed_plane_is_extended_round_a_corner() {
  // mitcad#2056: two adjacent picked edges of a 30 x 20 rectangle extended
  // by 3 meet at the mitre of their end lines: one rectangle of 33 x 23,
  // the corner filled. Its bottom edge in two, of which the right one is
  // picked: the left one runs along the end line, too flat to continue, so
  // the strip is square at x = 15 (strips of (15 + 18) / 2 * 3 and
  // (20 + 23) / 2 * 3).
  const g::ShapePtr sheet = plane_sheet({{0, 0}, {15, 0}, {30, 0}, {30, 20}, {0, 20}});
  CHECK_NEAR_TOL(area(*sheet), 600.0, 1e-6);
  const g::ShapePtr both = g::extend_surface(*sheet, {}, {{0, {}, {gp_Pnt(25, 0, 0), gp_Pnt(30, 0, 10)}}}, 3.0,
                                             g::ExtendMode::Natural);
  CHECK(both->face_count() == 1);
  CHECK(named(*both, "F2:side(c1)") == 1);
  CHECK_NEAR_TOL(area(*both), 600.0 + 49.5 + 64.5, 1e-6);
  CHECK(has_vertex(*both, gp_Pnt(33, 0, -3)));
  CHECK(has_vertex(*both, gp_Pnt(33, 0, 20)));
  CHECK(has_vertex(*both, gp_Pnt(15, 0, -3)));
  // The plain rectangle, its bottom and right edges picked.
  const g::ShapePtr box = plane_sheet({{0, 0}, {30, 0}, {30, 20}, {0, 20}});
  const g::ShapePtr grown = g::extend_surface(*box, {}, {{0, {}, {gp_Pnt(15, 0, 0), gp_Pnt(30, 0, 10)}}}, 3.0,
                                              g::ExtendMode::Natural);
  CHECK(grown->face_count() == 1);
  CHECK(grown->edge_count() == 4);
  CHECK_NEAR_TOL(area(*grown), 33.0 * 23.0, 1e-6);
  CHECK(has_vertex(*grown, gp_Pnt(0, 0, -3)));
  CHECK(has_vertex(*grown, gp_Pnt(33, 0, -3)));
  CHECK(has_vertex(*grown, gp_Pnt(33, 0, 20)));
  CHECK(has_vertex(*grown, gp_Pnt(0, 0, 20)));
  // The inputs are left as they were.
  CHECK_NEAR_TOL(area(*box), 600.0, 1e-6);
  CHECK(box->edge_count() == 4);
}

void test_a_trimmed_plane_falls_back_to_square_sides() {
  // mitcad#2056: where the neighbouring edge cannot be continued the side
  // is square to the picked edge. A reflex corner: the neighbour at
  // (10, 5) runs up and left to (5, 15), its continuation would turn back
  // into the face; the strip past the edge from (20, 5) to (10, 5) is
  // square at x = 10 (continued, it would end at (9, 7) and add 1 more).
  const g::ShapePtr notched = plane_sheet({{0, 0}, {20, 0}, {20, 5}, {10, 5}, {5, 15}, {0, 15}});
  CHECK_NEAR_TOL(area(*notched), 175.0, 1e-6);
  const g::ShapePtr reflex =
      g::extend_surface(*notched, {}, {{0, {}, {gp_Pnt(15, 0, 5)}}}, 2.0, g::ExtendMode::Natural);
  CHECK(reflex->face_count() == 1);
  CHECK_NEAR_TOL(area(*reflex), 175.0 + 20.0, 1e-6);
  CHECK(has_vertex(*reflex, gp_Pnt(10, 0, 7)));
  CHECK(has_vertex(*reflex, gp_Pnt(20, 0, 7)));
  // Under the angle limit: the neighbour at (40, 10) comes from (100, 0),
  // 9.5 degrees from the picked top edge's line; continued, it would meet
  // the end line 12 further on (a strip of 68 instead of 80).
  const g::ShapePtr flat = plane_sheet({{0, 0}, {100, 0}, {40, 10}, {0, 10}});
  CHECK_NEAR_TOL(area(*flat), 700.0, 1e-6);
  const g::ShapePtr sliver =
      g::extend_surface(*flat, {}, {{0, {}, {gp_Pnt(20, 0, 10)}}}, 2.0, g::ExtendMode::Natural);
  CHECK(sliver->face_count() == 1);
  CHECK_NEAR_TOL(area(*sliver), 700.0 + 80.0, 1e-6);
  CHECK(has_vertex(*sliver, gp_Pnt(40, 0, 12)));
  CHECK(has_vertex(*sliver, gp_Pnt(0, 0, 12)));
  // Above the limit (26.6 degrees, from (60, 0)) the neighbour continues:
  // to (36, 12), a strip of (40 + 36) / 2 * 2.
  const g::ShapePtr steeper = plane_sheet({{0, 0}, {60, 0}, {40, 10}, {0, 10}});
  const g::ShapePtr continued =
      g::extend_surface(*steeper, {}, {{0, {}, {gp_Pnt(20, 0, 10)}}}, 2.0, g::ExtendMode::Natural);
  CHECK(continued->face_count() == 1);
  CHECK_NEAR_TOL(area(*continued), 500.0 + 76.0, 1e-6);
  CHECK(has_vertex(*continued, gp_Pnt(36, 0, 12)));
  // The inputs are left as they were.
  CHECK_NEAR_TOL(area(*notched), 175.0, 1e-6);
  CHECK_NEAR_TOL(area(*flat), 700.0, 1e-6);
  CHECK(flat->edge_count() == 4);
}

// The name of the walls' edge between the two faces.
std::string edge_between(const g::Shape& shape, const std::string& a, const std::string& b) {
  for (int i = 0; i < shape.edge_count(); ++i) {
    const std::string name = shape.edge_name(i);
    if (name.find(a) != std::string::npos && name.find(b) != std::string::npos) {
      return name;
    }
  }
  return std::string();
}

void test_a_patch_closes_a_loop_of_free_and_named_edges() {
  // mitcad#1955: single free edges (the top and bottom edge of wall c1,
  // each picked by a point of its opening) and named edges (the wall's
  // corners) in one loop.
  const g::ShapePtr sheet = walls();
  const int top = top_opening(*sheet);
  CHECK(top >= 0 && g::open_boundaries(*sheet).size() == 2);
  const int bottom = 1 - top;
  g::PatchLoop loop;
  loop.edges = {edge_between(*sheet, "F2:side(c4)", "F2:side(c1)"),
                edge_between(*sheet, "F2:side(c1)", "F2:side(c2)")};
  CHECK(!loop.edges[0].empty() && !loop.edges[1].empty());
  loop.openings = {{top, {}, {gp_Pnt(10, 0, 5)}}, {bottom, {}, {gp_Pnt(10, 0, 0)}}};
  const g::ShapePtr patch = g::patch_loop("F3", *sheet, loop);
  CHECK(named(*patch, "F3:patch") == 1);
  CHECK_NEAR_TOL(area(*patch), 20.0 * 5.0, 1e-6);
  // The free edges alone do not close up.
  loop.edges.clear();
  CHECK(throws_with([&] { g::patch_loop("F3", *sheet, loop); }, "close"));
}

void test_a_patch_closes_free_edges_with_a_sketch_line() {
  // mitcad#1972 (GitHub mitcad/Mitcad#407): two single free edges of the
  // top opening (walls c1 and c2) and a sketch line across the opening
  // close a triangle of it.
  const g::ShapePtr sheet = walls();
  const int top = top_opening(*sheet);
  CHECK(top >= 0);
  g::PatchLoop loop;
  loop.openings = {{top, {}, {gp_Pnt(10, 0, 5), gp_Pnt(20, 5, 5)}}};
  loop.curves = {line("c9", gp_Pnt(20, 10, 5), gp_Pnt(0, 0, 5))};
  const g::ShapePtr patch = g::patch_loop("F3", *sheet, loop);
  CHECK(named(*patch, "F3:patch") == 1);
  CHECK_NEAR_TOL(area(*patch), 20.0 * 10.0 / 2.0, 1e-6);
  CHECK_NEAR_TOL(z_range(*patch).first, 5.0, 1e-6);
  // A line whose ends lie a few micrometres off the edges' ends still
  // meets them.
  loop.curves = {line("c9", gp_Pnt(20, 10.004, 5), gp_Pnt(0.003, 0, 5))};
  CHECK_NEAR_TOL(area(*g::patch_loop("F3", *sheet, loop)), 100.0, 0.1);
  // One that stops short of an edge's end does not close the loop.
  loop.curves = {line("c9", gp_Pnt(20, 10, 5), gp_Pnt(1, 0, 5))};
  CHECK(throws_with([&] { g::patch_loop("F3", *sheet, loop); }, "close"));
  // Nor do two curves across the same span.
  loop.curves = {line("c9", gp_Pnt(20, 10, 5), gp_Pnt(0, 0, 5)), line("c10", gp_Pnt(0, 0, 5), gp_Pnt(20, 10, 5))};
  CHECK(throws_with([&] { g::patch_loop("F3", *sheet, loop); }, "loop"));
}

// --- Trim, Untrim, Unstitch and Reverse Normal

void test_surfaces_are_trimmed() {
  const g::ShapePtr sheet = walls();
  // By a plane half way up, the upper half kept.
  g::TrimInput by_plane;
  by_plane.tool = g::Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 2.5), gp_Dir(0, 0, 1)));
  const g::ShapePtr upper = g::trim_surface(*sheet, by_plane, {gp_Pnt(10, -1, 4)});
  CHECK(!solid(*upper));
  CHECK_NEAR_TOL(area(*upper), 60.0 * 2.5, 1e-6);
  CHECK_NEAR(z_range(*upper).first, 2.5);
  CHECK(named(*upper, "F2:side(c1)") == 1);
  // Both halves kept: all of it, in pieces.
  CHECK_NEAR_TOL(area(*g::trim_surface(*sheet, by_plane, {gp_Pnt(10, -1, 4), gp_Pnt(10, -1, 1)})), 300.0, 1e-6);
  // By an open sketch line across the box (y = 5), swept along z: the half
  // with the wall at y = 0.
  g::TrimInput by_line;
  by_line.chains = {{line("c7", gp_Pnt(-5, 5, 0), gp_Pnt(25, 5, 0))}};
  const g::ShapePtr front = g::trim_surface(*sheet, by_line, {gp_Pnt(10, 0, 2)});
  CHECK_NEAR_TOL(area(*front), 20.0 * 5.0 + 2.0 * 5.0 * 5.0, 1e-6);
  CHECK(throws_with([&] { g::trim_surface(*sheet, by_plane, {}); }, "no point"));
  g::TrimInput above;
  above.tool = g::Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 20), gp_Dir(0, 0, 1)));
  CHECK(throws_with([&] { g::trim_surface(*sheet, above, {gp_Pnt(0, 0, 0)}); }, "does not cut"));
}

// A 20 x 10 x 5 box F2 with a round hole of radius 2 (c5) through it.
g::ShapePtr holed_box() {
  g::ExtrudeSpec spec;
  spec.feature = "F2";
  g::Region r = g::rectangle_region(0, 0, 20, 10);
  r.name = "r1";
  for (std::size_t i = 0; i < r.loops.front().segments.size(); ++i) {
    r.loops.front().segments[i].name = "c" + std::to_string(i + 1);
  }
  g::Region hole = g::circle_region(10, 5, 2);
  hole.loops.front().segments.front().name = "c5";
  r.loops.push_back(hole.loops.front());
  spec.regions = {r};
  spec.end = 5.0;
  return g::extrude(spec);
}

void test_faces_are_untrimmed() {
  const g::ShapePtr box = holed_box();
  // The top without its hole, a new surface.
  const g::ShapePtr top = g::untrim_surface("F3", *box, "F2:end(r1)", true, false, false);
  CHECK_NEAR_TOL(area(*top), 200.0, 1e-6);
  CHECK(named(*top, "F3:untrim(F2:end(r1))") == 1);
  // A plane's bounds are the face's span: the same.
  CHECK_NEAR_TOL(area(*g::untrim_surface("F3", *box, "F2:end(r1)", false, true, false)), 200.0 - 4.0 * kPi, 1e-6);
  // In place on a surface (the box without its bottom): the top whole.
  const g::ShapePtr open = g::open_faces(*box, "F2", {"start"});
  const g::ShapePtr whole = g::untrim_surface("F3", *open, "F2:end(r1)", true, false, true);
  CHECK_NEAR_TOL(area(*whole), area(*open) + 4.0 * kPi, 1e-6);
  CHECK(named(*whole, "F2:end(r1)") == 1);
  CHECK(throws_with([&] { g::untrim_surface("F3", *box, "F2:end(r1)", false, false, false); }, "nothing"));
}

void test_faces_come_apart_and_turn_over() {
  const g::ShapePtr sheet = walls();
  const auto [rest, apart] = g::unstitch(*sheet, {"F2:side(c1)"});
  CHECK(rest && rest->face_count() == 3);
  CHECK(apart.size() == 1 && named(*apart.front(), "F2:side(c1)") == 1);
  CHECK_NEAR(area(*apart.front()), 100.0);
  // A solid comes apart into its six faces.
  const auto every = g::unstitch(*box(), {});
  CHECK(!every.first && every.second.size() == 6);
  CHECK(!solid(*every.second.front()));
  // The walls turned inside out: an offset of 1 goes in, 2 (18 + 8) 5.
  const g::ShapePtr inward = g::reverse_faces(*sheet, {});
  CHECK(named(*inward, "F2:side(c3)") == 1);
  CHECK_NEAR_TOL(area(*g::offset_surface("F3", *inward, {}, 1.0, false)), 260.0, 1e-3);
  CHECK(throws_with([&] { g::reverse_faces(*box(), {}); }, "solid"));
}

// --- Blend Surface, Boundary Surface, Match Surface and Fair Surface

// The walls (sides c1..c4) of a 20 x 10 rectangle `feature` from z0 to z1.
g::ShapePtr walls_at(const std::string& feature, double z0, double z1) {
  g::ExtrudeSpec spec;
  spec.feature = feature;
  g::Region r = g::rectangle_region(0, 0, 20, 10);
  r.name = "r1";
  for (std::size_t i = 0; i < r.loops.front().segments.size(); ++i) {
    r.loops.front().segments[i].name = "c" + std::to_string(i + 1);
  }
  spec.regions = {r};
  spec.start = z0;
  spec.end = z1;
  return g::open_faces(*g::extrude(spec), feature, {"start", "end"});
}

// The index of the opening whose first point is above (or below) z.
int opening_at(const g::Shape& sheet, double z, bool above) {
  const std::vector<g::OpenBoundary> openings = g::open_boundaries(sheet);
  for (std::size_t i = 0; i < openings.size(); ++i) {
    if (!openings[i].points.empty() && (openings[i].points.front().Z() > z) == above) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

// The free edges of a side's face at the top (or bottom) of walls.
g::EdgeChain side_edge(const g::ShapePtr& sheet, const std::string& face, double z, bool top) {
  g::EdgeChain chain;
  chain.body = sheet.get();
  chain.openings = {{opening_at(*sheet, z, top), {face}, {}}};
  return chain;
}

void test_chains_are_blended() {
  const g::ShapePtr low = walls_at("F2", 0.0, 5.0);
  const g::ShapePtr high = walls_at("F3", 10.0, 15.0);
  g::BlendEnd from;
  from.chain = side_edge(low, "F2:side(c1)", 2.5, true);
  g::BlendEnd to;
  to.chain = side_edge(high, "F3:side(c1)", 12.5, false);
  // Straight up between the two walls in y = 0, whatever the continuity.
  for (const g::Continuity c : {g::Continuity::G0, g::Continuity::G1, g::Continuity::G2}) {
    from.continuity = c;
    to.continuity = c;
    const g::ShapePtr blend = g::blend_surface("F4", from, to);
    CHECK(!solid(*blend));
    CHECK(named(*blend, "F4:blend") == 1);
    CHECK_NEAR_TOL(area(*blend), 100.0, 1e-4);
    CHECK_NEAR_TOL(z_range(*blend).first, 5.0, 1e-4);
    CHECK_NEAR_TOL(z_range(*blend).second, 10.0, 1e-4);
  }
  // Across to the far side (y = 10, run the other way): a straight strip
  // without continuity, a longer S-bend tangent to both walls.
  to.chain = side_edge(high, "F3:side(c3)", 12.5, false);
  from.continuity = g::Continuity::G0;
  to.continuity = g::Continuity::G0;
  const double straight = area(*g::blend_surface("F4", from, to));
  CHECK_NEAR_TOL(straight, 20.0 * std::sqrt(125.0), 1e-4);
  from.continuity = g::Continuity::G1;
  to.continuity = g::Continuity::G1;
  const double bent = area(*g::blend_surface("F4", from, to));
  CHECK(bent > straight + 1.0);
  from.bulge = 1.0;
  CHECK(area(*g::blend_surface("F4", from, to)) > bent);
  from.bulge = -2.0;
  CHECK(throws_with([&] { g::blend_surface("F4", from, to); }, "bulge"));
}

g::BoundaryCurve sketch_curve(const std::string& name, gp_Pnt a, gp_Pnt b) {
  g::BoundaryCurve curve;
  curve.kind = g::BoundaryCurve::Kind::Curves;
  curve.curves = {line(name, a, b)};
  curve.plane = gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  return curve;
}

void test_boundaries_are_filled() {
  // A 10 x 10 square of four lines: flat, also tangent to its plane.
  std::vector<g::BoundaryCurve> square = {
      sketch_curve("c1", gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0)), sketch_curve("c2", gp_Pnt(10, 0, 0), gp_Pnt(10, 10, 0)),
      sketch_curve("c3", gp_Pnt(10, 10, 0), gp_Pnt(0, 10, 0)), sketch_curve("c4", gp_Pnt(0, 10, 0), gp_Pnt(0, 0, 0))};
  const g::ShapePtr flat = g::boundary_fill("F5", square);
  CHECK(named(*flat, "F5:boundary") == 1);
  CHECK_NEAR_TOL(area(*flat), 100.0, 1e-3);
  for (g::BoundaryCurve& curve : square) {
    curve.continuity = g::Continuity::G1;
  }
  CHECK_NEAR_TOL(area(*g::boundary_fill("F5", square)), 100.0, 1e-3);
  // The walls' top opening, all of it: a lid of 20 x 10.
  const g::ShapePtr sheet = walls_at("F2", 0.0, 5.0);
  g::BoundaryCurve top;
  top.kind = g::BoundaryCurve::Kind::Edges;
  top.edges.body = sheet.get();
  top.edges.openings = {{opening_at(*sheet, 2.5, true), {}, {}}};
  CHECK_NEAR_TOL(area(*g::boundary_fill("F5", {top})), 200.0, 1e-3);
  // The top edge of side c1 and three lines above it, tangent to the wall:
  // the wall continued up 10.
  g::BoundaryCurve edge;
  edge.kind = g::BoundaryCurve::Kind::Edges;
  edge.edges = side_edge(sheet, "F2:side(c1)", 2.5, true);
  edge.continuity = g::Continuity::G1;
  const g::ShapePtr up = g::boundary_fill(
      "F5", {edge, sketch_curve("c5", gp_Pnt(20, 0, 5), gp_Pnt(20, 0, 15)),
             sketch_curve("c6", gp_Pnt(20, 0, 15), gp_Pnt(0, 0, 15)), sketch_curve("c7", gp_Pnt(0, 0, 15), gp_Pnt(0, 0, 5))});
  CHECK_NEAR_TOL(area(*up), 200.0, 1e-3);
  CHECK_NEAR_TOL(z_range(*up).second, 15.0, 1e-4);
}

void test_networks_are_spanned() {
  // Two lines each way: a tilted plane 10 wide and sqrt(104) long.
  const std::vector<g::BoundaryCurve> u = {sketch_curve("c1", gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0)),
                                           sketch_curve("c2", gp_Pnt(0, 10, 2), gp_Pnt(10, 10, 2))};
  const std::vector<g::BoundaryCurve> v = {sketch_curve("c3", gp_Pnt(0, 0, 0), gp_Pnt(0, 10, 2)),
                                           sketch_curve("c4", gp_Pnt(10, 0, 0), gp_Pnt(10, 10, 2))};
  const g::ShapePtr quad = g::boundary_network("F6", u, v);
  CHECK(named(*quad, "F6:boundary") == 1);
  CHECK_NEAR_TOL(area(*quad), 10.0 * std::sqrt(104.0), 1e-4);
  // Lofted through the curves one way alone.
  CHECK_NEAR_TOL(area(*g::boundary_network("F6", u, {})), 10.0 * std::sqrt(104.0), 1e-4);
  // A line to a point: a triangle.
  g::BoundaryCurve apex;
  apex.kind = g::BoundaryCurve::Kind::Point;
  apex.point = gp_Pnt(5, 5, 0);
  CHECK_NEAR_TOL(area(*g::boundary_network("F6", {u.front(), apex}, {})), 25.0, 1e-4);
  CHECK(throws_with([&] { g::boundary_network("F6", {u.front()}, {}); }, "two curves or more"));
}

void test_faces_are_matched() {
  // A 10 x 10 face in y = 0 matched to the bottom edge of walls 12 up
  // (20 long): its top side moves there, its bottom stays.
  const g::ShapePtr sheet = g::extrude_sheet("F7", {{line("c1", gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0))}},
                                             gp_Vec(0, 0, 0), gp_Dir(0, 0, 1), 10.0, 0.0);
  const g::ShapePtr high = walls_at("F3", 12.0, 17.0);
  for (const g::Continuity c : {g::Continuity::G0, g::Continuity::G1, g::Continuity::G2}) {
    const g::ShapePtr matched =
        g::match_surface(*sheet, "", side_edge(high, "F3:side(c1)", 14.5, false), c);
    CHECK(named(*matched, "F7:side(c1)") == 1);
    CHECK_NEAR_TOL(z_range(*matched).first, 0.0, 1e-4);
    CHECK_NEAR_TOL(z_range(*matched).second, 12.0, 1e-4);
    // Bottom 10, top 20, 12 apart.
    CHECK_NEAR_TOL(area(*matched), 180.0, 1e-3);
  }
  CHECK(throws_with([&] { g::match_surface(*walls(), "", side_edge(high, "F3:side(c1)", 14.5, false), g::Continuity::G0); },
                    "name the one to match"));
}

void test_faces_are_faired() {
  // A 40 x 40 B-spline face with its middle control point 10 up.
  NCollection_Array2<gp_Pnt> poles(1, 5, 1, 5);
  for (int i = 1; i <= 5; ++i) {
    for (int j = 1; j <= 5; ++j) {
      poles(i, j) = gp_Pnt(10.0 * (i - 1), 10.0 * (j - 1), i == 3 && j == 3 ? 10.0 : 0.0);
    }
  }
  NCollection_Array1<double> knots(1, 3);
  knots(1) = 0.0;
  knots(2) = 0.5;
  knots(3) = 1.0;
  NCollection_Array1<int> mults(1, 3);
  mults(1) = 4;
  mults(2) = 1;
  mults(3) = 4;
  const occ::handle<Geom_BSplineSurface> bump = new Geom_BSplineSurface(poles, knots, knots, mults, mults, 3, 3);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(bump, 1.0e-7).Face();
  const g::Shape sheet(face, {{face, {"F1:bump"}}});
  const double top = z_range(sheet).second;
  g::Fairing fairing;
  fairing.tolerance = 2.0;
  fairing.iterations = 20;
  const g::ShapePtr faired = g::fair_surface(sheet, {}, fairing);
  CHECK(named(*faired, "F1:bump") == 1);
  const double lower = z_range(*faired).second;
  CHECK(lower < top - 0.1);
  // The middle point moves at most the tolerance.
  CHECK(lower > top - 2.0 * 1.01);
  // Its edges stay on z = 0.
  CHECK_NEAR_TOL(z_range(*faired).first, 0.0, 1e-6);
  // Planes are fair already.
  const g::ShapePtr plain = g::fair_surface(*walls(), {}, fairing);
  CHECK_NEAR(area(*plain), area(*walls()));
  CHECK(throws_with([&] { g::fair_surface(*box(), {}, fairing); }, "solid"));
}

// --- Fill a Loop through three sketches (mitcad#2220)

// A curve of the loop as the model hands it over (one boundary entry per
// sketch, mitcad#2198), with the same curve built here from its
// definition, not by the kernel, to measure the filled face against.
struct LoopCurve {
  g::BoundaryCurve boundary;
  occ::handle<Geom_Curve> curve;
  bool planar = true; // a planar sketch's curve (the 3D sketch's is not)
};

// The two planar sketches' planes: the xy plane, and a plane parallel to
// it with an origin and axes of its own.
gp_Pln first_sketch_plane() { return gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)); }
gp_Pln second_sketch_plane() { return gp_Pln(gp_Ax3(gp_Pnt(20, 0, 0), gp_Dir(0, 0, 1), gp_Dir(-0.6, 0.8, 0))); }

g::BoundaryCurve planar_entry(const g::PathCurve& piece, const gp_Pln& plane, g::Continuity continuity) {
  g::BoundaryCurve entry;
  entry.kind = g::BoundaryCurve::Kind::Curves;
  entry.curves = {piece};
  entry.plane = plane;
  entry.continuity = continuity;
  return entry;
}

// The first sketch's arc, from (0, 0, 0) through (10, -4, 0) to (20, 0, 0):
// centre (10, 10.5, 0), radius 14.5.
LoopCurve loop_arc(g::Continuity continuity) {
  const gp_Pnt center(10, 10.5, 0);
  const double radius = 14.5;
  g::PathCurve piece;
  piece.name = "c1";
  piece.curve.kind = g::ModelCurveKind::Conic;
  piece.curve.center = center;
  piece.curve.normal = gp_Dir(0, 0, 1);
  piece.curve.x_axis = gp_Dir(1, 0, 0);
  piece.curve.major = radius;
  piece.curve.minor = radius;
  piece.curve.first = std::atan2(-10.5, -10.0);
  piece.curve.last = std::atan2(-10.5, 10.0);
  const occ::handle<Geom_Circle> circle = new Geom_Circle(gp_Ax2(center, gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), radius);
  const occ::handle<Geom_Curve> curve = new Geom_TrimmedCurve(circle, piece.curve.first, piece.curve.last);
  return {planar_entry(piece, first_sketch_plane(), continuity), curve, true};
}

// The second sketch's line from (20, 0, 0) to (10, 15, 0), or run the
// other way (a sketch's curve need not run with the loop); its first
// corner `gap` off the arc's end (in y), as two sketches' curves the model
// joins within its tolerance may end.
LoopCurve loop_line(g::Continuity continuity, bool reversed, double gap = 0.0) {
  const gp_Pnt corner(20, gap, 0);
  const gp_Pnt a = reversed ? gp_Pnt(10, 15, 0) : corner;
  const gp_Pnt b = reversed ? corner : gp_Pnt(10, 15, 0);
  const occ::handle<Geom_Line> straight = new Geom_Line(a, gp_Dir(gp_Vec(a, b)));
  const occ::handle<Geom_Curve> curve = new Geom_TrimmedCurve(straight, 0.0, a.Distance(b));
  return {planar_entry(line("c1", a, b), second_sketch_plane(), continuity), curve, true};
}

// The 3D sketch's curve from (10, 15, 0) back to the origin over a hump 3
// high: a quartic level at both ends, so that a surface tangent to the
// planar sketches' plane along their curves can also meet it.
LoopCurve loop_hump() {
  const std::array<gp_Pnt, 5> poles = {{gp_Pnt(10, 15, 0), gp_Pnt(6.5, 11.85, 0), gp_Pnt(3.5, 8.5, 8),
                                        gp_Pnt(1.5, 4.35, 0), gp_Pnt(0, 0, 0)}};
  g::PathCurve piece;
  piece.name = "c1";
  piece.curve.kind = g::ModelCurveKind::BSpline;
  piece.curve.degree = 4;
  piece.curve.poles.assign(poles.begin(), poles.end());
  piece.curve.knots = {0, 0, 0, 0, 0, 1, 1, 1, 1, 1};
  g::BoundaryCurve entry;
  entry.kind = g::BoundaryCurve::Kind::Curves;
  entry.curves = {piece};
  entry.continuity = g::Continuity::G0; // a 3D sketch has no plane to be tangent to
  NCollection_Array1<gp_Pnt> array(1, 5);
  for (int i = 0; i < 5; ++i) {
    array.SetValue(i + 1, poles[static_cast<std::size_t>(i)]);
  }
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  NCollection_Array1<int> mults(1, 2);
  mults(1) = 5;
  mults(2) = 5;
  const occ::handle<Geom_Curve> curve = new Geom_BSplineCurve(array, knots, mults, 4);
  return {entry, curve, false};
}

// The distance from p to the curve (its ends included).
double distance_to(const Adaptor3d_Curve& curve, const gp_Pnt& p) {
  gp_Pnt projected;
  double param = 0.0;
  return ShapeAnalysis_Curve().Project(curve, p, 1.0e-9, projected, param);
}

// The farthest of points evenly along `from` (by its parameter, ends
// included) from the curve `to`.
double farthest(const Adaptor3d_Curve& from, const Adaptor3d_Curve& to) {
  constexpr int kSamples = 64;
  double result = 0.0;
  for (int i = 0; i <= kSamples; ++i) {
    const double t =
        from.FirstParameter() + (from.LastParameter() - from.FirstParameter()) * static_cast<double>(i) / kSamples;
    result = std::max(result, distance_to(to, from.Value(t)));
  }
  return result;
}

// The largest angle (radians) between the face's normal and the plane's
// normal (either way) along the edge, inside its ends.
double largest_tilt(const TopoDS_Face& face, const TopoDS_Edge& edge, const gp_Dir& normal) {
  constexpr int kSamples = 64;
  const BRepAdaptor_Curve2d pcurve(edge, face);
  const BRepAdaptor_Surface surface(face);
  double result = 0.0;
  for (int i = 1; i < kSamples; ++i) {
    const double t = pcurve.FirstParameter() +
                     (pcurve.LastParameter() - pcurve.FirstParameter()) * static_cast<double>(i) / kSamples;
    const gp_Pnt2d uv = pcurve.Value(t);
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(uv.X(), uv.Y(), p, du, dv);
    const gp_Vec n = du.Crossed(dv);
    if (n.Magnitude() < 1.0e-12) {
      continue;
    }
    const double angle = n.Angle(gp_Vec(normal));
    result = std::max(result, std::min(angle, kPi - angle));
  }
  return result;
}

// The loop filled and measured: one face whose outer wire closes through
// three vertices at the curves' ends, each of its three edges on one of
// the curves and along all of it within 0.000001 mm (sampled both ways).
// Returns the largest tilt of the face against the planar sketches' plane
// along their curves' edges (negative when the fill failed).
double fill_and_measure(const std::vector<LoopCurve>& loop, const char* what) {
  constexpr double kOnCurve = 1.0e-6;
  std::vector<g::BoundaryCurve> entries;
  for (const LoopCurve& curve : loop) {
    entries.push_back(curve.boundary);
  }
  const g::ShapePtr filled = g::boundary_fill("F8", entries);
  CHECK(named(*filled, "F8:boundary") == 1);
  CHECK(filled->face_count() == 1);
  const TopExp_Explorer faces(filled->occt(), TopAbs_FACE);
  CHECK(faces.More());
  if (!faces.More()) {
    return -1.0;
  }
  const TopoDS_Face face = TopoDS::Face(faces.Current());
  CHECK(BRepCheck_Analyzer(face, true, false, true).IsValid());
  // The chains of the three sketches joined: one closed wire, three corners.
  const TopoDS_Wire wire = BRepTools::OuterWire(face);
  CHECK(!wire.IsNull() && BRep_Tool::IsClosed(wire));
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
  TopExp::MapShapes(face, TopAbs_VERTEX, vertices);
  CHECK(vertices.Extent() == 3);
  const std::array<gp_Pnt, 3> corners = {{gp_Pnt(0, 0, 0), gp_Pnt(20, 0, 0), gp_Pnt(10, 15, 0)}};
  for (int i = 1; i <= vertices.Extent(); ++i) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vertices(i)));
    double nearest = 1.0e9;
    for (const gp_Pnt& corner : corners) {
      nearest = std::min(nearest, p.Distance(corner));
    }
    CHECK(nearest <= kOnCurve);
  }
  // Each edge on the curve it lies nearest, each curve with one edge.
  std::vector<TopoDS_Edge> edge_of(loop.size());
  int edges = 0;
  double face_gap = 0.0;
  double edge_tolerance = 0.0;
  double worst = 0.0;
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    ++edges;
    const TopoDS_Edge edge = TopoDS::Edge(it.Current());
    const BRepAdaptor_Curve along(edge);
    std::size_t best = loop.size();
    double best_distance = 1.0e9;
    for (std::size_t k = 0; k < loop.size(); ++k) {
      const double d = farthest(along, GeomAdaptor_Curve(loop[k].curve));
      if (d < best_distance) {
        best_distance = d;
        best = k;
      }
    }
    worst = std::max(worst, best_distance);
    if (best == loop.size()) {
      continue;
    }
    CHECK(edge_of[best].IsNull());
    edge_of[best] = edge;
    // All of the curve along the edge, not a part of it.
    worst = std::max(worst, farthest(GeomAdaptor_Curve(loop[best].curve), along));
    // Each edge's own tolerance must cover the mismatch, regardless of
    // the tolerances of the other edges.
    const BRepAdaptor_Curve on_face(edge, face);
    constexpr int kSamples = 64;
    double gap = 0.0;
    CHECK(BRep_Tool::SameParameter(edge));
    for (int i = 0; i <= kSamples; ++i) {
      const double param = along.FirstParameter() +
                           (along.LastParameter() - along.FirstParameter()) * static_cast<double>(i) / kSamples;
      gap = std::max(gap, on_face.Value(param).Distance(along.Value(param)));
    }
    const double tolerance = BRep_Tool::Tolerance(edge);
    if (gap > tolerance) {
      std::fprintf(stderr, "  %s: edge %d gap %.9g mm exceeds its tolerance %.9g mm\n", what, edges, gap,
                   tolerance);
    }
    CHECK(gap <= tolerance);
    face_gap = std::max(face_gap, gap);
    edge_tolerance = std::max(edge_tolerance, tolerance);
  }
  CHECK(edges == 3);
  CHECK(worst <= kOnCurve);
  // Sew a sheet to the straight side. A real seam forms one shell with
  // two faces, and the joined body must pass the same geometric checks.
  const auto side = g::extrude_sheet("F9", {loop[1].boundary.curves}, gp_Vec(), gp_Dir(0, 0, -1), 5.0, 0.0);
  CHECK(BRepCheck_Analyzer(side->occt(), true, false, true).IsValid());
  const auto sewn = g::stitch({filled.get(), side.get()}, 0.0);
  CHECK(sewn->face_count() == 2);
  CHECK(sewn->occt().ShapeType() == TopAbs_SHELL);
  CHECK(BRepCheck_Analyzer(sewn->occt(), true, false, true).IsValid());
  if (worst > kOnCurve) {
    std::fprintf(stderr, "  %s: an edge lies %.3g mm from its curve\n", what, worst);
  }
  // The face against the sketches' plane along the planar curves' edges.
  double tilt = 0.0;
  for (std::size_t k = 0; k < loop.size(); ++k) {
    CHECK(!edge_of[k].IsNull());
    if (!edge_of[k].IsNull() && loop[k].planar) {
      tilt = std::max(tilt, largest_tilt(face, edge_of[k], loop[k].boundary.plane.Axis().Direction()));
    }
  }
  std::printf("  %s: edges within %.3g mm of the curves; the face along them within %.3g mm (edge tolerance %.3g), "
              "tilted at most %.3g rad to the sketch plane\n",
              what, worst, face_gap, edge_tolerance, tilt);
  return tilt;
}

// mitcad#2220 (mitcad#2198's Done when): a loop of three curves of three
// sketches, two planar (an arc and a line, on parallel planes with frames
// of their own) and one 3D, filled to one face whose edges lie on the
// curves within 0.000001 mm, without continuity and with Tangent to Sketch
// Plane on the planar entries, also with a curve running against the loop
// and with two sketches' curves ending 0.0000005 mm apart (within the
// model's 0.00002 mm here, wider than the 0.0000001 mm the filling joined
// edges of different sketches within before).
void test_a_loop_through_three_sketches_fills_to_edges_on_its_curves() {
  const g::Continuity g0 = g::Continuity::G0;
  const g::Continuity g1 = g::Continuity::G1;
  const double plain = fill_and_measure({loop_arc(g0), loop_line(g0, false), loop_hump()}, "G0");
  const double tangent = fill_and_measure({loop_arc(g1), loop_line(g1, false), loop_hump()}, "G1");
  const double reversed = fill_and_measure({loop_arc(g1), loop_line(g1, true), loop_hump()}, "G1, line reversed");
  const double apart =
      fill_and_measure({loop_arc(g1), loop_line(g1, false, 5.0e-7), loop_hump()}, "G1, ends 5e-7 mm apart");
  // Without continuity the face rises off the plane towards the hump;
  // tangent to it, it leaves the planar curves level.
  CHECK(plain > 0.0);
  CHECK(tangent >= 0.0 && tangent < 0.5 * plain);
  CHECK(reversed >= 0.0 && reversed < 0.5 * plain);
  CHECK(apart >= 0.0 && apart < 0.5 * plain);
}

// --- A network whose curves run on past the outer crossing curves
// (mitcad#2333, GitHub #609)

// The arc of the cylinder of radius 25 about the line y = 10, z = -20 in
// the plane x = `x`, from y = 25 to y = -5 (z 0 at both ends, 5 at the top).
g::BoundaryCurve cylinder_arc(const std::string& name, double x) {
  g::PathCurve piece;
  piece.name = name;
  piece.curve.kind = g::ModelCurveKind::Conic;
  piece.curve.center = gp_Pnt(x, 10, -20);
  piece.curve.normal = gp_Dir(1, 0, 0);
  piece.curve.x_axis = gp_Dir(0, 1, 0);
  piece.curve.major = 25.0;
  piece.curve.minor = 25.0;
  piece.curve.first = std::acos(0.6);
  piece.curve.last = std::acos(-0.6);
  g::BoundaryCurve curve;
  curve.kind = g::BoundaryCurve::Kind::Curves;
  curve.curves = {piece};
  curve.plane = gp_Pln(gp_Pnt(x, 0, 0), gp_Dir(1, 0, 0));
  return curve;
}

// Three lines one way on that cylinder (y = 0, 10 and 20, from x = -5 to
// 25) and two arcs across it (x = 0 and 20): every curve runs on 5 mm past
// the outer curves of the other way. `lift` raises the middle line off its
// crossing with the second arc (by `lift` at x = 20, still meeting the
// first arc at x = 0).
std::pair<std::vector<g::BoundaryCurve>, std::vector<g::BoundaryCurve>> overhanging_grid(double lift) {
  const double h = std::sqrt(525.0) - 20.0; // the lines at y = 0 and 20
  const auto middle = [lift](double x) { return 5.0 + lift * x / 20.0; };
  return {{sketch_curve("c1", gp_Pnt(-5, 0, h), gp_Pnt(25, 0, h)),
           sketch_curve("c2", gp_Pnt(-5, 10, middle(-5)), gp_Pnt(25, 10, middle(25))),
           sketch_curve("c3", gp_Pnt(-5, 20, h), gp_Pnt(25, 20, h))},
          {cylinder_arc("c4", 0.0), cylinder_arc("c5", 20.0)}};
}

double distance_to_shape(const gp_Pnt& p, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), shape);
  return distance.IsDone() ? distance.Value() : 1.0e9;
}

void test_a_network_runs_between_the_outer_crossings_of_curves_that_run_on() {
  const auto [u, v] = overhanging_grid(0.0);
  const g::ShapePtr grid = g::boundary_network("F6", u, v);
  CHECK(named(*grid, "F6:boundary") == 1);
  const TopoDS_Shape& face = grid->occt();
  // The cylinder between the outer curves: 20 along it, an angle of
  // 2 asin(10 / 25) round it; the ends past them left off.
  CHECK_NEAR_TOL(area(*grid), 20.0 * 25.0 * 2.0 * std::asin(0.4), 1e-4);
  Bnd_Box box;
  BRepBndLib::AddOptimal(face, box, false, false);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  CHECK(std::abs(x0) < 1e-3 && std::abs(x1 - 20.0) < 1e-3);
  CHECK(std::abs(y0) < 1e-3 && std::abs(y1 - 20.0) < 1e-3);
  // Each of the five curves lies on the face between its outer crossings.
  const auto height = [](double y) { return -20.0 + std::sqrt(625.0 - (y - 10.0) * (y - 10.0)); };
  double worst = 0.0;
  for (int i = 0; i <= 20; ++i) {
    const double step = 20.0 * i / 20;
    for (const double y : {0.0, 10.0, 20.0}) {
      worst = std::max(worst, distance_to_shape(gp_Pnt(step, y, height(y)), face));
    }
    for (const double x : {0.0, 20.0}) {
      worst = std::max(worst, distance_to_shape(gp_Pnt(x, step, height(step)), face));
    }
  }
  CHECK(worst <= 1e-3);
  if (worst > 1e-3) {
    std::fprintf(stderr, "a curve lies %g mm off the network's face\n", worst);
  }
}

void test_a_network_names_the_pair_that_does_not_meet() {
  const auto [u, v] = overhanging_grid(0.5);
  try {
    g::boundary_network("F6", u, v);
    CHECK(false);
  } catch (const std::invalid_argument& error) {
    const std::string text = error.what();
    // The person reads which kind of problem it is, in words.
    CHECK(g::error_message(text) == "boundary surface: a curve one way does not meet a crossing curve (every curve "
                                    "one way must cross every curve the other way)");
    // Agents read the pair, its gap (0.5 across the line's slope) and
    // where.
    CHECK(text.find("curve one way 2 (c2) and crossing curve 2 (c5) do not meet: 0.4998 mm apart") !=
          std::string::npos);
    CHECK(text.find("crossing curve 1 (c4) do not") == std::string::npos);
    // Both curves painted: nine points along each and the nearest two.
    std::size_t points = 0;
    for (std::size_t at = text.find("\"kind\":\"point\""); at != std::string::npos;
         at = text.find("\"kind\":\"point\"", at + 1)) {
      ++points;
    }
    CHECK(points == 20);
    if (text.find("crossing curve 2 (c5) do not meet") == std::string::npos) {
      std::fprintf(stderr, "unexpected error: %s\n", text.c_str());
    }
  }
}

} // namespace
} // namespace test

int main() {
  mitcad::geometry::enable_input_checks(true);
  test::guarded("test_open_curves_are_extruded_into_a_surface", test::test_open_curves_are_extruded_into_a_surface);
  test::guarded("test_curves_are_revolved_into_a_surface", test::test_curves_are_revolved_into_a_surface);
  test::guarded("test_curves_are_swept_along_a_path", test::test_curves_are_swept_along_a_path);
  test::guarded("test_sections_of_curves_are_lofted", test::test_sections_of_curves_are_lofted);
  test::guarded("test_a_solid_extrusion_loses_its_caps", test::test_a_solid_extrusion_loses_its_caps);
  test::guarded("test_faces_are_offset_into_a_surface", test::test_faces_are_offset_into_a_surface);
  test::guarded("test_strips_are_ruled_off_edges", test::test_strips_are_ruled_off_edges);
  test::guarded("test_surfaces_are_extended", test::test_surfaces_are_extended);
  test::guarded("test_surfaces_are_trimmed", test::test_surfaces_are_trimmed);
  test::guarded("test_faces_are_untrimmed", test::test_faces_are_untrimmed);
  test::guarded("test_faces_come_apart_and_turn_over", test::test_faces_come_apart_and_turn_over);
  test::guarded("test_chains_are_blended", test::test_chains_are_blended);
  test::guarded("test_boundaries_are_filled", test::test_boundaries_are_filled);
  test::guarded("test_networks_are_spanned", test::test_networks_are_spanned);
  test::guarded("test_faces_are_matched", test::test_faces_are_matched);
  test::guarded("test_faces_are_faired", test::test_faces_are_faired);
  test::guarded("test_a_trimmed_plane_is_extended_past_one_edge", test::test_a_trimmed_plane_is_extended_past_one_edge);
  test::guarded("test_a_patch_closes_a_loop_of_free_and_named_edges",
                test::test_a_patch_closes_a_loop_of_free_and_named_edges);
  test::guarded("test_a_patch_closes_free_edges_with_a_sketch_line",
                test::test_a_patch_closes_free_edges_with_a_sketch_line);
  test::guarded("test_a_trimmed_plane_is_extended_round_a_corner", test::test_a_trimmed_plane_is_extended_round_a_corner);
  test::guarded("test_a_trimmed_plane_falls_back_to_square_sides",
                test::test_a_trimmed_plane_falls_back_to_square_sides);
  test::guarded("test_a_loop_through_three_sketches_fills_to_edges_on_its_curves",
                test::test_a_loop_through_three_sketches_fills_to_edges_on_its_curves);
  test::guarded("test_a_network_runs_between_the_outer_crossings_of_curves_that_run_on",
                test::test_a_network_runs_between_the_outer_crossings_of_curves_that_run_on);
  test::guarded("test_a_network_names_the_pair_that_does_not_meet",
                test::test_a_network_names_the_pair_that_does_not_meet);
  for (const std::string& change : mitcad::geometry::input_changes()) {
    std::fprintf(stderr, "an input changed: %s\n", change.c_str());
    ++test::failures;
  }
  if (test::failures == 0) {
    std::puts("test_surface_modelling: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
