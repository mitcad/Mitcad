// SPDX-License-Identifier: MIT
// The `grain` projection's directions in the application
// (render/GrainProjection.hpp, mitcad#2186) agree with the model's rule
// (core/model-base/src/appearance/grain.rs, whose tests these mirror): the
// grain runs along each face's longest side, a face seen from outside is
// not mirrored, a square face takes the world axis lying longest in it, a
// face's mesh does not change its directions, and a whole panel's mesh
// (MeshData, faces by faceTriangles) gets each vertex's grain coordinates
// in mm from its own face; each placed body's grain offset (mitcad#2605)
// is the model's, shifts its coordinates only, and gives it a Cycles mesh
// of its own.
#include "UnitTests.hpp"
#include "render/GrainProjection.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <vector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "grain_projection.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using V3 = std::array<double, 3>;
using Triangle = std::array<V3, 3>;
using mitcad::render::GrainAxes;
using mitcad::render::grainAxes;

V3 add(const V3& a, const V3& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }
V3 scaled(const V3& a, double s) { return {a[0] * s, a[1] * s, a[2] * s}; }
V3 cross(const V3& a, const V3& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}
bool close(const V3& a, const V3& b) {
  return std::abs(a[0] - b[0]) < 1e-9 && std::abs(a[1] - b[1]) < 1e-9 && std::abs(a[2] - b[2]) < 1e-9;
}

// A rectangle's two triangles: corner o, sides a and b (a x b out of it).
std::vector<Triangle> rectangle(const V3& o, const V3& a, const V3& b) {
  const auto p = [&](double s, double t) { return add(add(o, scaled(a, s)), scaled(b, t)); };
  return {Triangle{p(0, 0), p(1, 0), p(1, 1)}, Triangle{p(0, 0), p(1, 1), p(0, 1)}};
}

GrainAxes axesOf(const std::vector<Triangle>& triangles) {
  return grainAxes(triangles.size(), [&triangles](std::size_t i) { return triangles[i]; });
}

} // namespace

