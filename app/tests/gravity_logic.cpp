// SPDX-License-Identifier: MIT
// The plain logic of gravity in the view (mitcad#1713, app/gravity/
// GravityLogic.hpp): the design's direction and the hanging item as the
// model answers (its chain, its nodes and pins, the point nearest to a
// press), pins toggled, one step in flight with nothing queued, the time a
// step simulates, the commands, where a left press goes while an item
// hangs, the marker's arrow on the screen and the log line of a hanging
// item.
#include "UnitTests.hpp"
#include "gravity/GravityLogic.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonDocument>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "gravity_logic.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::gravity;

bool approx(double a, double b, double tolerance = 1e-9) { return std::abs(a - b) <= tolerance; }
bool same(const Vec3& a, const Vec3& b, double tolerance = 1e-9) { return length(sub(a, b)) <= tolerance; }

QJsonObject object(const char* json) { return QJsonDocument::fromJson(QByteArray(json)).object(); }

// A hose of three points hanging from its two ends, its middle on a plate
// and held by the pointer, as flex.hang_step answers.
const char* const kHanging = R"({
  "flex": "F2", "length": 100, "segment": 50, "at_rest": false, "time": 0.033, "taut": false,
  "grabbed": 50,
  "polyline": [[0, 0, 50], [50, 0, 10], [100, 0, 50]],
  "normals": [[0, 0, 1], [0, 1, 0], [0, 0, 1]],
  "touching": [false, true, false],
  "nodes": [{"id": "n1", "index": 0, "point": [0, 0, 50], "along": 0, "pinned": true, "support": "port",
             "moved": false},
            {"id": "n3", "index": 1, "point": [50, 0, 10], "along": 50, "pinned": false, "support": "face",
             "moved": false},
            {"id": "n2", "index": 2, "point": [100, 0, 50], "along": 100, "pinned": true, "support": "free",
             "moved": false}]
})";

void hangingTests() {
  const std::optional<Hanging> hanging = Hanging::read(object(kHanging));
  CHECK(hanging.has_value());
  if (!hanging) {
    return;
  }
  CHECK(hanging->flex == QStringLiteral("F2") && hanging->frames.size() == 3 && hanging->nodes.size() == 3);
  CHECK(approx(hanging->length, 100.0) && approx(hanging->segment, 50.0) && !hanging->atRest && !hanging->taut);
  CHECK(hanging->grabbed && approx(*hanging->grabbed, 50.0));
  // Its lengths along the chain and its directions from the points.
  CHECK(same(hanging->frames[1].point, {50.0, 0.0, 10.0}));
  CHECK(approx(hanging->frames[2].along, 2.0 * std::sqrt(4100.0)));
  CHECK(same(hanging->frames[1].tangent, {1.0, 0.0, 0.0}) && same(hanging->frames[1].normal, {0.0, 1.0, 0.0}));
  CHECK(approx(hanging->frames[0].along, 0.0) &&
        same(hanging->frames[0].tangent, unit({50.0, 0.0, -40.0}).value_or(Vec3{})));
  CHECK(hanging->touchingCount() == 1);
  CHECK(hanging->pins() == (QStringList{QStringLiteral("n1"), QStringLiteral("n2")}));
  const HangNode* middle = hanging->node(QStringLiteral("n3"));
  CHECK(middle != nullptr && middle->index == 1 && middle->support == QStringLiteral("face") && !middle->pinned);
  CHECK(hanging->node(QStringLiteral("n9")) == nullptr);
  // A pin let go, a free node pinned.
  CHECK(hanging->toggled(QStringLiteral("n1")) == (QStringList{QStringLiteral("n2")}));
  CHECK(hanging->toggled(QStringLiteral("n3")) ==
        (QStringList{QStringLiteral("n1"), QStringLiteral("n3"), QStringLiteral("n2")}));
  // The point nearest to where a press took it.
  CHECK(hanging->nearest({48.0, 1.0, 12.0}) == 1);
  CHECK(hanging->nearest({99.0, 0.0, 49.0}) == 2);
  // The log line.
  CHECK(hangText(*hanging, {0.0, 0.0, -1.0}) ==
        QStringLiteral("points 3, touching 1, pins n1,n2, length 100.0, lowest 10.0 along 0.000,0.000,-1.000, at "
                       "rest no"));
  // The panel's end pins: the ends as asked, the middle as it is.
  CHECK(hanging->startPinned() && hanging->endPinned() && approx(hanging->damping, 0.5));
  CHECK(hanging->pinsWithEnds(true, false) == (QStringList{QStringLiteral("n1")}));
  CHECK(hanging->toggled(QStringLiteral("n3")).size() == 3);
  Hanging clipped = *hanging;
  clipped.nodes[1].pinned = true;
  CHECK(clipped.pinsWithEnds(false, true) == (QStringList{QStringLiteral("n3"), QStringLiteral("n2")}));
  CHECK(!Hanging().startPinned() && Hanging().pinsWithEnds(true, true).isEmpty());
  // Not a hanging item: no `flex`, or fewer than two points.
  CHECK(!Hanging::read(object(R"({"polyline": [[0, 0, 0], [1, 0, 0]]})")));
  CHECK(!Hanging::read(object(R"({"flex": "F2", "polyline": [[0, 0, 0]]})")));
  // Nothing held: no `grabbed`.
  const std::optional<Hanging> loose = Hanging::read(object(R"({"flex": "F2", "grabbed": null, "taut": true,
    "damping": 0.8, "polyline": [[0, 0, 0], [1, 0, 0]]})"));
  CHECK(loose && !loose->grabbed && loose->taut && loose->pins().isEmpty() && approx(loose->damping, 0.8));
}

