// SPDX-License-Identifier: MIT
// Port patch 2463 (mitcad#2463): the arc join (Rounded) closes a face whose
// offset vanishes beside a removed face, as patch 2433 does for the
// intersection join.
//
// A box 40 x 30 x 20, its top edges rounded 2 mm, shelled inside with the
// top open and Rounded corners (the arc join), the wall thicker than the
// rounding (3 and 4 mm). The roundings' offsets turn inside out and vanish
// (patch 0100); the faces beyond them were not extended to the removed top,
// which got no section, and the thick solid failed at any such thickness.
// Expected: one valid solid, the box's sides' offsets meeting the top's
// plane: the body less the cavity (40 - 2 w) x (30 - 2 w) x (20 - w), within
// 1e-6 (as the intersection join gives it, port test 2433); a point in the
// corner the rounding fills, under the top's edge, is in the solid, and one
// in the cavity not.

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
TopoDS_Shape rounded_box() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(40, 30, 20).Shape();
  return port::rounded(box, port::edges_where(box, [](const gp_Pnt& p) { return std::abs(p.Z() - 20) < 1e-6; }),
                       2.0);
}

// The flat face of the shape lying at height z (null if none).
TopoDS_Shape face_at_height(const TopoDS_Shape& shape, double z) {
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

TopAbs_State state_in(const TopoDS_Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid, p, 1e-7);
  return classifier.State();
}

void test_rounded_top_open_arc_join() {
  const TopoDS_Shape shape = rounded_box();
  CHECK(port::valid(shape));
  const TopoDS_Shape top = face_at_height(shape, 20);
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
      maker.MakeThickSolidByJoin(shape, open, -wall, 1e-6, BRepOffset_Skin, false, false, GeomAbs_Arc, false);
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
    CHECK(state_in(result, gp_Pnt(1.5 + 0.5 * (wall - 2), 15, 19.5)) == TopAbs_IN);
    CHECK(state_in(result, gp_Pnt(wall + 0.5, 15, 19.5)) == TopAbs_OUT);
  }
}

} // namespace

MITCAD_PORT_TEST("2463", test_rounded_top_open_arc_join);
