// SPDX-License-Identifier: MIT
// Print Analysis (mitcad#620): the settings group print_analysis kept and
// read back, the analysis' definition and its defaults, the queries it
// asks (the narrow gaps', mitcad#1519, too), the answers in the log's line
// and the panel's words, and what the view draws.
#include "commands/PrintAnalysis.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QSettings>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "print_analysis.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::PrintAnalysisSettings;
using mitcad::PrintAnswers;

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

QString name(const QString& uid) { return uid == QStringLiteral("F2.b0") ? QStringLiteral("Body1") : uid; }

void settingsTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);

  // The ticket's defaults.
  const PrintAnalysisSettings s = PrintAnalysisSettings::load(settings);
  CHECK(s.overhang && same(s.angle, 45.0));
  CHECK(s.thinWall && same(s.minimum, 0.8));
  CHECK(s.estimate && s.material.isEmpty() && same(s.lineWidth, 0.4) && s.walls == 2);
  CHECK(same(s.layerHeight, 0.2) && s.topBottomLayers == 4 && same(s.infill, 0.15));
  CHECK(same(s.filamentDiameter, 1.75) && !s.pricePerKg);

  // Kept and read back.
  PrintAnalysisSettings changed = s;
  changed.overhang = false;
  changed.angle = 60.0;
  changed.minimum = 1.2;
  changed.material = QStringLiteral("abs");
  changed.walls = 3;
  changed.infill = 0.4;
  changed.pricePerKg = 25.5;
  changed.save(settings);
  const PrintAnalysisSettings back = PrintAnalysisSettings::load(settings);
  CHECK(!back.overhang && same(back.angle, 60.0) && same(back.minimum, 1.2));
  CHECK(back.material == QStringLiteral("abs") && back.walls == 3 && same(back.infill, 0.4));
  CHECK(back.pricePerKg && same(*back.pricePerKg, 25.5));

  // Values out of range are the defaults; an empty price is none.
  settings.setValue(QStringLiteral("print_analysis/angle"), 120.0);
  settings.setValue(QStringLiteral("print_analysis/infill"), 1.5);
  settings.setValue(QStringLiteral("print_analysis/walls"), -1);
  settings.setValue(QStringLiteral("print_analysis/price_per_kg"), QString());
  const PrintAnalysisSettings odd = PrintAnalysisSettings::load(settings);
  CHECK(same(odd.angle, 45.0) && same(odd.infill, 0.15) && odd.walls == 2 && !odd.pricePerKg);
}

void definitionTests() {
  const QJsonArray bodies{QStringLiteral("F2.b0")};
  const QJsonObject face{{QStringLiteral("body"), QStringLiteral("F2.b0")}, {QStringLiteral("face"), QStringLiteral("F4:end(r1)")}};
  PrintAnalysisSettings s;
  const QJsonObject def = s.def(bodies, QJsonValue());
  CHECK(def.value(QStringLiteral("type")).toString() == QStringLiteral("print"));
  CHECK(def.value(QStringLiteral("bodies")).toArray() == bodies);
  CHECK(def.value(QStringLiteral("bed_face")).isNull());
  const QJsonObject estimate = def.value(QStringLiteral("estimate")).toObject();
  CHECK(estimate.value(QStringLiteral("material")).isNull() && estimate.value(QStringLiteral("price_per_kg")).isNull());
  CHECK(estimate.value(QStringLiteral("walls")).toInt() == 2 && same(estimate.value(QStringLiteral("infill")).toDouble(), 0.15));
  CHECK(def.value(QStringLiteral("overhang")).toObject().value(QStringLiteral("enabled")).toBool());
  CHECK(same(def.value(QStringLiteral("thin_wall")).toObject().value(QStringLiteral("minimum")).toDouble(), 0.8));

  // Back from the definition; fields left out take the defaults.
  s.angle = 30.0;
  s.pricePerKg = 20.0;
  s.material = QStringLiteral("pla");
  const PrintAnalysisSettings back = PrintAnalysisSettings::fromDef(s.def(bodies, face));
  CHECK(same(back.angle, 30.0) && back.pricePerKg && same(*back.pricePerKg, 20.0));
  CHECK(back.material == QStringLiteral("pla"));
  const PrintAnalysisSettings bare = PrintAnalysisSettings::fromDef(
      {{QStringLiteral("type"), QStringLiteral("print")}, {QStringLiteral("bodies"), bodies}});
  CHECK(bare.overhang && same(bare.angle, 45.0) && bare.thinWall && bare.estimate && !bare.pricePerKg);

  // The queries: -Z down without a bed face, the bed face with one.
  const QJsonObject overhangs = mitcad::printOverhangsQuery(def);
  CHECK(overhangs.value(QStringLiteral("query")).toString() == QStringLiteral("print_overhangs"));
  CHECK(overhangs.value(QStringLiteral("down")).toArray() == (QJsonArray{0.0, 0.0, -1.0}));
  CHECK(!overhangs.contains(QStringLiteral("bed_face")) && same(overhangs.value(QStringLiteral("angle")).toDouble(), 45.0));
  const QJsonObject onFace = mitcad::printOverhangsQuery(s.def(bodies, face));
  CHECK(onFace.value(QStringLiteral("bed_face")).toObject() == face && !onFace.contains(QStringLiteral("down")));
  const QJsonObject thickness = mitcad::printThicknessQuery(def);
  CHECK(thickness.value(QStringLiteral("query")).toString() == QStringLiteral("print_thickness"));
  CHECK(same(thickness.value(QStringLiteral("minimum")).toDouble(), 0.8) && thickness.value(QStringLiteral("bodies")).toArray() == bodies);
  const QJsonObject estimated = mitcad::printEstimateQuery(def);
  CHECK(estimated.value(QStringLiteral("query")).toString() == QStringLiteral("print_estimate"));
  CHECK(!estimated.contains(QStringLiteral("enabled")) && estimated.contains(QStringLiteral("price_per_kg")));
  CHECK(same(estimated.value(QStringLiteral("filament_diameter")).toDouble(), 1.75));
  // A part switched off asks nothing.
  PrintAnalysisSettings off;
  off.overhang = false;
  off.thinWall = false;
  off.estimate = false;
  const QJsonObject none = off.def(bodies, QJsonValue());
  CHECK(mitcad::printOverhangsQuery(none).isEmpty() && mitcad::printThicknessQuery(none).isEmpty() &&
        mitcad::printEstimateQuery(none).isEmpty());
}

