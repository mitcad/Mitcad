// SPDX-License-Identifier: MIT
// A 3D sketch's Fillet without the view (mitcad#1548): which points are
// corners (exactly two ordinary lines, not in line; a construction line,
// a third line or a lone end makes none), the corner of two picked lines
// and why two lines make none; the tangent arc of a radius at a right
// corner and at a slanted obtuse one (tangent points, centre, normal,
// sweep) and the radius that does not fit; the radius a pointer's ray
// gives along the bisector, kept inside the largest that fits; and the
// sketch3d.fillet command of a corner by its point and by its lines.
#include "sketch/Sketch3dFillet.hpp"

#include <cmath>
#include <cstdio>
#include <utility>

#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch3d_fillet.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

constexpr double kPi = 3.14159265358979323846;

bool same(const Xyz& a, const Xyz& b, double tolerance = 1e-9) {
  return std::abs(a[0] - b[0]) <= tolerance && std::abs(a[1] - b[1]) <= tolerance &&
         std::abs(a[2] - b[2]) <= tolerance;
}

bool same(double a, double b, double tolerance = 1e-9) { return std::abs(a - b) <= tolerance; }

// The route A-B-C-D (40 along X, 30 along Y, 20 along Z), a line E-F
// straight on from D, a construction line from B and a lone line G-H.
const char* const kSketch = R"({
  "uid": "F2", "name": "3DSketch1", "type": "spatial_sketch", "support": "xy",
  "frame": {"origin": [0, 0, 0], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0], "normal": [0, 0, 1]},
  "points": [
    {"id": "p1", "at": [0, 0, 0]},
    {"id": "p2", "at": [40, 0, 0]},
    {"id": "p4", "at": [40, 30, 0]},
    {"id": "p6", "at": [40, 30, 20]},
    {"id": "p8", "at": [40, 30, 35]},
    {"id": "p10", "at": [40, -20, 0]},
    {"id": "p12", "at": [0, 50, 0]},
    {"id": "p13", "at": [10, 50, 0]}
  ],
  "curves": [
    {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
    {"id": "c5", "type": "line", "start": "p2", "end": "p4"},
    {"id": "c7", "type": "line", "start": "p4", "end": "p6"},
    {"id": "c9", "type": "line", "start": "p6", "end": "p8"},
    {"id": "c11", "type": "line", "start": "p2", "end": "p10", "construction": true},
    {"id": "c14", "type": "line", "start": "p12", "end": "p13"}
  ],
  "constraints": [], "dimensions": [], "references": [],
  "evaluated": true, "status": {"dof": 0, "fully_constrained": true, "redundant": []}})";

Sketch3dModel sketch() { return Sketch3dModel::read(QJsonDocument::fromJson(QByteArray(kSketch)).object()); }