// What the Let It Hang panel wants, and whether the design's direction is
// already the one its Direction From gives.
void wishTests() {
  const QJsonObject face{{QStringLiteral("body"), QStringLiteral("F1.b0")}, {QStringLiteral("face"), QStringLiteral("F1:top")}};
  HangWish wish;
  wish.flex = QStringLiteral("F4");
  wish.gravity = setCommand(face, QString(), false);
  wish.length = 200.0;
  wish.damping = 0.3;
  wish.endPinned = false;
  const std::optional<HangWish> read = HangWish::read(wish.toJson());
  CHECK(read && read->flex == QStringLiteral("F4") && read->gravity && *read->gravity == *wish.gravity &&
        read->length && approx(*read->length, 200.0) && approx(read->damping, 0.3) && read->startPinned &&
        !read->endPinned);
  // Without a direction of its own, a length or the pins: the design's, as
  // it is, both ends pinned.
  const std::optional<HangWish> plain = HangWish::read(object(R"({"flex": "F4", "length": 0, "damping": 4})"));
  CHECK(plain && !plain->gravity && !plain->length && approx(plain->damping, 1.0) && plain->startPinned &&
        plain->endPinned);
  CHECK(!HangWish::read(object(R"({"damping": 0.5})")));

  Direction design = Direction::read(object(R"({"direction": [0, 0, -1],
    "source": {"geometry": {"body": "F1.b0", "face": "F1:top"}}, "source_direction": [0, 0, -1]})"));
  CHECK(sameDirection(design, setCommand(face, QString(), false)));
  CHECK(!sameDirection(design, setCommand(face, QString(), true)));
  CHECK(!sameDirection(design, setCommand(face, QStringLiteral("O1"), false)));
  CHECK(!sameDirection(design, setCommand(QStringLiteral("xy"), QString(), false)));
  // Its face turned since, or no direction: set again.
  design.sourceMoved = true;
  CHECK(!sameDirection(design, setCommand(face, QString(), false)));
  CHECK(!sameDirection(Direction(), setCommand(face, QString(), false)));
}