// Answers as the model gives them (commands.md, "Print analysis").
QJsonObject overhangAnswer() {
  const QJsonArray triangle{0, 0, 20, 15, 0, 20, 15, 20, 20};
  const QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("area"), 6653.2},
                         {QStringLiteral("overhang_area"), 412.5},
                         {QStringLiteral("share"), 0.062},
                         {QStringLiteral("triangles"), QJsonArray{triangle, triangle}}};
  return {{QStringLiteral("down"), QJsonArray{0, 0, -1}},
          {QStringLiteral("bodies"), QJsonArray{body}},
          {QStringLiteral("overhang_area"), 412.5},
          {QStringLiteral("area"), 6653.2}};
}

QJsonObject thicknessAnswer() {
  const QJsonArray at{20, 10, 20.5};
  // One triangle in a list of its own, appended: QJsonArray{QJsonArray{...}}
  // is a copy of the inner array for MSVC, so its triangles came out flat.
  QJsonArray triangles;
  triangles.append(QJsonArray{0, 0, 20.5, 15, 0, 20.5, 15, 20, 20.5});
  const QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("thinnest"), 0.5},
                         {QStringLiteral("at"), at},
                         {QStringLiteral("regions"), 3},
                         {QStringLiteral("triangles"), triangles}};
  return {{QStringLiteral("bodies"), QJsonArray{body}}, {QStringLiteral("thinnest"), 0.5}, {QStringLiteral("at"), at}};
}

QJsonObject estimateAnswer(bool cost) {
  QJsonObject answer{{QStringLiteral("volume"), 33500.0},     {QStringLiteral("area"), 6653.2},
                     {QStringLiteral("shell_volume"), 5322.6}, {QStringLiteral("infill_volume"), 4226.6},
                     {QStringLiteral("filament_volume"), 33467.7}, {QStringLiteral("mass"), 41.5},
                     {QStringLiteral("length"), 13.9},         {QStringLiteral("material"), QStringLiteral("PLA Plastic")},
                     {QStringLiteral("density"), 1.24},        {QStringLiteral("cost"), QJsonValue()}};
  if (cost) {
    answer.insert(QStringLiteral("cost"), 1.04);
  }
  return answer;
}

