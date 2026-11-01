// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QString>
#include <QStringList>

namespace mitcad::sketch {

// The Sketch Palette's Create section (mitcad#1027): two rows of the
// sketch tools the user starts most, drawing and modifying. Plain Qt Core.

// The buttons of the section: two rows of seven, as the constraints'.
constexpr int kPaletteCreateColumns = 7;
constexpr int kPaletteCreateCount = 2 * kPaletteCreateColumns;

// The tools before any is counted: Line, Rectangle, Circle, Arc, Polygon,
// Slot, Spline, Point, Trim, Extend, Offset, Fillet, Mirror, Dimension.
const QStringList& paletteCreateDefaults();

struct PaletteCreateChoice {
  QStringList ids;         // in the ribbon's order
  bool fromCounts = false; // a counted tool chose them; false: the defaults
};

// The Create section's tools: the commands pinned on the Sketch tab first
// (in their order), then the most started ones (by count, a tie in the
// ribbon's order), then the defaults, at most `limit`, each only when it is
// in `ribbon` (the tools that can be there, in the ribbon's order: Create,
// then Modify). The choice is shown in the ribbon's order, not by count.
PaletteCreateChoice choosePaletteCreateTools(const QStringList& ribbon, const QStringList& pinned,
                                             const QHash<QString, int>& counts,
                                             int limit = kPaletteCreateCount);

// How often each tool has started in a sketch: the local settings,
// "sketchToolCounts/<command id>" (sketchToolCounts/sketch.line=12).
QString paletteToolCountsGroup();
QHash<QString, int> paletteToolCounts();
// One more start of the tool, saved at once.
void countPaletteTool(const QString& id);

// "sketch.rectangle_center" -> "rectangle_center", for the log lines.
QString paletteToolName(const QString& id);

} // namespace mitcad::sketch
