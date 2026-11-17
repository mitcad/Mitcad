// SPDX-License-Identifier: MIT
// The Flex command's route while it is laid (mitcad#1713,
// commands/FlexRoute.hpp): the end port a click takes (the cap itself, the
// end of the pipe's side within the cap's reach, never the far end), the
// nodes' ids and fields in the definition, a double click's same place, a
// port inside the route, and the item of the panel: the type's own with
// its origin while nothing differs, a copy without it otherwise; the
// section an item started at an end port takes (the pipe's or item's size).
#include "UnitTests.hpp"
#include "commands/FlexRoute.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "flex_route.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::cmd;

QJsonArray xyz(double x, double y, double z) { return flexXyz(gp_Pnt(x, y, z)); }

QJsonObject portRow(const char* end, const char* face, const QJsonArray& position, const QJsonArray& direction) {
  QJsonArray size;
  size.append(8.0);
  size.append(8.0);
  return {{QStringLiteral("feature"), QStringLiteral("F5")},
          {QStringLiteral("kind"), QStringLiteral("pipe")},
          {QStringLiteral("end"), QString::fromLatin1(end)},
          {QStringLiteral("ref"),
           QJsonObject{{QStringLiteral("body"), QStringLiteral("F5.b0")}, {QStringLiteral("face"), QString::fromLatin1(face)}}},
          {QStringLiteral("position"), position},
          {QStringLiteral("direction"), direction},
          {QStringLiteral("size"), size}};
}

// A pipe from the origin 40 mm along +X, 8 mm across.
std::vector<FlexPort> pipePorts() {
  QJsonArray rows;
  rows.append(portRow("start", "F5:start", xyz(0, 0, 0), xyz(-1, 0, 0)));
  rows.append(portRow("end", "F5:end", xyz(40, 0, 0), xyz(1, 0, 0)));
  // A row without a face is not a port.
  rows.append(QJsonObject{{QStringLiteral("feature"), QStringLiteral("F9")}});
  return flexPorts(rows);
}

void portTests() {
  const std::vector<FlexPort> ports = pipePorts();
  CHECK(ports.size() == 2);
  CHECK(flexPortsOf(ports, QStringLiteral("F5.b0")).size() == 2);
  CHECK(flexPortsOf(ports, QStringLiteral("F6.b0")).empty());
  if (ports.size() != 2) {
    return;
  }
  CHECK(std::abs(ports[1].reach - 8.0) < 1e-12);
  CHECK(ports[1].kind == QStringLiteral("pipe"));
  CHECK(std::abs(ports[1].width - 8.0) < 1e-12 && std::abs(ports[1].height - 8.0) < 1e-12);
  CHECK(ports[1].direction.IsEqual(gp_Dir(1, 0, 0), 1e-12));

  // The cap itself, wherever on it.
  auto port = flexPortAt(ports, QStringLiteral("F5.b0"), QStringLiteral("F5:end"), gp_Pnt(40, 3, 0));
  CHECK(port && port->end == QStringLiteral("end"));
  // The side of the pipe near its end: the end's port; near its start: the start's.
  port = flexPortAt(ports, QStringLiteral("F5.b0"), QStringLiteral("F5:side0"), gp_Pnt(36, 4, 0));
  CHECK(port && port->end == QStringLiteral("end"));
  port = flexPortAt(ports, QStringLiteral("F5.b0"), QStringLiteral("F5:side0"), gp_Pnt(2, 0, 4));
  CHECK(port && port->end == QStringLiteral("start"));
  // The middle of the pipe is a face like any other.
  CHECK(!flexPortAt(ports, QStringLiteral("F5.b0"), QStringLiteral("F5:side0"), gp_Pnt(20, 4, 0)));
  // An edge or a vertex (no face name) at the end takes the port too.
  CHECK(flexPortAt(ports, QStringLiteral("F5.b0"), QString(), gp_Pnt(40, 4, 0)).has_value());
  // Another body's face at the same place does not.
  CHECK(!flexPortAt(ports, QStringLiteral("F6.b0"), QStringLiteral("F6:top"), gp_Pnt(40, 0, 0)));
}

