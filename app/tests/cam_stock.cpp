// SPDX-License-Identifier: MIT
// The stock in Simulate (mitcad#1013, cam/CamStock.hpp): the grid read
// from the query, runs and their changes, the surface with its sides and
// shades by the material left, the snapshots' places along the playback
// and the log lines the UI tests read.
#include "cam/CamStock.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_stock.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::cam::StockGrid;

// A grid of 3 x 2 nodes 1 mm apart from (0, 0), the stock from -10 to 0.
QJsonObject gridJson() {
  return {{QStringLiteral("origin"), QJsonArray{0.0, 0.0}},
          {QStringLiteral("step"), QJsonArray{1.0, 1.0}},
          {QStringLiteral("nx"), 3},
          {QStringLiteral("ny"), 2},
          {QStringLiteral("bottom"), -10.0},
          {QStringLiteral("top"), 0.0}};
}

// The design's own axes at the model origin.
mitcad::cam::WorkFrame modelFrame() {
  return mitcad::cam::WorkFrame::of({{QStringLiteral("origin"), QJsonArray{0.0, 0.0, 0.0}},
                                     {QStringLiteral("x"), QJsonArray{1.0, 0.0, 0.0}},
                                     {QStringLiteral("y"), QJsonArray{0.0, 1.0, 0.0}},
                                     {QStringLiteral("z"), QJsonArray{0.0, 0.0, 1.0}}});
}

void gridTests() {
  const StockGrid grid = StockGrid::of(gridJson());
  CHECK(grid.valid());
  CHECK(grid.nodes() == 6);
  CHECK(grid.bottom == -10.0);
  CHECK(!StockGrid::of(QJsonObject{}).valid());
}

void runsTests() {
  std::vector<float> z(6, 0.0f);
  CHECK(mitcad::cam::applyRuns(QJsonArray{0, 2, -1250, 3, 0, 1}, z));
  CHECK(z[0] == 0.0f && z[2] == -1.25f && z[4] == -1.25f && z[5] == 0.0f);
  // Changes: two unchanged, one lowered, three unchanged.
  CHECK(mitcad::cam::applyRuns(QJsonArray{0, -2, -3000, 1, 0, -3}, z));
  CHECK(z[1] == 0.0f && z[2] == -3.0f && z[3] == -1.25f);
  CHECK(!mitcad::cam::applyRuns(QJsonArray{0, 5}, z));
  CHECK(!mitcad::cam::applyRuns(QJsonArray{0, 7}, z));
  CHECK(!mitcad::cam::applyRuns(QJsonArray{0}, z));
}

void meshTests() {
  const StockGrid grid = StockGrid::of(gridJson());
  const std::vector<float> flat(6, 0.0f);
  const mitcad::cam::StockMesh mesh = mitcad::cam::stockMesh(grid, flat, modelFrame());
  // Two cells of two triangles; six sides of two triangles (two along X
  // on each long edge, one along Y on each short edge).
  CHECK(mesh.triangleCount() == 4 + 2 * 6);
  CHECK(mesh.vertices() == 6 + 4 * 6);
  CHECK(!mesh.remaining);
  CHECK(mesh.points.size() == mesh.normals.size());
  CHECK(static_cast<int>(mesh.shades.size()) == mesh.vertices());
  // A flat top faces up.
  CHECK(std::abs(mesh.normals[2] - 1.0f) < 1e-6f);
  CHECK(mesh.log() == QStringLiteral("CAM stock overlay: 30 vertices, 16 triangles, plain"));
  // A stock cut through everywhere has nothing to draw.
  const std::vector<float> through(6, -10.0f);
  CHECK(mitcad::cam::stockMesh(grid, through, modelFrame()).empty());
  CHECK(mitcad::cam::stockMesh(grid, through, modelFrame()).log() == QStringLiteral("CAM stock overlay: none"));
  // Heights that do not fit the grid draw nothing.
  CHECK(mitcad::cam::stockMesh(grid, std::vector<float>(5, 0.0f), modelFrame()).empty());

  // Shaded by the material left over a part 1 mm under the top: the first
  // node 1 mm thick, the last cut 0.5 mm into it, the rest at the part.
  const std::vector<float> part(6, -1.0f);
  std::vector<float> cut(6, -1.0f);
  cut[0] = 0.0f;
  cut[5] = -1.5f;
  const mitcad::cam::StockMesh shaded = mitcad::cam::stockMesh(grid, cut, modelFrame(), &part, 0.01, 1.0);
  CHECK(shaded.remaining);
  CHECK(std::abs(shaded.shades[0] - 1.0f) < 1e-6f);
  CHECK(shaded.shades[5] == -1.0f);
  CHECK(shaded.shades[2] == 0.0f);
  CHECK(shaded.log().endsWith(QStringLiteral("by the material left")));
}

void frameTests() {
  CHECK(mitcad::cam::framePlaces(0, 10).empty());
  CHECK(mitcad::cam::framePlaces(3, 10) == (std::vector<int>{1, 2}));
  const std::vector<int> places = mitcad::cam::framePlaces(1001, 4);
  CHECK(places == (std::vector<int>{250, 500, 750, 1000}));
  CHECK(mitcad::cam::frameAt(places, 0) == -1);
  CHECK(mitcad::cam::frameAt(places, 250) == 0);
  CHECK(mitcad::cam::frameAt(places, 749) == 1);
  CHECK(mitcad::cam::frameAt(places, 1000) == 3);
}

void logTests() {
  CHECK(mitcad::cam::stockLog(QStringLiteral("Setup 1"), StockGrid::of(gridJson()), 60) ==
        QStringLiteral("CAM stock: Setup 1, 3 x 2 nodes, 60 snapshots"));
  const QJsonObject remaining{{QStringLiteral("max"), 0.5},
                              {QStringLiteral("mean"), 0.125},
                              {QStringLiteral("gouge"), 0.0},
                              {QStringLiteral("beside"), 0.0},
                              {QStringLiteral("thickest"), QJsonArray{QJsonObject{}, QJsonObject{}}}};
  CHECK(mitcad::cam::remainingLog(QStringLiteral("Setup 1"), remaining) ==
        QStringLiteral("CAM remaining material: Setup 1: max 0.5, mean 0.125, gouge 0, beside 0, 2 place(s)"));
}

} // namespace

int camStockTests() {
  failures = 0;
  gridTests();
  runsTests();
  meshTests();
  frameTests();
  logTests();
  return failures;
}
