// SPDX-License-Identifier: MIT
// The part grid's star on a tile (mitcad#1014): a selected tile's empty
// star, painted over a light, a dark, a grey and a selection-coloured
// picture on a light and a dark palette (and the application's), has an
// outline that stands out from what is behind it (its own empty middle);
// the outline the selected tile had before (the selection's text colour
// with nothing under it) does not over a light picture. A filled star gets
// no disc, and nothing is drawn outside the star's box (its hit area).
#include "library/PartStar.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>

#include <QApplication>
#include <QColor>
#include <QImage>
#include <QPainter>
#include <QPainterPath>
#include <QPainterPathStroker>
#include <QPalette>
#include <QPoint>
#include <QRect>
#include <QRectF>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "part_star.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// Relative luminance (0..1) from the sRGB channels, roughly.
double luminance(const QColor& c) { return (0.2126 * c.red() + 0.7152 * c.green() + 0.0722 * c.blue()) / 255.0; }

int channelGap(const QColor& a, const QColor& b) {
  return std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()), std::abs(a.blue() - b.blue())});
}

QPalette palette(bool dark) {
  QPalette p;
  p.setColor(QPalette::Window, dark ? QColor(0x2d, 0x2d, 0x2d) : QColor(0xef, 0xef, 0xef));
  p.setColor(QPalette::Base, dark ? QColor(0x1e, 0x1e, 0x1e) : QColor(0xff, 0xff, 0xff));
  p.setColor(QPalette::Text, dark ? QColor(0xff, 0xff, 0xff) : QColor(0x00, 0x00, 0x00));
  p.setColor(QPalette::PlaceholderText, dark ? QColor(0xff, 0xff, 0xff, 0x80) : QColor(0x00, 0x00, 0x00, 0x80));
  p.setColor(QPalette::Highlight, dark ? QColor(0x2a, 0x82, 0xda) : QColor(0x30, 0x8c, 0xc6));
  p.setColor(QPalette::HighlightedText, QColor(0xff, 0xff, 0xff));
  return p;
}

// A selected tile as the grid paints it (PartPicker.cpp, TileDelegate):
// the selection's band, the part's picture (here one colour) and, at the
// picture's top right corner, the star's box (starRect).
struct Tile {
  QImage image;
  QRectF star;
};

Tile selectedTile(const QPalette& p, const QColor& picture, int side) {
  Tile tile{QImage(120, 150, QImage::Format_ARGB32), QRectF()};
  tile.image.fill(p.color(QPalette::Highlight));
  const QRect pictureRect(12, 6, 96, 96);
  QPainter painter(&tile.image);
  painter.fillRect(pictureRect, picture);
  painter.end();
  tile.star = QRectF(pictureRect.right() - side + 1, pictureRect.top(), side, side);
  return tile;
}

QPoint pixelAt(const QPointF& point) {
  return QPoint(static_cast<int>(std::floor(point.x())), static_cast<int>(std::floor(point.y())));
}

// The largest luminance gap between the pixels on the star's outline and
// the star's empty middle (what the outline is seen against).
double outlineContrast(const QImage& image, const QRectF& box) {
  const QPainterPath star = partStarPath(box.adjusted(1.0, 1.0, -1.0, -1.0));
  QPainterPathStroker stroker;
  stroker.setWidth(0.8);
  const QPainterPath line = stroker.createStroke(star);
  const double behind = luminance(image.pixelColor(pixelAt(box.center())));
  double best = 0.0;
  const QRect pixels = box.toAlignedRect();
  for (int y = pixels.top(); y <= pixels.bottom(); ++y) {
    for (int x = pixels.left(); x <= pixels.right(); ++x) {
      if (line.contains(QPointF(x + 0.5, y + 0.5))) {
        best = std::max(best, std::abs(luminance(image.pixelColor(x, y)) - behind));
      }
    }
  }
  return best;
}

void emptyStarTests() {
  const QPalette palettes[] = {palette(false), palette(true), QApplication::palette()};
  for (const QPalette& p : palettes) {
    const QColor pictures[] = {QColor(0xf6, 0xf6, 0xf6), QColor(0x20, 0x20, 0x20), QColor(0x80, 0x80, 0x80),
                               p.color(QPalette::Highlight), p.color(QPalette::HighlightedText)};
    for (const QColor& picture : pictures) {
      for (const int side : {14, 22}) {
        Tile tile = selectedTile(p, picture, side);
        QPainter painter(&tile.image);
        paintTileStar(&painter, tile.star, false, p);
        painter.end();
        const double contrast = outlineContrast(tile.image, tile.star);
        if (contrast <= 0.3) {
          std::fprintf(stderr, "part_star.cpp: empty star over %s, side %d: outline contrast %.3f\n",
                       qPrintable(picture.name()), side, contrast);
        }
        CHECK(contrast > 0.3);
        // Nothing outside the box's inscribed disc: the box's corner keeps
        // the picture.
        CHECK(channelGap(tile.image.pixelColor(pixelAt(tile.star.topLeft())), picture) <= 2);
        CHECK(channelGap(tile.image.pixelColor(pixelAt(tile.star.bottomLeft() + QPointF(0.0, -0.5))), picture) <= 2);
      }
    }
  }
}

void oldOutlineTests() {
  // The selected tile's outline before mitcad#1014: the selection's text
  // colour (white) straight over the picture. Over a light picture it is
  // lost; the measure above sees that.
  const QPalette p = palette(false);
  Tile tile = selectedTile(p, QColor(0xf6, 0xf6, 0xf6), 22);
  QPainter painter(&tile.image);
  paintPartStar(&painter, tile.star, false, p.color(QPalette::HighlightedText));
  painter.end();
  CHECK(outlineContrast(tile.image, tile.star) < 0.1);
}

void filledStarTests() {
  for (const bool dark : {false, true}) {
    const QPalette p = palette(dark);
    const QColor picture = dark ? QColor(0x20, 0x20, 0x20) : QColor(0xf6, 0xf6, 0xf6);
    Tile tile = selectedTile(p, picture, 22);
    QPainter painter(&tile.image);
    paintTileStar(&painter, tile.star, true, p);
    painter.end();
    // Filled in the star's colour in the middle.
    CHECK(channelGap(tile.image.pixelColor(pixelAt(tile.star.center())), partStarColour()) <= 2);
    // No disc: between the star's two lower points the picture shows.
    const double radius = (tile.star.width() - 2.0) / 2.0;
    const QPointF below = tile.star.center() + QPointF(0.0, 0.8 * radius);
    CHECK(channelGap(tile.image.pixelColor(pixelAt(below)), picture) <= 2);
    CHECK(channelGap(tile.image.pixelColor(pixelAt(tile.star.topLeft())), picture) <= 2);
  }
}

} // namespace

int partStarTests() {
  emptyStarTests();
  oldOutlineTests();
  filledStarTests();
  return failures;
}