void cornerTests() {
  const Sketch3dModel model = sketch();
  // B: AB and BC (the construction line from B does not count).
  const auto b = filletCornerAt(model, QStringLiteral("p2"));
  CHECK(b.has_value());
  CHECK(b && b->first == QStringLiteral("c3") && b->second == QStringLiteral("c5") && !b->byLines);
  CHECK(b && same(b->at, {40, 0, 0}) && same(b->firstEnd, {0, 0, 0}) && same(b->secondEnd, {40, 30, 0}));
  CHECK(filletCornerAt(model, QStringLiteral("p4")).has_value());
  // D: CD and DE in line; A, a lone end; G, the lone line's end.
  CHECK(!filletCornerAt(model, QStringLiteral("p6")));
  CHECK(!filletCornerAt(model, QStringLiteral("p1")));
  CHECK(!filletCornerAt(model, QStringLiteral("p12")));
  CHECK(filletCornerPoints(model) == (QStringList{QStringLiteral("p2"), QStringLiteral("p4")}));
  // By its lines, in the order picked.
  const auto byLines = filletCornerOfLines(model, QStringLiteral("c7"), QStringLiteral("c5"));
  CHECK(byLines && byLines->point == QStringLiteral("p4") && byLines->byLines);
  CHECK(byLines && byLines->first == QStringLiteral("c7") && same(byLines->firstEnd, {40, 30, 20}));
  // The construction line and AB make a corner when named.
  CHECK(filletCornerOfLines(model, QStringLiteral("c3"), QStringLiteral("c11")).has_value());
  CHECK(!filletCornerOfLines(model, QStringLiteral("c3"), QStringLiteral("c7")));
  CHECK(!filletCornerOfLines(model, QStringLiteral("c7"), QStringLiteral("c9")));
  CHECK(!filletCornerOfLines(model, QStringLiteral("c3"), QStringLiteral("c3")));
  CHECK(filletLinesProblem(model, QStringLiteral("c3"), QStringLiteral("c7")).contains(QStringLiteral("do not share")));
  CHECK(filletLinesProblem(model, QStringLiteral("c7"), QStringLiteral("c9")).contains(QStringLiteral("in line")));
  CHECK(filletLinesProblem(model, QStringLiteral("c3"), QStringLiteral("c5")).isEmpty());
  // In words, as the palette names the lines (mitcad#1653).
  CHECK(filletLinesProblem(model, QStringLiteral("c3"), QStringLiteral("c7")) ==
        QStringLiteral("Line 1 and Line 3 do not share an end: pick two lines that meet."));
  CHECK(filletLinesProblem(model, QStringLiteral("c7"), QStringLiteral("c9")) ==
        QStringLiteral("Line 3 and Line 4 are in line at Point 4: there is no corner to round."));
}

// No id (p2, c5, F1), topological name (E{…}, F1:side0) or coordinate in a
// text a person reads.
bool hasIds(const QString& text) {
  static const QRegularExpression ids(
      QStringLiteral(R"(\b[pckFO][0-9]+\b|[EV]\{|\bF[0-9]+:|\bb[0-9]+\b|-?[0-9]+\.[0-9]+, -?[0-9]+\.[0-9]+)"));
  return ids.match(text).hasMatch();
}

void wordsTests() {
  const Sketch3dModel model = sketch();
  for (const auto& [first, second] :
       {std::pair<const char*, const char*>{"c3", "c7"}, {"c7", "c9"}, {"c3", "c99"}}) {
    const QString problem = filletLinesProblem(model, QString::fromLatin1(first), QString::fromLatin1(second));
    CHECK(!problem.isEmpty() && !hasIds(problem));
  }
  const FilletCorner3 b = *filletCornerAt(model, QStringLiteral("p2"));
  for (const double radius : {35.0, 0.0, -2.0}) {
    QString problem;
    CHECK(!filletArc(model, b, radius, &problem) && !problem.isEmpty() && !hasIds(problem));
  }
  // A corner whose lines the sketch lacks: "a line", no id.
  FilletCorner3 lost = b;
  lost.first = QStringLiteral("c40");
  lost.second = QStringLiteral("c41");
  QString problem;
  CHECK(!filletArc(model, lost, 35.0, &problem) && problem.contains(QStringLiteral("does not fit a line")) &&
        !hasIds(problem));
}

