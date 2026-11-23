// SPDX-License-Identifier: MIT
#include "PointCloudDeviation.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstring>

#include <QJsonArray>
#include <QJsonValue>
#include <QObject>
#include <QStringList>

#include "../framework/UserText.hpp"

namespace mitcad {
namespace {

// A number with `decimals` decimals, never "-0.000".
QString fixed(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.startsWith(QLatin1Char('-')) && text.toDouble() == 0.0) {
    text.remove(0, 1);
  }
  return text;
}

// A number without trailing zeros ("0.1", "2").
QString plain(double value) {
  QString text = QString::number(value, 'f', 6);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

int count(const QJsonObject& answer, const char* key) { return answer.value(QLatin1String(key)).toInt(); }

double number(const QJsonObject& answer, const char* key) { return answer.value(QLatin1String(key)).toDouble(); }

} // namespace

QJsonObject pointCloudDeviationDef(const QString& cloud, const QString& body, double tolerance, int maxSamples,
                                   bool isSigned, double limit, const QJsonArray& faces) {
  QJsonObject def{{QStringLiteral("type"), QStringLiteral("point_cloud_deviation")},
                  {QStringLiteral("cloud"), cloud},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("tolerance"), tolerance},
                  {QStringLiteral("max_samples"), maxSamples},
                  {QStringLiteral("signed"), isSigned}};
  if (limit > 0.0 && std::isfinite(limit)) {
    def.insert(QStringLiteral("limit"), limit);
  }
  // Some of the body's faces only (mitcad#2265).
  if (!faces.isEmpty()) {
    def.insert(QStringLiteral("faces"), faces);
  }
  return def;
}

QJsonArray deviationFacesOf(const QJsonObject& def) {
  QJsonArray faces;
  for (const QJsonValue& face : def.value(QStringLiteral("faces")).toArray()) {
    if (face.isString() && !face.toString().isEmpty()) {
      faces.append(face);
    }
  }
  return faces;
}

double deviationToleranceOf(const QJsonObject& def) {
  const double tolerance = def.value(QStringLiteral("tolerance")).toDouble(-1.0);
  return tolerance > 0.0 && std::isfinite(tolerance) ? tolerance : kDefaultDeviationTolerance;
}

int deviationSamplesOf(const QJsonObject& def) {
  const int samples = def.value(QStringLiteral("max_samples")).toInt(-1);
  return samples > 0 ? samples : kDefaultDeviationSamples;
}

double deviationLimitOf(const QJsonObject& def) {
  const double limit = def.value(QStringLiteral("limit")).toDouble(0.0);
  return limit > 0.0 && std::isfinite(limit) ? limit : 0.0;
}

QJsonObject pointCloudDeviationQuery(const QJsonObject& def) {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("point_cloud_deviation")},
                      {QStringLiteral("cloud"), def.value(QStringLiteral("cloud")).toString()},
                      {QStringLiteral("body"), def.value(QStringLiteral("body")).toString()},
                      {QStringLiteral("tolerance"), deviationToleranceOf(def)},
                      {QStringLiteral("max_samples"), deviationSamplesOf(def)},
                      {QStringLiteral("worst"), kDeviationWorstListed},
                      {QStringLiteral("distances"), true}};
  const QJsonValue isSigned = def.value(QStringLiteral("signed"));
  if (isSigned.isBool()) {
    request.insert(QStringLiteral("signed"), isSigned);
  }
  const QJsonArray faces = deviationFacesOf(def);
  if (!faces.isEmpty()) {
    request.insert(QStringLiteral("faces"), faces);
  }
  return request;
}

std::vector<float> cloudPlaces(const QJsonObject& points) {
  const QByteArray bytes = QByteArray::fromBase64(points.value(QStringLiteral("points")).toString().toLatin1());
  const QJsonArray linear = points.value(QStringLiteral("linear")).toArray();
  const QJsonArray translation = points.value(QStringLiteral("translation")).toArray();
  double m[3][3] = {{1, 0, 0}, {0, 1, 0}, {0, 0, 1}};
  double t[3] = {0, 0, 0};
  if (linear.size() == 3 && translation.size() == 3) {
    for (int r = 0; r < 3; ++r) {
      const QJsonArray row = linear.at(r).toArray();
      for (int c = 0; c < 3; ++c) {
        m[r][c] = row.at(c).toDouble(r == c ? 1.0 : 0.0);
      }
      t[r] = translation.at(r).toDouble();
    }
  }
  const std::size_t n = static_cast<std::size_t>(bytes.size()) / 12;
  std::vector<float> places;
  places.reserve(n * 3);
  for (std::size_t i = 0; i < n; ++i) {
    float p[3];
    std::memcpy(p, bytes.constData() + i * 12, sizeof p); // little-endian, as the model writes them
    for (int r = 0; r < 3; ++r) {
      places.push_back(static_cast<float>(m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + t[r]));
    }
  }
  return places;
}

