// SPDX-License-Identifier: MIT
// The 3D sketch's drawing tools without the view (mitcad#1525): where a
// pointer's ray places a point on the temporary plane, along an axis and
// along the plane's normal, and when that is unstable (edge-on, end-on,
// behind the eye); the snap priority (discrete before continuous before
// guidance, nearest, ties by kind and this sketch first); typed Offsets
// with a field left free and an explicit zero, Axis and Length, Keep Axis,
// and the problems and snap conflicts they give; the arcs' geometry
// (three points, center, tangent with a typed radius and arc length, Keep
// Plane); the sketch3d commands and follow-ups the tools send; and the
// manipulator's free rotation (mitcad#1661): the frame after a turn about
// each axis, the placement on the rotated plane, along a rotated axis and
// its normal, the handles' rings and the angle a drag turns, and the reset.
#include "sketch/Sketch3dDraw.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch3d_draw.cpp:%d: check failed: %s\n", line, expression);
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

Ray3 ray(const Xyz& from, const Xyz& direction) {
  Ray3 r;
  r.from = from;
  r.direction = *unit(direction);
  return r;
}

SnapTarget target(SnapKind kind, double pixels, bool own = false) {
  SnapTarget t;
  t.kind = kind;
  t.pixels = pixels;
  t.own = own;
  return t;
}

TypedValue typed(double value, const QString& expression) {
  TypedValue v;
  v.edited = true;
  v.text = expression;
  v.expression = expression;
  v.value = value;
  v.valid = true;
  return v;
}

PlacedPoint loose(const Xyz& at) {
  PlacedPoint p;
  p.at = at;
  p.snap.at = at;
  return p;
}

PlacedPoint sharedPoint(const QString& id, const Xyz& at) {
  PlacedPoint p = loose(at);
  p.snap.kind = SnapKind::Point;
  p.snap.point = id;
  return p;
}

void placementTests() {
  const Xyz origin{0.0, 0.0, 0.0};
  // On the XY plane through the anchor, looking down.
  CHECK(same(*onPlane(ray({10, 5, 100}, {0, 0, -1}), origin, {0, 0, 1}), {10, 5, 0}));
  // Edge-on and behind the eye: no place.
  CHECK(!onPlane(ray({10, 5, 0}, {1, 0, 0}), origin, {0, 0, 1}));
  CHECK(!onPlane(ray({10, 5, 0}, {1, 0, 0.01}), origin, {0, 0, 1}));
  CHECK(!onPlane(ray({0, 0, 10}, {0, 0, 1}), origin, {0, 0, 1}));
  // Along X through the anchor: the axis' point nearest to the ray.
  Placement axis;
  axis.mode = Placement::Mode::Axis;
  axis.axis = Axis3::X;
  CHECK(same(*placePoint(ray({10, 5, 100}, {0, 0, -1}), origin, axis), {10, 0, 0}));
  // An axis seen end-on gives no place.
  axis.axis = Axis3::Z;
  CHECK(!placePoint(ray({10, 5, 100}, {0, 0, -1}), origin, axis));
  // Normal holds the in-plane place and moves the elevation only.
  Placement normal;
  normal.mode = Placement::Mode::Normal;
  normal.plane = Plane3::XY;
  normal.normalBase = Xyz{3, 4, 0};
  CHECK(same(*placePoint(ray({100, 4, 7}, {-1, 0, 0}), origin, normal), {3, 4, 7}));
  // The planes' normals and axes.
  CHECK(planeNormalAxis(Plane3::YZ) == Axis3::X && planeNormalAxis(Plane3::ZX) == Axis3::Y);
  CHECK(planeAxes(Plane3::ZX)[0] == Axis3::Z && planeAxes(Plane3::ZX)[1] == Axis3::X);
  CHECK(placementText(axis) == QStringLiteral("along Z"));
  CHECK(placementText(Placement()) == QStringLiteral("on the XY plane"));
}

