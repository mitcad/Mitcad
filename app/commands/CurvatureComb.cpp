// SPDX-License-Identifier: MIT
#include "CurvatureComb.hpp"

#include <cmath>

#include <QJsonValue>
#include <QSettings>

namespace mitcad {

namespace {

const QString kTeethKey = QStringLiteral("curvatureComb/teeth");
const QString kSizeKey = QStringLiteral("curvatureComb/size");

CombPoint pointOf(const QJsonValue& value) {
  const QJsonArray xyz = value.toArray();
  return {xyz.at(0).toDouble(), xyz.at(1).toDouble(), xyz.at(2).toDouble()};
}

// A number to 0.1, without a trailing ".0" and never "-0".
QString number(double value) {
  const double rounded = std::round(value * 10.0) / 10.0 + 0.0;
  return QString::number(rounded, 'g', 12);
}

QString pointText(const CombPoint& p) {
  return QStringLiteral("(%1, %2, %3)").arg(number(p[0]), number(p[1]), number(p[2]));
}

} // namespace

CurvatureCombSettings CurvatureCombSettings::load() {
  QSettings settings;
  CurvatureCombSettings kept;
  kept.teeth = settings.value(kTeethKey, kDefaultTeeth).toInt();
  kept.size = settings.value(kSizeKey, 1.0).toDouble();
  if (!kept.valid()) {
    kept = CurvatureCombSettings();
  }
  return kept;
}

void CurvatureCombSettings::save() const {
  if (!valid()) {
    return;
  }
  QSettings settings;
  settings.setValue(kTeethKey, teeth);
  settings.setValue(kSizeKey, size);
}

bool CurvatureCombSettings::valid() const {
  return teeth >= kMinTeeth && teeth <= kMaxTeeth && std::isfinite(size) && size > 0.0;
}

QJsonObject curvatureCombQuery(const QJsonArray& items, const CurvatureCombSettings& settings,
                               std::optional<double> scale, bool inSketch) {
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("curvature_comb")},
                    {QStringLiteral("items"), items},
                    {QStringLiteral("teeth"), settings.teeth},
                    {QStringLiteral("size"), settings.size}};
  if (scale && *scale > 0.0) {
    query.insert(QStringLiteral("scale"), *scale);
  }
  if (inSketch) {
    query.insert(QStringLiteral("in_sketch"), true);
  }
  return query;
}

std::vector<std::pair<CombPoint, CombPoint>> CurvatureCombDrawing::teethLines() const {
  std::vector<std::pair<CombPoint, CombPoint>> lines;
  for (const Curve& curve : curves) {
    for (std::size_t i = 0; i < curve.points.size() && i < curve.tips.size(); ++i) {
      if (curve.points[i] != curve.tips[i]) {
        lines.emplace_back(curve.points[i], curve.tips[i]);
      }
    }
  }
  return lines;
}

std::vector<std::pair<CombPoint, CombPoint>> CurvatureCombDrawing::envelopeLines() const {
  std::vector<std::pair<CombPoint, CombPoint>> lines;
  for (const Curve& curve : curves) {
    const std::size_t count = curve.tips.size();
    for (std::size_t i = 0; i + 1 < count; ++i) {
      lines.emplace_back(curve.tips[i], curve.tips[i + 1]);
    }
    if (curve.closed && count > 2) {
      lines.emplace_back(curve.tips[count - 1], curve.tips[0]);
    }
  }
  return lines;
}

CurvatureCombDrawing curvatureCombDrawing(const QJsonObject& answer) {
  CurvatureCombDrawing drawing;
  drawing.scale = answer.value(QStringLiteral("scale")).toDouble();
  drawing.teeth = answer.value(QStringLiteral("teeth")).toInt();
  for (const QJsonValue& value : answer.value(QStringLiteral("combs")).toArray()) {
    const QJsonObject comb = value.toObject();
    const int item = comb.value(QStringLiteral("item")).toInt(-1);
    if (comb.contains(QStringLiteral("error"))) {
      drawing.errors << comb.value(QStringLiteral("error")).toString();
      continue;
    }
    ++drawing.answered;
    for (const QJsonValue& inflection : comb.value(QStringLiteral("inflections")).toArray()) {
      drawing.inflections.push_back(pointOf(inflection.toObject().value(QStringLiteral("point"))));
    }
    const double radius = comb.value(QStringLiteral("min_radius")).toDouble();
    if (radius > 0.0 && (drawing.minRadius <= 0.0 || radius < drawing.minRadius)) {
      drawing.minRadius = radius;
    }
    const QJsonArray teeth = comb.value(QStringLiteral("teeth")).toArray();
    if (comb.value(QStringLiteral("straight")).toBool() || teeth.isEmpty()) {
      ++drawing.straight;
      continue;
    }
    CurvatureCombDrawing::Curve curve;
    curve.item = item;
    curve.closed = comb.value(QStringLiteral("closed")).toBool();
    for (const QJsonValue& toothValue : teeth) {
      const QJsonObject tooth = toothValue.toObject();
      curve.points.push_back(pointOf(tooth.value(QStringLiteral("point"))));
      curve.tips.push_back(pointOf(tooth.value(QStringLiteral("tip"))));
    }
    drawing.curves.push_back(std::move(curve));
  }
  return drawing;
}

QString curvatureCombLog(const CurvatureCombDrawing& drawing) {
  QStringList parts;
  if (drawing.answered == 0) {
    parts << QStringLiteral("no curve");
  } else {
    QString curves = QStringLiteral("%1 curve(s)").arg(drawing.answered);
    if (drawing.straight > 0) {
      curves += QStringLiteral(" (%1 straight)").arg(drawing.straight);
    }
    parts << curves << QStringLiteral("%1 teeth each").arg(drawing.teeth);
    QStringList at;
    for (const CombPoint& p : drawing.inflections) {
      at << pointText(p);
    }
    parts << (at.isEmpty() ? QStringLiteral("no inflection")
                           : QStringLiteral("%1 inflection(s) at %2").arg(at.size()).arg(at.join(QStringLiteral(", "))));
    if (drawing.minRadius > 0.0) {
      parts << QStringLiteral("minimum radius %1 mm").arg(number(drawing.minRadius));
    }
    parts << QStringLiteral("scale %1").arg(number(drawing.scale));
  }
  QString log = parts.join(QStringLiteral(", "));
  for (const QString& error : drawing.errors) {
    log += QStringLiteral("; %1").arg(error);
  }
  return log;
}

} // namespace mitcad
