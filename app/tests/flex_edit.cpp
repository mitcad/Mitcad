// SPDX-License-Identifier: MIT
// Editing a Flex item without the view (mitcad#1713): the `flex` query's
// answer read (nodes, the section, problems, a previewed answer taking what
// it leaves out from the committed one); the command a pulled node's
// release sends (it slides on its face, takes another face or a port, comes
// off into space, or changes nothing on its own port); add, delete, twist
// and detach; the definitions the live previews evaluate; where on the
// route a new node goes; the twist's quarter-turn snap and its angle.
#include "UnitTests.hpp"
#include "flex/FlexEdit.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonDocument>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "flex_edit.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::flex;

const double kPi = 3.14159265358979323846;

bool same(const Xyz& a, const Xyz& b, double tolerance = 1e-9) { return distance(a, b) <= tolerance; }

QJsonObject object(const char* json) { return QJsonDocument::fromJson(QByteArray(json)).object(); }

// A hose from a pipe's end over the top of a block and off into space:
// 40 along X, then 30 along Y.
const char* const kAnswer = R"({
  "uid": "F7", "name": "Flex1", "status": "ok",
  "item": {"name": "Hose 10", "section": {"type": "round", "diameter": 10}, "wall": 1.5, "bend_radius": 30},
  "bend_radius": 30, "bend_radius_edge": null, "length": 70.0,
  "nodes": [
    {"id": "n1", "point": [0, 0, 0], "normal": null, "along": 0, "support": "port",
     "ref": {"body": "F2.b0", "face": "F2:end"}, "lost": false, "twist": 0},
    {"id": "n2", "point": [40, 0, 0], "normal": [0, 0, 1], "along": 40, "support": "face",
     "ref": {"body": "F3.b0", "face": "F3:top"}, "lost": false, "twist": 0},
    {"id": "n4", "point": [40, 30, 0], "normal": null, "along": 70, "support": "free",
     "ref": null, "lost": false, "twist": 0.5}],
  "polyline": [[0, 0, 0], [20, 0, 0], [40, 0, 0], [40, 30, 0]],
  "runs": [{"from": 10, "to": 45, "body": "F3.b0", "face": "F3:top"}],
  "problems": [
    {"kind": "bend_too_tight", "message": "the item bends more tightly than its bend radius allows",
     "from": 38, "to": 42, "places": [{"kind": "point", "at": [40, 0, 0]}]},
    {"kind": "bend_too_tight", "message": "the item bends more tightly than its bend radius allows",
     "from": 38, "to": 42, "places": [{"kind": "point", "at": [40, 0, 0]}]}]
})";

const char* const kDef = R"({
  "type": "flex",
  "item": {"name": "Hose 10", "section": {"type": "round", "diameter": 10}, "bend_radius": 30},
  "nodes": [
    {"id": "n1", "at": [0, 0, 0], "port": {"body": "F2.b0", "face": "F2:end"}},
    {"id": "n2", "at": [40, 0, 0], "on": {"body": "F3.b0", "face": "F3:top"}},
    {"id": "n4", "at": [40, 30, 0], "twist": 0.5}]
})";

