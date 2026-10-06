// SPDX-License-Identifier: MIT
// Revolution: angles, sides, full turns, targets and names. Volumes follow
// the T0 models (rev_*).

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The rectangle (10, 0)-(20, 30) about the Y axis, as in rev_full_sketch_line.
RevolveSpec tube(double angle) {
  RevolveSpec spec;
  spec.feature = "F3";
  spec.regions = {rectangle(1, 10, 0, 10, 30)};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0));
  spec.angle1 = angle;
  return spec;
}

const double kTube = kPi * (20 * 20 - 10 * 10) * 30;

void test_angles() {
  const ShapePtr full = revolve(tube(2 * kPi));
  CHECK(near(volume(*full), kTube));
  CHECK(full->face_count() == 4);
  CHECK(faces_named(*full, side("F3", 1, 1)) == 1); // x = 20: the outer cylinder
  CHECK(faces_named(*full, side("F3", 1, 3)) == 1); // x = 10: the inner one
  CHECK(full->find_faces(start_cap("F3", 1)).empty() && full->find_faces(end_cap("F3", 1)).empty());
  CHECK(full->find_edges(edge(side("F3", 1, 1), side("F3", 1, 1))).size() == 1); // the seam
  CHECK(edge_names_unique(*full));

  // A quarter, turning right-handed about +Y: from +x towards -z.
  const ShapePtr quarter = revolve(tube(kPi / 2));
  CHECK(near(volume(*quarter), kTube / 4));
  const BoundingBox box = bounding_box(*quarter);
  CHECK(near(box.min.Z(), -20) && near(box.max.Z(), 0) && near(box.min.X(), 0));
  CHECK(faces_named(*quarter, start_cap("F3", 1)) == 1 && faces_named(*quarter, end_cap("F3", 1)) == 1);
  const std::vector<int> start = quarter->find_faces(start_cap("F3", 1));
  CHECK(start.size() == 1 && near(bounding_box(Shape(quarter->face(start.at(0)))).max.Z(), 0) &&
        near(bounding_box(Shape(quarter->face(start.at(0)))).min.Z(), 0));
  CHECK(edge_names_unique(*quarter));

  // Symmetric 45 + 45 degrees (rev_symmetric) and two sides 60 + 30
  // (rev_two_sides): a quarter each; the start is the end of side one.
  RevolveSpec symmetric = tube(kPi / 4);
  symmetric.angle2 = kPi / 4;
  const ShapePtr both = revolve(symmetric);
  CHECK(near(volume(*both), kTube / 4));
  const std::vector<int> far = both->find_faces(start_cap("F3", 1));
  CHECK(far.size() == 1 && bounding_box(Shape(both->face(far.at(0)))).max.Z() < -1);
  RevolveSpec two = tube(kPi / 3);
  two.angle2 = kPi / 6;
  CHECK(near(volume(*revolve(two)), kTube / 4));
  two.angle1 = 2 * kPi;
  CHECK(throws_with([&] { revolve(two); }, "more than a full turn"));
}

void test_axes() {
  // About a profile edge: the cylinder of rev_groove_cut, r = 20, 40 long.
  RevolveSpec cylinder = tube(2 * kPi);
  cylinder.regions = {rectangle(1, 0, 0, 20, 40)};
  CHECK(near(volume(*revolve(cylinder)), kPi * 400 * 40));
  // The groove: (18, 15)-(20, 25) about the Y axis, cut.
  RevolveSpec groove = tube(2 * kPi);
  groove.feature = "F5";
  groove.regions = {rectangle(5, 18, 15, 2, 10)};
  const BooleanResult cut = boolean(BooleanOp::Cut, {revolve(cylinder).get()}, *revolve(groove));
  CHECK(cut.pieces.size() == 1 &&
        near(volume(*cut.pieces.at(0).shape), kPi * 400 * 40 - kPi * (400 - 324) * 10));

  // A profile across the axis fails; one through which the axis passes too.
  RevolveSpec across = tube(kPi);
  across.regions = {rectangle(1, -5, 0, 10, 30)};
  CHECK(throws_with([&] { revolve(across); }, "crosses the revolve axis"));
  across.axis = gp_Ax1(gp_Pnt(0, 10, 0), gp_Dir(0, 0, 1));
  CHECK(throws_with([&] { revolve(across); }, "passes through profile"));
  // An axis out of the sketch plane, parallel to it: a point at x lies
  // sqrt(x^2 + 25) from it, so the tube's area stays the same.
  RevolveSpec lifted = tube(2 * kPi);
  lifted.axis = gp_Ax1(gp_Pnt(0, 0, 5), gp_Dir(0, 1, 0));
  CHECK(near(volume(*revolve(lifted)), kTube, 1e-5));
}

void test_to_object() {
  // Up to the YZ plane: the first time the turn meets it is a quarter.
  RevolveSpec spec = tube(0);
  Target plane;
  plane.plane = gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0));
  spec.target = plane;
  const ShapePtr quarter = revolve(spec);
  CHECK(near(volume(*quarter), kTube / 4, 1e-5));
  CHECK(faces_named(*quarter, start_cap("F3", 1)) == 1 && faces_named(*quarter, end_cap("F3", 1)) == 1);
  spec.target->offset = 1;
  CHECK(throws_with([&] { revolve(spec); }, "no offset"));
}

} // namespace

void revolve_tests() {
  guarded("test_angles", test_angles);
  guarded("test_axes", test_axes);
  guarded("test_to_object", test_to_object);
}

} // namespace test
