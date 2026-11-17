// SPDX-License-Identifier: MIT
#include "FlexEdit.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonDocument>
#include <QLocale>
#include <QRegularExpression>

namespace mitcad::flex {
namespace {

constexpr double kPi = 3.14159265358979323846;

QJsonObject command(const char* name, const QString& uid) {
  return QJsonObject{{QStringLiteral("cmd"), QString::fromLatin1(name)}, {QStringLiteral("flex"), uid}};
}

// The node's fields of a target: its place and what it is attached to.
void putTarget(QJsonObject& fields, const NodeTarget& target) {
  fields.insert(QStringLiteral("at"), xyzJson(target.at));
  if (target.kind == NodeTarget::Kind::Face || target.kind == NodeTarget::Kind::Link) {
    fields.insert(QStringLiteral("on"), target.ref);
  } else if (target.kind == NodeTarget::Kind::Port) {
    fields.insert(QStringLiteral("port"), target.ref);
  }
}

int nodeRow(const QJsonArray& nodes, const QString& id) {
  for (int i = 0; i < nodes.size(); ++i) {
    if (nodes.at(i).toObject().value(QStringLiteral("id")).toString() == id) {
      return i;
    }
  }
  return -1;
}

std::optional<double> numberOf(const QJsonValue& value) {
  return value.isDouble() ? std::optional<double>(value.toDouble()) : std::nullopt;
}

} // namespace

Xyz add(const Xyz& a, const Xyz& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }

Xyz sub(const Xyz& a, const Xyz& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }

Xyz scaled(const Xyz& a, double by) { return {a[0] * by, a[1] * by, a[2] * by}; }

double dot(const Xyz& a, const Xyz& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }

Xyz cross(const Xyz& a, const Xyz& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}

double length(const Xyz& a) { return std::sqrt(dot(a, a)); }

double distance(const Xyz& a, const Xyz& b) { return length(sub(a, b)); }

Xyz unit(const Xyz& a, const Xyz& fallback) {
  const double size = length(a);
  return size < 1e-12 ? fallback : scaled(a, 1.0 / size);
}

Xyz turned(const Xyz& v, const Xyz& axis, double angle) {
  // Rodrigues' rotation.
  const double c = std::cos(angle);
  const double s = std::sin(angle);
  return add(add(scaled(v, c), scaled(cross(axis, v), s)), scaled(axis, dot(axis, v) * (1.0 - c)));
}

std::optional<Xyz> xyzOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() < 3 || !a.at(0).isDouble() || !a.at(1).isDouble() || !a.at(2).isDouble()) {
    return std::nullopt;
  }
  return Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()};
}

QJsonArray xyzJson(const Xyz& point) { return QJsonArray{point[0], point[1], point[2]}; }

