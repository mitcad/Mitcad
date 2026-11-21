// SPDX-License-Identifier: MIT
// Port patch 1730: the end of an analytic blend's contact found on an arc
// is put at a vertex of that arc only where it lies within that vertex's
// own tolerance. Patch 0692 (FillSD, a contact end hatched on an arc away
// from its face's boundary) compared each vertex with the larger tolerance
// of the arc's two vertices, so a wide vertex at the arc's other end drew
// the contact to the near vertex from as far away.
//
// The case: a contact that crosses an arc in the middle of the spine,
// where one analytic piece ends and the next begins (the reported chamfer
// ran from the pit's 45 degree face onto a rounding there). A 10 cube made
// of two halves fused (x from 0 to 5 and from 5 to 10): its top, front,
// back and bottom are each two faces, the top's two meeting on the arc
// S from V = (5, 0, 10) to W = (5, 10, 10), and the front's top edge is two
// edges meeting at V, one chain. W has a tolerance of 0.7 (as a corner
// patch's vertex of an earlier rounding can have); every other vertex is
// tight. That edge chain chamfered or rounded with 0.5: one analytic piece
// per edge (SplitKPart), the contact on the top running at y = 0.5 and
// crossing S at (5, 0.5, 10), 0.5 from V and 9.5 from W, inside S
// (INTERNAL). FillSD's snap there compared 0.5 with max(tol V, tol W) =
// 0.7 and put both pieces' ends at V, the spine's vertex the blend takes
// away: S was cut at V rather than at the contact, the contact's end 0.5
// from its vertex. With the patch each vertex counts with its own
// tolerance (1e-7 at V, 0.7 at W, 9.5 away): the ends stay on S at
// (5, 0.5, 10). Checked: valid, a vertex at (5, 0.5, 10), none at V, the
// exact volume (the cube less the section along its length 10).
//
// The boundary: W of tolerance 0.45, under the contact's 0.5 from V, which
// the snap before the patch did not reach either; the same result.
//
// A second form, at the spine's end: a plain cube whose top corner
// (0, 10, 10) has the tolerance 0.7, its front top edge chamfered and
// rounded, the contact on the top ending 0.5 from (0, 0, 10). There the
// snap drew the end to the spine's end vertex as well, but the corner at
// that vertex builds the end from the face at the end anew, so that form
// passes without the patch too; it is kept as a witness that the patch
// leaves such ends as they were.

#include <cmath>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Vertex.hxx>

#include "port_test.hpp"

namespace {

constexpr double kSide = 10.0;
constexpr double kHalf = 5.0;
constexpr double kSize = 0.5;
constexpr double kPi   = 3.14159265358979323846;

// The shape (copied) with the tolerance of its vertex at p set.
TopoDS_Shape with_vertex_tolerance(const TopoDS_Shape& shape, const gp_Pnt& p, double tolerance) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(shape).Shape();
  BRep_Builder       builder;
  int                found = 0;
  for (TopExp_Explorer v(copy, TopAbs_VERTEX); v.More(); v.Next()) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(v.Current());
    if (BRep_Tool::Pnt(vertex).Distance(p) < 1e-9) {
      builder.UpdateVertex(vertex, tolerance);
      ++found;
    }
  }
  CHECK(found > 0);
  return copy;
}

// The 10 cube made of two halves fused at x = 5, W = (5, 10, 10) with the
// given tolerance.
TopoDS_Shape halves_with_wide_corner(double tolerance) {
  const TopoDS_Shape left  = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(kHalf, kSide, kSide)).Shape();
  const TopoDS_Shape right = BRepPrimAPI_MakeBox(gp_Pnt(kHalf, 0, 0), gp_Pnt(kSide, kSide, kSide)).Shape();
  const TopoDS_Shape fused = BRepAlgoAPI_Fuse(left, right).Shape();
  return with_vertex_tolerance(fused, gp_Pnt(kHalf, kSide, kSide), tolerance);
}

// The plain cube, its top corner (0, 10, 10) with the given tolerance.
TopoDS_Shape cube_with_wide_corner(double tolerance) {
  return with_vertex_tolerance(BRepPrimAPI_MakeBox(kSide, kSide, kSide).Shape(), gp_Pnt(0, kSide, kSide),
                               tolerance);
}

