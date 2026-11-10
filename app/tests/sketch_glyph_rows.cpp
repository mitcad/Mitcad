// SPDX-License-Identifier: MIT
// Where a row of constraint symbols goes in the sketch overlay
// (mitcad#1594): clear of the curves, so that a symbol never covers a curve
// and takes its clicks. Two close parallel lines (a parallel's second
// symbol beside the lower line covered the upper one), a line through a
// row, a dimension's text over it, and no side clear near it.
#include "sketch/GlyphRows.hpp"

#include <cmath>
#include <cstdio>
#include <optional>
#include <vector>

#include <QPointF>
#include <QRectF>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_glyph_rows.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

// The overlay's symbol size (view pixels).
constexpr double kSize = 15.0;
// A symbol's row is drawn this far beside its curve's middle.
constexpr double kBeside = 14.0;

QRectF symbolAt(const QPointF& center) {
  return QRectF(center.x() - kSize / 2, center.y() - kSize / 2, kSize, kSize);
}

std::vector<QPointF> line(const QPointF& a, const QPointF& b) { return {a, b}; }

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

// A level line through y, 200 px long, centred on x = 0.
std::vector<QPointF> level(double y) { return line(QPointF(-100.0, y), QPointF(100.0, y)); }

void distanceTests() {
  const QRectF rect(0.0, 0.0, 10.0, 10.0);
  CHECK(same(rectDistance(rect, {QPointF(13.0, 14.0)}), 5.0));
  CHECK(same(rectDistance(rect, {QPointF(5.0, 5.0)}), 0.0));
  // A segment across the rect, and one beside it.
  CHECK(same(rectDistance(rect, line(QPointF(-5.0, 5.0), QPointF(15.0, 5.0))), 0.0));
  CHECK(same(rectDistance(rect, line(QPointF(-5.0, -3.0), QPointF(15.0, -3.0))), 3.0));
  // A slanted one nearest to a corner.
  CHECK(same(rectDistance(rect, line(QPointF(22.0, 0.0), QPointF(0.0, 22.0))), std::sqrt(2.0)));
  CHECK(std::isinf(rectDistance(rect, {})));
  CHECK(rectTouches(rect, {QPointF(11.0, 5.0)}));
  CHECK(!rectTouches(rect, {QPointF(13.0, 5.0)}));
}

// Two parallel lines 12 px apart: the lower line's symbol, 14 px above its
// middle, covered the upper line's middle and took its clicks.
void parallelTests() {
  const std::vector<QPointF> lower = level(0.0);
  const std::vector<QPointF> upper = level(-12.0);
  const std::vector<GlyphCurve> curves{glyphCurve(lower), glyphCurve(upper)};
  const QRectF row = symbolAt(QPointF(0.0, -kBeside));
  CHECK(rectDistance(row, upper) <= kGlyphCurveClearance); // the case reported
  // Alone: up, just past the upper line.
  const QPointF up = glyphRowOffset(row, QPointF(0.0, -1.0), {}, {}, curves, lower);
  CHECK(same(up.x(), 0.0));
  CHECK(up.y() < 0.0);
  CHECK(rectDistance(row.translated(up), upper) > kGlyphCurveClearance);
  CHECK(rectDistance(row.translated(up), lower) > kGlyphCurveClearance);
  CHECK(up.y() > -kSize); // no farther than it must
  // With the upper line's own symbol 14 px above it (placed before): below
  // the lower line, the nearer side clear of both lines and that symbol.
  const QRectF upperRow = symbolAt(QPointF(0.0, -12.0 - kBeside));
  const QPointF down = glyphRowOffset(row, QPointF(0.0, -1.0), {}, {upperRow}, curves, lower);
  CHECK(same(down.x(), 0.0));
  CHECK(down.y() > 0.0);
  const QRectF below = row.translated(down);
  CHECK(rectDistance(below, upper) > kGlyphCurveClearance);
  CHECK(rectDistance(below, lower) > kGlyphCurveClearance);
  CHECK(!below.intersects(upperRow));
  CHECK(!rectTouches(below, lower));
  // The upper line farther away: nothing moves.
  const std::vector<GlyphCurve> apart{glyphCurve(lower), glyphCurve(level(-30.0))};
  const QPointF stays = glyphRowOffset(row, QPointF(0.0, -1.0), {}, {}, apart, lower);
  CHECK(same(stays.x(), 0.0) && same(stays.y(), 0.0));
}

// A line from the row's own line's middle upwards, through the row: never
// clear above within the limit, so below the own line.
void throughTests() {
  const std::vector<QPointF> own = level(0.0);
  const std::vector<QPointF> rising = line(QPointF(0.0, 0.0), QPointF(0.0, -200.0));
  const std::vector<GlyphCurve> curves{glyphCurve(own), glyphCurve(rising)};
  const QRectF row = symbolAt(QPointF(0.0, -kBeside));
  CHECK(!clearDistance(row, QPointF(0.0, -1.0), {}, curves, kGlyphMoveLimit).has_value());
  const QPointF offset = glyphRowOffset(row, QPointF(0.0, -1.0), {}, {}, curves, own);
  CHECK(same(offset.x(), 0.0));
  CHECK(offset.y() > 0.0);
  CHECK(rectDistance(row.translated(offset), own) > kGlyphCurveClearance);
  CHECK(rectDistance(row.translated(offset), rising) > kGlyphCurveClearance);
}

// A dimension's text over the row: up past the text (the move down would
// put it over its own line, and further down is farther), as before.
void textTests() {
  const std::vector<QPointF> own = level(0.0);
  const std::vector<GlyphCurve> curves{glyphCurve(own)};
  const QRectF row = symbolAt(QPointF(0.0, -kBeside));
  const QRectF text(-20.0, -25.0, 40.0, 15.0);
  const QPointF offset = glyphRowOffset(row, QPointF(0.0, -1.0), {text}, {}, curves, own);
  // The text's top (-25) less the row's bottom (-6.5), and a pixel to spare.
  CHECK(same(offset.x(), 0.0));
  CHECK(same(offset.y(), -19.5));
  // Clear of everything at once: no move.
  const QPointF none = glyphRowOffset(row, QPointF(0.0, -1.0), {QRectF(50.0, -25.0, 40.0, 15.0)}, {}, curves, own);
  CHECK(same(none.x(), 0.0) && same(none.y(), 0.0));
}

// No side clear within the limit (a line across both sides): only the texts
// move it, to the side not over its own line.
void blockedTests() {
  const std::vector<QPointF> own = level(0.0);
  const std::vector<GlyphCurve> curves{glyphCurve(own), glyphCurve(line(QPointF(0.0, 200.0), QPointF(0.0, -200.0)))};
  const QRectF row = symbolAt(QPointF(0.0, -kBeside));
  const QPointF stays = glyphRowOffset(row, QPointF(0.0, -1.0), {}, {}, curves, own);
  CHECK(same(stays.x(), 0.0) && same(stays.y(), 0.0));
  const QRectF text(-20.0, -25.0, 40.0, 15.0);
  const QPointF moved = glyphRowOffset(row, QPointF(0.0, -1.0), {text}, {}, curves, own);
  CHECK(same(moved.x(), 0.0));
  CHECK(same(moved.y(), -19.5));
  // A null normal is up.
  const QPointF up = glyphRowOffset(row, QPointF(), {text}, {}, curves, own);
  CHECK(same(up.y(), -19.5));
}

} // namespace

int sketchGlyphRowsTests() {
  failures = 0;
  distanceTests();
  parallelTests();
  throughTests();
  textTests();
  blockedTests();
  return failures;
}