FlexItem FlexItem::read(const QJsonObject& answer, const FlexItem* committed) {
  FlexItem item;
  item.uid = answer.value(QStringLiteral("uid")).toString(committed != nullptr ? committed->uid : QString());
  item.name = answer.value(QStringLiteral("name")).toString(committed != nullptr ? committed->name : QString());
  item.status = answer.value(QStringLiteral("status")).toString();
  const QJsonObject section =
      answer.value(QStringLiteral("item")).toObject().value(QStringLiteral("section")).toObject();
  if (!section.isEmpty()) {
    const QString type = section.value(QStringLiteral("type")).toString();
    item.round = type == QStringLiteral("round");
    if (item.round) {
      item.width = section.value(QStringLiteral("diameter")).toDouble();
      item.height = item.width;
    } else {
      item.width = section.value(QStringLiteral("width")).toDouble();
      item.height = section.value(QStringLiteral("height")).toDouble();
    }
  } else if (committed != nullptr) {
    item.round = committed->round;
    item.width = committed->width;
    item.height = committed->height;
  }
  item.length = answer.value(QStringLiteral("length")).toDouble();
  item.bendRadius = numberOf(answer.value(QStringLiteral("bend_radius")));
  item.bendRadiusEdge = numberOf(answer.value(QStringLiteral("bend_radius_edge")));
  if (committed != nullptr && !answer.contains(QStringLiteral("bend_radius"))) {
    item.bendRadius = committed->bendRadius;
    item.bendRadiusEdge = committed->bendRadiusEdge;
  }
  const auto bendOf = [&answer](const QString& key) -> std::optional<Bend> {
    const QJsonObject bend = answer.value(key).toObject();
    const std::optional<Xyz> where = xyzOf(bend.value(QStringLiteral("point")));
    if (!bend.value(QStringLiteral("radius")).isDouble() || !where) {
      return std::nullopt;
    }
    return Bend{bend.value(QStringLiteral("radius")).toDouble(), bend.value(QStringLiteral("along")).toDouble(),
                *where};
  };
  item.smallestBend = bendOf(QStringLiteral("smallest_bend"));
  item.smallestBendEdge = bendOf(QStringLiteral("smallest_bend_edge"));
  for (const QJsonValue& value : answer.value(QStringLiteral("nodes")).toArray()) {
    const QJsonObject json = value.toObject();
    const std::optional<Xyz> point = xyzOf(json.value(QStringLiteral("point")));
    if (!point) {
      continue;
    }
    FlexNode node;
    node.id = json.value(QStringLiteral("id")).toString();
    node.point = *point;
    node.normal = xyzOf(json.value(QStringLiteral("normal")));
    node.along = json.value(QStringLiteral("along")).toDouble();
    node.support = json.value(QStringLiteral("support")).toString(QStringLiteral("free"));
    node.ref = json.value(QStringLiteral("ref"));
    node.lost = json.value(QStringLiteral("lost")).toBool();
    node.twist = json.value(QStringLiteral("twist")).toDouble();
    if (const FlexNode* known = committed != nullptr ? committed->node(node.id) : nullptr) {
      if (!json.contains(QStringLiteral("ref"))) {
        node.ref = known->ref;
      }
      if (!json.contains(QStringLiteral("twist"))) {
        node.twist = known->twist;
      }
    }
    item.nodes.push_back(node);
  }
  item.valid = !item.nodes.empty();
  for (const QJsonValue& value : answer.value(QStringLiteral("polyline")).toArray()) {
    if (const std::optional<Xyz> point = xyzOf(value)) {
      item.polyline.push_back(*point);
    }
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("frames")).toArray()) {
    const QJsonObject json = value.toObject();
    const std::optional<Xyz> point = xyzOf(json.value(QStringLiteral("point")));
    const std::optional<Xyz> tangent = xyzOf(json.value(QStringLiteral("tangent")));
    const std::optional<Xyz> normal = xyzOf(json.value(QStringLiteral("normal")));
    if (point && tangent && normal) {
      item.frames.push_back({json.value(QStringLiteral("along")).toDouble(), *point, unit(*tangent),
                             unit(*normal, Xyz{0.0, 1.0, 0.0})});
    }
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("runs")).toArray()) {
    const QJsonObject json = value.toObject();
    item.runs.push_back({json.value(QStringLiteral("from")).toDouble(), json.value(QStringLiteral("to")).toDouble(),
                         json.value(QStringLiteral("body")).toString(),
                         json.value(QStringLiteral("face")).toString()});
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("problems")).toArray()) {
    const QJsonObject json = value.toObject();
    item.problems.push_back({json.value(QStringLiteral("kind")).toString(),
                             json.value(QStringLiteral("message")).toString(),
                             json.value(QStringLiteral("places")).toArray()});
  }
  return item;
}

const FlexNode* FlexItem::node(const QString& id) const {
  const auto found = std::find_if(nodes.begin(), nodes.end(), [&id](const FlexNode& n) { return n.id == id; });
  return found != nodes.end() ? &*found : nullptr;
}

