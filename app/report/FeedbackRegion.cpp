// SPDX-License-Identifier: MIT
#include "FeedbackRegion.hpp"

#include <algorithm>
#include <cmath>
#include <cstdlib>

#include <QGuiApplication>
#include <QPainter>
#include <QPixmap>
#include <QScreen>

namespace mitcad::report {

QRect regionFromDrag(const QPoint& from, const QPoint& to, const QList<QRect>& screens) {
  const int width = std::abs(to.x() - from.x());
  const int height = std::abs(to.y() - from.y());
  if (width < kRegionMinSide || height < kRegionMinSide) {
    return {};
  }
  const QRect dragged(QPoint(std::min(from.x(), to.x()), std::min(from.y(), to.y())), QSize(width, height));
  QRect clipped;
  for (const QRect& part : regionParts(dragged, screens)) {
    if (!part.isEmpty()) {
      clipped = clipped.isNull() ? part : clipped.united(part);
    }
  }
  if (clipped.width() < kRegionMinSide || clipped.height() < kRegionMinSide) {
    return {};
  }
  return clipped;
}

QList<QRect> regionParts(const QRect& region, const QList<QRect>& screens) {
  QList<QRect> parts;
  parts.reserve(screens.size());
  for (const QRect& screen : screens) {
    const QRect part = region.intersected(screen);
    parts.append(part.isEmpty() ? QRect() : part);
  }
  return parts;
}

QImage grabScreenRegion(const QRect& region) {
  if (region.isEmpty()) {
    return {};
  }
  const QList<QScreen*> screens = QGuiApplication::screens();
  QList<QRect> geometries;
  geometries.reserve(screens.size());
  for (const QScreen* screen : screens) {
    geometries.append(screen->geometry());
  }
  const QList<QRect> parts = regionParts(region, geometries);
  // The picture in the device pixels of the sharpest screen it touches.
  qreal ratio = 1.0;
  int touched = 0;
  for (qsizetype i = 0; i < screens.size(); ++i) {
    if (!parts.at(i).isEmpty()) {
      ratio = std::max(ratio, screens.at(i)->devicePixelRatio());
      ++touched;
    }
  }
  if (touched == 0) {
    return {};
  }
  QImage picture;
  QPainter painter;
  bool grabbed = false;
  for (qsizetype i = 0; i < screens.size(); ++i) {
    const QRect& part = parts.at(i);
    if (part.isEmpty()) {
      continue;
    }
    QScreen* screen = screens.at(i);
    // Window 0: the screen as it shows, offsets from the screen's corner.
    const QRect geometry = screen->geometry();
    const QPixmap pixmap =
        screen->grabWindow(0, part.x() - geometry.x(), part.y() - geometry.y(), part.width(), part.height());
    if (pixmap.isNull()) {
      continue;
    }
    QImage image = pixmap.toImage();
    image.setDevicePixelRatio(1.0);
    if (touched == 1) {
      // One screen: its pixels as they are.
      return image;
    }
    if (picture.isNull()) {
      picture = QImage(static_cast<int>(std::ceil(region.width() * ratio)),
                       static_cast<int>(std::ceil(region.height() * ratio)), QImage::Format_RGB32);
      picture.fill(Qt::black);
      painter.begin(&picture);
      painter.setRenderHint(QPainter::SmoothPixmapTransform);
    }
    const QRectF target((part.x() - region.x()) * ratio, (part.y() - region.y()) * ratio, part.width() * ratio,
                        part.height() * ratio);
    painter.drawImage(target, image, QRectF(image.rect()));
    grabbed = true;
  }
  if (painter.isActive()) {
    painter.end();
  }
  return grabbed ? picture : QImage();
}

} // namespace mitcad::report
