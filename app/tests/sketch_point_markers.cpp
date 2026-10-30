// SPDX-License-Identifier: MIT
// The sketch points' markers (mitcad#946): a light fill with an outline
// that stands out from the selection's blue on a light and a dark
// background, the state told by the outline and a dot, the ball of a
// selected or hovered point with a thin light outline, the images OCCT
// draws them from; and the overlay's highlights painted with the
// highlighted curve's point markers over the highlight (painted into an
// image: a fit point's fill shows where the 4 px highlight runs under it).
#include "sketch/PointMarkers.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <vector>

#include <QColor>
#include <QImage>
#include <QPainter>
#include <QPointF>
#include <QPolygonF>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_point_markers.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

// The selection's and the hover's colours (SketchOverlay.cpp).
const QColor kSelection(0x1f, 0x7a, 0xff);
const QColor kHover(0x5a, 0xa8, 0xff);
const QColor kLightBackground(0xff, 0xff, 0xff);
const QColor kDarkBackground(0x28, 0x2b, 0x31);

// The largest difference of a channel.
int channelGap(const QColor& a, const QColor& b) {
  return std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()), std::abs(a.blue() - b.blue())});
}

// Relative luminance (0..1) from the sRGB channels, roughly.
double luminance(const QColor& c) {
  return (0.2126 * c.red() + 0.7152 * c.green() + 0.0722 * c.blue()) / 255.0;
}

bool alike(const QColor& a, const QColor& b, int tolerance) {
  return channelGap(a, b) <= tolerance;
}

void lookTests() {
  for (const bool dark : {false, true}) {
    const QColor background = dark ? kDarkBackground : kLightBackground;
    const PointMarkerLook freeLook = pointMarkerLook(PointState::Free, dark);
    const PointMarkerLook fixedLook = pointMarkerLook(PointState::Constrained, dark);
    // A light fill, clearly apart from the selection's and the hover's blue.
    const PointMarkerLook both[] = {freeLook, fixedLook};
    for (const PointMarkerLook& look : both) {
      CHECK(luminance(look.fill) > 0.85);
      CHECK(channelGap(look.fill, kSelection) > 150);
      CHECK(channelGap(look.fill, kHover) > 80);
      // The outline reads on the background and is not the selection's blue.
      CHECK(std::abs(luminance(look.outline) - luminance(background)) > 0.3);
      CHECK(std::abs(luminance(look.outline) - luminance(kSelection)) > 0.15);
      // And it stands apart from the fill, so that the ring shows.
      CHECK(std::abs(luminance(look.outline) - luminance(look.fill)) > 0.2);
    }
    // Dark outlines on a light background, lighter ones on a dark one.
    CHECK(dark ? luminance(freeLook.outline) > 0.3 : luminance(freeLook.outline) < 0.3);
    CHECK(dark ? luminance(fixedLook.outline) > 0.3 : luminance(fixedLook.outline) < 0.15);
    // The state reads without the colour: a dot in a point that is not
    // fully constrained, and the outlines differ as well.
    CHECK(freeLook.dot && !fixedLook.dot);
    CHECK(channelGap(freeLook.outline, fixedLook.outline) > 45);
    // Construction and projected points keep their own outlines.
    const PointMarkerLook construction = pointMarkerLook(PointState::Construction, dark);
    const PointMarkerLook reference = pointMarkerLook(PointState::Reference, dark);
    CHECK(channelGap(construction.outline, fixedLook.outline) > 45 && channelGap(reference.outline, fixedLook.outline) > 45);
    CHECK(channelGap(construction.outline, reference.outline) > 45);
    CHECK(luminance(construction.fill) > 0.85 && luminance(reference.fill) > 0.85);
  }
  // A little larger than the 9 px ring it replaces; the ball covers it.
  CHECK(2.0 * (kPointMarkerRadius + kPointMarkerOutline / 2.0) > 10.0);
  CHECK(kPointBallRadius + kPointBallOutline / 2.0 > kPointMarkerRadius + kPointMarkerOutline / 2.0);
  // The ball's outline is light.
  CHECK(luminance(pointBallOutline()) > 0.85);
  // Faded: nothing with no fade, the paper with all of it.
  const PointMarkerLook freeLook = pointMarkerLook(PointState::Free, false);
  const QColor paper(0xd8, 0xdc, 0xe2);
  CHECK(fadedLook(freeLook, paper, 0.0).outline == freeLook.outline);
  CHECK(alike(fadedLook(freeLook, paper, 1.0).outline, paper, 1) && alike(fadedLook(freeLook, paper, 1.0).fill, paper, 1));
  CHECK(fadedLook(freeLook, paper, 0.5).dot);
}