void readTests() {
  const FlexItem item = FlexItem::read(object(kAnswer));
  CHECK(item.valid && item.uid == QStringLiteral("F7") && item.name == QStringLiteral("Flex1"));
  CHECK(item.round && std::abs(item.width - 10.0) < 1e-12 && std::abs(item.reach() - 10.0) < 1e-12);
  CHECK(item.bendRadius && std::abs(*item.bendRadius - 30.0) < 1e-12 && !item.bendRadiusEdge);
  CHECK(!item.smallestBend);
  CHECK(item.nodes.size() == 3 && item.hasRoute());
  CHECK(item.node(QStringLiteral("n2")) != nullptr && item.node(QStringLiteral("n3")) == nullptr);
  CHECK(item.nodeIndex(QStringLiteral("n4")) == 2 && item.nodeIndex(QStringLiteral("n9")) == -1);
  CHECK(item.isEnd(QStringLiteral("n1")) && item.isEnd(QStringLiteral("n4")) && !item.isEnd(QStringLiteral("n2")));
  CHECK(item.node(QStringLiteral("n2"))->normal.has_value() && !item.node(QStringLiteral("n1"))->normal);
  CHECK(item.node(QStringLiteral("n1"))->attached() && !item.node(QStringLiteral("n4"))->attached());
  // The same place and message of two problems once.
  CHECK(item.problems.size() == 2 && item.problemPlaces().size() == 1 && item.problemMessages().size() == 1);

  // A previewed answer has no item, refs or twists: the committed one's.
  const FlexItem preview = FlexItem::read(object(R"({"status": "preview", "length": 82.5,
    "nodes": [{"id": "n1", "point": [0, 0, 0], "along": 0, "support": "port"},
              {"id": "n2", "point": [40, 10, 0], "along": 41.2, "support": "face"},
              {"id": "n4", "point": [40, 40, 5], "along": 82.5, "support": "free"}],
    "polyline": [[0, 0, 0], [40, 10, 0], [40, 40, 5]]})"),
                                           &item);
  CHECK(preview.valid && preview.uid == QStringLiteral("F7") && preview.round && preview.width == 10.0);
  CHECK(std::abs(preview.length - 82.5) < 1e-12 && preview.problems.empty());
  CHECK(std::abs(preview.node(QStringLiteral("n4"))->twist - 0.5) < 1e-12);
  CHECK(sameReference(preview.node(QStringLiteral("n2"))->ref, item.node(QStringLiteral("n2"))->ref));
  CHECK(preview.bendRadius && std::abs(*preview.bendRadius - 30.0) < 1e-12);

  // A flat cable's section; an answer without nodes is not valid.
  const FlexItem flat = FlexItem::read(object(R"({"uid": "F8", "status": "ok",
    "item": {"section": {"type": "rectangular", "width": 12, "height": 2}},
    "nodes": [{"id": "n1", "point": [0, 0, 0], "along": 0, "support": "free"},
              {"id": "n2", "point": [0, 0, 9], "along": 9, "support": "free"}],
    "polyline": [[0, 0, 0], [0, 0, 9]],
    "frames": [{"along": 0, "point": [0, 0, 0], "tangent": [0, 0, 1], "normal": [0, 1, 0]},
               {"along": 9, "point": [0, 0, 9], "tangent": [0, 0, 2], "normal": [1, 0, 0]}]})"));
  CHECK(!flat.round && flat.width == 12.0 && flat.height == 2.0 && flat.reach() == 12.0);
  CHECK(flat.frames.size() == 2 && same(flat.frames[1].tangent, Xyz{0.0, 0.0, 1.0}));
  const std::optional<FlexFrame> nearEnd = frameAt(flat, 8.0);
  CHECK(nearEnd && same(nearEnd->normal, Xyz{1.0, 0.0, 0.0}));
  CHECK(!FlexItem::read(object(R"({"uid": "F9", "status": "error"})")).valid);
}

