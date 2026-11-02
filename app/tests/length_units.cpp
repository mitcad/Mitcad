// SPDX-License-Identifier: MIT
// Lengths in the design's unit (mitcad#1165): a command's length default
// converted to a sensible value of mm, cm, m, in and ft (the table of nice
// values), what stays as written, and the lengths, areas, volumes and
// points Measure and the Browser show.
#include "framework/LengthUnits.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>

#include <QByteArray>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "length_units.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// A default's text in a unit, compared with what is expected (printed when
// it differs).
void expectDefault(const char* text, const char* unit, const char* expected, int line) {
  const QString got = mitcad::defaultLengthText(QString::fromUtf8(text), QString::fromUtf8(unit));
  if (got != QString::fromUtf8(expected)) {
    std::fprintf(stderr, "length_units.cpp:%d: \"%s\" in %s is \"%s\", not \"%s\"\n", line, text, unit,
                 got.toUtf8().constData(), expected);
    ++failures;
  }
}

#define EXPECT_DEFAULT(text, unit, expected) expectDefault(text, unit, expected, __LINE__)

bool near(double a, double b) { return std::abs(a - b) < 1e-9 * std::max(1.0, std::abs(b)); }

void sizeTests() {
  CHECK(near(mitcad::lengthUnitSize(QStringLiteral("mm")), 1.0));
  CHECK(near(mitcad::lengthUnitSize(QStringLiteral("cm")), 10.0));
  CHECK(near(mitcad::lengthUnitSize(QStringLiteral("m")), 1000.0));
  CHECK(near(mitcad::lengthUnitSize(QStringLiteral("in")), 25.4));
  CHECK(near(mitcad::lengthUnitSize(QStringLiteral("ft")), 304.8));
  CHECK(near(mitcad::lengthUnitSize(QStringLiteral("parsec")), 1.0));
}

// The nice values: metric exact, inches and feet from their tables,
// nearest by ratio.
void niceTests() {
  // Metric: exact.
  CHECK(near(mitcad::niceLength(10.0, QStringLiteral("mm")), 10.0));
  CHECK(near(mitcad::niceLength(10.0, QStringLiteral("cm")), 1.0));
  CHECK(near(mitcad::niceLength(1.75, QStringLiteral("cm")), 0.175));
  CHECK(near(mitcad::niceLength(2400.0, QStringLiteral("m")), 2.4));
  // Inches: the ticket's examples and a few more of the table.
  CHECK(near(mitcad::niceLength(10.0, QStringLiteral("in")), 0.5));
  CHECK(near(mitcad::niceLength(2.0, QStringLiteral("in")), 0.1));
  CHECK(near(mitcad::niceLength(1.0, QStringLiteral("in")), 0.05));
  CHECK(near(mitcad::niceLength(3.0, QStringLiteral("in")), 0.125));
  CHECK(near(mitcad::niceLength(6.0, QStringLiteral("in")), 0.25));
  CHECK(near(mitcad::niceLength(20.0, QStringLiteral("in")), 0.75));
  CHECK(near(mitcad::niceLength(25.4, QStringLiteral("in")), 1.0));
  CHECK(near(mitcad::niceLength(50.0, QStringLiteral("in")), 2.0));
  CHECK(near(mitcad::niceLength(100.0, QStringLiteral("in")), 4.0));
  CHECK(near(mitcad::niceLength(400.0, QStringLiteral("in")), 16.0));
  CHECK(near(mitcad::niceLength(600.0, QStringLiteral("in")), 24.0));
  CHECK(near(mitcad::niceLength(2400.0, QStringLiteral("in")), 96.0));
  CHECK(near(mitcad::niceLength(0.01, QStringLiteral("in")), 0.0005));
  CHECK(near(mitcad::niceLength(-5.0, QStringLiteral("in")), -0.2));
  // Beyond the table: two significant digits.
  CHECK(near(mitcad::niceLength(10000.0, QStringLiteral("in")), 390.0));
  // Feet: 1, 2, 2.5, 5 a decade below a foot, finer steps from a foot up.
  CHECK(near(mitcad::niceLength(10.0, QStringLiteral("ft")), 0.025));
  CHECK(near(mitcad::niceLength(1.0, QStringLiteral("ft")), 0.0025));
  CHECK(near(mitcad::niceLength(304.8, QStringLiteral("ft")), 1.0));
  CHECK(near(mitcad::niceLength(400.0, QStringLiteral("ft")), 1.5));
  CHECK(near(mitcad::niceLength(2400.0, QStringLiteral("ft")), 8.0));
  CHECK(near(mitcad::niceLength(0.0, QStringLiteral("ft")), 0.0));
}

