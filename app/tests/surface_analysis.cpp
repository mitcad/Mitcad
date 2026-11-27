// SPDX-License-Identifier: MIT
// Surface quality analyses (mitcad#2517): the four definitions and the
// query they ask, the types' commands, icons and names, the curvature
// scale's colors and words, the answers in the log's lines and the result
// area, and what the view draws: a Zebra's stripes, a Curvature Map's
// colored triangles, an Isocurve Analysis' curves and an Environment Map's
// mirrored faces with its studio picture, with their legends and the
// overlay's log line (logged counts).
#include "UnitTests.hpp"
#include "commands/PrintAnalysis.hpp"
#include "commands/SurfaceAnalysis.hpp"

#include <array>
#include <cmath>
#include <cstdio>
#include <optional>

#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "surface_analysis.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool same(double a, double b) { return std::abs(a - b) < 1e-6; }

QString name(const QString& uid) { return uid == QStringLiteral("F2.b0") ? QStringLiteral("Body1") : uid; }

// A triangle at height z.
QJsonArray triangle(double z) { return QJsonArray{0.0, 0.0, z, 1.0, 0.0, z, 0.0, 1.0, z}; }

// `count` such triangles, appended (MSVC reads a braced list of one
// QJsonArray as a copy of it).
QJsonArray triangles(int count, double z) {
  QJsonArray list;
  for (int i = 0; i < count; ++i) {
    list.append(triangle(z));
  }
  return list;
}

QJsonArray one(const QJsonValue& value) {
  QJsonArray list;
  list.append(value);
  return list;
}

const QJsonObject kFace{{QStringLiteral("body"), QStringLiteral("F2.b0")}, {QStringLiteral("face"), QStringLiteral("F2:top")}};

mitcad::SurfaceColors testColors() {
  mitcad::SurfaceColors colors;
  colors.zebra = {QColor(0x11, 0x11, 0x11), QColor(0xee, 0xee, 0xee)};
  colors.scale = {QColor(0x00, 0x00, 0xff), QColor(0x80, 0x80, 0x80), QColor(0xff, 0x00, 0x00)};
  colors.isocurve = QColor(0x10, 0x20, 0x30);
  colors.studio = {QColor(0xc0, 0xd0, 0xe0), QColor(0x30, 0x30, 0x30), QColor(0xff, 0xff, 0xff)};
  return colors;
}

