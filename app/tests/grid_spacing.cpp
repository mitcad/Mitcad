// SPDX-License-Identifier: MIT
// The layout grid in the design's length unit (mitcad#1210,
// view/GridSpacing.hpp): the automatic spacing's series per unit (metric
// 1 or 5 times a power of ten mm; in: halves of an inch below 1 in, then
// 1, 2, 5, 10, 20, ... in; ft: the inch series up to 10 in, then 1, 2, 5,
// 10, ... ft), the major lines, and Grid Settings' fixed spacing converted
// from mm to the unit and back.
#include "view/GridSpacing.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>

#include <QByteArray>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "grid_spacing.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool nearly(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(1.0, std::abs(b)); }

// The automatic step in units of `size` mm when the lines may
// be at most `limit` units apart: the grid's densest lines are 35 pixels.
double stepAt(double limit, const char* unit, double size) {
  const double pixelsPerMm = 35.0 / (limit * size);
  return mitcad::layoutGridStep(pixelsPerMm, QString::fromLatin1(unit)) / size;
}

// The step's text as the log shows it, compared with what is expected
// (printed when it differs).
void expectText(double millimetres, const char* unit, const char* expected, int line) {
  const QString got = mitcad::gridSpacingText(millimetres, QString::fromLatin1(unit));
  if (got != QString::fromUtf8(expected)) {
    std::fprintf(stderr, "grid_spacing.cpp:%d: %g mm in '%s' is \"%s\", not \"%s\"\n", line, millimetres, unit,
                 got.toUtf8().constData(), expected);
    ++failures;
  }
}

#define EXPECT_TEXT(mm, unit, expected) expectText(mm, unit, expected, __LINE__)

void metricTests() {
  // 1 or 5 times a power of ten mm, as before the units (no 2).
  CHECK(nearly(stepAt(35.0, "mm", 1.0), 10.0));
  CHECK(nearly(stepAt(7.0, "mm", 1.0), 5.0));
  CHECK(nearly(stepAt(3.0, "mm", 1.0), 1.0));
  CHECK(nearly(stepAt(0.3, "mm", 1.0), 0.1));
  CHECK(nearly(stepAt(70.0, "mm", 1.0), 50.0));
  // cm and m designs keep the metric series (in mm).
  CHECK(nearly(stepAt(7.0, "cm", 1.0), 5.0));
  CHECK(nearly(stepAt(200.0, "m", 1.0), 100.0));
  CHECK(!mitcad::imperialGrid(QStringLiteral("mm")) && !mitcad::imperialGrid(QStringLiteral("cm")) &&
        !mitcad::imperialGrid(QStringLiteral("m")) && !mitcad::imperialGrid(QString()));
  // A scale that makes no view.
  CHECK(mitcad::layoutGridStep(0.0, QStringLiteral("mm")) == 10.0);
  CHECK(mitcad::layoutGridStep(std::nan(""), QStringLiteral("cm")) == 10.0);
  // Every fifth line a major one.
  CHECK(mitcad::layoutGridMajor(5.0, QStringLiteral("mm")) == 5);
  CHECK(mitcad::layoutGridMajor(10.0, QStringLiteral("cm")) == 5);
  CHECK(mitcad::layoutGridMajor(1.0, QStringLiteral("m")) == 5);
}

void inchTests() {
  const double inch = 25.4;
  CHECK(mitcad::imperialGrid(QStringLiteral("in")) && mitcad::imperialGrid(QStringLiteral("ft")));
  // 1/16, 1/8, 1/4, 1/2 in, halving further down.
  CHECK(nearly(stepAt(0.07, "in", inch), 0.0625));
  CHECK(nearly(stepAt(0.2, "in", inch), 0.125));
  CHECK(nearly(stepAt(0.3, "in", inch), 0.25));
  CHECK(nearly(stepAt(0.7, "in", inch), 0.5));
  CHECK(nearly(stepAt(0.05, "in", inch), 0.03125));
  // 1, 2, 5, 10 in, then 20, 50, ... in.
  CHECK(nearly(stepAt(1.5, "in", inch), 1.0));
  CHECK(nearly(stepAt(3.0, "in", inch), 2.0));
  CHECK(nearly(stepAt(7.0, "in", inch), 5.0));
  CHECK(nearly(stepAt(12.0, "in", inch), 10.0));
  CHECK(nearly(stepAt(30.0, "in", inch), 20.0));
  CHECK(nearly(stepAt(60.0, "in", inch), 50.0));
  CHECK(mitcad::layoutGridStep(-1.0, QStringLiteral("in")) == inch);
  // Major lines: a quarter, a half, an inch or two below an inch; 5 or 10
  // from an inch up; 20 for 5.
  CHECK(mitcad::layoutGridMajor(0.0625 * inch, QStringLiteral("in")) == 4);
  CHECK(mitcad::layoutGridMajor(0.25 * inch, QStringLiteral("in")) == 4);
  CHECK(mitcad::layoutGridMajor(0.5 * inch, QStringLiteral("in")) == 4);
  CHECK(mitcad::layoutGridMajor(1.0 * inch, QStringLiteral("in")) == 5);
  CHECK(mitcad::layoutGridMajor(2.0 * inch, QStringLiteral("in")) == 5);
  CHECK(mitcad::layoutGridMajor(5.0 * inch, QStringLiteral("in")) == 4);
  CHECK(mitcad::layoutGridMajor(10.0 * inch, QStringLiteral("in")) == 5);
  CHECK(mitcad::layoutGridMajor(50.0 * inch, QStringLiteral("in")) == 4);
}