QString runPointCloudDeviation(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer,
                               std::vector<float>& places) {
  places.clear();
  answer = query(pointCloudDeviationQuery(def), true);
  // In plain words (mitcad#1612).
  const QJsonValue error = answer.value(QStringLiteral("error"));
  if (error.isString()) {
    return userText(error.toString());
  }
  // The measured points: every step-th of the cloud's, as the model took
  // them.
  const QJsonObject points =
      query({{QStringLiteral("query"), QStringLiteral("point_cloud_points")},
             {QStringLiteral("name"), def.value(QStringLiteral("cloud")).toString()}},
            false);
  if (points.value(QStringLiteral("error")).isString()) {
    return userText(points.value(QStringLiteral("error")).toString());
  }
  const std::vector<float> all = cloudPlaces(points);
  const std::size_t step = static_cast<std::size_t>(std::max(1, count(answer, "step")));
  const std::size_t sampled = static_cast<std::size_t>(std::max(0, count(answer, "sampled")));
  places.reserve(sampled * 3);
  for (std::size_t i = 0; i < sampled && (i * step + 1) * 3 <= all.size(); ++i) {
    places.insert(places.end(), all.begin() + static_cast<std::ptrdiff_t>(i * step * 3),
                  all.begin() + static_cast<std::ptrdiff_t>(i * step * 3 + 3));
  }
  return QString();
}

double deviationLimit(const QJsonObject& def, const QJsonObject& answer) {
  const double given = deviationLimitOf(def);
  if (given > 0.0) {
    return given;
  }
  const double tolerance = deviationToleranceOf(def);
  return std::max(number(answer, "max_abs_mm"), 2.0 * tolerance);
}

QString pointCloudDeviationText(const QJsonObject& def, const QJsonObject& answer, const QString& bodyName,
                                const std::array<QColor, 3>& colors) {
  const QString cloud = answer.value(QStringLiteral("cloud")).toString();
  const double tolerance = number(answer, "tolerance");
  const double limit = deviationLimit(def, answer);
  const bool isSigned = answer.value(QStringLiteral("signed")).toBool();
  const int measured = count(answer, "measured");
  QStringList parts;
  // From the chosen faces only, when some are (mitcad#2265).
  const int faces = static_cast<int>(deviationFacesOf(def).size());
  QString measuredText;
  if (faces > 1) {
    // The count before the names, which may hold a "%".
    measuredText = QObject::tr("%1 of %2 points of %4 measured from %3 faces of %5")
                       .arg(measured)
                       .arg(count(answer, "points"))
                       .arg(faces)
                       .arg(cloud.toHtmlEscaped(), bodyName.toHtmlEscaped());
  } else {
    measuredText = (faces == 1 ? QObject::tr("%1 of %2 points of %3 measured from one face of %4")
                               : QObject::tr("%1 of %2 points of %3 measured from the faces of %4"))
                       .arg(measured)
                       .arg(count(answer, "points"))
                       .arg(cloud.toHtmlEscaped(), bodyName.toHtmlEscaped());
  }
  if (count(answer, "step") > 1) {
    measuredText += QObject::tr(" (one point in %1)").arg(count(answer, "step"));
  }
  parts << measuredText + QLatin1Char('.');
  if (count(answer, "failures") > 0) {
    parts << QObject::tr("%1 points could not be measured.").arg(count(answer, "failures"));
  }
  QStringList stats;
  if (isSigned) {
    stats << QObject::tr("Mean: %1 mm").arg(fixed(number(answer, "mean_signed_mm"), 3));
  }
  stats << QObject::tr("Mean distance: %1 mm").arg(fixed(number(answer, "mean_abs_mm"), 3))
        << QObject::tr("RMS: %1 mm").arg(fixed(number(answer, "rms_mm"), 3))
        << QObject::tr("Largest distance: %1 mm").arg(fixed(number(answer, "max_abs_mm"), 3));
  const double share = 100.0 * number(answer, "within_tolerance_fraction");
  stats << QObject::tr("Within ±%1 mm: %2 points (%3 %)")
               .arg(plain(tolerance))
               .arg(count(answer, "within_tolerance"))
               .arg(fixed(share, 1));
  if (isSigned) {
    stats << QObject::tr("Outside the material: %1 points").arg(count(answer, "outside"))
          << QObject::tr("Inside the material: %1 points").arg(count(answer, "inside"))
          << QObject::tr("On the faces: %1 points").arg(count(answer, "on_boundary"));
  } else {
    stats << QObject::tr("<i>Measured without a sign: the distances do not tell inside from outside.</i>");
  }
  parts << stats.join(QStringLiteral("<br>"));
  // The legend: the scale's colours in words.
  const auto swatch = [](const QColor& color) {
    return QStringLiteral("<span style='color:%1'>&#9632;</span>").arg(color.name());
  };
  QStringList legend;
  if (isSigned) {
    legend << QStringLiteral("%1 %2").arg(swatch(colors[0]),
                                          QObject::tr("Inside the material, to −%1 mm").arg(fixed(limit, 3)));
  }
  legend << QStringLiteral("%1 %2").arg(swatch(colors[1]), QObject::tr("Within ±%1 mm").arg(plain(tolerance)));
  legend << QStringLiteral("%1 %2").arg(swatch(colors[2]),
                                        isSigned ? QObject::tr("Outside the material, to +%1 mm").arg(fixed(limit, 3))
                                                 : QObject::tr("Away from the faces, to %1 mm").arg(fixed(limit, 3)));
  parts << legend.join(QStringLiteral("<br>"));
  // The farthest points, with where the faces are nearest.
  const QJsonArray worst = answer.value(QStringLiteral("worst")).toArray();
  if (!worst.isEmpty()) {
    QStringList lines{QStringLiteral("<b>%1</b>").arg(QObject::tr("Farthest points"))};
    for (const QJsonValue& value : worst) {
      const QJsonObject point = value.toObject();
      const QJsonArray at = point.value(QStringLiteral("point")).toArray();
      lines << QObject::tr("%1 mm at (%2, %3, %4)")
                   .arg(fixed(point.value(QStringLiteral("distance")).toDouble(), 3),
                        fixed(at.at(0).toDouble(), 2), fixed(at.at(1).toDouble(), 2), fixed(at.at(2).toDouble(), 2));
    }
    parts << lines.join(QStringLiteral("<br>"));
  }
  return parts.join(QStringLiteral("<br><br>"));
}