void nodeTests() {
  QJsonObject start = flexFreeNode(gp_Pnt(40, 0, 0));
  start.insert(QStringLiteral("port"),
               QJsonObject{{QStringLiteral("body"), QStringLiteral("F5.b0")}, {QStringLiteral("face"), QStringLiteral("F5:end")}});
  QJsonObject onTop = flexFreeNode(gp_Pnt(60, 10, 20));
  onTop.insert(QStringLiteral("on"),
               QJsonObject{{QStringLiteral("body"), QStringLiteral("F3.b0")}, {QStringLiteral("face"), QStringLiteral("F3:top")}});
  onTop.insert(QStringLiteral("pick"), QJsonObject{{QStringLiteral("owner"), QStringLiteral("F3.b0")}});
  QJsonObject kept = flexFreeNode(gp_Pnt(80, 10, 5));
  kept.insert(QStringLiteral("id"), QStringLiteral("n2"));
  kept.insert(QStringLiteral("twist"), 3.14159265358979);
  const QJsonObject free = flexFreeNode(gp_Pnt(90, 40, 0));

  CHECK(flexIsPort(start));
  CHECK(!flexIsPort(onTop));

  // The same place: within the tolerance on the same thing.
  QJsonObject nearTop = onTop;
  nearTop.insert(QStringLiteral("at"), xyz(60.2, 10, 20));
  CHECK(flexSamePlace(onTop, nearTop, 0.5));
  CHECK(!flexSamePlace(onTop, nearTop, 0.1));
  QJsonObject freeThere = flexFreeNode(gp_Pnt(60, 10, 20));
  CHECK(!flexSamePlace(onTop, freeThere, 0.5)); // free is not on the face
  CHECK(flexSamePlace(start, start, 0.0));

  QJsonArray nodes;
  nodes.append(start);
  nodes.append(onTop);
  nodes.append(kept);
  nodes.append(free);
  const QJsonArray defined = flexDefinitionNodes(nodes);
  CHECK(defined.size() == 4);
  // New nodes take the free ids around the kept one.
  CHECK(defined.at(0).toObject().value(QStringLiteral("id")).toString() == QStringLiteral("n1"));
  CHECK(defined.at(1).toObject().value(QStringLiteral("id")).toString() == QStringLiteral("n3"));
  CHECK(defined.at(2).toObject().value(QStringLiteral("id")).toString() == QStringLiteral("n2"));
  CHECK(defined.at(3).toObject().value(QStringLiteral("id")).toString() == QStringLiteral("n4"));
  // Only what the model stores: no pick, a twist only when there is one.
  CHECK(defined.at(0).toObject().contains(QStringLiteral("port")));
  CHECK(!defined.at(0).toObject().contains(QStringLiteral("on")));
  CHECK(defined.at(1).toObject().contains(QStringLiteral("on")));
  CHECK(!defined.at(1).toObject().contains(QStringLiteral("pick")));
  CHECK(!defined.at(1).toObject().contains(QStringLiteral("twist")));
  CHECK(std::abs(defined.at(2).toObject().value(QStringLiteral("twist")).toDouble() - 3.14159265358979) < 1e-12);
  CHECK(!defined.at(3).toObject().contains(QStringLiteral("on")));
  CHECK(flexPoint(defined.at(3).toObject().value(QStringLiteral("at"))).Distance(gp_Pnt(90, 40, 0)) < 1e-12);

  // A port only at the ends.
  CHECK(!flexPortInside(nodes));
  QJsonArray through;
  through.append(free);
  through.append(start);
  through.append(onTop);
  CHECK(flexPortInside(through));
  QJsonArray toPort;
  toPort.append(free);
  toPort.append(start);
  CHECK(!flexPortInside(toPort));
}

