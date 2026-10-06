// SPDX-License-Identifier: MIT
// Modelled threads: the groove's volume against the basic profile, names,
// partial lengths and internal threads.

#include <cmath>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The basic ISO profile's depth, 5H/8.
double depth_of(double pitch) { return 0.625 * 0.8660254037844386 * pitch; }

// Removed per millimetre of thread: the groove's section (a trapezoid a
// wide at the cylinder, b wide `depth` away from it) swept once round its
// centroid per pitch (Pappus). `sign` is -1 into an external cylinder.
double removed_per_mm(double pitch, double radius, double a, double b, double sign) {
  const double depth = depth_of(pitch);
  const double area = (a + b) / 2.0 * depth;
  const double centroid = radius + sign * depth * (a + 2.0 * b) / (3.0 * (a + b));
  return area * 2.0 * kPi * centroid / pitch;
}

void test_external() {
  // An M6x1 thread over a 6 mm rod, 6 mm long, running out at both ends.
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 3)}, 0, 6);
  ThreadSpec spec;
  spec.feature = "F3";
  spec.faces = {"F2:side(c1)"};
  spec.pitch = 1.0;
  spec.depth = depth_of(1.0);
  const ShapePtr threaded = modeled_thread(*rod, spec);
  const double expected = kPi * 9 * 6 - 6 * removed_per_mm(1.0, 3.0, 7.0 / 8.0, 1.0 / 4.0, -1.0);
  CHECK(near(volume(*threaded), expected, 1e-3));
  CHECK(!threaded->find_faces("F3:thread(F2:side(c1))").empty());
  CHECK(threaded->find_faces("F2:end(r{c1})").size() == 1);
  CHECK(near(bounding_box(*threaded).max.Z(), 6, 1e-6));
  // A left-hand thread removes the same.
  spec.right_handed = false;
  const ShapePtr left = modeled_thread(*rod, spec);
  CHECK(near(volume(*left), expected, 1e-3));

  // 3 mm of it from the top, 1 mm down: less than half as much removed.
  spec.right_handed = true;
  spec.full_length = false;
  spec.length = 3;
  spec.offset = 1;
  const double partial = volume(*modeled_thread(*rod, spec));
  CHECK(partial > expected && partial < kPi * 9 * 6);
  spec.length = 6;
  CHECK(throws_with([&] { modeled_thread(*rod, spec); }, "does not fit"));
  spec.faces = {"F2:end(r{c1})"};
  CHECK(throws_with([&] { modeled_thread(*rod, spec); }, "not cylindrical"));
}

void test_internal() {
  // An M6x1 internal thread in a bore at the minor diameter through a
  // 6 mm plate.
  const double pitch = 1.0;
  const double minor = 6.0 - 1.082532 * pitch;
  const ShapePtr plate = extrude("F2", {rectangle(1, -10, -10, 20, 20)}, 0, 6);
  const ShapePtr bore = extrude("F3", {circle(5, 0, 0, minor / 2.0)}, -1, 7);
  const BooleanResult drilled = boolean(BooleanOp::Cut, {plate.get()}, *bore);
  CHECK(drilled.pieces.size() == 1);
  const ShapePtr nut = drilled.pieces.at(0).shape;
  ThreadSpec spec;
  spec.feature = "F4";
  spec.faces = {"F3:side(c5)"};
  spec.pitch = pitch;
  spec.depth = depth_of(pitch);
  const ShapePtr threaded = modeled_thread(*nut, spec);
  const double expected =
      volume(*nut) - 6 * removed_per_mm(pitch, minor / 2.0, 3.0 / 4.0, 1.0 / 8.0, 1.0);
  CHECK(near(volume(*threaded), expected, 1e-3));
  CHECK(!threaded->find_faces("F4:thread(F3:side(c5))").empty());
}

void test_long_and_in_holes() {
  // M10x1.5 over a 10 mm rod 20 long (T0 thread_modeled): 15 turns.
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 20);
  ThreadSpec spec;
  spec.feature = "F3";
  spec.faces = {"F2:side(c1)"};
  spec.pitch = 1.5;
  spec.depth = depth_of(1.5);
  const double expected = kPi * 25 * 20 - 20 * removed_per_mm(1.5, 5.0, 7.0 / 8.0 * 1.5, 1.0 / 4.0 * 1.5, -1.0);
  const double got = volume(*modeled_thread(*rod, spec));
  CHECK(near(got, expected, 1e-3));

  // An M6x1 thread in a drilled hole through a 6 mm plate: the wall's axis
  // points down, into the material.
  const ShapePtr plate = extrude("F2", {rectangle(1, 100, 0, 30, 30)}, 0, 6);
  HoleSpec hole;
  hole.feature = "F4";
  hole.positions = {gp_Ax1(gp_Pnt(115, 15, 6), gp_Dir(0, 0, -1))};
  hole.diameter = 6.0 - 1.082532;
  hole.extent = HoleExtent::ThroughAll;
  hole.bodies = {plate};
  const BooleanResult drilled = boolean(BooleanOp::Cut, {plate.get()}, *hole_tool(hole));
  const ShapePtr nut = drilled.pieces.at(0).shape;
  spec.feature = "F5";
  spec.faces = {"F4:hole0.wall"};
  spec.pitch = 1.0;
  spec.depth = depth_of(1.0);
  const double in_hole = volume(*modeled_thread(*nut, spec));
  const double removed = 6 * removed_per_mm(1.0, hole.diameter / 2.0, 3.0 / 4.0, 1.0 / 8.0, 1.0);
  CHECK(near(in_hole, volume(*nut) - removed, 1e-3));
}

} // namespace

void thread_tests() {
  guarded("test_long_and_in_holes", test_long_and_in_holes);
  guarded("test_external", test_external);
  guarded("test_internal", test_internal);
}

} // namespace test