void commandTests() {
  const FlexItem item = FlexItem::read(object(kAnswer));
  const QJsonObject top{{QStringLiteral("body"), QStringLiteral("F3.b0")}, {QStringLiteral("face"), QStringLiteral("F3:top")}};
  const QJsonObject side{{QStringLiteral("body"), QStringLiteral("F3.b0")}, {QStringLiteral("face"), QStringLiteral("F3:side1")}};
  const QJsonObject port{{QStringLiteral("body"), QStringLiteral("F2.b0")}, {QStringLiteral("face"), QStringLiteral("F2:end")}};

  // On its own face the node slides: move_node.
  QJsonObject cmd = dragCommand(item, QStringLiteral("n2"), {NodeTarget::Kind::Face, {30.0, 5.0, 0.0}, top});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.move_node"));
  CHECK(cmd.value(QStringLiteral("flex")).toString() == QStringLiteral("F7") &&
        cmd.value(QStringLiteral("node")).toString() == QStringLiteral("n2"));
  CHECK(same(*xyzOf(cmd.value(QStringLiteral("at"))), Xyz{30.0, 5.0, 0.0}) && !cmd.contains(QStringLiteral("on")));
  // Over another face it re-attaches there.
  cmd = dragCommand(item, QStringLiteral("n2"), {NodeTarget::Kind::Face, {40.0, 15.0, -5.0}, side});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.attach_node"));
  CHECK(cmd.value(QStringLiteral("on")).toObject() == side && cmd.contains(QStringLiteral("at")));
  // Off every face it becomes free where it is put.
  cmd = dragCommand(item, QStringLiteral("n2"), {NodeTarget::Kind::Free, {40.0, 0.0, 25.0}, QJsonObject()});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.attach_node"));
  CHECK(cmd.value(QStringLiteral("free")).toBool() && same(*xyzOf(cmd.value(QStringLiteral("at"))), Xyz{40.0, 0.0, 25.0}));
  // A free node moved in space: move_node; dropped on a face: attach_node.
  cmd = dragCommand(item, QStringLiteral("n4"), {NodeTarget::Kind::Free, {50.0, 30.0, 0.0}, QJsonObject()});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.move_node"));
  cmd = dragCommand(item, QStringLiteral("n4"), {NodeTarget::Kind::Face, {40.0, 20.0, 0.0}, top});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.attach_node") &&
        cmd.value(QStringLiteral("on")).toObject() == top);
  // An end pulled off its port, and dropped on it again.
  cmd = dragCommand(item, QStringLiteral("n1"), {NodeTarget::Kind::Free, {-5.0, 0.0, 10.0}, QJsonObject()});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.attach_node") &&
        cmd.value(QStringLiteral("free")).toBool());
  CHECK(dragCommand(item, QStringLiteral("n1"), {NodeTarget::Kind::Port, {0.0, 0.0, 0.0}, port}).isEmpty());
  cmd = dragCommand(item, QStringLiteral("n4"), {NodeTarget::Kind::Port, {0.0, 0.0, 0.0}, port});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.attach_node") &&
        cmd.value(QStringLiteral("port")).toObject() == port && !cmd.contains(QStringLiteral("at")));
  CHECK(dragCommand(item, QStringLiteral("n9"), {NodeTarget::Kind::Free, {}, QJsonObject()}).isEmpty());
  // Where it was picked is not part of what a reference names.
  QJsonObject elsewhere = top;
  elsewhere.insert(QStringLiteral("occurrence"), QStringLiteral("O2"));
  CHECK(sameReference(top, top) && !sameReference(top, side) && sameReference(top, elsewhere) &&
        !sameReference(QJsonValue(), top));

  // Add at the end, at the start and between; delete, twist, detach.
  cmd = addNodeCommand(QStringLiteral("F7"), {NodeTarget::Kind::Free, {60.0, 30.0, 0.0}, QJsonObject()},
                       QStringLiteral("end"));
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.add_node") &&
        cmd.value(QStringLiteral("place")).toString() == QStringLiteral("end") && !cmd.contains(QStringLiteral("on")));
  cmd = addNodeCommand(QStringLiteral("F7"), {NodeTarget::Kind::Face, {20.0, 0.0, 0.0}, top},
                       QJsonObject{{QStringLiteral("after"), QStringLiteral("n1")}});
  CHECK(cmd.value(QStringLiteral("place")).toObject().value(QStringLiteral("after")).toString() == QStringLiteral("n1") &&
        cmd.value(QStringLiteral("on")).toObject() == top);
  cmd = addNodeCommand(QStringLiteral("F7"), {NodeTarget::Kind::Port, {0.0, 0.0, 0.0}, port}, QStringLiteral("start"));
  CHECK(cmd.value(QStringLiteral("port")).toObject() == port && !cmd.contains(QStringLiteral("on")));
  cmd = deleteNodeCommand(QStringLiteral("F7"), QStringLiteral("n2"));
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.delete_node") &&
        cmd.value(QStringLiteral("node")).toString() == QStringLiteral("n2"));
  cmd = setTwistCommand(QStringLiteral("F7"), QStringLiteral("n2"), kPi);
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.set_twist") &&
        std::abs(cmd.value(QStringLiteral("twist")).toDouble() - kPi) < 1e-12);
  cmd = detachCommand(QStringLiteral("F7"), QStringLiteral("n2"));
  CHECK(cmd.value(QStringLiteral("free")).toBool() && !cmd.contains(QStringLiteral("at")));
}

