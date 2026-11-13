// SPDX-License-Identifier: MIT
#include "SpatialPaths.hpp"

#include <QCoreApplication>
#include <QJsonArray>

namespace mitcad::cmd {

QJsonObject spatialPath(const QString& sketch, const QStringList& curves, bool chain, bool reversed) {
  QJsonObject path{{QStringLiteral("spatial_sketch"), sketch},
                   {QStringLiteral("curves"), QJsonArray::fromStringList(curves)}};
  if (chain) {
    path.insert(QStringLiteral("chain"), true);
  }
  if (reversed) {
    path.insert(QStringLiteral("reversed"), true);
  }
  return path;
}

std::optional<SpatialPathParts> spatialPathParts(const QJsonValue& path) {
  const QJsonObject o = path.toObject();
  if (!o.value(QStringLiteral("spatial_sketch")).isString()) {
    return std::nullopt;
  }
  SpatialPathParts parts;
  parts.sketch = o.value(QStringLiteral("spatial_sketch")).toString();
  for (const QJsonValue& curve : o.value(QStringLiteral("curves")).toArray()) {
    parts.curves << curve.toString();
  }
  parts.chain = o.value(QStringLiteral("chain")).toBool();
  parts.reversed = o.value(QStringLiteral("reversed")).toBool();
  return parts;
}

QJsonObject keptDirection(const SpatialPathParts& parts) {
  if (parts.curves.isEmpty()) {
    return QJsonObject();
  }
  return {{QStringLiteral("first"), parts.curves.first()}, {QStringLiteral("reversed"), parts.reversed}};
}

bool reversedBy(const QJsonObject& kept, const QStringList& curves) {
  return !curves.isEmpty() && kept.value(QStringLiteral("reversed")).toBool() &&
         kept.value(QStringLiteral("first")).toString() == curves.first();
}

QString notAPiece(const QJsonObject& curve) {
  const QString type = curve.value(QStringLiteral("type")).toString();
  if (type == QStringLiteral("reference")) {
    return QCoreApplication::translate(
        "SpatialPaths",
        "is a reference curve, geometry from outside the sketch: it is no path piece (draw a curve on it)");
  }
  return QString();
}

QJsonObject spatialCurve(const QJsonObject& sketch3d, const QString& id) {
  for (const QJsonValue& value : sketch3d.value(QStringLiteral("curves")).toArray()) {
    const QJsonObject curve = value.toObject();
    if (curve.value(QStringLiteral("id")).toString() == id) {
      return curve;
    }
  }
  return QJsonObject();
}

QStringList wholeRoute(const QJsonObject& sketch3d, QString* why) {
  const QString name = sketch3d.value(QStringLiteral("name")).toString();
  const QJsonArray chains = sketch3d.value(QStringLiteral("chains")).toArray();
  QString reason;
  QStringList curves;
  if (chains.isEmpty()) {
    reason = QStringLiteral("%1 has no curves that make a path").arg(name);
  } else if (chains.size() > 1) {
    reason = QStringLiteral("%1 has %2 separate chains: pick a curve of the one to follow").arg(name).arg(chains.size());
  } else {
    // An open or a closed chain (mitcad#1550), whole.
    const QJsonObject chain = chains.first().toObject();
    if (!chain.value(QStringLiteral("branches")).toArray().isEmpty()) {
      // In words: the branch points' ids are not for the person
      // (mitcad#1612).
      reason = QStringLiteral("%1's curves branch: pick the curves of one route").arg(name);
    } else {
      for (const QJsonValue& curve :
           chain.value(QStringLiteral("path")).toObject().value(QStringLiteral("curves")).toArray()) {
        curves << curve.toString();
      }
    }
  }
  if (why != nullptr) {
    *why = reason;
  }
  return curves;
}

QString routeText(const QJsonObject& route, QString* why) {
  const QString error = route.value(QStringLiteral("error")).toString();
  if (why != nullptr) {
    *why = error;
  }
  QStringList curves;
  for (const QJsonValue& curve : route.value(QStringLiteral("curves")).toArray()) {
    curves << curve.toString();
  }
  if (!error.isEmpty() || curves.isEmpty()) {
    return QStringLiteral("Not one path");
  }
  // At most four by name; a longer route by its ends and its count.
  const QString listed = curves.size() <= 4
                             ? curves.join(QStringLiteral(", "))
                             : QStringLiteral("%1 ... %2, %3 curves").arg(curves.first(), curves.last()).arg(curves.size());
  // A closed route (mitcad#1550): from where it starts round to it again;
  // a closed curve has no such point.
  if (route.value(QStringLiteral("closed")).toBool()) {
    const QString start = route.value(QStringLiteral("start")).toString();
    return start.isEmpty() ? QStringLiteral("%1 (closed)").arg(listed)
                           : QStringLiteral("%1 (closed, from %2)").arg(listed, start);
  }
  return QStringLiteral("%1 (%2 to %3)")
      .arg(listed, route.value(QStringLiteral("start")).toString(), route.value(QStringLiteral("end")).toString());
}

QString routeWords(const QJsonObject& route, const QString& sketchName) {
  const int count = static_cast<int>(route.value(QStringLiteral("curves")).toArray().size());
  if (!route.value(QStringLiteral("error")).toString().isEmpty() || count == 0) {
    return QObject::tr("Not one path");
  }
  QString words = sketchName.isEmpty() ? QObject::tr("%n curve(s)", nullptr, count)
                                       : QObject::tr("%n curve(s) of %1", nullptr, count).arg(sketchName);
  if (route.value(QStringLiteral("closed")).toBool()) {
    words = QObject::tr("%1, closed").arg(words);
  }
  return words;
}

} // namespace mitcad::cmd
