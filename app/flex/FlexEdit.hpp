// SPDX-License-Identifier: MIT
#pragma once

// Editing a Flex item directly in the 3D view (mitcad#1713, app/COMMANDS.md
// "Flex editing"), the part without the view: the `flex` query's answer as
// the handles need it, where a pulled node goes (it stays on what holds it,
// takes another face, an end port, or is free in space), the `flex.*`
// command a release sends and the definition a live preview evaluates, the
// place on the route a new node is put in, and the twist a turned handle
// gives. The view's part is FlexEditController.hpp.
// Plain Qt Core.

#include <array>
#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

namespace mitcad::flex {

using Xyz = std::array<double, 3>;

Xyz add(const Xyz& a, const Xyz& b);
Xyz sub(const Xyz& a, const Xyz& b);
Xyz scaled(const Xyz& a, double by);
double dot(const Xyz& a, const Xyz& b);
Xyz cross(const Xyz& a, const Xyz& b);
double length(const Xyz& a);
double distance(const Xyz& a, const Xyz& b);
// The unit vector along `a`; `fallback` for one shorter than 1e-12.
Xyz unit(const Xyz& a, const Xyz& fallback = Xyz{0.0, 0.0, 1.0});
// `v` turned by `angle` radians, right hand about the unit `axis`.
Xyz turned(const Xyz& v, const Xyz& axis, double angle);
// The point of an array of at least three numbers; none for anything else.
std::optional<Xyz> xyzOf(const QJsonValue& value);
QJsonArray xyzJson(const Xyz& point);

// A node as the `flex` query gives it.
struct FlexNode {
  QString id;
  Xyz point{};               // where it is now, in the feature's component
  std::optional<Xyz> normal; // the surface's outward normal, on a face
  double along = 0.0;        // its length along the route
  QString support;           // free, face, edge, point or port
  QJsonValue ref;            // what it is attached to, as stored
  bool lost = false;         // what it was attached to is gone
  double twist = 0.0;

  bool attached() const { return support != QStringLiteral("free") && !support.isEmpty(); }
};

// A sample of the route: where the section is and how it is turned.
struct FlexFrame {
  double along = 0.0;
  Xyz point{};
  Xyz tangent{0.0, 0.0, 1.0};
  Xyz normal{0.0, 1.0, 0.0}; // the section's y
};

// A stretch of the route that lies on a face.
struct FlexRun {
  double from = 0.0;
  double to = 0.0;
  QString body;
  QString face;
};

struct FlexProblem {
  QString kind;
  QString message; // plain words, the model's
  QJsonArray places;
};

// A Flex item as the handles show it.
struct FlexItem {
  QString uid;
  QString name;
  QString status;
  bool valid = false; // the nodes were placed
  // The section: round with its diameter in `width`, else its width and
  // height (0 when the query does not say: an outline, a sketch profile).
  bool round = true;
  double width = 0.0;
  double height = 0.0;
  double length = 0.0;
  std::optional<double> bendRadius;
  std::optional<double> bendRadiusEdge;
  // The tightest bend of the route (`smallest_bend`; none for a straight
  // route): its radius, and where it is. Of a section that is not round
  // its bends over the flat side, and `smallestBendEdge` its bends over
  // the edge (`smallest_bend_edge`; none for a round one).
  struct Bend {
    double radius = 0.0;
    double along = 0.0;
    Xyz point{};
  };
  std::optional<Bend> smallestBend;
  std::optional<Bend> smallestBendEdge;
  // The radius `smallestBend` may not go below: the bend radius (of a
  // round section the edge's when only it is given); none without one.
  std::optional<double> bendLimit() const;
  // The radius `smallestBendEdge` may not go below; none for a round
  // section, or one that does not bend over its edge at all.
  std::optional<double> edgeBendLimit() const;
  // How the tightest bend stands to that limit.
  enum class BendState { Clear, Near, Over };
  // Over: tighter than the model lets pass without its warning (its bend
  // more than kBendSlack times what the limit allows, as the router's
  // check: geometry/src/flex_route.cpp). Near: within a quarter above the
  // limit, or under it within that slack: the route bends at its limit by
  // design, so a bend there is no fault (mitcad#1713: every item was ringed
  // in red or yellow at its bends). Clear without a bend or a limit.
  static constexpr double kBendSlack = 1.15;
  BendState bendState() const;
  // The same over the edge; for a section that does not bend over its
  // edge at all, Over where the model warns of it (`edge_bend_too_tight`).
  BendState edgeBendState() const;
  // Whether the model warns of a problem of the kind.
  bool hasProblem(const QString& kind) const;
  std::vector<FlexNode> nodes;
  std::vector<Xyz> polyline;
  std::vector<FlexFrame> frames;
  std::vector<FlexRun> runs;
  std::vector<FlexProblem> problems;

  // The `flex` query's answer; a previewed one (no item, no refs) takes
  // the section, the refs and the twists of `committed`.
  static FlexItem read(const QJsonObject& answer, const FlexItem* committed = nullptr);

