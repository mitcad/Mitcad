// SPDX-License-Identifier: MIT
#pragma once

// Print Analysis (mitcad#620, app/COMMANDS.md "Print Analysis"): the
// panel's settings (the settings group `print_analysis`) and the analysis'
// definition, the model's four queries (commands.md, "Print analysis" and
// "Narrow gaps between bodies printed in place", mitcad#1519) and
// their answers in words, in the log's line and as what the view draws over
// the bodies. Plain Qt: the command (InspectCommands.cpp), the window's
// shown analysis (MainWindowPrintAnalysis.cpp) and the unit tests share it.

#include <array>
#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

class QSettings;

namespace mitcad {

// What Print Analysis checks and estimates. The panel starts with the
// settings group's values; each analysis keeps its own in its `def`.
struct PrintAnalysisSettings {
  bool overhang = true;
  double angle = 45.0; // degrees from the vertical
  bool thinWall = true;
  double minimum = 0.8; // mm: twice the line width
  bool estimate = true;
  QString material; // a physical material's id; empty: the bodies' own, else PLA
  double lineWidth = 0.4; // mm
  int walls = 2;
  double layerHeight = 0.2; // mm
  int topBottomLayers = 4;
  double infill = 0.15;           // a fraction, 0 to 1
  double filamentDiameter = 1.75; // mm
  std::optional<double> pricePerKg; // per kg, in the user's currency; none: no cost
  bool minGap = true; // narrow gaps between the bodies (mitcad#1519)
  std::optional<double> gap; // mm; none: the design's print_in_place_gap, else 0.3 mm

  // The group `print_analysis`; values out of range are the defaults'.
  static PrintAnalysisSettings load(QSettings& settings);
  void save(QSettings& settings) const;
  // A definition's settings, the defaults for the fields it leaves out.
  static PrintAnalysisSettings fromDef(const QJsonObject& def);
  // The definition (commands.md, "Analyses"): {"type": "print", "bodies",
  // "bed_face" (a face reference or null), "overhang", "thin_wall",
  // "estimate", "min_gap"}.
  QJsonObject def(const QJsonArray& bodies, const QJsonValue& bedFace) const;
};

// The queries a definition asks; an empty object for a part switched off.
// Without a bed face the design's -Z is down.
QJsonObject printOverhangsQuery(const QJsonObject& def);
QJsonObject printThicknessQuery(const QJsonObject& def);
QJsonObject printEstimateQuery(const QJsonObject& def);
// The narrow gaps between the bodies (mitcad#1519); empty for fewer than two
// bodies too: one body has no gap.
QJsonObject printMinGapQuery(const QJsonObject& def);

// The answers of one analysis (empty for a part switched off).
struct PrintAnswers {
  QJsonObject overhangs;
  QJsonObject thickness;
  QJsonObject estimate;
  QJsonObject minGap;
};

// Answers a request; `slow` for overhangs, thickness and gaps, which run on the
// model's worker as jobs that can be cancelled.
using PrintQuery = std::function<QJsonObject(const QJsonObject& request, bool slow)>;
// Runs the definition's queries through `query`. An answer's error (a body
// or the bed face gone) stops it and is returned; empty when all answered.
QString runPrintAnalysis(const QJsonObject& def, const PrintQuery& query, PrintAnswers& answers);

// A body's name by its uid, for an answer that does not give the body's
// `name`.
using BodyNames = std::function<QString(const QString& uid)>;
// The panel's result area: per body (and in total, for several) the
// overhang area and its share, the thinnest wall (a link "point:x,y,z"
// that shows where) and the thin regions, the narrow gaps between the bodies
// (each a link to the middle of its closest points), then the estimate.
QString printAnalysisText(const PrintAnswers& answers, const BodyNames& name);
// The log's words (UI tests): "Body1 overhang 412.5 mm^2 (6.2 %), thinnest
// 0.50 mm, 3 thin region(s), estimate 41.5 g 13.9 m" (", cost 1.04" with a
// price); bodies separated by "; "; with gaps checked, then "; narrow gaps
// under 0.3 mm: Body1-Body2 0.20 mm" (or "none").
QString printAnalysisLog(const PrintAnswers& answers, const BodyNames& name);
// The estimate in a line: "41.5 g · 13.9 m · 1.04 €".
QString printEstimateLine(const QJsonObject& estimate);

// What the view draws over the bodies while the analysis is shown
// (OcctViewer::setPrintOverlay): overhang triangles in one colour, those of
// thin walls in another, a marker and a label at the thinnest point, the
// narrow gaps' closest points in a third with a marker and a label at the
// smallest (mitcad#1519), and a legend in the corner.
struct PrintOverlay {
  std::vector<float> overhangs; // 9 numbers per triangle, design coordinates (mm)
  std::vector<float> thin;
  std::vector<float> gaps; // 6 numbers per narrow gap: its closest points (mm)
  QColor overhangColor;
  QColor thinColor;
  QColor gapColor;
  std::optional<std::array<double, 3>> thinnest;
  QString thinnestLabel; // "Thinnest 0.50 mm"
  std::optional<std::array<double, 3>> narrowest; // the middle of the smallest gap
  QString narrowestLabel;                         // "Gap 0.20 mm"
  QString title;         // the legend's: the analysis' name
  std::vector<std::pair<QColor, QString>> legend;
  bool isEmpty() const {
    return overhangs.empty() && thin.empty() && gaps.empty() && !thinnest && !narrowest && legend.empty();
  }
};
// What an inspection carries for the view (Inspection::printOverlay): the
// triangles, the thinnest point, the narrow gaps and the settings of the
// legend; empty when the definition checks neither overhangs, walls nor gaps.
QJsonObject printOverlayJson(const QJsonObject& def, const PrintAnswers& answers, const QString& title);
PrintOverlay printOverlayOf(const QJsonObject& json, const QColor& overhangColor, const QColor& thinColor,
                            const QColor& gapColor = QColor());
// For the log, when it changes: "Print overlay: 12 overhang triangle(s)
// #rrggbb, 4 thin triangle(s) #rrggbb (150.0 mm^2: the area drawn thin),
// thinnest 0.50 mm at (x, y, z) (with narrow gaps then ", 1 narrow gap(s)
// #rrggbb, marker Gap 0.20 mm at (x, y, z)");
// legend Print1: Overhang over 45° | Wall under 0.8 mm", or "Print overlay:
// none".
QString printOverlayLog(const PrintOverlay& overlay);

// The point of a result link ("point:x,y,z"), if it is one.
std::optional<std::array<double, 3>> printLinkPoint(const QString& link);

} // namespace mitcad
