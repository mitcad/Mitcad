// SPDX-License-Identifier: MIT
// Point clouds and Point Cloud Deviation (mitcad#2240): a cloud's points
// read from the model's answer as the view draws them, the deviation's
// definition with its defaults, the query it asks, the measured points'
// places, the answer in the log's line and the panel's words, the colour
// scale, and the coloured points the view draws with their legend;
// mitcad#2265: some faces only, and the points not measured drawn dimmed.
#include "UnitTests.hpp"
#include "commands/PointCloudDeviation.hpp"
#include "commands/PrintAnalysis.hpp"
#include "view/PointClouds.hpp"

#include <array>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <vector>

#include <QByteArray>
#include <QColor>
#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "point_cloud_deviation.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool same(double a, double b) { return std::abs(a - b) < 1e-6; }

// Points as the model sends them: little-endian floats, base64.
QString encoded(const std::vector<float>& values) {
  QByteArray bytes(static_cast<qsizetype>(values.size() * sizeof(float)), '\0');
  std::memcpy(bytes.data(), values.data(), values.size() * sizeof(float));
  return QString::fromLatin1(bytes.toBase64());
}

// A cloud of four points 1000 mm along x, moved 10 up.
QJsonObject cloudAnswer() {
  return {{QStringLiteral("name"), QStringLiteral("Scan")},
          {QStringLiteral("count"), 4},
          {QStringLiteral("linear"), QJsonArray{QJsonArray{1, 0, 0}, QJsonArray{0, 1, 0}, QJsonArray{0, 0, 1}}},
          {QStringLiteral("translation"), QJsonArray{1000.0, 0.0, 10.0}},
          {QStringLiteral("points"), encoded({0, 0, 0, 1, 0, 0, 2, 0, 0, 3, 0, 0})},
          {QStringLiteral("colors"), QString::fromLatin1(QByteArray("\xff\0\0\0\xff\0\0\0\xff\x10\x20\x30", 12).toBase64())}};
}

void drawnCloudTests() {
  const mitcad::PointCloudDrawn drawn = mitcad::pointCloudDrawnOf(cloudAnswer());
  CHECK(drawn.name == QStringLiteral("Scan"));
  CHECK(drawn.count() == 4);
  CHECK(drawn.points[3] == 1.0f);
  CHECK(drawn.colors.size() == 12);
  CHECK(drawn.colors[0] == 255 && drawn.colors[11] == 0x30);
  CHECK(same(drawn.translation[0], 1000.0) && same(drawn.translation[2], 10.0));
  CHECK(mitcad::pointCloudsLog({drawn}) == QStringLiteral("Point clouds drawn: Scan 4 points coloured"));
  CHECK(mitcad::pointCloudsLog({}) == QStringLiteral("Point clouds drawn: none"));
  // Colours that do not match the points are left out.
  QJsonObject odd = cloudAnswer();
  odd.insert(QStringLiteral("colors"), QStringLiteral("AAAA"));
  CHECK(mitcad::pointCloudDrawnOf(odd).colors.empty());
  // The shown ones of the query, in order.
  const QJsonArray entries{QJsonObject{{QStringLiteral("name"), QStringLiteral("A")}},
                           QJsonObject{{QStringLiteral("name"), QStringLiteral("B")}, {QStringLiteral("visible"), false}},
                           QJsonObject{{QStringLiteral("name"), QStringLiteral("C")}, {QStringLiteral("visible"), true}}};
  CHECK(mitcad::shownPointClouds(entries) == (QStringList{QStringLiteral("A"), QStringLiteral("C")}));
  // The places in the design: the transform applied.
  const std::vector<float> places = mitcad::cloudPlaces(cloudAnswer());
  CHECK(places.size() == 12);
  CHECK(same(places[3], 1001.0) && same(places[5], 10.0));
}

