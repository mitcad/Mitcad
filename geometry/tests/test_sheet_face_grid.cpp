// SPDX-License-Identifier: MIT
// Faces sampled over their surface's parameters (mitcad#2442): a
// cylinder's side on its radius with radial normals out of the body and
// the face's own area, a cone's side with unit normals at its apex too, a
// box's top over its whole rectangle, and names of no face refused; their
// boundaries (mitcad#2558): a box top's and a cylinder side's wires on the
// grid's bounding lines.
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include "check.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/primitive.hpp"
#include "mitcad/geometry/sheet_face_grid.hpp"

#include "bundled_test.hpp"

namespace {
using namespace mitcad::geometry;

constexpr double kPi = 3.14159265358979323846;

void cylinder_grid() {
  PrimitiveSpec rod;
  rod.feature = "F1";
  rod.kind = PrimitiveKind::Cylinder;
  rod.a = 20;
  rod.c = 50;
  const std::vector<FaceGrid> pieces = sheet_face_grid(*primitive(rod), "F1:side0", 8, 4);
  CHECK(pieces.size() == 1);
  if (pieces.empty()) return;
  const FaceGrid& grid = pieces.front();
  CHECK(grid.points.size() == 9 * 5);
  CHECK(grid.normals.size() == 9 * 5);
  for (std::size_t k = 0; k < grid.points.size(); ++k) {
    const gp_Pnt& p = grid.points[k];
    const double r = std::hypot(p.X(), p.Y());
    CHECK_NEAR(r, 20.0);
    CHECK(p.Z() > -1e-9 && p.Z() < 50 + 1e-9);
    // Out of the body: along the radius, outwards.
    const gp_Vec& n = grid.normals[k];
    CHECK_NEAR(n.Magnitude(), 1.0);
    CHECK_NEAR((n.X() * p.X() + n.Y() * p.Y()) / r, 1.0);
  }
  CHECK_NEAR_TOL(grid.area, 2 * kPi * 20 * 50, 1e-6);
}

void cone_grid() {
  PrimitiveSpec cone;
  cone.feature = "F2";
  cone.kind = PrimitiveKind::Cone;
  cone.a = 30;
  cone.b = 0;
  cone.c = 40;
  const std::vector<FaceGrid> pieces = sheet_face_grid(*primitive(cone), "F2:side0", 12, 6);
  CHECK(pieces.size() == 1);
  if (pieces.empty()) return;
  const FaceGrid& grid = pieces.front();
  CHECK(grid.points.size() == 13 * 7);
  // Every normal a unit vector, the apex's too, pointing away from the
  // axis (out of the solid cone).
  for (std::size_t k = 0; k < grid.normals.size(); ++k) {
    const gp_Vec& n = grid.normals[k];
    CHECK_NEAR(n.Magnitude(), 1.0);
    CHECK(n.Z() > 0);
  }
  // The side's area: pi r s, s = 50.
  CHECK_NEAR_TOL(grid.area, kPi * 30 * 50, 1e-6);
}

void box_grid() {
  PrimitiveSpec box;
  box.feature = "F3";
  box.a = 60;
  box.b = 40;
  box.c = 20;
  const std::vector<FaceGrid> pieces = sheet_face_grid(*primitive(box), "F3:top", 3, 2);
  CHECK(pieces.size() == 1);
  if (pieces.empty()) return;
  const FaceGrid& grid = pieces.front();
  CHECK(grid.points.size() == 4 * 3);
  double least_x = 1e9, most_x = -1e9, least_y = 1e9, most_y = -1e9;
  for (const gp_Pnt& p : grid.points) {
    CHECK_NEAR(p.Z(), 20.0);
    least_x = std::fmin(least_x, p.X());
    most_x = std::fmax(most_x, p.X());
    least_y = std::fmin(least_y, p.Y());
    most_y = std::fmax(most_y, p.Y());
  }
  CHECK_NEAR(most_x - least_x, 60.0);
  CHECK_NEAR(most_y - least_y, 40.0);
  for (const gp_Vec& n : grid.normals) CHECK_NEAR(n.Z(), 1.0);
  CHECK_NEAR_TOL(grid.area, 2400.0, 1e-9);
  bool refused = false;
  try {
    sheet_face_grid(*primitive(box), "F3:nothing", 3, 2);
  } catch (const std::exception&) {
    refused = true;
  }
  CHECK(refused);
}

// Whether the place lies on one of the four lines bounding the grid.
bool on_bounds(const std::array<double, 2>& cell, double columns, double rows) {
  const double tolerance = 1e-6;
  return std::fabs(cell[0]) <= tolerance * columns || std::fabs(cell[0] - columns) <= tolerance * columns ||
         std::fabs(cell[1]) <= tolerance * rows || std::fabs(cell[1] - rows) <= tolerance * rows;
}

// The boundaries (mitcad#2558): a box's top, one outer wire of four edges
// along the grid's bounding lines; a cylinder's side, its circles and its
// seam twice, on its radius; none without edge steps.
void boundaries() {
  PrimitiveSpec box;
  box.feature = "F4";
  box.a = 60;
  box.b = 40;
  box.c = 20;
  const std::vector<FaceGrid> tops = sheet_face_grid(*primitive(box), "F4:top", 3, 2, 4);
  CHECK(tops.size() == 1);
  if (tops.empty()) return;
  CHECK(tops.front().wires.size() == 1);
  if (tops.front().wires.empty()) return;
  const FaceGridWire& wire = tops.front().wires.front();
  CHECK(wire.outer);
  CHECK(wire.edges.size() == 4);
  double length = 0;
  for (const FaceGridEdge& edge : wire.edges) {
    CHECK(edge.cells.size() == 5);
    CHECK(edge.points.size() == 5);
    CHECK(edge.normals.size() == 5);
    for (std::size_t k = 0; k < edge.points.size(); ++k) {
      CHECK(on_bounds(edge.cells[k], 3, 2));
      CHECK_NEAR(edge.points[k].Z(), 20.0);
      CHECK_NEAR(edge.normals[k].Z(), 1.0);
      if (k > 0) length += edge.points[k].Distance(edge.points[k - 1]);
    }
  }
  // The wire runs round the top: its perimeter.
  CHECK_NEAR_TOL(length, 200.0, 1e-9);
  // Its edges follow one another.
  for (std::size_t e = 0; e < wire.edges.size(); ++e) {
    const FaceGridEdge& next = wire.edges[(e + 1) % wire.edges.size()];
    CHECK(wire.edges[e].points.back().Distance(next.points.front()) < 1e-6);
  }
  CHECK(sheet_face_grid(*primitive(box), "F4:top", 3, 2).front().wires.empty());

  PrimitiveSpec rod;
  rod.feature = "F5";
  rod.kind = PrimitiveKind::Cylinder;
  rod.a = 20;
  rod.c = 50;
  const std::vector<FaceGrid> sides = sheet_face_grid(*primitive(rod), "F5:side0", 8, 4, 8);
  CHECK(sides.size() == 1);
  if (sides.empty()) return;
  CHECK(sides.front().wires.size() == 1);
  for (const FaceGridWire& w : sides.front().wires) {
    CHECK(w.outer);
    CHECK(w.edges.size() == 4);
    for (const FaceGridEdge& edge : w.edges) {
      for (std::size_t k = 0; k < edge.points.size(); ++k) {
        CHECK(on_bounds(edge.cells[k], 8, 4));
        CHECK_NEAR(std::hypot(edge.points[k].X(), edge.points[k].Y()), 20.0);
      }
    }
  }
}
} // namespace

MITCAD_BUNDLED_TEST(sheet_face_grid) {
  enable_input_checks(true);
  test::guarded("cylinder grid", cylinder_grid);
  test::guarded("cone grid", cone_grid);
  test::guarded("box grid", box_grid);
  test::guarded("boundaries", boundaries);
  CHECK(input_changes().empty());
  if (test::failures == 0) std::puts("test_sheet_face_grid: all checks passed");
  return test::failures == 0 ? 0 : 1;
}