void definitionTests() {
  using namespace mitcad;
  CHECK(surfaceAnalysisTypes() == (QStringList{QStringLiteral("zebra"), QStringLiteral("curvature"),
                                                QStringLiteral("isocurve"), QStringLiteral("environment")}));
  CHECK(isSurfaceAnalysis(QStringLiteral("curvature")) && !isSurfaceAnalysis(QStringLiteral("draft")));
  CHECK(surfaceAnalysisCommand(QStringLiteral("zebra")) == QStringLiteral("inspect.zebra"));
  CHECK(surfaceAnalysisCommand(QStringLiteral("curvature")) == QStringLiteral("inspect.curvature_map"));
  CHECK(surfaceAnalysisCommand(QStringLiteral("isocurve")) == QStringLiteral("inspect.isocurve_analysis"));
  CHECK(surfaceAnalysisCommand(QStringLiteral("environment")) == QStringLiteral("inspect.environment_map"));
  CHECK(surfaceAnalysisIcon(QStringLiteral("environment")) == QStringLiteral("environment-map"));
  CHECK(surfaceAnalysisTitle(QStringLiteral("isocurve")) == QStringLiteral("Isocurve Analysis"));
  CHECK(surfaceAnalysisKind(QStringLiteral("zebra")) == QStringLiteral("Zebra analysis"));
  CHECK(surfaceAnalysisCommand(QStringLiteral("print")).isEmpty());

  const QJsonArray bodies{QStringLiteral("F2.b0")};
  // Without a direction: null, the design's +Z; no occurrence.
  const QJsonObject zebra = zebraAnalysisDef(bodies, QJsonArray(), QJsonValue(), QStringLiteral("O1"), 12, 0.5);
  CHECK(zebra.value(QStringLiteral("type")).toString() == QStringLiteral("zebra"));
  CHECK(zebra.value(QStringLiteral("direction")).isNull());
  CHECK(!zebra.contains(QStringLiteral("occurrence")) && !zebra.contains(QStringLiteral("faces")));
  CHECK(zebra.value(QStringLiteral("stripes")).toInt() == 12 && same(zebra.value(QStringLiteral("width")).toDouble(), 0.5));
  const QJsonObject along = zebraAnalysisDef(QJsonArray(), one(kFace), QStringLiteral("x"), QStringLiteral("O1"), 8, 0.3);
  CHECK(along.value(QStringLiteral("direction")).toString() == QStringLiteral("x"));
  CHECK(along.value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1"));
  CHECK(along.value(QStringLiteral("faces")).toArray().size() == 1);
  CHECK(along.value(QStringLiteral("bodies")).toArray().isEmpty());

  const QJsonObject curvature = curvatureAnalysisDef(bodies, QJsonArray(), QStringLiteral("mean"), std::nullopt);
  CHECK(curvature.value(QStringLiteral("kind")).toString() == QStringLiteral("mean"));
  CHECK(!curvature.contains(QStringLiteral("limit")));
  CHECK(same(curvatureAnalysisDef(bodies, QJsonArray(), QString(), 0.5).value(QStringLiteral("limit")).toDouble(), 0.5));
  CHECK(curvatureAnalysisDef(bodies, QJsonArray(), QString(), std::nullopt).value(QStringLiteral("kind")).toString() ==
        QStringLiteral("gaussian"));
  // A limit only with the curvature's colors.
  CHECK(!isocurveAnalysisDef(bodies, QJsonArray(), 10, false, 0.5).contains(QStringLiteral("limit")));
  const QJsonObject colored = isocurveAnalysisDef(bodies, QJsonArray(), 4, true, 0.5);
  CHECK(colored.value(QStringLiteral("curvature")).toBool() && colored.value(QStringLiteral("count")).toInt() == 4);
  CHECK(same(colored.value(QStringLiteral("limit")).toDouble(), 0.5));
  CHECK(!environmentAnalysisDef(bodies, QJsonArray(), QStringLiteral(" ")).contains(QStringLiteral("image")));
  CHECK(environmentAnalysisDef(bodies, QJsonArray(), QStringLiteral("sky.png")).value(QStringLiteral("image")) ==
        QStringLiteral("sky.png"));

  const QJsonObject query = surfaceAnalysisQuery(curvature);
  CHECK(query.value(QStringLiteral("query")).toString() == QStringLiteral("surface_analysis"));
  CHECK(query.value(QStringLiteral("analysis")).toObject() == curvature);
  // The query goes as a slow one; an answer's error is the result.
  bool slow = false;
  QJsonObject answer;
  const QString error = runSurfaceAnalysis(
      curvature,
      [&slow](const QJsonObject&, bool asked) {
        slow = asked;
        return QJsonObject{{QStringLiteral("error"), QStringLiteral("body F2.b0 does not exist at the timeline marker")}};
      },
      answer);
  CHECK(slow && !error.isEmpty());
}

void scaleTests() {
  using namespace mitcad;
  const std::array<QColor, 3> scale = testColors().scale;
  CHECK(curvatureColor(0.0, 1.0, scale) == scale[1]);
  CHECK(curvatureColor(1.0, 1.0, scale) == scale[2]);
  CHECK(curvatureColor(5.0, 1.0, scale) == scale[2]);
  CHECK(curvatureColor(-2.0, 1.0, scale) == scale[0]);
  CHECK(curvatureColor(std::nullopt, 1.0, scale) == scale[1]);
  const QColor half = curvatureColor(0.5, 1.0, scale);
  CHECK(half.red() > scale[1].red() && half.red() < scale[2].red());
  CHECK(curvatureText(0.2, QStringLiteral("1/mm")) == QStringLiteral("0.2 1/mm (radius 5 mm)"));
  CHECK(curvatureText(-0.25, QStringLiteral("1/mm")) == QStringLiteral("-0.25 1/mm (radius 4 mm)"));
  CHECK(curvatureText(0.04, QStringLiteral("1/mm^2")) == QStringLiteral("0.04 1/mm²"));
  CHECK(curvatureText(0.0, QStringLiteral("1/mm")) == QStringLiteral("0 1/mm"));
}

// The model's answers for a 20 mm cube (core/model-api's surface_analysis
// on the mock kernel's box).
QJsonObject zebraAnswer() {
  const QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("name"), QStringLiteral("Body1")},
                         {QStringLiteral("dark_area"), 800.0},
                         {QStringLiteral("light_area"), 1600.0},
                         {QStringLiteral("dark_triangles"), 4},
                         {QStringLiteral("light_triangles"), 8},
                         {QStringLiteral("faces"), 6},
                         {QStringLiteral("triangles"), QJsonObject{{QStringLiteral("dark"), triangles(4, 20.0)},
                                                                   {QStringLiteral("light"), triangles(8, 5.0)}}}};
  return {{QStringLiteral("type"), QStringLiteral("zebra")},
          {QStringLiteral("direction"), QJsonArray{0.0, 0.0, 1.0}},
          {QStringLiteral("stripes"), 3},
          {QStringLiteral("width"), 0.5},
          {QStringLiteral("bodies"), one(body)},
          {QStringLiteral("dark_area"), 800.0},
          {QStringLiteral("light_area"), 1600.0}};
}

