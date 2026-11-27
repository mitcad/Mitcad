// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QJsonArray>
#include <QList>
#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad::drawings {

// A cut sheet of the design Nest Flat Patterns… may put the parts on.
struct NestCutSheet {
  QString uid;
  QString name;
  double width = 0.0;
  double height = 0.0;
  double kerf = 0.0;
};

// What Nest Flat Patterns… was asked (mitcad#2553): the cut sheet (a
// design's uid, or empty for a new one), its material and kerf, the
// spacing, and the bodies with their copies (`{"body", "count"}`).
struct NestChoice {
  QString cutSheet;
  double width = 0.0;
  double height = 0.0;
  double gap = 0.0;
  double kerf = 0.0;
  QJsonArray bodies;
  int count = 0;
};

// Make › Laser's Nest Flat Patterns… (mitcad#2553, app/COMMANDS.md "Nest
// Flat Patterns and the cutting program"): the sheet-metal parts of the
// `sheet_metal_list` query (`rows`) with a check and their copies (as
// many as the design places, a part whose Flat Pattern cannot be developed
// shown with why and not offered), those of `selected` (body uids) checked
// when any is, else all; the cut sheet (New Cut Sheet or one of
// `cutSheets`, whose material and kerf it shows), the material's width and
// height, the spacing and the kerf. Empty when cancelled. Logs "Nest Flat
// Patterns dialog opened: <n> part(s)".
std::optional<NestChoice> askNestFlatPatterns(QWidget* parent, const QJsonArray& rows,
                                              const QList<NestCutSheet>& cutSheets, const QStringList& selected);

} // namespace mitcad::drawings
