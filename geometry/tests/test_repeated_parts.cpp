// SPDX-License-Identifier: MIT
// Repeated parts (mitcad#1277): copies of a body moved by rigid motions are
// found as copies of the first, also where the body's symmetry gives it
// several frames of principal axes; a mirror image of a body without a
// mirror symmetry is not a copy, nor is another body.

#include <array>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <exception>
#include <memory>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/geometry.hpp"
#include "mitcad/geometry/repeated_parts.hpp"

namespace {

using namespace mitcad::geometry;
using test::kPi;

TopoDS_Shape box(double x0, double y0, double z0, double x1, double y1, double z1) {
  return BRepPrimAPI_MakeBox(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1)).Shape();
}

// A core with arms at its foot, each arm `turn` radians on from the one
// before about z: four arms a quarter turn apart make a pinwheel (equal
// moments about x and y, so its frame about z comes from its farthest
// points), two arms a half turn apart a propeller (three different
// moments). Neither has a mirror symmetry.
ShapePtr arms(int count) {
  TopoDS_Shape shape = box(-5.0, -5.0, 0.0, 5.0, 5.0, 10.0);
  // The arm at +x, and the others turned about z.
  const std::array<std::array<double, 4>, 4> footprints{{
      {5.0, -5.0, 20.0, 0.0},
      {0.0, 5.0, 5.0, 20.0},
      {-20.0, 0.0, -5.0, 5.0},
      {-5.0, -20.0, 0.0, -5.0},
  }};
  const int step = count == 4 ? 1 : 2;
  for (int k = 0; k < 4; k += step) {
    const auto& f = footprints[static_cast<std::size_t>(k)];
    BRepAlgoAPI_Fuse fuse(shape, box(f[0], f[1], 0.0, f[2], f[3], 4.0));
    fuse.Build();
    CHECK(fuse.IsDone());
    shape = fuse.Shape();
  }
  return std::make_shared<Shape>(shape);
}

Affine rotation(std::array<double, 3> axis, double angle, std::array<double, 3> translation) {
  const double n = std::sqrt(axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]);
  const double x = axis[0] / n;
  const double y = axis[1] / n;
  const double z = axis[2] / n;
  const double c = std::cos(angle);
  const double s = std::sin(angle);
  const double t = 1.0 - c;
  Affine map;
  map.linear = {{{t * x * x + c, t * x * y - s * z, t * x * z + s * y},
                 {t * x * y + s * z, t * y * y + c, t * y * z - s * x},
                 {t * x * z - s * y, t * y * z + s * x, t * z * z + c}}};
  map.translation = translation;
  return map;
}

std::array<double, 3> placed(const Affine& map, const gp_Pnt& p) {
  std::array<double, 3> out{};
  for (std::size_t r = 0; r < 3; ++r) {
    out[r] = map.linear[r][0] * p.X() + map.linear[r][1] * p.Y() + map.linear[r][2] * p.Z() + map.translation[r];
  }
  return out;
}

// Whether `map` puts every vertex of `from` on a vertex of `to`.
bool lays_on(const Shape& from, const Shape& to, const Affine& map) {
  for (int i = 0; i < from.vertex_count(); ++i) {
    const std::array<double, 3> p = placed(map, BRep_Tool::Pnt(from.vertex(i)));
    bool found = false;
    for (int j = 0; j < to.vertex_count() && !found; ++j) {
      const gp_Pnt q = BRep_Tool::Pnt(to.vertex(j));
      found = std::abs(p[0] - q.X()) < 1e-6 && std::abs(p[1] - q.Y()) < 1e-6 && std::abs(p[2] - q.Z()) < 1e-6;
    }
    if (!found) {
      return false;
    }
  }
  return true;
}

bool rotation_proper(const Affine& map) {
  const auto& m = map.linear;
  const double det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) -
                     m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) +
                     m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
  return std::abs(det - 1.0) < 1e-9;
}

void test_copies_of_a_pinwheel_and_a_propeller() {
  const ShapePtr pinwheel = arms(4);
  const ShapePtr propeller = arms(2);
  std::vector<ShapePtr> bodies;
  // 0: the pinwheel as made.
  bodies.push_back(pinwheel);
  // 1: moved along x.
  bodies.push_back(move_shape(*pinwheel, rotation({0.0, 0.0, 1.0}, 0.0, {50.0, 0.0, 0.0})));
  // 2: turned by its symmetry, a quarter turn about z, and moved.
  bodies.push_back(move_shape(*pinwheel, rotation({0.0, 0.0, 1.0}, kPi / 2.0, {0.0, 60.0, 0.0})));
  // 3: the propeller.
  bodies.push_back(propeller);
  // 4: the pinwheel turned about a slanted axis and moved.
  bodies.push_back(move_shape(*pinwheel, rotation({1.0, 2.0, 3.0}, 0.7, {-40.0, 10.0, 25.0})));
  // 5: the pinwheel's mirror image (x to -x), moved: not a copy.
  Affine mirror;
  mirror.linear = {{{-1.0, 0.0, 0.0}, {0.0, 1.0, 0.0}, {0.0, 0.0, 1.0}}};
  mirror.translation = {0.0, -60.0, 0.0};
  bodies.push_back(transform_shape(*pinwheel, mirror));
  // 6: the propeller turned a half turn about its axis, then about a
  // slanted axis, and moved.
  const ShapePtr half = move_shape(*propeller, rotation({0.0, 0.0, 1.0}, kPi, {0.0, 0.0, 0.0}));
  bodies.push_back(move_shape(*half, rotation({-2.0, 1.0, 0.5}, 1.9, {120.0, -30.0, 7.0})));

  std::vector<const Shape*> inputs;
  for (const ShapePtr& body : bodies) {
    inputs.push_back(body.get());
  }
  const std::vector<RigidCopy> copies = rigid_copies(inputs, 1e-3);
  CHECK(copies.size() == bodies.size());
  if (copies.size() != bodies.size()) {
    return;
  }
  const std::array<std::size_t, 7> expected{0, 0, 0, 3, 0, 5, 3};
  for (std::size_t i = 0; i < copies.size(); ++i) {
    if (copies[i].of != expected[i]) {
      std::fprintf(stderr, "body %zu: a copy of %zu, expected %zu\n", i, copies[i].of, expected[i]);
    }
    CHECK(copies[i].of == expected[i]);
    CHECK(rotation_proper(copies[i].placement));
    CHECK(lays_on(*bodies[copies[i].of], *bodies[i], copies[i].placement));
  }
  // Four occurrences of the pinwheel; the mirror image is a part of its own.
  int pinwheels = 0;
  for (const RigidCopy& copy : copies) {
    pinwheels += copy.of == 0 ? 1 : 0;
  }
  CHECK(pinwheels == 4);
  // The bodies are left as they were: the first one's vertices where they
  // were made.
  CHECK(lays_on(*pinwheel, *arms(4), Affine{}));
}

void test_tolerance_must_be_positive() {
  const ShapePtr body = arms(2);
  bool refused = false;
  try {
    rigid_copies({body.get()}, 0.0);
  } catch (const std::exception&) {
    refused = true;
  }
  CHECK(refused);
}

} // namespace

int main() {
  test::guarded("copies of a pinwheel and a propeller", test_copies_of_a_pinwheel_and_a_propeller);
  test::guarded("tolerance must be positive", test_tolerance_must_be_positive);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("repeated parts: all checks passed\n");
  return 0;
}
