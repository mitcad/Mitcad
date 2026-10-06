// SPDX-License-Identifier: MIT
// Profile regions from every segment kind, holes and loop orientation.

#include <cmath>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

Segment line(const char* name, double x0, double y0, double x1, double y1) {
  Segment s;
  s.name = name;
  s.kind = SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

double area_of(const Region& region, const Frame& frame = Frame()) {
  return mass_properties(*profile_shape(frame, {region})).area;
}

void test_rectangle_profile_names_its_edges() {
  const ProfileFace profile = make_profile(Frame(), rectangle(1, 0, 0, 60, 40));
  CHECK(profile.edges.size() == 4);
  CHECK(profile.edges[1].second == "c2[c1,c3]");
  CHECK(near(area_of(rectangle(1, 0, 0, 60, 40)), 2400.0));
}

void test_arcs_ellipses_and_splines() {
  // A half disc: a line and an arc counter-clockwise from 90 to 270 degrees.
  Segment arc;
  arc.name = "c2[c1,c1]";
  arc.kind = SegmentKind::Arc;
  arc.center = gp_Pnt2d(0, 0);
  arc.radius = 10;
  arc.start_angle = kPi / 2;
  arc.end_angle = 3 * kPi / 2;
  CHECK(near(area_of(Region{"r", {Loop{{line("c1[c2,c2]", 0, -10, 0, 10), arc}}}}), kPi * 50.0));

  Segment ellipse;
  ellipse.name = "c3";
  ellipse.kind = SegmentKind::Ellipse;
  ellipse.center = gp_Pnt2d(5, 5);
  ellipse.radius = 20;
  ellipse.minor_radius = 10;
  ellipse.rotation = kPi / 6;
  CHECK(near(area_of(Region{"r", {Loop{{ellipse}}}}), kPi * 200.0));

  // Half an ellipse closed by its major axis.
  Segment half = ellipse;
  half.kind = SegmentKind::EllipseArc;
  half.center = gp_Pnt2d(0, 0);
  half.rotation = 0;
  half.start_angle = 0;
  half.end_angle = kPi;
  CHECK(near(area_of(Region{"r", {Loop{{half, line("c4", -20, 0, 20, 0)}}}}), kPi * 100.0));

  // A quadratic Bezier arch over a 10 mm chord: 2/3 of base times height.
  Segment arch;
  arch.name = "c5";
  arch.kind = SegmentKind::BSpline;
  arch.degree = 2;
  arch.poles = {gp_Pnt2d(0, 0), gp_Pnt2d(5, 10), gp_Pnt2d(10, 0)};
  arch.knots = {0, 1};
  arch.multiplicities = {3, 3};
  CHECK(near(area_of(Region{"r", {Loop{{arch, line("c6", 10, 0, 0, 0)}}}}), 2.0 / 3.0 * 10 * 5));

  // A rational quadratic B-spline is an exact quarter circle.
  Segment quarter = arch;
  quarter.poles = {gp_Pnt2d(10, 0), gp_Pnt2d(10, 10), gp_Pnt2d(0, 10)};
  quarter.weights = {1, std::sqrt(0.5), 1};
  const Region sector{"r", {Loop{{quarter, line("c7", 0, 10, 0, 0), line("c8", 0, 0, 10, 0)}}}};
  CHECK(near(area_of(sector), kPi * 25.0));
}

void test_holes_and_orientation() {
  Region plate = rectangle(1, 0, 0, 40, 20);
  plate.loops.push_back(circle(5, 20, 10, 5).loops[0]);
  CHECK(near(area_of(plate), 800.0 - kPi * 25.0));

  // Loops given clockwise give the same face, with the sketch normal.
  Region reversed = plate;
  std::reverse(reversed.loops[0].segments.begin(), reversed.loops[0].segments.end());
  for (Segment& s : reversed.loops[0].segments) {
    std::swap(s.start, s.end);
  }
  CHECK(near(area_of(reversed), 800.0 - kPi * 25.0));
  ExtrudeSpec spec;
  spec.feature = "F2";
  spec.regions = {reversed};
  spec.end = 10;
  const BoundingBox box = bounding_box(*mitcad::geometry::extrude(spec));
  CHECK(near(box.min.Z(), 0.0) && near(box.max.Z(), 10.0));
}

void test_profile_on_another_plane() {
  // The XZ plane seen from -Y: sketch x = X, sketch y = Z.
  Frame frame;
  frame.origin = gp_Pnt(0, 5, 0);
  frame.x_axis = gp_Dir(1, 0, 0);
  frame.y_axis = gp_Dir(0, 0, 1);
  CHECK(frame.normal().IsEqual(gp_Dir(0, -1, 0), 1e-12));
  const BoundingBox box = bounding_box(*profile_shape(frame, {rectangle(1, 2, 3, 10, 20)}));
  CHECK(near(box.min.X(), 2) && near(box.max.X(), 12) && near(box.min.Z(), 3) && near(box.max.Z(), 23));
  CHECK(near(box.min.Y(), 5) && near(box.max.Y(), 5));
}

void test_segments_meeting_within_the_sketch_tolerance() {
  // A rectangle whose corner is 3e-7 mm off: over OCCT's tolerance, but one
  // point for the sketch (imported sketches).
  const double off = 3e-7;
  const Region region{"r", {Loop{{line("c1", 0, 0, 30, 0), line("c2", 30, -off, 30, 15),
                                  line("c3", 30, 15, 0, 15), line("c4", 0, 15, 0, 0)}}}};
  const ProfileFace profile = make_profile(Frame(), region);
  CHECK(profile.edges.size() == 4);
  CHECK(profile.edges[1].second == "c2");
  CHECK(near(area_of(region), 450.0));
}

void test_invalid_profiles() {
  CHECK(throws_with([] { make_profile(Frame(), Region{"r", {}}); }, "has no boundary"));
  const Region open{"r", {Loop{{line("c1", 0, 0, 10, 0), line("c2", 10, 0, 10, 10)}}}};
  CHECK(throws_with([&] { make_profile(Frame(), open); }, "not closed"));
  const Region gap{"r", {Loop{{line("c1", 0, 0, 10, 0), line("c2", 20, 0, 20, 10)}}}};
  CHECK(throws_with([&] { make_profile(Frame(), gap); }, "segment c2: it does not connect"));
  const Region point{"r", {Loop{{line("c1", 0, 0, 0, 0)}}}};
  CHECK(throws_with([&] { make_profile(Frame(), point); }, "segment c1: the line has zero length"));
  Segment ellipse;
  ellipse.name = "c3";
  ellipse.kind = SegmentKind::Ellipse;
  ellipse.radius = 5;
  ellipse.minor_radius = 10;
  CHECK(throws_with([&] { make_profile(Frame(), Region{"r", {Loop{{ellipse}}}}); },
                    "major radius is smaller"));
  CHECK(throws_with([] { rectangle_region(0, 0, 0, 5); }, "width must be greater than zero"));
}

} // namespace

void profile_tests() {
  test_rectangle_profile_names_its_edges();
  test_arcs_ellipses_and_splines();
  test_holes_and_orientation();
  test_profile_on_another_plane();
  test_segments_meeting_within_the_sketch_tolerance();
  test_invalid_profiles();
}

} // namespace test
