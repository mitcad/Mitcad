// SPDX-License-Identifier: MIT
#pragma once

// Minimum Radius (mitcad#2518, app/COMMANDS.md "Minimum Radius"): the
// analysis' definition, the model's `min_radius_analysis` query
// (commands.md, "Minimum radius analysis") and its answer in words, in the
// log's line and as what the view draws over the bodies (the Print Analysis
// overlay's marker and its thin-wall triangles, PrintAnalysis.hpp). Plain
// Qt: the command (InspectCommands.cpp), the window's shown analysis
// (MainWindowPrintAnalysis.cpp) and the unit tests share it.

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include "PrintAnalysis.hpp"

namespace mitcad {

// The definition (commands.md, "Analyses"): {"type": "min_radius", "bodies"
// (uids), "faces" (face references, when any), "limit" (mm, only when more
// than 0)}.
QJsonObject minRadiusAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, double limit);
// A definition's limit (mm); 0 when it has none.
double minRadiusLimitOf(const QJsonObject& def);
// The bodies a definition analyses: its bodies and those of its faces, each
// once (the window's shown analysis follows their shapes).
QJsonArray minRadiusBodiesOf(const QJsonObject& def);

// The query a definition asks: its fields with "query": "min_radius_analysis".
QJsonObject minRadiusAnalysisQuery(const QJsonObject& def);
// Runs the definition's query through `query` (on the model's worker as a
// job that can be cancelled). The answer's error (a body or a face gone),
// in words, is returned; empty when it answered.
QString runMinRadiusAnalysis(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer);

// The panel's result area: the smallest radius (a link that shows where)
// and the body it is on, or that no face bends; with a limit the faces
// under it and their area after a swatch of `color`; per body when there
// are several.
QString minRadiusAnalysisText(const QJsonObject& answer, const BodyNames& name, const QColor& color);
// The log's words (UI tests): "smallest 2.000 mm on Body1" (or "no radius,
// every face is flat"), then with a limit ", under 4 mm: 1 face(s), 62.8
// mm^2".
QString minRadiusAnalysisLog(const QJsonObject& answer, const BodyNames& name);
// What an inspection carries for the view (Inspection::printOverlay): the
// faces under the limit's triangles, the smallest radius' place and value
// and the legend's title.
QJsonObject minRadiusOverlayJson(const QJsonObject& def, const QJsonObject& answer, const QString& title);

} // namespace mitcad
