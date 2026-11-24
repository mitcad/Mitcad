// SPDX-License-Identifier: MIT
// A mill-turn's stock cut in 3D in Simulate (mitcad#2319,
// cam/CamStock.hpp solidStockMesh): the cubes' surface read from the
// `solid` of `cam_simulate`, each triangle with its own corners and its
// face's normal; nothing drawn from a mesh that does not read.
#include "UnitTests.hpp"
#include "cam/CamStock.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_solid_stock.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// A square face of a cube: two triangles over four corners, turning their
// front up (+Z).
QJsonObject square() {
  return {{QStringLiteral("mesh"),
           QJsonObject{{QStringLiteral("vertices"), QJsonArray{0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0}},
                       {QStringLiteral("triangles"), QJsonArray{0, 1, 2, 0, 2, 3}}}}};
}

} // namespace

int camSolidStockTests() {
  using mitcad::cam::solidStockMesh;
  const mitcad::cam::StockMesh face = solidStockMesh(square());
  CHECK(face.triangleCount() == 2);
  CHECK(face.vertices() == 6);
  CHECK(!face.remaining);
  // Every corner's normal is the face's: up.
  bool up = true;
  for (int v = 0; v < face.vertices(); ++v) {
    up = up && std::abs(face.normals[3 * v + 2] - 1.0f) < 1e-6f && std::abs(face.normals[3 * v]) < 1e-6f;
  }
  CHECK(up);
  CHECK(face.points[2] == 1.0f);
  // A triangle naming a corner the mesh does not have: nothing.
  QJsonObject broken = square();
  QJsonObject mesh = broken.value(QStringLiteral("mesh")).toObject();
  mesh.insert(QStringLiteral("triangles"), QJsonArray{0, 1, 7});
  broken.insert(QStringLiteral("mesh"), mesh);
  CHECK(solidStockMesh(broken).empty());
  CHECK(solidStockMesh(QJsonObject{}).empty());
  return failures;
}

MITCAD_UNIT_TEST(camSolidStockTests);
