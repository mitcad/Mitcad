// SPDX-License-Identifier: MIT
// Minimum Radius (mitcad#2518): the analysis' definition and its limit, the
// query it asks, the answer in the log's line and the result area's words,
// and what the view draws: the smallest radius' marker and the faces under
// the limit as the thin walls' triangles, with the legend.
#include "UnitTests.hpp"
#include "commands/MinRadiusAnalysis.hpp"
#include "commands/PrintAnalysis.hpp"

#include <array>
#include <cmath>
#include <cstdio>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "min_radius_analysis.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

QString name(const QString& uid) { return uid == QStringLiteral("F2.b0") ? QStringLiteral("Body1") : uid; }

QJsonObject faceRef(const QString& body, const QString& face) {
  return {{QStringLiteral("body"), body}, {QStringLiteral("face"), face}};
}

// A triangle at height z.
QJsonArray triangle(double z) { return QJsonArray{0.0, 0.0, z, 1.0, 0.0, z, 0.0, 1.0, z}; }

// `count` such triangles, appended (see draft_analysis.cpp: MSVC reads a
// braced list of one QJsonArray as a copy of it).
QJsonArray triangles(int count, double z) {
  QJsonArray list;
  for (int i = 0; i < count; ++i) {
    list.append(triangle(z));
  }
  return list;
}

// The model's answer for a block with roundings of 2, 5 and 10 mm and a
// limit of 4 mm (core/model-api's min_radius_analysis): the 2 mm rounding
// under it, a quarter of a cylinder 20 mm high.
QJsonObject roundedAnswer() {
  const QJsonArray at{39.4, 0.6, 10.0};
  const QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("name"), QStringLiteral("Body1")},
                         {QStringLiteral("faces"), 9},
                         {QStringLiteral("smallest"), 2.0},
                         {QStringLiteral("at"), at},
                         {QStringLiteral("face"), QStringLiteral("F3:fillet")},
                         {QStringLiteral("under_count"), 1},
                         {QStringLiteral("under_faces"), QJsonArray{QStringLiteral("F3:fillet")}},
                         {QStringLiteral("under_area"), 62.832},
                         {QStringLiteral("triangles"), triangles(3, 5.0)}};
  return {{QStringLiteral("limit"), 4.0},
          {QStringLiteral("smallest"), 2.0},
          {QStringLiteral("at"), at},
          {QStringLiteral("body"), QStringLiteral("F2.b0")},
          {QStringLiteral("body_name"), QStringLiteral("Body1")},
          {QStringLiteral("face"), QStringLiteral("F3:fillet")},
          {QStringLiteral("under_count"), 1},
          {QStringLiteral("under_area"), 62.832},
          {QStringLiteral("bodies"), QJsonArray{body}}};
}

// The answer for a block of planes without a limit.
QJsonObject flatAnswer() {
  const QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("name"), QStringLiteral("Body1")},
                         {QStringLiteral("faces"), 6},
                         {QStringLiteral("smallest"), QJsonValue()},
                         {QStringLiteral("at"), QJsonValue()},
                         {QStringLiteral("under_count"), 0},
                         {QStringLiteral("under_area"), 0.0},
                         {QStringLiteral("triangles"), QJsonArray()}};
  return {{QStringLiteral("limit"), QJsonValue()},  {QStringLiteral("smallest"), QJsonValue()},
          {QStringLiteral("at"), QJsonValue()},     {QStringLiteral("under_count"), 0},
          {QStringLiteral("under_area"), 0.0},      {QStringLiteral("bodies"), QJsonArray{body}}};
}