void definitionTests() {
  const QJsonObject def = object(kDef);
  const QJsonObject side{{QStringLiteral("body"), QStringLiteral("F3.b0")}, {QStringLiteral("face"), QStringLiteral("F3:side1")}};
  const auto nodeOf = [](const QJsonObject& d, int row) {
    return d.value(QStringLiteral("nodes")).toArray().at(row).toObject();
  };
  // A node put on another face, and in space: its old attachment goes.
  QJsonObject edited = withNodeAt(def, QStringLiteral("n2"), {NodeTarget::Kind::Face, {40.0, 15.0, -5.0}, side});
  CHECK(nodeOf(edited, 1).value(QStringLiteral("on")).toObject() == side);
  CHECK(same(*xyzOf(nodeOf(edited, 1).value(QStringLiteral("at"))), Xyz{40.0, 15.0, -5.0}));
  edited = withNodeAt(def, QStringLiteral("n1"), {NodeTarget::Kind::Free, {0.0, 0.0, 9.0}, QJsonObject()});
  CHECK(!nodeOf(edited, 0).contains(QStringLiteral("port")) && !nodeOf(edited, 0).contains(QStringLiteral("on")));
  // The other nodes and the item stay.
  CHECK(nodeOf(edited, 2) == nodeOf(def, 2) && edited.value(QStringLiteral("item")) == def.value(QStringLiteral("item")));
  CHECK(withNodeAt(def, QStringLiteral("n9"), {NodeTarget::Kind::Free, {}, QJsonObject()}) == def);

  // A node added: the next free id, at the end, the start or after a node.
  CHECK(nextNodeId(def) == QStringLiteral("n5"));
  QString id;
  edited = withNodeAdded(def, {NodeTarget::Kind::Free, {60.0, 30.0, 0.0}, QJsonObject()}, QStringLiteral("end"), &id);
  CHECK(id == QStringLiteral("n5") && edited.value(QStringLiteral("nodes")).toArray().size() == 4);
  CHECK(nodeOf(edited, 3).value(QStringLiteral("id")).toString() == QStringLiteral("n5"));
  edited = withNodeAdded(def, {NodeTarget::Kind::Free, {-9.0, 0.0, 0.0}, QJsonObject()}, QStringLiteral("start"));
  CHECK(nodeOf(edited, 0).value(QStringLiteral("id")).toString() == QStringLiteral("n5") &&
        nodeOf(edited, 1).value(QStringLiteral("id")).toString() == QStringLiteral("n1"));
  edited = withNodeAdded(def, {NodeTarget::Kind::Face, {20.0, 0.0, 0.0}, side},
                         QJsonObject{{QStringLiteral("after"), QStringLiteral("n1")}});
  CHECK(nodeOf(edited, 1).value(QStringLiteral("id")).toString() == QStringLiteral("n5") &&
        nodeOf(edited, 1).value(QStringLiteral("on")).toObject() == side &&
        nodeOf(edited, 2).value(QStringLiteral("id")).toString() == QStringLiteral("n2"));

  edited = withTwist(def, QStringLiteral("n2"), kPi / 2.0);
  CHECK(std::abs(nodeOf(edited, 1).value(QStringLiteral("twist")).toDouble() - kPi / 2.0) < 1e-12);
  const QJsonObject edit = editFeatureCommand(QStringLiteral("F7"), edited);
  CHECK(edit.value(QStringLiteral("cmd")).toString() == QStringLiteral("edit_feature") &&
        edit.value(QStringLiteral("uid")).toString() == QStringLiteral("F7") &&
        edit.value(QStringLiteral("def")).toObject() == edited);
}

