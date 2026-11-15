// SPDX-License-Identifier: MIT
// Form bodies (mitcad#1757): bicubic patches made faces and sewn. Six
// patches over the sides of a 20 mm cube, each bulged out in its middle
// (its boundary poles on the cube's edges, so neighbours share their
// boundaries), close into a valid solid larger than the cube with each face
// named by its patch; one of its edges takes a fillet and a cylinder cuts a
// hole through it. Five of them are a sheet; a missing one leaves the solid
// open and is refused, as is a patch whose knots do not fit its poles.

#include <cstddef>
#include <cstdio>
#include <exception>
#include <memory>
#include <string>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/form.hpp"
#include "mitcad/geometry/shape.hpp"

namespace {

using mitcad::geometry::BooleanOp;
using mitcad::geometry::Shape;
using mitcad::geometry::ShapePtr;
using mitcad::geometry::SurfacePatch;

constexpr double kHalf = 10.0;
constexpr double kBulge = 2.0;

// The bicubic Bezier patch over the cube's side square to `axis` (0, 1, 2)
// on its `sign` side, u and v along the side so that its normal points out,
// its four middle poles moved `kBulge` out.
SurfacePatch side(int axis, double sign, int id) {
  gp_Vec normal(0.0, 0.0, 0.0);
  normal.SetCoord(axis + 1, sign);
  gp_Vec u(0.0, 0.0, 0.0);
  u.SetCoord((axis + 1) % 3 + 1, 1.0);
  gp_Vec v = normal.Crossed(u);
  SurfacePatch patch;
  patch.name = "F1:cage(" + std::to_string(id) + ")#0";
  patch.nu = 4;
  patch.nv = 4;
  patch.u_knots = {0.0, 1.0};
  patch.u_mults = {4, 4};
  patch.v_knots = {0.0, 1.0};
  patch.v_mults = {4, 4};
  for (int i = 0; i < 4; ++i) {
    for (int j = 0; j < 4; ++j) {
      const double a = -kHalf + 2.0 * kHalf * i / 3.0;
      const double b = -kHalf + 2.0 * kHalf * j / 3.0;
      const bool middle = i > 0 && i < 3 && j > 0 && j < 3;
      const gp_Vec at = normal * (kHalf + (middle ? kBulge : 0.0)) + u * a + v * b;
      patch.poles.emplace_back(at.X(), at.Y(), at.Z());
    }
  }
  return patch;
}

std::vector<SurfacePatch> cube_sides() {
  std::vector<SurfacePatch> sides;
  int id = 1;
  for (int axis = 0; axis < 3; ++axis) {
    for (const double sign : {1.0, -1.0}) {
      sides.push_back(side(axis, sign, id++));
    }
  }
  return sides;
}

double solid_volume(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.Mass();
}

int count(const Shape& shape, TopAbs_ShapeEnum kind) {
  int n = 0;
  for (TopExp_Explorer it(shape.occt(), kind); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

void test_patches_close_into_a_named_solid() {
  const ShapePtr body = mitcad::geometry::patch_body(cube_sides(), true, 1e-6);
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  CHECK(count(*body, TopAbs_SOLID) == 1);
  CHECK(body->face_count() == 6);
  CHECK(body->edge_count() == 12);
  for (int id = 1; id <= 6; ++id) {
    CHECK(body->find_faces("F1:cage(" + std::to_string(id) + ")").size() == 1);
  }
  // Larger than the cube by the six bulges, oriented with its material
  // inside.
  const double v = solid_volume(*body);
  CHECK(v > 8000.0);
  CHECK(v < 8000.0 + 6.0 * 400.0 * kBulge);
}

void test_a_fillet_and_a_hole_on_the_solid() {
  const ShapePtr body = mitcad::geometry::patch_body(cube_sides(), true, 1e-6);
  std::string edge;
  for (int e = 0; e < body->edge_count() && edge.empty(); ++e) {
    edge = body->edge_name(e);
  }
  CHECK(!edge.empty());
  const ShapePtr rounded = mitcad::geometry::fillet("F2", *body, std::vector<std::string>{edge}, 1.0);
  CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
  CHECK(rounded->face_count() == 7);
  CHECK(solid_volume(*rounded) < solid_volume(*body));
  BRepPrimAPI_MakeCylinder cylinder(gp_Ax2(gp_Pnt(0.0, 0.0, -20.0), gp_Dir(0.0, 0.0, 1.0)), 3.0, 40.0);
  const Shape tool(cylinder.Shape());
  const auto cut = mitcad::geometry::boolean(BooleanOp::Cut, {body.get()}, tool);
  CHECK(cut.pieces.size() == 1);
  if (cut.pieces.size() == 1) {
    const Shape& holed = *cut.pieces.front().shape;
    CHECK(BRepCheck_Analyzer(holed.occt()).IsValid());
    CHECK(solid_volume(holed) < solid_volume(*body) - 9.0 * test::kPi * 20.0);
    CHECK(holed.find_faces("F1:cage(5)").size() == 1);
  }
}

void test_five_patches_are_a_sheet() {
  std::vector<SurfacePatch> sides = cube_sides();
  sides.pop_back();
  const ShapePtr sheet = mitcad::geometry::patch_body(sides, false, 1e-6);
  CHECK(BRepCheck_Analyzer(sheet->occt()).IsValid());
  CHECK(count(*sheet, TopAbs_SOLID) == 0);
  CHECK(count(*sheet, TopAbs_SHELL) == 1);
  CHECK(sheet->face_count() == 5);
}

void test_open_and_broken_patches_are_refused() {
  std::vector<SurfacePatch> sides = cube_sides();
  sides.pop_back();
  CHECK(test::throws_with([&] { mitcad::geometry::patch_body(sides, true, 1e-6); }, "do not close"));
  std::vector<SurfacePatch> broken = cube_sides();
  broken.front().u_mults = {4, 3};
  CHECK(test::throws_with([&] { mitcad::geometry::patch_body(broken, true, 1e-6); }, "do not fit"));
  CHECK(test::throws_with([&] { mitcad::geometry::patch_body({}, true, 1e-6); }, "no patches"));
}

} // namespace

int main() {
  test::guarded("patches close into a named solid", test_patches_close_into_a_named_solid);
  test::guarded("a fillet and a hole on the solid", test_a_fillet_and_a_hole_on_the_solid);
  test::guarded("five patches are a sheet", test_five_patches_are_a_sheet);
  test::guarded("open and broken patches are refused", test_open_and_broken_patches_are_refused);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("form: all checks passed\n");
  return 0;
}