// The manipulator's free rotation (mitcad#1661).
void rotationTests() {
  const Xyz origin{0.0, 0.0, 0.0};
  const Frame3 own = sketchFrame();
  CHECK(isSketchFrame(own));
  CHECK(!Placement().rotated());
  // A quarter turn about Z: X to Y, Y to -X, Z stays; exact.
  const Frame3 z90 = turnedFrame(own, Axis3::Z, kPi / 2.0);
  CHECK(same(z90[0], {0, 1, 0}, 0.0) && same(z90[1], {-1, 0, 0}, 0.0) && same(z90[2], {0, 0, 1}, 0.0));
  CHECK(!isSketchFrame(z90));
  // Four quarter turns about X are the sketch's frame again, exactly.
  Frame3 quarters = own;
  for (int i = 0; i < 4; ++i) {
    quarters = turnedFrame(quarters, Axis3::X, kPi / 2.0);
  }
  CHECK(isSketchFrame(quarters));
  CHECK(same(quarters[1], {0, 1, 0}, 0.0) && same(quarters[2], {0, 0, 1}, 0.0));
  // 30° about X: Y and Z turn in the YZ plane, X stays; square and
  // right-handed; -30° takes it back.
  const double c30 = std::cos(kPi / 6.0);
  const double s30 = std::sin(kPi / 6.0);
  const Frame3 x30 = turnedFrame(own, Axis3::X, kPi / 6.0);
  CHECK(same(x30[0], {1, 0, 0}) && same(x30[1], {0, c30, s30}) && same(x30[2], {0, -s30, c30}));
  CHECK(same(dot(x30[0], x30[1]), 0.0) && same(dot(x30[1], x30[2]), 0.0) && same(norm(x30[2]), 1.0));
  CHECK(same(dot(cross(x30[0], x30[1]), x30[2]), 1.0));
  CHECK(isSketchFrame(turnedFrame(x30, Axis3::X, -kPi / 6.0)));
  // Turns about the frame's own axes: 90° about Z, then 90° about the
  // rotated X (the sketch's Y): the rotated Y lies along the sketch's Z,
  // the rotated Z along its X.
  const Frame3 zx = turnedFrame(z90, Axis3::X, kPi / 2.0);
  CHECK(same(zx[0], {0, 1, 0}, 0.0) && same(zx[1], {0, 0, 1}, 0.0) && same(zx[2], {1, 0, 0}, 0.0));
  // Many small turns stay square.
  Frame3 drift = own;
  for (int i = 0; i < 360; ++i) {
    drift = turnedFrame(drift, static_cast<Axis3>(i % 3), 0.0123);
  }
  CHECK(same(norm(drift[0]), 1.0) && same(norm(drift[1]), 1.0) && same(dot(drift[0], drift[1]), 0.0));
  CHECK(same(dot(cross(drift[0], drift[1]), drift[2]), 1.0));

  // The rotated XY plane (a quarter turn about X: the sketch's ZX plane):
  // its normal the sketch's -Y; a point placed on it has y = 0.
  Placement turned;
  turned.frame = turnedFrame(own, Axis3::X, kPi / 2.0);
  CHECK(turned.rotated());
  CHECK(same(placementNormal(turned), {0, -1, 0}, 0.0));
  CHECK(same(placementPlaneAxes(turned)[0], {1, 0, 0}, 0.0) && same(placementPlaneAxes(turned)[1], {0, 0, 1}, 0.0));
  CHECK(same(*placePoint(ray({10, -100, 7}, {0, 1, 0}), origin, turned), {10, 0, 7}));
  // Looking down Z it is seen edge-on: no place, where the sketch's XY
  // would have one.
  CHECK(!placePoint(ray({10, 5, 100}, {0, 0, -1}), origin, turned));
  CHECK(placePoint(ray({10, 5, 100}, {0, 0, -1}), origin, Placement()).has_value());
  CHECK(placementText(turned) == QStringLiteral("on the rotated XY plane"));
  // Along the rotated Y (the sketch's Z) through the anchor.
  turned.mode = Placement::Mode::Axis;
  turned.axis = Axis3::Y;
  CHECK(same(placementAxis(turned, Axis3::Y), {0, 0, 1}, 0.0));
  CHECK(same(*placePoint(ray({5, 100, 7}, {0, -1, 0}), {40, 0, 0}, turned), {40, 0, 7}));
  CHECK(placementText(turned) == QStringLiteral("along the rotated Y"));
  // Normal: the in-plane place held, the elevation along the rotated
  // normal (the sketch's Y).
  turned.mode = Placement::Mode::Normal;
  turned.normalBase = Xyz{3, 0, 4};
  CHECK(same(*placePoint(ray({100, -6, 4}, {-1, 0, 0}), origin, turned), {3, -6, 4}));
  CHECK(placementText(turned) == QStringLiteral("along the normal of the rotated XY"));
  // The rotated YZ plane: the rotated frame's Y and Z, its X the normal.
  turned.mode = Placement::Mode::Plane;
  turned.plane = Plane3::YZ;
  CHECK(same(placementNormal(turned), {1, 0, 0}, 0.0));
  CHECK(same(placementPlaneAxes(turned)[0], {0, 0, 1}, 0.0) && same(placementPlaneAxes(turned)[1], {0, -1, 0}, 0.0));
  // Reset: the sketch's frame, the XY plane, On Plane.
  turned.frame = sketchFrame();
  turned.plane = Plane3::XY;
  CHECK(!turned.rotated() && placementText(turned) == QStringLiteral("on the XY plane"));
  CHECK(same(placementNormal(turned), {0, 0, 1}, 0.0));

  // A handle's ring: about Z through (1, 2, 3), radius 5, from the X side.
  const std::vector<Xyz> ring = handleRing({1, 2, 3}, own, Axis3::Z, 5.0, 48);
  CHECK(ring.size() == 49);
  CHECK(same(ring.front(), {6, 2, 3}) && same(ring.back(), {6, 2, 3}));
  bool onRing = true;
  for (const Xyz& p : ring) {
    onRing = onRing && same(p[2], 3.0) && same(distance(p, {1, 2, 3}), 5.0);
  }
  CHECK(onRing);
  // A rotated frame's ring about its X lies in the rotated YZ plane.
  for (const Xyz& p : handleRing(origin, x30, Axis3::X, 2.0, 12)) {
    CHECK(same(dot(p, x30[0]), 0.0));
  }
  // The angle a drag turns about Z, grabbed on +X: a quarter turn to +Y,
  // back to -Y; none when the ring's plane is seen edge-on or the pointer
  // is on its center.
  CHECK(same(*handleTurn(ray({0, 10, 100}, {0, 0, -1}), origin, {0, 0, 1}, {10, 0, 0}), kPi / 2.0));
  CHECK(same(*handleTurn(ray({0, -10, 100}, {0, 0, -1}), origin, {0, 0, 1}, {10, 0, 0}), -kPi / 2.0));
  CHECK(same(*handleTurn(ray({-10, 0, 100}, {0, 0, -1}), origin, {0, 0, 1}, {10, 0, 0}), kPi));
  CHECK(!handleTurn(ray({0, 10, 0}, {1, 0, 0}), origin, {0, 0, 1}, {10, 0, 0}));
  CHECK(!handleTurn(ray({0, 0, 100}, {0, 0, -1}), origin, {0, 0, 1}, {10, 0, 0}));
  // A grabbed place off the ring's plane counts by its projection.
  CHECK(same(*handleTurn(ray({0, 10, 100}, {0, 0, -1}), origin, {0, 0, 1}, {10, 0, 4}), kPi / 2.0));
  // Whole degrees and their text.
  CHECK(same(wholeDegrees(kPi / 180.0 * 29.6), kPi / 6.0));
  CHECK(same(wholeDegrees(-kPi / 180.0 * 0.4), 0.0));
  const auto degrees = [](const char* number) {
    QString text = QString::fromLatin1(number);
    text.append(QChar(0x00B0));
    return text;
  };
  CHECK(degreesText(kPi / 6.0) == degrees("30"));
  CHECK(degreesText(-kPi / 2.0) == degrees("-90"));
  CHECK(degreesText(-0.0) == degrees("0"));
  CHECK(degreesText(kPi / 180.0 * 12.5) == degrees("12.5"));

  // A frame's coordinates and back.
  CHECK(same(inFrame(z90, {3, 4, 5}), {4, -3, 5}));
  CHECK(same(outOfFrame(z90, inFrame(z90, {3, 4, 5})), {3, 4, 5}));
  CHECK(frameJson(x30).value(QStringLiteral("normal")).toArray().size() == 3);

  // Typed values along the rotated plane's axes: Delta X and Z typed with
  // the frame turned 90° about Z (its X the sketch's Y) put the end 10
  // along the sketch's Y and 4 along Z; Axis and Length along its Y (the
  // sketch's -X).
  SegmentInput offsets;
  offsets.offsets[0] = typed(10, QStringLiteral("10 mm"));
  offsets.offsets[2] = typed(4, QStringLiteral("4 mm"));
  CHECK(same(typedEnd({1, 2, 3}, {7, 7, 7}, offsets, z90), {7, 12, 7}));
  SegmentInput length;
  length.mode = SegmentInput::Mode::AxisLength;
  length.axis = Axis3::Y;
  length.length = typed(5, QStringLiteral("5 mm"));
  CHECK(same(typedEnd({0, 0, 0}, {9, 9, 9}, length, z90), {-5, 0, 0}));
  // The sketch's own frame: as before.
  CHECK(same(typedEnd({1, 2, 3}, {7, 7, 7}, offsets), {11, 7, 7}));
  // A snap that agrees with the typed values along the rotated axes is no
  // conflict; one that does not is.
  SnapTarget agreeing = target(SnapKind::Point, 1.0);
  agreeing.at = {7, 12, 7};
  CHECK(snapConflict({1, 2, 3}, agreeing, offsets, std::nullopt, z90).isEmpty());
  agreeing.at = {11, 2, 7};
  CHECK(!snapConflict({1, 2, 3}, agreeing, offsets, std::nullopt, z90).isEmpty());

  // A line on the rotated plane carries its frame; Keep Axis along its X.
  const QJsonObject line = lineCommand(QStringLiteral("F2"), loose({0, 0, 0}), loose({0, 5, 0}), SegmentInput(),
                                       Axis3::X, false, z90);
  CHECK(line.value(QStringLiteral("frame")).isObject());
  CHECK(line.value(QStringLiteral("axis")).toString() == QStringLiteral("x"));
  CHECK(line.value(QStringLiteral("sense")).toString() == QStringLiteral("positive"));
  CHECK(!lineCommand(QStringLiteral("F2"), loose({0, 0, 0}), loose({5, 0, 0}), SegmentInput(), Axis3::X, false)
             .contains(QStringLiteral("frame")));
  // A point's offsets typed on the rotated plane: Offset Along dimensions.
  PlacedPoint offsetPoint = loose({0, 10, 0});
  offsetPoint.offsets[0] = QStringLiteral("10 mm");
  offsetPoint.offsetFrame = z90;
  const QJsonArray steps =
      followUps(QStringLiteral("F2"), {offsetPoint}, {QStringLiteral("p4")}, QStringLiteral("p1"), false);
  CHECK(steps.size() == 1);
  const QJsonObject along = steps.at(0).toObject().value(QStringLiteral("dimension")).toObject();
  CHECK(along.value(QStringLiteral("type")).toString() == QStringLiteral("offset_along"));
  CHECK(along.value(QStringLiteral("direction")).toArray().at(1).toDouble() == 1.0);

  // Shapes on the rotated plane (mitcad#1661): the XY plane turned 30°
  // about X; its axes and normal are the frame's, and the commands carry
  // the frame as their plane.
  const ShapePlane tilted(Plane3::XY, x30);
  CHECK(tilted.rotated() && !ShapePlane(Plane3::YZ).rotated());
  CHECK(same(tilted.u(), x30[0]) && same(tilted.v(), x30[1]) && same(tilted.normal(), x30[2]));
  CHECK(same(ShapePlane(Plane3::YZ, z90).normal(), z90[0]));
  const auto square = rectangleCorners(RectangleMode::TwoPoint, {{0, 0, 0}, add(scaled(x30[0], 20), scaled(x30[1], 10))},
                                       tilted);
  CHECK(square.has_value());
  if (square) {
    for (const Xyz& corner : *square) {
      CHECK(same(dot(corner, x30[2]), 0.0));
    }
    CHECK(same(distance((*square)[0], (*square)[1]), 20.0) && same(distance((*square)[1], (*square)[2]), 10.0));
  }
  for (const Xyz& p : circlePoints({0, 0, 0}, 5.0, tilted, 12)) {
    CHECK(same(dot(p, x30[2]), 0.0) && same(norm(p), 5.0));
  }
  const QJsonObject onTilted =
      circleCommand(QStringLiteral("F2"), loose({0, 0, 0}), 5.0, tilted, QString(), false, {}, false);
  const QJsonObject planeJson = onTilted.value(QStringLiteral("plane")).toObject();
  CHECK(planeJson.value(QStringLiteral("x")).toArray().size() == 3);
  CHECK(same(planeJson.value(QStringLiteral("normal")).toArray().at(2).toDouble(), c30));
  CHECK(circleCommand(QStringLiteral("F2"), loose({0, 0, 0}), 5.0, Plane3::ZX, QString(), false, {}, false)
            .value(QStringLiteral("plane"))
            .toString() == QStringLiteral("zx"));
}

