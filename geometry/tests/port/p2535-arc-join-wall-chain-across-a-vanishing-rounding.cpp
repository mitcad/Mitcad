// SPDX-License-Identifier: MIT
// Port patch 2535 (mitcad#2535, GitHub mitcad/Mitcad#645): in the arc join,
// a wall whose edges to a rounding vanishing beside a removed face are a
// chain is extended to the removed face as one section.
//
// A box 40 x 30 x 20, its four top edges rounded r = 2 mm, shelled inside
// with the top open by w = 3 mm (thicker than the rounding, which vanishes)
// with Rounded corners (the arc join):
// - the edge where one long rounding meets its wall split in two by a
//   vertex: the wall has two edges to the rounding, the top one. Before, the
//   wall was extended to the top along the one edge the top's edge was
//   paired with first, and each extended edge got the whole section with
//   the top, so that the vertex between them had no place on it: no result.
// - that edge split at x = 15 and the rounding's edge on the top at x = 25:
//   chains on both sides, their vertices not across each other.
// Expected: one valid solid, the body less the cavity: the box offset inside
// by w with sharp edges, up to the top's plane, as the intersection join
// gives (the body has no concave edge, so the arc join rounds nothing).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>

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

// The box with its four top edges rounded, its edges split at `points`.
TopoDS_Shape box_split_at(const std::vector<gp_Pnt>& points) {
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
  for (const gp_Pnt& p : points) {
    tools.Append(BRepBuilderAPI_MakeVertex(p).Vertex());
  }
  splitter.SetArguments(arguments);
  splitter.SetTools(tools);
  splitter.Build();
  if (!splitter.IsDone() || splitter.HasErrors()) {
    std::fprintf(stderr, "the split of the rounding's edges failed\n");
    return TopoDS_Shape();
  }
  return splitter.Shape();
}

// The thick solid of the box with its top open, by kWall inside, Rounded;
// checked against the body's volume less the cavity.
void check_open_top_rounded(const TopoDS_Shape& shape, const char* body) {
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  CHECK(port::valid(shape));
  const TopoDS_Shape face = flat_face_at_height(shape, 20);
  CHECK(!face.IsNull());
  if (face.IsNull()) {
    return;
  }
  NCollection_List<TopoDS_Shape> open;
  open.Append(face);
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(shape, open, -kWall, 1e-6, BRepOffset_Skin, false, false, GeomAbs_Arc, false);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s: the thick solid raised '%s'\n", body, failure.what());
    CHECK(false);
    return;
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    std::fprintf(stderr, "%s: the thick solid not done: error %d\n", body,
                 static_cast<int>(maker.MakeOffset().Error()));
    return;
  }
  const TopoDS_Shape result = maker.Shape();
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  // the box offset inside by w, up to the top
  const double cavity   = (40 - 2 * kWall) * (30 - 2 * kWall) * (20 - kWall);
  const double v        = port::volume_of(result);
  const double expected = port::volume_of(shape) - cavity;
  if (std::abs(v - expected) > 1e-6 * expected) {
    std::fprintf(stderr, "%s: %.4f mm3, expected %.4f\n", body, v, expected);
  }
  CHECK(std::abs(v - expected) <= 1e-6 * expected);
}

void test_wall_of_two_edges_to_the_rounding() {
  const TopoDS_Shape shape = box_split_at({gp_Pnt(20, 0, 20 - kRadius)});
  CHECK(port::count_of(shape, TopAbs_FACE) == 10);
  check_open_top_rounded(shape, "the box, the wall's edge split");
}

void test_chains_on_both_sides_of_the_rounding() {
  const TopoDS_Shape shape = box_split_at({gp_Pnt(15, 0, 20 - kRadius), gp_Pnt(25, kRadius, 20)});
  CHECK(port::count_of(shape, TopAbs_FACE) == 10);
  check_open_top_rounded(shape, "the box, both edges split");
}

} // namespace

MITCAD_PORT_TEST("2535", test_wall_of_two_edges_to_the_rounding);
MITCAD_PORT_TEST("2535", test_chains_on_both_sides_of_the_rounding);
