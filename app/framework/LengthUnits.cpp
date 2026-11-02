// SPDX-License-Identifier: MIT
#include "LengthUnits.hpp"

#include <algorithm>
#include <cmath>
#include <iterator>
#include <vector>

#include <QChar>
#include <QRegularExpression>

namespace mitcad {
namespace {

struct UnitSize {
  const char* name;
  double millimetres;
};

const UnitSize kUnits[] = {{"mm", 1.0},   {"cm", 10.0},    {"m", 1000.0},     {"um", 0.001},
                           {"in", 25.4},  {"ft", 304.8},   {"mil", 0.0254},   {"yd", 914.4}};

bool knownUnit(const QString& unit) {
  return std::any_of(std::begin(kUnits), std::end(kUnits),
                     [&unit](const UnitSize& size) { return unit == QLatin1String(size.name); });
}

bool metric(const QString& unit) {
  return unit == QLatin1String("mm") || unit == QLatin1String("cm") || unit == QLatin1String("m") ||
         unit == QLatin1String("um");
}

// Values from 10^first to 10^last decades: `below` mantissas under 1 unit,
// `above` from 1 up.
std::vector<double> decades(int first, int last, const std::vector<double>& below, const std::vector<double>& above) {
  std::vector<double> values;
  for (int exponent = first; exponent <= last; ++exponent) {
    const double scale = std::pow(10.0, exponent);
    for (const double mantissa : exponent < 0 ? below : above) {
      values.push_back(mantissa * scale);
    }
  }
  return values;
}

// The usual sizes of an imperial unit, ascending.
const std::vector<double>& niceValues(const QString& unit) {
  static const std::vector<double> inches = {
      0.0001, 0.0002, 0.0005, 0.001, 0.002, 0.005, 0.01, 0.02, 0.025, 0.05, 0.1, 0.125, 0.2, 0.25,
      0.5,    0.75,   1,      1.25,  1.5,   2,     2.5,  3,    4,     5,    6,   8,     10,  12,
      16,     18,     20,     24,    30,    36,    40,   48,   60,    72,   96,  120,   144, 192, 240};
  static const std::vector<double> coarse = {1, 2, 2.5, 5};
  static const std::vector<double> fine = {1, 1.5, 2, 2.5, 3, 4, 5, 6, 8};
  static const std::vector<double> feet = decades(-4, 3, coarse, fine);
  static const std::vector<double> mils = decades(-1, 5, fine, fine);
  if (unit == QLatin1String("in")) {
    return inches;
  }
  if (unit == QLatin1String("mil")) {
    return mils;
  }
  return feet; // ft and yd
}

// Two significant digits.
double twoDigits(double value) {
  if (value <= 0.0) {
    return value;
  }
  const double scale = std::pow(10.0, std::floor(std::log10(value)) - 1.0);
  return std::round(value / scale) * scale;
}

// Decimals for a quantity of `power` (1 length, 2 area, 3 volume) in a
// unit: 3 for mm, one more per decade of the unit's size.
int decimalsFor(const QString& unit, int power) {
  const double steps = std::log10(lengthUnitSize(unit)) * power;
  return std::clamp(3 + static_cast<int>(std::lround(steps)), 0, 12);
}

QString unitName(const QString& unit) { return unit.isEmpty() ? QStringLiteral("mm") : unit; }

} // namespace

double lengthUnitSize(const QString& unit) {
  for (const UnitSize& size : kUnits) {
    if (unit == QLatin1String(size.name)) {
      return size.millimetres;
    }
  }
  return 1.0;
}

QString plainDecimal(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

double niceLength(double millimetres, const QString& unit) {
  const double exact = millimetres / lengthUnitSize(unit);
  if (metric(unit) || !knownUnit(unit) || exact == 0.0 || !std::isfinite(exact)) {
    return exact;
  }
  const double size = std::abs(exact);
  const std::vector<double>& values = niceValues(unit);
  double nice = 0.0;
  if (size < values.front() * 0.5 || size > values.back() * 1.5) {
    nice = twoDigits(size);
  } else {
    // The nearest by ratio: the one of the two around it whose ratio to it
    // is the smaller.
    const auto above = std::lower_bound(values.begin(), values.end(), size);
    if (above == values.end()) {
      nice = values.back();
    } else if (above == values.begin()) {
      nice = values.front();
    } else {
      const double upper = *above;
      const double lower = *(above - 1);
      nice = upper / size < size / lower ? upper : lower;
    }
  }
  return exact < 0.0 ? -nice : nice;
}

QString defaultLengthText(const QString& text, const QString& unit) {
  if (unit == QLatin1String("mm") || !knownUnit(unit)) {
    return text;
  }
  static const QRegularExpression number(QStringLiteral(R"(^\s*([+-]?(?:\d+(?:\.\d*)?|\.\d+))\s*(mm)?\s*$)"));
  const QRegularExpressionMatch match = number.match(text);
  if (!match.hasMatch()) {
    return text;
  }
  bool ok = false;
  const double millimetres = match.captured(1).toDouble(&ok);
  if (!ok || (millimetres == 0.0 && match.captured(2).isEmpty())) {
    return text;
  }
  return plainDecimal(niceLength(millimetres, unit), 9) + QLatin1Char(' ') + unit;
}

QString lengthInUnit(double millimetres, const QString& unit) {
  return coordinateInUnit(millimetres, unit) + QLatin1Char(' ') + unitName(unit);
}

QString areaInUnit(double squareMillimetres, const QString& unit) {
  const double size = lengthUnitSize(unit);
  return plainDecimal(squareMillimetres / (size * size), decimalsFor(unit, 2)) + QLatin1Char(' ') + unitName(unit) +
         QChar(0x00B2);
}

QString volumeInUnit(double cubicMillimetres, const QString& unit) {
  const double size = lengthUnitSize(unit);
  return plainDecimal(cubicMillimetres / (size * size * size), decimalsFor(unit, 3)) + QLatin1Char(' ') +
         unitName(unit) + QChar(0x00B3);
}

QString coordinateInUnit(double millimetres, const QString& unit) {
  return plainDecimal(millimetres / lengthUnitSize(unit), decimalsFor(unit, 1));
}

} // namespace mitcad