// The model's answer for three points measured every second one of six.
QJsonObject deviationAnswer() {
  return {{QStringLiteral("cloud"), QStringLiteral("Scan")},
          {QStringLiteral("signed"), true},
          {QStringLiteral("tolerance"), 0.1},
          {QStringLiteral("points"), 6},
          {QStringLiteral("step"), 2},
          {QStringLiteral("sampled"), 3},
          {QStringLiteral("measured"), 3},
          {QStringLiteral("failures"), 0},
          {QStringLiteral("mean_abs_mm"), 0.3},
          {QStringLiteral("mean_signed_mm"), 0.1},
          {QStringLiteral("rms_mm"), 0.35},
          {QStringLiteral("max_abs_mm"), 0.5},
          {QStringLiteral("within_tolerance"), 1},
          {QStringLiteral("within_tolerance_fraction"), 1.0 / 3.0},
          {QStringLiteral("outside"), 1},
          {QStringLiteral("inside"), 1},
          {QStringLiteral("on_boundary"), 1},
          {QStringLiteral("distances"), QJsonArray{0.5, 0.0, -0.4}},
          {QStringLiteral("worst"), QJsonArray{QJsonObject{{QStringLiteral("index"), 0},
                                                           {QStringLiteral("point"), QJsonArray{1000.0, 0.0, 10.0}},
                                                           {QStringLiteral("distance"), 0.5}}}}};
}

void definitionTests() {
  const QJsonObject def = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F2.b0"), 0.05, 5000,
                                                         true, 0.0);
  CHECK(def.value(QStringLiteral("type")).toString() == QStringLiteral("point_cloud_deviation"));
  CHECK(!def.contains(QStringLiteral("limit")));
  CHECK(same(mitcad::deviationToleranceOf(def), 0.05));
  CHECK(mitcad::deviationSamplesOf(def) == 5000);
  CHECK(mitcad::deviationLimitOf(def) == 0.0);
  // Out of range: the defaults.
  CHECK(same(mitcad::deviationToleranceOf(QJsonObject()), mitcad::kDefaultDeviationTolerance));
  CHECK(mitcad::deviationSamplesOf(QJsonObject()) == mitcad::kDefaultDeviationSamples);
  const QJsonObject query = mitcad::pointCloudDeviationQuery(def);
  CHECK(query.value(QStringLiteral("query")).toString() == QStringLiteral("point_cloud_deviation"));
  CHECK(query.value(QStringLiteral("distances")).toBool());
  CHECK(query.value(QStringLiteral("signed")).toBool());
  CHECK(query.value(QStringLiteral("body")).toString() == QStringLiteral("F2.b0"));
  // A limit given is kept; without one the scale reaches the farthest point.
  const QJsonObject limited = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F2.b0"), 0.1,
                                                             100, false, 2.0);
  CHECK(same(mitcad::deviationLimit(limited, deviationAnswer()), 2.0));
  CHECK(same(mitcad::deviationLimit(def, deviationAnswer()), 0.5));
}

void runTests() {
  const QJsonObject def = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F2.b0"), 0.1, 3,
                                                         true, 0.0);
  int slow = 0;
  QJsonObject points = cloudAnswer();
  points.insert(QStringLiteral("points"), encoded({0, 0, 0, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 5, 0, 0}));
  const mitcad::PrintQuery query = [&slow, &points](const QJsonObject& request, bool isSlow) {
    slow += isSlow ? 1 : 0;
    return request.value(QStringLiteral("query")).toString() == QStringLiteral("point_cloud_points") ? points
                                                                                                     : deviationAnswer();
  };
  QJsonObject answer;
  std::vector<float> places;
  CHECK(mitcad::runPointCloudDeviation(def, query, answer, places).isEmpty());
  CHECK(slow == 1);
  // Points 0, 2 and 4 of the six, placed.
  CHECK(places.size() == 9);
  CHECK(same(places[0], 1000.0) && same(places[3], 1002.0) && same(places[6], 1004.0));
  // An error in words.
  const mitcad::PrintQuery refused = [](const QJsonObject&, bool) {
    return QJsonObject{{QStringLiteral("error"), QStringLiteral("the point cloud Scan is gone")}};
  };
  CHECK(!mitcad::runPointCloudDeviation(def, refused, answer, places).isEmpty());
}

