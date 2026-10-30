// SPDX-License-Identifier: MIT
#include "SketchNormal.hpp"

#include <Standard_Failure.hxx>
#include <gp_Vec.hxx>

namespace mitcad::cmd {
namespace {

// The placement of 3 or 4 rows of 4 numbers; none when it is not one.
std::optional<gp_Trsf> placementOf(const QJsonArray& rows) {
  if (rows.size() < 3) {
    return std::nullopt;
  }
  const auto at = [&rows](int r, int c) { return rows[r].toArray()[c].toDouble(); };
  try {
    gp_Trsf placement;
    placement.SetValues(at(0, 0), at(0, 1), at(0, 2), at(0, 3), at(1, 0), at(1, 1), at(1, 2), at(1, 3),
                        at(2, 0), at(2, 1), at(2, 2), at(2, 3));
    return placement;
  } catch (const Standard_Failure&) {
    return std::nullopt; // a singular matrix
  }
}

// The occurrence at `path` under `level`, whose occurrences' paths start
// with `parent`.
std::optional<QJsonObject> occurrenceAt(const QJsonArray& level, const QString& parent, const QString& path) {
  for (const QJsonValue& value : level) {
    const QJsonObject node = value.toObject();
    const QString uid = node.value(QStringLiteral("uid")).toString();
    const QString here = parent.isEmpty() ? uid : parent + QLatin1Char('/') + uid;
    if (here == path) {
      return node;
    }
    if (path.startsWith(here + QLatin1Char('/'))) {
      return occurrenceAt(node.value(QStringLiteral("children")).toArray(), here, path);
    }
  }
  return std::nullopt;
}

} // namespace

std::optional<gp_Trsf> occurrenceWorldIn(const QJsonArray& occurrences, const QString& path) {
  if (path.isEmpty()) {
    return gp_Trsf();
  }
  const std::optional<QJsonObject> node = occurrenceAt(occurrences, QString(), path);
  if (!node) {
    return std::nullopt;
  }
  return placementOf(node->value(QStringLiteral("world")).toArray());
}

std::optional<gp_Dir> sketchNormalInDesign(const QJsonObject& frame, const QJsonArray& occurrences,
                                           const QString& path) {
  const QJsonArray xyz = frame.value(QStringLiteral("normal")).toArray();
  if (xyz.size() < 3) {
    return std::nullopt;
  }
  const gp_Vec vector(xyz[0].toDouble(), xyz[1].toDouble(), xyz[2].toDouble());
  if (vector.Magnitude() < 1e-12) {
    return std::nullopt;
  }
  gp_Dir normal(vector);
  if (const std::optional<gp_Trsf> world = occurrenceWorldIn(occurrences, path)) {
    normal.Transform(*world);
  }
  return normal;
}

} // namespace mitcad::cmd