// The front top edge (y = 0, z = 10) whose middle lies in x below `below`:
// the whole edge of the cube, the left half's of the fused halves (the
// chain goes on along the right half's by tangency).
std::vector<TopoDS_Edge> front_top_edge(const TopoDS_Shape& body, double below) {
  const std::vector<TopoDS_Edge> edges = port::edges_where(body, [below](const gp_Pnt& p) {
    return std::abs(p.Y()) < 1e-9 && std::abs(p.Z() - kSide) < 1e-9 && p.X() < below;
  });
  CHECK(edges.size() == 1);
  return edges;
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

// The chamfer's section: the triangle 0.5 by 0.5.
constexpr double kChamferSection = 0.5 * kSize * kSize;

// The rounding's section: the square less the quarter disc.
double rounding_section() {
  return kSize * kSize * (1.0 - kPi / 4.0);
}

// The result at x = x0 (the arc S at x = 5, or the cube's end at x = 0):
// the contact on the top 0.5 from the edge's vertex there, the one on the
// front 0.5 below it, no vertex left at the edge's vertex; the cube's
// volume less the section along the length 10.
void check_ends_and_volume(const TopoDS_Shape& result, double x0, double section, const char* what) {
  std::printf("%s\n", what);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(has_vertex_at(result, gp_Pnt(x0, kSize, kSide)));
  CHECK(has_vertex_at(result, gp_Pnt(x0, 0, kSide - kSize)));
  CHECK(!has_vertex_at(result, gp_Pnt(x0, 0, kSide)));
  CHECK_NEAR_TOL(port::volume_of(result), kSide * kSide * kSide - section * kSide, 1e-7);
}

void test_chamfer_crossing_an_arc_beside_a_wide_vertex_stays_on_it() {
  const TopoDS_Shape body = halves_with_wide_corner(0.7);
  check_ends_and_volume(port::chamfered(body, front_top_edge(body, kHalf), kSize), kHalf, kChamferSection,
                        "two halves, chamfer 0.5, the far end of the arc between the tops of tolerance 0.7");
}

void test_rounding_crossing_an_arc_beside_a_wide_vertex_stays_on_it() {
  const TopoDS_Shape body = halves_with_wide_corner(0.7);
  check_ends_and_volume(port::rounded(body, front_top_edge(body, kHalf), kSize), kHalf, rounding_section(),
                        "two halves, rounding 0.5, the far end of the arc between the tops of tolerance 0.7");
}

// The boundary: W's tolerance 0.45, under the 0.5 between the contact and
// V, as the snap before the patch already left it.
void test_crossing_an_arc_beside_a_vertex_under_the_distance() {
  const TopoDS_Shape body = halves_with_wide_corner(0.45);
  check_ends_and_volume(port::chamfered(body, front_top_edge(body, kHalf), kSize), kHalf, kChamferSection,
                        "two halves, chamfer 0.5, the arc's far end of tolerance 0.45");
  check_ends_and_volume(port::rounded(body, front_top_edge(body, kHalf), kSize), kHalf, rounding_section(),
                        "two halves, rounding 0.5, the arc's far end of tolerance 0.45");
}

// At the spine's end (passes without the patch too: the corner there
// builds the end anew).
void test_chamfer_and_rounding_ending_beside_a_wide_vertex() {
  const TopoDS_Shape cube = cube_with_wide_corner(0.7);
  check_ends_and_volume(port::chamfered(cube, front_top_edge(cube, kSide), kSize), 0.0, kChamferSection,
                        "cube, chamfer 0.5, the top's far corner of tolerance 0.7");
  check_ends_and_volume(port::rounded(cube, front_top_edge(cube, kSide), kSize), 0.0, rounding_section(),
                        "cube, rounding 0.5, the top's far corner of tolerance 0.7");
}

} // namespace

MITCAD_PORT_TEST("1730", test_chamfer_crossing_an_arc_beside_a_wide_vertex_stays_on_it);
MITCAD_PORT_TEST("1730", test_rounding_crossing_an_arc_beside_a_wide_vertex_stays_on_it);
MITCAD_PORT_TEST("1730", test_crossing_an_arc_beside_a_vertex_under_the_distance);
MITCAD_PORT_TEST("1730", test_chamfer_and_rounding_ending_beside_a_wide_vertex);
