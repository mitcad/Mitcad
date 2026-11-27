// SPDX-License-Identifier: MIT
// Port patch 2494 (mitcad#2494, GitHub mitcad/Mitcad#645): a rounding whose
// sides are chains of edges vanishes beside a removed face, and the walls of
// a slot, which meet tangentially, are extended to the removed face across
// their vanishing roundings.
//
// Shelled inside with the top open by w = 3 mm, thicker than the r = 2 mm
// rounding of the top's edges, Sharp (the intersection join) and Rounded
// (the arc join):
// - a box 40 x 30 x 20, its four top edges rounded, the edge where one of
//   the long roundings meets the top split in two by a vertex: that rounding
//   has five edges, one side a chain of two. Before, only roundings of four
//   edges were found vanishing; this one was kept, its offset turned inside
//   out and the offsets did not close (no result).
// - a slot 50 x 20 (two half-cylinders tangent to two planes), 15 high, its
//   top edges rounded: the walls' offsets meet the top across the vanishing
//   roundings, and the offsets of the edges where they meet (tangential
//   edges ending at the roundings, 2 mm under the top) were cut within their
//   offsets instead of being extended to the top: the walls' offsets got no
//   wire there (no result).
// Expected: one valid solid, the body less the cavity: the body offset
// inside by w with its convex edges sharp (the roundings vanish), up to the
// top's plane, as the intersection join and the arc join give alike for a
// body without a concave edge.

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 2.0;
constexpr double kWall   = 3.0;

// The flat face of the shape lying at height z (null if none).
TopoDS_Shape flat_face_at_height(const TopoDS_Shape& shape, double z) {
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(faces.Current(), box, false, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    box.Get(x1, y1, z1, x2, y2, z2);
    if (std::abs(z1 - z) < 1e-6 && std::abs(z2 - z) < 1e-6) {
      return faces.Current();
    }
  }
  return TopoDS_Shape();
}

// The box with its four top edges rounded, the long rounding's edge on the
// top at y = r split at x = 20.
TopoDS_Shape box_with_split_rounding() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(40, 30, 20).Shape();
  const TopoDS_Shape rounded =
    port::rounded(box, port::edges_where(box, [](const gp_Pnt& p) { return std::abs(p.Z() - 20) < 1e-6; }),
                  kRadius);
  if (rounded.IsNull()) {
    return rounded;
  }
  BRepAlgoAPI_Splitter           splitter;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(rounded);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(BRepBuilderAPI_MakeVertex(gp_Pnt(20, kRadius, 20)).Vertex());
  splitter.SetArguments(arguments);
  splitter.SetTools(tools);
  splitter.Build();
  if (!splitter.IsDone() || splitter.HasErrors()) {
    std::fprintf(stderr, "the split of the rounding's edge failed\n");
    return TopoDS_Shape();
  }
  return splitter.Shape();
}

// The slot: a box 50 x 20 x 15 with half-cylinders of radius 10 at both
// ends, merged into four walls, its top edges rounded.
TopoDS_Shape slot_with_rounded_top() {
  const TopoDS_Shape middle = BRepPrimAPI_MakeBox(gp_Pnt(0, -10, 0), 50, 20, 15).Shape();
  const TopoDS_Shape end1   = BRepPrimAPI_MakeCylinder(10, 15).Shape();
  const TopoDS_Shape end2 =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(50, 0, 0), gp_Dir(0, 0, 1)), 10, 15).Shape();
  const TopoDS_Shape fused = BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(middle, end1).Shape(), end2).Shape();
  ShapeUpgrade_UnifySameDomain unify(fused, true, true, false);
  unify.Build();
  const TopoDS_Shape slot = unify.Shape();
  return port::rounded(slot, port::edges_where(slot, [](const gp_Pnt& p) { return std::abs(p.Z() - 15) < 1e-6; }),
                       kRadius);
}

// The thick solid of the shape with its top at height `top` open, by
// kWall inside; checked against the body's volume less `cavity`.
void check_open_top(const TopoDS_Shape& shape, double top, double cavity, const char* body) {
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  CHECK(port::valid(shape));
  const TopoDS_Shape face = flat_face_at_height(shape, top);
  CHECK(!face.IsNull());
  if (face.IsNull()) {
    return;
  }
  for (const GeomAbs_JoinType join : {GeomAbs_Intersection, GeomAbs_Arc}) {
    const char*                    what = join == GeomAbs_Arc ? "Rounded" : "Sharp";
    NCollection_List<TopoDS_Shape> open;
    open.Append(face);
    BRepOffsetAPI_MakeThickSolid maker;
    try {
      OCC_CATCH_SIGNALS
      maker.MakeThickSolidByJoin(shape, open, -kWall, 1e-6, BRepOffset_Skin, false, false, join, false);
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "%s, %s: the thick solid raised '%s'\n", body, what, failure.what());
      CHECK(false);
      continue;
    }
    CHECK(maker.IsDone());
    if (!maker.IsDone()) {
      std::fprintf(stderr, "%s, %s: the thick solid not done: error %d\n", body, what,
                   static_cast<int>(maker.MakeOffset().Error()));
      continue;
    }
    const TopoDS_Shape result = maker.Shape();
    CHECK(port::valid(result));
    CHECK(port::count_of(result, TopAbs_SOLID) == 1);
    const double v        = port::volume_of(result);
    const double expected = port::volume_of(shape) - cavity;
    if (std::abs(v - expected) > 1e-6 * expected) {
      std::fprintf(stderr, "%s, %s: %.4f mm3, expected %.4f\n", body, what, v, expected);
    }
    CHECK(std::abs(v - expected) <= 1e-6 * expected);
  }
}

void test_box_rounding_of_five_edges() {
  // the box offset inside by w, up to the top
  const double cavity = (40 - 2 * kWall) * (30 - 2 * kWall) * (20 - kWall);
  const TopoDS_Shape shape = box_with_split_rounding();
  CHECK(port::count_of(shape, TopAbs_FACE) == 10);
  CHECK(port::count_of(shape, TopAbs_EDGE) == 21);
  check_open_top(shape, 20, cavity, "the box");
}

void test_slot_walls_met_tangentially() {
  // the slot offset inside by w: a rectangle 50 x (20 - 2w) and a circle of
  // radius 10 - w, up to the top
  const double kPi    = std::acos(-1.0);
  const double inner  = 10 - kWall;
  const double cavity = (50 * 2 * inner + kPi * inner * inner) * (15 - kWall);
  check_open_top(slot_with_rounded_top(), 15, cavity, "the slot");
}

} // namespace

MITCAD_PORT_TEST("2494", test_box_rounding_of_five_edges);
MITCAD_PORT_TEST("2494", test_slot_walls_met_tangentially);
