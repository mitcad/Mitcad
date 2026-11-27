// SPDX-License-Identifier: MIT
#pragma once

// Minimum Distance (mitcad#2519, app/COMMANDS.md "Minimum Distance"): the
// analysis' definition, the model's `minimum_distance` query (commands.md,
// "Minimum distance") and its answer in words, in the log's line and as
// what the view draws (the Print Analysis overlay's line between the
// closest points with its value, PrintAnalysis.hpp). Plain Qt: the command
// (InspectCommands.cpp), the window's shown analysis
// (MainWindowPrintAnalysis.cpp) and the unit tests share it.

#include <array>
#include <functional>
#include <optional>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include "../framework/Selection.hpp"
#include "PrintAnalysis.hpp"

namespace mitcad {

// A selection's items as the analysis' picks (commands.md, "Minimum
// distance"): a body {"body", "occurrence"}, a face, edge or vertex with
// its body ({"body", "face"}, ...), a component as the occurrence it was
// picked through ({"occurrence"} alone: every body in it); the occurrence
// left out in the root. Other kinds and repeats are left out.
QJsonArray distancePicks(const Selection& items);
// A pick as the item the panel shows (Edit Minimum Distance): a body, or
// its face, edge or vertex; invalid for a component's pick, which needs the
// component's uid (InspectCommands.cpp).
SelectionItem distancePickItem(const QJsonObject& pick);

// The definition (commands.md, "Analyses"): {"type": "minimum_distance",
// "first", "second", "clearance" (mm, when one is given)}.
QJsonObject minimumDistanceDef(const QJsonArray& first, const QJsonArray& second, std::optional<double> clearance);
// A definition's clearance, when it has one greater than zero.
std::optional<double> clearanceOf(const QJsonObject& def);

// The query a definition asks: its picks and clearance with
// "query": "minimum_distance".
QJsonObject minimumDistanceQuery(const QJsonObject& def);
// Runs the definition's query through `query` (slow: on the model's worker
// as a job that can be cancelled). The answer's error (a body or a
// component gone, the two selections the same), in words, is returned;
// empty when it answered.
QString runMinimumDistance(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer);

// A length in the design's unit ("5 mm", "0.197 in").
using LengthText = std::function<QString(double millimetres)>;
// The panel's result area: the distance, where it is measured from and to
// (each body by its name), whether the selections touch, and with a
// clearance whether they come closer than it, after a swatch of the line's
// color (`colors`: the distance's, the clearance's).
QString minimumDistanceText(const QJsonObject& answer, const LengthText& length, const std::array<QColor, 2>& colors);
// The log's words (UI tests): "5.000 mm from Body1 (5, 5, 10) to Body2
// (5, 5, 15)", with " (touching)" after the distance when they touch, and
// with a clearance "; clearance 6 mm: closer" or "...: clear".
QString minimumDistanceLog(const QJsonObject& answer);
// What an inspection carries for the view (Inspection::printOverlay): the
// closest points, the distance, the clearance and whether the selections
// come closer than it, and the legend's title.
QJsonObject minimumDistanceOverlayJson(const QJsonObject& answer, const QString& title);

} // namespace mitcad
