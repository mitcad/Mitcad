// SPDX-License-Identifier: MIT
// The sketch's origin and axes as picks (mitcad#500): the pick ids, the
// fixed line and point a tool's view of the sketch gets for each the sketch
// lacks (once, however often it is asked), and the entities the model made
// for them found again instead. With no tool running (mitcad#899): what a
// click on an axis or the origin selects, the selection's items for them,
// and the picks a preselection of a point and an axis gives Coincident and
// Sketch Dimension.
#include "sketch/SketchModel.hpp"

#include <cmath>
#include <cstdio>
#include <optional>

#include <QPointF>
#include <QString>
#include <QStringList>

#include "framework/Selection.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_axes.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

const PointData* pointOf(const SketchModel& model, const QString& id) {
  for (const PointData& p : model.points) {
    if (p.id == id) {
      return &p;
    }
  }
  return nullptr;
}

const CurveData* curveOf(const SketchModel& model, const QString& id) {
  for (const CurveData& c : model.curves) {
    if (c.id == id) {
      return &c;
    }
  }
  return nullptr;
}

void pickIdTests() {
  CHECK(originPick() == QStringLiteral("origin"));
  CHECK(axisPick(QStringLiteral("x")) == QStringLiteral("x_axis"));
  CHECK(axisPick(QStringLiteral("y")) == QStringLiteral("y_axis"));
  CHECK(isAxisPick(QStringLiteral("origin")) && isAxisPick(QStringLiteral("x_axis")) &&
        isAxisPick(QStringLiteral("y_axis")));
  CHECK(!isAxisPick(QStringLiteral("c3")) && !isAxisPick(QString()) && !isAxisPick(QStringLiteral("x")));
  CHECK(axisPickName(QStringLiteral("origin")) == QStringLiteral("origin"));
  CHECK(axisPickName(QStringLiteral("x_axis")) == QStringLiteral("x"));
  CHECK(axisPickName(QStringLiteral("y_axis")) == QStringLiteral("y"));
  CHECK(axisPickName(QStringLiteral("p1")).isEmpty());
}

void emptySketchTests() {
  // A new sketch: one free point, nothing at the origin.
  SketchModel model;
  PointData free;
  free.id = QStringLiteral("p1");
  free.at = V2{12.0, 20.0};
  model.points.push_back(free);
  CHECK(model.originPoint().isEmpty());
  CHECK(model.axisLine(QStringLiteral("x")).isEmpty());
  CHECK(model.axisOf(QStringLiteral("x_axis")) == QStringLiteral("x"));
  CHECK(model.isAxisOrOrigin(QStringLiteral("origin")));
  CHECK(!model.isAxisOrOrigin(QStringLiteral("p1")) && !model.isAxisOrOrigin(QString()));

  model.addAxisPicks();
  // The origin as a fixed point, each axis as a fixed construction line
  // from it along the axis with its far end.
  CHECK(model.points.size() == 4 && model.curves.size() == 2);
  const PointData* origin = pointOf(model, QStringLiteral("origin"));
  CHECK(origin != nullptr && origin->fixed && norm(origin->at) < 1e-12 && origin->axis.isEmpty());
  CHECK(model.originPoint() == QStringLiteral("origin"));
  const CurveData* x = curveOf(model, QStringLiteral("x_axis"));
  const CurveData* y = curveOf(model, QStringLiteral("y_axis"));
  CHECK(x != nullptr && y != nullptr);
  if (x != nullptr && y != nullptr) {
    CHECK(x->isLine() && x->construction && x->fixed && x->axis == QStringLiteral("x"));
    CHECK(x->start == QStringLiteral("origin") && y->start == QStringLiteral("origin"));
    CHECK(dist(x->curve.a, V2{}) < 1e-12 && dist(x->curve.b, V2{kAxisLineLength, 0.0}) < 1e-12);
    CHECK(dist(y->curve.b, V2{0.0, kAxisLineLength}) < 1e-12);
    // A point's distance from the line, as a tool measures it.
    CHECK(std::abs(x->curve.distanceTo(V2{30.0, 30.0}) - 30.0) < 1e-9);
    const PointData* far = pointOf(model, x->end);
    CHECK(far != nullptr && far->axis == QStringLiteral("x") && far->fixed);
  }
  CHECK(model.axisLine(QStringLiteral("x")) == QStringLiteral("x_axis"));
  CHECK(model.axisOf(QStringLiteral("y_axis")) == QStringLiteral("y"));
  CHECK(model.axisOf(QStringLiteral("p1")).isEmpty());
  CHECK(model.isAxisOrOrigin(QStringLiteral("x_axis")) && model.isAxisOrOrigin(QStringLiteral("origin")));
  // The far ends are not the origin.
  CHECK(!model.isAxisOrOrigin(QStringLiteral("p1")));

  // Asked again: nothing more.
  model.addAxisPicks();
  CHECK(model.points.size() == 4 && model.curves.size() == 2);
}

