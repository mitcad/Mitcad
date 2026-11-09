// SPDX-License-Identifier: MIT
// Chains of sketch curves: Offset's Chain Selection and a double-click on
// a curve (mitcad#1541, app/COMMANDS.md "Chain selection by a
// double-click").
#pragma once

#include <QString>
#include <QStringList>

namespace mitcad::sketch {

class SketchModel;

// The curves joined to the picked ones end to end, as a chain selection
// takes them: through points where exactly two curves of the same
// construction kind meet (lines, arcs, elliptical arcs and open splines;
// a closed curve is a chain of its own). A picked curve that joins
// nothing while a curve it lies on does is replaced by that curve
// (mitcad#741).
QStringList chainOf(const SketchModel& model, const QStringList& picked);

// What a double-click on `curve` selects: `loop`, the smallest closed loop
// the model's `sketch_loop` query found for it, when there is one, else
// the open chain through points where exactly two curves meet (chainOf).
QStringList doubleClickChain(const SketchModel& model, const QString& curve, const QStringList& loop);

} // namespace mitcad::sketch
