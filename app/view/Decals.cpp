// SPDX-License-Identifier: MIT
// Decals (mitcad#2244): the model's entries as the view and the renderer
// draw them, which decal a point of a face lies on, and the log line UI
// tests read. Plain Qt Core.
#include "Decals.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>

#include <QJsonValue>
#include <QStringList>

namespace mitcad {
namespace {

DecalPoint minus(const DecalPoint& a, const DecalPoint& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }

double dot(const DecalPoint& a, const DecalPoint& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }

DecalPoint cross(const DecalPoint& a, const DecalPoint& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}

std::optional<DecalPoint> pointOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() != 3) {
    return std::nullopt;
  }
  return DecalPoint{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()};
}

// Whether `p` lies on the triangle a, b, c: within `tolerance` of its
// plane and inside it (by a little more than the tolerance at its sides).
bool onTriangle(const DecalPoint& p, const DecalPoint& a, const DecalPoint& b, const DecalPoint& c,
                double tolerance) {
  const DecalPoint ab = minus(b, a);
  const DecalPoint ac = minus(c, a);
  const DecalPoint n = cross(ab, ac);
  const double area2 = dot(n, n);
  if (area2 < 1e-24) {
    return false;
  }
  const DecalPoint ap = minus(p, a);
  const double off = dot(ap, n) / std::sqrt(area2);
  if (std::abs(off) > tolerance) {
    return false;
  }
  // Barycentric coordinates of p's projection.
  const double d00 = dot(ab, ab);
  const double d01 = dot(ab, ac);
  const double d11 = dot(ac, ac);
  const double d20 = dot(ap, ab);
  const double d21 = dot(ap, ac);
  const double denominator = d00 * d11 - d01 * d01;
  if (std::abs(denominator) < 1e-24) {
    return false;
  }
  const double v = (d11 * d20 - d01 * d21) / denominator;
  const double w = (d00 * d21 - d01 * d20) / denominator;
  const double u = 1.0 - v - w;
  const double side = std::max({std::sqrt(d00), std::sqrt(d11), 1e-9});
  const double slack = tolerance / side;
  return u >= -slack && v >= -slack && w >= -slack;
}

} // namespace