void madeAxesTests() {
  // The model made the origin point p1 (a snap, mitcad#355) and the x axis
  // c2 with its far end p3; the y axis is still to be made.
  SketchModel model;
  PointData origin;
  origin.id = QStringLiteral("p1");
  origin.fixed = true;
  model.points.push_back(origin);
  PointData far;
  far.id = QStringLiteral("p3");
  far.at = V2{kAxisLineLength, 0.0};
  far.fixed = true;
  far.axis = QStringLiteral("x");
  model.points.push_back(far);
  CurveData line;
  line.id = QStringLiteral("c2");
  line.type = QStringLiteral("line");
  line.curve = Curve2::line(V2{}, far.at);
  line.start = QStringLiteral("p1");
  line.end = QStringLiteral("p3");
  line.points << line.start << line.end;
  line.construction = true;
  line.fixed = true;
  line.axis = QStringLiteral("x");
  model.curves.push_back(line);
  // A fixed point of the user's elsewhere.
  PointData fixed;
  fixed.id = QStringLiteral("p4");
  fixed.at = V2{5.0, 5.0};
  fixed.fixed = true;
  model.points.push_back(fixed);

  CHECK(model.originPoint() == QStringLiteral("p1"));
  CHECK(model.axisLine(QStringLiteral("x")) == QStringLiteral("c2"));
  CHECK(model.axisLine(QStringLiteral("y")).isEmpty());
  CHECK(model.axisOf(QStringLiteral("c2")) == QStringLiteral("x"));
  CHECK(model.isAxisOrOrigin(QStringLiteral("p1")) && model.isAxisOrOrigin(QStringLiteral("c2")));
  CHECK(!model.isAxisOrOrigin(QStringLiteral("p4")));

  model.addAxisPicks();
  // Only the y axis, from the origin point there is.
  CHECK(model.points.size() == 4 && model.curves.size() == 2);
  CHECK(pointOf(model, QStringLiteral("origin")) == nullptr);
  CHECK(curveOf(model, QStringLiteral("x_axis")) == nullptr);
  const CurveData* y = curveOf(model, QStringLiteral("y_axis"));
  CHECK(y != nullptr && y->start == QStringLiteral("p1") && y->axis == QStringLiteral("y"));
  CHECK(model.axisLine(QStringLiteral("x")) == QStringLiteral("c2"));
}

// A new sketch with one free point p1 at (12, 20).
SketchModel freePointSketch() {
  SketchModel model;
  PointData free;
  free.id = QStringLiteral("p1");
  free.at = V2{12.0, 20.0};
  model.points.push_back(free);
  return model;
}