void itemTests() {
  const QJsonObject type{
      {QStringLiteral("id"), QStringLiteral("hose_10")},
      {QStringLiteral("library"), QStringLiteral("built_in")},
      {QStringLiteral("built_in"), true},
      {QStringLiteral("name"), QStringLiteral("Hose 10")},
      {QStringLiteral("section"), QJsonObject{{QStringLiteral("type"), QStringLiteral("round")}, {QStringLiteral("diameter"), 10.0}}},
      {QStringLiteral("wall"), 1.5},
      {QStringLiteral("bend_radius"), 30.0}};
  const QJsonObject item = flexItemOfType(type);
  CHECK(!item.contains(QStringLiteral("id")) && !item.contains(QStringLiteral("built_in")));
  CHECK(item.value(QStringLiteral("origin")).toObject().value(QStringLiteral("id")).toString() == QStringLiteral("hose_10"));
  CHECK(item.value(QStringLiteral("origin")).toObject().value(QStringLiteral("library")).toString() ==
        QStringLiteral("built_in"));
  CHECK(flexIsRound(item));

  FlexSection shown = flexSectionOf(item);
  CHECK(shown.type == QStringLiteral("round") && shown.hollow);
  CHECK(std::abs(shown.diameter - 10.0) < 1e-12 && std::abs(shown.wall - 1.5) < 1e-12);

  // Nothing changed: the type's item, origin and all.
  CHECK(flexItemWith(item, shown, QString()) == item);

  // Another diameter: a copy without the origin, the rest kept.
  FlexSection wider = shown;
  wider.diameter = 12.0;
  QJsonObject changed = flexItemWith(item, wider, QString());
  CHECK(!changed.contains(QStringLiteral("origin")));
  CHECK(changed.value(QStringLiteral("section")).toObject().value(QStringLiteral("diameter")).toDouble() == 12.0);
  CHECK(changed.value(QStringLiteral("bend_radius")).toDouble() == 30.0);
  CHECK(changed.value(QStringLiteral("name")).toString() == QStringLiteral("Hose 10"));

  // Solid: no wall.
  FlexSection solid = shown;
  solid.hollow = false;
  changed = flexItemWith(item, solid, QString());
  CHECK(!changed.contains(QStringLiteral("wall")) && !changed.contains(QStringLiteral("origin")));

  // An appearance of its own.
  FlexSection red = shown;
  red.appearance = QStringLiteral("red_plastic");
  changed = flexItemWith(item, red, QString());
  CHECK(changed.value(QStringLiteral("appearance")).toString() == QStringLiteral("red_plastic"));
  CHECK(!changed.contains(QStringLiteral("origin")));

  // A flat cable out of it: a rectangle, named.
  FlexSection flat;
  flat.type = QStringLiteral("rectangular");
  flat.width = 20.0;
  flat.height = 2.0;
  changed = flexItemWith(item, flat, QStringLiteral("Flat 20"));
  CHECK(!flexIsRound(changed));
  CHECK(changed.value(QStringLiteral("name")).toString() == QStringLiteral("Flat 20"));
  CHECK(!changed.value(QStringLiteral("section")).toObject().contains(QStringLiteral("corner_radius")));
  CHECK(!changed.contains(QStringLiteral("wall")));
  const FlexSection back = flexSectionOf(changed);
  CHECK(back.type == QStringLiteral("rectangular") && back.width == 20.0 && back.height == 2.0 && !back.hollow);

  // And round again: the limits of a flat section go.
  QJsonObject cable = changed;
  cable.insert(QStringLiteral("bend_radius_edge"), 80.0);
  cable.insert(QStringLiteral("max_twist"), 0.05);
  changed = flexItemWith(cable, shown, QString());
  CHECK(flexIsRound(changed));
  CHECK(!changed.contains(QStringLiteral("bend_radius_edge")) && !changed.contains(QStringLiteral("max_twist")));

  // An outline keeps its loops whatever the sizes say.
  QJsonObject outlined = item;
  outlined.insert(QStringLiteral("section"),
                  QJsonObject{{QStringLiteral("type"), QStringLiteral("outline")}, {QStringLiteral("loops"), QJsonArray()}});
  outlined.remove(QStringLiteral("wall"));
  const FlexSection outline = flexSectionOf(outlined);
  CHECK(outline.type == QStringLiteral("outline"));
  CHECK(flexItemWith(outlined, outline, QString()) == outlined);
}

// An end port of `kind` with a cap of `width` x `height`.
FlexPort capOf(const char* kind, double width, double height) {
  FlexPort port;
  port.feature = QStringLiteral("F5");
  port.kind = QString::fromLatin1(kind);
  port.end = QStringLiteral("end");
  port.width = width;
  port.height = height;
  port.reach = std::max(width, height);
  return port;
}

// The `feature` query's answer for a pipe of `section`.
QJsonObject pipeFeature(const char* section, double size) {
  return {{QStringLiteral("uid"), QStringLiteral("F5")},
          {QStringLiteral("type"), QStringLiteral("pipe")},
          {QStringLiteral("def"), QJsonObject{{QStringLiteral("type"), QStringLiteral("pipe")},
                                              {QStringLiteral("section"), QString::fromLatin1(section)},
                                              {QStringLiteral("size"), size},
                                              {QStringLiteral("thickness"), 1.0}}}};
}

