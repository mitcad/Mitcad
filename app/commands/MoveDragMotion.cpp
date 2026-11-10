// SPDX-License-Identifier: MIT
#include "MoveDragMotion.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonValue>
#include <QString>

#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

namespace mitcad::cmd {

gp_Trsf freeRotation(double rx, double ry, double rz) {
  gp_Trsf x;
  x.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), rx);
  gp_Trsf y;
  y.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)), ry);
  gp_Trsf z;
  z.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), rz);
  return z * y * x;
}

std::optional<gp_Trsf> moveMotion(const CommandState& state, const gp_Pnt& pivot) {
  const QString type = state.choice(QStringLiteral("type"));
  const auto value = [&state](const char* id) { return state.value(QString::fromLatin1(id)); };
  if (type == QStringLiteral("free")) {
    const double rx = value("rx");
    const double ry = value("ry");
    const double rz = value("rz");
    const gp_XYZ t(value("tx"), value("ty"), value("tz"));
    if (!std::isfinite(rx) || !std::isfinite(ry) || !std::isfinite(rz) || !std::isfinite(t.X()) ||
        !std::isfinite(t.Y()) || !std::isfinite(t.Z())) {
      return std::nullopt;
    }
    // p -> R (p - c) + c + t, as the definition's matrix (transformOf).
    const gp_Trsf rotation = freeRotation(rx, ry, rz);
    gp_XYZ turned = pivot.XYZ();
    rotation.Transforms(turned);
    const gp_XYZ moved = pivot.XYZ() - turned + t;
    gp_Trsf motion;
    motion.SetValues(rotation.Value(1, 1), rotation.Value(1, 2), rotation.Value(1, 3), moved.X(),
                     rotation.Value(2, 1), rotation.Value(2, 2), rotation.Value(2, 3), moved.Y(),
                     rotation.Value(3, 1), rotation.Value(3, 2), rotation.Value(3, 3), moved.Z());
    return motion;
  }
  if (type == QStringLiteral("translate")) {
    const gp_XYZ t(value("x"), value("y"), value("z"));
    if (!std::isfinite(t.X()) || !std::isfinite(t.Y()) || !std::isfinite(t.Z())) {
      return std::nullopt;
    }
    gp_Trsf motion;
    motion.SetTranslation(gp_Vec(t));
    return motion;
  }
  return std::nullopt;
}

std::optional<gp_Pnt> freeMovedMiddle(const CommandState& state, const gp_Pnt& pivot) {
  const gp_XYZ moved(state.value(QStringLiteral("tx")), state.value(QStringLiteral("ty")),
                     state.value(QStringLiteral("tz")));
  if (!std::isfinite(moved.X()) || !std::isfinite(moved.Y()) || !std::isfinite(moved.Z())) {
    return std::nullopt;
  }
  return gp_Pnt(pivot.XYZ() + moved);
}

std::optional<gp_Pnt> freeArrowOrigin(const CommandState& state, const gp_Pnt& pivot, int axis) {
  if (axis < 0 || axis > 2) {
    return std::nullopt;
  }
  gp_XYZ moved(state.value(QStringLiteral("tx")), state.value(QStringLiteral("ty")), state.value(QStringLiteral("tz")));
  if (!std::isfinite(moved.X()) || !std::isfinite(moved.Y()) || !std::isfinite(moved.Z())) {
    return std::nullopt;
  }
  // Its own distance left out exactly (not added and taken off again):
  // the start stays the same while its own value is dragged, and the drag
  // keeps its grab (ManipulatorOverlay::setHandles).
  moved.SetCoord(axis + 1, 0.0);
  return gp_Pnt(pivot.XYZ() + moved);
}

FreeMove freeMoveOf(const QJsonArray& rows, const gp_Pnt& pivot) {
  const auto at = [&rows](int r, int c) { return rows.at(r).toArray().at(c).toDouble(); };
  // R = Rz Ry Rx.
  FreeMove move;
  move.ry = std::asin(std::clamp(-at(2, 0), -1.0, 1.0));
  move.rx = std::atan2(at(2, 1), at(2, 2));
  move.rz = std::atan2(at(1, 0), at(0, 0));
  // t' = c - R c + t, so t = t' - c + R c.
  gp_XYZ turned = pivot.XYZ();
  freeRotation(move.rx, move.ry, move.rz).Transforms(turned);
  move.moved = gp_XYZ(at(0, 3), at(1, 3), at(2, 3)) - pivot.XYZ() + turned;
  return move;
}

QJsonArray freeMatrixOf(const FreeMove& move, const gp_Pnt& pivot) {
  // Turn about the pivot, then move: p -> R (p - c) + c + t.
  const gp_Trsf rotation = freeRotation(move.rx, move.ry, move.rz);
  gp_XYZ turned = pivot.XYZ();
  rotation.Transforms(turned);
  const gp_XYZ t = pivot.XYZ() - turned + move.moved;
  QJsonArray rows;
  for (int r = 1; r <= 3; ++r) {
    rows.append(QJsonArray{rotation.Value(r, 1), rotation.Value(r, 2), rotation.Value(r, 3), t.Coord(r)});
  }
  return rows;
}