int FlexItem::nodeIndex(const QString& id) const {
  for (std::size_t i = 0; i < nodes.size(); ++i) {
    if (nodes[i].id == id) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

bool FlexItem::isEnd(const QString& id) const {
  return !nodes.empty() && (nodes.front().id == id || nodes.back().id == id);
}

QJsonArray FlexItem::problemPlaces() const {
  QJsonArray places;
  for (const FlexProblem& problem : problems) {
    for (const QJsonValue& place : problem.places) {
      if (!places.contains(place)) {
        places.append(place);
      }
    }
  }
  return places;
}

std::optional<double> FlexItem::bendLimit() const {
  if (!bendRadius && round) {
    return bendRadiusEdge;
  }
  return bendRadius;
}

std::optional<double> FlexItem::edgeBendLimit() const {
  if (round) {
    return std::nullopt;
  }
  return bendRadiusEdge;
}

namespace {

FlexItem::BendState stateOf(const std::optional<FlexItem::Bend>& bend, const std::optional<double>& limit) {
  if (!bend || !limit || *limit <= 0.0) {
    return FlexItem::BendState::Clear;
  }
  if (bend->radius * FlexItem::kBendSlack < *limit) {
    return FlexItem::BendState::Over;
  }
  return bend->radius < 1.25 * *limit ? FlexItem::BendState::Near : FlexItem::BendState::Clear;
}

} // namespace

FlexItem::BendState FlexItem::bendState() const { return stateOf(smallestBend, bendLimit()); }

FlexItem::BendState FlexItem::edgeBendState() const {
  if (round || !smallestBendEdge) {
    return BendState::Clear;
  }
  if (!edgeBendLimit()) {
    return hasProblem(QStringLiteral("edge_bend_too_tight")) ? BendState::Over : BendState::Clear;
  }
  return stateOf(smallestBendEdge, edgeBendLimit());
}

bool FlexItem::hasProblem(const QString& kind) const {
  return std::any_of(problems.begin(), problems.end(),
                     [&kind](const FlexProblem& problem) { return problem.kind == kind; });
}

QStringList FlexItem::problemMessages() const {
  QStringList messages;
  for (const FlexProblem& problem : problems) {
    if (!problem.message.isEmpty() && !messages.contains(problem.message)) {
      messages << problem.message;
    }
  }
  return messages;
}

std::vector<double> lengthsAlong(const std::vector<Xyz>& polyline) {
  std::vector<double> lengths;
  lengths.reserve(polyline.size());
  double sum = 0.0;
  for (std::size_t i = 0; i < polyline.size(); ++i) {
    if (i > 0) {
      sum += distance(polyline[i - 1], polyline[i]);
    }
    lengths.push_back(sum);
  }
  return lengths;
}

Xyz pointAlong(const std::vector<Xyz>& polyline, const std::vector<double>& lengths, double along) {
  if (polyline.empty() || lengths.size() != polyline.size()) {
    return Xyz{};
  }
  if (along <= 0.0) {
    return polyline.front();
  }
  for (std::size_t i = 1; i < polyline.size(); ++i) {
    if (along <= lengths[i]) {
      const double span = lengths[i] - lengths[i - 1];
      const double t = span > 1e-12 ? (along - lengths[i - 1]) / span : 0.0;
      return add(polyline[i - 1], scaled(sub(polyline[i], polyline[i - 1]), t));
    }
  }
  return polyline.back();
}

QString nodeBefore(const FlexItem& item, double along) {
  if (item.nodes.size() < 2) {
    return QString();
  }
  QString before = item.nodes.front().id;
  for (std::size_t i = 0; i + 1 < item.nodes.size(); ++i) {
    if (item.nodes[i].along <= along + 1e-9) {
      before = item.nodes[i].id;
    }
  }
  return before;
}

std::optional<FlexRun> runAt(const FlexItem& item, double along) {
  for (const FlexRun& run : item.runs) {
    if (along >= run.from - 1e-9 && along <= run.to + 1e-9 && !run.body.isEmpty() && !run.face.isEmpty()) {
      return run;
    }
  }
  return std::nullopt;
}

std::optional<FlexFrame> frameAt(const FlexItem& item, double along) {
  if (!item.frames.empty()) {
    const auto nearest =
        std::min_element(item.frames.begin(), item.frames.end(), [along](const FlexFrame& a, const FlexFrame& b) {
          return std::abs(a.along - along) < std::abs(b.along - along);
        });
    return *nearest;
  }
  if (!item.hasRoute()) {
    return std::nullopt;
  }
  const std::vector<double> lengths = lengthsAlong(item.polyline);
  std::size_t segment = 1;
  while (segment + 1 < item.polyline.size() && lengths[segment] < along) {
    ++segment;
  }
  FlexFrame frame;
  frame.along = std::clamp(along, 0.0, lengths.back());
  frame.point = pointAlong(item.polyline, lengths, along);
  frame.tangent = unit(sub(item.polyline[segment], item.polyline[segment - 1]));
  // Any direction square to the route: no sample says how the section lies.
  const Xyz side = std::abs(frame.tangent[2]) < 0.9 ? Xyz{0.0, 0.0, 1.0} : Xyz{1.0, 0.0, 0.0};
  frame.normal = unit(cross(cross(frame.tangent, side), frame.tangent), Xyz{0.0, 1.0, 0.0});
  return frame;
}

std::vector<Stretch> stretches(const FlexItem& item) {
  std::vector<Stretch> out;
  if (!item.hasRoute() || item.nodes.size() < 2) {
    return out;
  }
  const std::vector<double> lengths = lengthsAlong(item.polyline);
  for (std::size_t i = 0; i + 1 < item.nodes.size(); ++i) {
    const double middle = 0.5 * (item.nodes[i].along + item.nodes[i + 1].along);
    out.push_back({item.nodes[i].id, {pointAlong(item.polyline, lengths, middle), middle}});
  }
  return out;
}

bool sameReference(const QJsonValue& a, const QJsonValue& b) {
  if (a.isObject() && b.isObject()) {
    QJsonObject first = a.toObject();
    QJsonObject second = b.toObject();
    for (QJsonObject* reference : {&first, &second}) {
      reference->remove(QStringLiteral("occurrence"));
      reference->remove(QStringLiteral("context"));
    }
    return !first.isEmpty() && first == second;
  }
  return a.isString() && b.isString() && !a.toString().isEmpty() && a.toString() == b.toString();
}

QJsonObject dragCommand(const FlexItem& item, const QString& node, const NodeTarget& target) {
  const FlexNode* pulled = item.node(node);
  if (pulled == nullptr) {
    return QJsonObject();
  }
  QJsonObject move = command("flex.move_node", item.uid);
  move.insert(QStringLiteral("node"), node);
  move.insert(QStringLiteral("at"), xyzJson(target.at));
  QJsonObject attach = command("flex.attach_node", item.uid);
  attach.insert(QStringLiteral("node"), node);
  const bool sameHold = !pulled->lost && sameReference(pulled->ref, target.ref);
  switch (target.kind) {
  case NodeTarget::Kind::Free:
    if (pulled->support == QStringLiteral("free")) {
      return move;
    }
    attach.insert(QStringLiteral("free"), true);
    attach.insert(QStringLiteral("at"), xyzJson(target.at));
    return attach;
  case NodeTarget::Kind::Face:
    if (pulled->support == QStringLiteral("face") && sameHold) {
      return move; // it slides on its face
    }
    attach.insert(QStringLiteral("on"), target.ref);
    attach.insert(QStringLiteral("at"), xyzJson(target.at));
    return attach;
  case NodeTarget::Kind::Link:
    if (pulled->support == QStringLiteral("edge") && sameHold) {
      return move; // it slides along its edge
    }
    if (pulled->support == QStringLiteral("point") && sameHold) {
      return QJsonObject(); // on the point it is on
    }
    attach.insert(QStringLiteral("on"), target.ref);
    attach.insert(QStringLiteral("at"), xyzJson(target.at));
    return attach;
  case NodeTarget::Kind::Port:
    if (pulled->support == QStringLiteral("port") && sameHold) {
      return QJsonObject(); // dropped on the port it is on
    }
    attach.insert(QStringLiteral("port"), target.ref);
    return attach;
  }
  return QJsonObject();
}

QJsonObject addNodeCommand(const QString& uid, const NodeTarget& target, const QJsonValue& place) {
  QJsonObject add = command("flex.add_node", uid);
  putTarget(add, target);
  add.insert(QStringLiteral("place"), place);
  return add;
}

QJsonObject deleteNodeCommand(const QString& uid, const QString& node) {
  QJsonObject remove = command("flex.delete_node", uid);
  remove.insert(QStringLiteral("node"), node);
  return remove;
}

QJsonObject setTwistCommand(const QString& uid, const QString& node, double twist) {
  QJsonObject set = command("flex.set_twist", uid);
  set.insert(QStringLiteral("node"), node);
  set.insert(QStringLiteral("twist"), twist);
  return set;
}

QJsonObject detachCommand(const QString& uid, const QString& node) {
  QJsonObject attach = command("flex.attach_node", uid);
  attach.insert(QStringLiteral("node"), node);
  attach.insert(QStringLiteral("free"), true);
  return attach;
}

QJsonObject withNodeAt(QJsonObject def, const QString& node, const NodeTarget& target) {
  QJsonArray nodes = def.value(QStringLiteral("nodes")).toArray();
  const int row = nodeRow(nodes, node);
  if (row < 0) {
    return def;
  }
  QJsonObject fields = nodes.at(row).toObject();
  fields.remove(QStringLiteral("on"));
  fields.remove(QStringLiteral("port"));
  putTarget(fields, target);
  nodes.replace(row, fields);
  def.insert(QStringLiteral("nodes"), nodes);
  return def;
}

QString nextNodeId(const QJsonObject& def) {
  static const QRegularExpression pattern(QStringLiteral("^n([0-9]+)$"));
  int largest = 0;
  for (const QJsonValue& node : def.value(QStringLiteral("nodes")).toArray()) {
    const QRegularExpressionMatch match = pattern.match(node.toObject().value(QStringLiteral("id")).toString());
    if (match.hasMatch()) {
      largest = std::max(largest, match.captured(1).toInt());
    }
  }
  return QStringLiteral("n%1").arg(largest + 1);
}

QJsonObject withNodeAdded(QJsonObject def, const NodeTarget& target, const QJsonValue& place, QString* id) {
  QJsonArray nodes = def.value(QStringLiteral("nodes")).toArray();
  QJsonObject fields;
  const QString made = nextNodeId(def);
  fields.insert(QStringLiteral("id"), made);
  putTarget(fields, target);
  if (id != nullptr) {
    *id = made;
  }
  if (place.isObject()) {
    const int row = nodeRow(nodes, place.toObject().value(QStringLiteral("after")).toString());
    nodes.insert(row < 0 ? nodes.size() : row + 1, fields);
  } else if (place.toString() == QStringLiteral("start")) {
    nodes.prepend(fields);
  } else {
    nodes.append(fields);
  }
  def.insert(QStringLiteral("nodes"), nodes);
  return def;
}

QJsonObject withTwist(QJsonObject def, const QString& node, double twist) {
  QJsonArray nodes = def.value(QStringLiteral("nodes")).toArray();
  const int row = nodeRow(nodes, node);
  if (row < 0) {
    return def;
  }
  QJsonObject fields = nodes.at(row).toObject();
  fields.insert(QStringLiteral("twist"), twist);
  nodes.replace(row, fields);
  def.insert(QStringLiteral("nodes"), nodes);
  return def;
}

FlexItem itemOfDefinition(const QJsonObject& def, const FlexItem& like) {
  FlexItem item;
  item.uid = like.uid;
  item.name = like.name;
  item.status = like.status;
  item.round = like.round;
  item.width = like.width;
  item.height = like.height;
  item.bendRadius = like.bendRadius;
  item.bendRadiusEdge = like.bendRadiusEdge;
  for (const QJsonValue& value : def.value(QStringLiteral("nodes")).toArray()) {
    const QJsonObject json = value.toObject();
    const std::optional<Xyz> at = xyzOf(json.value(QStringLiteral("at")));
    if (!at) {
      continue;
    }
    FlexNode node;
    node.id = json.value(QStringLiteral("id")).toString();
    node.point = *at;
    node.twist = json.value(QStringLiteral("twist")).toDouble();
    if (json.contains(QStringLiteral("port"))) {
      node.support = QStringLiteral("port");
      node.ref = json.value(QStringLiteral("port"));
    } else if (json.contains(QStringLiteral("on")) && !json.value(QStringLiteral("on")).isNull()) {
      node.ref = json.value(QStringLiteral("on"));
      const QJsonObject on = node.ref.toObject();
      node.support = on.contains(QStringLiteral("face"))   ? QStringLiteral("face")
                     : on.contains(QStringLiteral("edge")) ? QStringLiteral("edge")
                                                           : QStringLiteral("point");
    } else {
      node.support = QStringLiteral("free");
    }
    item.nodes.push_back(node);
  }
  item.valid = !item.nodes.empty();
  return item;
}

QJsonObject withNodeCommand(QJsonObject def, const QJsonObject& command, QString* made) {
  const QString name = command.value(QStringLiteral("cmd")).toString();
  QJsonArray nodes = def.value(QStringLiteral("nodes")).toArray();
  const QString node = command.value(QStringLiteral("node")).toString();
  const int row = nodeRow(nodes, node);
  const auto put = [&def, &nodes] {
    def.insert(QStringLiteral("nodes"), nodes);
    return def;
  };
  if (name == QStringLiteral("flex.add_node")) {
    QJsonObject fields{{QStringLiteral("id"), nextNodeId(def)}};
    for (const char* key : {"at", "on", "port", "twist"}) {
      const QString field = QString::fromLatin1(key);
      if (command.contains(field) && !command.value(field).isNull()) {
        fields.insert(field, command.value(field));
      }
    }
    if (made != nullptr) {
      *made = fields.value(QStringLiteral("id")).toString();
    }
    const QJsonValue place = command.value(QStringLiteral("place"));
    if (place.isObject()) {
      const int after = nodeRow(nodes, place.toObject().value(QStringLiteral("after")).toString());
      if (after < 0) {
        return QJsonObject();
      }
      nodes.insert(after + 1, fields);
    } else if (place.toString() == QStringLiteral("start")) {
      nodes.prepend(fields);
    } else {
      nodes.append(fields);
    }
    return put();
  }
  if (row < 0) {
    return QJsonObject();
  }
  QJsonObject fields = nodes.at(row).toObject();
  if (name == QStringLiteral("flex.move_node")) {
    if (!command.value(QStringLiteral("at")).isArray()) {
      return QJsonObject();
    }
    fields.insert(QStringLiteral("at"), command.value(QStringLiteral("at")));
  } else if (name == QStringLiteral("flex.attach_node")) {
    fields.remove(QStringLiteral("on"));
    fields.remove(QStringLiteral("port"));
    if (command.contains(QStringLiteral("port"))) {
      fields.insert(QStringLiteral("port"), command.value(QStringLiteral("port")));
    } else if (command.contains(QStringLiteral("on"))) {
      fields.insert(QStringLiteral("on"), command.value(QStringLiteral("on")));
    } else if (!command.value(QStringLiteral("free")).toBool()) {
      return QJsonObject();
    }
    if (command.value(QStringLiteral("at")).isArray()) {
      fields.insert(QStringLiteral("at"), command.value(QStringLiteral("at")));
    }
  } else if (name == QStringLiteral("flex.set_twist")) {
    fields.insert(QStringLiteral("twist"), command.value(QStringLiteral("twist")).toDouble());
  } else if (name == QStringLiteral("flex.delete_node")) {
    if (nodes.size() <= 2) {
      return QJsonObject();
    }
    nodes.removeAt(row);
    return put();
  } else {
    return QJsonObject();
  }
  nodes.replace(row, fields);
  return put();
}

QJsonObject editFeatureCommand(const QString& uid, const QJsonObject& def) {
  return QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_feature")},
                     {QStringLiteral("uid"), uid},
                     {QStringLiteral("def"), def}};
}

QJsonObject routeOnly(QJsonObject def) {
  def.insert(QStringLiteral("route_only"), true);
  return def;
}

QString editFailure(const QJsonObject& answer, const QString& uid) {
  for (const QJsonValue& value : answer.value(QStringLiteral("failed_features")).toArray()) {
    const QJsonObject failed = value.toObject();
    if (failed.value(QStringLiteral("uid")).toString() != uid || !failed.value(QStringLiteral("new")).toBool()) {
      continue;
    }
    QString words = failed.value(QStringLiteral("message")).toString();
    if (words.isEmpty()) {
      words = failed.value(QStringLiteral("error")).toString();
    }
    // (Failed without words: still failed.)
    return words.isEmpty() ? QStringLiteral("the item could not be made") : words;
  }
  return QString();
}

double snappedTwist(double twist, double tolerance) {
  const double quarter = kPi / 2.0;
  const double nearest = std::round(twist / quarter) * quarter;
  return std::abs(twist - nearest) <= tolerance ? nearest : twist;
}

double angleAbout(const Xyz& from, const Xyz& to, const Xyz& axis) {
  const Xyz a = sub(from, scaled(axis, dot(from, axis)));
  const Xyz b = sub(to, scaled(axis, dot(to, axis)));
  if (length(a) < 1e-12 || length(b) < 1e-12) {
    return 0.0;
  }
  return std::atan2(dot(cross(a, b), axis), dot(a, b));
}

QString degreesText(double radians) {
  double degrees = std::round(radians * 180.0 / kPi * 10.0) / 10.0;
  if (std::abs(degrees) < 0.05) {
    degrees = 0.0; // never "-0"
  }
  const bool whole = std::abs(degrees - std::round(degrees)) < 0.05;
  return QLocale().toString(degrees, 'f', whole ? 0 : 1) + QChar(0x00B0);
}

QString xyzLog(const Xyz& point) {
  const auto number = [](double v) { return QString::number(std::abs(v) < 5e-4 ? 0.0 : v, 'f', 3); };
  return QStringLiteral("(%1, %2, %3)").arg(number(point[0]), number(point[1]), number(point[2]));
}

QString targetLog(const NodeTarget& target) {
  const QJsonObject reference = target.ref.toObject();
  const QString body = reference.value(QStringLiteral("body")).toString();
  const QString face = reference.value(QStringLiteral("face")).toString();
  switch (target.kind) {
  case NodeTarget::Kind::Face:
    return QStringLiteral("face %1 of %2").arg(face, body);
  case NodeTarget::Kind::Port:
    return QStringLiteral("port %1 of %2").arg(face, body);
  case NodeTarget::Kind::Link:
    return QStringLiteral("link %1")
        .arg(target.ref.isObject()
                 ? QString::fromUtf8(QJsonDocument(reference).toJson(QJsonDocument::Compact))
                 : target.ref.toString());
  case NodeTarget::Kind::Free:
    break;
  }
  return QStringLiteral("free");
}

} // namespace mitcad::flex
