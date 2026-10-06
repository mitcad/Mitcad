// SPDX-License-Identifier: MIT
// Construction geometry queries: surfaces, curves and points of named
// sub-shapes, paths and datum display shapes.

#include <TopAbs_ShapeEnum.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

bool at(const gp_Pnt& p, double x, double y, double z) {
  return p.Distance(gp_Pnt(x, y, z)) < 1e-7;
}

bool along(const gp_Dir& d, double x, double y, double z) {
  return d.IsEqual(gp_Dir(x, y, z), 1e-9);
}

// The block x 0..60, y 0..40, z 0..20 of F2 (lines c1..c4) and the
// cylinder of radius 5 and height 10 at the origin of F3 (circle c5).
const ShapePtr& block() {
  static const ShapePtr shape = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  return shape;
}

const ShapePtr& cylinder() {
  static const ShapePtr shape = extrude("F3", {circle(5, 0, 0, 5)}, 0, 10);
  return shape;
}

void test_surfaces() {
  const SurfaceDescription top = face_geometry(*block(), end_cap("F2", 1));
  CHECK(top.type == "plane");
  CHECK(along(top.axis, 0, 0, 1));
  CHECK(at(top.origin, 0, 0, 20));
  const SurfaceDescription bottom = face_geometry(*block(), start_cap("F2", 1));
  CHECK(along(bottom.axis, 0, 0, -1) && at(bottom.origin, 0, 0, 0));
  // Side c1 is the face y = 0, its outward normal -Y; c2 is x = 60.
  const SurfaceDescription front = face_geometry(*block(), side("F2", 1, 0));
  CHECK(along(front.axis, 0, -1, 0) && at(front.origin, 0, 0, 0));
  const SurfaceDescription right = face_geometry(*block(), side("F2", 1, 1));
  CHECK(along(right.axis, 1, 0, 0) && at(right.origin, 60, 0, 0));

  const SurfaceDescription wall = face_geometry(*cylinder(), face_name("F3", "side", "c5"));
  CHECK(wall.type == "cylinder");
  CHECK(near(wall.radius, 5.0));
  CHECK(along(wall.axis, 0, 0, 1));
  CHECK(at(wall.origin, 0, 0, 5));
  CHECK(throws_with([] { face_geometry(*block(), "F2:side(c9)"); }, "the body has no such face"));
}

void test_split_faces_on_one_plane() {
  // A groove across the top splits it into two pieces on one plane.
  const ShapePtr groove = extrude("F4", {rectangle(11, 25, -1, 10, 42)}, 10, 21);
  const BooleanResult cut = boolean(BooleanOp::Cut, {block().get()}, *groove);
  CHECK(cut.pieces.size() == 1);
  const Shape& grooved = *cut.pieces.front().shape;
  CHECK(grooved.find_faces(end_cap("F2", 1)).size() == 2);
  const SurfaceDescription top = face_geometry(grooved, end_cap("F2", 1));
  CHECK(top.type == "plane" && at(top.origin, 0, 0, 20));
  const SurfacePoint touch = face_point_normal(grooved, end_cap("F2", 1), gp_Pnt(30, 20, 30));
  CHECK(along(touch.normal, 0, 0, 1));
  CHECK(near(touch.point.Z(), 20.0) && near(std::abs(touch.point.X() - 30.0), 5.0));
}

void test_curves_and_vertices() {
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  const CurveDescription line = edge_geometry(*block(), top_front);
  CHECK(line.type == "line");
  CHECK((at(line.start, 0, 0, 20) && at(line.end, 60, 0, 20)) ||
        (at(line.start, 60, 0, 20) && at(line.end, 0, 0, 20)));
  const std::string rim = edge(face_name("F3", "end", "r{c5}"), face_name("F3", "side", "c5"));
  const CurveDescription circle = edge_geometry(*cylinder(), rim);
  CHECK(circle.type == "circle");
  CHECK(at(circle.center, 0, 0, 10) && near(circle.radius, 5.0));
  CHECK(circle.start.Distance(circle.end) < 1e-7);
  CHECK(throws_with([] { edge_geometry(*block(), "E{F2:a|F2:b}"); }, "no such edge"));

  const std::string corner =
      mitcad::geometry::make_vertex_name({side("F2", 1, 0), side("F2", 1, 1), start_cap("F2", 1)});
  CHECK(at(vertex_point(*block(), corner), 60, 0, 0));
}

void test_paths() {
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  const std::string top_right = edge(end_cap("F2", 1), side("F2", 1, 1));
  const gp_Pnt none;
  // Front then right: from (0, 0, 20) to (60, 0, 20) to (60, 40, 20).
  const std::vector<std::string> path{top_front, top_right};
  const PathPoint half = path_point(*block(), path, PathAt::Fraction, 0.5, none);
  CHECK(at(half.point, 50, 0, 20) && along(half.tangent, 1, 0, 0));
  const PathPoint later = path_point(*block(), path, PathAt::Length, 80.0, none);
  CHECK(at(later.point, 60, 20, 20) && along(later.tangent, 0, 1, 0));
  const PathPoint beyond = path_point(*block(), path, PathAt::Fraction, 1.1, none);
  CHECK(at(beyond.point, 60, 50, 20));
  const PathPoint before = path_point(*block(), path, PathAt::Length, -5.0, none);
  CHECK(at(before.point, -5, 0, 20));
  // The other way round.
  const std::vector<std::string> back{top_right, top_front};
  const PathPoint other = path_point(*block(), back, PathAt::Fraction, 0.5, none);
  CHECK(at(other.point, 50, 0, 20) && along(other.tangent, -1, 0, 0));
  const PathPoint nearest = path_point(*block(), path, PathAt::Near, 0.0, gp_Pnt(30, -5, 25));
  CHECK(at(nearest.point, 30, 0, 20) && along(nearest.tangent, 1, 0, 0));
  const std::string bottom_back = edge(start_cap("F2", 1), side("F2", 1, 2));
  CHECK(throws_with(
      [&] { path_point(*block(), {top_front, bottom_back}, PathAt::Fraction, 0.5, none); },
      "not connected"));
}

void test_face_points_and_display() {
  const SurfacePoint on = face_point_normal(*cylinder(), face_name("F3", "side", "c5"),
                                            gp_Pnt(20, 0, 5));
  CHECK(at(on.point, 5, 0, 5) && along(on.normal, 1, 0, 0));
  const ShapePtr plane = plane_shape(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 10);
  CHECK(plane->occt().ShapeType() == TopAbs_FACE);
  CHECK(near(mass_properties(*plane).area, 100.0));
  CHECK(bounding_box(*plane).max.Z() < 5.0 + 1e-7);
  const ShapePtr axis = axis_shape(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0), 8);
  CHECK(axis->occt().ShapeType() == TopAbs_EDGE);
  CHECK(near(bounding_box(*axis).min.Y(), -4.0));
  CHECK(point_shape(gp_Pnt(1, 2, 3))->occt().ShapeType() == TopAbs_VERTEX);
  CHECK(throws_with([] { plane_shape(gp_Pnt(), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 0); }, "size"));
}

} // namespace

void datum_tests() {
  test_surfaces();
  test_split_faces_on_one_plane();
  test_curves_and_vertices();
  test_paths();
  test_face_points_and_display();
}

} // namespace test
