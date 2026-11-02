// SPDX-License-Identifier: MIT
#pragma once

#include <QString>

namespace mitcad {

// Lengths in the design's unit (mitcad#1165, app/COMMANDS.md "Length
// defaults in the design's unit"): a command's length default converted to
// a sensible value of the unit, and the lengths, areas and volumes the
// Browser and Measure show. Plain Qt Core, without the model: the callers
// pass the design's length unit ("mm", "cm", "m", "in", "ft", ...).

// The size of a length unit in mm: mm, cm, m, um, in, ft, mil, yd; 1 for
// any other text.
double lengthUnitSize(const QString& unit);

// A number with at most `decimals` decimals, without trailing zeros, "-0"
// as "0".
QString plainDecimal(double value, int decimals);

// `millimetres` as a sensible value of `unit`, in that unit. The metric
// units take the exact value (10 mm is 1 cm); in, ft, yd and mil the
// nearest value of a small table of the unit's usual sizes (in: 0.05,
// 0.1, 0.125, 0.25, 0.5, 0.75, 1, 1.5, 2, ..., 12, 16, 24, ..., 96; ft:
// 1, 2, 2.5 and 5 in each decade below a foot, 1, 1.5, 2, 2.5, 3, 4, 5, 6
// and 8 from a foot up), nearest by ratio (10 mm is 0.5 in, 2 mm 0.1 in, 1
// mm 0.05 in, 400 mm 16 in). Beyond the table, two significant digits.
// The sign is kept.
double niceLength(double millimetres, const QString& unit);

// A command's length default in the design's unit (InputDef::defaultText,
// applied where a panel's field gets it): a number in mm ("10 mm") or a
// bare number, which commands write in mm too ("2"), becomes the nice
// value of the unit with the unit ("0.5 in"). Unchanged: a design in mm
// (or a unit not in the table), an empty text, an expression ("d1 * 2"),
// a number in another unit ("1 in"), a bare 0.
QString defaultLengthText(const QString& text, const QString& unit);

// A length, an area and a volume in mm, mm² and mm³ as Measure and the
// Browser show them in the design's unit: "12.7 mm", "0.5 in", "2 in²",
// "1 in³". The decimals follow the unit's size: 3 for mm, 4 for cm and in,
// more for larger units (a mm design shows what it showed before).
QString lengthInUnit(double millimetres, const QString& unit);
QString areaInUnit(double squareMillimetres, const QString& unit);
QString volumeInUnit(double cubicMillimetres, const QString& unit);
// A coordinate in the design's unit without the unit ("0.5"), for points.
QString coordinateInUnit(double millimetres, const QString& unit);

} // namespace mitcad