// One triangle's mesh with a value at each corner (null: none).
QJsonObject mesh(const QJsonArray& values) {
  return {{QStringLiteral("positions"), QJsonArray{0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0}},
          {QStringLiteral("normals"), QJsonArray{0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0}},
          {QStringLiteral("indices"), QJsonArray{0, 1, 2}},
          {QStringLiteral("values"), values}};
}

QJsonObject curvatureAnswer() {
  const QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("mesh"), mesh(QJsonArray{0.2, -0.2, QJsonValue(QJsonValue::Null)})},
                         {QStringLiteral("min"), -0.2},
                         {QStringLiteral("min_at"), QJsonArray{10.0, 0.0, 0.0}},
                         {QStringLiteral("max"), 0.2},
                         {QStringLiteral("max_at"), QJsonArray{0.0, 0.0, 0.0}},
                         {QStringLiteral("triangles"), 1}};
  return {{QStringLiteral("type"), QStringLiteral("curvature")}, {QStringLiteral("kind"), QStringLiteral("mean")},
          {QStringLiteral("unit"), QStringLiteral("1/mm")},      {QStringLiteral("limit"), 0.2},
          {QStringLiteral("automatic"), true},                   {QStringLiteral("bodies"), one(body)}};
}

QJsonObject isocurveAnswer(bool colored) {
  const QJsonObject curve{{QStringLiteral("face"), 0},
                          {QStringLiteral("along"), QStringLiteral("u")},
                          {QStringLiteral("at"), 0.5},
                          {QStringLiteral("points"), QJsonArray{0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 10.0, 0.0, 0.0}},
                          {QStringLiteral("normals"), QJsonArray{0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0}},
                          {QStringLiteral("curvatures"), QJsonArray{0.0, 0.1, 0.2}}};
  QJsonArray curves;
  curves.append(curve);
  curves.append(curve);
  QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                   {QStringLiteral("count"), 2},
                   {QStringLiteral("points"), 6},
                   {QStringLiteral("curves"), curves},
                   {QStringLiteral("min"), 0.0},
                   {QStringLiteral("max"), 0.2}};
  QJsonObject answer{{QStringLiteral("type"), QStringLiteral("isocurve")},
                     {QStringLiteral("count"), 1},
                     {QStringLiteral("curvature"), colored},
                     {QStringLiteral("bodies"), one(body)}};
  if (colored) {
    answer.insert(QStringLiteral("limit"), 0.2);
  }
  return answer;
}

