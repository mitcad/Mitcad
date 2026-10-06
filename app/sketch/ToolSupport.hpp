// SPDX-License-Identifier: MIT
#pragma once

// Helpers shared by the sketch tools.

#include <cmath>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

#include "SketchController.hpp"
#include "SketchModel.hpp"

namespace mitcad::sketch {

inline QJsonArray xy(const V2& p) { return QJsonArray{p.x, p.y}; }

// "(12.5, -3)" for logs.
inline QString text(const V2& p) {
  const auto n = [](double v) { return QString::number(std::abs(v) < 5e-10 ? 0.0 : v, 'g', 8); };
  return QStringLiteral("(%1, %2)").arg(n(p.x), n(p.y));
}

// The ids a command result lists under "made" (the main curves).
inline QStringList made(const QJsonObject& result) {
  QStringList ids;
  for (const QJsonValue& id : result.value(QStringLiteral("made")).toArray()) {
    ids << id.toString();
  }
  return ids;
}

// The new entities of a command result.
inline QStringList entities(const QJsonObject& result) {
  QStringList ids;
  for (const QJsonValue& id : result.value(QStringLiteral("entities")).toArray()) {
    ids << id.toString();
  }
  return ids;
}

// The new point of a result nearest to `at`.
inline QString pointNear(const SketchModel& model, const QJsonObject& result, const V2& at) {
  QString best;
  double bestDistance = 1e300;
  for (const QString& id : entities(result)) {
    if (const auto p = model.pointAt(id)) {
      const double d = dist(*p, at);
      if (d < bestDistance) {
        best = id;
        bestDistance = d;
      }
    }
  }
  return best;
}

// The arc from `start` leaving along `direction` to `end`; none when it
// would be straight.
std::optional<Curve2> tangentArc(const V2& start, const V2& direction, const V2& end);
// The arc from `a` through `m` to `b`.
std::optional<Curve2> arcThrough(const V2& a, const V2& m, const V2& b);
// The direction a curve leaves its end point `point` with (outwards).
std::optional<V2> leavingDirection(const SketchModel& model, const QString& point,
                                   QString* curve = nullptr);

} // namespace mitcad::sketch
