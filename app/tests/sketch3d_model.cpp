// SPDX-License-Identifier: MIT
// A 3D sketch as its editing context reads it (mitcad#1524): a `sketch3d`
// query's answer (in the form core/model-api/src/sketch3d.rs gives it) read
// into the model, its status, details, sources and descriptions, and the
// log line; a failed sketch; a planar sketch's answer read as nothing.
#include "sketch/Sketch3dModel.hpp"

#include <cstdio>
#include <utility>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch3d_model.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

QJsonObject parse(const char* text) { return QJsonDocument::fromJson(QByteArray(text)).object(); }

// A converted-looking sketch: a route of a line and an arc, a fit-point
// spline with a free end, a reference curve of a box's edge with a point
// on it, an in-plane dimension and On Sketch Plane.
const char* const kRoute = R"({
  "uid": "F3", "name": "3DSketch1", "type": "spatial_sketch", "support": "xy",
  "frame": {"origin": [0, 0, 10], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0], "normal": [0, 0, 1]},
  "points": [
    {"id": "p1", "at": [0, 0, 0], "position": [0, 0, 10], "determined": true, "on_sketch_plane": true},
    {"id": "p2", "at": [40, 0, 0], "position": [40, 0, 10], "determined": true},
    {"id": "p4", "at": [50, 10, 0], "position": [50, 10, 10], "determined": false},
    {"id": "p5", "at": [40, 10, 0], "position": [40, 10, 10], "determined": true},
    {"id": "p7", "at": [40, 0, 14], "position": [40, 0, 24], "determined": false}
  ],
  "curves": [
    {"id": "c3", "type": "line", "start": "p1", "end": "p2", "determined": true,
     "geometry": {"start": [0, 0, 0], "end": [40, 0, 0], "length": 40}},
    {"id": "c6", "type": "arc", "center": "p5", "start": "p2", "end": "p4", "normal": [0, 0, 1],
     "determined": false, "geometry": {"normal": [0, 0, 1], "radius": 10, "sweep": 1.5707963, "length": 15.7}},
    {"id": "c8", "type": "reference", "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}"},
     "geometry": {"kind": "line", "start": [40, 0, -10], "end": [40, 0, 10]}},
    {"id": "c9", "type": "fitted_spline", "points": ["p2", "p7"], "ends": ["natural", "free"], "construction": true,
     "geometry": {"degree": 3, "knots": [], "weights": [], "control_points": [[40, 0, 0], [40, 0, 14]]}}
  ],
  "constraints": [
    {"id": "k1", "type": "attach", "point": "p1", "source": "origin", "target": [0, 0, 0]},
    {"id": "k2", "type": "point_on", "point": "p7", "curve": "c8"},
    {"id": "k10", "type": "on_sketch_plane", "points": ["p1"]},
    {"id": "k11", "type": "coincident", "a": "p2", "b": "p5", "redundant": true}
  ],
  "dimensions": [
    {"id": "k12", "type": "length", "line": "c3", "value": "d1", "driven": false, "expression": "40 mm",
     "measured": 40},
    {"id": "k13", "type": "in_plane", "in_plane": true, "driven": true, "measured": 10, "relation": {"entities": ["p4", "p5"]}}
  ],
  "references": [
    {"id": "k1", "kind": "point", "source": "origin", "mode": "include", "users": ["p1"], "status": "ok"},
    {"id": "c8", "kind": "curve", "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}",
     "link": {"source": "O1", "target": "O2"}}, "body": "F1.b0", "occurrence": "O1", "mode": "include",
     "users": ["k2"], "status": "ok"}
  ],
  "evaluated": true,
  "status": {"dof": 4, "fully_constrained": false,
             "determined": {"points": ["p1", "p2", "p5"], "lines": ["c3"], "curves": []},
             "redundant": ["k11"], "measured": {"k12": 40}},
  "warnings": ["a relation is redundant: the other relations already hold what it holds\n[placed: {\"message\": \"a relation is redundant: the other relations already hold what it holds\", \"detail\": \"k11 is redundant: the other relations already hold what it holds\"}]"]
})";