QJsonObject environmentAnswer() {
  const QJsonObject body{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("mesh"), mesh(QJsonArray())},
                         {QStringLiteral("triangles"), 1}};
  return {{QStringLiteral("type"), QStringLiteral("environment")},
          {QStringLiteral("image"), QJsonValue(QJsonValue::Null)},
          {QStringLiteral("bodies"), one(body)}};
}

void answerTests() {
  using namespace mitcad;
  CHECK(surfaceAnalysisLog(zebraAnswer(), name) ==
        QStringLiteral("direction (0, 0, 1), 3 stripes, width 0.5: Body1 dark 800.0 mm^2 (4 triangles), light "
                       "1600.0 mm^2 (8 triangles)"));
  CHECK(surfaceAnalysisLog(curvatureAnswer(), name) ==
        QStringLiteral("mean 1/mm, limit 0.2 (automatic): Body1 min -0.2, max 0.2, 1 triangles"));
  CHECK(surfaceAnalysisLog(isocurveAnswer(true), name) ==
        QStringLiteral("1 per face, curvature colored: Body1 2 curves, 6 points, min 0, max 0.2"));
  CHECK(surfaceAnalysisLog(isocurveAnswer(false), name) == QStringLiteral("1 per face: Body1 2 curves, 6 points"));
  CHECK(surfaceAnalysisLog(environmentAnswer(), name) == QStringLiteral("studio picture: Body1 1 triangles"));

  const SurfaceColors colors = testColors();
  const QString zebra = surfaceAnalysisText(zebraAnswer(), name, colors.scale, colors.zebra);
  CHECK(zebra.contains(QStringLiteral("Stripes from (0, 0, 1): 3 over the half turn, 50 % of each dark.")));
  CHECK(zebra.contains(QStringLiteral("<b>Body1</b>")));
  CHECK(zebra.contains(QStringLiteral("color:#111111")) && zebra.contains(QStringLiteral("Dark stripes: 800.0 mm²")));
  const QString curvature = surfaceAnalysisText(curvatureAnswer(), name, colors.scale, colors.zebra);
  CHECK(curvature.contains(QStringLiteral("Mean curvature in 1/mm")));
  CHECK(curvature.contains(QStringLiteral("Bulging, 0.2 1/mm or more")));
  CHECK(curvature.contains(QStringLiteral("Smallest: -0.2 1/mm (radius 5 mm) at <a href='point:10,0,0'>")));
  const QString isocurves = surfaceAnalysisText(isocurveAnswer(true), name, colors.scale, colors.zebra);
  CHECK(isocurves.contains(QStringLiteral("1 u and 1 v iso curves per face.")));
  CHECK(isocurves.contains(QStringLiteral("Curves: 2, points: 6")));
}

