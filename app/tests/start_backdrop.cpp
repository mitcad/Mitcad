// SPDX-License-Identifier: MIT
// The Start page's rounded backdrop (mitcad#1439, files/StartBackdrop.hpp),
// painted into an image as the page paints itself over the 3D view: its
// rectangle inset from the view's edges (below the rows over the view's top
// and above a band over its bottom when they are there), the backdrop's
// colour in its middle and along its sides, and at its corners, outside the
// rounding, the view's background, not the backdrop's colour; outside the
// backdrop the view's gradient, on a light and a dark palette, at 1x and 2x.
#include "files/StartBackdrop.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>

#include <QColor>
#include <QImage>
#include <QMargins>
#include <QPainter>
#include <QPoint>
#include <QPointF>
#include <QRectF>
#include <QSize>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "start_backdrop.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// As the page: FloatingLayout's kMargin and kGap, GlassCard's kRadius.
constexpr int kInset = 12;
constexpr int kGap = 8;
constexpr qreal kRadius = 14;

int channelGap(const QColor& a, const QColor& b) {
  return std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()), std::abs(a.blue() - b.blue())});
}

// The pixel at `point` of the page (in the page's points).
QColor pixel(const QImage& image, const QPointF& point) {
  const qreal ratio = image.devicePixelRatio();
  return image.pixelColor(QPoint(static_cast<int>(std::floor(point.x() * ratio)),
                                 static_cast<int>(std::floor(point.y() * ratio))));
}

start::Backdrop backdrop(bool dark) {
  start::Backdrop b;
  b.fill = dark ? QColor(0x2d, 0x2d, 0x2d) : QColor(0xef, 0xef, 0xef);
  b.border = dark ? QColor(0x48, 0x48, 0x48) : QColor(0xc8, 0xc8, 0xc8);
  // The view's background: a gradient from its top to its bottom.
  b.viewTop = dark ? QColor(0x5a, 0x64, 0x78) : QColor(0xa0, 0xb0, 0xc8);
  b.viewBottom = dark ? QColor(0x10, 0x14, 0x1c) : QColor(0x60, 0x70, 0x88);
  b.radius = kRadius;
  return b;
}

QImage page(const QSize& size, qreal ratio, const QRectF& rect, const start::Backdrop& b) {
  QImage image(size * ratio, QImage::Format_ARGB32);
  image.setDevicePixelRatio(ratio);
  image.fill(Qt::magenta); // what the GL view under the page drew last
  QPainter painter(&image);
  start::paintBackdrop(painter, size, rect, b);
  painter.end();
  return image;
}

void rectTests() {
  const QSize size(800, 500);
  // Nothing over the view (the Docked layout, the header above it on
  // macOS): inset on all four sides.
  CHECK(start::backdropRect(size, kInset, 0, 0, kGap) == QRectF(12, 12, 776, 476));
  CHECK(start::backdropMargins(kInset, 0, 0, kGap) == QMargins(12, 12, 12, 12));
  // The workspace tabs floating over the view's top end 40 down; a band
  // over its bottom keeps 50 of it.
  CHECK(start::backdropRect(size, kInset, 40, 50, kGap) == QRectF(12, 52, 776, 500 - 52 - 58));
  CHECK(start::backdropMargins(kInset, 40, 50, kGap) == QMargins(12, 52, 12, 58));
  // A band lower than the gap keeps at least the inset.
  CHECK(start::backdropMargins(kInset, 0, 2, kGap).bottom() == kInset);
  // A view too small for it: none.
  CHECK(start::backdropRect(QSize(20, 20), kInset, 0, 0, kGap).isEmpty());
  CHECK(start::backdropRect(QSize(300, 60), kInset, 40, 0, kGap).isEmpty());
}

void paintTests() {
  const QSize size(400, 300);
  for (const bool dark : {false, true}) {
    for (const qreal ratio : {1.0, 2.0}) {
      const start::Backdrop b = backdrop(dark);
      const QRectF rect = start::backdropRect(size, kInset, 0, 0, kGap);
      const QImage image = page(size, ratio, rect, b);
      // The backdrop's colour in its middle and inside its straight sides.
      CHECK(channelGap(pixel(image, rect.center()), b.fill) <= 1);
      CHECK(channelGap(pixel(image, QPointF(rect.left() + 3, rect.center().y())), b.fill) <= 1);
      CHECK(channelGap(pixel(image, QPointF(rect.center().x(), rect.top() + 3)), b.fill) <= 1);
      CHECK(channelGap(pixel(image, QPointF(rect.right() - 3, rect.center().y())), b.fill) <= 1);
      CHECK(channelGap(pixel(image, QPointF(rect.center().x(), rect.bottom() - 3)), b.fill) <= 1);
      // Its four corners, inside its rectangle but outside the rounding: not
      // the backdrop's colour, nor anything the view drew under the page.
      for (const QPointF& corner : {QPointF(rect.left() + 1.5, rect.top() + 1.5),
                                    QPointF(rect.right() - 1.5, rect.top() + 1.5),
                                    QPointF(rect.left() + 1.5, rect.bottom() - 1.5),
                                    QPointF(rect.right() - 1.5, rect.bottom() - 1.5)}) {
        const QColor at = pixel(image, corner);
        CHECK(channelGap(at, b.fill) > 20);
        CHECK(channelGap(at, QColor(Qt::magenta)) > 60);
        CHECK(at.alpha() == 255);
      }
      // Outside the backdrop the view's background: its top colour at the
      // top, its bottom colour at the bottom, everywhere opaque.
      CHECK(channelGap(pixel(image, QPointF(size.width() / 2.0, 0.5)), b.viewTop) <= 2);
      CHECK(channelGap(pixel(image, QPointF(size.width() / 2.0, size.height() - 0.5)), b.viewBottom) <= 2);
      CHECK(channelGap(pixel(image, QPointF(4, size.height() / 2.0)), b.fill) > 20);
      bool opaque = true;
      for (int y = 0; y < image.height(); ++y) {
        for (int x = 0; x < image.width(); ++x) {
          opaque = opaque && qAlpha(image.pixel(x, y)) == 255;
        }
      }
      CHECK(opaque);
    }
  }
}

void emptyTests() {
  // No backdrop: the view's background alone.
  const start::Backdrop b = backdrop(false);
  const QImage image = page(QSize(20, 20), 1.0, QRectF(), b);
  CHECK(channelGap(pixel(image, QPointF(10, 10)), b.fill) > 20);
  CHECK(channelGap(pixel(image, QPointF(10, 10)), QColor(Qt::magenta)) > 60);
}

} // namespace

int startBackdropTests() {
  rectTests();
  paintTests();
  emptyTests();
  return failures;
}
