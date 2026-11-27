// SPDX-License-Identifier: MIT
#pragma once

// Curvature Comb (mitcad#2520, app/COMMANDS.md "Curvature Comb"): the
// curvature_comb query for picked sketch curves and edges, or for the
// curves sketch mode reads, and its answer as what the view draws: each
// curve's teeth (from the curve to the tip), the envelope through the tips
// and the inflection points. Inspect > Curvature Comb draws it over the
// design; sketch mode's Curvature comb option over the sketch it edits.
// Plain Qt Core: read by the unit test curvature_comb.

#include <array>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

using CombPoint = std::array<double, 3>;

// The comb's settings: teeth per curve and the size of the longest tooth
// (1: a quarter of the curves' size). Inspect > Curvature Comb keeps them
// (QSettings curvatureComb/teeth and curvatureComb/size) and sketch mode
// draws with them.
struct CurvatureCombSettings {
  static constexpr int kDefaultTeeth = 50;
  static constexpr int kMinTeeth = 2;
  static constexpr int kMaxTeeth = 1000;
  int teeth = kDefaultTeeth;
  double size = 1.0;

  static CurvatureCombSettings load();
  void save() const;
  // Teeth and size within their ranges (2 to 1000 teeth, a size above 0).
  bool valid() const;
};

// The curvature_comb query (core/model-api/src/commands.md "Curvature
// Comb"): `items` as the query takes them; without `scale` the longest
// tooth is `size` times a quarter of the curves' size.
QJsonObject curvatureCombQuery(const QJsonArray& items, const CurvatureCombSettings& settings,
                               std::optional<double> scale = std::nullopt, bool inSketch = false);

// The query's answer as it is drawn.
struct CurvatureCombDrawing {
  struct Curve {
    int item = -1; // the query's item it combs
    std::vector<CombPoint> points; // where the teeth stand on the curve
    std::vector<CombPoint> tips;   // their free ends, in the same order
    bool closed = false;           // the envelope runs on from the last tip to the first
  };
  std::vector<Curve> curves; // the combed curves (a straight one has none)
  std::vector<CombPoint> inflections;
  QStringList errors; // an item the model could not comb, with why
  int answered = 0;   // the curves the model answered, straight ones too
  int straight = 0;
  double scale = 0.0;
  double minRadius = 0.0; // the smallest radius of curvature; 0: none
  int teeth = 0;          // per curve

  bool empty() const { return curves.empty() && inflections.empty(); }
  // The teeth drawn, each from the curve to its tip (zero-length ones,
  // where a curve is straight, left out).
  std::vector<std::pair<CombPoint, CombPoint>> teethLines() const;
  // The envelope's pieces, tip to tip along each curve.
  std::vector<std::pair<CombPoint, CombPoint>> envelopeLines() const;
};

CurvatureCombDrawing curvatureCombDrawing(const QJsonObject& answer);

// For the log and the UI tests: "2 curve(s) (1 straight), 50 teeth each,
// 1 inflection(s) at (15, 0, 0), minimum radius 8 mm, scale 40" (mm,
// numbers to 0.1 mm); "no curve" when none was answered; an item's error
// after a semicolon.
QString curvatureCombLog(const CurvatureCombDrawing& drawing);

} // namespace mitcad