void definitionTests() {
  const QJsonArray bodies{QStringLiteral("F2.b0")};
  // Without faces or a limit: neither is written.
  const QJsonObject plain = mitcad::minRadiusAnalysisDef(bodies, QJsonArray(), 0.0);
  CHECK(plain.value(QStringLiteral("type")).toString() == QStringLiteral("min_radius"));
  CHECK(plain.value(QStringLiteral("bodies")).toArray() == bodies);
  CHECK(!plain.contains(QStringLiteral("faces")));
  CHECK(!plain.contains(QStringLiteral("limit")));
  CHECK(same(mitcad::minRadiusLimitOf(plain), 0.0));
  // Faces of another body with a limit.
  const QJsonArray faces{faceRef(QStringLiteral("F4.b0"), QStringLiteral("F4:side0")),
                         faceRef(QStringLiteral("F2.b0"), QStringLiteral("F2:end_a"))};
  const QJsonObject picked = mitcad::minRadiusAnalysisDef(bodies, faces, 3.0);
  CHECK(picked.value(QStringLiteral("faces")).toArray() == faces);
  CHECK(same(mitcad::minRadiusLimitOf(picked), 3.0));
  CHECK(same(mitcad::minRadiusLimitOf(QJsonObject{{QStringLiteral("limit"), -1.0}}), 0.0));
  // Its bodies and those of its faces, each once.
  CHECK(mitcad::minRadiusBodiesOf(picked) == (QJsonArray{QStringLiteral("F2.b0"), QStringLiteral("F4.b0")}));

  // The query: the definition's fields; the limit only when there is one.
  const QJsonObject query = mitcad::minRadiusAnalysisQuery(picked);
  CHECK(query.value(QStringLiteral("query")).toString() == QStringLiteral("min_radius_analysis"));
  CHECK(query.value(QStringLiteral("bodies")).toArray() == bodies);
  CHECK(query.value(QStringLiteral("faces")).toArray() == faces);
  CHECK(same(query.value(QStringLiteral("limit")).toDouble(), 3.0));
  CHECK(!mitcad::minRadiusAnalysisQuery(plain).contains(QStringLiteral("limit")));
}

void answerTests() {
  // Asked as a slow query; the answer kept.
  QStringList asked;
  QJsonObject answer;
  const QJsonObject def = mitcad::minRadiusAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(), 4.0);
  QString error = mitcad::runMinRadiusAnalysis(
      def,
      [&asked](const QJsonObject& request, bool slow) {
        asked << request.value(QStringLiteral("query")).toString() + (slow ? QStringLiteral(" slow") : QString());
        return roundedAnswer();
      },
      answer);
  CHECK(error.isEmpty());
  CHECK(asked == QStringList{QStringLiteral("min_radius_analysis slow")});
  CHECK(answer == roundedAnswer());
  // A gone body: the answer's error in words.
  error = mitcad::runMinRadiusAnalysis(
      def,
      [](const QJsonObject&, bool) {
        return QJsonObject{{QStringLiteral("error"), QStringLiteral("body F2.b0 does not exist at the timeline marker")}};
      },
      answer);
  CHECK(error.contains(QStringLiteral("does not exist")));

  // The log's line: the smallest, then the faces under the limit.
  CHECK(mitcad::minRadiusAnalysisLog(roundedAnswer(), name) ==
        QStringLiteral("smallest 2.000 mm on Body1, under 4 mm: 1 face(s), 62.8 mm^2"));
  CHECK(mitcad::minRadiusAnalysisLog(flatAnswer(), name) == QStringLiteral("no radius, every face is flat"));

  // The result area: the smallest as a link to where it is, the faces
  // under the limit after the swatch, the note; per body only for several.
  const QColor color(0x11, 0x22, 0x33);
  const QString text = mitcad::minRadiusAnalysisText(roundedAnswer(), name, color);
  CHECK(text.contains(QStringLiteral("Smallest radius: <b><a href='point:39.4,0.6,10'>2.000 mm</a></b> on Body1")));
  CHECK(text.contains(QStringLiteral("color:#112233")));
  CHECK(text.contains(QStringLiteral("Radius under 4 mm: 1 face(s), 62.8 mm²")));
  CHECK(text.contains(QStringLiteral("convex or concave")));
  CHECK(!text.contains(QStringLiteral("<b>Body1</b>:")));
  const QString flat = mitcad::minRadiusAnalysisText(flatAnswer(), name, color);
  CHECK(flat.contains(QStringLiteral("No face bends")));
  CHECK(!flat.contains(QStringLiteral("Radius under")));
  QJsonObject two = roundedAnswer();
  QJsonArray bodies = two.value(QStringLiteral("bodies")).toArray();
  bodies.append(flatAnswer().value(QStringLiteral("bodies")).toArray().at(0));
  two.insert(QStringLiteral("bodies"), bodies);
  const QString both = mitcad::minRadiusAnalysisText(two, name, color);
  CHECK(both.contains(QStringLiteral("<b>Body1</b>: smallest")));
  CHECK(both.contains(QStringLiteral("<b>Body1</b>: flat")));
}