  const FlexNode* node(const QString& id) const;
  int nodeIndex(const QString& id) const;
  bool isEnd(const QString& id) const;
  bool hasRoute() const { return polyline.size() >= 2; }
  // The places of every problem, each once, in order.
  QJsonArray problemPlaces() const;
  // The problems' messages, each once, in order.
  QStringList problemMessages() const;
  // The section's reach across the route as it shows from the side: a
  // round one's diameter, else the larger of the width and the height.
  double reach() const { return round ? width : (width > height ? width : height); }
};

// A place on the route's centre line.
struct RoutePlace {
  Xyz point{};
  double along = 0.0;
};

// The lengths along a polyline up to each of its points.
std::vector<double> lengthsAlong(const std::vector<Xyz>& polyline);
// The point at a length along it (clamped to its ends); the origin for an
// empty polyline.
Xyz pointAlong(const std::vector<Xyz>& polyline, const std::vector<double>& lengths, double along);
// The node a place at `along` comes after: the last node at or before it
// that is not the item's last node; empty with fewer than two nodes.
QString nodeBefore(const FlexItem& item, double along);
// The stretch on a face that holds `along`, when one does.
std::optional<FlexRun> runAt(const FlexItem& item, double along);
// The section's frame at `along`: the nearest sample when the item has
// samples, else the centre line's direction there with a normal square to
// it; none without a route.
std::optional<FlexFrame> frameAt(const FlexItem& item, double along);
// The middle of each stretch between two neighbouring nodes, by the
// length along the route: `after` is the node before it.
struct Stretch {
  QString after;
  RoutePlace middle;
};
std::vector<Stretch> stretches(const FlexItem& item);

// Where a pulled or new node goes.
struct NodeTarget {
  enum class Kind {
    Free, // in space at `at`
    Face, // on the face `ref` ({"body", "face"}; of another component with its occurrence), nearest to `at`
    Port, // on the end port `ref` (a cap face of a Pipe or a Flex item)
    Link, // on the edge, vertex, sketch point, construction point or origin `ref`, nearest to `at`
  };
  Kind kind = Kind::Free;
  Xyz at{};
  QJsonValue ref; // a reference: an object, or a datum's uid

  bool operator==(const NodeTarget& other) const {
    return kind == other.kind && ref == other.ref && distance(at, other.at) <= 1e-9;
  }
  bool operator!=(const NodeTarget& other) const { return !(*this == other); }
};

// Whether two references name the same thing: the same fields but for
// where it was picked (`occurrence`, `context`: the stored reference is
// the plain one), or the same datum; false when either is none.
bool sameReference(const QJsonValue& a, const QJsonValue& b);

// What a release of a pulled node sends: `flex.move_node` when the node
// stays on what holds it (the same face or edge, or free as before),
// `flex.attach_node` when it takes another face, an edge, a point or a
// port or comes off into space; an empty object when nothing would
// change (an end dropped on the port it is on, a node on its own point).
QJsonObject dragCommand(const FlexItem& item, const QString& node, const NodeTarget& target);
// `flex.add_node` at a target: `place` is "end", "start" or
// {"after": "n2"}.
QJsonObject addNodeCommand(const QString& uid, const NodeTarget& target, const QJsonValue& place);
QJsonObject deleteNodeCommand(const QString& uid, const QString& node);
QJsonObject setTwistCommand(const QString& uid, const QString& node, double twist);
// `flex.attach_node` with "free": the node comes off what holds it and
// stays where it is.
QJsonObject detachCommand(const QString& uid, const QString& node);

// The definition a live preview evaluates (an `edit_feature` of it): the
// feature's own with one node put at a target, one node added, or one
// node's twist changed. `def` unchanged when the node is not in it.
QJsonObject withNodeAt(QJsonObject def, const QString& node, const NodeTarget& target);
QJsonObject withNodeAdded(QJsonObject def, const NodeTarget& target, const QJsonValue& place, QString* id = nullptr);
QJsonObject withTwist(QJsonObject def, const QString& node, double twist);
// The item as an edit session's definition holds it (owner 14.11.2026):
// its nodes where their `at` puts them, with their references, supports
// and twists, the uid, name and section of `like` (the committed item); no
// route. The handles read the previewed route with it as `committed`
// (FlexItem::read), and stand on it alone while there is none.
FlexItem itemOfDefinition(const QJsonObject& def, const FlexItem& like);

// The definition a `flex.*` command of the handles makes of `def`, for the
// item's edit session, which keeps its changes in the panel until OK
// (mitcad#1713, owner 14.11.2026): flex.move_node puts the node's `at`
// there (its attachment stays), flex.attach_node gives it the reference or
// the port (`free`: none) and `at` when given, flex.add_node adds a node
// with the next free id (`made`) at its place, flex.delete_node takes it out,
// flex.set_twist sets its twist. Empty when the command is none of them,
// names no node of `def`, or would leave fewer than two nodes.
QJsonObject withNodeCommand(QJsonObject def, const QJsonObject& command, QString* made = nullptr);
QJsonObject editFeatureCommand(const QString& uid, const QJsonObject& def);
// The definition for a pointer move's preview: with `route_only`, so that
// the route is worked out and no solid is built.
QJsonObject routeOnly(QJsonObject def);
// The next free node id of a definition: `n<k>`.
QString nextNodeId(const QJsonObject& def);

// Whether an edit's answer says the item could not be made at all: its
// feature newly failed with the edit (`failed_features`, `new`), as when no
// solid or no route could be made. The words a person reads (the failure's
// `message`, else its `error`), or empty: an item made with warnings (a
// bend tighter than its radius, a body passed through) is made, and the
// edit is kept (mitcad#1713: such an edit was taken to be undone).
QString editFailure(const QJsonObject& answer, const QString& uid);

// A twist snapped to the nearest quarter turn when it is within
// `tolerance` radians of one.
double snappedTwist(double twist, double tolerance);
// The angle from `from` to `to` about the unit `axis`, right hand, in
// (-pi, pi]; 0 when either is along the axis.
double angleAbout(const Xyz& from, const Xyz& to, const Xyz& axis);
// Degrees as the twist handle shows them: "90°", "-12.5°".
QString degreesText(double radians);

// For the log: "(1.000, 2.000, 3.000)".
QString xyzLog(const Xyz& point);
// For the log: "free", "face F1:top of F1.b0", "port F2:end of F2.b0",
// "link {…}" (the reference as it is sent).
QString targetLog(const NodeTarget& target);

} // namespace mitcad::flex