// A click with no tool running (mitcad#899): how far it is from an axis
// across the view, and what it selects of what entityAt() found there.
void clickTests() {
  // The view: the origin at (400, 300), 1 px per mm, y up the screen.
  const QPointF origin(400.0, 300.0);
  const QPointF xAlong(100.0, 0.0);
  const QPointF yAlong(0.0, -100.0);
  const auto about = [](std::optional<double> d, double want) { return d.has_value() && std::abs(*d - want) < 1e-9; };
  CHECK(about(axisScreenDistance(QPointF(520.0, 302.0), origin, xAlong), 2.0));
  // Across the view: beyond the line the model would make.
  CHECK(about(axisScreenDistance(QPointF(-900.0, 297.0), origin, xAlong), 3.0));
  CHECK(about(axisScreenDistance(QPointF(400.0, 250.0), origin, xAlong), 50.0));
  CHECK(about(axisScreenDistance(QPointF(403.0, 40.0), origin, yAlong), 3.0));
  // The sketch plane seen edge on: no axis to click.
  CHECK(!axisScreenDistance(QPointF(400.0, 300.0), origin, QPointF(0.5, 0.0)).has_value());

  // A new sketch: entityAt() gives the picks there.
  const SketchModel fresh = freePointSketch();
  CHECK(fresh.axisSelectionPick(QStringLiteral("x_axis")) == QStringLiteral("x_axis"));
  CHECK(fresh.axisSelectionPick(QStringLiteral("y_axis")) == QStringLiteral("y_axis"));
  CHECK(fresh.axisSelectionPick(QStringLiteral("origin")) == QStringLiteral("origin"));
  // The sketch's own point, and nothing: the view's to pick.
  CHECK(fresh.axisSelectionPick(QStringLiteral("p1")).isEmpty());
  CHECK(fresh.axisSelectionPick(QString()).isEmpty());

  // The model made the x axis c2 and the origin point p1: a click on the
  // line still selects the axis by its pick id; the origin point is a
  // point of the sketch.
  SketchModel made;
  PointData originPoint;
  originPoint.id = QStringLiteral("p1");
  originPoint.fixed = true;
  made.points.push_back(originPoint);
  CurveData line;
  line.id = QStringLiteral("c2");
  line.type = QStringLiteral("line");
  line.curve = Curve2::line(V2{}, V2{kAxisLineLength, 0.0});
  line.start = QStringLiteral("p1");
  line.end = QStringLiteral("p3");
  line.axis = QStringLiteral("x");
  made.curves.push_back(line);
  CHECK(made.axisSelectionPick(QStringLiteral("c2")) == QStringLiteral("x_axis"));
  CHECK(made.axisSelectionPick(QStringLiteral("p1")).isEmpty());
  CHECK(made.axisEntity(QStringLiteral("x_axis")) == QStringLiteral("c2"));
  CHECK(made.axisEntity(QStringLiteral("origin")) == QStringLiteral("p1"));
  CHECK(made.axisEntity(QStringLiteral("y_axis")) == QStringLiteral("y_axis"));
  CHECK(made.axisEntity(QStringLiteral("c2")) == QStringLiteral("c2"));

  // The selection's items: an axis a sketch line, the origin a sketch
  // point, by their pick ids.
  const mitcad::SelectionItem y = axisSelectionItem(QStringLiteral("F1"), QStringLiteral("y_axis"), QString());
  CHECK(y.kind == mitcad::SelectKind::SketchCurve && y.owner == QStringLiteral("F1"));
  CHECK(y.name == QStringLiteral("y_axis") && y.geometry == QStringLiteral("line"));
  CHECK(isAxisSelection(y));
  const mitcad::SelectionItem o = axisSelectionItem(QStringLiteral("F1"), QStringLiteral("origin"), QStringLiteral("O1"));
  CHECK(o.kind == mitcad::SelectKind::SketchPoint && o.name == QStringLiteral("origin"));
  CHECK(o.geometry == QStringLiteral("point") && o.occurrence == QStringLiteral("O1"));
  CHECK(isAxisSelection(o));
  // The sketch's own entities are not.
  mitcad::SelectionItem own;
  own.kind = mitcad::SelectKind::SketchPoint;
  own.owner = QStringLiteral("F1");
  own.name = QStringLiteral("p1");
  CHECK(!isAxisSelection(own));
  mitcad::SelectionItem edge;
  edge.kind = mitcad::SelectKind::Edge;
  edge.name = QStringLiteral("x_axis");
  CHECK(!isAxisSelection(edge));
}