void overlayTests() {
  using namespace mitcad;
  const SurfaceColors colors = testColors();
  const QJsonObject zebraDef = zebraAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(), QJsonValue(),
                                                QString(), 3, 0.5);
  PrintOverlay zebra;
  addSurfaceOverlay(surfaceOverlayJson(zebraDef, zebraAnswer(), QStringLiteral("Zebra1")), colors, zebra);
  CHECK(zebra.surface && !zebra.isEmpty());
  CHECK(zebra.zebra[0].size() == 4 * 9 && zebra.zebra[1].size() == 8 * 9);
  CHECK(zebra.title == QStringLiteral("Zebra1"));
  CHECK(zebra.legend.size() == 2 && zebra.legend[0].first == colors.zebra[0]);
  CHECK(zebra.legend[0].second == QStringLiteral("Dark stripes, 3 over 180°"));
  CHECK(printOverlayLog(zebra).contains(QStringLiteral("zebra 4 dark #111111, 8 light #eeeeee")));

  // Each corner in its own color on the scale, none in the flat one.
  PrintOverlay shaded;
  const QJsonObject curvatureDef = curvatureAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(),
                                                        QStringLiteral("mean"), std::nullopt);
  addSurfaceOverlay(surfaceOverlayJson(curvatureDef, curvatureAnswer(), QStringLiteral("Curvature1")), colors, shaded);
  CHECK(shaded.shaded.size() == 9 && shaded.shadedColors.size() == 3);
  CHECK(shaded.shadedColors.size() == 3 && QColor::fromRgb(shaded.shadedColors[0]) == colors.scale[2]);
  CHECK(shaded.shadedColors.size() == 3 && QColor::fromRgb(shaded.shadedColors[1]) == colors.scale[0]);
  CHECK(shaded.shadedColors.size() == 3 && QColor::fromRgb(shaded.shadedColors[2]) == colors.scale[1]);
  CHECK(shaded.legend.size() == 3 && shaded.legend[0].second == QStringLiteral("Bulging, 0.2 1/mm or more"));
  CHECK(printOverlayLog(shaded).contains(QStringLiteral("curvature 1 triangle(s)")));

  // Two curves of three points, lifted off the face along its normal.
  PrintOverlay lines;
  const QJsonObject isoDef = isocurveAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(), 1, true, 0.2);
  addSurfaceOverlay(surfaceOverlayJson(isoDef, isocurveAnswer(true), QStringLiteral("Isocurves1")), colors, lines);
  CHECK(lines.isoLines.size() == 2 && lines.isoColors.size() == 2);
  CHECK(!lines.isoLines.empty() && lines.isoLines[0].size() == 9 && lines.isoLines[0][2] > 0.0f);
  CHECK(!lines.isoColors.empty() && QColor::fromRgb(lines.isoColors[0][2]) == colors.scale[2]);
  CHECK(printOverlayLog(lines).contains(QStringLiteral("isocurves 2 curve(s) 6 point(s)")));
  PrintOverlay plain;
  addSurfaceOverlay(surfaceOverlayJson(isoDef, isocurveAnswer(false), QStringLiteral("Isocurves1")), colors, plain);
  CHECK(!plain.isoColors.empty() && QColor::fromRgb(plain.isoColors[0][0]) == colors.isocurve);
  CHECK(plain.legend.size() == 1 && plain.legend[0].first == colors.isocurve);

  // The mirrored faces with their normals and the studio picture.
  PrintOverlay mirror;
  const QJsonObject environmentDef =
      environmentAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(), QString());
  addSurfaceOverlay(surfaceOverlayJson(environmentDef, environmentAnswer(), QStringLiteral("Environment1")), colors,
                    mirror);
  CHECK(mirror.mirror.size() == 9 && mirror.mirrorNormals.size() == 9);
  CHECK(same(mirror.mirrorNormals[2], 1.0));
  CHECK(mirror.environment.width() == 256 && mirror.environment.height() == 256);
  CHECK(printOverlayLog(mirror).contains(QStringLiteral("environment 1 triangle(s) 256x256 picture")));
  // A picture that cannot be read: the studio's, said in the legend.
  PrintOverlay missing;
  QJsonObject unread = environmentAnswer();
  unread.insert(QStringLiteral("image"), QStringLiteral("no such picture.png"));
  addSurfaceOverlay(surfaceOverlayJson(environmentAnalysisDef(QJsonArray{QStringLiteral("F2.b0")}, QJsonArray(),
                                                              QStringLiteral("no such picture.png")),
                                       unread, QStringLiteral("Environment1")),
                    colors, missing);
  CHECK(!missing.environment.isNull());
  CHECK(missing.legend.size() == 1 && missing.legend[0].second.contains(QStringLiteral("could not be read")));

  // The studio picture: the sky above, the darker floor below.
  const QImage studio = studioEnvironment(colors.studio[0], colors.studio[1], colors.studio[2], 64);
  CHECK(studio.width() == 64);
  CHECK(QColor(studio.pixel(2, 62)).lightness() < QColor(studio.pixel(62, 4)).lightness());

  // Another analysis' overlay has no surface part.
  PrintOverlay none;
  addSurfaceOverlay(QJsonObject{{QStringLiteral("title"), QStringLiteral("Print1")}}, colors, none);
  CHECK(!none.surface && none.isEmpty());
}

} // namespace

int surfaceAnalysisTests() {
  failures = 0;
  definitionTests();
  scaleTests();
  answerTests();
  overlayTests();
  return failures;
}
MITCAD_UNIT_TEST(surfaceAnalysisTests);