void readTests() {
  const Sketch3dModel model = Sketch3dModel::read(parse(kRoute));
  CHECK(model.uid == QStringLiteral("F3"));
  CHECK(model.name == QStringLiteral("3DSketch1"));
  CHECK(model.hasFrame);
  CHECK(model.origin[2] == 10.0);
  CHECK(model.points.size() == 5);
  CHECK(model.curves.size() == 4);
  CHECK(model.items.size() == 6); // four relations, two dimensions
  CHECK(model.references.size() == 2);
  CHECK(model.solved && model.dof == 4 && !model.fullyConstrained);
  CHECK(model.redundant == QStringList{QStringLiteral("k11")});
  CHECK(!model.empty());
  // The points a curve and a relation name, not their own ids or sources.
  const Sketch3dCurve* arc = model.curve(QStringLiteral("c6"));
  CHECK(arc != nullptr && arc->points.size() == 3 && arc->points.contains(QStringLiteral("p5")) &&
        arc->points.contains(QStringLiteral("p2")) && arc->points.contains(QStringLiteral("p4")));
  const Sketch3dCurve* reference = model.curve(QStringLiteral("c8"));
  CHECK(reference != nullptr && reference->points.isEmpty());
  const Sketch3dItem* attach = model.item(QStringLiteral("k1"));
  CHECK(attach != nullptr && attach->entities == QStringList{QStringLiteral("p1")});
  const Sketch3dItem* length = model.item(QStringLiteral("k12"));
  CHECK(length != nullptr && length->dimension && length->entities == QStringList{QStringLiteral("c3")});
  // Counts.
  CHECK(model.determinedPoints() == 3);
  CHECK(model.determinedCurves() == 1); // the reference curve is no curve of the sketch's
  CHECK(model.inPlaneItems() == QStringList{QStringLiteral("k13")});
  CHECK(model.onSketchPlanePoints() == 1);
  CHECK(model.failedReferences().isEmpty());
  // The record a point is linked by: an attachment's, or a reference
  // curve's through its Point on Curve relation.
  CHECK(model.referenceOf(QStringLiteral("p1")) != nullptr &&
        model.referenceOf(QStringLiteral("p1"))->id == QStringLiteral("k1"));
  CHECK(model.referenceOf(QStringLiteral("p7")) != nullptr &&
        model.referenceOf(QStringLiteral("p7"))->id == QStringLiteral("c8"));
  CHECK(model.referenceOf(QStringLiteral("p4")) == nullptr);
}

