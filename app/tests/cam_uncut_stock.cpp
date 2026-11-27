// SPDX-License-Identifier: MIT
// The uncut stock (mitcad#2552, cam/CamStock.hpp uncutStock): a mill
// setup's box in a translated and rotated work frame, a lathe setup's bar
// with its diameter and ends on the spindle axis, a tube with its bore,
// every triangle wound round its normal (the box's least sides too), the
// setups shown together, a hidden or failed setup drawing nothing, and the
// log line.
#include "UnitTests.hpp"
#include "cam/CamStock.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "cam_uncut_stock.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::cam::StockMesh;
using mitcad::cam::UncutStock;

bool sameAs(double a, double b, double tolerance = 1e-3) {
  return std::abs(a - b) <= tolerance;
}

// A mill setup's work: the frame turned a quarter about the design's Z
// (work X along the design's Y, work Y along the design's -X), its origin
// at (100, 50, 20), the stock 100 x 50 x 20 under the origin.
QJsonObject millWork() {
  return {{QStringLiteral("origin"), QJsonArray{100.0, 50.0, 20.0}},
          {QStringLiteral("x"), QJsonArray{0.0, 1.0, 0.0}},
          {QStringLiteral("y"), QJsonArray{-1.0, 0.0, 0.0}},
          {QStringLiteral("z"), QJsonArray{0.0, 0.0, 1.0}},
          {QStringLiteral("stock_min"), QJsonArray{0.0, 0.0, -20.0}},
          {QStringLiteral("stock_max"), QJsonArray{100.0, 50.0, 0.0}}};
}

// A lathe setup's work: the spindle axis (work Z) along the design's X,
// the origin at (10, 0, 0); a bar 50 across from its face at 1 back to
// -299, with a bore of radius `inner` (none at 0). The milling fields as
// the model gives them (a box about the axis).
QJsonObject latheWork(double inner) {
  return {{QStringLiteral("origin"), QJsonArray{10.0, 0.0, 0.0}},
          {QStringLiteral("x"), QJsonArray{0.0, 1.0, 0.0}},
          {QStringLiteral("y"), QJsonArray{0.0, 0.0, 1.0}},
          {QStringLiteral("z"), QJsonArray{1.0, 0.0, 0.0}},
          {QStringLiteral("stock_min"), QJsonArray{-25.0, -25.0, -299.0}},
          {QStringLiteral("stock_max"), QJsonArray{25.0, 25.0, 1.0}},
          {QStringLiteral("turning"),
           QJsonObject{{QStringLiteral("part"), QJsonObject{{QStringLiteral("stock_radius"), 25.0},
                                                            {QStringLiteral("stock_inner_radius"), inner},
                                                            {QStringLiteral("stock_face"), 1.0}}},
                       {QStringLiteral("stock_back"), -299.0}}}};
}

QJsonObject setup(const QJsonObject& work, bool visible) {
  return {{QStringLiteral("visible"), visible}, {QStringLiteral("work"), work}};
}

// Every triangle that is not a sliver turns about its vertices' normals.
bool woundRoundNormals(const StockMesh& mesh) {
  const auto point = [&mesh](int v, int k) {
    return static_cast<double>(mesh.points[static_cast<std::size_t>(3 * v + k)]);
  };
  const auto normal = [&mesh](int v, int k) {
    return static_cast<double>(mesh.normals[static_cast<std::size_t>(3 * v + k)]);
  };
  for (std::size_t t = 0; t + 2 < mesh.triangles.size(); t += 3) {
    const int a = mesh.triangles[t];
    const int b = mesh.triangles[t + 1];
    const int c = mesh.triangles[t + 2];
    double u[3];
    double w[3];
    double n[3];
    for (int k = 0; k < 3; ++k) {
      u[k] = point(b, k) - point(a, k);
      w[k] = point(c, k) - point(a, k);
      n[k] = normal(a, k) + normal(b, k) + normal(c, k);
    }
    const double cross[3] = {u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]};
    const double area = std::sqrt(cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]);
    if (area < 1e-6) {
      continue;
    }
    if (cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2] <= 0.0) {
      return false;
    }
  }
  return true;
}