// A preselected point and axis (mitcad#899): the picks the constraint
// tools and Sketch Dimension take, and the values they decide on in the
// sketch with the picks added.
void preselectionTests() {
  const SketchModel fresh = freePointSketch();
  const QStringList coincident =
      fresh.selectionPicks({QStringLiteral("p1"), QStringLiteral("y_axis")}, true);
  CHECK(coincident == QStringList({QStringLiteral("p1"), QStringLiteral("y_axis")}));
  // Fix/Unfix: the axes left out.
  CHECK(fresh.selectionPicks({QStringLiteral("p1"), QStringLiteral("y_axis")}, false) ==
        (QStringList() << QStringLiteral("p1")));
  CHECK(fresh.selectionPicks({QStringLiteral("origin")}, false).isEmpty());

  SketchModel view = fresh;
  view.addAxisPicks();
  // Coincident: a point and a line, one of them the sketch's axes; the
  // point goes onto the y axis, 12 mm from it now.
  const PointData* point = pointOf(view, coincident[0]);
  const CurveData* axis = curveOf(view, coincident[1]);
  CHECK(point != nullptr && axis != nullptr);
  if (point != nullptr && axis != nullptr) {
    CHECK(axis->isLine() && axis->axis == QStringLiteral("y"));
    CHECK(!view.isAxisOrOrigin(coincident[0]) && view.isAxisOrOrigin(coincident[1]));
    CHECK(std::abs(axis->curve.distanceTo(point->at) - 12.0) < 1e-9);
  }
  // Sketch Dimension: the point and the x axis give the point's distance
  // from the axis, its y coordinate.
  const QStringList dimension = fresh.selectionPicks({QStringLiteral("p1"), QStringLiteral("x_axis")}, true);
  const CurveData* x = curveOf(view, dimension.value(1));
  CHECK(x != nullptr && x->isLine() && x->axis == QStringLiteral("x"));
  if (x != nullptr) {
    CHECK(std::abs(x->curve.distanceTo(V2{12.0, 20.0}) - 20.0) < 1e-9);
  }
  // The origin and the point: the distances between two points.
  const QStringList distance = fresh.selectionPicks({QStringLiteral("origin"), QStringLiteral("p1")}, true);
  const PointData* at = pointOf(view, distance.value(0));
  CHECK(at != nullptr && norm(at->at) < 1e-12);
  if (at != nullptr) {
    CHECK(std::abs(dist(at->at, V2{12.0, 20.0}) - std::hypot(12.0, 20.0)) < 1e-9);
  }

  // The model made the x axis c2 and the origin point p1 since the
  // selection: the picks are those entities; the y axis is still a pick.
  SketchModel made;
  PointData originPoint;
  originPoint.id = QStringLiteral("p1");
  originPoint.fixed = true;
  made.points.push_back(originPoint);
  CurveData line;
  line.id = QStringLiteral("c2");
  line.type = QStringLiteral("line");
  line.axis = QStringLiteral("x");
  made.curves.push_back(line);
  CHECK(made.selectionPicks({QStringLiteral("p4"), QStringLiteral("x_axis"), QStringLiteral("origin"),
                             QStringLiteral("y_axis")},
                            true) == QStringList({QStringLiteral("p4"), QStringLiteral("c2"), QStringLiteral("p1"),
                                                  QStringLiteral("y_axis")}));
}

// The direction a line input such as Rectangular Pattern's Direction takes
// (mitcad#1655): a sketch of a circle alone has the axes' picks, a line
// its own way from start to end, the model's x axis line its axis.
void lineDirectionTests() {
  SketchModel model;
  CurveData circle;
  circle.id = QStringLiteral("c1");
  circle.type = QStringLiteral("circle");
  model.curves.push_back(circle);
  const auto runs = [](const std::optional<V2>& got, V2 wanted) {
    return got.has_value() && dist(*got, wanted) < 1e-12;
  };
  CHECK(runs(model.lineDirection(QStringLiteral("x_axis")), V2{1.0, 0.0}));
  CHECK(runs(model.lineDirection(QStringLiteral("y_axis")), V2{0.0, 1.0}));
  CHECK(!model.lineDirection(QStringLiteral("c1")).has_value());
  CHECK(!model.lineDirection(QStringLiteral("origin")).has_value());
  CHECK(!model.lineDirection(QStringLiteral("c9")).has_value());

  CurveData slanted;
  slanted.id = QStringLiteral("c2");
  slanted.type = QStringLiteral("line");
  slanted.curve = Curve2::line(V2{10.0, 10.0}, V2{7.0, 14.0});
  model.curves.push_back(slanted);
  CHECK(runs(model.lineDirection(QStringLiteral("c2")), V2{-0.6, 0.8}));
  CurveData point;
  point.id = QStringLiteral("c3");
  point.type = QStringLiteral("line");
  point.curve = Curve2::line(V2{5.0, 5.0}, V2{5.0, 5.0});
  model.curves.push_back(point);
  CHECK(!model.lineDirection(QStringLiteral("c3")).has_value());

  // The y axis the model made, drawn here from its far end: still the
  // axis's own way.
  CurveData made;
  made.id = QStringLiteral("c4");
  made.type = QStringLiteral("line");
  made.curve = Curve2::line(V2{0.0, kAxisLineLength}, V2{});
  made.axis = QStringLiteral("y");
  model.curves.push_back(made);
  CHECK(runs(model.lineDirection(QStringLiteral("c4")), V2{0.0, 1.0}));
}

} // namespace

int sketchAxesTests() {
  failures = 0;
  pickIdTests();
  emptySketchTests();
  madeAxesTests();
  clickTests();
  preselectionTests();
  lineDirectionTests();
  return failures;
}
