// SPDX-License-Identifier: MIT
#include "GridSpacing.hpp"

#include <algorithm>
#include <cmath>
#include <initializer_list>

#include "../framework/LengthUnits.hpp"

namespace mitcad {
namespace {

// The densest lines on screen (pixels apart); the next coarser step is
// chosen before lines get further apart than this.
constexpr double kMaxPixels = 35.0;
constexpr double kInch = 25.4;
constexpr double kFoot = 12.0; // inches

// The largest of 1, 2 or 5 times a power of ten at most `limit` (>= 1).
double decadeStep(double limit) {
  const double power = std::pow(10.0, std::floor(std::log10(limit)));
  for (const double mantissa : {5.0, 2.0}) {
    if (mantissa * power <= limit) {
      return mantissa * power;
    }
  }
  return power;
}

// The inch series' largest value at most `limit` inches: halves below an
// inch, 1, 2, 5 times a power of ten from an inch up.
double inchStep(double limit) {
  if (limit >= 1.0) {
    return decadeStep(limit);
  }
  return std::pow(2.0, std::floor(std::log2(limit)));
}

} // namespace

bool imperialGrid(const QString& unit) { return unit == QLatin1String("in") || unit == QLatin1String("ft"); }

double layoutGridStep(double pixelsPerMm, const QString& unit) {
  const bool imperial = imperialGrid(unit);
  if (!(pixelsPerMm > 0.0) || !std::isfinite(pixelsPerMm)) {
    return imperial ? kInch : 10.0;
  }
  const double limit = kMaxPixels / pixelsPerMm; // mm
  if (!imperial) {
    // 1 or 5 times a power of ten: no 2, so the major lines (five steps)
    // fall on the next step's lines.
    const double power = std::pow(10.0, std::floor(std::log10(limit)));
    return 5.0 * power <= limit ? 5.0 * power : power;
  }
  const double inches = limit / kInch;
  if (unit == QLatin1String("ft") && inches >= kFoot) {
    return decadeStep(inches / kFoot) * kFoot * kInch;
  }
  return inchStep(inches) * kInch;
}

int layoutGridMajor(double step, const QString& unit) {
  if (!imperialGrid(unit) || !(step > 0.0) || !std::isfinite(step)) {
    return kLayoutGridMajor;
  }
  const double inches = step / kInch;
  if (inches < 1.0 - 1e-9) {
    return 4; // a quarter, a half, an inch or two
  }
  const double value = unit == QLatin1String("ft") && inches >= kFoot - 1e-9 ? inches / kFoot : inches;
  const double mantissa = value / std::pow(10.0, std::floor(std::log10(value) + 1e-9));
  return std::abs(mantissa - 5.0) < 1e-6 ? 4 : 5;
}

double gridSpacingInUnit(double millimetres, const QString& unit) { return millimetres / lengthUnitSize(unit); }

double gridSpacingMillimetres(double value, const QString& unit) { return value * lengthUnitSize(unit); }

int gridSpacingDecimals(const QString& unit) {
  if (imperialGrid(unit)) {
    return 4;
  }
  const double size = lengthUnitSize(unit);
  return std::clamp(2 + static_cast<int>(std::lround(std::log10(size))), 2, 6);
}

QString gridSpacingText(double millimetres, const QString& unit) {
  const QString name = unit.isEmpty() ? QStringLiteral("mm") : unit;
  return QString::number(gridSpacingInUnit(millimetres, name), 'g', 9) + QLatin1Char(' ') + name;
}

} // namespace mitcad