void answerTests() {
  const QJsonObject def = PrintAnalysisSettings().def(QJsonArray{QStringLiteral("F2.b0")}, QJsonValue());
  // Each enabled query once, overhangs and thickness as slow ones.
  QStringList asked;
  PrintAnswers answers;
  QString error = mitcad::runPrintAnalysis(
      def,
      [&asked](const QJsonObject& request, bool slow) {
        const QString query = request.value(QStringLiteral("query")).toString();
        asked << QStringLiteral("%1 %2").arg(query, slow ? QStringLiteral("slow") : QStringLiteral("quick"));
        return query == QStringLiteral("print_overhangs")   ? overhangAnswer()
               : query == QStringLiteral("print_thickness") ? thicknessAnswer()
                                                             : estimateAnswer(false);
      },
      answers);
  CHECK(error.isEmpty());
  CHECK(asked == (QStringList{QStringLiteral("print_overhangs slow"), QStringLiteral("print_thickness slow"),
                              QStringLiteral("print_estimate quick")}));

  // The log's line, as the ticket has it.
  CHECK(mitcad::printAnalysisLog(answers, name) ==
        QStringLiteral("Body1 overhang 412.5 mm^2 (6.2 %), thinnest 0.50 mm, 3 thin region(s), estimate 41.5 g 13.9 m"));
  CHECK(mitcad::printEstimateLine(answers.estimate) == QStringLiteral("41.5 g · 13.9 m"));
  // The body's name as the answer gives it, before the window's.
  PrintAnswers named = answers;
  QJsonArray entries = named.overhangs.value(QStringLiteral("bodies")).toArray();
  QJsonObject entry = entries.at(0).toObject();
  entry.insert(QStringLiteral("name"), QStringLiteral("Bracket"));
  entries.replace(0, entry);
  named.overhangs.insert(QStringLiteral("bodies"), entries);
  CHECK(mitcad::printAnalysisLog(named, name).startsWith(QStringLiteral("Bracket overhang 412.5 mm^2")));
  answers.estimate = estimateAnswer(true);
  CHECK(mitcad::printAnalysisLog(answers, name).endsWith(QStringLiteral("estimate 41.5 g 13.9 m, cost 1.04")));
  CHECK(mitcad::printEstimateLine(answers.estimate) == QStringLiteral("41.5 g · 13.9 m · 1.04 €"));

  // The panel's words: the body's name, the share, a link to the thinnest
  // point, the estimate labelled as one.
  const QString text = mitcad::printAnalysisText(answers, name);
  CHECK(text.contains(QStringLiteral("<b>Body1</b>")));
  CHECK(text.contains(QStringLiteral("412.5 mm² (6.2 %)")));
  CHECK(text.contains(QStringLiteral("href='point:20,10,20.5'")) && text.contains(QStringLiteral("0.50 mm</a>")));
  CHECK(text.contains(QStringLiteral("3 thin region(s)")));
  CHECK(text.contains(QStringLiteral("41.5 g · 13.9 m · 1.04 €")) && text.contains(QStringLiteral("estimate")));
  const auto point = mitcad::printLinkPoint(QStringLiteral("point:20,10,20.5"));
  CHECK(point && same((*point)[0], 20.0) && same((*point)[1], 10.0) && same((*point)[2], 20.5));
  CHECK(!mitcad::printLinkPoint(QStringLiteral("point:1,2")) && !mitcad::printLinkPoint(QStringLiteral("http://x")));

  // An answer's error (a body or the bed face gone) stops the analysis.
  asked.clear();
  error = mitcad::runPrintAnalysis(
      def,
      [&asked](const QJsonObject& request, bool) {
        asked << request.value(QStringLiteral("query")).toString();
        return QJsonObject{{QStringLiteral("error"), QStringLiteral("the bed face cannot be found")}};
      },
      answers);
  CHECK(error == QStringLiteral("the bed face cannot be found") && asked.size() == 1);

  // Only the estimate: its words alone.
  PrintAnswers estimateOnly;
  estimateOnly.estimate = estimateAnswer(false);
  CHECK(mitcad::printAnalysisLog(estimateOnly, name) == QStringLiteral("estimate 41.5 g 13.9 m"));
  CHECK(mitcad::printAnalysisLog(PrintAnswers(), name) == QStringLiteral("nothing checked"));
}

