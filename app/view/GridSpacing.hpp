// SPDX-License-Identifier: MIT
#pragma once

// The layout grid's spacing in the design's length unit (mitcad#1210,
// app/COMMANDS.md "Layout grid"): the automatic spacing's series per unit,
// its major lines, and a spacing converted between millimetres (what the
// view settings keep) and the unit Grid Settings shows. Plain Qt Core and
// <cmath>, without OCCT: OcctViewer and ViewController call it, the unit
// tests (tests/grid_spacing.cpp) too.

#include <QString>

namespace mitcad {

// Every this many minor lines of a metric grid is a major line.
constexpr int kLayoutGridMajor = 5;

// Whether the automatic grid of a design in `unit` steps through inches:
// in and ft; every other unit keeps the metric series.
bool imperialGrid(const QString& unit);

// The layout grid's spacing (mm) when it follows the zoom, at `pixelsPerMm`,
// for a design in `unit`: the largest value of the unit's series whose
// lines are at most 35 pixels apart.
//   - metric (mm, cm, m, any other unit): 1 or 5 times a power of ten mm
//     (no 2, so that the major lines, five steps, fall on the next step's
//     lines);
//   - in: 1/16, 1/8, 1/4, 1/2 in (halving further down), then 1, 2 and 5
//     times a power of ten inches (1, 2, 5, 10, 20, ... in);
//   - ft: the inch series up to 10 in, then 1, 2 and 5 times a power of ten
//     feet (1, 2, 5, 10, ... ft).
// A scale that is no number or not positive gives 10 mm (metric) or 1 in.
double layoutGridStep(double pixelsPerMm, const QString& unit = QStringLiteral("mm"));

// Every how many minor lines a major one is drawn for a step (mm) of a
// design in `unit`: 5 on a metric grid; on an imperial one 4 below an inch
// (1/16 to 1/4 in, 1/8 to 1/2 in, 1/4 to 1 in, 1/2 to 2 in), 5 from an inch
// up (1 to 5, 2 to 10), 4 for a step of 5 times a power of ten (5 to 20).
int layoutGridMajor(double step, const QString& unit);

// A spacing in mm as a value of `unit`, and a value of `unit` in mm (the
// unit's size: mm, cm, m, in, ft, ...; 1 for an unknown unit).
double gridSpacingInUnit(double millimetres, const QString& unit);
double gridSpacingMillimetres(double value, const QString& unit);

// The decimals Grid Settings' fixed spacing shows in `unit`: 2 for mm, one
// more per decade of a metric unit's size, 4 for in and ft (1/16 in is
// 0.0625 in).
int gridSpacingDecimals(const QString& unit);

// A spacing (mm) as the log names it in `unit`: "5 mm", "0.25 in",
// "2 ft" (nine significant digits, no trailing zeros; an empty unit is mm).
QString gridSpacingText(double millimetres, const QString& unit);

} // namespace mitcad
