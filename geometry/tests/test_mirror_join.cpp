// SPDX-License-Identifier: MIT
// A body joined with a near copy (removed.hpp, mitcad#88): a block with a
// free-form bump on each side of x = 30, the left one 0.02 mm off the
// mirror image of the right one (as replayed faces lie off stored ones),
// and a hole on the left only. Its mirror image lies on it almost
// everywhere; the join adds only the hole back, found without a boolean of
// the near-coincident bumps. An exactly symmetric body joins to itself; a
// mirror image beside the body is no near copy.

#include <chrono>
#include <cmath>
#include <cstdio>
#include <optional>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/removed.hpp"
#include "mitcad/geometry/transform.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// A spline bump over x 40..56 (mirrored about x = 30 to x 4..20 when
// `left`), y 4..36, swept 12 mm down; `shift` moves its poles up by up to
// that much (mm), unevenly.
TopoDS_Shape bump(bool left, double shift) {
  const int n = 6;
  NCollection_Array2<gp_Pnt> poles(1, n, 1, n);
  for (int i = 1; i <= n; ++i) {
    for (int j = 1; j <= n; ++j) {
      const double z = 13.0 + 1.5 * std::sin(i * 1.1) * std::cos(j * 0.7) +
                       shift * (0.5 + 0.5 * std::cos(i + 2.0 * j));
      const double x = 40.0 + 16.0 * (i - 1) / (n - 1);
      poles(i, j) = gp_Pnt(left ? 60.0 - x : x, 4.0 + 32.0 * (j - 1) / (n - 1), z);
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
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(surface, 1e-7).Face(), gp_Vec(0, 0, -12)).Shape();
}

// The 60 x 40 x 10 block with both bumps, the left one `shift` off, and a
// hole of radius 3 at (25, 20) through the block when `hole`.
Shape nearly_symmetric(double shift, bool hole) {
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(60, 40, 10)).Shape();
  body = BRepAlgoAPI_Fuse(body, bump(false, 0.0)).Shape();
  body = BRepAlgoAPI_Fuse(body, bump(true, shift)).Shape();
  if (hole) {
    const TopoDS_Shape tool = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(25, 20, -1), gp_Dir(0, 0, 1)), 3, 12).Shape();
    body = BRepAlgoAPI_Cut(body, tool).Shape();
  }
  return Shape(body);
}

// The body's mirror image about x = `at`.
ShapePtr mirrored(const Shape& body, double at) {
  Affine mirror;
  mirror.linear = {{{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  mirror.translation = {2.0 * at, 0, 0};
  return transform_shape(body, mirror, "F9:inst1");
}

bool valid(const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); }

void test_hole_filled_by_the_image() {
  const Shape body = nearly_symmetric(0.02, true);
  const ShapePtr image = mirrored(body, 30.0);
  const auto start = std::chrono::steady_clock::now();
  const std::optional<BooleanResult> joined = join_near_copy(body, *image, 0.1);
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("join of a near copy: %.2f s\n", seconds);
  CHECK(joined.has_value());
  if (!joined) {
    return;
  }
  CHECK(joined->touched == std::vector<bool>{true});
  CHECK(joined->pieces.size() == 1);
  if (joined->pieces.size() == 1) {
    const Shape& result = *joined->pieces.front().shape;
    // The hole is filled; the bumps' 0.02 mm differ by less than the slack.
    CHECK(near(volume(result), volume(body) + kPi * 9.0 * 10.0, 1e-4));
    CHECK(valid(result));
    CHECK(joined->pieces.front().sources == std::vector<std::size_t>{0});
  }
}

void test_symmetric_body_joins_to_itself() {
  const Shape body = nearly_symmetric(0.0, false);
  const ShapePtr image = mirrored(body, 30.0);
  const std::optional<BooleanResult> joined = join_near_copy(body, *image, 0.1);
  CHECK(joined.has_value());
  if (joined && joined->pieces.size() == 1) {
    const Shape& result = *joined->pieces.front().shape;
    CHECK(result.face_count() == body.face_count());
    CHECK_NEAR(volume(result), volume(body));
  }
}

void test_image_beside_the_body_is_no_near_copy() {
  // Mirrored about its end face x = 60: the image touches the body there
  // only, and the caller joins the whole shapes.
  const Shape body = nearly_symmetric(0.02, true);
  const ShapePtr image = mirrored(body, 60.0);
  CHECK(!join_near_copy(body, *image, 0.1).has_value());
}

} // namespace

void mirror_join_tests() {
  test_hole_filled_by_the_image();
  test_symmetric_body_joins_to_itself();
  test_image_beside_the_body_is_no_near_copy();
}

} // namespace test
