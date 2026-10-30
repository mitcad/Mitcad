// SPDX-License-Identifier: MIT
#pragma once

// How sketch mode draws its points (mitcad#946): the plain marker of every
// point, a light fill with an outline that stands out from the selection's
// blue on either background, and the ball of a selected or hovered point;
// and the overlay's highlights of curves and points, painted so that the
// highlighted curves' point markers come after (over) their highlight.
// The view makes OCCT's marker images with the same painting, so that a
// marker the overlay paints again over a highlight looks as the view's.
// Plain Qt Gui: the app's unit tests take this file alone.

#include <vector>

#include <QColor>
#include <QImage>
#include <QPointF>
#include <QPolygonF>
#include <QString>

class QPainter;

namespace mitcad::sketch {

// What a point's marker tells, as the colours of the curves do: not fully
// constrained, fully constrained, construction, projected.
enum class PointState { Free, Constrained, Construction, Reference };

struct PointMarkerLook {
  QColor fill;
  QColor outline;
  // A dot in the outline's colour in the middle: a point that is not fully
  // constrained, so that the state reads without its colour.
  bool dot = false;
};

// Sizes in view pixels (device independent). The plain marker: the outline
// is drawn on a circle of kPointMarkerRadius, kPointMarkerOutline wide
// (about 11 px across, a little larger than OCCT's 9 px ring it replaces).
// A selected or hovered point's ball covers it: filled in the highlight's
// colour, its thin light outline on a circle of kPointBallRadius.
constexpr double kPointMarkerRadius = 4.7;
constexpr double kPointMarkerOutline = 1.6;
constexpr double kPointDotRadius = 1.5;
constexpr double kPointBallRadius = 5.6;
constexpr double kPointBallOutline = 1.4;

// A point's marker on a light or a dark background: a light fill
// everywhere; the outline dark on a light background and light enough to
// read on a dark one, never the selection's blue.
PointMarkerLook pointMarkerLook(PointState state, bool darkBackground);
// The look faded `fade` of the way towards `paper` (0 none, 1 all): a point
// outside the active component or shown only for a command, as its curves.
PointMarkerLook fadedLook(const PointMarkerLook& look, const QColor& paper, double fade);
// The thin light outline of a selected or hovered point's ball.
QColor pointBallOutline();

void paintPointMarker(QPainter& painter, const QPointF& at, const PointMarkerLook& look);
void paintPointBall(QPainter& painter, const QPointF& at, const QColor& color);

// The same as images for OCCT's markers, `ratio` device pixels per view
// pixel (OCCT draws a marker image pixel for pixel): odd sized, centred on
// the middle pixel, transparent around. A key that names the look and
// size, for sharing one image between the markers that look alike.
QImage pointMarkerImage(const PointMarkerLook& look, double ratio);
QImage pointBallImage(const QColor& color, double ratio);
QString pointMarkerKey(const PointMarkerLook& look, double ratio);
QString pointBallKey(const QColor& color, double ratio);

// The overlay's highlights in view pixels (SketchOverlay::highlightLayers).
struct HighlightStroke {
  QPolygonF path;
  QColor color;
  // Body geometry out of the sketch plane where it is (mitcad#866): thin
  // and dashed; a single point as a small ring.
  bool dashed = false;
};
struct HighlightMarker {
  QPointF at;
  PointMarkerLook look;
};
struct HighlightBall {
  QPointF at;
  QColor color;
};
struct HighlightLayers {
  // Curves, lines and axes highlighted (4 px), in the order given: a later
  // one (the hover) over an earlier one (the selection).
  std::vector<HighlightStroke> strokes;
  // The plain markers of the highlighted curves' points that are not
  // highlighted themselves, as the view draws them.
  std::vector<HighlightMarker> markers;
  // The highlighted points (sketch points, body vertices, the origin).
  std::vector<HighlightBall> balls;
};

enum class HighlightPass { Strokes, Markers, Balls };

// The order of the overlay's highlights, the one place that decides it
// (mitcad#946): all strokes, then the markers over them, then the balls
// over those. The overlay paints after OCCT's scene, so a highlight it
// painted last covered the view's point markers whatever OCCT's layers
// were.
std::vector<HighlightPass> highlightPassOrder();
// Paints the highlights in highlightPassOrder(); returns the passes in the
// order painted, for the tests.
std::vector<HighlightPass> paintHighlightLayers(QPainter& painter, const HighlightLayers& layers);
// Whether `order` paints the markers after the strokes and the balls after
// the markers.
bool markersOverHighlights(const std::vector<HighlightPass>& order);

} // namespace mitcad::sketch