// The distance of a point from the design's X axis (the lathe's spindle).
double fromAxis(const std::vector<float>& coords, std::size_t at) {
  const double y = coords[at + 1];
  const double z = coords[at + 2];
  return std::sqrt(y * y + z * z);
}

} // namespace

int camUncutStockTests() {
  using mitcad::cam::uncutStock;
  using mitcad::cam::uncutStockOfSetups;

  // The mill's box: six faces of two triangles and twelve edges, its
  // corners in the design by the turned and moved frame: X from 50 to 100,
  // Y from 50 to 150, Z from 0 to 20.
  const UncutStock box = uncutStock(millWork());
  CHECK(box.surface.vertices() == 24);
  CHECK(box.surface.triangleCount() == 12);
  CHECK(box.edgeCount() == 12);
  CHECK(box.kinds == QStringList{QStringLiteral("box")});
  double lo[3] = {1e9, 1e9, 1e9};
  double hi[3] = {-1e9, -1e9, -1e9};
  for (std::size_t k = 0; k + 2 < box.surface.points.size(); k += 3) {
    for (std::size_t axis = 0; axis < 3; ++axis) {
      lo[axis] = std::min(lo[axis], static_cast<double>(box.surface.points[k + axis]));
      hi[axis] = std::max(hi[axis], static_cast<double>(box.surface.points[k + axis]));
    }
  }
  CHECK(sameAs(lo[0], 50.0) && sameAs(hi[0], 100.0));
  CHECK(sameAs(lo[1], 50.0) && sameAs(hi[1], 150.0));
  CHECK(sameAs(lo[2], 0.0) && sameAs(hi[2], 20.0));
  // Every face out of the box and wound round its normal, the least sides
  // too.
  CHECK(woundRoundNormals(box.surface));
  bool outward = true;
  for (std::size_t t = 0; t + 2 < box.surface.triangles.size(); t += 3) {
    double centroid[3] = {0.0, 0.0, 0.0};
    const int first = box.surface.triangles[t];
    for (std::size_t c = 0; c < 3; ++c) {
      const std::size_t v = static_cast<std::size_t>(box.surface.triangles[t + c]);
      for (std::size_t axis = 0; axis < 3; ++axis) {
        centroid[axis] += box.surface.points[3 * v + axis] / 3.0;
      }
    }
    double along = 0.0;
    for (std::size_t axis = 0; axis < 3; ++axis) {
      along += (centroid[axis] - (lo[axis] + hi[axis]) / 2.0) *
               box.surface.normals[3 * static_cast<std::size_t>(first) + axis];
    }
    outward = outward && along > 0.0;
  }
  CHECK(outward);
  // The edges on the box's outline: each along one design axis.
  bool onOutline = true;
  for (std::size_t k = 0; k + 5 < box.edges.size(); k += 6) {
    int changed = 0;
    for (std::size_t axis = 0; axis < 3; ++axis) {
      changed += sameAs(box.edges[k + axis], box.edges[k + 3 + axis]) ? 0 : 1;
    }
    onOutline = onOutline && changed == 1;
  }
  CHECK(onOutline);

  // The lathe's bar: 50 across about the design's X, from the face at
  // X 11 back to X -289; its two end circles as edges.
  const UncutStock bar = uncutStock(latheWork(0.0), 12);
  CHECK(bar.kinds == QStringList{QStringLiteral("bar")});
  CHECK(bar.surface.vertices() == 3 * 2 * 13);
  CHECK(bar.surface.triangleCount() == 3 * 2 * 12);
  CHECK(bar.edgeCount() == 2 * 12);
  double xMin = 1e9;
  double xMax = -1e9;
  double widest = 0.0;
  for (std::size_t k = 0; k + 2 < bar.surface.points.size(); k += 3) {
    xMin = std::min(xMin, static_cast<double>(bar.surface.points[k]));
    xMax = std::max(xMax, static_cast<double>(bar.surface.points[k]));
    widest = std::max(widest, fromAxis(bar.surface.points, k));
  }
  CHECK(sameAs(xMin, -289.0) && sameAs(xMax, 11.0));
  CHECK(sameAs(widest, 25.0));
  bool circles = true;
  for (std::size_t k = 0; k + 2 < bar.edges.size(); k += 3) {
    circles = circles && sameAs(fromAxis(bar.edges, k), 25.0) &&
              (sameAs(bar.edges[k], -289.0) || sameAs(bar.edges[k], 11.0));
  }
  CHECK(circles);
  CHECK(woundRoundNormals(bar.surface));

  // A tube: the bore's band and its circles too, its normals towards the
  // axis, nothing inside the bore.
  const UncutStock tube = uncutStock(latheWork(5.0), 12);
  CHECK(tube.kinds == QStringList{QStringLiteral("tube")});
  CHECK(tube.surface.vertices() == 4 * 2 * 13);
  CHECK(tube.surface.triangleCount() == 4 * 2 * 12);
  CHECK(tube.edgeCount() == 4 * 12);
  double narrowest = 1e9;
  for (std::size_t k = 0; k + 2 < tube.surface.points.size(); k += 3) {
    narrowest = std::min(narrowest, fromAxis(tube.surface.points, k));
  }
  CHECK(sameAs(narrowest, 5.0));
  CHECK(woundRoundNormals(tube.surface));
  // Simulate's turned section with a bore wound the same way.
  const StockMesh turned = mitcad::cam::turnedStockMesh(
      {{QStringLiteral("z"), QJsonArray{-3.0, -2.0, -1.0, 0.0}},
       {QStringLiteral("inner"), QJsonArray{0.0, 2.0, 2.0, 3.0}},
       {QStringLiteral("outer"), QJsonArray{0.0, 5.0, 4.0, 4.0}}},
      mitcad::cam::WorkFrame::of(latheWork(0.0)), 12);
  CHECK(!turned.empty());
  CHECK(woundRoundNormals(turned));

  // Nothing for a setup in error, without stock or with an empty one.
  CHECK(uncutStock({{QStringLiteral("error"), QStringLiteral("The setup has no bodies.")}}).empty());
  CHECK(uncutStock(QJsonObject{}).empty());
  QJsonObject flat = millWork();
  flat.insert(QStringLiteral("stock_max"), QJsonArray{100.0, 50.0, -20.0});
  CHECK(uncutStock(flat).empty());

  // The setups shown together: both, in their order, the second's
  // triangles after the first's vertices.
  const UncutStock both = uncutStockOfSetups({setup(millWork(), true), setup(latheWork(0.0), true)}, 12);
  CHECK(both.kinds == (QStringList{QStringLiteral("box"), QStringLiteral("bar")}));
  CHECK(both.surface.triangleCount() == box.surface.triangleCount() + bar.surface.triangleCount());
  CHECK(both.edgeCount() == box.edgeCount() + bar.edgeCount());
  CHECK(*std::max_element(both.surface.triangles.begin(), both.surface.triangles.end()) == both.surface.vertices() - 1);
  CHECK(woundRoundNormals(both.surface));
  CHECK(both.log() == QStringLiteral("CAM uncut stock: 2 setup(s): box, bar; 102 vertices, 84 triangles, 36 edge(s)"));
  // Each eye hides its own setup's stock only.
  const UncutStock millOnly = uncutStockOfSetups({setup(millWork(), true), setup(latheWork(0.0), false)}, 12);
  CHECK(millOnly.kinds == QStringList{QStringLiteral("box")});
  CHECK(millOnly.surface.points == box.surface.points);
  const UncutStock latheOnly = uncutStockOfSetups({setup(millWork(), false), setup(latheWork(0.0), true)}, 12);
  CHECK(latheOnly.kinds == QStringList{QStringLiteral("bar")});
  CHECK(latheOnly.surface.points == bar.surface.points);
  // A setup without `visible` is shown; a failed one leaves nothing.
  CHECK(uncutStockOfSetups({QJsonObject{{QStringLiteral("work"), millWork()}}}).kinds.size() == 1);
  const UncutStock failed =
      uncutStockOfSetups({setup({{QStringLiteral("error"), QStringLiteral("No machine.")}}, true)});
  CHECK(failed.empty());
  CHECK(failed.log() == QStringLiteral("CAM uncut stock: none"));
  CHECK(uncutStockOfSetups(QJsonArray{}).empty());
  return failures;
}

MITCAD_UNIT_TEST(camUncutStockTests);