void directionTests() {
  Direction d = Direction::read(object(R"({"direction": null, "source": null, "source_direction": null,
                                           "hanging": null})"));
  CHECK(!d.direction && d.hanging.isEmpty());
  d = Direction::read(object(R"({"direction": [0, 0, -1],
    "source": {"geometry": {"body": "F1.b0", "face": "F1:top"}, "occurrence": "", "reverse": false},
    "source_direction": [0, 0, -1], "hanging": "F7"})"));
  CHECK(d.direction && same(*d.direction, {0.0, 0.0, -1.0}) && d.source == QStringLiteral("face") &&
        !d.reversed && !d.sourceMoved && d.hanging == QStringLiteral("F7"));
  // The face turned since: the stored direction stays, the source moved.
  d = Direction::read(object(R"({"direction": [0, 0, -1],
    "source": {"geometry": {"body": "F1.b0", "face": "F1:top"}, "reverse": true},
    "source_direction": [0, -1, 0]})"));
  CHECK(d.reversed && d.sourceMoved);
  // A vector alone.
  d = Direction::read(object(R"({"direction": [0, -1, 0], "source": null, "source_direction": null})"));
  CHECK(d.direction && d.source.isEmpty() && !d.sourceMoved);
  CHECK(sourceKind(QStringLiteral("xy")) == QStringLiteral("plane"));
  CHECK(sourceKind(QStringLiteral("z")) == QStringLiteral("axis"));
  CHECK(sourceKind(QStringLiteral("F5")) == QStringLiteral("construction"));
  CHECK(sourceKind(object(R"({"body": "F2.b0", "edge": "E{a|b}"})")) == QStringLiteral("edge"));
  CHECK(sourceKind(object(R"({"sketch": "F1", "curve": "c4"})")) == QStringLiteral("line"));
  CHECK(sourceKind(object(R"({"origin": [0, 0, 0], "direction": [0, 0, 1]})")) == QStringLiteral("axis"));
}

void stepTests() {
  // As long as the real time, within its bounds.
  CHECK(approx(stepSeconds(1.0 / 60.0), 1.0 / 60.0));
  CHECK(approx(stepSeconds(0.0), 1.0 / 240.0));
  // A slow frame: at most one frame of the model's, the rest left out.
  CHECK(approx(stepSeconds(0.5), 1.0 / 30.0) && approx(stepSeconds(0.04), kStepMostSeconds));
  CHECK(approx(stepSeconds(std::nan("")), 1.0 / 60.0));

  // One step in flight: a tick meanwhile is skipped, not queued.
  StepGate gate;
  CHECK(!gate.busy() && gate.begin() && gate.busy());
  CHECK(!gate.begin() && !gate.begin() && gate.missed() && gate.skipped() == 2 && gate.started() == 1);
  gate.end();
  CHECK(!gate.busy() && gate.begin() && !gate.missed() && gate.started() == 2);
  gate.end();
  gate.reset();
  CHECK(gate.started() == 0 && gate.skipped() == 0 && !gate.busy());

  // The steps' times for the log line of its rest.
  StepTimes times;
  CHECK(times.text() == QStringLiteral("no steps timed"));
  times.add(2.0);
  times.add(6.0);
  CHECK(times.count() == 2 && times.text() == QStringLiteral("steps of 4.0 ms on average, 6.0 ms at most"));
}

