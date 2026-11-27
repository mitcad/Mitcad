// SPDX-License-Identifier: MIT
#pragma once

// Surface quality analyses (mitcad#2517, app/COMMANDS.md "Zebra",
// "Curvature Map", "Isocurve Analysis", "Environment Map"): the analyses'
// definitions, the model's `surface_analysis` query (commands.md, "Surface
// quality analyses") and its answer in words, in the log's line and as what
// the view draws over the faces (the Print Analysis overlay's surface part,
// PrintAnalysis.hpp). Plain Qt: the commands (InspectCommands.cpp), the
// window's shown analysis (MainWindowPrintAnalysis.cpp), the browser's rows
// and the unit tests share it.

#include <array>
#include <optional>

#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

#include "PrintAnalysis.hpp"

namespace mitcad {

// The defaults the model takes for the fields a definition leaves out.
constexpr int kDefaultZebraStripes = 12;
constexpr double kDefaultZebraWidth = 0.5;
constexpr int kDefaultIsocurves = 10;

// The analyses' types as the model writes them: "zebra", "curvature",
// "isocurve", "environment".
QStringList surfaceAnalysisTypes();
bool isSurfaceAnalysis(const QString& type);
// Each type's command ("inspect.zebra"), icon ("zebra-analysis"), name in
// the UI ("Zebra") and words in the log ("Zebra analysis"); empty for
// another type.
QString surfaceAnalysisCommand(const QString& type);
QString surfaceAnalysisIcon(const QString& type);
QString surfaceAnalysisTitle(const QString& type);
QString surfaceAnalysisKind(const QString& type);

// The definitions (commands.md, "Analyses"): the bodies analysed whole and
// faces of other bodies (face references), with each type's settings. A
// limit or an image left out is none.
QJsonObject zebraAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, const QJsonValue& direction,
                             const QString& occurrence, int stripes, double width);
QJsonObject curvatureAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, const QString& kind,
                                 std::optional<double> limit);
QJsonObject isocurveAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, int count, bool curvature,
                                std::optional<double> limit);
QJsonObject environmentAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, const QString& image);

// The query a definition asks: {"query": "surface_analysis", "analysis": def}.
QJsonObject surfaceAnalysisQuery(const QJsonObject& def);
// Runs the definition's query through `query` (slow: on the model's worker
// as a job that can be cancelled). The answer's error (a body, a face or
// the direction gone), in words, is returned; empty when it answered.
QString runSurfaceAnalysis(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer);

// A curvature on the scale (`colors`: hollow, flat, bulging): the flat
// color at zero, shading to the hollow or the bulging one at `limit` either
// way and staying there beyond; the flat one for none.
QColor curvatureColor(std::optional<double> value, double limit, const std::array<QColor, 3>& colors);
// A curvature in words: "0.2 1/mm (radius 5 mm)", "0.04 1/mm²"; the radius
// for a curvature in 1/mm that is not zero.
QString curvatureText(double value, const QString& unit);

// The panel's result area per type: the settings in a sentence, then per
// body its numbers; a Zebra's and a Curvature Map's after swatches of their
// colors (`scale`: the curvature scale's three, `zebra`: dark and light).
QString surfaceAnalysisText(const QJsonObject& answer, const BodyNames& name, const std::array<QColor, 3>& scale,
                            const std::array<QColor, 2>& zebra);
// The log's words (UI tests), per type:
//   zebra "direction (0, 0, 1), 3 stripes, width 0.5: Body1 dark 800.0
//     mm^2 (4 triangles), light 1600.0 mm^2 (8 triangles)";
//   curvature "mean 1/mm, limit 0.2 (automatic): Body1 min 0.2, max 0.2,
//     128 triangles";
//   isocurve "10 per face, curvature colored: Body1 120 curves, 2400
//     points, min 0, max 0.2";
//   environment "studio picture: Body1 128 triangles";
// bodies separated by "; ".
QString surfaceAnalysisLog(const QJsonObject& answer, const BodyNames& name);
// What an inspection carries for the view (Inspection::printOverlay): the
// answer's geometry under "surface" with the definition's type and the
// legend's title.
QJsonObject surfaceOverlayJson(const QJsonObject& def, const QJsonObject& answer, const QString& title);

// The colors the view draws the analyses in.
struct SurfaceColors {
  std::array<QColor, 2> zebra;  // dark, light
  std::array<QColor, 3> scale;  // hollow, flat, bulging
  QColor isocurve;              // the iso curves without curvature colors
  std::array<QColor, 3> studio; // the studio picture's sky, floor and lights
};
// The view's colors of the analyses, from the roles (Theme.hpp).
SurfaceColors surfaceColors();
// Adds an overlay object's surface part (surfaceOverlayJson) to `overlay`:
// the Zebra's stripes, the Curvature Map's colored triangles, the curves or
// the mirrored faces, with the legend's entries.
void addSurfaceOverlay(const QJsonObject& json, const SurfaceColors& colors, PrintOverlay& overlay);

// The Environment Map's studio picture (`sky`, `floor`, `light`): a sky
// shading to a horizon over a darker floor, with soft light bands above,
// as a sphere map the view reflects in the faces.
QImage studioEnvironment(const QColor& sky, const QColor& floor, const QColor& light, int size = 256);

} // namespace mitcad
