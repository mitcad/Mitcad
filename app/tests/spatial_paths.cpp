// SPDX-License-Identifier: MIT
// Paths along 3D sketch curves in the Pipe and Sweep panels (mitcad#1527)
// and the other path inputs (mitcad#1550): the typed path written and read
// back, the direction an edited path keeps, the curves that are no path
// pieces (closed curves are paths by themselves), the route a whole-sketch
// pick takes, a closed one too, and the panel's listing of a route, open or
// closed, from answers in the form
// core/model-api/src/sketch3d.rs gives them.
#include "commands/SpatialPaths.hpp"

#include <cstdio>
#include <initializer_list>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "spatial_paths.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::cmd;

QJsonObject parse(const char* text) { return QJsonDocument::fromJson(QByteArray(text)).object(); }

const QStringList kRoute = {QStringLiteral("c3"), QStringLiteral("c6"), QStringLiteral("c9")};

void typedPathTests() {
  // Written as the model takes it: chain and reversed only when set.
  const QJsonObject plain = spatialPath(QStringLiteral("F3"), kRoute, false, false);
  CHECK(plain == parse(R"({"spatial_sketch": "F3", "curves": ["c3", "c6", "c9"]})"));
  const QJsonObject chained = spatialPath(QStringLiteral("F3"), {QStringLiteral("c6")}, true, true);
  CHECK(chained == parse(R"({"spatial_sketch": "F3", "curves": ["c6"], "chain": true, "reversed": true})"));
  // Read back; other path forms are not one.
  const auto parts = spatialPathParts(chained);
  CHECK(parts && parts->sketch == QStringLiteral("F3") && parts->curves == QStringList{QStringLiteral("c6")} &&
        parts->chain && parts->reversed);
  CHECK(!spatialPathParts(parse(R"({"sketch": "F1", "curves": ["c1"]})")));
  CHECK(!spatialPathParts(parse(R"({"body": "F2.b0", "edges": ["E{a|b}"]})")));
  CHECK(!spatialPathParts(QJsonValue()));
}

void directionTests() {
  // An edited path keeps running against its first curve while that curve
  // stays first; another first curve runs along its own direction.
  SpatialPathParts parts;
  parts.sketch = QStringLiteral("F3");
  parts.curves = {QStringLiteral("c9"), QStringLiteral("c6")};
  parts.reversed = true;
  const QJsonObject kept = keptDirection(parts);
  CHECK(kept == parse(R"({"first": "c9", "reversed": true})"));
  CHECK(reversedBy(kept, {QStringLiteral("c9")}));
  CHECK(reversedBy(kept, {QStringLiteral("c9"), QStringLiteral("c3")}));
  CHECK(!reversedBy(kept, {QStringLiteral("c3"), QStringLiteral("c9")}));
  CHECK(!reversedBy(kept, {}));
  parts.reversed = false;
  CHECK(!reversedBy(keptDirection(parts), {QStringLiteral("c9")}));
  CHECK(!reversedBy(QJsonObject(), {QStringLiteral("c9")}));
  CHECK(keptDirection(SpatialPathParts()).isEmpty());
}

// A route A-B-C-D of lines, a tangent arc and a spline, a construction
// line, a reference curve, a circle and a closed spline.
const char* const kSketch = R"({
  "uid": "F3", "name": "3DSketch1", "type": "spatial_sketch",
  "curves": [
    {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
    {"id": "c6", "type": "arc", "center": "p5", "start": "p2", "end": "p4", "normal": [0, 0, 1]},
    {"id": "c9", "type": "spline", "points": ["p4", "p7", "p8"], "degree": 2},
    {"id": "c10", "type": "line", "start": "p8", "end": "p11", "construction": true},
    {"id": "c12", "type": "reference", "source": {"body": "F1.b0", "edge": "E{a|b}"}},
    {"id": "c13", "type": "circle", "center": "p14", "normal": [0, 0, 1]},
    {"id": "c15", "type": "fitted_spline", "points": ["p16", "p17", "p18"], "closed": true},
    {"id": "c19", "type": "fitted_spline", "points": ["p20", "p21"]},
    {"id": "c22", "type": "ellipse", "center": "p23", "major": "p24", "minor_radius": 2}
  ],
  "chains": [{"curves": ["c3", "c6", "c9"], "closed": false,
              "path": {"spatial_sketch": "F3", "curves": ["c3", "c6", "c9"]}}]
})";