void commandTests() {
  const QJsonObject face{{QStringLiteral("body"), QStringLiteral("F1.b0")}, {QStringLiteral("face"), QStringLiteral("F1:top")}};
  QJsonObject cmd = setCommand(face, QString(), false);
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("gravity.set") &&
        cmd.value(QStringLiteral("geometry")).toObject() == face && !cmd.contains(QStringLiteral("occurrence")) &&
        !cmd.contains(QStringLiteral("reverse")));
  cmd = setCommand(face, QStringLiteral("O1"), true);
  CHECK(cmd.value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1") &&
        cmd.value(QStringLiteral("reverse")).toBool());
  CHECK(clearCommand().value(QStringLiteral("cmd")).toString() == QStringLiteral("gravity.clear"));
  cmd = hangCommand(QStringLiteral("F2"));
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.hang") &&
        cmd.value(QStringLiteral("flex")).toString() == QStringLiteral("F2") && !cmd.contains(QStringLiteral("damping")));
  cmd = hangCommand(QStringLiteral("F2"), 0.25);
  CHECK(approx(cmd.value(QStringLiteral("damping")).toDouble(), 0.25));
  cmd = hangDampingCommand(0.7);
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.hang_set") &&
        approx(cmd.value(QStringLiteral("damping")).toDouble(), 0.7) && !cmd.contains(QStringLiteral("pins")));
  cmd = hangStepCommand(0.02, std::nullopt);
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.hang_step") &&
        approx(cmd.value(QStringLiteral("seconds")).toDouble(), 0.02) && !cmd.contains(QStringLiteral("grab")));
  cmd = hangStepCommand(0.02, std::make_pair(Vec3{1.0, 2.0, 3.0}, Vec3{4.0, 5.0, 6.0}));
  const QJsonObject grab = cmd.value(QStringLiteral("grab")).toObject();
  CHECK(grab.value(QStringLiteral("point")).toArray() == (QJsonArray{1.0, 2.0, 3.0}) &&
        grab.value(QStringLiteral("target")).toArray() == (QJsonArray{4.0, 5.0, 6.0}));
  cmd = hangPinsCommand({QStringLiteral("n1")});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.hang_set") &&
        cmd.value(QStringLiteral("pins")).toArray() == (QJsonArray{QStringLiteral("n1")}) &&
        !cmd.contains(QStringLiteral("length")));
  cmd = hangLengthCommand(150.0);
  CHECK(approx(cmd.value(QStringLiteral("length")).toDouble(), 150.0) && !cmd.contains(QStringLiteral("pins")));
  CHECK(hangLockCommand().value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.hang_lock"));
  CHECK(hangCancelCommand().value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.hang_cancel"));
}

void markerTests() {
  // Looking down -Y with +Z up: gravity down -Z points down the screen.
  ScreenArrow arrow = screenArrow({0.0, 0.0, -1.0}, {0.0, 1.0, 0.0}, {0.0, 0.0, 1.0});
  CHECK(approx(arrow.x, 0.0) && approx(arrow.y, -1.0) && !arrow.endOn);
  // +X is to the right in that view.
  arrow = screenArrow({1.0, 0.0, 0.0}, {0.0, 1.0, 0.0}, {0.0, 0.0, 1.0});
  CHECK(approx(arrow.x, 1.0) && approx(arrow.y, 0.0));
  // Looking straight down: gravity points away from the viewer, end on.
  arrow = screenArrow({0.0, 0.0, -1.0}, {0.0, 0.0, -1.0}, {0.0, 1.0, 0.0});
  CHECK(arrow.endOn && arrow.away);
  arrow = screenArrow({0.0, 0.0, 1.0}, {0.0, 0.0, -1.0}, {0.0, 1.0, 0.0});
  CHECK(arrow.endOn && !arrow.away);
  CHECK(vectorText({-0.0001, 1.0, 2.5}) == QStringLiteral("0.000,1.000,2.500"));
}

// Where a left press goes while an item hangs (mitcad#1713): on the item
// the item's before the view's.
void pressTests() {
  PressInput press;
  press.left = true;
  // Beside the item: the view's, so that the panel's Direction From takes
  // a face there.
  CHECK(routePress(press) == PressRoute::View);
  // On the item: grabbed where the press hit; on a node's handle, before
  // the item: its pin toggled.
  press.onChain = true;
  CHECK(routePress(press) == PressRoute::Grab);
  press.onNode = true;
  CHECK(routePress(press) == PressRoute::TogglePin);
  // Another button, or a key held, or on the cube or a panel: the view's.
  press.left = false;
  CHECK(routePress(press) == PressRoute::View);
  press.left = true;
  press.viewArea = false;
  CHECK(routePress(press) == PressRoute::View);
  // Any press while the item is dragged: swallowed, the drag goes on.
  press.dragging = true;
  press.left = false;
  CHECK(routePress(press) == PressRoute::Swallow);
}

} // namespace

int gravityLogicTests() {
  failures = 0;
  hangingTests();
  wishTests();
  directionTests();
  stepTests();
  commandTests();
  pressTests();
  markerTests();
  return failures;
}
MITCAD_UNIT_TEST(gravityLogicTests);