void snapTests() {
  // Discrete points before points on curves before guidance.
  auto chosen = chooseSnap({target(SnapKind::Guide, 0.0), target(SnapKind::OnCurve, 1.0, true),
                            target(SnapKind::Point, 6.0, true)},
                           10.0);
  CHECK(chosen && chosen->kind == SnapKind::Point);
  // The nearest discrete point; at equal distance by kind: a point of the
  // sketch before a linked point, the origin before a midpoint.
  chosen = chooseSnap({target(SnapKind::LinkedPoint, 3.2), target(SnapKind::Point, 3.4, true)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::Point);
  chosen = chooseSnap({target(SnapKind::Origin, 3.0), target(SnapKind::Point, 4.0, true)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::Origin);
  chosen = chooseSnap({target(SnapKind::Midpoint, 2.0), target(SnapKind::Origin, 2.3)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::Origin);
  // Among points on curves this sketch's first at equal distance.
  chosen = chooseSnap({target(SnapKind::OnEdge, 2.0), target(SnapKind::OnCurve, 2.2, true)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::OnCurve && chosen->own);
  // Out of reach: only what is near counts.
  chosen = chooseSnap({target(SnapKind::Point, 12.0, true), target(SnapKind::Guide, 3.0)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::Guide);
  CHECK(!chooseSnap({target(SnapKind::Point, 12.0, true)}, 10.0));
  CHECK(!chooseSnap({}, 10.0));
  // The badges: in words, the ids the log's (mitcad#1653).
  SnapTarget vertex = target(SnapKind::LinkedPoint, 0.0);
  vertex.what = QStringLiteral("vertex V{a|b|c} of F1.b0");
  vertex.words = QStringLiteral("a vertex of Box1");
  CHECK(snapBadge(vertex) == QStringLiteral("Linked Point: a vertex of Box1"));
  CHECK(snapDetail(vertex) == QStringLiteral("Linked Point: vertex V{a|b|c} of F1.b0"));
  CHECK(snapBadge(target(SnapKind::Origin, 0.0)) == QStringLiteral("Origin"));
  CHECK(snapDetail(target(SnapKind::Origin, 0.0)) == QStringLiteral("Origin"));
  SnapTarget guide = target(SnapKind::Guide, 0.0);
  guide.what = QStringLiteral("Y");
  CHECK(snapBadge(guide) == QStringLiteral("Along Y"));
  // A face (mitcad#1549): the lowest, after edges, curves and guidance,
  // whatever their distance within reach; alone, it is taken.
  chosen = chooseSnap({target(SnapKind::OnFace, 0.0), target(SnapKind::OnEdge, 4.0)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::OnEdge);
  chosen = chooseSnap({target(SnapKind::OnFace, 0.0), target(SnapKind::Guide, 5.0)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::Guide);
  chosen = chooseSnap({target(SnapKind::OnFace, 0.0), target(SnapKind::LinkedPoint, 8.0)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::LinkedPoint);
  chosen = chooseSnap({target(SnapKind::OnFace, 0.0), target(SnapKind::Point, 12.0, true)}, 10.0);
  CHECK(chosen && chosen->kind == SnapKind::OnFace && !chosen->fixesPlace());
  SnapTarget face = target(SnapKind::OnFace, 0.0);
  face.what = QStringLiteral("face F1:side0 of F1.b0");
  face.words = QStringLiteral("a face of Box1");
  CHECK(snapBadge(face) == QStringLiteral("On Face: a face of Box1"));
  CHECK(snapDetail(face) == QStringLiteral("On Face: face F1:side0 of F1.b0"));
  CHECK(snapDetail(guide) == QStringLiteral("Along Y"));
  // A snap with no words gives its kind alone, never its ids.
  SnapTarget bare = target(SnapKind::OnEdge, 0.0);
  bare.what = QStringLiteral("edge E{a|b} of F1.b0");
  CHECK(snapBadge(bare) == QStringLiteral("On Edge"));
}

// No id (p2, c5, F1, O1), topological name (E{…}, F1:side0) or coordinate
// in a text a person reads.
bool hasIds(const QString& text) {
  static const QRegularExpression ids(
      QStringLiteral(R"(\b[pckFO][0-9]+\b|[EV]\{|\bF[0-9]+:|\bb[0-9]+\b|-?[0-9]+\.[0-9]+, -?[0-9]+\.[0-9]+)"));
  return ids.match(text).hasMatch();
}

void snapWordsTests() {
  // The sources the tools offer, in the badge's words (mitcad#1653): a body
  // by its feature's name, another component's by the component's name
  // (the tools carry it as `occurrence`), a middle left to the badge's
  // kind; no names known, no ids either.
  SourceNames names;
  names.feature = [](const QString& uid) {
    return uid == QStringLiteral("F1")   ? QStringLiteral("Box1")
           : uid == QStringLiteral("F3") ? QStringLiteral("Sketch1")
           : uid == QStringLiteral("F5") ? QStringLiteral("Intersection Curve1")
                                         : QString();
  };
  names.component = [](const QString& occurrence) {
    return occurrence == QStringLiteral("O2") ? QStringLiteral("Post") : QString();
  };
  const auto parse = [](const char* text) {
    return QJsonValue(QJsonDocument::fromJson(QByteArray(text)).object());
  };
  const char* const sources[] = {
      R"({"body": "F1.b0", "vertex": "V{a|b|c}"})",
      R"({"body": "F1.b0", "vertex": "V{a|b|c}", "occurrence": "O2", "context": "O1"})",
      R"({"body": "F1.b0", "edge": "E{a|b}", "at": "start"})",
      R"({"body": "F1.b0", "edge": "E{a|b}", "at": "middle"})",
      R"({"body": "F1.b0", "edge": "E{a|b}"})",
      R"({"body": "F1.b0", "face": "F1:side0"})",
      R"({"sketch": "F3", "point": "p2"})",
      R"({"sketch": "F3", "curve": "c4", "at": "middle"})",
      R"({"feature": "F5", "at": "end"})",
      R"({"feature": "F5"})",
  };
  const QStringList expected = {
      QStringLiteral("a vertex of Box1"),
      QStringLiteral("a vertex of Box1 in Post"),
      QStringLiteral("the start of an edge of Box1"),
      QStringLiteral("an edge of Box1"),
      QStringLiteral("an edge of Box1"),
      QStringLiteral("a face of Box1"),
      QStringLiteral("a point of Sketch1"),
      QStringLiteral("a curve of Sketch1"),
      QStringLiteral("the end of Intersection Curve1"),
      QStringLiteral("Intersection Curve1"),
  };
  int i = 0;
  for (const char* source : sources) {
    const QString named = snapSourceWords(parse(source), names);
    CHECK(named == expected.at(i));
    const QString unnamed = snapSourceWords(parse(source), SourceNames{});
    CHECK(!named.isEmpty() && !hasIds(named) && !unnamed.isEmpty() && !hasIds(unnamed));
    // The badge says the same with its kind.
    SnapTarget snap = target(SnapKind::LinkedPoint, 0.0);
    snap.what = QStringLiteral("vertex V{a|b|c} of F1.b0");
    snap.words = named;
    CHECK(!hasIds(snapBadge(snap)));
    ++i;
  }
  CHECK(snapSourceWords(QJsonValue(QStringLiteral("origin")), names) == QStringLiteral("the origin"));
  // The places the view paints for a snap: its source outside the sketch,
  // another component's through its occurrence; none for the origin, a
  // guide or the sketch's own point.
  SnapTarget edge = target(SnapKind::OnEdge, 0.0);
  edge.source = parse(R"({"body": "F1.b0", "edge": "E{a|b}", "occurrence": "O2", "context": "O1"})");
  const QJsonArray places = snapPlaces(edge);
  CHECK(places.size() == 1 &&
        places.at(0) == parse(R"({"kind": "edge", "name": "E{a|b}", "body": "F1.b0", "occurrence": "O2"})"));
  SnapTarget curve = target(SnapKind::Midpoint, 0.0);
  curve.source = parse(R"({"sketch": "F3", "curve": "c4", "at": "middle"})");
  CHECK(snapPlaces(curve).size() == 1 &&
        snapPlaces(curve).at(0) == parse(R"({"kind": "sketch_curve", "sketch": "F3", "name": "c4"})"));
  SnapTarget origin = target(SnapKind::Origin, 0.0);
  origin.source = QStringLiteral("origin");
  CHECK(snapPlaces(origin).isEmpty());
  CHECK(snapPlaces(target(SnapKind::Point, 0.0, true)).isEmpty());
  CHECK(snapPlaces(target(SnapKind::Guide, 0.0)).isEmpty());
}

void inputTests() {
  // Offsets: a typed Delta X drives, the others are where the pointer is.
  SegmentInput input;
  input.offsets[0] = typed(40.0, QStringLiteral("40 mm"));
  CHECK(same(typedEnd({0, 0, 0}, {37, 2, 1}, input), {40, 2, 1}));
  PlacedPoint start = loose({0, 0, 0});
  start.snap.kind = SnapKind::Origin;
  start.snap.source = QStringLiteral("origin");
  QJsonObject line = lineCommand(QStringLiteral("F2"), start, loose({40, 2, 1}), input, std::nullopt, false);
  CHECK(line.value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.add_line"));
  CHECK(line.value(QStringLiteral("start_source")).toString() == QStringLiteral("origin"));
  QJsonArray offsets = line.value(QStringLiteral("offsets")).toArray();
  CHECK(offsets.size() == 3 && offsets.at(0).toString() == QStringLiteral("40 mm") && offsets.at(1).isNull() &&
        offsets.at(2).isNull());
  CHECK(!line.contains(QStringLiteral("axis")) && !line.contains(QStringLiteral("end_source")));
  // An explicit zero is kept as a driving value.
  input.offsets[2] = typed(0.0, QStringLiteral("0 mm"));
  CHECK(same(typedEnd({0, 0, 5}, {37, 2, 9}, input), {40, 2, 5}));
  offsets = lineCommand(QStringLiteral("F2"), start, loose({40, 2, 0}), input, std::nullopt, false)
                .value(QStringLiteral("offsets"))
                .toArray();
  CHECK(offsets.at(2).toString() == QStringLiteral("0 mm") && offsets.at(1).isNull());
  // Typed offsets are frame differences from the start on every placement
  // plane (mitcad#1661): the pointer's place on YZ (x = 0) or ZX (y = 0)
  // through the start gives the coordinates not typed only.
  {
    SegmentInput all;
    all.offsets[0] = typed(10.0, QStringLiteral("10 mm"));
    all.offsets[1] = typed(0.0, QStringLiteral("0 mm"));
    all.offsets[2] = typed(15.0, QStringLiteral("15 mm"));
    CHECK(same(typedEnd({0, 0, 0}, {0, 7, 3}, all), {10, 0, 15}));  // on YZ
    CHECK(same(typedEnd({0, 0, 0}, {4, 0, 9}, all), {10, 0, 15}));  // on ZX
    CHECK(same(typedEnd({1, 2, 3}, {1, 8, -4}, all), {11, 2, 18})); // on YZ through (1, 2, 3)
    CHECK(inputProblem({0, 0, 0}, typedEnd({0, 0, 0}, {0, 7, 3}, all), all, std::nullopt).isEmpty());
    SegmentInput dz;
    dz.offsets[2] = typed(15.0, QStringLiteral("15 mm"));
    CHECK(same(typedEnd({0, 0, 0}, {0, 7, 3}, dz), {0, 7, 15})); // YZ: Delta Z, the pointer's y
    CHECK(same(typedEnd({0, 0, 0}, {4, 0, 9}, dz), {4, 0, 15})); // ZX: Delta Z, the pointer's x
    // The pointer resting on the pending start: its origin or point snap
    // is no candidate for the next point, a guide or another place is.
    SnapTarget origin = target(SnapKind::Origin, 0.0, true);
    CHECK(snapsOntoStart(origin, {0, 0, 0}));
    CHECK(!snapsOntoStart(origin, {10, 0, 15}));
    SnapTarget point = target(SnapKind::Point, 0.0, true);
    point.at = {5, 5, 5};
    CHECK(snapsOntoStart(point, {5, 5, 5}) && !snapsOntoStart(point, {5, 5, 6}));
    CHECK(!snapsOntoStart(target(SnapKind::Guide, 0.0), {0, 0, 0}));
    CHECK(!snapsOntoStart(target(SnapKind::OnFace, 0.0), {0, 0, 0}));
  }
  // Switching the mode drops the other mode's typed values.
  input.mode = SegmentInput::Mode::AxisLength;
  input.clearOtherMode();
  CHECK(!input.offsets[0].edited && !input.offsets[2].edited && !input.anyEdited());
  // Axis and Length: along -Y, 30 long, from B; without a typed length as
  // far along as the pointer.
  input.axis = Axis3::Y;
  input.negative = true;
  CHECK(same(typedEnd({40, 0, 0}, {44, -12, 3}, input), {40, -12, 0}));
  input.length = typed(30.0, QStringLiteral("30 mm"));
  CHECK(same(typedEnd({40, 0, 0}, {44, -12, 3}, input), {40, -30, 0}));
  line = lineCommand(QStringLiteral("F2"), sharedPoint(QStringLiteral("p2"), {40, 0, 0}), loose({40, -30, 0}), input,
                     std::nullopt, true);
  CHECK(line.value(QStringLiteral("start")).toString() == QStringLiteral("p2"));
  CHECK(line.value(QStringLiteral("axis")).toString() == QStringLiteral("y"));
  CHECK(line.value(QStringLiteral("sense")).toString() == QStringLiteral("negative"));
  CHECK(line.value(QStringLiteral("length")).toString() == QStringLiteral("30 mm"));
  CHECK(line.value(QStringLiteral("construction")).toBool() && !line.contains(QStringLiteral("offsets")));
  // Keep Axis stores the alignment in the sense the segment runs.
  line = lineCommand(QStringLiteral("F2"), loose({0, 0, 0}), loose({-20, 0, 0}), SegmentInput(), Axis3::X, false);
  CHECK(line.value(QStringLiteral("axis")).toString() == QStringLiteral("x"));
  CHECK(line.value(QStringLiteral("sense")).toString() == QStringLiteral("negative"));
  // A snapped end's source goes with the line.
  PlacedPoint vertex = loose({40, 30, 20});
  vertex.snap.kind = SnapKind::LinkedPoint;
  vertex.snap.source = QJsonObject{{QStringLiteral("body"), QStringLiteral("F1.b0")},
                                   {QStringLiteral("vertex"), QStringLiteral("V{a|b|c}")}};
  line = lineCommand(QStringLiteral("F2"), loose({0, 0, 0}), vertex, SegmentInput(), std::nullopt, false);
  CHECK(line.value(QStringLiteral("end_source")).toObject().value(QStringLiteral("vertex")).toString() ==
        QStringLiteral("V{a|b|c}"));
  CHECK(line.value(QStringLiteral("end")).toArray().at(2).toDouble() == 20.0);
}

void problemTests() {
  SegmentInput input;
  input.mode = SegmentInput::Mode::AxisLength;
  input.length = typed(0.0, QStringLiteral("0 mm"));
  CHECK(inputProblem({0, 0, 0}, {0, 0, 0}, input, std::nullopt) == QStringLiteral("The length must be greater than zero."));
  CHECK(inputProblem({1, 1, 1}, {1, 1, 1}, SegmentInput(), std::nullopt) ==
        QStringLiteral("The segment would have no length."));
  SegmentInput offsets;
  offsets.offsets[1] = typed(5.0, QStringLiteral("5 mm"));
  CHECK(inputProblem({0, 0, 0}, {10, 5, 0}, offsets, Axis3::X).startsWith(QStringLiteral("Keep Axis along X")));
  CHECK(inputProblem({0, 0, 0}, {10, 5, 0}, offsets, std::nullopt).isEmpty());
  SegmentInput invalid;
  invalid.offsets[0].edited = true;
  invalid.offsets[0].valid = false;
  invalid.offsets[0].problem = QStringLiteral("unknown parameter q");
  CHECK(inputProblem({0, 0, 0}, {10, 0, 0}, invalid, std::nullopt) ==
        QStringLiteral("Delta X: unknown parameter q"));
  // A snap that fixes the place must agree with the typed values; a snap
  // on a curve leaves it to the solve.
  SnapTarget linked = target(SnapKind::LinkedPoint, 0.0);
  linked.at = {50, 0, 0};
  linked.what = QStringLiteral("vertex V{x}");
  SegmentInput dx;
  dx.offsets[0] = typed(40.0, QStringLiteral("40 mm"));
  CHECK(snapConflict({10, 0, 0}, linked, dx, std::nullopt).isEmpty());
  dx.offsets[0] = typed(30.0, QStringLiteral("30 mm"));
  CHECK(snapConflict({10, 0, 0}, linked, dx, std::nullopt).contains(QStringLiteral("Delta X")));
  SegmentInput along;
  along.mode = SegmentInput::Mode::AxisLength;
  along.length = typed(40.0, QStringLiteral("40 mm"));
  CHECK(snapConflict({10, 0, 0}, linked, along, std::nullopt).isEmpty());
  linked.at = {50, 1, 0};
  CHECK(!snapConflict({10, 0, 0}, linked, along, std::nullopt).isEmpty());
  CHECK(!snapConflict({10, 0, 0}, linked, SegmentInput(), Axis3::X).isEmpty());
  SnapTarget onCurve = target(SnapKind::OnCurve, 0.0);
  onCurve.at = {3, 3, 3};
  CHECK(snapConflict({10, 0, 0}, onCurve, dx, std::nullopt).isEmpty());
  CHECK(formatLength(25.4, 25.4) == QStringLiteral("1.00"));
}

void arcTests() {
  const double c = std::sqrt(0.5) * 10.0;
  // Through three points counter-clockwise about +Z.
  auto arc = threePointArc({10, 0, 0}, {0, 10, 0}, {c, c, 0});
  CHECK(arc && same(arc->center, {0, 0, 0}) && same(arc->radius, 10.0) && same(arc->normal, {0, 0, 1}) &&
        same(arc->sweep, kPi / 2.0));
  // The long way round: the normal turns over.
  arc = threePointArc({10, 0, 0}, {0, 10, 0}, {-10, 0, 0});
  CHECK(arc && same(arc->normal, {0, 0, -1}) && same(arc->sweep, 1.5 * kPi));
  CHECK(!threePointArc({0, 0, 0}, {20, 0, 0}, {10, 0, 0}));
  CHECK(!threePointArc({0, 0, 0}, {0, 0, 0}, {10, 3, 0}));
  // Around a center: the end turned onto the circle.
  Xyz end{};
  arc = centerArc({0, 0, 0}, {10, 0, 0}, {0, 3, 0}, {0, 0, 1}, &end);
  CHECK(arc && same(end, {0, 10, 0}) && same(arc->radius, 10.0) && same(arc->sweep, kPi / 2.0));
  CHECK(!centerArc({0, 0, 0}, {0, 0, 0}, {0, 3, 0}, {0, 0, 1}, nullptr));
  // Tangent from B along +X to C (W11): the circle of radius 10 about
  // (40, 10, 0).
  arc = tangentArc({40, 0, 0}, {1, 0, 0}, {50, 10, 0});
  CHECK(arc && same(arc->center, {40, 10, 0}) && same(arc->radius, 10.0) && same(arc->normal, {0, 0, 1}) &&
        same(arc->sweep, kPi / 2.0));
  CHECK(!tangentArc({0, 0, 0}, {1, 0, 0}, {30, 0, 0}));
  // A typed radius, then an arc length too.
  arc = tangentArcSized({0, 0, 0}, {1, 0, 0}, {10, 10, 0}, 5.0, std::nullopt, &end);
  CHECK(arc && same(arc->center, {0, 5, 0}) && same(distance(end, arc->center), 5.0));
  CHECK(same(norm(cross(sub(end, arc->center), Xyz{10, 5, 0})), 0.0, 1e-9));
  arc = tangentArcSized({0, 0, 0}, {1, 0, 0}, {10, 10, 0}, 5.0, 2.5 * kPi, &end);
  CHECK(arc && same(end, {5, 5, 0}));
  const std::vector<Xyz> points = arcPoints(*arc, {0, 0, 0}, 4);
  CHECK(points.size() == 5 && same(points.front(), {0, 0, 0}) && same(points.back(), {5, 5, 0}));
  // Keep Plane: along the active plane's normal, in either sense.
  auto kept = keptNormal({0, 0, -1}, Plane3::XY);
  CHECK(kept && kept->first == Axis3::Z && kept->second);
  CHECK(!keptNormal({1, 0, 0}, Plane3::XY));
  kept = keptNormal({1, 0, 0}, Plane3::YZ);
  CHECK(kept && kept->first == Axis3::X && !kept->second);
}

void commandTests() {
  // A tangent arc continuing c3 at p2, its radius typed, Keep Plane.
  ArcRequest tangent;
  tangent.mode = QStringLiteral("tangent");
  tangent.from = QStringLiteral("c3");
  tangent.start = sharedPoint(QStringLiteral("p2"), {40, 0, 0});
  tangent.end = loose({50, 10, 0});
  tangent.radius = QStringLiteral("10 mm");
  tangent.keepPlane = std::make_pair(Axis3::Z, false);
  QJsonObject cmd = arcCommand(QStringLiteral("F2"), tangent);
  CHECK(cmd.value(QStringLiteral("mode")).toString() == QStringLiteral("tangent"));
  CHECK(cmd.value(QStringLiteral("from")).toString() == QStringLiteral("c3"));
  CHECK(cmd.value(QStringLiteral("start")).toString() == QStringLiteral("p2"));
  CHECK(cmd.value(QStringLiteral("end")).toArray().at(0).toDouble() == 50.0);
  CHECK(cmd.value(QStringLiteral("radius")).toString() == QStringLiteral("10 mm"));
  CHECK(cmd.value(QStringLiteral("normal_axis")).toString() == QStringLiteral("z"));
  CHECK(cmd.value(QStringLiteral("normal_sense")).toString() == QStringLiteral("positive"));
  CHECK(!cmd.contains(QStringLiteral("normal")) && !cmd.contains(QStringLiteral("arc_length")));
  // A center arc names its normal unless Keep Plane binds it.
  ArcRequest center;
  center.mode = QStringLiteral("center");
  center.center = loose({0, 0, 0});
  center.start = loose({10, 0, 0});
  center.end = loose({0, 10, 0});
  center.normal = {0, 1, 0};
  cmd = arcCommand(QStringLiteral("F2"), center);
  CHECK(cmd.value(QStringLiteral("normal")).toArray().at(1).toDouble() == 1.0);
  CHECK(cmd.value(QStringLiteral("center")).toArray().size() == 3);
  // Splines: degree min(3, n - 1) by control points, closed fit points.
  const std::vector<PlacedPoint> two{loose({0, 0, 0}), loose({10, 0, 0})};
  CHECK(splineCommand(QStringLiteral("F2"), two, true, false, false).value(QStringLiteral("degree")).toInt() == 1);
  const std::vector<PlacedPoint> four{loose({0, 0, 0}), loose({10, 0, 0}), loose({10, 10, 5}), loose({0, 10, 9})};
  CHECK(splineCommand(QStringLiteral("F2"), four, true, false, false).value(QStringLiteral("degree")).toInt() == 3);
  cmd = splineCommand(QStringLiteral("F2"), four, false, true, false);
  CHECK(cmd.value(QStringLiteral("closed")).toBool() && !cmd.contains(QStringLiteral("degree")));
  CHECK(cmd.value(QStringLiteral("points")).toArray().size() == 4);
  // The follow-ups in one undo step: an attachment for a linked point the
  // spline's command cannot take, an offset typed from the point before,
  // a Point on Curve on this sketch's curve.
  std::vector<PlacedPoint> placed{loose({0, 0, 0}), loose({5, 0, 0}), loose({9, 1, 0})};
  placed[0].snap.kind = SnapKind::LinkedPoint;
  placed[0].snap.source =
      QJsonObject{{QStringLiteral("sketch"), QStringLiteral("F3")}, {QStringLiteral("point"), QStringLiteral("p2")}};
  placed[1].offsets[0] = QStringLiteral("5 mm");
  placed[2].snap.kind = SnapKind::OnCurve;
  placed[2].snap.curve = QStringLiteral("c4");
  placed[2].snap.own = true;
  const QJsonArray steps = followUps(QStringLiteral("F2"), placed,
                                     {QStringLiteral("p7"), QStringLiteral("p8"), QStringLiteral("p9")},
                                     QStringLiteral("p1"), true);
  CHECK(steps.size() == 3);
  CHECK(steps.at(0).toObject().value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.attach"));
  CHECK(steps.at(0).toObject().value(QStringLiteral("point")).toString() == QStringLiteral("p7"));
  const QJsonObject dimension = steps.at(1).toObject();
  CHECK(dimension.value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.add_dimension"));
  CHECK(dimension.value(QStringLiteral("dimension")).toObject() ==
        QJsonObject({{QStringLiteral("type"), QStringLiteral("offset")},
                     {QStringLiteral("a"), QStringLiteral("p7")},
                     {QStringLiteral("b"), QStringLiteral("p8")},
                     {QStringLiteral("axis"), QStringLiteral("x")}}));
  CHECK(dimension.value(QStringLiteral("value")).toString() == QStringLiteral("5 mm"));
  CHECK(steps.at(2).toObject().value(QStringLiteral("constraint")).toObject() ==
        QJsonObject({{QStringLiteral("type"), QStringLiteral("point_on")},
                     {QStringLiteral("point"), QStringLiteral("p9")},
                     {QStringLiteral("curve"), QStringLiteral("c4")}}));
  // A line's end on this sketch's line's middle: a Midpoint relation; the
  // line's own command takes the sources.
  PlacedPoint middle = loose({20, 0, 0});
  middle.snap.kind = SnapKind::Midpoint;
  middle.snap.curve = QStringLiteral("c3");
  const QJsonArray relation =
      followUps(QStringLiteral("F2"), {loose({0, 5, 0}), middle}, {QStringLiteral("p5"), QStringLiteral("p6")},
                QString(), false);
  CHECK(relation.size() == 1 && relation.at(0).toObject().value(QStringLiteral("constraint")).toObject().value(
                                    QStringLiteral("type")) == QStringLiteral("midpoint"));
  CHECK(pointSource(middle).isNull());
  CHECK(pointCommand(QStringLiteral("F2"), placed[0]).value(QStringLiteral("source")).toObject().value(
            QStringLiteral("sketch")) == QStringLiteral("F3"));
}

void curveTests() {
  // Previews pass through their points; a control-point spline starts and
  // ends at its first and last control point.
  const std::vector<Xyz> fit{{0, 0, 0}, {10, 0, 5}, {20, 10, 5}};
  const std::vector<Xyz> sampled = fitSplinePreview(fit, false);
  CHECK(same(sampled.front(), fit.front()) && same(sampled.back(), fit.back()));
  bool through = false;
  for (const Xyz& p : sampled) {
    through = through || same(p, fit[1]);
  }
  CHECK(through);
  const std::vector<Xyz> control = controlSplinePreview(fit);
  CHECK(same(control.front(), fit.front()) && same(control.back(), fit.back()));
  // The directions a Tangent Arc continues: a line's from its other end,
  // an arc's along its turn.
  const Sketch3dModel model = Sketch3dModel::read(QJsonDocument::fromJson(QByteArray(R"({
    "uid": "F2", "name": "3DSketch1", "type": "spatial_sketch", "support": "xy",
    "frame": {"origin": [0, 0, 0], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0], "normal": [0, 0, 1]},
    "points": [{"id": "p1", "at": [0, 0, 0]}, {"id": "p2", "at": [40, 0, 0]},
               {"id": "p4", "at": [50, 10, 0]}, {"id": "p5", "at": [40, 10, 0]}],
    "curves": [
      {"id": "c3", "type": "line", "start": "p1", "end": "p2",
       "geometry": {"start": [0, 0, 0], "end": [40, 0, 0], "length": 40}},
      {"id": "c6", "type": "arc", "center": "p5", "start": "p2", "end": "p4", "normal": [0, 0, 1],
       "geometry": {"normal": [0, 0, 1], "radius": 10, "sweep": 1.5707963267948966, "length": 15.7}}],
    "constraints": [], "dimensions": [], "references": [],
    "evaluated": true, "status": {"dof": 3, "fully_constrained": false, "redundant": []}})"))
                                                      .object());
  CHECK(same(*endTangent(model, QStringLiteral("c3"), QStringLiteral("p2")), {1, 0, 0}));
  CHECK(same(*endTangent(model, QStringLiteral("c3"), QStringLiteral("p1")), {-1, 0, 0}));
  CHECK(same(*endTangent(model, QStringLiteral("c6"), QStringLiteral("p4")), {0, 1, 0}));
  CHECK(same(*endTangent(model, QStringLiteral("c6"), QStringLiteral("p2")), {-1, 0, 0}));
  CHECK(!endTangent(model, QStringLiteral("c3"), QStringLiteral("p4")));
  CHECK(continuableCurves(model, QStringLiteral("p4")) == QStringList{QStringLiteral("c6")});
  const QStringList joint = continuableCurves(model, QStringLiteral("p2"));
  CHECK(joint.size() == 2 && joint.contains(QStringLiteral("c3")) && joint.contains(QStringLiteral("c6")));
}

// mitcad#1550: the plane-bound shapes' geometry on the frame planes and
// the commands their tools send.
void shapeTests() {
  // Planar coordinates: YZ's are y and z, ZX's z and x.
  CHECK(planarOf({1, 2, 3}, Plane3::YZ)[0] == 2.0 && planarOf({1, 2, 3}, Plane3::YZ)[1] == 3.0);
  CHECK(planarOf({1, 2, 3}, Plane3::ZX)[0] == 3.0 && planarOf({1, 2, 3}, Plane3::ZX)[1] == 1.0);
  CHECK(same(fromPlanar({2, 3}, 1, Plane3::YZ), {1, 2, 3}));
  CHECK(same(ontoPlane({5, 6, 7}, {0, 0, 2}, Plane3::XY), {5, 6, 2}));
  CHECK(planeKey(Plane3::ZX) == QStringLiteral("zx"));

  // Circles: a 2-point one on the plane through its first point, a
  // 3-point one through all three.
  const auto two = twoPointCircle({0, 0, 5}, {10, 0, 9}, Plane3::XY);
  CHECK(two && same(two->center, {5, 0, 5}) && same(two->radius, 5.0));
  CHECK(!twoPointCircle({0, 0, 0}, {0, 0, 4}, Plane3::XY));
  const auto three = threePointCircle({3, 10, 0}, {3, 0, 10}, {3, -10, 0}, Plane3::YZ);
  CHECK(three && same(three->center, {3, 0, 0}, 1e-9) && same(three->radius, 10.0, 1e-9));
  CHECK(!threePointCircle({0, 0, 0}, {1, 1, 0}, {2, 2, 0}, Plane3::XY));
  const std::vector<Xyz> ring = circlePoints({0, 0, 0}, 2, Plane3::ZX, 8);
  // ZX's first axis is Z, its second X.
  CHECK(ring.size() == 9 && same(ring[0], {0, 0, 2}) && same(ring[2], {2, 0, 0}, 1e-12));

  // An ellipse's minor radius from a point, across its major axis.
  CHECK(same(ellipseMinor({0, 0, 0}, {10, 0, 0}, {3, 4, 0}, Plane3::XY), 4.0));
  CHECK(ellipsePoints({0, 0, 0}, {10, 0, 0}, 4, Plane3::XY, 4).size() == 5);

  // Rectangles: numbered from the lower left counter-clockwise in the
  // plane's own axes, the clicks' corners as the model joins them.
  const auto box = rectangleCorners(RectangleMode::TwoPoint, {{5, 40, 20}, {5, 0, 0}}, Plane3::YZ);
  CHECK(box && same((*box)[0], {5, 0, 0}) && same((*box)[1], {5, 40, 0}) && same((*box)[2], {5, 40, 20}));
  const std::vector<int> joined = rectangleClickCorners(RectangleMode::TwoPoint, {{5, 40, 20}, {5, 0, 0}}, Plane3::YZ);
  CHECK(joined.size() == 2 && joined[0] == 2 && joined[1] == 0);
  const auto centred = rectangleCorners(RectangleMode::Center, {{0, 0, 0}, {3, 2, 0}}, Plane3::XY);
  CHECK(centred && same((*centred)[0], {-3, -2, 0}) && same((*centred)[2], {3, 2, 0}));
  CHECK(!rectangleCorners(RectangleMode::TwoPoint, {{0, 0, 0}, {0, 5, 0}}, Plane3::XY));
  // A 3-point rectangle drawn clockwise: its second click is the fourth.
  const std::vector<Xyz> clockwise{{0, 0, 0}, {10, 0, 0}, {10, -5, 0}};
  const auto turned = rectangleCorners(RectangleMode::ThreePoint, clockwise, Plane3::XY);
  CHECK(turned && same((*turned)[3], {10, 0, 0}) && same((*turned)[1], {0, -5, 0}));
  CHECK(rectangleClickCorners(RectangleMode::ThreePoint, clockwise, Plane3::XY)[1] == 3);

  // Polygons: a hexagon's corners 10 from its centre; a circumscribed
  // one's apothem the vertex's distance; an Edge Polygon's centre.
  const std::vector<Xyz> hexagon = polygonCorners({0, 0, 0}, {10, 0, 0}, 6, true, Plane3::XY);
  CHECK(hexagon.size() == 6 && same(hexagon[0], {10, 0, 0}) && same(norm(hexagon[3]), 10.0));
  const std::vector<Xyz> square = polygonCorners({0, 0, 0}, {0, 0, 5}, 4, false, Plane3::ZX);
  CHECK(square.size() == 4 && same(norm(square[0]), 5.0 * std::sqrt(2.0), 1e-9));
  const auto middle = edgePolygonCenter({0, 0, 0}, {10, 0, 0}, {5, 3, 0}, 4, Plane3::XY);
  CHECK(middle && same(*middle, {5, 5, 0}, 1e-9));

  // The commands: on their planes, typed sizes as values, drawn ones not
  // dimensioned.
  PlacedPoint center;
  center.at = {1, 2, 3};
  const QJsonObject circle =
      circleCommand(QStringLiteral("F2"), center, 4.0, Plane3::YZ, QString(), false, {}, false);
  CHECK(circle.value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.add_circle"));
  CHECK(circle.value(QStringLiteral("plane")).toString() == QStringLiteral("yz"));
  CHECK(circle.value(QStringLiteral("radius")).toDouble() == 4.0);
  CHECK(circle.value(QStringLiteral("dimension")).toBool(true) == false);
  const QJsonObject typedCircle = circleCommand(QStringLiteral("F2"), center, 4.0, Plane3::XY,
                                                QStringLiteral("20 mm"), true, {QStringLiteral("p3")}, true);
  CHECK(typedCircle.value(QStringLiteral("radius")).toString() == QStringLiteral("20 mm"));
  CHECK(typedCircle.value(QStringLiteral("diameter")).toBool());
  CHECK(typedCircle.value(QStringLiteral("through")).toArray().size() == 1);
  CHECK(typedCircle.value(QStringLiteral("construction")).toBool());
  PlacedPoint major;
  major.at = {11, 2, 3};
  major.snap.kind = SnapKind::Point;
  major.snap.point = QStringLiteral("p7");
  const QJsonObject ellipse = ellipseCommand(QStringLiteral("F2"), center, major, 3.0, Plane3::XY, QString(),
                                             QStringLiteral("5 mm"), false);
  CHECK(ellipse.value(QStringLiteral("major")).toString() == QStringLiteral("p7"));
  CHECK(ellipse.value(QStringLiteral("minor_value")).toString() == QStringLiteral("5 mm"));
  CHECK(!ellipse.contains(QStringLiteral("major_value")));
  PlacedPoint a;
  a.at = {0, 0, 0};
  PlacedPoint b;
  b.at = {10, 5, 0};
  PlacedPoint c;
  c.at = {3, 9, 0};
  const QJsonObject rectangle = rectangleCommand(QStringLiteral("F2"), RectangleMode::ThreePoint, {a, b, c},
                                                 Plane3::XY, QStringLiteral("12 mm"), QString(), false);
  CHECK(rectangle.value(QStringLiteral("mode")).toString() == QStringLiteral("three_point"));
  CHECK(rectangle.value(QStringLiteral("c")).toArray().size() == 3);
  CHECK(rectangle.value(QStringLiteral("width")).toString() == QStringLiteral("12 mm"));
  const QJsonObject centered = rectangleCommand(QStringLiteral("F2"), RectangleMode::Center, {a, b}, Plane3::ZX,
                                                QString(), QString(), true);
  CHECK(centered.contains(QStringLiteral("center")) && centered.contains(QStringLiteral("corner")));
  CHECK(!centered.contains(QStringLiteral("a")));
  const QJsonObject polygon =
      polygonCommand(QStringLiteral("F2"), center, major, 5, false, Plane3::XY, QStringLiteral("8 mm"), false);
  // A circumscribed polygon's vertex is a place even on a point.
  CHECK(polygon.value(QStringLiteral("vertex")).isArray());
  CHECK(polygon.value(QStringLiteral("sides")).toInt() == 5 && !polygon.value(QStringLiteral("inscribed")).toBool());
  const QJsonObject text =
      textCommand(QStringLiteral("F2"), QStringLiteral("AB"), {0, 1, 2}, Xyz{0, 30, 12}, Plane3::YZ, 6.0);
  CHECK(text.value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.add_text"));
  CHECK(text.value(QStringLiteral("frame")).toArray().size() == 3);
  CHECK(text.value(QStringLiteral("height")).toDouble() == 6.0);
}

} // namespace

int sketch3dDrawTests() {
  failures = 0;
  placementTests();
  rotationTests();
  snapTests();
  snapWordsTests();
  inputTests();
  problemTests();
  arcTests();
  commandTests();
  curveTests();
  shapeTests();
  return failures;
}
