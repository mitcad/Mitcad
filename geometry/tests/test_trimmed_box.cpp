// SPDX-License-Identifier: MIT
// Boxes over the trimmed faces, not their surfaces' parameter rectangles
// (mitcad#2020).
//
// Synthetic faces on a degree-2 Bezier surface whose x and y are its
// parameters u and v (so a face's extent in x and y is its parameter
// domain's), with known extents:
// - a triangle in a corner of the surface's square on z = u^2 + v^2: the
//   surface beyond the triangle's long side rises higher than the face,
//   its extremes all lie on the boundary; also with the face flagged as
//   its surface's natural restriction, where OCCT's optimal box is that of
//   the whole surface;
// - a diamond on the cap z = 4u(1-u) + 4v(1-v): its top (2 at the centre)
//   lies inside the face, above every edge; also moved by a location.
// Each is boxed by trimmed_face_box, trimmed_box, bounding_box and
// face_bounding_boxes, within 1e-6 of the exact extent.
//
// A planar face of 120 edges (a gear's outline) on a tilted plane: a
// ruled face, boxed by its boundary without a search inside it, which on
// such a face took seconds (0.66.0-rc1); within 1e-6 of its corners'
// extent and in well under a second.
//
// The case `fillet-corner-walk-over-vertex` of MITCAD_GEOMETRY_CASES (the
// reported bent guide's body before its rounding at bend radii 24 and 30,
// see test_fillet_corner_walk_over_vertex.cpp): the body itself and the
// body with the rib's sloped edge 1 degree from the bend's end rounded by
// 0.5 (whose corner is a plate, trimmed from a larger surface). The box
// must hold every node of a fine triangulation of a copy (deflection
// 2e-4) and reach no more than 1e-3 past them on any side: the trimmed
// extent within 1e-3 mm. OCCT's optimal box is printed beside it.

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepLib.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomAbs_Shape.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_BezierSurface.hxx>
#include <NCollection_Array2.hxx>
#include <Poly_Triangulation.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/trimmed_box.hpp"