// The default texts: converted where a panel's field gets them.
void defaultTests() {
  // A design in mm keeps every default as written.
  EXPECT_DEFAULT("10 mm", "mm", "10 mm");
  EXPECT_DEFAULT("2", "mm", "2");
  EXPECT_DEFAULT("0 mm", "mm", "0 mm");
  // cm and m: the exact value.
  EXPECT_DEFAULT("10 mm", "cm", "1 cm");
  EXPECT_DEFAULT("2", "cm", "0.2 cm");
  EXPECT_DEFAULT("1.75 mm", "cm", "0.175 cm");
  EXPECT_DEFAULT("2400 mm", "m", "2.4 m");
  EXPECT_DEFAULT("0.01 mm", "m", "0.00001 m");
  // in: the nice values (Extrude's 10 mm, a chamfer's bare 2, Sheet
  // Rules' 1 mm, BUILD's 400 mm spacing).
  EXPECT_DEFAULT("10 mm", "in", "0.5 in");
  EXPECT_DEFAULT("10mm", "in", "0.5 in");
  EXPECT_DEFAULT("2", "in", "0.1 in");
  EXPECT_DEFAULT("1 mm", "in", "0.05 in");
  EXPECT_DEFAULT("-5 mm", "in", "-0.2 in");
  EXPECT_DEFAULT("400 mm", "in", "16 in");
  EXPECT_DEFAULT("2400 mm", "in", "96 in");
  EXPECT_DEFAULT("0 mm", "in", "0 in");
  // ft.
  EXPECT_DEFAULT("2400 mm", "ft", "8 ft");
  EXPECT_DEFAULT("10 mm", "ft", "0.025 ft");
  // Left as written: a bare 0, an empty text, an expression, another unit,
  // an angle's text, a unit not in the table.
  EXPECT_DEFAULT("0", "in", "0");
  EXPECT_DEFAULT("", "in", "");
  EXPECT_DEFAULT("d1 * 2", "in", "d1 * 2");
  EXPECT_DEFAULT("1 in", "cm", "1 in");
  EXPECT_DEFAULT("2 cm", "in", "2 cm");
  EXPECT_DEFAULT("45 deg", "in", "45 deg");
  EXPECT_DEFAULT("10 mm", "parsec", "10 mm");
}

// What Measure and the Browser show: as before in mm, in the unit else.
void shownTests() {
  CHECK(mitcad::lengthInUnit(10.0, QStringLiteral("mm")) == QStringLiteral("10 mm"));
  CHECK(mitcad::lengthInUnit(12.34567, QStringLiteral("mm")) == QStringLiteral("12.346 mm"));
  CHECK(mitcad::lengthInUnit(-0.0001, QStringLiteral("mm")) == QStringLiteral("0 mm"));
  CHECK(mitcad::lengthInUnit(1.27, QStringLiteral("in")) == QStringLiteral("0.05 in"));
  CHECK(mitcad::lengthInUnit(25.4, QStringLiteral("in")) == QStringLiteral("1 in"));
  CHECK(mitcad::lengthInUnit(10.0, QStringLiteral("in")) == QStringLiteral("0.3937 in"));
  CHECK(mitcad::lengthInUnit(15.0, QStringLiteral("cm")) == QStringLiteral("1.5 cm"));
  CHECK(mitcad::lengthInUnit(304.8, QStringLiteral("ft")) == QStringLiteral("1 ft"));
  CHECK(mitcad::lengthInUnit(5.0, QString()) == QStringLiteral("5 mm"));
  CHECK(mitcad::areaInUnit(628.31853, QStringLiteral("mm")) == QString::fromUtf8("628.319 mm²"));
  CHECK(mitcad::areaInUnit(1290.32, QStringLiteral("in")) == QString::fromUtf8("2 in²"));
  CHECK(mitcad::areaInUnit(100.0, QStringLiteral("cm")) == QString::fromUtf8("1 cm²"));
  CHECK(mitcad::volumeInUnit(48000.0, QStringLiteral("mm")) == QString::fromUtf8("48000 mm³"));
  CHECK(mitcad::volumeInUnit(16387.064, QStringLiteral("in")) == QString::fromUtf8("1 in³"));
  CHECK(mitcad::coordinateInUnit(50.8, QStringLiteral("in")) == QStringLiteral("2"));
  CHECK(mitcad::coordinateInUnit(30.0, QStringLiteral("mm")) == QStringLiteral("30"));
  CHECK(mitcad::plainDecimal(-0.0, 3) == QStringLiteral("0"));
  CHECK(mitcad::plainDecimal(2.5, 3) == QStringLiteral("2.5"));
}

} // namespace

int lengthUnitsTests() {
  sizeTests();
  niceTests();
  defaultTests();
  shownTests();
  return failures;
}
