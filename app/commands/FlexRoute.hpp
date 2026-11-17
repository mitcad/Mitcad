// SPDX-License-Identifier: MIT
#pragma once

// The Flex command's route while it is laid (mitcad#1713; commands.md,
// "Flex"): the nodes the clicks made, the end ports a click snaps to, the
// item its panel describes, and the `flex` definition they give. Plain
// JSON and geometry, without the panel or the model (app/tests/flex_route.cpp).

#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

namespace mitcad::cmd {

QJsonArray flexXyz(const gp_Pnt& point);
gp_Pnt flexPoint(const QJsonValue& xyz);

// An end port of the `end_ports` query: a cap of a Pipe or of a Flex item.
struct FlexPort {
  QString feature;
  QString kind;     // "pipe" or "flex"
  QString end;      // "start" or "end"
  QJsonObject ref;  // {"body", "face"}: what a node's `port` stores
  gp_Pnt position;  // the middle of the cap, in its body's component
  gp_Dir direction = gp_Dir(0, 0, 1); // the cap's outward normal
  double width = 0.0;  // the cap's extent along its `x_axis`
  double height = 0.0; // and across it
  double reach = 0.0;  // the larger of the cap's sizes
};
std::vector<FlexPort> flexPorts(const QJsonArray& answer);
// The ports of a body.
std::vector<FlexPort> flexPortsOf(const std::vector<FlexPort>& ports, const QString& body);
// The port a click on `body` at `at` takes: the one whose cap is the face
// clicked, else the nearest whose middle is within its reach of the click
// (a hollow cap is a thin ring to hit: the end of the pipe's side is the
// port too); none elsewhere.
std::optional<FlexPort> flexPortAt(const std::vector<FlexPort>& ports, const QString& body, const QString& face,
                                   const gp_Pnt& at);

// A node as the command's state keeps it: `at` (in the item's component),
// `on` or `port` (a reference) or neither (free), `twist` (radians), `id`
// (an edited item's node; a new one has none) and `pick`, the item clicked
// ({"kind", "owner", "name", "geometry", "occurrence"}; none for a free
// node).
QJsonObject flexFreeNode(const gp_Pnt& at);
bool flexIsPort(const QJsonObject& node);
// Whether two nodes are one place: both free or attached to the same
// thing, their points within `tolerance` mm.
bool flexSamePlace(const QJsonObject& a, const QJsonObject& b, double tolerance);
// The definition's `nodes`: each with its id (the kept one, else the next
// free `n<k>`), `at`, `on` or `port`, and a `twist` that is not 0.
QJsonArray flexDefinitionNodes(const QJsonArray& nodes);
// Whether a port is at a node that is neither the first nor the last,
// which the model refuses.
bool flexPortInside(const QJsonArray& nodes);

// --- The item

// A type of the `flex_types` query as an item: its fields without the
// library's own (`id`, `library`, `built_in`, `version`), with its `origin`.
QJsonObject flexItemOfType(const QJsonObject& type);
bool flexIsRound(const QJsonObject& item);
// What the panel shows of an item's section and wall.
struct FlexSection {
  QString type = QStringLiteral("round"); // round, rectangular or outline
  double diameter = 0.0;
  double width = 0.0;
  double height = 0.0;
  double cornerRadius = 0.0;
  bool hollow = false;
  double wall = 0.0;
  QString appearance; // an appearance's id; empty: the component's
};
FlexSection flexSectionOf(const QJsonObject& item);
// The item with the panel's section, wall and appearance: `base` itself
// (its origin kept) while they are its own, else a copy with them and
// without the origin, named `name` when that is not empty. An outline
// keeps its loops whatever the sizes say.
QJsonObject flexItemWith(const QJsonObject& base, const FlexSection& section, const QString& name);

// The section an item started at an end port takes (owner 13.11.2026): the
// size of the pipe or Flex item there. `feature` is the `feature` query's
// answer for the port's feature (its `type` and `def`: the shape of the
// section, which `end_ports` does not give); the sizes are the port's. A
// round pipe gives a round section of its outer diameter, a square one a
// rectangle of its width and height, a triangular one the round section
// round the triangle; a Flex item its own section. `current` is the panel's
// section: its wall and appearance stay, the wall dropped (solid) when the
// new section has no room for it. None when the port's section is not one
// the panel shows (an outline item) or has no size.
std::optional<FlexSection> flexSectionAtPort(const FlexPort& port, const QJsonObject& feature,
                                             const FlexSection& current);

} // namespace mitcad::cmd
