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
#include <QImage>
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
  // Draft Analysis (mitcad#2166, DraftAnalysis.hpp): the triangles of its
  // three classes (towards the pull, needs more draft, against the pull),
  // drawn as the overhangs are, in their colours.
  std::array<std::vector<float>, 3> draft;
  std::array<QColor, 3> draftColors;
  // Point Cloud Deviation (mitcad#2240, PointCloudDeviation.hpp): the
  // measured points (3 numbers each, design coordinates, mm), drawn as
  // points depth-tested with the bodies, each in its colour on the
  // diverging scale (inside the material, within tolerance, outside), and
  // each one's distance (mm, negative inside), which the hover tells.
  std::vector<float> cloudPoints;
  std::vector<QRgb> cloudColors;
  std::vector<float> cloudDistances;
  std::array<QColor, 3> deviationColors; // inside, within tolerance, outside
  // The colour the window draws the cloud's points that were not measured
  // in (mitcad#2265: outside the sample, or failed), the legend's entry
  // "Measured 8 of 32 points; the others dimmed" when there are any.
  QColor unmeasuredColor;
  // Minimum Distance (mitcad#2519, MinimumDistance.hpp): `gaps` holds its
  // one line between the closest points and `narrowest` the line's middle
  // with the distance as its label, in `gapColor`: the distance's color,
  // or the clearance's when the selections come closer than it.
  bool distanceLine = false;
  bool closerThanClearance = false;
  // Surface quality analyses (mitcad#2517, SurfaceAnalysis.hpp,
  // addSurfaceOverlay): one of them is the overlay (`surface`: its legend's
  // colors stay as they are on another background, they are the same on
  // both). A Zebra's dark and light stripes (9 numbers per triangle, design
  // coordinates) drawn as the overhangs are; a Curvature Map's triangles
  // with each corner's color (9 numbers and 3 colors per triangle); an
  // Isocurve Analysis' curves (3 numbers per point) with each point's
  // color; an Environment Map's triangles with each corner's normal (9
  // numbers each), reflecting `environment`.
  bool surface = false;
  std::array<std::vector<float>, 2> zebra;
  std::array<QColor, 2> zebraColors;
  std::vector<float> shaded;
  std::vector<QRgb> shadedColors;
  std::vector<std::vector<float>> isoLines;
  std::vector<std::vector<QRgb>> isoColors;
  std::vector<float> mirror;
  std::vector<float> mirrorNormals;
  QImage environment;
  bool isEmpty() const {
    return overhangs.empty() && thin.empty() && gaps.empty() && !thinnest && !narrowest && legend.empty() &&
           draft[0].empty() && draft[1].empty() && draft[2].empty() && cloudPoints.empty() && zebra[0].empty() &&
           zebra[1].empty() && shaded.empty() && isoLines.empty() && mirror.empty();
  }
};
// A deviation's colour (mitcad#2240): within `tolerance` of the faces the
// neutral colour, beyond it towards the inside or outside colour, reached at
// `limit` (mm; distances negative inside the material).
QColor deviationColor(double distance, double tolerance, double limit, const std::array<QColor, 3>& colors);
// What an inspection carries for the view (Inspection::printOverlay): the
// triangles, the thinnest point, the narrow gaps and the settings of the
// legend; empty when the definition checks neither overhangs, walls nor gaps.
QJsonObject printOverlayJson(const QJsonObject& def, const PrintAnswers& answers, const QString& title);
// The view's overlay of such an object; a Draft Analysis' (`draft`,
// draftOverlayJson) in `draftColors`; a Point Cloud Deviation's
// (`deviation`, pointCloudDeviationOverlayJson, mitcad#2240) in
// `deviationColors`, its points not measured in `unmeasuredColor`
// (mitcad#2265); a Minimum Radius' (`min_radius`, minRadiusOverlayJson,
// mitcad#2518) as the thin walls are: its smallest radius the marker, the
// faces under its limit the thin triangles, in `thinColor`; a Minimum
// Distance's (`minimum_distance`, minimumDistanceOverlayJson, mitcad#2519)
// line in `distanceColors` (the distance's, the clearance's when closer
// than it).
PrintOverlay printOverlayOf(const QJsonObject& json, const QColor& overhangColor, const QColor& thinColor,
                            const QColor& gapColor = QColor(), const std::array<QColor, 3>& draftColors = {},
                            const std::array<QColor, 3>& deviationColors = {},
                            const QColor& unmeasuredColor = QColor(),
                            const std::array<QColor, 2>& distanceColors = {});
// For the log, when it changes: "Print overlay: 12 overhang triangle(s)
// #rrggbb, 4 thin triangle(s) #rrggbb (150.0 mm^2: the area drawn thin),
// thinnest 0.50 mm at (x, y, z) (with narrow gaps then ", 1 narrow gap(s)
// #rrggbb, marker Gap 0.20 mm at (x, y, z)");
// legend Print1: Overhang over 45° | Wall under 0.8 mm", or "Print overlay:
// none". A Draft Analysis' adds ", draft 2 along #rrggbb, 8 under #rrggbb, 2
// against #rrggbb" (triangles) after the markers. A Minimum Distance's line
// (mitcad#2519) is ", minimum distance #rrggbb[ closer than the clearance],
// marker 5.00 mm at (x, y, z)" in place of the narrow gaps. A surface quality
// analysis' (mitcad#2517) adds ", zebra 4 dark #rrggbb, 8 light #rrggbb",
// ", curvature 12 triangle(s)", ", isocurves 36 curve(s) 72 point(s)" or
// ", environment 12 triangle(s) 256x256 picture".
QString printOverlayLog(const PrintOverlay& overlay);

// The point of a result link ("point:x,y,z"), if it is one.
std::optional<std::array<double, 3>> printLinkPoint(const QString& link);

} // namespace mitcad
