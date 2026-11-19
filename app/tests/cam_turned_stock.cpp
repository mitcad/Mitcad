// SPDX-License-Identifier: MIT
// A lathe setup's turned stock in Simulate (mitcad#1869,
// cam/CamStock.hpp turnedStockMesh): the simulated section revolved about
// the work Z, its bands only where a radius changes, the ends of the
// material as rings, a bore as an inner band, and the points in the design
// by the work frame.
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
    std::fprintf(stderr, "cam_turned_stock.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// The work frame: Z along the design's X (the spindle axis), the origin at
// (10, 0, 0).
mitcad::cam::WorkFrame axisFrame() {
  return mitcad::cam::WorkFrame::of({{QStringLiteral("origin"), QJsonArray{10.0, 0.0, 0.0}},
                                     {QStringLiteral("x"), QJsonArray{0.0, 1.0, 0.0}},
                                     {QStringLiteral("y"), QJsonArray{0.0, 0.0, 1.0}},
                                     {QStringLiteral("z"), QJsonArray{1.0, 0.0, 0.0}}});
}

QJsonObject section(const QJsonArray& inner) {
  // Four nodes along Z: material of radius 5 on the first three, none on
  // the last (parted off).
  return {{QStringLiteral("z"), QJsonArray{-3.0, -2.0, -1.0, 0.0}},
          {QStringLiteral("inner"), inner},
          {QStringLiteral("outer"), QJsonArray{0.0, 5.0, 5.0, 5.0}}};
}

} // namespace

int camTurnedStockTests() {
  using mitcad::cam::turnedStockMesh;
  // A bar with nothing left at Z -3: the outside band from -2 to 0 and a ring at
  // each end (the middle node adds nothing), 12 steps round.
  const mitcad::cam::StockMesh bar = turnedStockMesh(section(QJsonArray{0.0, 0.0, 0.0, 0.0}), axisFrame(), 12);
  CHECK(bar.vertices() == 3 * 2 * 13);
  CHECK(bar.triangleCount() == 3 * 2 * 12);
  CHECK(!bar.remaining);
  // Every point within the bar's radius of the axis (design X), between
  // the design's X 8 and 10.
  bool within = true;
  for (int v = 0; v < bar.vertices(); ++v) {
    const float x = bar.points[3 * v];
    const float y = bar.points[3 * v + 1];
    const float z = bar.points[3 * v + 2];
    within = within && x >= 7.999f && x <= 10.001f && std::sqrt(y * y + z * z) <= 5.001f;
  }
  CHECK(within);
  // A tube: the bore's band too.
  const mitcad::cam::StockMesh tube = turnedStockMesh(section(QJsonArray{0.0, 2.0, 2.0, 2.0}), axisFrame(), 12);
  CHECK(tube.vertices() == 4 * 2 * 13);
  CHECK(tube.triangleCount() == 4 * 2 * 12);
  // Nothing to draw without the section.
  CHECK(turnedStockMesh(QJsonObject{}, axisFrame(), 12).empty());
  return failures;
}

MITCAD_UNIT_TEST(camTurnedStockTests);