void textTests() {
  const Sketch3dModel model = Sketch3dModel::read(parse(kRoute));
  CHECK(statusText(model) == QStringLiteral("4 degree(s) of freedom") ||
        statusText(model) == QStringLiteral("4 degrees of freedom"));
  const QStringList details = detailLines(model);
  CHECK(details.contains(QStringLiteral("Determined: 3 of 5 points, 1 of 3 curves")));
  // Counts in words; the ids are the agents' (mitcad#1644).
  CHECK(details.contains(QStringLiteral("Redundant: 1 relation(s) or dimension(s) the others hold already")) ||
        details.contains(QStringLiteral("Redundant: 1 relation or dimension the others hold already")));
  CHECK(details.contains(QStringLiteral("In plane: 1 relation(s) or dimension(s)")) ||
        details.contains(QStringLiteral("In plane: 1 relation or dimension")));
  // The model's warning without its tail (mitcad#1644).
  CHECK(model.warnings ==
        QStringList{QStringLiteral("a relation is redundant: the other relations already hold what it holds")});
  CHECK(!details.join(QLatin1Char('\n')).contains(QStringLiteral("k11")));
  CHECK(!details.join(QLatin1Char('\n')).contains(QStringLiteral("k13")));
  CHECK(details.last() == QStringLiteral("No profiles: no closed loop lies in the sketch plane."));
  // A closed loop in the sketch plane bounds a profile (mitcad#1550).
  Sketch3dModel square = model;
  square.regions = {QStringLiteral("r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}")};
  const QStringList squareDetails = detailLines(square);
  CHECK(squareDetails.last() == QStringLiteral("Profiles: 1 region(s) in the sketch plane") ||
        squareDetails.last() == QStringLiteral("Profiles: 1 region in the sketch plane"));
  // Names in words (mitcad#1652): the kind and its running number in the
  // sketch's order, no ids.
  CHECK(entityName(model, QStringLiteral("p1")) == QStringLiteral("Point 1"));
  CHECK(entityName(model, QStringLiteral("p5")) == QStringLiteral("Point 4"));
  CHECK(entityName(model, QStringLiteral("c3")) == QStringLiteral("Line 1"));
  CHECK(entityName(model, QStringLiteral("c8")) == QStringLiteral("Reference curve 1"));
  CHECK(entityName(model, QStringLiteral("k2")) == QStringLiteral("Point on Curve 1"));
  CHECK(entityName(model, QStringLiteral("k12")) == QStringLiteral("Length 1"));
  CHECK(entityName(model, QStringLiteral("k13")) == QStringLiteral("In-plane dimension 1"));
  CHECK(entityName(model, QStringLiteral("p99")).isEmpty());
  CHECK(entityNames(model, {QStringLiteral("p2"), QStringLiteral("p99"), QStringLiteral("c6")}) ==
        QStringLiteral("Point 2, Arc 1"));
  // Sources in words: a body by its feature's name, another component's by
  // the component's name; "a body", "another sketch", "another component"
  // where no name is known, never an id.
  SourceNames names;
  names.feature = [](const QString& uid) {
    return uid == QStringLiteral("F1")   ? QStringLiteral("Box1")
           : uid == QStringLiteral("F2") ? QStringLiteral("Sketch1")
           : uid == QStringLiteral("F5") ? QStringLiteral("Intersection Curve1")
                                         : QString();
  };
  names.component = [](const QString& occurrence) {
    return occurrence == QStringLiteral("O1") || occurrence == QStringLiteral("O1/O3") ? QStringLiteral("Bracket")
                                                                                      : QString();
  };
  CHECK(sourceText(QJsonValue(QStringLiteral("origin"))) == QStringLiteral("the origin"));
  CHECK(sourceText(parse(R"({"body": "F1.b0", "edge": "E{a|b}", "at": "middle"})"), names) ==
        QStringLiteral("the middle of an edge of Box1"));
  CHECK(sourceText(parse(R"({"body": "F1.b0", "edge": "E{a|b}"})"), names) == QStringLiteral("an edge of Box1"));
  CHECK(sourceText(parse(R"({"body": "F1.b0", "edge": "E{a|b}"})")) == QStringLiteral("an edge of a body"));
  CHECK(sourceText(parse(R"({"sketch": "F2", "point": "p3"})"), names) == QStringLiteral("a point of Sketch1"));
  CHECK(sourceText(parse(R"({"sketch": "F2", "curve": "c3", "at": "start"})")) ==
        QStringLiteral("the start of a curve of another sketch"));
  CHECK(sourceText(parse(R"({"feature": "F5", "at": "end"})"), names) ==
        QStringLiteral("the end of Intersection Curve1"));
  CHECK(sourceText(parse(R"({"feature": "F5"})")) == QStringLiteral("an Intersection Curve"));
  CHECK(sourceText(parse(R"({"body": "F1.b0", "vertex": "V{a|b|c}", "link": {"source": "O1/O3"}})"), names) ==
        QStringLiteral("a vertex of Box1 in Bracket"));
  CHECK(sourceText(parse(R"({"body": "F1.b0", "vertex": "V{a|b|c}", "link": {"source": "O2"}})"), names) ==
        QStringLiteral("a vertex of Box1 in another component"));
  // A face (mitcad#1549), and its relation's name.
  CHECK(sourceText(parse(R"({"body": "F1.b0", "face": "F1:side0"})"), names) == QStringLiteral("a face of Box1"));
  CHECK(itemKindName(QStringLiteral("on_face"), false) == QStringLiteral("On Face"));
  // The agents' words of the same, with the names and ids.
  CHECK(sourceDetail(QJsonValue(QStringLiteral("origin"))) == QStringLiteral("the origin"));
  CHECK(sourceDetail(parse(R"({"body": "F1.b0", "edge": "E{a|b}", "at": "middle"})")) ==
        QStringLiteral("the middle of edge E{a|b} of F1.b0"));
  CHECK(sourceDetail(parse(R"({"sketch": "F2", "point": "p3"})")) == QStringLiteral("point p3 of F2"));
  CHECK(sourceDetail(parse(R"({"feature": "F5", "at": "end"})")) == QStringLiteral("Intersection Curve F5"));
  CHECK(sourceDetail(parse(R"({"body": "F1.b0", "vertex": "V{a|b|c}", "link": {"source": "O1/O3"}})")) ==
        QStringLiteral("vertex V{a|b|c} of F1.b0 in O1/O3"));
  CHECK(sourceDetail(parse(R"({"body": "F1.b0", "face": "F1:side0"})")) == QStringLiteral("face F1:side0 of F1.b0"));
  // Descriptions in words: no ids, topological names or coordinates; the
  // view paints where they are.
  CHECK(describeEntity(model, QStringLiteral("p2"), names) == QStringLiteral("Point 2; determined"));
  CHECK(describeEntity(model, QStringLiteral("p1"), names) ==
        QStringLiteral("Point 1; determined; On Sketch Plane; linked to the origin"));
  CHECK(describeEntity(model, QStringLiteral("p7"), names) ==
        QStringLiteral("Point 5; free; linked to an edge of Box1 in Bracket"));
  CHECK(describeEntity(model, QStringLiteral("c8"), names) ==
        QStringLiteral("Reference curve 1 (an edge of Box1 in Bracket); included in 3D"));
  CHECK(describeEntity(model, QStringLiteral("c9"), names) ==
        QStringLiteral("Fit point spline 1; its points: Point 2, Point 5; free; construction"));
  CHECK(describeEntity(model, QStringLiteral("k1"), names) ==
        QStringLiteral("Attachment 1 on Point 1; linked to the origin"));
  CHECK(describeEntity(model, QStringLiteral("k2"), names) ==
        QStringLiteral("Point on Curve 1 on Reference curve 1, Point 5"));
  CHECK(describeEntity(model, QStringLiteral("k11"), names) ==
        QStringLiteral("Coincident 1 on Point 2, Point 4; redundant"));
  CHECK(describeEntity(model, QStringLiteral("k13"), names).contains(QStringLiteral("; In plane")));
  CHECK(describeEntity(model, QStringLiteral("p99"), names).isEmpty());
  // Every point's, curve's, relation's and dimension's words, with the
  // names known and without: no id (p2, c8, k5, F1, O1), no topological
  // name (E{…}, V{…}, F1:side0), no coordinates.
  static const QRegularExpression ids(
      QStringLiteral(R"(\b[pckFO][0-9]+\b|[EV]\{|\bF[0-9]+:|\bb[0-9]+\b|-?[0-9]+\.[0-9]+, -?[0-9]+\.[0-9]+)"));
  QStringList everything;
  for (const Sketch3dPoint& point : model.points) {
    everything << point.id;
  }
  for (const Sketch3dCurve& curve : model.curves) {
    everything << curve.id;
  }
  for (const Sketch3dItem& item : model.items) {
    everything << item.id;
  }
  for (const QString& id : std::as_const(everything)) {
    for (const SourceNames& known : {names, SourceNames{}}) {
      const QString text = describeEntity(model, id, known);
      CHECK(!text.isEmpty());
      if (ids.match(text).hasMatch()) {
        std::fprintf(stderr, "sketch3d_model.cpp: %s has an id: %s\n", qPrintable(id), qPrintable(text));
        CHECK(!ids.match(text).hasMatch());
      }
    }
  }
  // The agents' descriptions keep the ids and coordinates, in the
  // document's unit.
  CHECK(entityDetail(model, QStringLiteral("p2"), QStringLiteral("mm"), 1.0) ==
        QStringLiteral("Point p2 at 40.00, 0.00, 0.00 mm; 40.00, 0.00, 10.00 mm in the component; determined"));
  CHECK(entityDetail(model, QStringLiteral("p2"), QStringLiteral("in"), 25.4)
            .startsWith(QStringLiteral("Point p2 at 1.57, 0.00, 0.00 in")));
  CHECK(entityDetail(model, QStringLiteral("p1"), QStringLiteral("mm"), 1.0)
            .contains(QStringLiteral("; On Sketch Plane; linked to the origin")));
  CHECK(entityDetail(model, QStringLiteral("c8"), QStringLiteral("mm"), 1.0) ==
        QStringLiteral("Reference curve c8: edge E{F1:side0|F1:side1} of F1.b0 in O1; included in 3D"));
  CHECK(entityDetail(model, QStringLiteral("c9"), QStringLiteral("mm"), 1.0) ==
        QStringLiteral("Fit point spline c9; points p2, p7; free; construction"));
  CHECK(entityDetail(model, QStringLiteral("k13"), QStringLiteral("mm"), 1.0).contains(QStringLiteral("; In plane")));
  CHECK(entityDetail(model, QStringLiteral("k11"), QStringLiteral("mm"), 1.0) ==
        QStringLiteral("Coincident k11; on p2, p5; redundant"));
  CHECK(entityDetail(model, QStringLiteral("p99"), QStringLiteral("mm"), 1.0).isEmpty());
  // The places a row under the pointer paints: the entity, then the source
  // it uses (another component's through its occurrence); the origin is
  // none.
  const auto place = [](const char* text) { return QJsonValue(parse(text)); };
  const QJsonArray curvePlaces = entityPlaces(model, QStringLiteral("c8"));
  CHECK(curvePlaces.size() == 2);
  CHECK(curvePlaces.size() == 2 &&
        curvePlaces.at(0) == place(R"({"kind": "sketch_curve", "sketch": "F3", "name": "c8"})") &&
        curvePlaces.at(1) == place(R"({"kind": "edge", "name": "E{F1:side0|F1:side1}", "body": "F1.b0",
                                       "occurrence": "O1"})"));
  const QJsonArray onCurve = entityPlaces(model, QStringLiteral("k2"));
  CHECK(onCurve.size() == 3 && onCurve.at(0) == place(R"({"kind": "sketch_curve", "sketch": "F3", "name": "c8"})") &&
        onCurve.at(1) == place(R"({"kind": "sketch_point", "sketch": "F3", "name": "p7"})") &&
        onCurve.at(2) == curvePlaces.at(1));
  CHECK(entityPlaces(model, QStringLiteral("k1")).size() == 1); // its point; the origin is no place
  CHECK(entityPlaces(model, QStringLiteral("p7")).size() == 2); // the point, the edge it is on
  CHECK(entityPlaces(model, QStringLiteral("p99")).isEmpty());
  CHECK(sourcePlaces(QJsonValue(QStringLiteral("origin"))).isEmpty());
  CHECK(sourcePlaces(parse(R"({"sketch": "F2", "curve": "c3", "at": "end"})")).at(0) ==
        place(R"({"kind": "sketch_curve", "sketch": "F2", "name": "c3"})"));
  CHECK(sourcePlaces(parse(R"({"feature": "F5"})")).at(0) == place(R"({"kind": "feature", "name": "F5"})"));
  CHECK(sourcePlaces(parse(R"({"body": "F1.b0", "face": "F1:side0"})")).at(0) ==
        place(R"({"kind": "face", "name": "F1:side0", "body": "F1.b0"})"));
  // The log line of the UI tests.
  CHECK(statusLog(model) ==
        QStringLiteral("3D sketch F3 (3DSketch1): 5 points, 3 curves, dof 4, determined 3 points 1 curves, "
                       "redundant k11, in plane k13, on sketch plane 1, references 2 (failed none), failure none"));
}

