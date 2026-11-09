// SPDX-License-Identifier: MIT
// Stepping a value field's number with Up and Down and the wheel
// (mitcad#1254): the step in the unit the field shows (mm, in, degrees, a
// plain number), the Shift and fine modifiers (Ctrl, Option on macOS), the
// minimum, and the texts left alone (an expression, a parameter's name, a
// unit of another kind).
#include "framework/ValueStep.hpp"

#include <cmath>
#include <cstdio>
#include <optional>

#include <QByteArray>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "value_step.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::ParameterKind;
using mitcad::ValueStepLimit;

// `text` stepped by `step` is `expected` ("-" for not stepped at all).
void expectStep(const char* text, ParameterKind kind, double step, const char* expected, int line,
                const ValueStepLimit& limit = {}, double bareUnit = 1.0) {
  const std::optional<QString> got = mitcad::steppedValue(QString::fromUtf8(text), kind, step, limit, bareUnit);
  const QString shown = got ? *got : QStringLiteral("-");
  if (shown != QString::fromUtf8(expected)) {
    std::fprintf(stderr, "value_step.cpp:%d: \"%s\" stepped by %g is \"%s\", not \"%s\"\n", line, text, step,
                 shown.toUtf8().constData(), expected);
    ++failures;
  }
}

#define EXPECT_STEP(text, kind, step, expected) expectStep(text, kind, step, expected, __LINE__)
#define EXPECT_LIMITED(text, kind, step, limit, expected) expectStep(text, kind, step, expected, __LINE__, limit)

void factorTests() {
  // Windows and Linux: Ctrl is the fine step.
  CHECK(mitcad::valueStepFactor(Qt::NoModifier, false) == 1.0);
  CHECK(mitcad::valueStepFactor(Qt::ShiftModifier, false) == 10.0);
  CHECK(mitcad::valueStepFactor(Qt::ControlModifier, false) == 0.1);
  CHECK(mitcad::valueStepFactor(Qt::KeypadModifier, false) == 1.0);
  CHECK(mitcad::valueStepFactor(Qt::ShiftModifier | Qt::KeypadModifier, false) == 10.0);
  CHECK(mitcad::valueStepFactor(Qt::AltModifier, false) == 0.0);
  CHECK(mitcad::valueStepFactor(Qt::MetaModifier, false) == 0.0);
  CHECK(mitcad::valueStepFactor(Qt::ShiftModifier | Qt::ControlModifier, false) == 0.0);
  // macOS: Option is the fine step; Cmd (reported as Ctrl) and Control
  // (Meta) are the system's.
  CHECK(mitcad::valueStepFactor(Qt::NoModifier, true) == 1.0);
  CHECK(mitcad::valueStepFactor(Qt::ShiftModifier, true) == 10.0);
  CHECK(mitcad::valueStepFactor(Qt::AltModifier, true) == 0.1);
  CHECK(mitcad::valueStepFactor(Qt::ControlModifier, true) == 0.0);
  CHECK(mitcad::valueStepFactor(Qt::MetaModifier, true) == 0.0);
  // The tooltip's line.
  CHECK(mitcad::valueStepHint(false) == QString::fromUtf8("Up/Down steps the value; Shift ×10, Ctrl ×0.1"));
  CHECK(mitcad::valueStepHint(true) == QString::fromUtf8("Up/Down steps the value; Shift ×10, Option ×0.1"));
}

