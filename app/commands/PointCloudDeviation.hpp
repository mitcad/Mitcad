// SPDX-License-Identifier: MIT
#pragma once

// Point Cloud Deviation (mitcad#2240, app/COMMANDS.md "Point Cloud
// Deviation"): the analysis' definition, the model's
// `point_cloud_deviation` query (commands.md, "Point clouds") and its answer
// in words, in the log's line and as what the view draws: the cloud's
// measured points coloured on a diverging scale (the Print Analysis
// overlay's `deviation` part, PrintAnalysis.hpp). Plain Qt: the command
// (InspectCommands.cpp), the window's shown analysis
// (MainWindowPrintAnalysis.cpp) and the unit tests share it.

#include <array>
#include <vector>

#include <QByteArray>
#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include "PrintAnalysis.hpp"

namespace mitcad {

// A deviation's tolerance (mm) and the points it measures at most, when
// none are given (the model's own defaults).
constexpr double kDefaultDeviationTolerance = 0.1;
constexpr int kDefaultDeviationSamples = 100000;
// The worst points the panel lists.
constexpr int kDeviationWorstListed = 5;

// The definition (commands.md, "Analyses"): {"type":
// "point_cloud_deviation", "cloud", "body", "tolerance", "max_samples",
// "signed", "limit" (the colour scale's, mm) when it is greater than zero,
// and "faces" (the body's face names measured from, mitcad#2265) when some
// are given: every face of the body without}.
QJsonObject pointCloudDeviationDef(const QString& cloud, const QString& body, double tolerance, int maxSamples,
                                   bool isSigned, double limit, const QJsonArray& faces = QJsonArray());
// The face names a definition measures from; empty: every face.
QJsonArray deviationFacesOf(const QJsonObject& def);
// A definition's tolerance and sample count, the defaults when out of range;
// its colour limit, 0 when the scale follows the farthest point.
double deviationToleranceOf(const QJsonObject& def);
int deviationSamplesOf(const QJsonObject& def);
double deviationLimitOf(const QJsonObject& def);

// The query a definition asks: its fields with "query":
// "point_cloud_deviation" and each measured point's distance.
QJsonObject pointCloudDeviationQuery(const QJsonObject& def);

// The places of a cloud's points in the design (3 numbers each, mm) from
// the model's `point_cloud_points` answer: its points from its origin
// (base64 of little-endian floats) moved by `linear` and `translation`.
std::vector<float> cloudPlaces(const QJsonObject& points);

// Runs the definition's query through `query` (slow: on the model's worker
// as a job that can be cancelled) and asks the cloud's points; `places` are
// the measured points' places, in the order of the answer's distances. The
// answer's error (the cloud or body gone, a sign asked of an open body),
// in words, is returned; empty when it answered.
QString runPointCloudDeviation(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer,
                               std::vector<float>& places);

// The colour scale's limit for an answer: the definition's, else the
// farthest distance measured (at least twice the tolerance).
double deviationLimit(const QJsonObject& def, const QJsonObject& answer);

// The panel's result area: what was measured, the statistics, the legend's
// swatches (`colors`: inside, within tolerance, outside) and the worst
// points.
QString pointCloudDeviationText(const QJsonObject& def, const QJsonObject& answer, const QString& bodyName,
                                const std::array<QColor, 3>& colors);
// The log's words (UI tests): "PointCloud1 to Body1: 120 of 120 points,
// signed, mean 0.100 mm, mean absolute 0.400 mm, RMS 0.412 mm, max 0.500 mm,
// within 0.4 mm 60 (50.0 %), outside 60, inside 60, on the faces 0".
QString pointCloudDeviationLog(const QJsonObject& answer, const QString& bodyName);
// What an inspection carries for the view (Inspection::printOverlay): the
// cloud's name, the measured points, their distances, the tolerance and
// limit, the farthest point and the legend's title, and what was measured
// of the cloud (mitcad#2265): its points' `count`, the `step` and the
// `measured` count; the window draws the cloud's other points dimmed while it is shown
// (view/PointClouds.hpp, deviationSampleOf).
QJsonObject pointCloudDeviationOverlayJson(const QJsonObject& def, const QJsonObject& answer,
                                           const std::vector<float>& places, const QString& title);

} // namespace mitcad
