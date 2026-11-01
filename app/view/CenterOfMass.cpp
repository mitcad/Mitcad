// SPDX-License-Identifier: MIT
#include "CenterOfMass.hpp"

#include <cmath>

#include <QJsonArray>
#include <QJsonValue>
#include <QObject>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QPointF>
#include <QRectF>

#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// A number as the Physical Properties window writes it: `decimals` at
// most, trailing zeros dropped.
QString shortNumber(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

} // namespace

std::optional<CenterOfMassShown> centerOfMassOf(const QJsonObject& answer) {
  const QJsonValue marker = answer.value(QStringLiteral("marker"));
  if (!marker.isObject()) {
    return std::nullopt;
  }
  CenterOfMassShown shown;
  shown.visible = marker.toObject().value(QStringLiteral("visible")).toBool(true);
  shown.error = answer.value(QStringLiteral("error")).toString();
  shown.mass = answer.value(QStringLiteral("mass")).toDouble();
  shown.bodies = answer.value(QStringLiteral("bodies")).toInt();
  shown.defaultMaterial = answer.value(QStringLiteral("default_material")).toInt();
  const QJsonArray point = answer.value(QStringLiteral("center_of_mass")).toArray();
  if (shown.error.isEmpty() && point.size() == 3) {
    const std::array<double, 3> at{point.at(0).toDouble(), point.at(1).toDouble(), point.at(2).toDouble()};
    if (std::isfinite(at[0]) && std::isfinite(at[1]) && std::isfinite(at[2])) {
      shown.point = at;
    }
  }
  return shown;
}

QString centerOfMassTip(const CenterOfMassShown& shown) {
  if (!shown.error.isEmpty()) {
    return QObject::tr("Could not compute the center of mass: %1").arg(shown.error);
  }
  if (!shown.point) {
    return QObject::tr("No bodies to compute the center of mass of");
  }
  const std::array<double, 3>& at = *shown.point;
  QString tip = QObject::tr("(%1, %2, %3) mm, %4 kg")
                    .arg(shortNumber(at[0], 3), shortNumber(at[1], 3), shortNumber(at[2], 3),
                         shortNumber(shown.mass, 6));
  if (shown.defaultMaterial == 1) {
    tip += QObject::tr("; 1 body without a material counted as steel");
  } else if (shown.defaultMaterial > 1) {
    tip += QObject::tr("; %1 bodies without a material counted as steel").arg(shown.defaultMaterial);
  }
  return tip;
}

QString centerOfMassLog(const std::optional<CenterOfMassShown>& shown) {
  if (!shown) {
    return QStringLiteral("Center of Mass: none");
  }
  const QString state = shown->visible ? QStringLiteral("shown") : QStringLiteral("hidden");
  if (!shown->error.isEmpty()) {
    return QStringLiteral("Center of Mass: failed: %1 (%2)").arg(shown->error, state);
  }
  if (!shown->point) {
    return QStringLiteral("Center of Mass: no bodies (%1)").arg(state);
  }
  const std::array<double, 3>& at = *shown->point;
  return QStringLiteral("Center of Mass: at %1, %2, %3 mm, mass %4 kg, %5 bodies, %6 without a material (%7)")
      .arg(at[0], 0, 'f', 9)
      .arg(at[1], 0, 'f', 9)
      .arg(at[2], 0, 'f', 9)
      .arg(shown->mass, 0, 'f', 12)
      .arg(shown->bodies)
      .arg(shown->defaultMaterial)
      .arg(state);
}

QColor centerOfMassInk() { return viewColor(ColorRole::PreviewTarget, false); }

QColor centerOfMassDisc(bool darkBackground) { return viewColor(ColorRole::SketchHalo, darkBackground); }

QImage centerOfMassImage(bool darkBackground, double ratio) {
  const double scale = ratio > 0.0 ? ratio : 1.0;
  const double extent = kCenterOfMassRadius + kCenterOfMassRing / 2.0 + kCenterOfMassHalo;
  const int half = static_cast<int>(std::ceil((extent + 1.0) * scale));
  const int size = 2 * half + 1;
  QImage image(size, size, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.translate(half + 0.5, half + 0.5);
    painter.scale(scale, scale);
    const QColor ink = centerOfMassInk();
    // The disc in the background's tone, under the whole symbol.
    painter.setPen(Qt::NoPen);
    painter.setBrush(centerOfMassDisc(darkBackground));
    painter.drawEllipse(QPointF(0.0, 0.0), extent, extent);
    // The upper right and the lower left quadrants filled.
    const double r = kCenterOfMassRadius;
    const QRectF circle(-r, -r, 2.0 * r, 2.0 * r);
    painter.setBrush(ink);
    QPainterPath quadrants;
    quadrants.moveTo(0.0, 0.0);
    quadrants.arcTo(circle, 0.0, 90.0);
    quadrants.closeSubpath();
    quadrants.moveTo(0.0, 0.0);
    quadrants.arcTo(circle, 180.0, 90.0);
    quadrants.closeSubpath();
    painter.drawPath(quadrants);
    // The ring and the cross.
    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(ink, kCenterOfMassRing));
    painter.drawEllipse(QPointF(0.0, 0.0), r, r);
    painter.setPen(QPen(ink, kCenterOfMassRing / 2.0));
    painter.drawLine(QPointF(-r, 0.0), QPointF(r, 0.0));
    painter.drawLine(QPointF(0.0, -r), QPointF(0.0, r));
  }
  // Straight alpha, as OCCT's RGBA marker images are.
  return image.convertToFormat(QImage::Format_RGBA8888);
}

} // namespace mitcad