void overlayTests() {
  const QColor overhang(0x7e, 0x3f, 0xd0);
  const QColor thin(0xc2, 0x18, 0x8b);
  const QJsonObject def = mitcad::minRadiusAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(), 4.0);
  const QJsonObject json = mitcad::minRadiusOverlayJson(def, roundedAnswer(), QStringLiteral("MinRadius1"));
  CHECK(json.value(QStringLiteral("title")).toString() == QStringLiteral("MinRadius1"));
  const QJsonObject radius = json.value(QStringLiteral("min_radius")).toObject();
  CHECK(same(radius.value(QStringLiteral("limit")).toDouble(), 4.0));
  CHECK(radius.value(QStringLiteral("parts")).toArray().size() == 1); // one body
  const mitcad::PrintOverlay overlay = mitcad::printOverlayOf(json, overhang, thin);
  CHECK(!overlay.isEmpty());
  CHECK(overlay.overhangs.empty() && overlay.gaps.empty());
  CHECK(overlay.thin.size() == 27);
  CHECK(overlay.thinnest.has_value());
  if (overlay.thinnest) {
    CHECK(same((*overlay.thinnest)[0], 39.4) && same((*overlay.thinnest)[2], 10.0));
  }
  CHECK(overlay.thinnestLabel == QStringLiteral("Smallest radius 2.00 mm"));
  CHECK(overlay.legend.size() == 2);
  if (overlay.legend.size() == 2) {
    CHECK(overlay.legend[0].first == thin && overlay.legend[0].second == QStringLiteral("Smallest radius 2.00 mm"));
    CHECK(overlay.legend[1].first == thin && overlay.legend[1].second == QStringLiteral("Radius under 4 mm"));
  }
  CHECK(mitcad::printOverlayLog(overlay) ==
        QStringLiteral("Print overlay: 0 overhang triangle(s) #7e3fd0, 3 thin triangle(s) #c2188b (1.5 mm^2), "
                       "marker Smallest radius 2.00 mm at (39.4, 0.6, 10); legend MinRadius1: "
                       "#c2188b Smallest radius 2.00 mm | #c2188b Radius under 4 mm"));

  // Without a limit nothing is drawn but the marker; a flat body has no
  // marker, the legend says so.
  const QJsonObject unlimited = mitcad::minRadiusAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(), 0.0);
  const mitcad::PrintOverlay marker =
      mitcad::printOverlayOf(mitcad::minRadiusOverlayJson(unlimited, roundedAnswer(), QStringLiteral("R")), overhang,
                             thin);
  CHECK(marker.thin.empty() && marker.thinnest.has_value() && marker.legend.size() == 1);
  const mitcad::PrintOverlay none =
      mitcad::printOverlayOf(mitcad::minRadiusOverlayJson(unlimited, flatAnswer(), QStringLiteral("R")), overhang, thin);
  CHECK(!none.thinnest && none.thin.empty());
  CHECK(none.legend.size() == 1 && none.legend[0].second == QStringLiteral("No face bends"));
}

} // namespace

int minRadiusAnalysisTests() {
  failures = 0;
  definitionTests();
  answerTests();
  overlayTests();
  return failures;
}

MITCAD_UNIT_TEST(minRadiusAnalysisTests);