void arcTests() {
  const Sketch3dModel model = sketch();
  const FilletCorner3 b = *filletCornerAt(model, QStringLiteral("p2"));
  QString problem;
  const auto arc = filletArc(model, b, 10.0, &problem);
  CHECK(arc.has_value() && problem.isEmpty());
  CHECK(arc && same(arc->start, {30, 0, 0}) && same(arc->end, {40, 10, 0}) && same(arc->center, {30, 10, 0}));
  CHECK(arc && same(arc->normal, {0, 0, 1}) && same(arc->sweep, kPi / 2.0) && same(arc->setback, 10.0));
  const std::vector<Xyz> points = arc ? filletArcPoints(*arc, 4) : std::vector<Xyz>();
  CHECK(points.size() == 5 && same(points.front(), {30, 0, 0}) && same(points.back(), {40, 10, 0}));
  // 35 takes 35 mm of BC, which is 30 long; nothing, and a negative one.
  CHECK(!filletArc(model, b, 35.0, &problem) && problem.contains(QStringLiteral("does not fit Line 2")));
  CHECK(!filletArc(model, b, 0.0, &problem) && !problem.isEmpty());
  CHECK(!filletArc(model, b, -2.0, nullptr));
  CHECK(same(filletMaxRadius(b), 30.0 - 1e-6, 1e-9));

  // An obtuse corner in a slanted plane: tangent to both lines.
  FilletCorner3 slanted;
  slanted.point = QStringLiteral("p2");
  slanted.first = QStringLiteral("c1");
  slanted.second = QStringLiteral("c3");
  slanted.at = {30, 0, 10};
  slanted.firstEnd = {0, 0, 0};
  slanted.secondEnd = {45, 25.980762113533157, 10};
  const auto round = filletArc(model, slanted, 4.0, nullptr);
  CHECK(round.has_value());
  if (round) {
    const Xyz u1 = *unit(sub(slanted.at, slanted.firstEnd));
    const Xyz u2 = *unit(sub(slanted.secondEnd, slanted.at));
    CHECK(same(distance(round->center, round->start), 4.0) && same(distance(round->center, round->end), 4.0));
    CHECK(same(dot(sub(round->start, round->center), u1), 0.0));
    CHECK(same(dot(sub(round->end, round->center), u2), 0.0));
    CHECK(same(dot(round->normal, u1), 0.0) && same(dot(round->normal, u2), 0.0));
    CHECK(same(distance(slanted.at, round->start), round->setback) &&
          same(distance(slanted.at, round->end), round->setback));
    CHECK(round->sweep > 0.0 && round->sweep < kPi);
  }
}

void dragTests() {
  const Sketch3dModel model = sketch();
  const FilletCorner3 b = *filletCornerAt(model, QStringLiteral("p2"));
  // Looking down Z at (40 - 5, 5): on the bisector 5 sqrt 2 from B, the
  // middle of the arc of radius r at r (sqrt 2 - 1) from it.
  Ray3 down;
  down.from = {35, 5, 100};
  down.direction = {0, 0, -1};
  const auto radius = filletRadiusOnRay(b, down);
  CHECK(radius && same(*radius, 5.0 * std::sqrt(2.0) / (std::sqrt(2.0) - 1.0), 1e-9));
  // Outside the corner: none; far inside: kept inside the largest.
  down.from = {45, -5, 100};
  CHECK(!filletRadiusOnRay(b, down));
  down.from = {0, 40, 100};
  const auto largest = filletRadiusOnRay(b, down);
  CHECK(largest && *largest < filletMaxRadius(b) && *largest > 0.99 * filletMaxRadius(b));
  CHECK(largest && filletArc(model, b, *largest, nullptr).has_value());
}

void commandTests() {
  const Sketch3dModel model = sketch();
  const QJsonObject byPoint = filletCommandFor(QStringLiteral("F2"), *filletCornerAt(model, QStringLiteral("p2")),
                                               QJsonValue(10.0));
  CHECK(byPoint.value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.fillet"));
  CHECK(byPoint.value(QStringLiteral("sketch")).toString() == QStringLiteral("F2"));
  CHECK(byPoint.value(QStringLiteral("points")).toArray() == QJsonArray{QStringLiteral("p2")});
  CHECK(byPoint.value(QStringLiteral("radius")).toDouble() == 10.0);
  CHECK(!byPoint.contains(QStringLiteral("a")));
  const QJsonObject byLines =
      filletCommandFor(QStringLiteral("F2"), *filletCornerOfLines(model, QStringLiteral("c5"), QStringLiteral("c7")),
                       QJsonValue(QStringLiteral("bend * 2")));
  CHECK(byLines.value(QStringLiteral("a")).toString() == QStringLiteral("c5"));
  CHECK(byLines.value(QStringLiteral("b")).toString() == QStringLiteral("c7"));
  CHECK(byLines.value(QStringLiteral("radius")).toString() == QStringLiteral("bend * 2"));
  CHECK(!byLines.contains(QStringLiteral("points")));
}

} // namespace

int sketch3dFilletTests() {
  failures = 0;
  cornerTests();
  arcTests();
  dragTests();
  commandTests();
  wordsTests();
  return failures;
}
