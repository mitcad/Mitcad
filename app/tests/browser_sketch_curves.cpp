// SPDX-License-Identifier: MIT
// Sketch curves picked in the Browser (mitcad#1978; browser/SketchCurveRows,
// BrowserPanel::setSketchCurves): a sketch's curves read from the model's
// `sketch` and `sketch3d` answers, named in words and numbered per kind in
// the sketch's order, without points, texts, axis lines or curves without
// geometry; their rows under the sketch's row stand for the view's pick of
// the curve, are kept over a rebuild, go when the map is emptied, and the
// pointer over one says which curve to paint.
#include "UnitTests.hpp"
#include "browser/BrowserPanel.hpp"
#include "browser/SketchCurveRows.hpp"

#include <cstdio>

#include <QByteArray>
#include <QHash>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_sketch_curves.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject parse(const char* json) { return QJsonDocument::fromJson(QByteArray(json)).object(); }

// A planar sketch's answer: a point, two lines, a centre line, an arc, the
// line of a sketch axis, a construction circle, a line without geometry,
// a fit-point spline and a text.
const char* const kSketch = R"({
  "uid": "F3", "name": "Cross",
  "entities": [
    {"id": "p1", "type": "point", "at": [0, 0]},
    {"id": "c2", "type": "line", "start": "p1", "end": "p9", "geometry": {"start": [0, 0], "end": [1, 0]}},
    {"id": "c3", "type": "line", "centerline": true, "geometry": {"start": [0, 0], "end": [0, 1]}},
    {"id": "c4", "type": "arc", "geometry": {"center": [0, 0], "radius": 2}},
    {"id": "c5", "type": "line", "axis": "x", "geometry": {"start": [0, 0], "end": [9, 0]}},
    {"id": "c6", "type": "circle", "construction": true, "geometry": {"center": [0, 0], "radius": 3}},
    {"id": "c7", "type": "line"},
    {"id": "c8", "type": "fitted_spline", "geometry": {"degree": 3}},
    {"id": "c9", "type": "line", "geometry": {"start": [1, 1], "end": [2, 2]}}
  ],
  "texts": [{"id": "t1", "outline": []}]
})";

void readTests() {
  const QVector<BrowserSketchCurve> rows = sketchCurveRows(parse(kSketch));
  CHECK(rows.size() == 6);
  if (rows.size() != 6) {
    return;
  }
  CHECK(rows.at(0).id == QStringLiteral("c2") && rows.at(0).name == QStringLiteral("Line 1"));
  CHECK(rows.at(0).geometry == QStringLiteral("line") && rows.at(0).icon == QStringLiteral("line"));
  CHECK(rows.at(1).id == QStringLiteral("c3") && rows.at(1).name == QStringLiteral("Centerline 1"));
  CHECK(rows.at(1).icon == QStringLiteral("centerline"));
  CHECK(rows.at(2).id == QStringLiteral("c4") && rows.at(2).name == QStringLiteral("Arc 1"));
  CHECK(rows.at(3).id == QStringLiteral("c6") && rows.at(3).name == QStringLiteral("Circle 1"));
  CHECK(rows.at(3).construction && !rows.at(0).construction);
  CHECK(rows.at(4).id == QStringLiteral("c8") && rows.at(4).name == QStringLiteral("Fit Point Spline 1"));
  // Numbered per kind: the second line is Line 2 (the axis line and the
  // one without geometry are not counted).
  CHECK(rows.at(5).id == QStringLiteral("c9") && rows.at(5).name == QStringLiteral("Line 2"));
  // No row names a curve by its id.
  for (const BrowserSketchCurve& row : rows) {
    CHECK(!row.name.contains(row.id));
  }

  // A 3D sketch's curves, every one with an id and a type.
  const QVector<BrowserSketchCurve> spatial = spatialSketchCurveRows(parse(R"({
    "uid": "F5", "points": [{"id": "p1"}],
    "curves": [{"id": "c2", "type": "line"}, {"id": "c3", "type": "arc"},
               {"id": "c4", "type": "reference"}, {"id": "c5", "type": "line", "construction": true},
               {"type": "line"}]
  })"));
  CHECK(spatial.size() == 4);
  if (spatial.size() == 4) {
    CHECK(spatial.at(0).name == QStringLiteral("Line 1") && spatial.at(1).name == QStringLiteral("Arc 1"));
    CHECK(spatial.at(2).name == QStringLiteral("Reference Curve 1"));
    CHECK(spatial.at(3).name == QStringLiteral("Line 2") && spatial.at(3).construction);
  }
  CHECK(sketchCurveKind(QStringLiteral("something_new")) == QStringLiteral("Curve"));
}