void footTests() {
  const double inch = 25.4;
  // The inch series up to 10 in, then 1, 2, 5, 10 ft (here in inches).
  CHECK(nearly(stepAt(0.3, "ft", inch), 0.25));
  CHECK(nearly(stepAt(3.0, "ft", inch), 2.0));
  CHECK(nearly(stepAt(11.0, "ft", inch), 10.0));
  CHECK(nearly(stepAt(13.0, "ft", inch), 12.0));
  CHECK(nearly(stepAt(30.0, "ft", inch), 24.0));
  CHECK(nearly(stepAt(70.0, "ft", inch), 60.0));
  CHECK(nearly(stepAt(130.0, "ft", inch), 120.0));
  CHECK(mitcad::layoutGridMajor(0.5 * inch, QStringLiteral("ft")) == 4);
  CHECK(mitcad::layoutGridMajor(12.0 * inch, QStringLiteral("ft")) == 5);
  CHECK(mitcad::layoutGridMajor(24.0 * inch, QStringLiteral("ft")) == 5);
  CHECK(mitcad::layoutGridMajor(60.0 * inch, QStringLiteral("ft")) == 4);
}

void conversionTests() {
  // mm to the unit and back: the view settings keep mm.
  for (const char* unit : {"mm", "cm", "m", "in", "ft"}) {
    const QString name = QString::fromLatin1(unit);
    for (const double millimetres : {0.01, 1.5875, 6.35, 10.0, 304.8, 1234.5}) {
      CHECK(nearly(mitcad::gridSpacingMillimetres(mitcad::gridSpacingInUnit(millimetres, name), name), millimetres));
    }
  }
  CHECK(nearly(mitcad::gridSpacingInUnit(6.35, QStringLiteral("in")), 0.25));
  CHECK(nearly(mitcad::gridSpacingInUnit(609.6, QStringLiteral("ft")), 2.0));
  CHECK(nearly(mitcad::gridSpacingInUnit(25.0, QStringLiteral("cm")), 2.5));
  CHECK(nearly(mitcad::gridSpacingInUnit(10.0, QStringLiteral("mm")), 10.0));
  CHECK(nearly(mitcad::gridSpacingMillimetres(0.0625, QStringLiteral("in")), 1.5875));
  CHECK(nearly(mitcad::gridSpacingMillimetres(1.5, QStringLiteral("ft")), 457.2));
  CHECK(nearly(mitcad::gridSpacingMillimetres(0.002, QStringLiteral("m")), 2.0));
  // The dialog's decimals: 1/16 in shows whole.
  CHECK(mitcad::gridSpacingDecimals(QStringLiteral("mm")) == 2);
  CHECK(mitcad::gridSpacingDecimals(QStringLiteral("cm")) == 3);
  CHECK(mitcad::gridSpacingDecimals(QStringLiteral("m")) == 5);
  CHECK(mitcad::gridSpacingDecimals(QStringLiteral("in")) == 4);
  CHECK(mitcad::gridSpacingDecimals(QStringLiteral("ft")) == 4);
  // As the log names a spacing; a design in mm as before.
  EXPECT_TEXT(5.0, "mm", "5 mm");
  EXPECT_TEXT(0.1, "mm", "0.1 mm");
  EXPECT_TEXT(1e-5, "mm", "1e-05 mm");
  EXPECT_TEXT(5.0, "", "5 mm");
  EXPECT_TEXT(6.35, "in", "0.25 in");
  EXPECT_TEXT(0.0625 * 25.4, "in", "0.0625 in");
  EXPECT_TEXT(5.0 * 25.4, "in", "5 in");
  EXPECT_TEXT(609.6, "ft", "2 ft");
  EXPECT_TEXT(5.0, "cm", "0.5 cm");
}

} // namespace

int gridSpacingTests() {
  metricTests();
  inchTests();
  footTests();
  conversionTests();
  return failures;
}