namespace {

// A point or a direction of a basis: [x, y, z]; none unless three finite
// numbers.
std::optional<gp_XYZ> xyzOf(const QJsonValue& value) {
  const QJsonArray array = value.toArray();
  if (array.size() != 3) {
    return std::nullopt;
  }
  const gp_XYZ xyz(array.at(0).toDouble(std::nan("")), array.at(1).toDouble(std::nan("")),
                   array.at(2).toDouble(std::nan("")));
  if (!std::isfinite(xyz.X()) || !std::isfinite(xyz.Y()) || !std::isfinite(xyz.Z())) {
    return std::nullopt;
  }
  return xyz;
}

} // namespace

std::optional<gp_Trsf> basisMotion(const CommandState& state, const QJsonObject& basis) {
  const QString type = state.choice(QStringLiteral("type"));
  if (basis.value(QStringLiteral("type")).toString() != type) {
    return std::nullopt;
  }
  gp_Trsf motion;
  if (type == QStringLiteral("rotate")) {
    const auto origin = xyzOf(basis.value(QStringLiteral("origin")));
    const auto direction = xyzOf(basis.value(QStringLiteral("direction")));
    const double angle = state.value(QStringLiteral("angle"));
    if (!origin || !direction || direction->Modulus() < 1e-12 || !std::isfinite(angle)) {
      return std::nullopt;
    }
    // Right-handed about the axis, as the model's rotation.
    motion.SetRotation(gp_Ax1(gp_Pnt(*origin), gp_Dir(*direction)), angle);
    return motion;
  }
  if (type == QStringLiteral("point_to_point")) {
    const auto from = xyzOf(basis.value(QStringLiteral("from")));
    const auto to = xyzOf(basis.value(QStringLiteral("to")));
    if (!from || !to) {
      return std::nullopt;
    }
    motion.SetTranslation(gp_Vec(*to - *from));
    return motion;
  }
  if (type == QStringLiteral("point_to_position")) {
    const auto point = xyzOf(basis.value(QStringLiteral("point")));
    const gp_XYZ position(state.value(QStringLiteral("px")), state.value(QStringLiteral("py")),
                          state.value(QStringLiteral("pz")));
    if (!point || !std::isfinite(position.X()) || !std::isfinite(position.Y()) || !std::isfinite(position.Z())) {
      return std::nullopt;
    }
    motion.SetTranslation(gp_Vec(position - *point));
    return motion;
  }
  return std::nullopt;
}

std::optional<gp_Ax1> keptRotateAxis(const QJsonObject& kept, const QJsonValue& axis, const QString& occurrence,
                                     const gp_Trsf& frame) {
  const QJsonObject basis = kept.value(QStringLiteral("basis")).toObject();
  if (basis.value(QStringLiteral("type")).toString() != QStringLiteral("rotate") ||
      kept.value(QStringLiteral("axis")) != axis || kept.value(QStringLiteral("occurrence")).toString() != occurrence) {
    return std::nullopt;
  }
  const auto origin = xyzOf(basis.value(QStringLiteral("origin")));
  const auto direction = xyzOf(basis.value(QStringLiteral("direction")));
  if (!origin || !direction || direction->Modulus() < 1e-12) {
    return std::nullopt;
  }
  return gp_Ax1(gp_Pnt(*origin), gp_Dir(*direction)).Transformed(frame);
}

QJsonObject rotateBasisToKeep(const QJsonObject& report, const QString& type, const QJsonValue& axis,
                              const QString& occurrence) {
  const QJsonObject basis = report.value(QStringLiteral("motion")).toObject();
  if (type != QStringLiteral("rotate") || basis.value(QStringLiteral("type")).toString() != QStringLiteral("rotate") ||
      axis.isNull() || axis.isUndefined()) {
    return QJsonObject();
  }
  return QJsonObject{
      {QStringLiteral("basis"), basis}, {QStringLiteral("axis"), axis}, {QStringLiteral("occurrence"), occurrence}};
}

std::optional<gp_Ax1> rotateRingAxis(const QJsonObject& kept, const QJsonValue& axis, const QString& occurrence,
                                     const gp_Trsf& frame, const std::function<std::optional<gp_Ax1>()>& picked,
                                     const gp_Pnt& center) {
  // The axis the model turns about once a preview reported it, the picked
  // geometry's own until then.
  std::optional<gp_Ax1> line;
  if (!kept.isEmpty()) {
    line = keptRotateAxis(kept, axis, occurrence, frame);
  }
  if (!line && picked) {
    line = picked();
  }
  if (!line) {
    return std::nullopt;
  }
  const gp_Vec along(line->Direction());
  const gp_Pnt foot = line->Location().Translated(along * gp_Vec(line->Location(), center).Dot(along));
  return gp_Ax1(foot, line->Direction());
}

} // namespace mitcad::cmd
