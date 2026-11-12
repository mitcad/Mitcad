// SPDX-License-Identifier: MIT
#pragma once

// File > Export Drawing (mitcad#471): the dialog of a drawing's file and
// choices, which it starts with from the settings (DrawingSettings, group
// `drawing`) and saves back on OK.

#include <optional>

#include <QList>
#include <QString>
#include <QStringList>

#include "DrawingSettings.hpp"

class QWidget;

namespace mitcad {

struct DrawingChoice {
  QString path;
  DrawingSettings settings;
  bool selectedOnly = false;
  QString section; // a Section Analysis by name; empty: none
  // A drawing of the design written as it is (mitcad#725): its id (`D1`);
  // empty: a new one laid out from the settings.
  QString drawing;
  // A DXF file's length unit (mitcad#1408): the design's unless chosen.
  QString dxfUnit;
};

// A drawing of the design the dialog can write: its id and name.
struct DrawingTarget {
  QString uid;
  QString name;
};

// `selectedBodies`: how many bodies the selection stands for;
// `analyses`: the document's Section Analyses by name; `drawings`: the
// root's drawings it can write as they are, `last` the one it laid out
// last (chosen first; else the only drawing when there is one, else New
// Drawing, whose layout the settings make);
// `designUnit`: the design's length unit, a DXF file's first (mitcad#1408).
std::optional<DrawingChoice> askDrawing(QWidget* parent, const QString& path, int selectedBodies,
                                        const QStringList& analyses, const QList<DrawingTarget>& drawings = {},
                                        const QString& last = {}, const QString& designUnit = {});

} // namespace mitcad
