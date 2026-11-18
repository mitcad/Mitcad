// SPDX-License-Identifier: MIT
#include "browser/SketchCurveRows.hpp"

#include <QCoreApplication>
#include <QHash>
#include <QJsonArray>
#include <QJsonValue>

namespace mitcad {

namespace {

// The drawing tool's icon of a curve's type.
QString curveIcon(const QString& type, bool centerline) {
  if (type == QStringLiteral("line")) {
    return centerline ? QStringLiteral("centerline") : QStringLiteral("line");
  }
  if (type == QStringLiteral("circle")) {
    return QStringLiteral("circle");
  }
  if (type == QStringLiteral("arc")) {
    return QStringLiteral("arc-3pt");
  }
  if (type == QStringLiteral("ellipse") || type == QStringLiteral("elliptical_arc")) {
    return QStringLiteral("ellipse");
  }
  if (type == QStringLiteral("spline")) {
    return QStringLiteral("spline-control");
  }
  return QStringLiteral("spline-fit");
}

// Numbers each kind in the order given: "Line 1", "Line 2", "Arc 1".
class Counter {
public:
  BrowserSketchCurve row(const QString& id, const QString& type, bool centerline, bool construction) {
    BrowserSketchCurve made;
    made.id = id;
    made.geometry = type;
    made.icon = curveIcon(type, centerline);
    const QString kind = sketchCurveKind(type, centerline);
    const int number = ++m_counts[kind];
    made.name = QCoreApplication::translate("SketchCurveRows", "%1 %2").arg(kind).arg(number);
    made.construction = construction;
    return made;
  }

private:
  QHash<QString, int> m_counts;
};

} // namespace

QString sketchCurveKind(const QString& type, bool centerline) {
  if (type == QStringLiteral("line")) {
    return centerline ? QCoreApplication::translate("SketchCurveRows", "Centerline")
                      : QCoreApplication::translate("SketchCurveRows", "Line");
  }
  if (type == QStringLiteral("circle")) {
    return QCoreApplication::translate("SketchCurveRows", "Circle");
  }
  if (type == QStringLiteral("arc")) {
    return QCoreApplication::translate("SketchCurveRows", "Arc");
  }
  if (type == QStringLiteral("ellipse")) {
    return QCoreApplication::translate("SketchCurveRows", "Ellipse");
  }
  if (type == QStringLiteral("elliptical_arc")) {
    return QCoreApplication::translate("SketchCurveRows", "Elliptical Arc");
  }
  if (type == QStringLiteral("fitted_spline")) {
    return QCoreApplication::translate("SketchCurveRows", "Fit Point Spline");
  }
  if (type == QStringLiteral("spline")) {
    return QCoreApplication::translate("SketchCurveRows", "Control Point Spline");
  }
  if (type == QStringLiteral("conic")) {
    return QCoreApplication::translate("SketchCurveRows", "Conic");
  }
  if (type == QStringLiteral("reference")) {
    return QCoreApplication::translate("SketchCurveRows", "Reference Curve");
  }
  return QCoreApplication::translate("SketchCurveRows", "Curve");
}

QVector<BrowserSketchCurve> sketchCurveRows(const QJsonObject& sketch) {
  QVector<BrowserSketchCurve> rows;
  Counter counter;
  for (const QJsonValue& value : sketch.value(QStringLiteral("entities")).toArray()) {
    const QJsonObject entity = value.toObject();
    const QString type = entity.value(QStringLiteral("type")).toString();
    const QString id = entity.value(QStringLiteral("id")).toString();
    // As the view draws them (ModelShapes' sketchEntities): no points, no
    // line standing for a sketch axis (mitcad#500), none without geometry.
    if (type.isEmpty() || type == QStringLiteral("point") || id.isEmpty() ||
        entity.contains(QStringLiteral("axis")) || entity.value(QStringLiteral("geometry")).toObject().isEmpty()) {
      continue;
    }
    rows.append(counter.row(id, type, entity.value(QStringLiteral("centerline")).toBool(),
                            entity.value(QStringLiteral("construction")).toBool()));
  }
  return rows;
}

QVector<BrowserSketchCurve> spatialSketchCurveRows(const QJsonObject& sketch) {
  QVector<BrowserSketchCurve> rows;
  Counter counter;
  for (const QJsonValue& value : sketch.value(QStringLiteral("curves")).toArray()) {
    const QJsonObject curve = value.toObject();
    const QString type = curve.value(QStringLiteral("type")).toString();
    const QString id = curve.value(QStringLiteral("id")).toString();
    if (type.isEmpty() || id.isEmpty()) {
      continue;
    }
    rows.append(counter.row(id, type, false, curve.value(QStringLiteral("construction")).toBool()));
  }
  return rows;
}

} // namespace mitcad
