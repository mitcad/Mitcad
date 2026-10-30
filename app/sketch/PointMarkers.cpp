// SPDX-License-Identifier: MIT
#include "PointMarkers.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>

#include <QPainter>
#include <QPen>

namespace mitcad::sketch {
namespace {

// The fill of every marker: light, far from the selection's blue (#1f7aff)
// on either background.
const QColor kMarkerFill(0xff, 0xff, 0xff);
// The outlines on a light background: dark, the free one a deep blue that
// is much darker than the selection's; construction and projected points in
// darker tones of their curves' orange and purple.
const QColor kFreeOutline(0x18, 0x46, 0xb0);
const QColor kConstrainedOutline(0x1b, 0x1d, 0x22);
const QColor kConstructionOutline(0xc2, 0x5e, 0x00);
const QColor kReferenceOutline(0x7a, 0x2a, 0xa0);
// On a dark background: light enough to read on it, the free one a light
// blue, the fully constrained one a light grey.
const QColor kFreeOutlineDark(0x8a, 0xb4, 0xff);
const QColor kConstrainedOutlineDark(0xaa, 0xb2, 0xbe);
const QColor kConstructionOutlineDark(0xf2, 0xa4, 0x47);
const QColor kReferenceOutlineDark(0xc2, 0x7c, 0xf0);
// The ball's thin outline: light, so that the ball stands apart from a
// curve's highlight in the same colour under it.
const QColor kBallOutline(0xff, 0xff, 0xff);

QColor towards(const QColor& from, const QColor& to, double fade) {
  const auto mix = [fade](double a, double b) { return a + (b - a) * fade; };
  return QColor::fromRgbF(static_cast<float>(mix(from.redF(), to.redF())),
                          static_cast<float>(mix(from.greenF(), to.greenF())),
                          static_cast<float>(mix(from.blueF(), to.blueF())), from.alphaF());
}

// An image `ratio` device pixels per view pixel, large enough for a circle
// of `extent` view pixels round its middle, with `paint` drawing at (0, 0)
// in view pixels: odd sized, so that the middle is a pixel's middle.
template <typename Paint>
QImage markerImage(double extent, double ratio, Paint paint) {
  const double scale = ratio > 0.0 ? ratio : 1.0;
  const int half = static_cast<int>(std::ceil((extent + 1.0) * scale));
  const int size = 2 * half + 1;
  QImage image(size, size, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  {
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.translate(half + 0.5, half + 0.5);
    painter.scale(scale, scale);
    paint(painter);
  }
  // Straight alpha, as OCCT's RGBA marker images are.
  return image.convertToFormat(QImage::Format_RGBA8888);
}

} // namespace

PointMarkerLook pointMarkerLook(PointState state, bool darkBackground) {
  PointMarkerLook look;
  look.fill = kMarkerFill;
  switch (state) {
  case PointState::Free:
    look.outline = darkBackground ? kFreeOutlineDark : kFreeOutline;
    look.dot = true;
    break;
  case PointState::Constrained:
    look.outline = darkBackground ? kConstrainedOutlineDark : kConstrainedOutline;
    break;
  case PointState::Construction:
    look.outline = darkBackground ? kConstructionOutlineDark : kConstructionOutline;
    break;
  case PointState::Reference:
    look.outline = darkBackground ? kReferenceOutlineDark : kReferenceOutline;
    break;
  }
  return look;
}

PointMarkerLook fadedLook(const PointMarkerLook& look, const QColor& paper, double fade) {
  if (fade <= 0.0 || !paper.isValid()) {
    return look;
  }
  PointMarkerLook faded = look;
  faded.fill = towards(look.fill, paper, fade);
  faded.outline = towards(look.outline, paper, fade);
  return faded;
}

QColor pointBallOutline() {
  return kBallOutline;
}

void paintPointMarker(QPainter& painter, const QPointF& at, const PointMarkerLook& look) {
  painter.save();
  painter.setPen(QPen(look.outline, kPointMarkerOutline));
  painter.setBrush(look.fill);
  painter.drawEllipse(at, kPointMarkerRadius, kPointMarkerRadius);
  if (look.dot) {
    painter.setPen(Qt::NoPen);
    painter.setBrush(look.outline);
    painter.drawEllipse(at, kPointDotRadius, kPointDotRadius);
  }
  painter.restore();
}

void paintPointBall(QPainter& painter, const QPointF& at, const QColor& color) {
  painter.save();
  painter.setPen(QPen(kBallOutline, kPointBallOutline));
  painter.setBrush(color);
  painter.drawEllipse(at, kPointBallRadius, kPointBallRadius);
  painter.restore();
}

QImage pointMarkerImage(const PointMarkerLook& look, double ratio) {
  return markerImage(kPointMarkerRadius + kPointMarkerOutline / 2.0, ratio,
                     [&look](QPainter& painter) { paintPointMarker(painter, QPointF(0.0, 0.0), look); });
}

QImage pointBallImage(const QColor& color, double ratio) {
  return markerImage(kPointBallRadius + kPointBallOutline / 2.0, ratio,
                     [&color](QPainter& painter) { paintPointBall(painter, QPointF(0.0, 0.0), color); });
}

QString pointMarkerKey(const PointMarkerLook& look, double ratio) {
  return QStringLiteral("marker %1 %2 %3 %4")
      .arg(look.fill.name(QColor::HexArgb), look.outline.name(QColor::HexArgb),
           look.dot ? QStringLiteral("dot") : QStringLiteral("plain"))
      .arg(ratio, 0, 'f', 3);
}

QString pointBallKey(const QColor& color, double ratio) {
  return QStringLiteral("ball %1 %2").arg(color.name(QColor::HexArgb)).arg(ratio, 0, 'f', 3);
}

std::vector<HighlightPass> highlightPassOrder() {
  return {HighlightPass::Strokes, HighlightPass::Markers, HighlightPass::Balls};
}

std::vector<HighlightPass> paintHighlightLayers(QPainter& painter, const HighlightLayers& layers) {
  const std::vector<HighlightPass> order = highlightPassOrder();
  for (const HighlightPass pass : order) {
    switch (pass) {
    case HighlightPass::Strokes:
      painter.save();
      painter.setBrush(Qt::NoBrush);
      for (const HighlightStroke& stroke : layers.strokes) {
        if (stroke.dashed) {
          painter.setPen(QPen(stroke.color, 1.5, Qt::DashLine, Qt::RoundCap, Qt::RoundJoin));
        } else {
          painter.setPen(QPen(stroke.color, 4.0, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        }
        if (stroke.path.size() == 1) {
          painter.drawEllipse(stroke.path.front(), 3.0, 3.0);
        } else {
          painter.drawPolyline(stroke.path);
        }
      }
      painter.restore();
      break;
    case HighlightPass::Markers:
      for (const HighlightMarker& marker : layers.markers) {
        paintPointMarker(painter, marker.at, marker.look);
      }
      break;
    case HighlightPass::Balls:
      for (const HighlightBall& ball : layers.balls) {
        paintPointBall(painter, ball.at, ball.color);
      }
      break;
    }
  }
  return order;
}

bool markersOverHighlights(const std::vector<HighlightPass>& order) {
  const auto at = [&order](HighlightPass pass) -> std::ptrdiff_t {
    return std::find(order.begin(), order.end(), pass) - order.begin();
  };
  const std::ptrdiff_t strokes = at(HighlightPass::Strokes);
  const std::ptrdiff_t markers = at(HighlightPass::Markers);
  const std::ptrdiff_t balls = at(HighlightPass::Balls);
  const auto end = static_cast<std::ptrdiff_t>(order.size());
  return strokes < end && markers < end && balls < end && strokes < markers && markers < balls;
}

} // namespace mitcad::sketch
