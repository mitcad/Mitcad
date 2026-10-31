// SPDX-License-Identifier: MIT
#include "PartStar.hpp"

#include <algorithm>
#include <cmath>

#include <QPainter>
#include <QPen>

namespace mitcad {
namespace {

constexpr double kPi = 3.14159265358979323846;

// How opaque the disc under an empty star is: the picture shows through a
// little, the outline stands out from it on any picture.
constexpr int kBackingAlpha = 220;
// How opaque the empty star's outline (the palette's text) is.
constexpr int kOutlineAlpha = 170;

} // namespace

QColor partStarColour() { return QColor(232, 170, 0); }

QPainterPath partStarPath(const QRectF& box) {
  QPainterPath path;
  const QPointF centre = box.center();
  const double outer = std::min(box.width(), box.height()) / 2.0;
  const double inner = outer * 0.45;
  for (int i = 0; i < 10; ++i) {
    const double radius = i % 2 == 0 ? outer : inner;
    const double angle = -kPi / 2.0 + i * kPi / 5.0;
    const QPointF point(centre.x() + radius * std::cos(angle), centre.y() + radius * std::sin(angle));
    if (i == 0) {
      path.moveTo(point);
    } else {
      path.lineTo(point);
    }
  }
  path.closeSubpath();
  return path;
}

void paintPartStar(QPainter* painter, const QRectF& box, bool on, const QColor& outline) {
  painter->save();
  painter->setRenderHint(QPainter::Antialiasing);
  const QPainterPath path = partStarPath(box.adjusted(1.0, 1.0, -1.0, -1.0));
  if (on) {
    painter->fillPath(path, partStarColour());
    painter->setPen(QPen(partStarColour().darker(130), 1.0));
  } else {
    painter->setPen(QPen(outline, 1.2));
  }
  painter->setBrush(Qt::NoBrush);
  painter->drawPath(path);
  painter->restore();
}

void paintTileStar(QPainter* painter, const QRectF& box, bool on, const QPalette& palette) {
  if (on) {
    paintPartStar(painter, box, true, QColor());
    return;
  }
  // A view whose base is see-through (a panel's) takes the window's colour.
  QColor backing = palette.color(QPalette::Base);
  if (backing.alpha() < 255) {
    backing = palette.color(QPalette::Window);
  }
  backing.setAlpha(kBackingAlpha);
  QColor outline = palette.color(QPalette::Text);
  outline.setAlpha(kOutlineAlpha);
  painter->save();
  painter->setRenderHint(QPainter::Antialiasing);
  painter->setPen(Qt::NoPen);
  painter->setBrush(backing);
  painter->drawEllipse(box);
  painter->restore();
  paintPartStar(painter, box, false, outline);
}

} // namespace mitcad