int grainProjectionTests() {
  const double l = 800.0;
  const double w = 300.0;
  const double t = 18.0;
  {
    // A shelf panel lying flat: top, bottom (not mirrored seen from
    // below), a long edge along the length, a short end along the width,
    // and the panel upright (a cabinet's side) along Z.
    GrainAxes top = axesOf(rectangle({0, 0, t}, {l, 0, 0}, {0, w, 0}));
    CHECK(close(top.u, {1, 0, 0}) && close(top.v, {0, 1, 0}));
    GrainAxes bottom = axesOf(rectangle({0, 0, 0}, {0, w, 0}, {l, 0, 0}));
    CHECK(close(bottom.u, {1, 0, 0}) && close(cross(bottom.u, bottom.v), {0, 0, -1}));
    GrainAxes front = axesOf(rectangle({0, 0, 0}, {l, 0, 0}, {0, 0, t}));
    CHECK(close(front.u, {1, 0, 0}) && close(cross(front.u, front.v), {0, -1, 0}));
    GrainAxes end = axesOf(rectangle({l, 0, 0}, {0, w, 0}, {0, 0, t}));
    CHECK(close(end.u, {0, 1, 0}));
    GrainAxes side = axesOf(rectangle({0, 0, 0}, {0, w, 0}, {0, 0, l}));
    CHECK(close(side.u, {0, 0, 1}));
  }
  {
    // A turned rectangle follows its length; a finer mesh of it agrees.
    const double c = std::cos(0.5);
    const double s = std::sin(0.5);
    const V3 along{600 * c, 600 * s, 0};
    const V3 across{-200 * s, 200 * c, 0};
    const GrainAxes coarse = axesOf(rectangle({10, 20, 5}, along, across));
    CHECK(close(coarse.u, {c, s, 0}) && close(coarse.v, {-s, c, 0}));
    std::vector<Triangle> fine;
    for (int i = 0; i < 10; ++i) {
      for (int j = 0; j < 3; ++j) {
        const V3 o = add(add(V3{10, 20, 5}, scaled(along, i / 10.0)), scaled(across, j / 3.0));
        for (const Triangle& triangle : rectangle(o, scaled(along, 0.1), scaled(across, 1.0 / 3.0))) {
          fine.push_back(triangle);
        }
      }
    }
    const GrainAxes refined = axesOf(fine);
    CHECK(close(refined.u, coarse.u) && close(refined.v, coarse.v));
    const std::array<double, 2> point = mitcad::render::grainPoint(coarse, {c * 100, s * 100, 7});
    CHECK(std::abs(point[0] - 100.0) < 1e-9 && std::abs(point[1]) < 1e-9);
  }
  {
    // Square faces take the world axis lying longest in them (X, then Y);
    // a face without area X and Y.
    const GrainAxes yz = axesOf(rectangle({0, 0, 0}, {0, 400, 0}, {0, 0, 400}));
    CHECK(close(yz.u, {0, 1, 0}) && close(yz.v, {0, 0, 1}));
    const GrainAxes xy = axesOf(rectangle({0, 0, 0}, {400, 0, 0}, {0, 400, 0}));
    CHECK(close(xy.u, {1, 0, 0}));
    const GrainAxes none = axesOf({Triangle{}});
    CHECK(close(none.u, {1, 0, 0}) && close(none.v, {0, 1, 0}));
  }
  {
    // A dowel's side: along its axis.
    std::vector<Triangle> dowel;
    const double r = 10.0;
    const double h = 200.0;
    const int n = 48;
    for (int i = 0; i < n; ++i) {
      const double a0 = 2.0 * 3.14159265358979323846 * i / n;
      const double a1 = 2.0 * 3.14159265358979323846 * (i + 1) / n;
      const auto p = [r](double a, double z) { return V3{r * std::cos(a), r * std::sin(a), z}; };
      dowel.push_back({p(a0, 0), p(a1, 0), p(a1, h)});
      dowel.push_back({p(a0, 0), p(a1, h), p(a0, h)});
    }
    CHECK(close(axesOf(dowel).u, {0, 0, 1}));
  }
  {
    // A panel's mesh, face by face with their own vertices: each vertex's
    // grain coordinates by its face (the top's along X and Y, the short
    // end's along Y and Z), and its face's directions.
    mitcad::render::MeshData mesh;
    const auto face = [&mesh](const std::vector<Triangle>& triangles) {
      for (const Triangle& triangle : triangles) {
        for (const V3& p : triangle) {
          mesh.indices.push_back(static_cast<std::uint32_t>(mesh.positions.size() / 3));
          for (const double x : p) {
            mesh.positions.push_back(static_cast<float>(x));
          }
        }
      }
      mesh.faceTriangles.push_back(static_cast<std::uint32_t>(triangles.size()));
    };
    face(rectangle({0, 0, t}, {l, 0, 0}, {0, w, 0}));
    face(rectangle({l, 0, 0}, {0, w, 0}, {0, 0, t}));
    const mitcad::render::GrainMesh grain = mitcad::render::grainMeshOf(mesh);
    CHECK(grain.coordinates.size() == mesh.positions.size() / 3 * 2);
    CHECK(grain.u.size() == mesh.positions.size());
    // The top's third corner (l, w, t): (l, w).
    CHECK(std::abs(grain.coordinates[2 * 2] - l) < 1e-3 && std::abs(grain.coordinates[2 * 2 + 1] - w) < 1e-3);
    const auto near = [](float a, double b) { return std::abs(static_cast<double>(a) - b) < 1e-6; };
    CHECK(near(grain.u[0], 1) && near(grain.u[1], 0) && near(grain.u[2], 0));
    // The end's third corner (l, w, t): (w, t), its u along Y.
    CHECK(std::abs(grain.coordinates[8 * 2] - w) < 1e-3 && std::abs(grain.coordinates[8 * 2 + 1] - t) < 1e-3);
    CHECK(near(grain.u[6 * 3], 0) && near(grain.u[6 * 3 + 1], 1) && near(grain.u[6 * 3 + 2], 0));

    // A body's grain offset (mitcad#2605) shifts every vertex's
    // coordinates by the same amount, on every face, and leaves the
    // directions as they are: the repeat and the grain stay.
    const std::array<double, 2> offset = mitcad::render::grainOffset("F3.b0");
    const mitcad::render::GrainMesh shifted = mitcad::render::grainMeshOf(mesh, offset);
    CHECK(shifted.coordinates.size() == grain.coordinates.size());
    bool same = shifted.u == grain.u && shifted.v == grain.v;
    for (std::size_t i = 0; i < grain.coordinates.size(); ++i) {
      const double moved = static_cast<double>(shifted.coordinates[i]) - static_cast<double>(grain.coordinates[i]);
      same = same && std::abs(moved - offset[i % 2]) < 2e-3;
    }
    CHECK(same);
  }
  {
    // The grain offsets are the model's (grain.rs,
    // separate_bodies_take_different_parts_of_the_grain): fixed values by
    // the body's key, so a body keeps its part of the images on a redraw,
    // a move and a reopening, and the shaded view, the rendered view and
    // the exports agree. Two pine boards side by side (F3.b0, F4.b0) lie
    // far apart within the woods' 800 mm repeat.
    using mitcad::render::grainKey;
    using mitcad::render::grainOffset;
    const auto near = [](const std::array<double, 2>& a, double u, double v) {
      return std::abs(a[0] - u) < 1e-6 && std::abs(a[1] - v) < 1e-6;
    };
    CHECK(near(grainOffset("F3.b0"), 8159.940994810313, 5121.577237732708));
    CHECK(near(grainOffset("F4.b0"), 1406.768704764545, 9617.54277581349));
    CHECK(grainKey("F3.b0", "") == "F3.b0");
    CHECK(grainKey("F3.b0", "O1/O4") == "F3.b0@O1/O4");
    CHECK(near(grainOffset(grainKey("F3.b0", "O1/O4")), 6916.328517254442, 6243.558961432427));
    const auto apart = [](const std::array<double, 2>& a, const std::array<double, 2>& b, double repeat) {
      double most = 0.0;
      for (int k = 0; k < 2; ++k) {
        double d = std::fmod(a[k] - b[k], repeat);
        if (d < 0.0) {
          d += repeat;
        }
        most = std::max(most, std::min(d, repeat - d));
      }
      return most;
    };
    CHECK(apart(grainOffset("F3.b0"), grainOffset("F4.b0"), 800.0) > 100.0);
    CHECK(apart(grainOffset("F1.b0"), grainOffset("F1.b1"), 800.0) > 50.0);
    CHECK(apart(grainOffset("F3.b0@O1"), grainOffset("F3.b0@O2"), 800.0) > 50.0);
  }
  {
    // The rendered view (mitcad#2605): a body laid along the grain carries
    // its offset in its Cycles mesh, so its look names the body, and two
    // occurrences of one mesh no longer share one; without the grain they
    // still do.
    mitcad::render::BodyData a;
    a.id = "F3.b0@O1";
    a.mesh = "mesh";
    a.materialKey = "pine";
    mitcad::render::BodyData b = a;
    b.id = "F3.b0@O2";
    CHECK(a.lookKey() == b.lookKey());
    a.material.textureGrain = true;
    b.material.textureGrain = true;
    CHECK(a.lookKey() != b.lookKey());
    CHECK(mitcad::render::alongGrain(a));
    // A face group along the grain is enough.
    mitcad::render::BodyData c = b;
    c.material.textureGrain = false;
    mitcad::render::FaceMaterial group;
    group.faces = {0};
    group.material.textureGrain = true;
    group.materialKey = "pine";
    c.faces = {group};
    CHECK(mitcad::render::alongGrain(c));
    mitcad::render::BodyData d = c;
    d.id = "F3.b0@O3";
    CHECK(c.lookKey() != d.lookKey());
  }
  return failures;
}
MITCAD_UNIT_TEST(grainProjectionTests);
