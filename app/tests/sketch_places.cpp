// SPDX-License-Identifier: MIT
// Sketch Move/Copy's places (mitcad#1563): a place the sketch's snaps
// found where no point is goes into Point to Point's input as a pick id
// that gives the same place back to the last bit; a point's id is its
// place; the objects' middle, where Translate's handle sits and Rotate
// turns without a Pivot Point, is the center of the box round their
// points with a circle's whole round. Texts (mitcad#1911): the objects'
// texts for the model's `texts`, a text on a path named for the panel's
// refusal, a text's anchor, and Edit Text's Position shown and sent back.
#include "UnitTests.hpp"
#include "sketch/SketchModel.hpp"

#include <cmath>
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

// mitcad#1911: moveSketch with a text t7 at (10, 5), a text t8 in a frame
// of p2, p3 and p6, a QR code t9 at (-30, -20) and a text t10 on the line
// c1.
SketchModel textSketch() {
  SketchModel model = moveSketch();
  TextData placed;
  placed.id = QStringLiteral("t7");
  placed.text = QStringLiteral("AB");
  placed.at = V2{10, 5};
  TextData framed;
  framed.id = QStringLiteral("t8");
  framed.text = QStringLiteral("Frame");
  framed.at = V2{-25, 10};
  framed.frame = {QStringLiteral("p2"), QStringLiteral("p3"), QStringLiteral("p6")};
  TextData code;
  code.id = QStringLiteral("t9");
  code.text = QStringLiteral("M");
  code.at = V2{-30, -20};
  code.code = true;
  TextData onPath;
  onPath.id = QStringLiteral("t10");
  onPath.text = QStringLiteral("Path");
  onPath.path = QStringLiteral("c1");
  model.texts = {placed, framed, code, onPath};
  return model;
}

void moveTextTests() {
  const SketchModel model = textSketch();
  // Move/Copy's objects: texts and codes for the model's `texts`, the
  // rest for its `entities`, each once, in their order.
  const MoveObjects split = moveObjects(
      model, {QStringLiteral("c1"), QStringLiteral("t7"), QStringLiteral("p6"), QStringLiteral("t9"),
              QStringLiteral("t7")});
  CHECK(split.entities == (QStringList{QStringLiteral("c1"), QStringLiteral("p6")}));
  CHECK(split.texts == (QStringList{QStringLiteral("t7"), QStringLiteral("t9")}));
  // A text on a path is named, so the panel refuses it.
  CHECK(textOnPath(model, {QStringLiteral("t7"), QStringLiteral("t9")}).isEmpty());
  CHECK(textOnPath(model, {QStringLiteral("c1"), QStringLiteral("t10")}) == QStringLiteral("t10"));
  // Anchors: a text's at, a framed text's corner, none on a path.
  CHECK(same(textAnchor(model, QStringLiteral("t7")), V2{10, 5}));
  CHECK(same(textAnchor(model, QStringLiteral("t8")), V2{-25, 10}));
  CHECK(same(textAnchor(model, QStringLiteral("t9")), V2{-30, -20}));
  CHECK(!textAnchor(model, QStringLiteral("t10")));
  CHECK(!textAnchor(model, QStringLiteral("c1")));
  // The middle counts a text by its anchor, a framed one by its corners:
  // t7 alone; t7 and the code from (-30, -20) to (10, 5); the frame from
  // (-25, 10) to (40, 30).
  CHECK(same(middleOf(model, {QStringLiteral("t7")}), V2{10, 5}));
  CHECK(same(middleOf(model, {QStringLiteral("t7"), QStringLiteral("t9")}), V2{-10, -7.5}));
  CHECK(same(middleOf(model, {QStringLiteral("t8")}), V2{7.5, 20}));
  CHECK(!middleOf(model, {QStringLiteral("t10")}));
}

void textPositionTests() {
  const SketchModel model = textSketch();
  const TextData& placed = model.texts.at(0);
  const TextData& framed = model.texts.at(1);
  const TextData& code = model.texts.at(2);
  const TextData& onPath = model.texts.at(3);
  // Edit Text shows a free text's position, and Edit QR Code a code's.
  CHECK(same(freeTextPosition(placed), V2{10, 5}));
  CHECK(same(freeTextPosition(code), V2{-30, -20}));
  CHECK(!freeTextPosition(framed));
  CHECK(!freeTextPosition(onPath));
  // The position shown comes back unchanged: nothing is sent.
  const V2 shown = *freeTextPosition(placed);
  CHECK(!changedTextPosition(placed, shown.x, shown.y));
  // As the panel shows it to 12 digits, still unchanged.
  CHECK(!changedTextPosition(placed, QString::number(10.0 / 25.4, 'g', 12).toDouble() * 25.4, 5.0));
  // A typed one is sent; a text in a frame or on a path has none, nor
  // does a field without a value.
  CHECK(same(changedTextPosition(placed, -12.5, 7.25), V2{-12.5, 7.25}));
  CHECK(same(changedTextPosition(code, -30, -10), V2{-30, -10}));
  CHECK(!changedTextPosition(framed, 0, 0));
  CHECK(!changedTextPosition(onPath, 0, 0));
  CHECK(!changedTextPosition(placed, std::nan(""), 5));
}

} // namespace

int sketchPlacesTests() {
  failures = 0;
  placeTests();
  middleTests();
  moveTextTests();
  textPositionTests();
  return failures;
}
MITCAD_UNIT_TEST(sketchPlacesTests);
