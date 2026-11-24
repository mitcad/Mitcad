// SPDX-License-Identifier: MIT
#include "MoveFaceText.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QStringList>

namespace mitcad::cmd {

namespace {

constexpr double kPi = 3.14159265358979323846;

QString numberText(double value) {
  double rounded = std::round(value * 1e6) / 1e6;
  if (rounded == 0.0) {
    rounded = 0.0; // never "-0"
  }
  return QString::number(rounded, 'g', 12);
}

// A slot's text: its number, or as written.
QString slotText(const QJsonValue& slot, const SlotValue& value) {
  if (slot.isDouble()) {
    return numberText(slot.toDouble());
  }
  if (value) {
    if (const std::optional<double> resolved = value(slot)) {
      return numberText(*resolved);
    }
  }
  return slot.toString();
}

// An angle slot in degrees: its number, or as written.
QString angleText(const QJsonValue& slot, const SlotValue& value) {
  std::optional<double> radians;
  if (slot.isDouble()) {
    radians = slot.toDouble();
  } else if (value) {
    radians = value(slot);
  }
  return radians ? numberText(*radians * 180.0 / kPi) + QStringLiteral(" deg") : slot.toString();
}

QString vectorText(const QJsonValue& x, const QJsonValue& y, const QJsonValue& z, const SlotValue& value) {
  return QStringLiteral("(%1, %2, %3) mm").arg(slotText(x, value), slotText(y, value), slotText(z, value));
}

// What an axis reference is: "x", "F5" (a datum), "an edge", ...
QString referenceText(const QJsonValue& reference) {
  if (reference.isString()) {
    return reference.toString();
  }
  const QJsonObject o = reference.toObject();
  if (o.contains(QStringLiteral("edge"))) {
    return QStringLiteral("an edge");
  }
  if (o.contains(QStringLiteral("face"))) {
    return QStringLiteral("a face");
  }
  if (o.contains(QStringLiteral("curve")) || o.contains(QStringLiteral("curves"))) {
    return QStringLiteral("a sketch curve");
  }
  if (o.contains(QStringLiteral("direction"))) {
    return QStringLiteral("a fixed axis");
  }
  return QStringLiteral("an axis");
}

} // namespace

QString motionText(const QJsonObject& transform, const SlotValue& value) {
  const QString type = transform.value(QStringLiteral("type")).toString();
  if (type == QStringLiteral("translate_xyz")) {
    return QStringLiteral("translate %1").arg(vectorText(transform.value(QStringLiteral("x")),
                                                         transform.value(QStringLiteral("y")),
                                                         transform.value(QStringLiteral("z")), value));
  }
  if (type == QStringLiteral("translate_along")) {
    return QStringLiteral("translate %1 mm along %2")
        .arg(slotText(transform.value(QStringLiteral("distance")), value),
             referenceText(transform.value(QStringLiteral("axis"))));
  }
  if (type == QStringLiteral("rotate")) {
    return QStringLiteral("rotate %1 about %2")
        .arg(angleText(transform.value(QStringLiteral("angle")), value),
             referenceText(transform.value(QStringLiteral("axis"))));
  }
  if (type == QStringLiteral("point_to_point")) {
    // Along one axis only (mitcad#2339).
    const QString along = transform.value(QStringLiteral("along")).toString();
    if (!along.isEmpty() && along != QStringLiteral("all")) {
      return QStringLiteral("point to point along %1").arg(along);
    }
    return QStringLiteral("point to point");
  }
  if (type == QStringLiteral("point_to_position")) {
    return QStringLiteral("point to %1").arg(vectorText(transform.value(QStringLiteral("x")),
                                                        transform.value(QStringLiteral("y")),
                                                        transform.value(QStringLiteral("z")), value));
  }
  if (type == QStringLiteral("free")) {
    // Rows of a rotation and a translation: the turn's angle from the
    // trace, and the translation.
    const QJsonArray rows = transform.value(QStringLiteral("matrix")).toArray();
    const auto at = [&rows](int r, int c) { return rows.at(r).toArray().at(c).toDouble(); };
    const double trace = at(0, 0) + at(1, 1) + at(2, 2);
    const double angle = std::acos(std::clamp((trace - 1.0) / 2.0, -1.0, 1.0)) * 180.0 / kPi;
    const QString move = QStringLiteral("translate (%1, %2, %3) mm")
                             .arg(numberText(at(0, 3)), numberText(at(1, 3)), numberText(at(2, 3)));
    const QString turn = numberText(angle);
    return turn == QStringLiteral("0") ? move : QStringLiteral("turn %1 deg, %2").arg(turn, move);
  }
  return type;
}

QString moveFaceSummary(const QString& name, int faces, const QString& body, const QJsonObject& transform,
                        const SlotValue& value) {
  return QStringLiteral("%1: %2 %3 of %4, %5")
      .arg(name)
      .arg(faces)
      .arg(faces == 1 ? QStringLiteral("face") : QStringLiteral("faces"), body, motionText(transform, value));
}

} // namespace mitcad::cmd