DocumentSnapshot::Feature sketchFeature(const QString& uid, const QString& name, int index) {
  DocumentSnapshot::Feature made;
  made.uid = uid;
  made.name = name;
  made.type = QStringLiteral("sketch");
  made.status = QStringLiteral("ok");
  made.component = QStringLiteral("C0");
  made.index = index;
  return made;
}

// The root (C0) with Sketch1 (F1) and Cross (F3).
DocumentSnapshot design() {
  DocumentSnapshot snapshot;
  DocumentSnapshot::Component root;
  root.uid = QStringLiteral("C0");
  root.name = QStringLiteral("Root");
  snapshot.components.append(root);
  snapshot.features.append(sketchFeature(QStringLiteral("F1"), QStringLiteral("Sketch1"), 0));
  snapshot.features.append(sketchFeature(QStringLiteral("F3"), QStringLiteral("Cross"), 1));
  snapshot.marker = 2;
  snapshot.activeComponent = QStringLiteral("C0");
  return snapshot;
}

QHash<QString, BrowserPanel::PresentationRow> rowsByPath(const BrowserPanel& panel) {
  QHash<QString, BrowserPanel::PresentationRow> rows;
  for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
    rows.insert(row.node.path, row);
  }
  return rows;
}

void rowTests() {
  BrowserPanel panel;
  const DocumentSnapshot snapshot = design();
  panel.rebuild(snapshot, false);
  QHash<QString, BrowserPanel::PresentationRow> rows = rowsByPath(panel);
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Cross")));
  CHECK(!rows.contains(QStringLiteral("Root/Sketches/Cross/Line 1")));

  // An input takes sketch curves: Cross's curves under its row, in place.
  QHash<QString, QVector<BrowserSketchCurve>> curves;
  curves.insert(QStringLiteral("F3"), sketchCurveRows(parse(kSketch)));
  panel.setSketchCurves(curves);
  rows = rowsByPath(panel);
  const BrowserPanel::PresentationRow line = rows.value(QStringLiteral("Root/Sketches/Cross/Line 1"));
  CHECK(line.node.type == BrowserNode::Type::SketchCurve);
  CHECK(line.parent == rows.value(QStringLiteral("Root/Sketches/Cross")).key);
  CHECK(!line.expanded);
  // The view's pick of the curve.
  const SelectionItem item = line.node.item();
  CHECK(item.kind == SelectKind::SketchCurve);
  CHECK(item.owner == QStringLiteral("F3") && item.name == QStringLiteral("c2"));
  CHECK(item.geometry == QStringLiteral("line"));
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Cross/Line 2")));
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Cross/Arc 1")));
  // Named in words: neither the name nor the tooltip has the curve's id.
  CHECK(!line.tooltip.contains(QStringLiteral("c2")) && line.tooltip.contains(QStringLiteral("Cross")));
  // Sketch1 has none listed.
  int sketch1Children = 0;
  for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
    sketch1Children += row.parent == rows.value(QStringLiteral("Root/Sketches/Sketch1")).key ? 1 : 0;
  }
  CHECK(sketch1Children == 0);

  // The pointer over the row names that curve; leaving it, none.
  BrowserNode hovered;
  int hovers = 0;
  QObject::connect(&panel, &BrowserPanel::curveHovered, [&hovered, &hovers](const BrowserNode& node) {
    hovered = node;
    ++hovers;
  });
  panel.hoverNode(line.node);
  CHECK(hovers == 1 && hovered.type == BrowserNode::Type::SketchCurve && hovered.curve == QStringLiteral("c2"));
  panel.hoverNode(line.node); // the same row: no new signal
  CHECK(hovers == 1);
  panel.hoverNode(rows.value(QStringLiteral("Root/Sketches/Cross")).node);
  CHECK(hovers == 2 && !hovered.isValid());

  // Kept over a rebuild.
  panel.rebuild(snapshot, false);
  CHECK(rowsByPath(panel).contains(QStringLiteral("Root/Sketches/Cross/Arc 1")));

  // A hovered row that goes takes its paint with it.
  panel.hoverNode(rowsByPath(panel).value(QStringLiteral("Root/Sketches/Cross/Arc 1")).node);
  CHECK(hovers == 3 && hovered.curve == QStringLiteral("c4"));
  panel.setSketchCurves({});
  CHECK(hovers == 4 && !hovered.isValid());
  rows = rowsByPath(panel);
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Cross")));
  CHECK(!rows.contains(QStringLiteral("Root/Sketches/Cross/Line 1")));
  CHECK(panel.sketchCurves().isEmpty());
}

} // namespace

int browserSketchCurvesTests() {
  readTests();
  rowTests();
  return failures;
}
MITCAD_UNIT_TEST(browserSketchCurvesTests);
