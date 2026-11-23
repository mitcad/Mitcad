// SPDX-License-Identifier: MIT
#include "PointClouds.hpp"

#include <algorithm>
#include <cstring>

#include <QByteArray>
#include <QJsonValue>
#include <QStringList>

namespace mitcad {

PointCloudDrawn pointCloudDrawnOf(const QJsonObject& answer) {
  PointCloudDrawn drawn;
  drawn.name = answer.value(QStringLiteral("name")).toString();
  const QByteArray points = QByteArray::fromBase64(answer.value(QStringLiteral("points")).toString().toLatin1());
  const std::size_t count = static_cast<std::size_t>(points.size()) / 12;
  drawn.points.resize(count * 3);
  if (count > 0) {
    // Little-endian floats, as the model writes them.
    std::memcpy(drawn.points.data(), points.constData(), count * 12);
  }
  const QJsonValue colors = answer.value(QStringLiteral("colors"));
  if (colors.isString()) {
    const QByteArray bytes = QByteArray::fromBase64(colors.toString().toLatin1());
    if (static_cast<std::size_t>(bytes.size()) == count * 3) {
      drawn.colors.assign(bytes.constBegin(), bytes.constEnd());
    }
  }
  const QJsonArray linear = answer.value(QStringLiteral("linear")).toArray();
  const QJsonArray translation = answer.value(QStringLiteral("translation")).toArray();
  if (linear.size() == 3 && translation.size() == 3) {
    for (int r = 0; r < 3; ++r) {
      const QJsonArray row = linear.at(r).toArray();
      for (int c = 0; c < 3; ++c) {
        drawn.linear[static_cast<std::size_t>(r)][static_cast<std::size_t>(c)] = row.at(c).toDouble(r == c ? 1.0 : 0.0);
      }
      drawn.translation[static_cast<std::size_t>(r)] = translation.at(r).toDouble();
    }
  }
  return drawn;
}

QStringList shownPointClouds(const QJsonArray& entries) {
  QStringList names;
  for (const QJsonValue& value : entries) {
    const QJsonObject entry = value.toObject();
    if (entry.value(QStringLiteral("visible")).toBool(true)) {
      names << entry.value(QStringLiteral("name")).toString();
    }
  }
  return names;
}

DeviationSample deviationSampleOf(const QJsonObject& overlay) {
  DeviationSample sample;
  const QJsonObject deviation = overlay.value(QStringLiteral("deviation")).toObject();
  sample.cloud = deviation.value(QStringLiteral("cloud")).toString();
  if (sample.cloud.isEmpty()) {
    return sample;
  }
  sample.step = static_cast<std::size_t>(std::max(1, deviation.value(QStringLiteral("step")).toInt(1)));
  const QJsonArray distances = deviation.value(QStringLiteral("distances")).toArray();
  sample.measured.reserve(static_cast<std::size_t>(distances.size()));
  for (const QJsonValue& distance : distances) {
    // A failed point's distance is null.
    sample.measured.push_back(distance.isDouble());
  }
  return sample;
}

PointCloudDrawn unmeasuredPoints(const PointCloudDrawn& cloud, const DeviationSample& sample) {
  PointCloudDrawn rest;
  rest.name = cloud.name;
  rest.linear = cloud.linear;
  rest.translation = cloud.translation;
  rest.unmeasured = true;
  const std::size_t step = std::max<std::size_t>(1, sample.step);
  const std::size_t count = cloud.count();
  rest.points.reserve((count - std::min(count, sample.measured.size())) * 3);
  for (std::size_t i = 0; i < count; ++i) {
    const std::size_t k = i / step;
    if (i % step == 0 && k < sample.measured.size() && sample.measured[k]) {
      continue;
    }
    rest.points.insert(rest.points.end(), cloud.points.begin() + static_cast<std::ptrdiff_t>(i * 3),
                       cloud.points.begin() + static_cast<std::ptrdiff_t>(i * 3 + 3));
  }
  return rest;
}

QString pointCloudsLog(const std::vector<PointCloudDrawn>& drawn) {
  if (drawn.empty()) {
    return QStringLiteral("Point clouds drawn: none");
  }
  QStringList parts;
  for (const PointCloudDrawn& cloud : drawn) {
    parts << QStringLiteral("%1 %2 points%3")
                 .arg(cloud.name)
                 .arg(static_cast<qulonglong>(cloud.count()))
                 .arg(cloud.unmeasured       ? QStringLiteral(" not measured")
                      : cloud.colors.empty() ? QString()
                                             : QStringLiteral(" coloured"));
  }
  return QStringLiteral("Point clouds drawn: ") + parts.join(QStringLiteral(", "));
}

} // namespace mitcad
