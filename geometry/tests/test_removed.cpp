// SPDX-License-Identifier: MIT
// The material a near copy of a body lacks (removed.hpp, mitcad#85): a
// body with a free-form bump against a copy whose bump lies 0.02 mm off
// (as a replayed body against the one stored for it) and that has a hole
// the original lacks. Only the hole is removed material, found without a
// boolean of the near-coincident bumps; a near copy without the hole lacks
// nothing; a change over most of the body is cut whole.

#include <chrono>
#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/removed.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The 60 x 40 x 10 block with a wavy bump over x 40..56, y 4..36 (a spline
// patch swept 12 mm down into the block); `shift` moves the patch's poles
// up by up to that much (mm), unevenly.
TopoDS_Shape bumped(double shift) {
  const int n = 6;
  NCollection_Array2<gp_Pnt> poles(1, n, 1, n);
  for (int i = 1; i <= n; ++i) {
    for (int j = 1; j <= n; ++j) {
      const double z = 13.0 + 1.5 * std::sin(i * 1.1) * std::cos(j * 0.7) +
                       shift * (0.5 + 0.5 * std::cos(i + 2.0 * j));
      poles(i, j) = gp_Pnt(40.0 + 16.0 * (i - 1) / (n - 1), 4.0 + 32.0 * (j - 1) / (n - 1), z);
    }
  }
  NCollection_Array1<double> knots(1, 3);
  knots(1) = 0.0;
  knots(2) = 0.5;
  knots(3) = 1.0;
  NCollection_Array1<int> mults(1, 3);
  mults(1) = 4;
  mults(2) = 2;
  mults(3) = 4;
  const Handle(Geom_BSplineSurface) surface = new Geom_BSplineSurface(poles, knots, knots, mults, mults, 3, 3);
  const TopoDS_Shape bump =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(surface, 1e-7).Face(), gp_Vec(0, 0, -12)).Shape();
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(60, 40, 10)).Shape();
  BRepAlgoAPI_Fuse fuse(block, bump);
  fuse.SimplifyResult();
  return fuse.Shape();
}

// A vertical hole of radius 3 at (x, y) through the block.
TopoDS_Shape drill(const TopoDS_Shape& body, double x, double y) {
  const TopoDS_Shape tool = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x, y, -1), gp_Dir(0, 0, 1)), 3, 12).Shape();
  return BRepAlgoAPI_Cut(body, tool).Shape();
}

void test_hole_beside_near_faces() {
  const Shape before(bumped(0.0));
  const Shape after(drill(bumped(0.02), 15, 20));
  CHECK(volume(before) > 0.0);
  const auto start = std::chrono::steady_clock::now();
  const BooleanResult removed = removed_material(before, after, 0.1);
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("removed material beside near-coincident faces: %.2f s\n", seconds);
  CHECK(removed.touched.size() == 1 && removed.touched[0]);
  CHECK(removed.pieces.size() == 1);
  if (removed.pieces.size() == 1) {
    const Shape& hole = *removed.pieces.front().shape;
    CHECK_NEAR(volume(hole), kPi * 9.0 * 10.0);
    const BoundingBox b = bounding_box(hole);
    CHECK(near(b.min.X(), 12.0, 1e-4) && near(b.max.X(), 18.0, 1e-4));
    CHECK(std::abs(b.min.Z()) < 1e-4 && near(b.max.Z(), 10.0, 1e-4));
    CHECK(removed.pieces.front().sources == std::vector<std::size_t>{0});
  }
}

void test_near_copy_lacks_nothing() {
  const Shape before(bumped(0.0));
  const Shape after(bumped(0.02));
  const BooleanResult removed = removed_material(before, after, 0.1);
  CHECK(removed.pieces.empty());
  CHECK(removed.touched.size() == 1 && !removed.touched[0]);
}

void test_large_change_cuts_whole() {
  // Half the block gone: the boxes around the faces that differ would take
  // most of the body's, so the whole bodies are cut.
  const Shape before(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(60, 40, 10)).Shape());
  const Shape after(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(30, 40, 10)).Shape());
  const BooleanResult removed = removed_material(before, after, 0.1);
  CHECK(removed.pieces.size() == 1);
  if (removed.pieces.size() == 1) {
    CHECK_NEAR(volume(*removed.pieces.front().shape), 12000.0);
  }
}

} // namespace

void removed_tests() {
  test_hole_beside_near_faces();
  test_near_copy_lacks_nothing();
  test_large_change_cuts_whole();
}

} // namespace test