void wordsTests() {
  const QJsonObject def = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F2.b0"), 0.1, 3,
                                                         true, 0.0);
  const QString log = mitcad::pointCloudDeviationLog(deviationAnswer(), QStringLiteral("Body1"));
  CHECK(log == QStringLiteral("Scan to Body1: 3 of 6 points, signed, mean 0.100 mm, mean absolute 0.300 mm, RMS 0.350 "
                              "mm, max 0.500 mm, within 0.1 mm 1 (33.3 %), outside 1, inside 1, on the faces 1"));
  const std::array<QColor, 3> colors = {QColor(0x21, 0x66, 0xac), QColor(0xa0, 0xa0, 0xa0), QColor(0xc5, 0x1b, 0x7d)};
  const QString text = mitcad::pointCloudDeviationText(def, deviationAnswer(), QStringLiteral("Body1"), colors);
  CHECK(text.contains(QStringLiteral("3 of 6 points of Scan measured from the faces of Body1")));
  CHECK(text.contains(QStringLiteral("one point in 2")));
  CHECK(text.contains(QStringLiteral("#2166ac")) && text.contains(QStringLiteral("#c51b7d")));
  CHECK(text.contains(QStringLiteral("Farthest points")));
}

void overlayTests() {
  const QJsonObject def = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F2.b0"), 0.1, 3,
                                                         true, 0.0);
  const std::vector<float> places = {0, 0, 0, 1, 0, 0, 2, 0, 0};
  const QJsonObject json =
      mitcad::pointCloudDeviationOverlayJson(def, deviationAnswer(), places, QStringLiteral("Deviation1"));
  CHECK(json.value(QStringLiteral("deviation")).toObject().value(QStringLiteral("cloud")).toString() ==
        QStringLiteral("Scan"));
  const std::array<QColor, 3> colors = {QColor(0, 0, 255), QColor(128, 128, 128), QColor(255, 0, 255)};
  const mitcad::PrintOverlay overlay =
      mitcad::printOverlayOf(json, QColor(Qt::red), QColor(Qt::magenta), QColor(), {}, colors);
  CHECK(!overlay.isEmpty());
  CHECK(overlay.cloudPoints.size() == 9);
  CHECK(overlay.cloudDistances.size() == 3);
  // Outside at the limit: the full outside colour; on the faces: neutral;
  // inside, 0.4 of 0.5 past the tolerance of 0.1: most of the way to blue.
  CHECK(QColor::fromRgb(overlay.cloudColors[0]) == colors[2]);
  CHECK(QColor::fromRgb(overlay.cloudColors[1]) == colors[1]);
  CHECK(QColor::fromRgb(overlay.cloudColors[2]).blue() > 200);
  // The scale's three entries, and (mitcad#2265) how many of the cloud's
  // six points were measured, in the colour of the others.
  CHECK(overlay.legend.size() == 4);
  CHECK(overlay.legend.back().second == QStringLiteral("Measured 3 of 6 points; the others dimmed"));
  CHECK(!overlay.legend.back().first.isValid());
  CHECK(overlay.thinnest.has_value());
  CHECK(overlay.thinnestLabel == QStringLiteral("Farthest 0.500 mm"));
  CHECK(mitcad::printOverlayLog(overlay).contains(QStringLiteral("deviation 3 point(s)")));
  // The scale: within tolerance neutral, beyond it towards the end.
  CHECK(mitcad::deviationColor(0.05, 0.1, 1.0, colors) == colors[1]);
  CHECK(mitcad::deviationColor(-1.0, 0.1, 1.0, colors) == colors[0]);
  CHECK(mitcad::deviationColor(2.0, 0.1, 1.0, colors) == colors[2]);
}