void routeTests() {
  const FlexItem item = FlexItem::read(object(kAnswer));
  const std::vector<double> lengths = lengthsAlong(item.polyline);
  CHECK(lengths.size() == 4 && std::abs(lengths.back() - 70.0) < 1e-12);
  CHECK(same(pointAlong(item.polyline, lengths, 30.0), Xyz{30.0, 0.0, 0.0}));
  CHECK(same(pointAlong(item.polyline, lengths, 55.0), Xyz{40.0, 15.0, 0.0}));
  CHECK(same(pointAlong(item.polyline, lengths, -3.0), Xyz{0.0, 0.0, 0.0}) &&
        same(pointAlong(item.polyline, lengths, 99.0), Xyz{40.0, 30.0, 0.0}));
  // A new node between two nodes goes after the one before its place, never
  // after the last node.
  CHECK(nodeBefore(item, 10.0) == QStringLiteral("n1") && nodeBefore(item, 55.0) == QStringLiteral("n2"));
  CHECK(nodeBefore(item, 70.0) == QStringLiteral("n2") && nodeBefore(item, -1.0) == QStringLiteral("n1"));
  // On the face the route lies on there, free elsewhere.
  CHECK(runAt(item, 20.0) && runAt(item, 20.0)->face == QStringLiteral("F3:top") && !runAt(item, 5.0) && !runAt(item, 60.0));
  const std::vector<Stretch> between = stretches(item);
  CHECK(between.size() == 2 && between[0].after == QStringLiteral("n1") && between[1].after == QStringLiteral("n2"));
  CHECK(same(between[0].middle.point, Xyz{20.0, 0.0, 0.0}) && same(between[1].middle.point, Xyz{40.0, 15.0, 0.0}));
  // Without samples the frame is the centre line's direction.
  const std::optional<FlexFrame> frame = frameAt(item, 55.0);
  CHECK(frame && same(frame->tangent, Xyz{0.0, 1.0, 0.0}) && std::abs(dot(frame->normal, frame->tangent)) < 1e-12);
  CHECK(std::abs(length(frame->normal) - 1.0) < 1e-12);
}

void twistTests() {
  const double tolerance = 6.0 * kPi / 180.0;
  CHECK(std::abs(snappedTwist(kPi / 2.0 + 0.05, tolerance) - kPi / 2.0) < 1e-12);
  CHECK(std::abs(snappedTwist(-kPi + 0.08, tolerance) + kPi) < 1e-12);
  CHECK(std::abs(snappedTwist(0.6, tolerance) - 0.6) < 1e-12);
  CHECK(std::abs(snappedTwist(0.02, tolerance)) < 1e-12);
  const Xyz z{0.0, 0.0, 1.0};
  CHECK(std::abs(angleAbout(Xyz{1.0, 0.0, 0.0}, Xyz{0.0, 2.0, 0.0}, z) - kPi / 2.0) < 1e-12);
  CHECK(std::abs(angleAbout(Xyz{1.0, 0.0, 0.0}, Xyz{0.0, -1.0, 5.0}, z) + kPi / 2.0) < 1e-12);
  CHECK(angleAbout(Xyz{0.0, 0.0, 3.0}, Xyz{0.0, 1.0, 0.0}, z) == 0.0);
  CHECK(same(turned(Xyz{1.0, 0.0, 0.0}, z, kPi / 2.0), Xyz{0.0, 1.0, 0.0}));
  CHECK(degreesText(kPi / 2.0) == QStringLiteral("90") + QChar(0x00B0));
  CHECK(degreesText(-1e-9) == QStringLiteral("0") + QChar(0x00B0));
  CHECK(xyzLog(Xyz{1.0, -2.5, 0.0}) == QStringLiteral("(1.000, -2.500, 0.000)"));
  CHECK(targetLog({NodeTarget::Kind::Free, {}, QJsonObject()}) == QStringLiteral("free"));
  CHECK(targetLog({NodeTarget::Kind::Face, {},
                   QJsonObject{{QStringLiteral("body"), QStringLiteral("F3.b0")},
                               {QStringLiteral("face"), QStringLiteral("F3:top")}}}) ==
        QStringLiteral("face F3:top of F3.b0"));
}


