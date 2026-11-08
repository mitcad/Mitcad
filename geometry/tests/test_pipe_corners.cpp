// SPDX-License-Identifier: MIT
// A pipe's Corners (mitcad#1448): a circular and a square section swept
// along an L-shaped path, mitered (the default, as before) and rounded (the
// section turned about the corner, OCCT's round corner). The rounded pipe
// is a sound solid that differs from the mitered one only at the corner:
// both hold the same material along the legs away from it, and at the
// corner the rounded one lacks the tip of the miter beyond the sphere (the
// circle) or the cylinder (the square) the section sweeps there.
//
// The path runs from (-40, 0, 0) to the corner at the origin and on to
// (0, 40, 0). A leg of the pipe ends in the corner's bisecting plane when
// mitered, so the mitered pipe holds A (40 + 40) for a section of area A.
// Rounded, both legs end square at the corner, their insides overlapping,
// and the section turned a quarter turn about the z axis fills the outside
// of the corner. For a circle of radius r: the legs' overlap is a quarter
// of two crossing cylinders, 4 r^3 / 3, the filling a quarter of a ball,
// pi r^3 / 3, and the miter's two tips beyond the legs' ends 2 r^3 / 3
// each, so the rounded pipe holds (4 - pi) r^3 / 3 less. For a square of
// half side h (its sides along z and in the path's plane): the overlap
// 2 h^3, the filling a quarter of a cylinder pi h^3 / 2, the tips h^3
// each, so (2 - pi / 2) h^3 less.

#include <cstdio>
#include <string>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <GProp_GProps.hxx>
#include <TopAbs_State.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kLeg = 40.0;
constexpr double kSize = 8.0;

ModelCurve line3(const gp_Pnt& a, const gp_Pnt& b) {
  ModelCurve curve;
  curve.kind = ModelCurveKind::Line;
  curve.start = a;
  curve.end = b;
  return curve;
}

// Along x to the corner at the origin, then along y.
Path l_path() {
  return {{"c1", line3(gp_Pnt(-kLeg, 0, 0), gp_Pnt(0, 0, 0))}, {"c2", line3(gp_Pnt(0, 0, 0), gp_Pnt(0, kLeg, 0))}};
}

ShapePtr l_pipe(PipeSection section, PipeCorners corners, double thickness = 0.0) {
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = l_path();
  spec.size = kSize;
  spec.section = section;
  spec.thickness = thickness;
  spec.corners = corners;
  return pipe(spec);
}

// The volume of the shape's part inside the box from `low` to `high`.
double volume_in(const Shape& shape, const gp_Pnt& low, const gp_Pnt& high) {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(low, high).Shape();
  BRepAlgoAPI_Common common(shape.occt(), box);
  CHECK(common.IsDone());
  if (!common.IsDone()) {
    return 0.0;
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(common.Shape(), props);
  return props.Mass();
}

bool inside(const Shape& shape, const gp_Pnt& point) {
  BRepClass3d_SolidClassifier classifier(shape.occt(), point, 1.0e-7);
  return classifier.State() == TopAbs_IN;
}

// A valid solid that encloses a volume.
bool sound(const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid() && volume(shape) > 0.0; }

// The rounded pipe against the mitered one of the same section, whose
// volumes differ by `less`, the rounded one smaller; `area` is the
// section's.
void compare(PipeSection section, double area, double less, const char* last_side) {
  const ShapePtr mitered = l_pipe(section, PipeCorners::Mitered);
  const ShapePtr rounded = l_pipe(section, PipeCorners::Rounded);
  CHECK(sound(*mitered));
  CHECK(sound(*rounded));
  // Mitered as before: the section's area along the whole path.
  CHECK_NEAR_TOL(volume(*mitered), area * 2 * kLeg, 1e-4);
  CHECK_NEAR_TOL(volume(*rounded), area * 2 * kLeg - less, 1e-4);
  // The difference itself, within 3 % of it (the surfaces are
  // approximated within 1e-4 mm).
  CHECK_NEAR_TOL(volume(*mitered) - volume(*rounded), less, 0.03);
  // The faces keep their names.
  for (const ShapePtr& shape : {mitered, rounded}) {
    CHECK(!shape->find_faces("F2:start").empty());
    CHECK(!shape->find_faces("F2:end").empty());
    CHECK(!shape->find_faces("F2:side0").empty());
    CHECK(!shape->find_faces(std::string("F2:") + last_side).empty());
  }
  // Along the legs, 10 mm and more from the corner, both hold the same.
  const gp_Pnt first_low(-kLeg - 1, -10, -10);
  const gp_Pnt first_high(-10, 10, 10);
  const gp_Pnt second_low(-10, 10, -10);
  const gp_Pnt second_high(10, kLeg + 1, 10);
  const double first_leg = area * (kLeg - 10);
  CHECK_NEAR_TOL(volume_in(*mitered, first_low, first_high), first_leg, 1e-4);
  CHECK_NEAR_TOL(volume_in(*rounded, first_low, first_high), first_leg, 1e-4);
  CHECK_NEAR_TOL(volume_in(*mitered, second_low, second_high), first_leg, 1e-4);
  CHECK_NEAR_TOL(volume_in(*rounded, second_low, second_high), first_leg, 1e-4);
  // At the corner they differ: the miter's tip outside the corner, 4.6
  // from it (past the 4 the section reaches round it), is only in the
  // mitered pipe; a point 1.4 from the corner and points along the legs are
  // in both.
  const gp_Pnt tip(3.0, -3.5, 0.0);
  CHECK(inside(*mitered, tip));
  CHECK(!inside(*rounded, tip));
  for (const gp_Pnt& point : {gp_Pnt(1.0, -1.0, 0.0), gp_Pnt(-20, 0, 0), gp_Pnt(0, 20, 0)}) {
    CHECK(inside(*mitered, point));
    CHECK(inside(*rounded, point));
  }
}

void test_circular_section() {
  const double r = kSize / 2;
  compare(PipeSection::Circular, kPi * r * r, (4 - kPi) * r * r * r / 3, "side0");
}

void test_square_section() {
  // mitcad#25: the size is the square's side.
  const double h = kSize / 2;
  compare(PipeSection::Square, kSize * kSize, (2 - kPi / 2) * h * h * h, "side3");
}

void test_hollow_circular_section() {
  // A 1 mm wall: the rounded tube is the rounded rod of radius 4 less the
  // rounded rod of radius 3.
  const ShapePtr tube = l_pipe(PipeSection::Circular, PipeCorners::Rounded, 1.0);
  CHECK(sound(*tube));
  const auto rod = [](double r) { return kPi * r * r * 2 * kLeg - (4 - kPi) * r * r * r / 3; };
  CHECK_NEAR_TOL(volume(*tube), rod(4) - rod(3), 1e-4);
  CHECK(!tube->find_faces("F2:inner0").empty());
}

void test_straight_path() {
  // Without a corner both are the same.
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(50, 0, 0))}};
  spec.size = kSize;
  spec.corners = PipeCorners::Rounded;
  CHECK_NEAR_TOL(volume(*pipe(spec)), kPi * 16 * 50, 1e-6);
}

} // namespace
} // namespace test

int main() {
  test::guarded("circular section", test::test_circular_section);
  test::guarded("square section", test::test_square_section);
  test::guarded("hollow circular section", test::test_hollow_circular_section);
  test::guarded("straight path", test::test_straight_path);
  if (test::failures == 0) {
    std::puts("test_pipe_corners: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
