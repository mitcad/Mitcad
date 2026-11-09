// SPDX-License-Identifier: MIT
#include "ValueStep.hpp"

#include <algorithm>
#include <cmath>

#include <QCoreApplication>
#include <QRegularExpression>
#include <QRegularExpressionMatch>

#include "LengthUnits.hpp"

namespace mitcad {
namespace {

// A unit as the model's expressions spell it (core/expr/src/units.rs,
// UNIT_NAMES) and its size in mm or degrees.
struct UnitSize {
  const char* name; // UTF-8
  double size;
};

constexpr double kPi = 3.14159265358979323846;

const UnitSize kLengthUnits[] = {
    {"um", 0.001},    {"micron", 0.001}, {"µm", 0.001},  {"μm", 0.001},      {"mm", 1.0},
    {"cm", 10.0},     {"m", 1000.0},     {"km", 1.0e6},  {"in", 25.4},       {"inch", 25.4},
    {"ft", 304.8},    {"foot", 304.8},   {"yd", 914.4},  {"mi", 1609344.0},  {"mil", 0.0254},
};

const UnitSize kAngleUnits[] = {
    {"deg", 1.0},
    {"°", 1.0},
    {"rad", 180.0 / kPi},
    {"grad", 0.9},
};

// The size of the unit a value of the kind shows, in mm or degrees; none
// for a unit of another kind (or any unit after a plain number).
std::optional<double> unitSize(ParameterKind kind, const QString& unit, double bareUnit) {
  if (kind == ParameterKind::Text) {
    return std::nullopt;
  }
  if (unit.isEmpty()) {
    // A bare length is in the design's unit, a bare angle in degrees.
    return kind == ParameterKind::Length && bareUnit > 0.0 && std::isfinite(bareUnit) ? bareUnit : 1.0;
  }
  if (kind == ParameterKind::Length) {
    for (const UnitSize& known : kLengthUnits) {
      if (unit == QString::fromUtf8(known.name)) {
        return known.size;
      }
    }
  } else if (kind == ParameterKind::Angle) {
    for (const UnitSize& known : kAngleUnits) {
      if (unit == QString::fromUtf8(known.name)) {
        return known.size;
      }
    }
  }
  return std::nullopt;
}

// The decimals a step needs: 0 for 1 and 10, 1 for 0.1.
int decimalsOf(double step) {
  double size = std::abs(step);
  int decimals = 0;
  while (decimals < 6 && std::abs(size - std::round(size)) > 1e-9 * std::max(1.0, size)) {
    size *= 10.0;
    ++decimals;
  }
  return decimals;
}

} // namespace

double valueStepFactor(Qt::KeyboardModifiers modifiers, bool macKeys) {
  const Qt::KeyboardModifiers held = modifiers & ~Qt::KeyboardModifiers(Qt::KeypadModifier);
  const Qt::KeyboardModifier fine = macKeys ? Qt::AltModifier : Qt::ControlModifier;
  if (held == Qt::NoModifier) {
    return 1.0;
  }
  if (held == Qt::ShiftModifier) {
    return 10.0;
  }
  if (held == fine) {
    return 0.1;
  }
  return 0.0;
}

QString valueStepHint(bool macKeys) {
  return macKeys ? QCoreApplication::translate("ValueStep", "Up/Down steps the value; Shift ×10, Option ×0.1")
                 : QCoreApplication::translate("ValueStep", "Up/Down steps the value; Shift ×10, Ctrl ×0.1");
}

std::optional<QString> steppedValue(const QString& text, ParameterKind kind, double step,
                                    const ValueStepLimit& limit, double bareUnit) {
  if (kind == ParameterKind::Text || !std::isfinite(step) || step == 0.0) {
    return std::nullopt;
  }
  // An optional `name = ` (the field's own parameter, mitcad#231), a number
  // with an optional sign and a decimal point or comma, and at most one
  // word after it, which must be a unit of the kind: no operator, no
  // parameter, no exponent.
  static const QRegularExpression plain(
      QStringLiteral(R"(^\s*((?:[^\W\d]\w*\s*=(?!=)\s*)?)([-+]?(?:\d+(?:[.,]\d*)?|[.,]\d+))(\s*)([^\s\d.,+\-*/^()=;<>!%"']*)\s*$)"),
      QRegularExpression::UseUnicodePropertiesOption);
  const QRegularExpressionMatch match = plain.match(text);
  if (!match.hasMatch()) {
    return std::nullopt;
  }
  const QString naming = match.captured(1);
  QString number = match.captured(2);
  const QString space = match.captured(3);
  const QString unit = match.captured(4);
  const std::optional<double> size = unitSize(kind, unit, bareUnit);
  if (!size) {
    return std::nullopt;
  }
  number.replace(QLatin1Char(','), QLatin1Char('.'));
  bool ok = false;
  const double value = number.toDouble(&ok); // the C locale: points only
  if (!ok || !std::isfinite(value)) {
    return std::nullopt;
  }
  const qsizetype point = number.indexOf(QLatin1Char('.'));
  const int written = point < 0 ? 0 : static_cast<int>(number.size() - point - 1);
  int decimals = std::max(written, decimalsOf(step));
  double next = value + step;
  if (std::isfinite(limit.minimum) && step < 0.0) {
    // The minimum in the unit shown (1 for a count, 0 for a length).
    const double least = limit.minimum / *size;
    const double slack = 1e-9 * std::max(1.0, std::abs(least));
    if (limit.above ? next <= least + slack : next < least - slack) {
      if (limit.above || value <= least + slack) {
        return text; // stopped: at or below its minimum already
      }
      next = least;
      decimals = std::max(decimals, 6);
    }
  }
  // Without a unit the space is only trailing blank, which is dropped.
  return QString(naming + plainDecimal(next, decimals) + (unit.isEmpty() ? QString() : space) + unit);
}

ValueStepLimit sketchDimensionStepLimit(const QString& type, bool signedOffset) {
  ValueStepLimit limit;
  if (signedOffset || type == QLatin1String("angle")) {
    return limit; // ValueRange::Signed; an angle is left as typed
  }
  static const char* const kNotNegative[] = {"horizontal_distance",    "vertical_distance",
                                             "point_line_distance",    "line_distance",
                                             "tangent_distance",       "concentric_distance",
                                             "tangent_circle_distance", "tangent_point_distance"};
  limit.minimum = 0.0;
  limit.above = true; // ValueRange::Positive
  for (const char* name : kNotNegative) {
    if (type == QLatin1String(name)) {
      limit.above = false; // ValueRange::NotNegative
    }
  }
  return limit;
}

} // namespace mitcad