namespace {

using mitcad::geometry::BoundingBox;
using mitcad::geometry::Shape;

int failures = 0;

void check(bool ok, const std::string& what) {
  if (!ok) {
    std::fprintf(stderr, "check failed: %s\n", what.c_str());
    ++failures;
  }
}

// xmin, ymin, zmin, xmax, ymax, zmax.
using Sides = std::array<double, 6>;

const char* const kSideNames[6] = {"xmin", "ymin", "zmin", "xmax", "ymax", "zmax"};

Sides sides_of(const Bnd_Box& box) {
  Sides s{};
  box.Get(s[0], s[1], s[2], s[3], s[4], s[5]);
  return s;
}

Sides sides_of(const BoundingBox& box) {
  return {box.min.X(), box.min.Y(), box.min.Z(), box.max.X(), box.max.Y(), box.max.Z()};
}

void print_sides(const char* name, const Sides& s) {
  std::printf("  %-28s x [%.6f, %.6f] y [%.6f, %.6f] z [%.6f, %.6f]\n", name, s[0], s[3], s[1], s[4], s[2], s[5]);
}

void check_near(const Sides& got, const Sides& want, double tolerance, const std::string& what) {
  for (std::size_t k = 0; k < 6; ++k) {
    check(std::abs(got[k] - want[k]) <= tolerance,
          what + ": " + kSideNames[k] + " " + std::to_string(got[k]) + ", not " + std::to_string(want[k]));
  }
}

// The degree-2 Bezier surface x = u, y = v, z = a(u) + a(v), where a is
// the quadratic with Bernstein coefficients `a` on [0, 1].
occ::handle<Geom_BezierSurface> surface(const std::array<double, 3>& a) {
  NCollection_Array2<gp_Pnt> poles(1, 3, 1, 3);
  for (int i = 0; i < 3; ++i) {
    for (int j = 0; j < 3; ++j) {
      poles.SetValue(i + 1, j + 1,
                     gp_Pnt(0.5 * i, 0.5 * j, a[static_cast<std::size_t>(i)] + a[static_cast<std::size_t>(j)]));
    }
  }
  return new Geom_BezierSurface(poles);
}

// The face of `s` within the polygon of parameter points `corners`
// (counter-clockwise), its edges' curves in space built from their
// parameter lines.
TopoDS_Face polygon_face(const occ::handle<Geom_BezierSurface>& s, const std::vector<gp_Pnt2d>& corners) {
  BRepBuilderAPI_MakeWire wire;
  for (std::size_t k = 0; k < corners.size(); ++k) {
    const gp_Pnt2d& a = corners[k];
    const gp_Pnt2d& b = corners[(k + 1) % corners.size()];
    const occ::handle<Geom2d_Line> line = new Geom2d_Line(a, gp_Dir2d(b.XY() - a.XY()));
    wire.Add(BRepBuilderAPI_MakeEdge(line, s, 0.0, a.Distance(b)).Edge());
  }
  TopoDS_Face face = BRepBuilderAPI_MakeFace(s, wire.Wire(), true).Face();
  BRepLib::BuildCurves3d(face, 1.0e-7, GeomAbs_C1, 14, 0);
  return face;
}

// Every box of `face` within 1e-6 of `want`.
void check_face(const char* name, const TopoDS_Face& face, const Sides& want) {
  std::printf("%s\n", name);
  Bnd_Box occt;
  BRepBndLib::AddOptimal(face, occt, false, false);
  print_sides("OCCT's optimal box", sides_of(occt));
  const Sides trimmed = sides_of(mitcad::geometry::trimmed_face_box(face));
  print_sides("the trimmed face's box", trimmed);
  print_sides("its extent", want);
  check_near(trimmed, want, 1e-6, std::string(name) + ", trimmed_face_box");
  check_near(sides_of(mitcad::geometry::trimmed_box(face)), want, 1e-6, std::string(name) + ", trimmed_box");
  const Shape shape(face);
  check_near(sides_of(mitcad::geometry::bounding_box(shape)), want, 1e-6, std::string(name) + ", bounding_box");
  const std::vector<BoundingBox> boxes = mitcad::geometry::face_bounding_boxes(shape, {0});
  check(boxes.size() == 1 && !boxes[0].empty, std::string(name) + ": one face box");
  if (boxes.size() == 1 && !boxes[0].empty) {
    check_near(sides_of(boxes[0]), want, 1e-6, std::string(name) + ", face_bounding_boxes");
  }
}

void test_synthetic() {
  // z = u^2 + v^2: its extremes on the triangle are at the corners; the
  // surface's square reaches z = 2 at (1, 1), its parameter rectangle
  // [0, 0.5]^2 z = 0.5 at (0.5, 0.5).
  const occ::handle<Geom_BezierSurface> bowl = surface({0.0, 0.0, 1.0});
  const TopoDS_Face triangle = polygon_face(bowl, {gp_Pnt2d(0, 0), gp_Pnt2d(0.5, 0), gp_Pnt2d(0, 0.5)});
  const Sides corner = {0.0, 0.0, 0.0, 0.5, 0.5, 0.25};
  check_face("a triangle in the corner of a bowl", triangle, corner);
  // Flagged as its surface's natural restriction: OCCT boxes the whole
  // surface.
  TopoDS_Face flagged = triangle;
  BRep_Builder().NaturalRestriction(flagged, true);
  check_face("the triangle flagged as its surface's natural restriction", flagged, corner);

  // z = 4u(1-u) + 4v(1-v): its top 2 at the diamond's centre; its lowest
  // 1.19 at the diamond's corners.
  const occ::handle<Geom_BezierSurface> cap = surface({0.0, 2.0, 0.0});
  const TopoDS_Face diamond =
      polygon_face(cap, {gp_Pnt2d(0.5, 0.05), gp_Pnt2d(0.95, 0.5), gp_Pnt2d(0.5, 0.95), gp_Pnt2d(0.05, 0.5)});
  check_face("a diamond around the top of a cap", diamond, {0.05, 0.05, 1.19, 0.95, 0.95, 2.0});
  gp_Trsf move;
  move.SetTranslation(gp_Vec(10.0, -20.0, 5.0));
  const TopoDS_Face moved = TopoDS::Face(diamond.Moved(TopLoc_Location(move)));
  check_face("the diamond moved", moved, {10.05, -19.95, 6.19, 10.95, -19.05, 7.0});
}

void test_ruled() {
  // 60 teeth: corners alternately at radius 20 and 22, the plane turned
  // 30 degrees about a slanted axis, so that its surface's parameter box
  // reaches past the outline.
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(1, 2, 3)), 30.0 * M_PI / 180.0);
  BRepBuilderAPI_MakePolygon polygon;
  Sides want = {1e100, 1e100, 1e100, -1e100, -1e100, -1e100};
  for (int k = 0; k < 120; ++k) {
    const double radius = k % 2 == 0 ? 20.0 : 22.0;
    const double angle = 2.0 * M_PI * k / 120.0;
    const gp_Pnt p = gp_Pnt(radius * std::cos(angle), radius * std::sin(angle), 5.0).Transformed(turn);
    polygon.Add(p);
    for (int axis = 0; axis < 3; ++axis) {
      const auto a = static_cast<std::size_t>(axis);
      want[a] = std::min(want[a], p.Coord(axis + 1));
      want[a + 3] = std::max(want[a + 3], p.Coord(axis + 1));
    }
  }
  polygon.Close();
  const TopoDS_Face gear = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  const auto start = std::chrono::steady_clock::now();
  check_face("a gear's outline on a tilted plane", gear, want);
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("  boxed four ways in %.3f s\n", seconds);
  check(seconds < 1.0, "the gear's outline boxed in under a second, not " + std::to_string(seconds) + " s");
}