std::optional<DecalShown> decalOf(const QJsonObject& entry) {
  const QJsonObject mesh = entry.value(QStringLiteral("mesh")).toObject();
  if (entry.contains(QStringLiteral("error")) || mesh.isEmpty()) {
    return std::nullopt;
  }
  DecalShown decal;
  decal.name = entry.value(QStringLiteral("name")).toString();
  decal.sha256 = entry.value(QStringLiteral("sha256")).toString();
  decal.surface = entry.value(QStringLiteral("surface")).toString();
  const QJsonObject face = entry.value(QStringLiteral("face")).toObject();
  decal.body = face.value(QStringLiteral("body")).toString();
  decal.face = face.value(QStringLiteral("face")).toString();
  decal.occurrence = entry.value(QStringLiteral("occurrence")).toString();
  decal.columns = mesh.value(QStringLiteral("columns")).toInt();
  decal.rows = mesh.value(QStringLiteral("rows")).toInt();
  if (decal.columns < 1 || decal.rows < 1) {
    return std::nullopt;
  }
  const QJsonArray points = mesh.value(QStringLiteral("points")).toArray();
  const QJsonArray normals = mesh.value(QStringLiteral("normals")).toArray();
  const QJsonArray uvs = mesh.value(QStringLiteral("uvs")).toArray();
  const qsizetype grid = static_cast<qsizetype>(decal.columns + 1) * (decal.rows + 1);
  const qsizetype count = points.size();
  if (count < grid || normals.size() != count || uvs.size() != count) {
    return std::nullopt;
  }
  for (qsizetype i = 0; i < count; ++i) {
    const std::optional<DecalPoint> point = pointOf(points.at(i));
    const std::optional<DecalPoint> normal = pointOf(normals.at(i));
    const QJsonArray uv = uvs.at(i).toArray();
    if (!point || !normal || uv.size() != 2) {
      return std::nullopt;
    }
    decal.points.push_back(*point);
    decal.normals.push_back(*normal);
    decal.uvs.push_back({uv.at(0).toDouble(), uv.at(1).toDouble()});
  }
  decal.flip = entry.value(QStringLiteral("flip")).toBool();
  if (const QJsonValue triangles = mesh.value(QStringLiteral("triangles")); triangles.isArray()) {
    // The model's: the part of the grid on the face (mitcad#2266).
    for (const QJsonValue& value : triangles.toArray()) {
      const QJsonArray corners = value.toArray();
      if (corners.size() != 3) {
        return std::nullopt;
      }
      std::array<std::size_t, 3> triangle{};
      for (qsizetype k = 0; k < 3; ++k) {
        const int index = corners.at(k).toInt(-1);
        if (index < 0 || index >= count) {
          return std::nullopt;
        }
        triangle[static_cast<std::size_t>(k)] = static_cast<std::size_t>(index);
      }
      decal.triangles.push_back(triangle);
    }
  } else {
    // All of the grid: two triangles per piece, counter-clockwise seen from
    // outside the material (mirrored when the decal is flipped, as its
    // grid then runs right to left).
    for (int row = 0; row < decal.rows; ++row) {
      for (int column = 0; column < decal.columns; ++column) {
        const std::size_t topLeft = decal.at(row, column);
        const std::size_t topRight = decal.at(row, column + 1);
        const std::size_t bottomRight = decal.at(row + 1, column + 1);
        const std::size_t bottomLeft = decal.at(row + 1, column);
        if (decal.flip) {
          decal.triangles.push_back({topLeft, topRight, bottomRight});
          decal.triangles.push_back({topLeft, bottomRight, bottomLeft});
        } else {
          decal.triangles.push_back({topLeft, bottomLeft, bottomRight});
          decal.triangles.push_back({topLeft, bottomRight, topRight});
        }
      }
    }
  }
  decal.cut = mesh.value(QStringLiteral("cut")).toBool();
  decal.width = entry.value(QStringLiteral("width")).toDouble();
  decal.height = entry.value(QStringLiteral("height")).toDouble();
  decal.angle = entry.value(QStringLiteral("angle")).toDouble();
  decal.opacity = entry.value(QStringLiteral("opacity")).toDouble(1.0);
  decal.visible = entry.value(QStringLiteral("visible")).toBool(true);
  const QJsonArray pixels = entry.value(QStringLiteral("pixels")).toArray();
  decal.pixelsWide = pixels.at(0).toInt();
  decal.pixelsHigh = pixels.at(1).toInt();
  return decal;
}

std::vector<DecalShown> shownDecals(const QJsonArray& entries) {
  std::vector<DecalShown> shown;
  for (const QJsonValue& value : entries) {
    if (const std::optional<DecalShown> decal = decalOf(value.toObject()); decal && decal->visible) {
      shown.push_back(*decal);
    }
  }
  return shown;
}

std::optional<QString> decalAt(const std::vector<DecalShown>& decals, const DecalPoint& point, double tolerance) {
  for (auto it = decals.rbegin(); it != decals.rend(); ++it) {
    const DecalShown& decal = *it;
    for (const std::array<std::size_t, 3>& t : decal.triangles) {
      if (onTriangle(point, decal.points[t[0]], decal.points[t[1]], decal.points[t[2]], tolerance)) {
        return decal.name;
      }
    }
  }
  return std::nullopt;
}

QString decalsLog(const std::vector<DecalShown>& shown) {
  if (shown.empty()) {
    return QStringLiteral("Decals drawn: none");
  }
  QStringList parts;
  for (const DecalShown& decal : shown) {
    QString part = QStringLiteral("%1 %2 x %3 mm on a %4")
                       .arg(decal.name, QString::number(decal.width, 'f', 1), QString::number(decal.height, 'f', 1),
                            decal.surface);
    if (std::abs(decal.angle) > 1e-9) {
      part += QStringLiteral(" turned %1 deg").arg(QString::number(decal.angle, 'g', 6));
    }
    if (decal.flip) {
      part += QStringLiteral(" flipped");
    }
    part += QStringLiteral(" %1%").arg(std::lround(decal.opacity * 100.0));
    if (decal.cut) {
      part += QStringLiteral(" cut");
    }
    if (decal.columns > 1 || decal.rows > 1) {
      part += QStringLiteral(" (%1 x %2)").arg(decal.columns).arg(decal.rows);
    }
    if (decal.panel) {
      part += QStringLiteral(" in the panel");
    }
    parts << part;
  }
  return QStringLiteral("Decals drawn: ") + parts.join(QStringLiteral(", "));
}

} // namespace mitcad