void pieceTests() {
  // Every curve the sketch draws, the circle, the closed spline and the
  // ellipse too (closed paths by themselves, mitcad#1550); a reference
  // curve is none.
  const QJsonObject sketch = parse(kSketch);
  for (const char* id : {"c3", "c6", "c9", "c10", "c13", "c15", "c19", "c22"}) {
    CHECK(notAPiece(spatialCurve(sketch, QString::fromLatin1(id))).isEmpty());
  }
  CHECK(notAPiece(spatialCurve(sketch, QStringLiteral("c12"))).startsWith(QStringLiteral("is a reference curve")));
  CHECK(spatialCurve(sketch, QStringLiteral("c99")).isEmpty());
}

void wholeRouteTests() {
  // One open chain: its curves from its start.
  QString why;
  CHECK(wholeRoute(parse(kSketch), &why) == kRoute);
  CHECK(why.isEmpty());
  // A closed chain (mitcad#1550): its curves round from its first.
  const QJsonObject loop = parse(R"({"name": "3DSketch1", "chains": [
    {"curves": ["c3", "c4", "c5"], "closed": true,
     "path": {"spatial_sketch": "F3", "curves": ["c3", "c4", "c5"]}}]})");
  CHECK(wholeRoute(loop, &why) == (QStringList{QStringLiteral("c3"), QStringLiteral("c4"), QStringLiteral("c5")}));
  CHECK(why.isEmpty());
  // Two chains, a branch, nothing: no whole route, and why.
  const QJsonObject two = parse(R"({"name": "3DSketch1", "chains": [
    {"curves": ["c3"], "closed": false, "path": {"spatial_sketch": "F3", "curves": ["c3"]}},
    {"curves": ["c7"], "closed": false, "path": {"spatial_sketch": "F3", "curves": ["c7"]}}]})");
  CHECK(wholeRoute(two, &why).isEmpty());
  CHECK(why == QStringLiteral("3DSketch1 has 2 separate chains: pick a curve of the one to follow"));
  const QJsonObject branched = parse(R"({"name": "3DSketch1", "chains": [
    {"curves": ["c3", "c5", "c8"], "closed": false, "branches": ["p2"]}]})");
  CHECK(wholeRoute(branched, &why).isEmpty());
  CHECK(why == QStringLiteral("3DSketch1's curves branch: pick the curves of one route"));
  CHECK(wholeRoute(parse(R"({"name": "3DSketch1", "chains": []})"), &why).isEmpty());
  CHECK(why == QStringLiteral("3DSketch1 has no curves that make a path"));
}

void listingTests() {
  QString why;
  CHECK(routeText(parse(R"({"curves": ["c3", "c6", "c9"], "forward": [true, true, true],
                            "start": "p1", "end": "p8"})"),
                  &why) == QStringLiteral("c3, c6, c9 (p1 to p8)"));
  CHECK(why.isEmpty());
  CHECK(routeText(parse(R"({"curves": ["c9", "c6", "c3"], "forward": [false, false, false],
                            "start": "p8", "end": "p1"})")) == QStringLiteral("c9, c6, c3 (p8 to p1)"));
  CHECK(routeText(parse(R"({"curves": ["c1", "c2", "c3", "c4", "c5"], "start": "p1", "end": "p9"})")) ==
        QStringLiteral("c1 ... c5, 5 curves (p1 to p9)"));
  CHECK(routeText(parse(R"({"error": "c9 does not join the others"})"), &why) == QStringLiteral("Not one path"));
  CHECK(why == QStringLiteral("c9 does not join the others"));
  // Closed (mitcad#1550): a loop from where it starts, a closed curve alone.
  CHECK(routeText(parse(R"({"curves": ["c3", "c4", "c5"], "start": "p1", "end": "p1", "closed": true})"),
                  &why) == QStringLiteral("c3, c4, c5 (closed, from p1)"));
  CHECK(why.isEmpty());
  CHECK(routeText(parse(R"({"curves": ["c13"], "start": null, "end": null, "closed": true})")) ==
        QStringLiteral("c13 (closed)"));
}

} // namespace

int spatialPathsTests() {
  failures = 0;
  typedPathTests();
  directionTests();
  pieceTests();
  wholeRouteTests();
  listingTests();
  return failures;
}
