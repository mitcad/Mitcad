// SPDX-License-Identifier: MIT
#pragma once

// Draft Analysis (mitcad#2166, app/COMMANDS.md "Draft Analysis"): the
// analysis' definition, the model's `draft_analysis` query (commands.md,
// "Draft analysis") and its answer in words, in the log's line and as what
// the view draws over the bodies (the Print Analysis overlay's draft part,
// PrintAnalysis.hpp). Plain Qt: the command (InspectCommands.cpp), the
// window's shown analysis (MainWindowPrintAnalysis.cpp) and the unit tests
// share it.

#include <array>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

#include "PrintAnalysis.hpp"

namespace mitcad {

// The least draft a face needs when none is given, degrees.
constexpr double kDefaultDraftAngle = 1.0;

// The definition (commands.md, "Analyses"): {"type": "draft", "bodies",
// "pull" (a plane or line reference, or null for the design's +Z),
// "occurrence" (the pull reference's, when not empty), "flip", "angle"}.
QJsonObject draftAnalysisDef(const QJsonArray& bodies, const QJsonValue& pull, const QString& occurrence,
                             bool flip, double angle);
// A definition's angle, the default when it has none in range.
double draftAngleOf(const QJsonObject& def);

// The query a definition asks: its fields with "query": "draft_analysis".
QJsonObject draftAnalysisQuery(const QJsonObject& def);
// Runs the definition's query through `query` (slow: on the model's worker
// as a job that can be cancelled). The answer's error (a body or the pull
// reference gone), in words, is returned; empty when it answered.
QString runDraftAnalysis(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer);

// The panel's result area: the pull direction and the angle, then per body
// (and in total, for several) each class' area and share, each after a
// swatch of its colour (`colors`: towards the pull, needs more draft,
// against the pull); the legend is the swatches' words.
QString draftAnalysisText(const QJsonObject& answer, const BodyNames& name, const std::array<QColor, 3>& colors);
// The log's words (UI tests): "pull (0, 0, 1), angle 1: Body1 along 400.0
// mm^2 (16.7 %), under 1600.0 mm^2 (66.7 %), against 400.0 mm^2 (16.7 %)",
// bodies separated by "; ", with several then "; total …".
QString draftAnalysisLog(const QJsonObject& answer, const BodyNames& name);
// What an inspection carries for the view (Inspection::printOverlay): the
// three classes' triangles, the angle and the legend's title.
QJsonObject draftOverlayJson(const QJsonObject& def, const QJsonObject& answer, const QString& title);

} // namespace mitcad
