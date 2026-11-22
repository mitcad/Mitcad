// SPDX-License-Identifier: MIT
// Draft Analysis (mitcad#2166): the analysis' definition and its default
// angle, the query it asks, the answer in the log's line and the result
// area's words with the legend's swatches, and the three classes the view
// draws with their legend.
#include "UnitTests.hpp"
#include "commands/DraftAnalysis.hpp"
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
    std::fprintf(stderr, "draft_analysis.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

QString name(const QString& uid) { return uid == QStringLiteral("F2.b0") ? QStringLiteral("Body1") : uid; }

// A triangle at height z.
QJsonArray triangle(double z) { return QJsonArray{0.0, 0.0, z, 1.0, 0.0, z, 0.0, 1.0, z}; }

// `count` such triangles. Appended, not braced: MSVC reads a braced list
// whose one element is a QJsonArray as a copy of that array, so
// QJsonArray{triangle(5.0)} was the triangle's nine numbers, not one
// triangle.
QJsonArray triangles(int count, double z) {
  QJsonArray list;
  for (int i = 0; i < count; ++i) {
    list.append(triangle(z));
  }
  return list;
}

// The model's answer for a 20 mm cube pulled along +Z (core/model-api's
// draft_analysis): the top along, the sides under, the bottom against.
QJsonObject cubeAnswer() {
  const QJsonObject body{
      {QStringLiteral("body"), QStringLiteral("F2.b0")},
      {QStringLiteral("name"), QStringLiteral("Body1")},
      {QStringLiteral("area"), 2400.0},
      {QStringLiteral("positive_area"), 400.0},
      {QStringLiteral("under_area"), 1600.0},
      {QStringLiteral("negative_area"), 400.0},
      {QStringLiteral("positive_share"), 1.0 / 6.0},
      {QStringLiteral("under_share"), 2.0 / 3.0},
      {QStringLiteral("negative_share"), 1.0 / 6.0},
      {QStringLiteral("triangles"),
       QJsonObject{{QStringLiteral("positive"), triangles(2, 20.0)},
                   {QStringLiteral("under"), triangles(1, 5.0)},
                   {QStringLiteral("negative"), triangles(3, 0.0)}}}};
  return {{QStringLiteral("pull"), QJsonArray{0.0, 0.0, 1.0}},
          {QStringLiteral("angle"), 1.0},
          {QStringLiteral("bodies"), QJsonArray{body}},
          {QStringLiteral("area"), 2400.0},
          {QStringLiteral("positive_area"), 400.0},
          {QStringLiteral("under_area"), 1600.0},
          {QStringLiteral("negative_area"), 400.0},
          {QStringLiteral("positive_share"), 1.0 / 6.0},
          {QStringLiteral("under_share"), 2.0 / 3.0},
          {QStringLiteral("negative_share"), 1.0 / 6.0}};
}

void definitionTests() {
  const QJsonArray bodies{QStringLiteral("F2.b0")};
  // Without a pull reference: null, the design's +Z; no occurrence.
  const QJsonObject plain = mitcad::draftAnalysisDef(bodies, QJsonValue(), QStringLiteral("O1"), false, 1.0);
  CHECK(plain.value(QStringLiteral("type")).toString() == QStringLiteral("draft"));
  CHECK(plain.value(QStringLiteral("bodies")).toArray() == bodies);
  CHECK(plain.value(QStringLiteral("pull")).isNull());
  CHECK(!plain.contains(QStringLiteral("occurrence")));
  CHECK(plain.value(QStringLiteral("flip")) == QJsonValue(false));
  CHECK(same(plain.value(QStringLiteral("angle")).toDouble(), 1.0));
  // A face in an occurrence, flipped.
  const QJsonObject face{{QStringLiteral("body"), QStringLiteral("F2.b0")}, {QStringLiteral("face"), QStringLiteral("F2:end_a")}};
  const QJsonObject picked = mitcad::draftAnalysisDef(bodies, face, QStringLiteral("O1/O3"), true, 2.5);
  CHECK(picked.value(QStringLiteral("pull")).toObject() == face);
  CHECK(picked.value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1/O3"));
  CHECK(picked.value(QStringLiteral("flip")).toBool());
  // The root's occurrence is left out.
  CHECK(!mitcad::draftAnalysisDef(bodies, QStringLiteral("xy"), QString(), false, 1.0)
             .contains(QStringLiteral("occurrence")));

  // The angle: the definition's when in range, else the default.
  CHECK(same(mitcad::draftAngleOf(picked), 2.5));
  CHECK(same(mitcad::draftAngleOf(QJsonObject()), mitcad::kDefaultDraftAngle));
  CHECK(same(mitcad::draftAngleOf(QJsonObject{{QStringLiteral("angle"), 90.0}}), mitcad::kDefaultDraftAngle));
  CHECK(same(mitcad::draftAngleOf(QJsonObject{{QStringLiteral("angle"), 0.0}}), 0.0));

  // The query: the definition's fields.
  const QJsonObject query = mitcad::draftAnalysisQuery(picked);
  CHECK(query.value(QStringLiteral("query")).toString() == QStringLiteral("draft_analysis"));
  CHECK(query.value(QStringLiteral("bodies")).toArray() == bodies);
  CHECK(query.value(QStringLiteral("pull")).toObject() == face);
  CHECK(query.value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1/O3"));
  CHECK(query.value(QStringLiteral("flip")).toBool());
  CHECK(same(query.value(QStringLiteral("angle")).toDouble(), 2.5));
  const QJsonObject plainQuery = mitcad::draftAnalysisQuery(plain);
  CHECK(!plainQuery.contains(QStringLiteral("pull")) && !plainQuery.contains(QStringLiteral("occurrence")));
}

void answerTests() {
  // Asked as a slow query; the answer kept.
  QStringList asked;
  QJsonObject answer;
  const QJsonObject def = mitcad::draftAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonValue(), QString(),
                                                   false, 1.0);
  QString error = mitcad::runDraftAnalysis(
      def,
      [&asked](const QJsonObject& request, bool slow) {
        asked << request.value(QStringLiteral("query")).toString() + (slow ? QStringLiteral(" slow") : QString());
        return cubeAnswer();
      },
      answer);
  CHECK(error.isEmpty());
  CHECK(asked == QStringList{QStringLiteral("draft_analysis slow")});
  CHECK(answer == cubeAnswer());
  // A gone body: the answer's error in words.
  error = mitcad::runDraftAnalysis(
      def,
      [](const QJsonObject&, bool) {
        return QJsonObject{{QStringLiteral("error"), QStringLiteral("body F2.b0 does not exist at the timeline marker")}};
      },
      answer);
  CHECK(error.contains(QStringLiteral("does not exist")));

  // The log's line.
  CHECK(mitcad::draftAnalysisLog(cubeAnswer(), name) ==
        QStringLiteral("pull (0, 0, 1), angle 1: Body1 along 400.0 mm^2 (16.7 %), under 1600.0 mm^2 (66.7 %), "
                       "against 400.0 mm^2 (16.7 %)"));
  // Two bodies: each, then the total.
  QJsonObject two = cubeAnswer();
  QJsonArray bodies = two.value(QStringLiteral("bodies")).toArray();
  QJsonObject second = bodies.at(0).toObject();
  second.insert(QStringLiteral("body"), QStringLiteral("F4.b0"));
  second.remove(QStringLiteral("name"));
  bodies.append(second);
  two.insert(QStringLiteral("bodies"), bodies);
  for (const char* key : {"area", "positive_area", "under_area", "negative_area"}) {
    two.insert(QLatin1String(key), 2.0 * two.value(QLatin1String(key)).toDouble());
  }
  const QString log = mitcad::draftAnalysisLog(two, name);
  CHECK(log.contains(QStringLiteral("; F4.b0 along 400.0 mm^2 (16.7 %)")));
  CHECK(log.endsWith(QStringLiteral("; total along 800.0 mm^2 (16.7 %), under 3200.0 mm^2 (66.7 %), "
                                    "against 800.0 mm^2 (16.7 %)")));

  // The result area: the pull, each class after its swatch, the undercut
  // note; a total only for several bodies.
  const std::array<QColor, 3> colors = {QColor(0x11, 0x22, 0x33), QColor(0x44, 0x55, 0x66), QColor(0x77, 0x88, 0x99)};
  const QString text = mitcad::draftAnalysisText(cubeAnswer(), name, colors);
  CHECK(text.contains(QStringLiteral("Pulled along (0, 0, 1)")));
  CHECK(text.contains(QStringLiteral("<b>Body1</b>")));
  CHECK(text.contains(QStringLiteral("color:#112233")) && text.contains(QStringLiteral("color:#445566")) &&
        text.contains(QStringLiteral("color:#778899")));
  CHECK(text.contains(QStringLiteral("Draft 1° or more along the pull: 400.0 mm² (16.7 %)")));
  CHECK(text.contains(QStringLiteral("Needs more draft, under 1°: 1600.0 mm² (66.7 %)")));
  CHECK(text.contains(QStringLiteral("Draft 1° or more against the pull: 400.0 mm² (16.7 %)")));
  CHECK(text.contains(QStringLiteral("undercuts")));
  CHECK(!text.contains(QStringLiteral("Total")));
  CHECK(mitcad::draftAnalysisText(two, name, colors).contains(QStringLiteral("<b>Total</b>")));
}

void overlayTests() {
  const QJsonObject def = mitcad::draftAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonValue(), QString(),
                                                   false, 1.0);
  const QJsonObject json = mitcad::draftOverlayJson(def, cubeAnswer(), QStringLiteral("Draft1"));
  CHECK(json.value(QStringLiteral("title")).toString() == QStringLiteral("Draft1"));
  const QJsonObject draft = json.value(QStringLiteral("draft")).toObject();
  CHECK(same(draft.value(QStringLiteral("angle")).toDouble(), 1.0));
  CHECK(draft.value(QStringLiteral("positive")).toArray().size() == 1); // one body
  const QColor overhang(0x7e, 0x3f, 0xd0);
  const QColor thin(0xc2, 0x18, 0x8b);
  const QColor gap(0x00, 0x89, 0x7b);
  const std::array<QColor, 3> colors = {QColor(0x2b, 0x8c, 0xbe), QColor(0xc2, 0x18, 0x5b), QColor(0x7b, 0x5c, 0xc4)};
  const mitcad::PrintOverlay overlay = mitcad::printOverlayOf(json, overhang, thin, gap, colors);
  CHECK(!overlay.isEmpty());
  CHECK(overlay.overhangs.empty() && overlay.thin.empty() && overlay.gaps.empty() && !overlay.thinnest);
  CHECK(overlay.draft[0].size() == 18 && overlay.draft[1].size() == 9 && overlay.draft[2].size() == 27);
  CHECK(same(overlay.draft[0][2], 20.0) && same(overlay.draft[2][2], 0.0));
  CHECK(overlay.draftColors == colors);
  CHECK(overlay.legend.size() == 3);
  CHECK(overlay.legend[0].first == colors[0] &&
        overlay.legend[0].second == QStringLiteral("Draft 1° or more along the pull"));
  CHECK(overlay.legend[1].first == colors[1] && overlay.legend[1].second == QStringLiteral("Needs more draft, under 1°"));
  CHECK(overlay.legend[2].first == colors[2] &&
        overlay.legend[2].second == QStringLiteral("Draft 1° or more against the pull"));
  CHECK(mitcad::printOverlayLog(overlay) ==
        QStringLiteral("Print overlay: 0 overhang triangle(s) #7e3fd0, 0 thin triangle(s) #c2188b (0.0 mm^2), "
                       "no marker, draft 2 along #2b8cbe, 1 under #c2185b, 3 against #7b5cc4; legend Draft1: "
                       "#2b8cbe Draft 1° or more along the pull | #c2185b Needs more draft, under 1° | "
                       "#7b5cc4 Draft 1° or more against the pull"));
  // A Print Analysis' overlay has no draft classes.
  CHECK(mitcad::printOverlayOf(QJsonObject(), overhang, thin, gap, colors).isEmpty());
}

} // namespace

int draftAnalysisTests() {
  failures = 0;
  definitionTests();
  answerTests();
  overlayTests();
  return failures;
}
MITCAD_UNIT_TEST(draftAnalysisTests);
