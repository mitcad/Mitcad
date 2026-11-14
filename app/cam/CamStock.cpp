// SPDX-License-Identifier: MIT
// The stock in Simulate: the height grid's runs, its surface and shades,
// the snapshots' places (mitcad#1013; CamStock.hpp).
#include "CamStock.hpp"

#include <algorithm>
#include <array>
#include <cmath>

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