// An item started at an end port takes the size there (owner 13.11.2026).
void portSectionTests() {
  FlexSection hose; // the panel's section: a 10 mm hose with a 1.5 mm wall
  hose.diameter = 10.0;
  hose.hollow = true;
  hose.wall = 1.5;
  hose.appearance = QStringLiteral("black_rubber");

  // A 12 mm round pipe (hollow or not, its cap's outer size): a 12 mm item,
  // the type's wall and appearance kept.
  auto section = flexSectionAtPort(capOf("pipe", 12.0, 12.0), pipeFeature("circular", 12.0), hose);
  CHECK(section && section->type == QStringLiteral("round"));
  CHECK(section && std::abs(section->diameter - 12.0) < 1e-12);
  CHECK(section && section->hollow && std::abs(section->wall - 1.5) < 1e-12);
  CHECK(section && section->appearance == QStringLiteral("black_rubber"));
  // A solid type stays solid.
  FlexSection wire = hose;
  wire.hollow = false;
  section = flexSectionAtPort(capOf("pipe", 12.0, 12.0), pipeFeature("circular", 12.0), wire);
  CHECK(section && !section->hollow && std::abs(section->diameter - 12.0) < 1e-12);
  // A pipe whose section is left out of its definition is round.
  QJsonObject plain = pipeFeature("circular", 12.0);
  QJsonObject def = plain.value(QStringLiteral("def")).toObject();
  def.remove(QStringLiteral("section"));
  plain.insert(QStringLiteral("def"), def);
  section = flexSectionAtPort(capOf("pipe", 12.0, 12.0), plain, hose);
  CHECK(section && section->type == QStringLiteral("round") && std::abs(section->diameter - 12.0) < 1e-12);

  // A square pipe: a rectangle of its width and height.
  section = flexSectionAtPort(capOf("pipe", 12.0, 12.0), pipeFeature("square", 12.0), hose);
  CHECK(section && section->type == QStringLiteral("rectangular"));
  CHECK(section && std::abs(section->width - 12.0) < 1e-12 && std::abs(section->height - 12.0) < 1e-12);
  CHECK(section && section->cornerRadius == 0.0);

  // A triangular pipe 9 mm high (its side 2 * 9 / sqrt(3)): the round
  // section round the triangle, 4/3 of its height.
  const double side = 2.0 * 9.0 / std::sqrt(3.0);
  section = flexSectionAtPort(capOf("pipe", side, 9.0), pipeFeature("triangular", 9.0), hose);
  CHECK(section && section->type == QStringLiteral("round"));
  CHECK(section && std::abs(section->diameter - 12.0) < 1e-9);

  // A wall the smaller section has no room for: solid.
  section = flexSectionAtPort(capOf("pipe", 3.0, 3.0), pipeFeature("circular", 3.0), hose);
  CHECK(section && !section->hollow && std::abs(section->diameter - 3.0) < 1e-12);

  // A Flex item's end: its section, a flat one with its corners.
  const QJsonObject flat{
      {QStringLiteral("uid"), QStringLiteral("F5")},
      {QStringLiteral("type"), QStringLiteral("flex")},
      {QStringLiteral("def"),
       QJsonObject{{QStringLiteral("type"), QStringLiteral("flex")},
                   {QStringLiteral("item"),
                    QJsonObject{{QStringLiteral("section"), QJsonObject{{QStringLiteral("type"), QStringLiteral("rectangular")},
                                                                        {QStringLiteral("width"), 25.0},
                                                                        {QStringLiteral("height"), 1.5},
                                                                        {QStringLiteral("corner_radius"), 0.5}}}}}}}};
  section = flexSectionAtPort(capOf("flex", 25.0, 1.5), flat, wire);
  CHECK(section && section->type == QStringLiteral("rectangular"));
  CHECK(section && section->width == 25.0 && section->height == 1.5 && section->cornerRadius == 0.5);
  // A flat item's end with the hose's wall: no room for it.
  section = flexSectionAtPort(capOf("flex", 25.0, 1.5), flat, hose);
  CHECK(section && !section->hollow);
  // An outline item's end: nothing the panel's sizes give.
  QJsonObject outlined = flat;
  outlined.insert(QStringLiteral("def"),
                  QJsonObject{{QStringLiteral("type"), QStringLiteral("flex")},
                              {QStringLiteral("item"),
                               QJsonObject{{QStringLiteral("section"), QJsonObject{{QStringLiteral("type"), QStringLiteral("outline")},
                                                                                   {QStringLiteral("loops"), QJsonArray()}}}}}});
  CHECK(!flexSectionAtPort(capOf("flex", 25.0, 1.5), outlined, hose));

  // Without the feature's definition (the query failed): equal sizes round,
  // others a rectangle; a cap without a size gives nothing.
  section = flexSectionAtPort(capOf("pipe", 12.0, 12.0), QJsonObject(), hose);
  CHECK(section && section->type == QStringLiteral("round") && std::abs(section->diameter - 12.0) < 1e-12);
  section = flexSectionAtPort(capOf("flex", 20.0, 2.0), QJsonObject(), wire);
  CHECK(section && section->type == QStringLiteral("rectangular") && section->width == 20.0);
  CHECK(!flexSectionAtPort(capOf("pipe", 0.0, 0.0), pipeFeature("circular", 12.0), hose));
}

} // namespace

int flexRouteTests() {
  failures = 0;
  portTests();
  nodeTests();
  itemTests();
  portSectionTests();
  if (failures == 0) {
    std::printf("flex route tests passed\n");
  }
  return failures;
}
MITCAD_UNIT_TEST(flexRouteTests);
