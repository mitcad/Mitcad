// SPDX-License-Identifier: MIT
// Sketch Move/Copy's places (mitcad#1563): a place the sketch's snaps
// found where no point is goes into Point to Point's input as a pick id
// that gives the same place back to the last bit; a point's id is its
// place; the objects' middle, where Translate's handle sits and Rotate
// turns without a Pivot Point, is the center of the box round their
// points with a circle's whole round.
#include "sketch/SketchModel.hpp"

#include <cstdio>

#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_places.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

bool same(const std::optional<V2>& a, const V2& b) {
  return a && a->x == b.x && a->y == b.y;
}

PointData pointAt(const char* id, double x, double y) {
  PointData p;
  p.id = QString::fromLatin1(id);
  p.at = V2{x, y};
  return p;
}

// A line c1 from p2 (-25, 10) to p3 (-5, 10), a circle c4 of radius 3
// about p5 (20, -10) and a free point p6 (40, 30).
SketchModel moveSketch() {
  SketchModel model;
  model.uid = QStringLiteral("F1");
  model.points = {pointAt("p2", -25, 10), pointAt("p3", -5, 10), pointAt("p5", 20, -10), pointAt("p6", 40, 30)};
  CurveData line;
  line.id = QStringLiteral("c1");
  line.type = QStringLiteral("line");
  line.curve = Curve2::line(V2{-25, 10}, V2{-5, 10});
  line.start = QStringLiteral("p2");
  line.end = QStringLiteral("p3");
  line.points = {line.start, line.end};
  CurveData circle;
  circle.id = QStringLiteral("c4");
  circle.type = QStringLiteral("circle");
  circle.curve = Curve2::circle(V2{20, -10}, 3);
  circle.center = QStringLiteral("p5");
  circle.points = {circle.center};
  model.curves = {line, circle};
  return model;
}

void placeTests() {
  // A place round-trips exactly, also one no decimal writes exactly.
  const V2 middle{-15.0, 10.0};
  CHECK(placePick(middle) == QStringLiteral("@-15,10"));
  CHECK(same(pickedPlace(placePick(middle)), middle));
  const V2 third{1.0 / 3.0, -2.0 / 3.0};
  CHECK(same(pickedPlace(placePick(third)), third));
  // Point ids and other ids are no places.
  CHECK(!pickedPlace(QStringLiteral("p3")));
  CHECK(!pickedPlace(QStringLiteral("@1")));
  CHECK(!pickedPlace(QStringLiteral("@a,b")));
  CHECK(!pickedPlace(QString()));
  // A pick's place: a point's own, or the place's.
  const SketchModel model = moveSketch();
  CHECK(same(pickPlace(model, QStringLiteral("p5")), V2{20, -10}));
  CHECK(same(pickPlace(model, placePick(middle)), middle));
  CHECK(!pickPlace(model, QStringLiteral("p9")));
  CHECK(!pickPlace(model, QStringLiteral("c1")));
}

void middleTests() {
  const SketchModel model = moveSketch();
  // A line's middle.
  CHECK(same(middleOf(model, {QStringLiteral("c1")}), V2{-15, 10}));
  // A circle's center, its round included.
  CHECK(same(middleOf(model, {QStringLiteral("c4")}), V2{20, -10}));
  // Together: the box from (-25, -13) to (23, 10).
  CHECK(same(middleOf(model, {QStringLiteral("c1"), QStringLiteral("c4")}), V2{-1, -1.5}));
  // Points too; ids the sketch lacks are left out.
  CHECK(same(middleOf(model, {QStringLiteral("p6"), QStringLiteral("p3"), QStringLiteral("t9")}), V2{17.5, 20}));
  CHECK(!middleOf(model, {}));
  CHECK(!middleOf(model, {QStringLiteral("c9")}));
}

} // namespace

int sketchPlacesTests() {
  failures = 0;
  placeTests();
  middleTests();
  return failures;
}