void failureTests() {
  // A lost source: no status, the failure and the failed record.
  const Sketch3dModel failed = Sketch3dModel::read(parse(R"({
    "uid": "F2", "name": "3DSketch1", "type": "spatial_sketch", "support": "xy", "frame": null,
    "points": [{"id": "p1", "at": [1, 2, 3]}], "curves": [], "constraints": [], "dimensions": [],
    "references": [{"id": "c4", "kind": "curve", "source": {"body": "F1.b0", "edge": "E{x|y}"},
                    "mode": "project", "users": [], "status": "failed", "message": "F1 is suppressed"}],
    "evaluated": true, "status": null,
    "failure": {"kind": "reference", "message": "F1 is suppressed", "constraints": [], "entities": ["c4"]},
    "warnings": []})"));
  CHECK(!failed.hasFrame);
  CHECK(!failed.solved && failed.dof == -1);
  CHECK(failed.failureKind == QStringLiteral("reference"));
  CHECK(statusText(failed) == QStringLiteral("Not solved: F1 is suppressed"));
  CHECK(failed.failedReferences() == QStringList{QStringLiteral("c4")});
  CHECK(detailLines(failed).contains(QStringLiteral("Involved: 1 item(s), marked in the list")) ||
        detailLines(failed).contains(QStringLiteral("Involved: 1 item, marked in the list")));
  CHECK(detailLines(failed).contains(QStringLiteral("1 linked reference(s), failed: 1")) ||
        detailLines(failed).contains(QStringLiteral("1 linked reference, failed: 1")));
  CHECK(!detailLines(failed).join(QLatin1Char('\n')).contains(QStringLiteral("c4")));
  // The failed reference's reason: in plain words, and for the agents the
  // same where the query gives no detail (mitcad#1652).
  CHECK(failed.references.size() == 1 && failed.references.front().message == QStringLiteral("F1 is suppressed") &&
        failed.references.front().detail == QStringLiteral("F1 is suppressed"));
  CHECK(statusLog(failed).endsWith(QStringLiteral("references 1 (failed c4), failure reference: F1 is suppressed")));
  // An empty new 3D sketch is empty; a planar sketch's answer is no 3D
  // sketch at all.
  const Sketch3dModel empty = Sketch3dModel::read(parse(R"({"uid": "F1", "name": "3DSketch1",
    "type": "spatial_sketch", "points": [], "curves": [], "constraints": [], "dimensions": [], "references": [],
    "evaluated": true, "status": {"dof": 0, "fully_constrained": true, "redundant": []}})"));
  CHECK(empty.empty());
  CHECK(statusText(empty) == QStringLiteral("Fully constrained"));
  const Sketch3dModel planar = Sketch3dModel::read(parse(R"({"uid": "F1", "name": "Sketch1", "type": "sketch"})"));
  CHECK(planar.uid.isEmpty() && planar.empty());
}

} // namespace

int sketch3dModelTests() {
  failures = 0;
  readTests();
  textTests();
  failureTests();
  return failures;
}
