// SPDX-License-Identifier: MIT
#pragma once

// Where a row of constraint symbols goes in the sketch overlay (mitcad#1594).
// A row starts beside its point or curve; only a collision moves it, along
// its normal to the nearer side that is clear: clear of the dimensions'
// texts, of the symbol rows placed before it and of every curve, its own
// included, so that a symbol never covers a curve and takes the clicks
// meant for it (a parallel's second symbol beside the other line of two
// close parallel lines covered the first line's middle). When neither side
// is clear within a few symbols' width, only the texts move it, as before.
// View pixels throughout. Plain Qt Core: the app's unit tests take this
// file alone.

#include <optional>
#include <vector>

#include <QPointF>
#include <QRectF>

namespace mitcad::sketch {

// How far (view pixels) a symbol keeps from a curve: a click on a curve's
// line, or this near it, reaches the curve and not a symbol beside it.
constexpr double kGlyphCurveClearance = 3.0;
// How far a row moves at most to clear the curves and the other rows;
// farther it would no longer read as beside its own point or curve.
constexpr double kGlyphMoveLimit = 45.0;

// A curve as the rows keep clear of it: a polyline (or a single point) and
// its bounds.
struct GlyphCurve {
  std::vector<QPointF> outline;
  QRectF bounds;
};
GlyphCurve glyphCurve(std::vector<QPointF> outline);

// Whether `rect`, with a 2 px margin, holds a point of `outline` (a single
// point, or a polyline) or crosses one of its segments.
bool rectTouches(const QRectF& rect, const std::vector<QPointF>& outline);

// The shortest distance from `rect` to `outline` (a single point, or a
// polyline): 0 where they meet, infinity for an empty outline.
double rectDistance(const QRectF& rect, const std::vector<QPointF>& outline);

// How far `row` moves along `direction` (a unit vector) until it is clear
// of `boxes` (each left behind with a pixel to spare) and keeps
// kGlyphCurveClearance from each of `curves`; none when that takes more
// than `limit`. Each box moves it once at most (as many moves as there are
// boxes); a curve a pixel at a time.
std::optional<double> clearDistance(const QRectF& row, const QPointF& direction, const std::vector<QRectF>& boxes,
                                    const std::vector<GlyphCurve>& curves, double limit);

// The offset that moves a row of symbols (`row`, its bounds) along
// `normal` (a unit vector; up when null): none while it is clear; else to
// the nearer side clear of `texts`, `rows` (the rows placed before it) and
// `curves` (every curve, its own included), within kGlyphMoveLimit. When
// neither side is, clear of the texts alone, to the nearer side that does
// not put it over `own` (its point or curve; the original side when both
// would).
QPointF glyphRowOffset(const QRectF& row, const QPointF& normal, const std::vector<QRectF>& texts,
                       const std::vector<QRectF>& rows, const std::vector<GlyphCurve>& curves,
                       const std::vector<QPointF>& own);

} // namespace mitcad::sketch