QString pointCloudDeviationLog(const QJsonObject& answer, const QString& bodyName) {
  const double share = 100.0 * number(answer, "within_tolerance_fraction");
  return QStringLiteral("%1 to %2: %3 of %4 points, %5, mean %6 mm, mean absolute %7 mm, RMS %8 mm, max %9 mm, "
                        "within %10 mm %11 (%12 %), outside %13, inside %14, on the faces %15")
      .arg(answer.value(QStringLiteral("cloud")).toString(), bodyName)
      .arg(count(answer, "measured"))
      .arg(count(answer, "points"))
      .arg(answer.value(QStringLiteral("signed")).toBool() ? QStringLiteral("signed") : QStringLiteral("unsigned"))
      .arg(fixed(number(answer, "mean_signed_mm"), 3), fixed(number(answer, "mean_abs_mm"), 3),
           fixed(number(answer, "rms_mm"), 3), fixed(number(answer, "max_abs_mm"), 3),
           plain(number(answer, "tolerance")))
      .arg(count(answer, "within_tolerance"))
      .arg(fixed(share, 1))
      .arg(count(answer, "outside"))
      .arg(count(answer, "inside"))
      .arg(count(answer, "on_boundary"));
}

QJsonObject pointCloudDeviationOverlayJson(const QJsonObject& def, const QJsonObject& answer,
                                           const std::vector<float>& places, const QString& title) {
  QJsonArray points;
  for (const float value : places) {
    points.append(static_cast<double>(value));
  }
  QJsonObject deviation{{QStringLiteral("cloud"), def.value(QStringLiteral("cloud")).toString()},
                        {QStringLiteral("points"), points},
                        {QStringLiteral("distances"), answer.value(QStringLiteral("distances")).toArray()},
                        {QStringLiteral("tolerance"), number(answer, "tolerance")},
                        {QStringLiteral("limit"), deviationLimit(def, answer)},
                        {QStringLiteral("signed"), answer.value(QStringLiteral("signed")).toBool()},
                        // What was measured of the cloud (mitcad#2265): the
                        // rest is drawn dimmed, the legend says how many.
                        {QStringLiteral("count"), count(answer, "points")},
                        {QStringLiteral("step"), std::max(1, count(answer, "step"))},
                        {QStringLiteral("measured"), count(answer, "measured")}};
  const QJsonArray worst = answer.value(QStringLiteral("worst")).toArray();
  if (!worst.isEmpty()) {
    const QJsonObject first = worst.first().toObject();
    deviation.insert(QStringLiteral("farthest"), first.value(QStringLiteral("point")).toArray());
    deviation.insert(QStringLiteral("farthest_distance"), first.value(QStringLiteral("distance")).toDouble());
  }
  return {{QStringLiteral("deviation"), deviation}, {QStringLiteral("title"), title}};
}

} // namespace mitcad
