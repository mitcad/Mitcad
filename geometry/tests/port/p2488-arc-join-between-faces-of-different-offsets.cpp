// SPDX-License-Identifier: MIT
// Port patch 2488 (mitcad#2488, GitHub mitcad/Mitcad#642): the arc join
// (rounded) between faces of different offsets of their own.
//
// A box 60 x 40 x 20 offset 2 mm outward with its bottom at 5 mm
// (SetOffsetOnFace), the arc join, OCCT's offset alone. Without the patch
// the offset gave every face a tube or a corner joins to the bottom the
// bottom's 5 mm (the box grown 5 mm all round). With it the tubes along the
// bottom's edges run from 5 mm down to 2 mm out (a quarter ellipse), the
// bottom's corners are eighths of an ellipsoid 2, 2 and 5 mm, the rest as
// for 2 mm.
// Expected:
// - the thick solid, its top open: one valid solid of 20000 + 1780 pi / 3
//   mm3 (the box's 2 mm wall round its sides, 5 mm under it, the tubes and
//   corners), within 1e-6;
// - the offset of the whole box: one valid solid of 72800 + 2396 pi / 3;
// - a wedge (the box's side x = 60 leaning in to x = 50 at the top), its top
//   open: the corners at the leaning side's foot are not square, so there
//   the faces keep one offset as before (the bottom's 5 mm spreads to the
//   front, back and leaning side round them), while the left side, joined
//   to them by straight tubes and square corners, keeps its 2 mm: one valid
//   solid with its left face at x = -2 and its front at y = -5.

#include <cmath>
#include <cstdio>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

// The flat face of the shape lying at `value` along x (0), y (1) or z (2)
// (null if none).
TopoDS_Face flat_face_at(const TopoDS_Shape& shape, int axis, double value) {
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(faces.Current(), box, false, false);
    double low[3] = {0, 0, 0};
    double high[3] = {0, 0, 0};
    box.Get(low[0], low[1], low[2], high[0], high[1], high[2]);
    if (std::abs(low[axis] - value) < 1e-6 && std::abs(high[axis] - value) < 1e-6) {
      return TopoDS::Face(faces.Current());
    }
  }
  return TopoDS_Face();
}

// The offset of `shape` by `offset` with the arc join, `bottom` at
// `bottom_offset`; a thick solid without `open` where it is given.
TopoDS_Shape offset_with_bottom(const TopoDS_Shape& shape, double offset, const TopoDS_Face& bottom,
                                double bottom_offset, const TopoDS_Face& open, const char* what) {
  BRepOffset_MakeOffset maker;
  try {
    OCC_CATCH_SIGNALS
    maker.Initialize(shape, offset, 1e-7, BRepOffset_Skin, false, false, GeomAbs_Arc, false, false);
    if (!open.IsNull()) {
      maker.AddFace(open);
    }
    maker.SetOffsetOnFace(bottom, bottom_offset);
    if (open.IsNull()) {
      maker.MakeOffsetShape();
    } else {
      maker.MakeThickSolid();
    }
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s: the offset raised '%s'\n", what, failure.what());
    CHECK(false);
    return TopoDS_Shape();
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    std::fprintf(stderr, "%s: the offset not done: error %d\n", what, static_cast<int>(maker.Error()));
    return TopoDS_Shape();
  }
  const TopoDS_Shape result = maker.Shape();
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  return result;
}

void check_volume(const TopoDS_Shape& shape, double expected, const char* what) {
  const double v = port::volume_of(shape);
  if (std::abs(v - expected) > 1e-6 * expected) {
    std::fprintf(stderr, "%s: %.4f mm3, expected %.4f\n", what, v, expected);
  }
  CHECK(std::abs(v - expected) <= 1e-6 * expected);
}

void test_box_bottom_thicker_rounded() {
  // A box of its own for each offset: the offset alters its input.
  for (const bool open_top : {true, false}) {
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(60, 40, 20).Shape();
    const TopoDS_Face bottom = flat_face_at(box, 2, 0);
    const TopoDS_Face top = flat_face_at(box, 2, 20);
    CHECK(!bottom.IsNull() && !top.IsNull());
    if (bottom.IsNull() || top.IsNull()) {
      return;
    }
    const char* what = open_top ? "the thick solid" : "the whole offset";
    const TopoDS_Shape result = offset_with_bottom(box, 2, bottom, 5, open_top ? top : TopoDS_Face(), what);
    if (!result.IsNull()) {
      check_volume(result, open_top ? 20000.0 + 1780.0 * test::kPi / 3.0 : 72800.0 + 2396.0 * test::kPi / 3.0,
                   what);
    }
  }
}

void test_wedge_corner_not_square() {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(0, 0, 0), gp_Pnt(60, 0, 0), gp_Pnt(50, 0, 20), gp_Pnt(0, 0, 20), true);
  const TopoDS_Shape wedge =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire(), true).Face(), gp_Vec(0, 40, 0)).Shape();
  const TopoDS_Face bottom = flat_face_at(wedge, 2, 0);
  const TopoDS_Face top = flat_face_at(wedge, 2, 20);
  CHECK(!bottom.IsNull() && !top.IsNull());
  if (bottom.IsNull() || top.IsNull()) {
    return;
  }
  const TopoDS_Shape thick = offset_with_bottom(wedge, 2, bottom, 5, top, "the wedge");
  if (!thick.IsNull()) {
    CHECK(!flat_face_at(thick, 0, -2).IsNull());
    CHECK(!flat_face_at(thick, 1, -5).IsNull());
    CHECK(!flat_face_at(thick, 2, -5).IsNull());
  }
}

} // namespace

MITCAD_PORT_TEST("2488", test_box_bottom_thicker_rounded);
MITCAD_PORT_TEST("2488", test_wedge_corner_not_square);
