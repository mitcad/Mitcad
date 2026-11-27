// SPDX-License-Identifier: MIT
// Curvature Comb (mitcad#2520): the curvature_comb query the panel and
// sketch mode ask, its answer as the teeth, the envelope and the
// inflection points the view draws, and the log line.
#include "UnitTests.hpp"
#include "commands/CurvatureComb.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "curvature_comb.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonArray xyz(double x, double y, double z) {
  QJsonArray p;
  p.append(x);
  p.append(y);
  p.append(z);
  return p;
}

QJsonObject tooth(double x, double y, double tipY) {
  return {{QStringLiteral("t"), x}, {QStringLiteral("point"), xyz(x, y, 0.0)},
          {QStringLiteral("tip"), xyz(x, tipY, 0.0)}, {QStringLiteral("curvature"), std::abs(tipY - y)}};
}

void queryTests() {
  mitcad::CurvatureCombSettings settings;
  CHECK(settings.teeth == 50 && settings.size == 1.0 && settings.valid());
  QJsonArray items;
  items.append(QJsonObject{{QStringLiteral("sketch"), QStringLiteral("F1")}, {QStringLiteral("curve"), QStringLiteral("c5")}});
  const QJsonObject query = mitcad::curvatureCombQuery(items, settings);
  CHECK(query.value(QStringLiteral("query")).toString() == QStringLiteral("curvature_comb"));
  CHECK(query.value(QStringLiteral("items")).toArray() == items);
  CHECK(query.value(QStringLiteral("teeth")).toInt() == 50);
  CHECK(query.value(QStringLiteral("size")).toDouble() == 1.0);
  CHECK(!query.contains(QStringLiteral("scale")) && !query.contains(QStringLiteral("in_sketch")));
  // A scale kept through a drag; sketch coordinates.
  const QJsonObject held = mitcad::curvatureCombQuery(items, settings, 12.5, true);
  CHECK(held.value(QStringLiteral("scale")).toDouble() == 12.5);
  CHECK(held.value(QStringLiteral("in_sketch")).toBool());
  // No scale for none yet.
  CHECK(!mitcad::curvatureCombQuery(items, settings, 0.0).contains(QStringLiteral("scale")));
  settings.teeth = 1;
  CHECK(!settings.valid());
  settings.teeth = 1000;
  settings.size = 0.0;
  CHECK(!settings.valid());
}

void answerTests() {
  // An open S curve with three teeth (the middle one on its inflection),
  // a straight line, a closed curve with three teeth and an item the model
  // could not comb.
  QJsonArray sTeeth;
  sTeeth.append(tooth(0.0, 0.0, 2.0));
  sTeeth.append(tooth(15.0, 0.0, 0.0));
  sTeeth.append(tooth(30.0, 0.0, -2.0));
  QJsonArray sInflections;
  sInflections.append(QJsonObject{{QStringLiteral("t"), 0.5}, {QStringLiteral("point"), xyz(15.0, 0.0, 0.0)}});
  QJsonArray roundTeeth;
  roundTeeth.append(tooth(0.0, 10.0, 15.0));
  roundTeeth.append(tooth(5.0, 10.0, 15.0));
  roundTeeth.append(tooth(10.0, 10.0, 15.0));
  QJsonArray combs;
  combs.append(QJsonObject{{QStringLiteral("item"), 0},
                           {QStringLiteral("kind"), QStringLiteral("edge")},
                           {QStringLiteral("closed"), false},
                           {QStringLiteral("straight"), false},
                           {QStringLiteral("min_radius"), 8.04},
                           {QStringLiteral("teeth"), sTeeth},
                           {QStringLiteral("inflections"), sInflections}});
  combs.append(QJsonObject{{QStringLiteral("item"), 1},
                           {QStringLiteral("closed"), false},
                           {QStringLiteral("straight"), true},
                           {QStringLiteral("min_radius"), QJsonValue()},
                           {QStringLiteral("teeth"), QJsonArray()},
                           {QStringLiteral("inflections"), QJsonArray()}});
  combs.append(QJsonObject{{QStringLiteral("item"), 2},
                           {QStringLiteral("closed"), true},
                           {QStringLiteral("straight"), false},
                           {QStringLiteral("min_radius"), 2.0},
                           {QStringLiteral("teeth"), roundTeeth},
                           {QStringLiteral("inflections"), QJsonArray()}});
  combs.append(QJsonObject{{QStringLiteral("item"), 3}, {QStringLiteral("error"), QStringLiteral("Sketch1 has no curve c99")}});
  const QJsonObject answer{{QStringLiteral("scale"), 40.0},
                           {QStringLiteral("teeth"), 3},
                           {QStringLiteral("combs"), combs},
                           {QStringLiteral("inflections"), 1}};
  const mitcad::CurvatureCombDrawing drawing = mitcad::curvatureCombDrawing(answer);
  CHECK(drawing.answered == 3 && drawing.straight == 1);
  CHECK(drawing.curves.size() == 2);
  CHECK(drawing.curves[0].item == 0 && !drawing.curves[0].closed && drawing.curves[0].tips.size() == 3);
  CHECK(drawing.curves[1].item == 2 && drawing.curves[1].closed);
  CHECK(drawing.inflections.size() == 1 && drawing.inflections[0][0] == 15.0);
  CHECK(drawing.errors == QStringList{QStringLiteral("Sketch1 has no curve c99")});
  CHECK(drawing.minRadius == 2.0 && drawing.scale == 40.0 && drawing.teeth == 3);
  CHECK(!drawing.empty());
  // The teeth: the S curve's middle one has no length.
  CHECK(drawing.teethLines().size() == 5);
  // The envelope: two pieces of the open curve, three of the closed one.
  const auto envelope = drawing.envelopeLines();
  CHECK(envelope.size() == 5);
  CHECK(envelope.back().first[0] == 10.0 && envelope.back().second[0] == 0.0);
  CHECK(mitcad::curvatureCombLog(drawing) ==
        QStringLiteral("3 curve(s) (1 straight), 3 teeth each, 1 inflection(s) at (15, 0, 0), minimum radius 2 mm, "
                       "scale 40; Sketch1 has no curve c99"));
  // Nothing answered.
  const mitcad::CurvatureCombDrawing none = mitcad::curvatureCombDrawing(QJsonObject());
  CHECK(none.empty() && none.answered == 0);
  CHECK(mitcad::curvatureCombLog(none) == QStringLiteral("no curve"));
}

} // namespace

int curvatureCombTests() {
  failures = 0;
  queryTests();
  answerTests();
  return failures;
}
MITCAD_UNIT_TEST(curvatureCombTests);