void secondRoundTests() {
  // The tightest bend against the smaller of the radii the item has.
  QJsonObject answer = object(kAnswer);
  CHECK(FlexItem::read(answer).bendState() == FlexItem::BendState::Clear);
  const auto withBend = [&answer](double radius) {
    answer.insert(QStringLiteral("smallest_bend"),
                  QJsonObject{{QStringLiteral("radius"), radius},
                              {QStringLiteral("along"), 40.0},
                              {QStringLiteral("point"), QJsonArray{40.0, 0.0, 0.0}}});
    return FlexItem::read(answer);
  };
  FlexItem item = withBend(31.4);
  CHECK(item.smallestBend && std::abs(item.smallestBend->radius - 31.4) < 1e-12 &&
        same(item.smallestBend->point, Xyz{40.0, 0.0, 0.0}));
  CHECK(item.bendLimit() && std::abs(*item.bendLimit() - 30.0) < 1e-12);
  CHECK(item.bendState() == FlexItem::BendState::Near);
  CHECK(withBend(22.0).bendState() == FlexItem::BendState::Over);
  // A little under the limit, within the model's slack: the router's
  // rounding, no fault (and no warning from the model).
  CHECK(withBend(28.0).bendState() == FlexItem::BendState::Near);
  CHECK(withBend(60.0).bendState() == FlexItem::BendState::Clear);
  answer.insert(QStringLiteral("bend_radius_edge"), 80.0);
  CHECK(std::abs(*withBend(60.0).bendLimit() - 30.0) < 1e-12);
  answer.insert(QStringLiteral("smallest_bend"), QJsonValue());
  CHECK(!FlexItem::read(answer).smallestBend);
  // A round section: no limit over the edge.
  CHECK(!withBend(60.0).edgeBendLimit() && withBend(60.0).edgeBendState() == FlexItem::BendState::Clear);

  // A flat cable: its bends over the flat side against the bend radius,
  // those over its edge apart (mitcad#1713: the badge showed the flat bend
  // beside a warning about the edge).
  {
    QJsonObject flat = object(kAnswer);
    flat.insert(QStringLiteral("item"),
                QJsonObject{{QStringLiteral("name"), QStringLiteral("Flat cable 10")},
                            {QStringLiteral("section"), QJsonObject{{QStringLiteral("type"), QStringLiteral("rectangular")},
                                                                    {QStringLiteral("width"), 10.0},
                                                                    {QStringLiteral("height"), 1.0}}}});
    flat.insert(QStringLiteral("bend_radius"), 5.0);
    const auto bendJson = [](double radius, double along) {
      return QJsonObject{{QStringLiteral("radius"), radius},
                         {QStringLiteral("along"), along},
                         {QStringLiteral("point"), QJsonArray{along, 0.0, 0.0}}};
    };
    flat.insert(QStringLiteral("smallest_bend"), bendJson(5.15, 10.0));
    flat.insert(QStringLiteral("smallest_bend_edge"), bendJson(20.0, 30.0));
    FlexItem cable = FlexItem::read(flat);
    CHECK(!cable.round && cable.smallestBendEdge && std::abs(cable.smallestBendEdge->radius - 20.0) < 1e-12 &&
          same(cable.smallestBendEdge->point, Xyz{30.0, 0.0, 0.0}));
    CHECK(cable.bendLimit() && std::abs(*cable.bendLimit() - 5.0) < 1e-12);
    CHECK(cable.bendState() == FlexItem::BendState::Near);
    // It does not bend over its edge at all: over only where the model
    // warns of it.
    CHECK(!cable.edgeBendLimit());
    CHECK(cable.edgeBendState() == FlexItem::BendState::Clear);
    flat.insert(QStringLiteral("problems"),
                QJsonArray{QJsonObject{
                    {QStringLiteral("kind"), QStringLiteral("edge_bend_too_tight")},
                    {QStringLiteral("message"),
                     QStringLiteral("the item bends over its edge more tightly than its type allows")},
                    {QStringLiteral("places"), QJsonArray{}}}});
    cable = FlexItem::read(flat);
    CHECK(cable.hasProblem(QStringLiteral("edge_bend_too_tight")));
    CHECK(cable.edgeBendState() == FlexItem::BendState::Over);
    CHECK(cable.bendState() == FlexItem::BendState::Near);
    // With a radius over the edge, against it.
    flat.insert(QStringLiteral("bend_radius_edge"), 50.0);
    cable = FlexItem::read(flat);
    CHECK(cable.edgeBendLimit() && std::abs(*cable.edgeBendLimit() - 50.0) < 1e-12);
    CHECK(cable.edgeBendState() == FlexItem::BendState::Over);
    flat.insert(QStringLiteral("smallest_bend_edge"), bendJson(55.0, 30.0));
    CHECK(FlexItem::read(flat).edgeBendState() == FlexItem::BendState::Near);
    CHECK(std::abs(*FlexItem::read(flat).bendLimit() - 5.0) < 1e-12);
  }

  // A pointer move's preview builds no solid.
  const QJsonObject def = object(kDef);
  CHECK(routeOnly(def).value(QStringLiteral("route_only")).toBool() && !def.contains(QStringLiteral("route_only")));
  CHECK(routeOnly(def).value(QStringLiteral("nodes")) == def.value(QStringLiteral("nodes")));

  // A node dropped on an edge, a vertex or a point is attached to it.
  item = FlexItem::read(object(kAnswer));
  const QJsonObject edge{{QStringLiteral("body"), QStringLiteral("F3.b0")},
                         {QStringLiteral("edge"), QStringLiteral("E{F3:top|F3:side1}")}};
  QJsonObject cmd = dragCommand(item, QStringLiteral("n4"), {NodeTarget::Kind::Link, {40.0, 20.0, 0.0}, edge});
  CHECK(cmd.value(QStringLiteral("cmd")).toString() == QStringLiteral("flex.attach_node") &&
        cmd.value(QStringLiteral("on")).toObject() == edge && cmd.contains(QStringLiteral("at")));
  cmd = dragCommand(item, QStringLiteral("n2"), {NodeTarget::Kind::Link, {0.0, 0.0, 0.0}, QStringLiteral("origin")});
  CHECK(cmd.value(QStringLiteral("on")).toString() == QStringLiteral("origin"));
  cmd = addNodeCommand(QStringLiteral("F7"), {NodeTarget::Kind::Link, {40.0, 20.0, 0.0}, edge}, QStringLiteral("end"));
  CHECK(cmd.value(QStringLiteral("on")).toObject() == edge && !cmd.contains(QStringLiteral("port")));
  CHECK(sameReference(QStringLiteral("origin"), QStringLiteral("origin")) &&
        !sameReference(QStringLiteral("origin"), edge));
  CHECK(targetLog({NodeTarget::Kind::Link, {}, QStringLiteral("origin")}) == QStringLiteral("link origin"));
}

