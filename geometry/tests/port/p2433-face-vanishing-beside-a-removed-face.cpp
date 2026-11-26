// SPDX-License-Identifier: MIT
// Port patch 2433 (mitcad#2433, GitHub mitcad/Mitcad#645): a face whose
// offset vanishes beside a removed face is closed by the removed face.
//
// A box 40 x 30 x 20, its top edges rounded 2 mm, shelled inside with the
// top open and Sharp corners (the intersection join), the wall thicker than
// the rounding (3 and 4 mm). The roundings' offsets turn inside out, so the
// offset leaves them out and meets the faces on both sides of them with each
// other (patch 0100); but a rounding beside the removed top had nothing on
// that side to meet, was kept, and the thick solid failed ("the offset faces
// do not close up") at any thickness.
// Expected: one valid solid, the box's sides' offsets meeting the top's
// plane: the body less the cavity (40 - 2 w) x (30 - 2 w) x (20 - w), within
// 1e-6; a point in the corner the rounding fills, under the top's edge, is
// in the solid, and one in the cavity not.

#include <cmath>
#include <cstdio>

#include <BRepBndLib.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>

#include "port_test.hpp"

namespace {

// The box with its four top edges rounded 2 mm.
TopoDS_Shape body() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(40, 30, 20).Shape();
  return port::rounded(box, port::edges_where(box, [](const gp_Pnt& p) { return std::abs(p.Z() - 20) < 1e-6; }),
                       2.0);
}

// The flat face of the shape lying at height z (null if none).
TopoDS_Shape flat_face_at(const TopoDS_Shape& shape, double z) {
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

TopAbs_State state_of(const TopoDS_Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid, p, 1e-7);
  return classifier.State();
}

void test_rounded_top_open_wall_thicker_than_rounding() {
  const TopoDS_Shape shape = body();
  CHECK(port::valid(shape));
  const TopoDS_Shape top = flat_face_at(shape, 20);
  CHECK(!top.IsNull());
  if (shape.IsNull() || top.IsNull()) {
    return;
  }
  for (const double wall : {3.0, 4.0}) {
    NCollection_List<TopoDS_Shape> open;
    open.Append(top);
    BRepOffsetAPI_MakeThickSolid maker;
    try {
      OCC_CATCH_SIGNALS
      maker.MakeThickSolidByJoin(shape, open, -wall, 1e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                                 false);
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "%g: the thick solid raised '%s'\n", wall, failure.what());
      CHECK(false);
      continue;
    }
    CHECK(maker.IsDone());
    if (!maker.IsDone()) {
      std::fprintf(stderr, "%g: the thick solid not done: error %d\n", wall,
                   static_cast<int>(maker.MakeOffset().Error()));
      continue;
    }
    const TopoDS_Shape result = maker.Shape();
    CHECK(port::valid(result));
    CHECK(port::count_of(result, TopAbs_SOLID) == 1);
    const double v = port::volume_of(result);
    const double expected = port::volume_of(shape) - (40 - 2 * wall) * (30 - 2 * wall) * (20 - wall);
    if (std::abs(v - expected) > 1e-6 * expected) {
      std::fprintf(stderr, "%g: %.4f mm3, expected %.4f\n", wall, v, expected);
    }
    CHECK(std::abs(v - expected) <= 1e-6 * expected);
    // Under the top's edge at x = 2 (where the rounding meets the top), the
    // corner the rounding fills is wall; past the wall, the cavity.
    CHECK(state_of(result, gp_Pnt(1.5 + 0.5 * (wall - 2), 15, 19.5)) == TopAbs_IN);
    CHECK(state_of(result, gp_Pnt(wall + 0.5, 15, 19.5)) == TopAbs_OUT);
  }
}

} // namespace

MITCAD_PORT_TEST("2433", test_rounded_top_open_wall_thicker_than_rounding);