void overlayTests() {
  const QJsonObject def = PrintAnalysisSettings().def(QJsonArray{QStringLiteral("F2.b0")}, QJsonValue());
  PrintAnswers answers;
  answers.overhangs = overhangAnswer();
  answers.thickness = thicknessAnswer();
  const QJsonObject json = mitcad::printOverlayJson(def, answers, QStringLiteral("Print1"));
  const QColor warning(0xc0, 0x80, 0x00);
  const QColor error(0xd0, 0x20, 0x20);
  const mitcad::PrintOverlay overlay = mitcad::printOverlayOf(json, warning, error);
  CHECK(overlay.overhangs.size() == 18 && overlay.thin.size() == 9);
  CHECK(same(overlay.overhangs[2], 20.0) && same(overlay.thin[2], 20.5));
  CHECK(overlay.thinnest && same((*overlay.thinnest)[2], 20.5));
  CHECK(overlay.thinnestLabel == QStringLiteral("Thinnest 0.50 mm"));
  CHECK(overlay.title == QStringLiteral("Print1") && overlay.legend.size() == 2);
  CHECK(overlay.legend[0].first == warning && overlay.legend[0].second == QStringLiteral("Overhang, angle 45°"));
  CHECK(overlay.legend[1].first == error && overlay.legend[1].second == QStringLiteral("Thin wall, under 0.8 mm"));
  CHECK(mitcad::printOverlayLog(overlay) ==
        QStringLiteral("Print overlay: 2 overhang triangle(s) #c08000, 1 thin triangle(s) #d02020 (150.0 mm^2), "
                       "marker Thinnest 0.50 mm at (20, 10, 20.5); legend Print1: #c08000 Overhang, angle 45° | "
                       "#d02020 Thin wall, under 0.8 mm"));

  // Thin walls off: no thin triangles, no marker, one legend row.
  PrintAnalysisSettings noWalls;
  noWalls.thinWall = false;
  const mitcad::PrintOverlay overhangsOnly = mitcad::printOverlayOf(
      mitcad::printOverlayJson(noWalls.def(QJsonArray{QStringLiteral("F2.b0")}, QJsonValue()), answers,
                               QStringLiteral("Print1")),
      warning, error);
  CHECK(overhangsOnly.thin.empty() && !overhangsOnly.thinnest && overhangsOnly.legend.size() == 1);

  // Nothing checked in the view: nothing drawn.
  PrintAnalysisSettings estimateOnly;
  estimateOnly.overhang = false;
  estimateOnly.thinWall = false;
  CHECK(mitcad::printOverlayJson(estimateOnly.def(QJsonArray(), QJsonValue()), answers, QStringLiteral("P")).isEmpty());
  CHECK(mitcad::printOverlayOf(QJsonObject(), warning, error).isEmpty());
  CHECK(mitcad::printOverlayLog(mitcad::PrintOverlay()) == QStringLiteral("Print overlay: none"));
}

// Narrow gaps between bodies printed in place (mitcad#1519).
QJsonObject gapAnswer(bool any) {
  QJsonArray pairs;
  if (any) {
    pairs.append(QJsonObject{{QStringLiteral("a"), QStringLiteral("F2.b0")},
                             {QStringLiteral("b"), QStringLiteral("F4.b0")},
                             {QStringLiteral("names"), QJsonArray{QStringLiteral("Body1"), QStringLiteral("Body2")}},
                             {QStringLiteral("distance"), 0.2},
                             {QStringLiteral("on_a"), QJsonArray{20, 10, 10}},
                             {QStringLiteral("on_b"), QJsonArray{20.2, 10, 10}}});
  }
  return {{QStringLiteral("gap"), 0.3},
          {QStringLiteral("gap_from"), QStringLiteral("default")},
          {QStringLiteral("bodies"), 2},
          {QStringLiteral("pairs"), pairs},
          {QStringLiteral("closest"), any ? QJsonValue(0.2) : QJsonValue()},
          {QStringLiteral("touching"), 0}};
}