// The owner's case (mitcad#1713): a node dragged along a profile, the item
// bent tighter than its bend radius; the edit was taken to be undone. An
// item made with warnings is kept; only an item that newly failed (no solid
// or no route at all) is taken back, with the words a person reads.
void failureTests() {
  // Made, with a warning: kept.
  const QJsonObject warned = object(R"({
    "error": null, "flex": "F4", "node": "n4", "length": 992.8,
    "warnings": ["the item bends more tightly than its bend radius allows"],
    "notices": [{"level": "warning", "message": "the item bends more tightly than its bend radius allows"}]})");
  CHECK(editFailure(warned, QStringLiteral("F4")).isEmpty());
  // Another feature failed already before, or another one newly: not this
  // item's failure.
  const QJsonObject elsewhere = object(R"({"failed_features": [
    {"uid": "F4", "name": "Flex1", "error": "no route", "edited": true, "new": false},
    {"uid": "F6", "name": "Fillet1", "error": "fillet failed", "edited": false, "new": true}]})");
  CHECK(editFailure(elsewhere, QStringLiteral("F4")).isEmpty());
  // This item newly failed: the person's words, else the agents'.
  const QJsonObject failed = object(R"({"failed_features": [
    {"uid": "F4", "name": "Flex1", "error": "F4: no route could be made: x",
     "message": "no route could be made", "edited": true, "new": true}]})");
  CHECK(editFailure(failed, QStringLiteral("F4")) == QStringLiteral("no route could be made"));
  const QJsonObject bare = object(R"({"failed_features": [
    {"uid": "F4", "error": "the route has no length", "edited": true, "new": true}]})");
  CHECK(editFailure(bare, QStringLiteral("F4")) == QStringLiteral("the route has no length"));
}