// mitcad#2265: some of the body's faces only, in the definition, the query
// and the panel's words.
void facesTests() {
  const QJsonArray top{QStringLiteral("F1:top")};
  const QJsonObject def = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F1.b0"), 0.1, 100,
                                                         true, 0.0, top);
  CHECK(def.value(QStringLiteral("faces")).toArray() == top);
  CHECK(mitcad::deviationFacesOf(def) == top);
  CHECK(mitcad::pointCloudDeviationQuery(def).value(QStringLiteral("faces")).toArray() == top);
  // None: every face, and no `faces` at all.
  const QJsonObject all =
      mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F1.b0"), 0.1, 100, true, 0.0);
  CHECK(!all.contains(QStringLiteral("faces")));
  CHECK(!mitcad::pointCloudDeviationQuery(all).contains(QStringLiteral("faces")));
  CHECK(mitcad::deviationFacesOf(QJsonObject{{QStringLiteral("faces"), QJsonArray{QString(), 3}}}).isEmpty());
  const std::array<QColor, 3> colors = {QColor(0x21, 0x66, 0xac), QColor(0xa0, 0xa0, 0xa0), QColor(0xc5, 0x1b, 0x7d)};
  CHECK(mitcad::pointCloudDeviationText(def, deviationAnswer(), QStringLiteral("Body1"), colors)
            .contains(QStringLiteral("3 of 6 points of Scan measured from one face of Body1")));
  const QJsonObject two = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F1.b0"), 0.1, 100,
                                                         true, 0.0, {QStringLiteral("F1:top"), QStringLiteral("F1:bottom")});
  CHECK(mitcad::pointCloudDeviationText(two, deviationAnswer(), QStringLiteral("Body1"), colors)
            .contains(QStringLiteral("3 of 6 points of Scan measured from 2 faces of Body1")));
}

// mitcad#2265: the cloud's points a shown deviation did not measure, read
// from its overlay, as their own cloud drawn dimmed.
void unmeasuredTests() {
  const QJsonObject def = mitcad::pointCloudDeviationDef(QStringLiteral("Scan"), QStringLiteral("F2.b0"), 0.1, 3,
                                                         true, 0.0);
  QJsonObject answer = deviationAnswer();
  const std::vector<float> places = {0, 0, 0, 2, 0, 0, 4, 0, 0};
  const QJsonObject json = mitcad::pointCloudDeviationOverlayJson(def, answer, places, QStringLiteral("Deviation1"));
  const QJsonObject deviation = json.value(QStringLiteral("deviation")).toObject();
  CHECK(deviation.value(QStringLiteral("count")).toInt() == 6);
  CHECK(deviation.value(QStringLiteral("step")).toInt() == 2);
  CHECK(deviation.value(QStringLiteral("measured")).toInt() == 3);
  const mitcad::DeviationSample sample = mitcad::deviationSampleOf(json);
  CHECK(sample.cloud == QStringLiteral("Scan"));
  CHECK(sample.step == 2);
  CHECK(sample.measured == (std::vector<bool>{true, true, true}));
  CHECK(mitcad::deviationSampleOf(QJsonObject()).cloud.isEmpty());
  // Six points, 0, 2 and 4 measured: 1, 3 and 5 are drawn dimmed, without
  // their colours, placed as the cloud is.
  mitcad::PointCloudDrawn cloud = mitcad::pointCloudDrawnOf(cloudAnswer());
  cloud.points = {0, 0, 0, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 5, 0, 0};
  cloud.colors.assign(18, 7);
  const mitcad::PointCloudDrawn rest = mitcad::unmeasuredPoints(cloud, sample);
  CHECK(rest.unmeasured);
  CHECK(rest.count() == 3);
  CHECK(rest.points.size() == 9 && rest.points[0] == 1.0f && rest.points[3] == 3.0f && rest.points[6] == 5.0f);
  CHECK(rest.colors.empty());
  CHECK(same(rest.translation[0], 1000.0));
  CHECK(mitcad::pointCloudsLog({rest}) == QStringLiteral("Point clouds drawn: Scan 3 points not measured"));
  // A point whose distance failed (null) is not measured either.
  answer.insert(QStringLiteral("distances"), QJsonArray{0.5, QJsonValue(), -0.4});
  const mitcad::DeviationSample failed =
      mitcad::deviationSampleOf(mitcad::pointCloudDeviationOverlayJson(def, answer, places, QString()));
  CHECK(failed.measured == (std::vector<bool>{true, false, true}));
  CHECK(mitcad::unmeasuredPoints(cloud, failed).count() == 4);
  // All measured: nothing dimmed.
  mitcad::DeviationSample every;
  every.cloud = QStringLiteral("Scan");
  every.measured.assign(6, true);
  CHECK(mitcad::unmeasuredPoints(cloud, every).count() == 0);
}

} // namespace

int pointCloudDeviationTests() {
  failures = 0;
  drawnCloudTests();
  definitionTests();
  runTests();
  wordsTests();
  overlayTests();
  facesTests();
  unmeasuredTests();
  return failures;
}

MITCAD_UNIT_TEST(pointCloudDeviationTests);