void gapTests() {
  const QJsonArray two{QStringLiteral("F2.b0"), QStringLiteral("F4.b0")};
  PrintAnalysisSettings s;
  CHECK(s.minGap && !s.gap);
  // The definition: on, the gap null (the design's print_in_place_gap,
  // else 0.3 mm), and back.
  const QJsonObject def = s.def(two, QJsonValue());
  const QJsonObject minGap = def.value(QStringLiteral("min_gap")).toObject();
  CHECK(minGap.value(QStringLiteral("enabled")).toBool() && minGap.value(QStringLiteral("gap")).isNull());
  PrintAnalysisSettings given = s;
  given.gap = 0.4;
  const PrintAnalysisSettings back = PrintAnalysisSettings::fromDef(given.def(two, QJsonValue()));
  CHECK(back.minGap && back.gap && same(*back.gap, 0.4));

  // The query: the bodies, the gap only when given; none for one body or
  // switched off.
  const QJsonObject request = mitcad::printMinGapQuery(def);
  CHECK(request.value(QStringLiteral("query")).toString() == QStringLiteral("print_min_gap"));
  CHECK(request.value(QStringLiteral("bodies")).toArray() == two && !request.contains(QStringLiteral("gap")));
  CHECK(same(mitcad::printMinGapQuery(given.def(two, QJsonValue())).value(QStringLiteral("gap")).toDouble(), 0.4));
  CHECK(mitcad::printMinGapQuery(s.def(QJsonArray{QStringLiteral("F2.b0")}, QJsonValue())).isEmpty());
  PrintAnalysisSettings off;
  off.minGap = false;
  CHECK(mitcad::printMinGapQuery(off.def(two, QJsonValue())).isEmpty());

  // Asked last, as a slow query.
  QStringList asked;
  PrintAnswers answers;
  const QString error = mitcad::runPrintAnalysis(
      def,
      [&asked](const QJsonObject& r, bool slow) {
        const QString query = r.value(QStringLiteral("query")).toString();
        asked << QStringLiteral("%1 %2").arg(query, slow ? QStringLiteral("slow") : QStringLiteral("quick"));
        return query == QStringLiteral("print_overhangs")   ? overhangAnswer()
               : query == QStringLiteral("print_thickness") ? thicknessAnswer()
               : query == QStringLiteral("print_estimate")  ? estimateAnswer(false)
                                                             : gapAnswer(true);
      },
      answers);
  CHECK(error.isEmpty());
  CHECK(asked == (QStringList{QStringLiteral("print_overhangs slow"), QStringLiteral("print_thickness slow"),
                              QStringLiteral("print_estimate quick"), QStringLiteral("print_min_gap slow")}));

  // The log and the panel's words: the pair, its distance as a link to the
  // middle of its closest points.
  CHECK(mitcad::printAnalysisLog(answers, name).endsWith(
      QStringLiteral("estimate 41.5 g 13.9 m; narrow gaps under 0.3 mm: Body1-Body2 0.20 mm")));
  const QString text = mitcad::printAnalysisText(answers, name);
  CHECK(text.contains(QStringLiteral("Narrow gaps</b> (under 0.3 mm)")));
  CHECK(text.contains(QStringLiteral("Body1 and Body2: <a href='point:20.1,10,10'>0.20 mm</a>")));
  PrintAnswers clear;
  clear.minGap = gapAnswer(false);
  CHECK(mitcad::printAnalysisLog(clear, name) == QStringLiteral("narrow gaps under 0.3 mm: none"));

  // The view: the closest points, the smallest gap's marker, the legend.
  const QColor overhang(0x7e, 0x3f, 0xd0);
  const QColor thin(0xc2, 0x18, 0x8b);
  const QColor gap(0x00, 0x89, 0x7b);
  PrintAnswers onlyGaps;
  onlyGaps.minGap = gapAnswer(true);
  const mitcad::PrintOverlay overlay =
      mitcad::printOverlayOf(mitcad::printOverlayJson(def, onlyGaps, QStringLiteral("Print1")), overhang, thin, gap);
  CHECK(overlay.gaps.size() == 6 && same(overlay.gaps[0], 20.0) && std::abs(overlay.gaps[3] - 20.2) < 1e-5);
  CHECK(overlay.narrowest && same((*overlay.narrowest)[0], 20.1));
  CHECK(overlay.narrowestLabel == QStringLiteral("Gap 0.20 mm"));
  CHECK(overlay.legend.size() == 1 && overlay.legend[0].first == gap &&
        overlay.legend[0].second == QStringLiteral("Narrow gap, under 0.3 mm"));
  CHECK(mitcad::printOverlayLog(overlay) ==
        QStringLiteral("Print overlay: 0 overhang triangle(s) #7e3fd0, 0 thin triangle(s) #c2188b (0.0 mm^2), "
                       "no marker, 1 narrow gap(s) #00897b, marker Gap 0.20 mm at (20.1, 10, 10); legend Print1: "
                       "#00897b Narrow gap, under 0.3 mm"));
  // None narrow: no gaps drawn, the legend still says what was checked.
  PrintAnswers none;
  none.minGap = gapAnswer(false);
  const mitcad::PrintOverlay clearOverlay =
      mitcad::printOverlayOf(mitcad::printOverlayJson(def, none, QStringLiteral("Print1")), overhang, thin, gap);
  CHECK(clearOverlay.gaps.empty() && !clearOverlay.narrowest && clearOverlay.legend.size() == 1);
}

} // namespace

int printAnalysisTests() {
  failures = 0;
  settingsTests();
  definitionTests();
  answerTests();
  overlayTests();
  gapTests();
  return failures;
}