// A fitted spline's highlight as a level 4 px line at y = 20.5 from x = 4 to
// 76, its three fit points on it at x = 20.5, 40.5, 60.5 (pixel middles).
HighlightLayers splineSelected(bool dark) {
  HighlightLayers layers;
  QPolygonF path;
  path << QPointF(4.0, 20.5) << QPointF(40.5, 20.5) << QPointF(76.0, 20.5);
  layers.strokes.push_back({path, kSelection, false});
  for (const double x : {20.5, 40.5, 60.5}) {
    layers.markers.push_back({QPointF(x, 20.5), pointMarkerLook(PointState::Free, dark)});
  }
  return layers;
}

QImage canvas(const QColor& background) {
  QImage image(80, 40, QImage::Format_ARGB32_Premultiplied);
  image.fill(background);
  return image;
}

void orderTests() {
  for (const bool dark : {false, true}) {
    const HighlightLayers layers = splineSelected(dark);
    const PointMarkerLook look = pointMarkerLook(PointState::Free, dark);
    QImage image = canvas(dark ? kDarkBackground : kLightBackground);
    std::vector<HighlightPass> order;
    {
      QPainter painter(&image);
      painter.setRenderHint(QPainter::Antialiasing);
      order = paintHighlightLayers(painter, layers);
    }
    // Strokes, then markers, then balls, as highlightPassOrder says (the
    // overlay's log reads it there).
    CHECK(markersOverHighlights(order));
    CHECK(order == highlightPassOrder());
    CHECK(order.size() == 3 && order.front() == HighlightPass::Strokes && order.back() == HighlightPass::Balls);
    // The highlight between the points is the selection's blue.
    CHECK(alike(image.pixelColor(10, 20), kSelection, 8));
    CHECK(alike(image.pixelColor(30, 20), kSelection, 8));
    for (const int x : {20, 40, 60}) {
      // Inside each fit point's ring, between its dot and its outline, its
      // light fill over the highlight: the point shows on the curve.
      CHECK(alike(image.pixelColor(x + 2, 20), look.fill, 12));
      CHECK(alike(image.pixelColor(x, 20 + 2), look.fill, 12));
      CHECK(channelGap(image.pixelColor(x + 2, 20), kSelection) > 150);
      // Its outline over the highlight too (4.5 to 5.5 px from the middle).
      CHECK(alike(image.pixelColor(x + 5, 20), look.outline, 40));
      // And its dot in the middle.
      CHECK(alike(image.pixelColor(x, 20), look.outline, 40));
    }
  }
  // Painted the other way round (the order before mitcad#946), the
  // highlight covers the fill: the check above tells the two apart.
  const HighlightLayers layers = splineSelected(false);
  QImage wrong = canvas(kLightBackground);
  {
    QPainter painter(&wrong);
    painter.setRenderHint(QPainter::Antialiasing);
    HighlightLayers markers = layers;
    markers.strokes.clear();
    HighlightLayers strokes = layers;
    strokes.markers.clear();
    paintHighlightLayers(painter, markers);
    paintHighlightLayers(painter, strokes);
  }
  CHECK(alike(wrong.pixelColor(22, 20), kSelection, 8));
  CHECK(!markersOverHighlights({HighlightPass::Markers, HighlightPass::Strokes, HighlightPass::Balls}));
  CHECK(!markersOverHighlights({HighlightPass::Strokes, HighlightPass::Balls, HighlightPass::Markers}));
  CHECK(!markersOverHighlights({}));
}

