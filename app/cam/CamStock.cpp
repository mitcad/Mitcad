// SPDX-License-Identifier: MIT
// The stock in Simulate: the height grid's runs, its surface and shades,
// the snapshots' places (mitcad#1013; CamStock.hpp).
#include "CamStock.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <utility>

#include <QJsonValue>

namespace mitcad::cam {
namespace {

// A node at the stock's bottom this close holds no material (mm).
constexpr double kEmpty = 1e-6;

void push3(std::vector<float>& out, const Point3& p) {
  out.push_back(static_cast<float>(p[0]));
  out.push_back(static_cast<float>(p[1]));
  out.push_back(static_cast<float>(p[2]));
}

Point3 unitOr(const Point3& v, const Point3& fallback) {
  const double l = std::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
  if (!(l > 1e-12)) {
    return fallback;
  }
  return {v[0] / l, v[1] / l, v[2] / l};
}

} // namespace

StockGrid StockGrid::of(const QJsonObject& grid) {
  StockGrid out;
  const QJsonArray origin = grid.value(QStringLiteral("origin")).toArray();
  const QJsonArray step = grid.value(QStringLiteral("step")).toArray();
  if (origin.size() != 2 || step.size() != 2) {
    return out;
  }
  out.originX = origin.at(0).toDouble();
  out.originY = origin.at(1).toDouble();
  out.stepX = step.at(0).toDouble(1.0);
  out.stepY = step.at(1).toDouble(1.0);
  out.nx = grid.value(QStringLiteral("nx")).toInt();
  out.ny = grid.value(QStringLiteral("ny")).toInt();
  out.bottom = grid.value(QStringLiteral("bottom")).toDouble();
  out.top = grid.value(QStringLiteral("top")).toDouble();
  if (out.nx < 0 || out.ny < 0) {
    out.nx = 0;
    out.ny = 0;
  }
  return out;
}

bool applyRuns(const QJsonArray& runs, std::vector<float>& heights) {
  if (runs.size() % 2 != 0) {
    return false;
  }
  std::size_t at = 0;
  for (qsizetype k = 0; k + 1 < runs.size(); k += 2) {
    const double microns = runs.at(k).toDouble();
    const double count = runs.at(k + 1).toDouble();
    const std::size_t n = static_cast<std::size_t>(std::llround(std::abs(count)));
    if (at + n > heights.size()) {
      return false;
    }
    if (count > 0.0) {
      const float z = static_cast<float>(microns / 1000.0);
      std::fill(heights.begin() + static_cast<std::ptrdiff_t>(at),
                heights.begin() + static_cast<std::ptrdiff_t>(at + n), z);
    }
    at += n;
  }
  return at == heights.size();
}

QString StockMesh::log() const {
  if (empty()) {
    return QStringLiteral("CAM stock overlay: none");
  }
  return QStringLiteral("CAM stock overlay: %1 vertices, %2 triangles, %3")
      .arg(vertices())
      .arg(triangleCount())
      .arg(remaining ? QStringLiteral("by the material left") : QStringLiteral("plain"));
}

StockMesh stockMesh(const StockGrid& grid, const std::vector<float>& heights, const WorkFrame& frame,
                    const std::vector<float>* part, double tolerance, double thickest) {
  StockMesh mesh;
  if (!grid.valid() || heights.size() != grid.nodes() || grid.nx < 2 || grid.ny < 2) {
    return mesh;
  }
  const bool shaded = part != nullptr && part->size() == heights.size();
  mesh.remaining = shaded;
  const int nx = grid.nx;
  const int ny = grid.ny;
  const auto index = [nx](int i, int j) { return static_cast<std::size_t>(j) * static_cast<std::size_t>(nx) + static_cast<std::size_t>(i); };
  const auto height = [&heights, &index](int i, int j) { return static_cast<double>(heights[index(i, j)]); };
  const auto work = [&grid](int i, int j, double z) -> Point3 {
    return {grid.originX + grid.stepX * i, grid.originY + grid.stepY * j, z};
  };
  const double span = std::max(thickest - tolerance, 1e-9);
  // The shade of a node (CamStock.hpp).
  const auto shadeOf = [&](int i, int j) -> float {
    if (!shaded) {
      return 0.0f;
    }
    const double z = height(i, j);
    const double over = static_cast<double>((*part)[index(i, j)]);
    const double left = z - std::max(over, grid.bottom);
    if (left > tolerance) {
      return static_cast<float>(std::clamp((left - tolerance) / span, 1e-3, 1.0));
    }
    if (over > grid.bottom + kEmpty) {
      // Against the lowest part round it: a node on a wall the tool runs
      // along is cut to the wall's foot, which is no gouge.
      double low = over;
      for (int jj = std::max(0, j - 1); jj <= std::min(ny - 1, j + 1); ++jj) {
        for (int ii = std::max(0, i - 1); ii <= std::min(nx - 1, i + 1); ++ii) {
          low = std::min(low, static_cast<double>((*part)[index(ii, jj)]));
        }
      }
      if (low - z > tolerance) {
        return -1.0f;
      }
    }
    return 0.0f;
  };

  // The top: a vertex per node, its normal from its neighbours' heights.
  const Point3 up = unitOr(frame.directionToModel({0.0, 0.0, 1.0}), {0.0, 0.0, 1.0});
  for (int j = 0; j < ny; ++j) {
    for (int i = 0; i < nx; ++i) {
      push3(mesh.points, frame.toModel(work(i, j, height(i, j))));
      const int i0 = std::max(0, i - 1);
      const int i1 = std::min(nx - 1, i + 1);
      const int j0 = std::max(0, j - 1);
      const int j1 = std::min(ny - 1, j + 1);
      const double dx = (height(i1, j) - height(i0, j)) / (grid.stepX * (i1 - i0));
      const double dy = (height(i, j1) - height(i, j0)) / (grid.stepY * (j1 - j0));
      push3(mesh.normals, unitOr(frame.directionToModel({-dx, -dy, 1.0}), up));
      mesh.shades.push_back(shadeOf(i, j));
    }
  }
  const auto empty = [&](int i, int j) { return height(i, j) <= grid.bottom + kEmpty; };
  for (int j = 0; j + 1 < ny; ++j) {
    for (int i = 0; i + 1 < nx; ++i) {
      if (empty(i, j) && empty(i + 1, j) && empty(i, j + 1) && empty(i + 1, j + 1)) {
        continue;
      }
      const int a = static_cast<int>(index(i, j));
      const int b = static_cast<int>(index(i + 1, j));
      const int c = static_cast<int>(index(i + 1, j + 1));
      const int d = static_cast<int>(index(i, j + 1));
      // Counter-clockwise seen from above.
      mesh.triangles.insert(mesh.triangles.end(), {a, b, c, a, c, d});
    }
  }

  // The sides: along each edge of the grid, from the top down to the
  // bottom, facing out.
  const auto side = [&](int ia, int ja, int ib, int jb, const Point3& outward) {
    if (empty(ia, ja) && empty(ib, jb)) {
      return;
    }
    const Point3 normal = unitOr(frame.directionToModel(outward), up);
    const int first = mesh.vertices();
    const std::array<std::array<int, 2>, 2> nodes{{{ia, ja}, {ib, jb}}};
    for (const auto& node : nodes) {
      push3(mesh.points, frame.toModel(work(node[0], node[1], height(node[0], node[1]))));
      push3(mesh.points, frame.toModel(work(node[0], node[1], grid.bottom)));
      for (int k = 0; k < 2; ++k) {
        push3(mesh.normals, normal);
        mesh.shades.push_back(shadeOf(node[0], node[1]));
      }
    }
    // first: a's top, +1 a's foot, +2 b's top, +3 b's foot.
    mesh.triangles.insert(mesh.triangles.end(), {first, first + 1, first + 3, first, first + 3, first + 2});
  };
  for (int i = 0; i + 1 < nx; ++i) {
    side(i, 0, i + 1, 0, {0.0, -1.0, 0.0});
    side(i + 1, ny - 1, i, ny - 1, {0.0, 1.0, 0.0});
  }
  for (int j = 0; j + 1 < ny; ++j) {
    side(0, j + 1, 0, j, {-1.0, 0.0, 0.0});
    side(nx - 1, j, nx - 1, j + 1, {1.0, 0.0, 0.0});
  }
  return mesh;
}

StockMesh turnedStockMesh(const QJsonObject& stock, const WorkFrame& frame, int segments) {
  StockMesh mesh;
  const QJsonArray zs = stock.value(QStringLiteral("z")).toArray();
  const QJsonArray inners = stock.value(QStringLiteral("inner")).toArray();
  const QJsonArray outers = stock.value(QStringLiteral("outer")).toArray();
  const qsizetype n = zs.size();
  if (n < 2 || inners.size() != n || outers.size() != n || segments < 3) {
    return mesh;
  }
  const auto z = [&zs](qsizetype i) { return zs.at(i).toDouble(); };
  const auto in = [&inners](qsizetype i) { return std::max(0.0, inners.at(i).toDouble()); };
  const auto out = [&outers](qsizetype i) { return outers.at(i).toDouble(); };
  const auto solid = [&](qsizetype i) { return out(i) > in(i) + kEmpty; };
  // The nodes where a radius changes (and the ends): the bands run
  // between them.
  std::vector<qsizetype> kept;
  for (qsizetype i = 0; i < n; ++i) {
    const bool same = i > 0 && i + 1 < n && solid(i - 1) == solid(i) && solid(i + 1) == solid(i) &&
                      std::abs(out(i - 1) - out(i)) < 1e-6 && std::abs(out(i + 1) - out(i)) < 1e-6 &&
                      std::abs(in(i - 1) - in(i)) < 1e-6 && std::abs(in(i + 1) - in(i)) < 1e-6;
    if (!same) {
      kept.push_back(i);
    }
  }
  const Point3 axis = unitOr(frame.directionToModel({0.0, 0.0, 1.0}), {0.0, 0.0, 1.0});
  const auto at = [&frame](double r, double angle, double h) {
    return frame.toModel({r * std::cos(angle), r * std::sin(angle), h});
  };
  const double step = 2.0 * 3.14159265358979323846 / segments;
  // A band of the surface of revolution from (ra, za) to (rb, zb), its
  // normal out from the axis (`sign` +1) or towards it (-1).
  const auto band = [&](double ra, double za, double rb, double zb, double sign) {
    if (ra < kEmpty && rb < kEmpty) {
      return;
    }
    const int first = mesh.vertices();
    const double dz = zb - za;
    const double dr = rb - ra;
    for (int k = 0; k <= segments; ++k) {
      const double angle = step * k;
      const Point3 radial{std::cos(angle), std::sin(angle), 0.0};
      // Perpendicular to the band in its plane through the axis.
      const Point3 normal{sign * radial[0] * dz, sign * radial[1] * dz, -sign * dr};
      for (const auto& [r, h] : {std::pair<double, double>{ra, za}, std::pair<double, double>{rb, zb}}) {
        push3(mesh.points, at(r, angle, h));
        push3(mesh.normals, unitOr(frame.directionToModel(normal), axis));
        mesh.shades.push_back(0.0f);
      }
    }
    for (int k = 0; k < segments; ++k) {
      const int a = first + 2 * k;
      mesh.triangles.insert(mesh.triangles.end(), {a, a + 2, a + 3, a, a + 3, a + 1});
    }
  };
  // A flat ring at `h` from `r0` to `r1`, facing along the axis (`sign`).
  const auto ring = [&](double h, double r0, double r1, double sign) {
    if (r1 - r0 < kEmpty) {
      return;
    }
    const int first = mesh.vertices();
    const Point3 normal = unitOr(frame.directionToModel({0.0, 0.0, sign}), axis);
    for (int k = 0; k <= segments; ++k) {
      for (const double r : {r0, r1}) {
        push3(mesh.points, at(r, step * k, h));
        push3(mesh.normals, normal);
        mesh.shades.push_back(0.0f);
      }
    }
    for (int k = 0; k < segments; ++k) {
      const int a = first + 2 * k;
      mesh.triangles.insert(mesh.triangles.end(), {a, a + 1, a + 3, a, a + 3, a + 2});
    }
  };
  for (std::size_t k = 0; k < kept.size(); ++k) {
    const qsizetype i = kept[k];
    if (!solid(i)) {
      continue;
    }
    // Where material starts or ends along Z: a ring.
    const bool startsHere = k == 0 || !solid(kept[k - 1]);
    const bool endsHere = k + 1 == kept.size() || !solid(kept[k + 1]);
    if (startsHere) {
      ring(z(i), in(i), out(i), -1.0);
    }
    if (endsHere) {
      ring(z(i), in(i), out(i), 1.0);
      continue;
    }
    const qsizetype j = kept[k + 1];
    band(out(i), z(i), out(j), z(j), 1.0);
    band(in(i), z(i), in(j), z(j), -1.0);
  }
  return mesh;
}

std::vector<int> framePlaces(int points, int count) {
  std::vector<int> out;
  if (points <= 0 || count <= 0) {
    return out;
  }
  const int n = std::min(points, count);
  for (int k = 1; k <= n; ++k) {
    // The k-th of n evenly to the last place.
    const long long place = (static_cast<long long>(k) * (points - 1) + n / 2) / n;
    const int at = static_cast<int>(place);
    if (out.empty() || out.back() < at) {
      out.push_back(at);
    }
  }
  if (out.empty() || out.back() != points - 1) {
    out.push_back(points - 1);
  }
  return out;
}

int frameAt(const std::vector<int>& places, int index) {
  const auto after = std::upper_bound(places.begin(), places.end(), index);
  return static_cast<int>(after - places.begin()) - 1;
}

StockMesh solidStockMesh(const QJsonObject& solid) {
  StockMesh mesh;
  const QJsonObject surface = solid.value(QStringLiteral("mesh")).toObject();
  const QJsonArray coords = surface.value(QStringLiteral("vertices")).toArray();
  const QJsonArray corners = surface.value(QStringLiteral("triangles")).toArray();
  const qsizetype count = coords.size() / 3;
  if (coords.size() % 3 != 0 || corners.size() % 3 != 0) {
    return mesh;
  }
  const auto point = [&coords](qsizetype v) {
    return Point3{coords.at(3 * v).toDouble(), coords.at(3 * v + 1).toDouble(), coords.at(3 * v + 2).toDouble()};
  };
  // Each triangle with its own corners, its face's normal on them: the
  // cubes' faces are flat.
  for (qsizetype t = 0; t + 2 < corners.size(); t += 3) {
    std::array<qsizetype, 3> ids{};
    bool inside = true;
    for (qsizetype k = 0; k < 3; ++k) {
      ids[static_cast<std::size_t>(k)] = corners.at(t + k).toInteger(-1);
      inside = inside && ids[static_cast<std::size_t>(k)] >= 0 && ids[static_cast<std::size_t>(k)] < count;
    }
    if (!inside) {
      return StockMesh{};
    }
    const Point3 a = point(ids[0]);
    const Point3 b = point(ids[1]);
    const Point3 c = point(ids[2]);
    const Point3 u{b[0] - a[0], b[1] - a[1], b[2] - a[2]};
    const Point3 w{c[0] - a[0], c[1] - a[1], c[2] - a[2]};
    const Point3 normal = unitOr({u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]},
                                 {0.0, 0.0, 1.0});
    const int first = mesh.vertices();
    for (const Point3& p : {a, b, c}) {
      push3(mesh.points, p);
      push3(mesh.normals, normal);
      mesh.shades.push_back(0.0f);
    }
    mesh.triangles.insert(mesh.triangles.end(), {first, first + 1, first + 2});
  }
  return mesh;
}

QString stockLog(const QString& setup, const StockGrid& grid, int snapshots) {
  return QStringLiteral("CAM stock: %1, %2 x %3 nodes, %4 snapshots").arg(setup).arg(grid.nx).arg(grid.ny).arg(snapshots);
}

QString remainingLog(const QString& setup, const QJsonObject& remaining) {
  const auto value = [&remaining](const char* key) {
    return shortNumber(remaining.value(QLatin1String(key)).toDouble());
  };
  return QStringLiteral("CAM remaining material: %1: max %2, mean %3, gouge %4, beside %5, %6 place(s)")
      .arg(setup, value("max"), value("mean"), value("gouge"), value("beside"))
      .arg(remaining.value(QStringLiteral("thickest")).toArray().size());
}

} // namespace mitcad::cam