// The edit session (owner 14.11.2026): the handles' commands change the
// session's definition, not the model, until OK.
void sessionTests() {
  const QJsonObject def = object(kDef);
  const auto nodes = [](const QJsonObject& changed) { return changed.value(QStringLiteral("nodes")).toArray(); };
  const auto row = [&nodes](const QJsonObject& changed, int i) { return nodes(changed).at(i).toObject(); };
  const QJsonObject face{{QStringLiteral("body"), QStringLiteral("F3.b0")}, {QStringLiteral("face"), QStringLiteral("F3:top")}};
  // Moved on its face: its place, its face kept.
  QJsonObject changed = withNodeCommand(def, object(R"({"cmd": "flex.move_node", "flex": "F7", "node": "n2", "at": [35, 1, 0]})"));
  CHECK(nodes(changed).size() == 3 && same(*xyzOf(row(changed, 1).value(QStringLiteral("at"))), {35.0, 1.0, 0.0}));
  CHECK(row(changed, 1).value(QStringLiteral("on")).toObject() == face);
  // Free: its face gone; onto a face: that face.
  changed = withNodeCommand(def, object(R"({"cmd": "flex.attach_node", "flex": "F7", "node": "n2", "free": true})"));
  CHECK(!row(changed, 1).contains(QStringLiteral("on")) && row(changed, 1).contains(QStringLiteral("at")));
  changed = withNodeCommand(def, object(R"({"cmd": "flex.attach_node", "flex": "F7", "node": "n4",
                                            "on": {"body": "F3.b0", "face": "F3:top"}, "at": [50, 30, 0]})"));
  CHECK(row(changed, 2).value(QStringLiteral("on")).toObject() == face &&
        same(*xyzOf(row(changed, 2).value(QStringLiteral("at"))), {50.0, 30.0, 0.0}) &&
        std::abs(row(changed, 2).value(QStringLiteral("twist")).toDouble() - 0.5) < 1e-12);
  // Added after n2 with the next free id, at the start, at the end.
  QString made;
  changed = withNodeCommand(def, object(R"({"cmd": "flex.add_node", "flex": "F7", "at": [40, 15, 0], "place": {"after": "n2"}})"),
                            &made);
  CHECK(made == QStringLiteral("n5") && nodes(changed).size() == 4 &&
        row(changed, 2).value(QStringLiteral("id")).toString() == made);
  changed = withNodeCommand(def, object(R"({"cmd": "flex.add_node", "flex": "F7", "at": [-10, 0, 0], "place": "start"})"));
  CHECK(row(changed, 0).value(QStringLiteral("id")).toString() == QStringLiteral("n5"));
  changed = withNodeCommand(def, object(R"({"cmd": "flex.add_node", "flex": "F7", "at": [40, 50, 0], "place": "end"})"));
  CHECK(row(changed, 3).value(QStringLiteral("id")).toString() == QStringLiteral("n5"));
  // Deleted, never below two; a twist; a node or a command that is none.
  changed = withNodeCommand(def, object(R"({"cmd": "flex.delete_node", "flex": "F7", "node": "n2"})"));
  CHECK(nodes(changed).size() == 2 && row(changed, 1).value(QStringLiteral("id")).toString() == QStringLiteral("n4"));
  CHECK(withNodeCommand(changed, object(R"({"cmd": "flex.delete_node", "flex": "F7", "node": "n4"})")).isEmpty());
  changed = withNodeCommand(def, object(R"({"cmd": "flex.set_twist", "flex": "F7", "node": "n2", "twist": 1.5})"));
  CHECK(std::abs(row(changed, 1).value(QStringLiteral("twist")).toDouble() - 1.5) < 1e-12);
  CHECK(withNodeCommand(def, object(R"({"cmd": "flex.move_node", "flex": "F7", "node": "n9", "at": [0, 0, 0]})")).isEmpty());
  CHECK(withNodeCommand(def, object(R"({"cmd": "edit_feature", "uid": "F7"})")).isEmpty());
  // The rest of the definition stays.
  CHECK(changed.value(QStringLiteral("item")) == def.value(QStringLiteral("item")));

  // The handles stand on the session's definition until its route is read.
  const FlexItem committed = FlexItem::read(object(kAnswer));
  const FlexItem held = itemOfDefinition(def, committed);
  CHECK(held.valid && held.uid == QStringLiteral("F7") && held.round && std::abs(held.width - 10.0) < 1e-12);
  CHECK(held.nodes.size() == 3 && !held.hasRoute());
  CHECK(held.nodes[0].support == QStringLiteral("port") && held.nodes[1].support == QStringLiteral("face") &&
        held.nodes[2].support == QStringLiteral("free"));
  CHECK(held.nodes[1].ref.toObject() == face && std::abs(held.nodes[2].twist - 0.5) < 1e-12);
  CHECK(same(held.nodes[2].point, {40.0, 30.0, 0.0}));
}

} // namespace

int flexEditTests() {
  failures = 0;
  readTests();
  commandTests();
  definitionTests();
  routeTests();
  twistTests();
  secondRoundTests();
  failureTests();
  sessionTests();
  return failures;
}
MITCAD_UNIT_TEST(flexEditTests);