void ballTests() {
  // A selected fit point on its selected spline (a line with its end points
  // alike): the ball over the highlight and the plain marker, filled in the
  // selection's colour, its thin light outline round it.
  for (const bool dark : {false, true}) {
    HighlightLayers layers = splineSelected(dark);
    layers.markers.erase(layers.markers.begin() + 1); // the selected point: a ball, no marker
    layers.balls.push_back({QPointF(40.5, 20.5), kSelection});
    QImage image = canvas(dark ? kDarkBackground : kLightBackground);
    {
      QPainter painter(&image);
      painter.setRenderHint(QPainter::Antialiasing);
      paintHighlightLayers(painter, layers);
    }
    CHECK(alike(image.pixelColor(40, 20), kSelection, 8));
    CHECK(alike(image.pixelColor(42, 20), kSelection, 8));
    // The light outline 5.6 px out (5.5 to 6.5 px from the middle), across
    // the highlight and above it.
    CHECK(luminance(image.pixelColor(46, 20)) > 0.75);
    CHECK(luminance(image.pixelColor(40, 14)) > 0.75);
    // The other points still show their fill.
    CHECK(alike(image.pixelColor(22, 20), pointMarkerLook(PointState::Free, dark).fill, 12));
  }
  // A hovered point alike in the hover colour, its outline light on a dark
  // background where nothing is under it.
  HighlightLayers hovered;
  hovered.balls.push_back({QPointF(40.5, 20.5), kHover});
  QImage image = canvas(kDarkBackground);
  {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    paintHighlightLayers(painter, hovered);
  }
  CHECK(alike(image.pixelColor(40, 20), kHover, 8));
  CHECK(luminance(image.pixelColor(46, 20)) > 0.6);
  CHECK(luminance(image.pixelColor(40, 14)) > 0.6);
}

void imageTests() {
  // The view's markers: odd sized, the middle on a pixel's middle,
  // transparent in the corners, opaque fill and outline; twice the pixels
  // at a device pixel ratio of 2.
  const PointMarkerLook fixedLook = pointMarkerLook(PointState::Constrained, false);
  const QImage one = pointMarkerImage(fixedLook, 1.0);
  CHECK(one.format() == QImage::Format_RGBA8888);
  CHECK(one.width() == one.height() && one.width() % 2 == 1);
  CHECK(one.width() >= 13 && one.width() <= 17);
  const int middle = one.width() / 2;
  CHECK(alike(one.pixelColor(middle, middle), fixedLook.fill, 2) && one.pixelColor(middle, middle).alpha() == 255);
  CHECK(one.pixelColor(0, 0).alpha() == 0);
  const QColor ring = one.pixelColor(middle + 5, middle);
  CHECK(ring.alpha() > 200 && alike(ring, fixedLook.outline, 40));
  const QImage two = pointMarkerImage(fixedLook, 2.0);
  CHECK(two.width() % 2 == 1 && two.width() > 2 * one.width() - 6);
  CHECK(alike(two.pixelColor(two.width() / 2, two.height() / 2), fixedLook.fill, 2));
  // A free point's dot in the middle.
  const PointMarkerLook freeLook = pointMarkerLook(PointState::Free, false);
  const QImage dotted = pointMarkerImage(freeLook, 1.0);
  CHECK(alike(dotted.pixelColor(dotted.width() / 2, dotted.height() / 2), freeLook.outline, 40));
  // The ball: larger, the colour in the middle, a light rim.
  const QImage ball = pointBallImage(kSelection, 1.0);
  CHECK(ball.width() % 2 == 1 && ball.width() >= one.width());
  CHECK(alike(ball.pixelColor(ball.width() / 2, ball.height() / 2), kSelection, 4));
  const QColor rim = ball.pixelColor(ball.width() / 2 + 6, ball.height() / 2);
  CHECK(rim.alpha() > 100 && luminance(rim) > 0.85);
  // One image per look and size.
  CHECK(pointMarkerKey(freeLook, 1.0) != pointMarkerKey(fixedLook, 1.0));
  CHECK(pointMarkerKey(fixedLook, 1.0) != pointMarkerKey(fixedLook, 2.0));
  CHECK(pointMarkerKey(fixedLook, 1.0) == pointMarkerKey(pointMarkerLook(PointState::Constrained, false), 1.0));
  CHECK(pointBallKey(kSelection, 1.0) != pointBallKey(kHover, 1.0));
}

} // namespace

int sketchPointMarkersTests() {
  failures = 0;
  lookTests();
  orderTests();
  ballTests();
  imageTests();
  return failures;
}
