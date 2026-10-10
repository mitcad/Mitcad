// SPDX-License-Identifier: MIT
// Mass properties, bounding boxes and face and edge descriptions.

#include <thread>

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

void test_mass_properties_and_box() {
  const ShapePtr block = extrude("F2", {rectangle(1, -10, 5, 60, 40)}, 0, 20);
  const MassProperties props = mass_properties(*block);
  CHECK(near(props.volume, 48000.0));
  CHECK(near(props.area, 2 * (60.0 * 40.0 + 60.0 * 20.0 + 40.0 * 20.0)));
  CHECK(near(props.center.X(), 20) && near(props.center.Y(), 25) && near(props.center.Z(), 10));
  const BoundingBox box = bounding_box(*block);
  CHECK(!box.empty);
  CHECK(near(box.min.X(), -10) && near(box.min.Y(), 5) && near(box.min.Z(), 0));
  CHECK(near(box.max.X(), 50) && near(box.max.Y(), 45) && near(box.max.Z(), 20));
  // A cylinder's box is tight, not enlarged by tolerances or control points.
  const BoundingBox round = bounding_box(*extrude("F3", {circle(5, 0, 0, 5)}, 0, 10));
  CHECK(near(round.min.X(), -5, 1e-4) && near(round.max.Y(), 5, 1e-4));
  CHECK(bounding_box(Shape(TopoDS_Shape())).empty);
}

void test_face_and_edge_infos() {
  const ShapePtr cylinder = extrude("F3", {circle(5, 0, 0, 5)}, 0, 10);
  const std::vector<FaceInfo> faces = face_infos(*cylinder);
  CHECK(faces.size() == 3);
  int planes = 0;
  for (const FaceInfo& face : faces) {
    if (face.surface == "plane") {
      ++planes;
      CHECK(near(face.area, kPi * 25.0));
    } else {
      CHECK(face.surface == "cylinder");
      CHECK(face.names == NameList{"F3:side(c5)"});
      CHECK(near(face.area, 2 * kPi * 5.0 * 10.0));
    }
  }
  CHECK(planes == 2);
  const std::vector<EdgeInfo> edges = edge_infos(*cylinder);
  CHECK(edges.size() == 3);
  int circles = 0;
  for (const EdgeInfo& e : edges) {
    CHECK(!e.name.empty());
    if (e.curve == "circle") {
      ++circles;
      CHECK(near(e.length, 2 * kPi * 5.0));
    } else {
      CHECK(e.curve == "line" && near(e.length, 10.0)); // the seam
    }
  }
  CHECK(circles == 2);
}

// The model's worker and the application measure the same shapes at once
// (P7): each thread gets the same answer, and copies made meanwhile keep
// what was measured.
void test_measured_from_threads() {
  const ShapePtr block = extrude("F2", {rectangle(1, -10, 5, 60, 40)}, 0, 20);
  constexpr std::size_t kThreads = 8;
  std::vector<double> volumes(kThreads, 0.0);
  std::vector<double> widths(kThreads, 0.0);
  std::vector<std::thread> threads;
  for (std::size_t i = 0; i < kThreads; ++i) {
    threads.emplace_back([&, i] {
      for (int round = 0; round < 20; ++round) {
        const Shape copy = *block;
        volumes[i] = (round % 2 == 0 ? mass_properties(*block) : mass_properties(copy)).volume;
        const BoundingBox box = bounding_box(round % 3 == 0 ? copy : *block);
        widths[i] = box.max.X() - box.min.X();
      }
    });
  }
  for (std::thread& thread : threads) {
    thread.join();
  }
  for (std::size_t i = 0; i < kThreads; ++i) {
    CHECK_NEAR(volumes[i], 48000.0);
    CHECK_NEAR(widths[i], 60.0);
  }
  const std::optional<std::array<double, 5>> measured = block->measured();
  CHECK(measured && near((*measured)[0], 48000.0));
  const Shape copy(*block);
  CHECK(copy.measured() == measured && copy.bounds() == block->bounds());
}

// The faces an operation made or changed (the .f3d import's geometric
// check, mitcad#138): a rounding of a block's top front edge makes its own
// face and trims the four faces it meets; the back and the bottom stay. A
// copy rebuilt from the same data has none.
void test_new_faces() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const double r = 5;
  const ShapePtr rounded = fillet("F5", *block, {edge(end_cap("F2", 1), side("F2", 1, 0))}, r);
  const std::vector<NewFace> faces = new_faces({block->occt()}, rounded->occt(), 9);
  CHECK(faces.size() == 5);
  double area = 0;
  int round = 0;
  for (const NewFace& face : faces) {
    area += face.area;
    CHECK(face.points.size() == 9);
    for (const gp_Pnt& p : face.points) {
      BRepExtrema_DistShapeShape on(BRepBuilderAPI_MakeVertex(p).Vertex(), rounded->occt());
      CHECK(on.IsDone() && on.Value() < 1e-7);
    }
    // The rounding's face: its points r from the edge's axis.
    if (near(face.area, kPi / 2 * r * 60, 1e-4)) {
      ++round;
      for (const gp_Pnt& p : face.points) {
        CHECK_NEAR(std::hypot(p.Y() - r, p.Z() - (20 - r)), r);
      }
    }
  }
  CHECK(round == 1);
  // All of the block's faces but the back and the bottom, less what the
  // rounding took, and the rounding.
  const double kept = 60.0 * 20.0 + 60.0 * 40.0;
  const double total = 2 * (60.0 * 40.0 + 60.0 * 20.0 + 40.0 * 20.0);
  const double taken = 2 * r * 60 + 2 * (r * r - kPi * r * r / 4);
  CHECK_NEAR(area, total - kept - taken + kPi / 2 * r * 60);
  // Nothing new against itself or a copy of itself.
  CHECK(new_faces({rounded->occt()}, rounded->occt(), 9).empty());
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(rounded->occt()).Shape();
  CHECK(new_faces({rounded->occt()}, copy, 9).empty());
  // Without bodies before, every face is new.
  CHECK(new_faces({}, rounded->occt(), 4).size() == 7);
}

} // namespace

void query_tests() {
  test_mass_properties_and_box();
  test_face_and_edge_infos();
  test_measured_from_threads();
  test_new_faces();
}

} // namespace test