void unitTests() {
  // A count: by 1, 10 with Shift, 0.1 with the fine modifier.
  EXPECT_STEP("4", ParameterKind::Unitless, 1.0, "5");
  EXPECT_STEP("4", ParameterKind::Unitless, -1.0, "3");
  EXPECT_STEP("4", ParameterKind::Unitless, 10.0, "14");
  EXPECT_STEP("4", ParameterKind::Unitless, 0.1, "4.1");
  EXPECT_STEP(" 4 ", ParameterKind::Unitless, 1.0, "5");
  // Lengths in the unit shown: mm, inches (an inch design's 0.5 in), cm.
  EXPECT_STEP("10 mm", ParameterKind::Length, 1.0, "11 mm");
  EXPECT_STEP("10 mm", ParameterKind::Length, 10.0, "20 mm");
  EXPECT_STEP("10 mm", ParameterKind::Length, -0.1, "9.9 mm");
  EXPECT_STEP("10mm", ParameterKind::Length, 1.0, "11mm");
  EXPECT_STEP("0.5 in", ParameterKind::Length, 1.0, "1.5 in");
  EXPECT_STEP("0.5 in", ParameterKind::Length, 10.0, "10.5 in");
  EXPECT_STEP("0.5 in", ParameterKind::Length, -0.1, "0.4 in");
  EXPECT_STEP("2 inch", ParameterKind::Length, 1.0, "3 inch");
  EXPECT_STEP("1.25 cm", ParameterKind::Length, 0.1, "1.35 cm");
  EXPECT_STEP("2.50 mm", ParameterKind::Length, 1.0, "3.5 mm");
  // No rounding noise: 0.2 + 0.1 is 0.3.
  EXPECT_STEP("0.2 mm", ParameterKind::Length, 0.1, "0.3 mm");
  // A decimal comma, a sign; across zero without a limit.
  EXPECT_STEP("12,5 mm", ParameterKind::Length, 1.0, "13.5 mm");
  EXPECT_STEP("-8 mm", ParameterKind::Length, 10.0, "2 mm");
  EXPECT_STEP("+3 mm", ParameterKind::Length, 1.0, "4 mm");
  EXPECT_STEP("0.5 mm", ParameterKind::Length, -1.0, "-0.5 mm");
  EXPECT_STEP("1 mm", ParameterKind::Length, -1.0, "0 mm");
  // A bare number keeps having no unit (it is in the design's).
  EXPECT_STEP("15", ParameterKind::Length, 1.0, "16");
  // Angles: deg, the degree sign, radians.
  EXPECT_STEP("360 deg", ParameterKind::Angle, 1.0, "361 deg");
  EXPECT_STEP("360 deg", ParameterKind::Angle, 0.1, "360.1 deg");
  EXPECT_STEP("45°", ParameterKind::Angle, -10.0, "35°");
  EXPECT_STEP("1 rad", ParameterKind::Angle, 0.1, "1.1 rad");
  // The field's own parameter named in it (mitcad#231) keeps its name.
  EXPECT_STEP("w = 10 mm", ParameterKind::Length, 1.0, "w = 11 mm");
}

void minimumTests() {
  // A count of at least 1: a step down stops there.
  const ValueStepLimit count{1.0, false};
  EXPECT_LIMITED("2", ParameterKind::Unitless, -1.0, count, "1");
  EXPECT_LIMITED("1", ParameterKind::Unitless, -1.0, count, "1");
  EXPECT_LIMITED("3", ParameterKind::Unitless, -10.0, count, "1");
  EXPECT_LIMITED("1", ParameterKind::Unitless, 1.0, count, "2");
  // From below the minimum a step up is taken, a step down is not.
  EXPECT_LIMITED("0", ParameterKind::Unitless, 1.0, count, "1");
  EXPECT_LIMITED("0", ParameterKind::Unitless, -1.0, count, "0");
  // A positive length: Down from 0.5 mm stays, the fine step goes on.
  const ValueStepLimit positive{0.0, true};
  EXPECT_LIMITED("0.5 mm", ParameterKind::Length, -1.0, positive, "0.5 mm");
  EXPECT_LIMITED("0.5 mm", ParameterKind::Length, -0.1, positive, "0.4 mm");
  EXPECT_LIMITED("0.1 mm", ParameterKind::Length, -0.1, positive, "0.1 mm");
  EXPECT_LIMITED("5 mm", ParameterKind::Length, -1.0, positive, "4 mm");
  EXPECT_LIMITED("0.5 in", ParameterKind::Length, -1.0, positive, "0.5 in");
  // An inclusive minimum in mm read in the unit shown: 25.4 mm is 1 in,
  // and a bare number is in the design's unit (inches here).
  const ValueStepLimit inch{25.4, false};
  EXPECT_LIMITED("3 in", ParameterKind::Length, -10.0, inch, "1 in");
  EXPECT_LIMITED("30 mm", ParameterKind::Length, -10.0, inch, "25.4 mm");
  expectStep("3", ParameterKind::Length, -10.0, "1", __LINE__, inch, 25.4);
}