std::string cases_dir() {
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

// The straight edge from the flat (z = 1.1) up to z = 2.2 whose lower end
// lies at `foot`, or a null edge.
TopoDS_Edge sloped_edge(const TopoDS_Shape& body, const gp_Pnt& foot) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    const BRepAdaptor_Curve c(e);
    if (c.GetType() != GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    const gp_Pnt& low = a.Z() < b.Z() ? a : b;
    const gp_Pnt& high = a.Z() < b.Z() ? b : a;
    if (low.Distance(foot) < 1e-3 && std::abs(high.Z() - 2.2) < 1e-6) {
      return e;
    }
  }
  return TopoDS_Edge();
}

// The edge rounded with 0.5 by OCCT's fillet; a null shape when it fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, const TopoDS_Edge& edge) {
  BRepFilletAPI_MakeFillet maker(body);
  maker.Add(0.5, edge);
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The box of the nodes of a triangulation of a copy of `shape` with the
// deflection `deflection`: points of its trimmed faces.
Sides mesh_nodes(const TopoDS_Shape& shape, double deflection) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(shape, true, false).Shape();
  const BRepMesh_IncrementalMesh mesh(copy, deflection, false, 0.2, true);
  check(mesh.IsDone(), "the copy is triangulated");
  Bnd_Box box;
  for (TopExp_Explorer it(copy, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> triangles = BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location);
    check(!triangles.IsNull(), "every face is triangulated");
    if (triangles.IsNull()) {
      continue;
    }
    const gp_Trsf place = location.Transformation();
    for (int i = 1; i <= triangles->NbNodes(); ++i) {
      box.Add(triangles->Node(i).Transformed(place));
    }
  }
  return sides_of(box);
}

// The body's box holds the triangulation's nodes and reaches at most 1e-3
// past them.
void check_against_mesh(const std::string& name, const TopoDS_Shape& body) {
  std::printf("%s\n", name.c_str());
  Bnd_Box occt;
  BRepBndLib::AddOptimal(body, occt, false, false);
  print_sides("OCCT's optimal box", sides_of(occt));
  const Shape shape(body);
  const BoundingBox found = mitcad::geometry::bounding_box(shape);
  check(!found.empty, name + ": a box");
  if (found.empty) {
    return;
  }
  const Sides box = sides_of(found);
  print_sides("the trimmed faces' box", box);
  const Sides nodes = mesh_nodes(body, 2e-4);
  print_sides("a triangulation's nodes", nodes);
  for (std::size_t k = 0; k < 6; ++k) {
    // Outwards positive: past the nodes on this side.
    const double past = k < 3 ? nodes[k] - box[k] : box[k] - nodes[k];
    check(past >= -1e-6, name + ": the box holds the faces' points at " + kSideNames[k] + " (" +
                             std::to_string(past) + " past them)");
    check(past <= 1e-3, name + ": the box reaches " + std::to_string(past) + " past the faces' points at " +
                            kSideNames[k]);
  }
}

void test_case(const char* file, const gp_Pnt& foot) {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("%s: skipped (MITCAD_GEOMETRY_CASES is not set)\n", file);
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  const std::string path = dir + "/fillet-corner-walk-over-vertex/" + file;
  if (!BRepTools::Read(body, path.c_str(), builder) || body.IsNull()) {
    std::printf("%s: skipped (no case fillet-corner-walk-over-vertex)\n", file);
    return;
  }
  check_against_mesh(std::string(file) + ", the body", body);
  const TopoDS_Edge edge = sloped_edge(body, foot);
  check(!edge.IsNull(), std::string(file) + ": the rib's sloped edge is found");
  if (edge.IsNull()) {
    return;
  }
  const TopoDS_Shape result = rounded(body, edge);
  if (result.IsNull() || !BRepCheck_Analyzer(result).IsValid()) {
    // The rounding is geometry.fillet_corner_walk_over_vertex's (OCCT port
    // patches 1801 and 1802); without it there is no corner plate to box.
    std::printf("%s: the rounding gave no valid body here: its box is not checked\n", file);
    return;
  }
  check_against_mesh(std::string(file) + ", the rib's edge rounded", result);
}

} // namespace

int main() {
  test_synthetic();
  test_ruled();
  test_case("bend-24.brep", gp_Pnt(-25.901310, 0.452109, 1.1));
  test_case("bend-30.brep", gp_Pnt(-28.090465, 0.490321, 1.1));
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("ok\n");
  return 0;
}
