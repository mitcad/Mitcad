// SPDX-License-Identifier: MIT
// Port patch 1730: the end of an analytic blend's contact found on an arc
// is put at a vertex of that arc only where it lies within that vertex's
// own tolerance. Patch 0692 compared each vertex with the larger tolerance
// of the arc's two vertices, so a wide vertex at the arc's other end drew
// the contact to the near vertex from as far away.
//
// A cube 10 x 10 x 10 whose top corner W = (0, 10, 10) has a tolerance of
// 0.7 (as a corner patch's vertex of a kernel's earlier rounding can have);
// every other vertex is tight. Its edge from V = (0, 0, 10) to (10, 0, 10)
// chamfered with 0.5 or rounded with 0.5: the contact on the top ends on
// the top's edge from V to W, 0.5 from V and 9.5 from W. Before the patch
// it was put at V (0.5 <= 0.7); now it stays where it is, the result is
// valid, has a vertex at (0, 0.5, 10) and none at V, and its volume is the
// exact one. The same cube with W tight is the control.

#include <cmath>

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Vertex.hxx>

#include "port_test.hpp"

namespace {

constexpr double kSide = 10.0;
constexpr double kSize = 0.5;
constexpr double kPi   = 3.14159265358979323846;

// The cube, its top corner W = (0, 10, 10) with the given tolerance.
TopoDS_Shape cube_with_wide_corner(double tolerance) {
  const TopoDS_Shape box  = BRepPrimAPI_MakeBox(kSide, kSide, kSide).Shape();
  const TopoDS_Shape cube = BRepBuilderAPI_Copy(box).Shape();
  BRep_Builder       builder;
  for (TopExp_Explorer v(cube, TopAbs_VERTEX); v.More(); v.Next()) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(v.Current());
    if (BRep_Tool::Pnt(vertex).Distance(gp_Pnt(0, kSide, kSide)) < 1e-9) {
      builder.UpdateVertex(vertex, tolerance);
    }
  }
  return cube;
}

// The cube's edge from (0, 0, 10) to (10, 0, 10).
std::vector<TopoDS_Edge> front_top_edge(const TopoDS_Shape& cube) {
  return port::edges_where(cube, [](const gp_Pnt& p) {
    return std::abs(p.Y()) < 1e-9 && std::abs(p.Z() - kSide) < 1e-9;
  });
}

// Whether the shape has a vertex within 1e-6 of p.
bool has_vertex_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p) < 1e-6) {
      return true;
    }
  }
  return false;
}

// The result's ends at x = 0: the contact on the top 0.5 from V, the one on
// the front 0.5 below V, no vertex left at V; the cube's volume less the
// section taken along the edge's length.
void check_ends_and_volume(const TopoDS_Shape& result, double section, const char* what) {
  std::printf("%s\n", what);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(has_vertex_at(result, gp_Pnt(0, kSize, kSide)));
  CHECK(has_vertex_at(result, gp_Pnt(0, 0, kSide - kSize)));
  CHECK(!has_vertex_at(result, gp_Pnt(0, 0, kSide)));
  CHECK_NEAR_TOL(port::volume_of(result), kSide * kSide * kSide - section * kSide, 1e-7);
}

// The chamfer's section: the triangle 0.5 by 0.5.
constexpr double kChamferSection = 0.5 * kSize * kSize;

// The rounding's section: the square less the quarter disc.
double rounding_section() {
  return kSize * kSize * (1.0 - kPi / 4.0);
}

void test_chamfer_end_beside_a_wide_vertex_stays_on_the_arc() {
  const TopoDS_Shape cube = cube_with_wide_corner(0.7);
  check_ends_and_volume(port::chamfered(cube, front_top_edge(cube), kSize), kChamferSection,
                        "chamfer 0.5, the top's far corner of tolerance 0.7");
}

void test_rounding_end_beside_a_wide_vertex_stays_on_the_arc() {
  const TopoDS_Shape cube = cube_with_wide_corner(0.7);
  check_ends_and_volume(port::rounded(cube, front_top_edge(cube), kSize), rounding_section(),
                        "rounding 0.5, the top's far corner of tolerance 0.7");
}

// The control: every vertex tight, as before the patch.
void test_chamfer_and_rounding_of_a_tight_cube() {
  const TopoDS_Shape cube = cube_with_wide_corner(1e-7);
  check_ends_and_volume(port::chamfered(cube, front_top_edge(cube), kSize), kChamferSection,
                        "chamfer 0.5, every vertex tight");
  check_ends_and_volume(port::rounded(cube, front_top_edge(cube), kSize), rounding_section(),
                        "rounding 0.5, every vertex tight");
}

} // namespace

MITCAD_PORT_TEST("1730", test_chamfer_end_beside_a_wide_vertex_stays_on_the_arc);
MITCAD_PORT_TEST("1730", test_rounding_end_beside_a_wide_vertex_stays_on_the_arc);
MITCAD_PORT_TEST("1730", test_chamfer_and_rounding_of_a_tight_cube);