// The sketch dimension editor's limit by the dimension's kind, as the
// sketch solver checks it (core/sketch/src/lib.rs, DimensionKind::range).
void sketchDimensionTests() {
  using mitcad::sketchDimensionStepLimit;
  for (const char* type : {"distance", "length", "radius", "diameter", "arc_length", "major_radius"}) {
    const ValueStepLimit limit = sketchDimensionStepLimit(QString::fromLatin1(type), false);
    CHECK(limit.minimum == 0.0 && limit.above);
  }
  for (const char* type : {"horizontal_distance", "vertical_distance", "point_line_distance", "line_distance",
                           "tangent_distance", "concentric_distance", "tangent_circle_distance",
                           "tangent_point_distance"}) {
    const ValueStepLimit limit = sketchDimensionStepLimit(QString::fromLatin1(type), false);
    CHECK(limit.minimum == 0.0 && !limit.above);
  }
  // A signed horizontal distance and an angle are not limited.
  CHECK(std::isinf(sketchDimensionStepLimit(QStringLiteral("horizontal_distance"), true).minimum));
  CHECK(std::isinf(sketchDimensionStepLimit(QStringLiteral("angle"), false).minimum));
  // A radius of 0.5 stays on Down; a distance between lines goes to 0.
  EXPECT_LIMITED("0.5", ParameterKind::Length, -1.0, sketchDimensionStepLimit(QStringLiteral("radius"), false),
                 "0.5");
  EXPECT_LIMITED("0.5", ParameterKind::Length, -1.0,
                 sketchDimensionStepLimit(QStringLiteral("line_distance"), false), "0");
  EXPECT_LIMITED("0.5", ParameterKind::Length, -1.0,
                 sketchDimensionStepLimit(QStringLiteral("horizontal_distance"), true), "-0.5");
}

// Non-negative fields (a shell's thickness, a stitch tolerance): 0 is the
// last step down; a positive one keeps the fine step above 0.
void zeroTests() {
  const ValueStepLimit notNegative{0.0, false};
  EXPECT_LIMITED("0.5 mm", ParameterKind::Length, -1.0, notNegative, "0 mm");
  EXPECT_LIMITED("0 mm", ParameterKind::Length, -1.0, notNegative, "0 mm");
  EXPECT_LIMITED("0 mm", ParameterKind::Length, 1.0, notNegative, "1 mm");
  const ValueStepLimit positive{0.0, true};
  EXPECT_LIMITED("10 mm", ParameterKind::Length, -10.0, positive, "10 mm");
  EXPECT_LIMITED("10 mm", ParameterKind::Length, -1.0, positive, "9 mm");
  EXPECT_LIMITED("45 deg", ParameterKind::Angle, -10.0, positive, "35 deg");
  EXPECT_LIMITED("5 deg", ParameterKind::Angle, -10.0, positive, "5 deg");
  EXPECT_LIMITED("3", ParameterKind::Unitless, -10.0, positive, "3");
  EXPECT_LIMITED("0.3", ParameterKind::Unitless, -0.1, positive, "0.2");
}

void leftAloneTests() {
  // An expression or a parameter's name is not stepped.
  EXPECT_STEP("d1*2", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("d1 * 2", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("d1", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("2 d1", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("10 mm + 2 mm", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("-(4 mm)", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("1e3 mm", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("Screws.Size", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("w = d1", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("mm", ParameterKind::Length, 1.0, "-");
  // A unit of another kind, or any unit after a count.
  EXPECT_STEP("10 deg", ParameterKind::Length, 1.0, "-");
  EXPECT_STEP("2 in", ParameterKind::Angle, 1.0, "-");
  EXPECT_STEP("4 mm", ParameterKind::Unitless, 1.0, "-");
  // A text field.
  EXPECT_STEP("4", ParameterKind::Text, 1.0, "-");
}

} // namespace

int valueStepTests() {
  factorTests();
  unitTests();
  minimumTests();
  leftAloneTests();
  sketchDimensionTests();
  zeroTests();
  return failures;
}
