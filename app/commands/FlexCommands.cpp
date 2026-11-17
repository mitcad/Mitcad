// SPDX-License-Identifier: MIT
// Flex (mitcad#1713; commands.md, "Flex"; app/COMMANDS.md, "Flex"): a wire,
// a cable, a hose, a flat cable or a strip laid through the model by
// clicks. The panel holds the item (a Flex type of the library, its main
// values editable) and the route: each click in the view is a node, on the
// face, edge or point under the pointer, at an end of a pipe or of another
// item (an end port), or free in space on the placement plane. The model
// finds the route between the nodes; the preview follows the pointer.
// Backspace takes the last node back; Enter, a double click (a click on the
// last node again) or a click on an end port ends the item.
#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>
#include <memory>
#include <optional>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QtLogging>

#include <gp_Ax3.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../browser/DocumentHost.hpp"
#include "../flex/FlexRope.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/CommandSession.hpp"
#include "../framework/RememberedValues.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"
#include "FlexRoute.hpp"
#include "SketchNormal.hpp"

namespace mitcad {
namespace cmd {
namespace {

const QString kCommandId = QStringLiteral("solid.flex");
constexpr double kPi = 3.14159265358979323846;

// The inputs.
const QString kType = QStringLiteral("type");
const QString kNodes = QStringLiteral("nodes");
const QString kTwist = QStringLiteral("twist");
const QString kPlaneChoice = QStringLiteral("free_plane");
const QString kSection = QStringLiteral("section");
const QString kDiameter = QStringLiteral("diameter");
const QString kWidth = QStringLiteral("width");
const QString kHeight = QStringLiteral("height");
const QString kCorner = QStringLiteral("corner_radius");
const QString kHollow = QStringLiteral("hollow");
const QString kWall = QStringLiteral("wall");
const QString kBend = QStringLiteral("bend_radius");
const QString kBendEdge = QStringLiteral("bend_radius_edge");
const QString kAppearance = QStringLiteral("appearance");
const QString kSaving = QStringLiteral("saving");
const QString kTypeName = QStringLiteral("type_name");
// What the state keeps beside them.
const QString kRoute = QStringLiteral("route");     // {"nodes": [...]}: the nodes placed (FlexRoute.hpp)
const QString kHover = QStringLiteral("hover");     // the node under the pointer; empty: none
// The pointer is on a place a click makes a node of (a face, an edge, a
// point, a port; in space with Alt held): only there the route to it is
// worked out (owner 14.11.2026).
const QString kHoverTarget = QStringLiteral("hover_target");
const QString kPlane = QStringLiteral("plane");     // {"origin", "normal"}: where a click off the model lands
const QString kOver = QStringLiteral("over");       // the pointer is on something a click picks
const QString kFinish = QStringLiteral("finish");   // the last click also ends the item
const QString kPortsOf = QStringLiteral("ports_of"); // {"body", "occurrence"}: the hovered body with end ports
const QString kCustom = QStringLiteral("custom");   // an item that is no type of the library (an edited one)
const QString kKept = QStringLiteral("kept");       // an edited item's definition

const QString kTwistNode = QStringLiteral("twist_node"); // {"index"}: the node the panel's Twist is of; empty: the last
const QString kSizeTyped = QStringLiteral("size_typed");     // the user changed the section or a size in this run
const QString kPortSection = QStringLiteral("port_section"); // the end port's `ref` the panel's section is from; empty: none

const QString kCustomType = QStringLiteral("__custom");
const QString kPlaneAuto = QStringLiteral("auto");
const QString kPlaneXy = QStringLiteral("xy");
const QString kPlaneYz = QStringLiteral("yz");
const QString kPlaneZx = QStringLiteral("zx");
const QString kRound = QStringLiteral("round");
const QString kRectangular = QStringLiteral("rectangular");
const QString kOutline = QStringLiteral("outline");

constexpr SelectFilter kNodeKinds =
    SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::SketchPoint | SelectKind::Point;

// What the command reads of the model once (when it starts and when the
// model changed beside it), so that a move of the pointer asks nothing.
struct Shared {
  QJsonArray types;                     // the `flex_types` query's rows
  int typeProblems = 0;                 // its `problems`: files of types that could not be read
  std::vector<FlexPort> ports;          // the `end_ports` query: each in its body's component
  // The ports of a body picked in an occurrence, in the item's component
  // (the query with `occurrence` and `component`), by "<body>|<occurrence>".
  QHash<QString, std::vector<FlexPort>> placed;
  // The `feature` query's answers for the features of the ports an item
  // started at (the shape of their section), by uid; asked once each.
  QHash<QString, QJsonObject> portFeatures;
  Choices appearances;                  // (id, name)
  QJsonArray occurrences;               // the `components` query's tree
  QString homeComponent = QStringLiteral("C0"); // where the item is made
  QString homeOccurrence;               // and seen: a path of uids, empty in the root
  QString lastType;                     // the type the last item was made of
  QString savedType;                    // a type just saved: chosen when the panel shows again
  QPointer<flex::FlexRopeOverlay> rope; // the route under the pointer, drawn over the view
  QString loggedRope;
  // An item's edit session (owner 14.11.2026): each change's definition and
  // the panel's state with it, oldest first, the state now last; Undo takes
  // the last one back.
  std::vector<std::pair<QJsonObject, CommandState>> history;
};

// Whether the panel edits an item (its edit session, owner 14.11.2026):
// clicks in the view then place no node and nothing follows the pointer;
// the item's handles in the view (flex/FlexEditController.hpp) change its
// nodes, "+" there makes it longer.
bool editing(const CommandState& state) { return !state.object(kKept).isEmpty(); }

// The item used while the library gives no type (an older model).
QJsonObject fallbackItem() {
  return {{QStringLiteral("name"), QStringLiteral("Hose 10")},
          {QStringLiteral("section"), QJsonObject{{QStringLiteral("type"), kRound}, {QStringLiteral("diameter"), 10.0}}},
          {QStringLiteral("wall"), 1.5},
          {QStringLiteral("bend_radius"), 30.0}};
}

QString typeValue(const QJsonObject& type) {
  return type.value(QStringLiteral("library")).toString(QStringLiteral("built_in")) + QLatin1Char('/') +
         type.value(QStringLiteral("id")).toString();
}

std::optional<QJsonObject> typeOf(const Shared& shared, const QString& value) {
  for (const QJsonValue& row : shared.types) {
    if (typeValue(row.toObject()) == value) {
      return row.toObject();
    }
  }
  return std::nullopt;
}

// The item the chosen type gives, before the panel's own values.
QJsonObject baseItem(const CommandState& state, const Shared& shared) {
  if (const auto type = typeOf(shared, state.choice(kType))) {
    return flexItemOfType(*type);
  }
  const QJsonObject custom = state.object(kCustom);
  return custom.isEmpty() ? fallbackItem() : custom;
}

// The library's types (the `flex_types` query); `told`: its problems and
// failure logged.
void readTypes(Shared& shared, const CommandContext& context, bool told) {
  shared.types = QJsonArray();
  shared.typeProblems = 0;
  try {
    const QJsonObject library = context.queryObject({{QStringLiteral("query"), QStringLiteral("flex_types")}});
    shared.types = library.value(QStringLiteral("types")).toArray();
    shared.typeProblems = static_cast<int>(library.value(QStringLiteral("problems")).toArray().size());
    if (told && shared.typeProblems > 0) {
      qWarning().noquote() << QStringLiteral("Flex: %1 type file(s) of the library could not be read")
                                  .arg(shared.typeProblems);
    }
  } catch (const std::exception& error) {
    if (told) {
      qWarning().noquote() << QStringLiteral("Flex: no types from the library: %1").arg(modelText(error));
    }
  }
}

void readModel(Shared& shared, const CommandContext& context) {
  readTypes(shared, context, true);
  shared.ports.clear();
  shared.placed.clear();
  shared.portFeatures.clear();
  try {
    shared.ports = flexPorts(context.queryArray(QStringLiteral("end_ports")));
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Flex: no end ports: %1").arg(modelText(error));
  }
  shared.appearances.clear();
  try {
    for (const QJsonValue& value : context.queryArray(QStringLiteral("appearances"))) {
      const QJsonObject appearance = value.toObject();
      shared.appearances.append(qMakePair(str(appearance, "id"), str(appearance, "name")));
    }
  } catch (const std::exception&) {
    // The component's default only.
  }
  shared.occurrences = QJsonArray();
  shared.homeComponent = QStringLiteral("C0");
  shared.homeOccurrence.clear();
  try {
    const QJsonObject components = componentsOf(context);
    shared.occurrences = components.value(QStringLiteral("occurrences")).toArray();
    shared.homeComponent = components.value(QStringLiteral("active")).toString(QStringLiteral("C0"));
  } catch (const std::exception&) {
    // The root then.
  }
  if (shared.homeComponent != QStringLiteral("C0")) {
    // Where the view shows the finished item: the component's first
    // occurrence, as a construction feature's home (mitcad#1029).
    bool found = false;
    eachOccurrence(context, [&shared, &found](const QJsonObject& node, const QString& at) {
      if (!found && str(node, "component") == shared.homeComponent) {
        shared.homeOccurrence = at;
        found = true;
      }
    });
  }
  qInfo().noquote() << QStringLiteral("Flex: %1 type(s), %2 end port(s)").arg(shared.types.size()).arg(shared.ports.size());
}

// --- Coordinates: a node's `at` is in the item's component, a pick's in
// its own component, the view's in the design.

gp_Trsf worldOf(const Shared& shared, const QString& occurrence) {
  if (occurrence.isEmpty()) {
    return gp_Trsf();
  }
  return occurrenceWorldIn(shared.occurrences, occurrence).value_or(gp_Trsf());
}

gp_Pnt toDesign(const Shared& shared, const gp_Pnt& point, const QString& occurrence) {
  return point.Transformed(worldOf(shared, occurrence));
}

gp_Pnt designToHome(const Shared& shared, const gp_Pnt& point) {
  return point.Transformed(worldOf(shared, shared.homeOccurrence).Inverted());
}

gp_Pnt homeToDesign(const Shared& shared, const gp_Pnt& point) { return toDesign(shared, point, shared.homeOccurrence); }

// --- The nodes in the state

QJsonArray nodesOf(const CommandState& state) { return state.object(kRoute).value(QStringLiteral("nodes")).toArray(); }

void setNodes(CommandState& state, const QJsonArray& nodes) {
  state.setObject(kRoute, QJsonObject{{QStringLiteral("nodes"), nodes}});
}

QJsonObject pickOf(const SelectionItem& item) {
  return {{QStringLiteral("kind"), static_cast<int>(item.kind)},
          {QStringLiteral("owner"), item.owner},
          {QStringLiteral("name"), item.name},
          {QStringLiteral("geometry"), item.geometry},
          {QStringLiteral("occurrence"), item.occurrence}};
}

SelectionItem itemOfPick(const QJsonObject& pick) {
  SelectionItem item;
  item.kind = static_cast<SelectKind>(pick.value(QStringLiteral("kind")).toInt());
  item.owner = str(pick, "owner");
  item.name = str(pick, "name");
  item.geometry = str(pick, "geometry");
  item.occurrence = str(pick, "occurrence");
  return item;
}

// The picks of the nodes, each once: what the Route input lists and the
// view highlights.
Selection picksOf(const QJsonArray& nodes) {
  Selection items;
  for (const QJsonValue& value : nodes) {
    const QJsonObject pick = value.toObject().value(QStringLiteral("pick")).toObject();
    if (pick.isEmpty()) {
      continue;
    }
    const SelectionItem item = itemOfPick(pick);
    if (item.isValid() && !items.contains(item)) {
      items.append(item);
    }
  }
  return items;
}

bool onBody(const SelectionItem& item) {
  return item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
}

// Whether the route has ended: it has a start, and its last node is at an
// end port.
bool ended(const QJsonArray& nodes) { return nodes.size() >= 2 && flexIsPort(nodes.last().toObject()); }

// The ports of the body of `item` where it was picked, in the item's
// component (commands.md, "End ports": the query with `occurrence` and
// `component`), asked once for each body and occurrence while the pointer
// is over it. A body of the item's own component needs none: the ports
// are in its coordinates already.
void placedPorts(Shared& shared, const SelectionItem& item, const CommandContext& context) {
  if (item.occurrence.isEmpty() || item.occurrence == shared.homeOccurrence) {
    return;
  }
  const QString key = item.owner + QLatin1Char('|') + item.occurrence;
  if (shared.placed.contains(key)) {
    return;
  }
  try {
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("end_ports")},
                      {QStringLiteral("body"), item.owner},
                      {QStringLiteral("occurrence"), item.occurrence},
                      {QStringLiteral("component"), shared.homeComponent}};
    if (!shared.homeOccurrence.isEmpty()) {
      query.insert(QStringLiteral("context"), shared.homeOccurrence);
    }
    shared.placed.insert(key, flexPorts(context.query(query).toArray()));
  } catch (const std::exception& error) {
    // The ports in the body's own coordinates then, moved by its occurrence.
    qWarning().noquote() << QStringLiteral("Flex: no placed end ports of %1: %2").arg(item.owner, modelText(error));
  }
}

// The node a click on `item` at its `at` makes: at the end port there when
// the item can begin or end at one, else on the item.
QJsonObject nodeOn(const SelectionItem& item, const QJsonArray& nodes, const Shared& shared) {
  const gp_Pnt at(item.at->at(0), item.at->at(1), item.at->at(2));
  QJsonObject node{{QStringLiteral("pick"), pickOf(item)}};
  if (onBody(item)) {
    const QString face = item.kind == SelectKind::Face ? item.name : QString();
    const auto placed = shared.placed.constFind(item.owner + QLatin1Char('|') + item.occurrence);
    if (placed != shared.placed.constEnd()) {
      // In the item's component already.
      const gp_Pnt home = designToHome(shared, toDesign(shared, at, item.occurrence));
      const std::optional<FlexPort> port = flexPortAt(*placed, item.owner, face, home);
      const bool begunThere = port && !nodes.isEmpty() &&
                              nodes.first().toObject().value(QStringLiteral("port")).toObject() == port->ref;
      if (port && !begunThere) {
        node.insert(QStringLiteral("at"), flexXyz(port->position));
        node.insert(QStringLiteral("port"), port->ref);
        return node;
      }
    }
    const std::optional<FlexPort> port = flexPortAt(shared.ports, item.owner, face, at);
    // Not the port the item began at: a click there is a node on the face.
    const bool begunThere = port && !nodes.isEmpty() &&
                            nodes.first().toObject().value(QStringLiteral("port")).toObject() == port->ref;
    if (port && !begunThere) {
      node.insert(QStringLiteral("at"), flexXyz(designToHome(shared, toDesign(shared, port->position, item.occurrence))));
      node.insert(QStringLiteral("port"), port->ref);
      return node;
    }
  }
  node.insert(QStringLiteral("at"), flexXyz(designToHome(shared, toDesign(shared, at, item.occurrence))));
  node.insert(QStringLiteral("on"), item.reference());
  return node;
}

// A few pixels of the view at a point of the design, in millimetres.
double pixels(const CommandContext& context, const gp_Pnt& design, double count) {
  const double size = context.pixelSize(design);
  return std::isfinite(size) && size > 0.0 ? size * count : 0.25 * count;
}

// Whether two points of the design lie within `count` pixels of each other
// as the view shows them: across the way to the viewer, whatever their
// depths (a click on a node's mark lands on the face or the plane behind
// it).
bool nearOnScreen(const CommandContext& context, const gp_Pnt& a, const gp_Pnt& b, double count) {
  gp_Vec between(a, b);
  if (const std::optional<gp_Dir> toward = context.towardViewer(a)) {
    const gp_Vec way(*toward);
    between -= way * between.Dot(way);
  }
  return between.Magnitude() <= pixels(context, a, count);
}

// --- The placement plane of clicks off the model: through the last node
// (the origin before any). Automatic: across the main direction nearest to
// the view's, so that a click lands where it looks to be; or the plane
// chosen in the panel (Tab steps it), as a 3D sketch's drawing tools have
// XY, YZ and ZX.

void updatePlane(CommandState& state, const CommandContext& context, const Shared& shared) {
  const QJsonArray nodes = nodesOf(state);
  const gp_Pnt origin = homeToDesign(
      shared, nodes.isEmpty() ? gp_Pnt(0.0, 0.0, 0.0) : flexPoint(nodes.last().toObject().value(QStringLiteral("at"))));
  gp_Dir normal(0.0, 0.0, 1.0);
  const QString chosen = state.choice(kPlaneChoice);
  if (chosen == kPlaneYz) {
    normal = gp_Dir(1.0, 0.0, 0.0);
  } else if (chosen == kPlaneZx) {
    normal = gp_Dir(0.0, 1.0, 0.0);
  } else if (chosen != kPlaneXy) {
    if (const std::optional<gp_Dir> toward = context.towardViewer(origin)) {
      // The horizontal plane unless the view looks along it.
      if (std::abs(toward->Z()) < 0.35) {
        normal = std::abs(toward->X()) >= std::abs(toward->Y()) ? gp_Dir(1.0, 0.0, 0.0) : gp_Dir(0.0, 1.0, 0.0);
      }
    }
  }
  // The planes of the item's component, as the view shows them.
  normal = normal.Transformed(worldOf(shared, shared.homeOccurrence));
  state.setObject(kPlane, QJsonObject{{QStringLiteral("origin"), flexXyz(origin)},
                                      {QStringLiteral("normal"), flexXyz(gp_Pnt(normal.X(), normal.Y(), normal.Z()))}});
}

// --- The twist of a node (a section that is not round): the panel's Twist
// is the twist of the node placed last, or of the node clicked since.

int twistNode(const CommandState& state, const QJsonArray& nodes) {
  const QJsonObject chosen = state.object(kTwistNode);
  const int index = chosen.value(QStringLiteral("index")).toInt(-1);
  return chosen.isEmpty() || index < 0 || index >= nodes.size() ? static_cast<int>(nodes.size()) - 1 : index;
}

double twistOf(const CommandState& state) {
  const double twist = state.value(kTwist);
  return std::isfinite(twist) ? twist : 0.0;
}

void showTwist(CommandState& state, double radians) {
  const QString text = QStringLiteral("%1 deg").arg(number(radians * 180.0 / kPi));
  state.setText(kTwist, text);
  state.setExpression(kTwist, text);
  state.setValue(kTwist, radians);
}

// The nodes with the panel's twist in the node it is of.
QJsonArray withPanelTwist(const CommandState& state, QJsonArray nodes) {
  const int index = twistNode(state, nodes);
  if (index >= 0 && state.choice(kSection) != kRound) {
    QJsonObject node = nodes.at(index).toObject();
    node.insert(QStringLiteral("twist"), twistOf(state));
    nodes.replace(index, node);
  }
  return nodes;
}

// --- The panel's values of the item

void showLength(CommandState& state, const QString& key, const QJsonValue& millimetres, const CommandContext& context) {
  if (!millimetres.isDouble()) {
    state.setText(key, QString());
    state.setExpression(key, QString());
    state.setValue(key, std::numeric_limits<double>::quiet_NaN());
    return;
  }
  const QString text = lengthText(millimetres.toDouble(), context);
  state.setText(key, text);
  state.setExpression(key, text);
  state.setValue(key, millimetres.toDouble());
}

// The panel shows a section: its kind, sizes and wall.
void showSection(CommandState& state, const FlexSection& section, const CommandContext& context) {
  state.setChoice(kSection, section.type);
  if (section.type == kRound) {
    showLength(state, kDiameter, section.diameter, context);
  } else if (section.type == kRectangular) {
    showLength(state, kWidth, section.width, context);
    showLength(state, kHeight, section.height, context);
    showLength(state, kCorner, section.cornerRadius, context);
  }
  state.setChecked(kHollow, section.hollow);
  if (section.hollow) {
    showLength(state, kWall, section.wall, context);
  }
}

// The panel shows an item: its section, wall, bend radii, appearance and
// name.
void showItem(CommandState& state, const QJsonObject& item, const CommandContext& context) {
  const FlexSection section = flexSectionOf(item);
  showSection(state, section, context);
  showLength(state, kBend, item.value(kBend), context);
  showLength(state, kBendEdge, item.value(kBendEdge), context);
  state.setChoice(kAppearance, section.appearance);
  state.setText(kTypeName, str(item, "name"));
}

FlexSection sectionIn(const CommandState& state) {
  FlexSection section;
  section.type = state.choice(kSection);
  section.diameter = state.value(kDiameter);
  section.width = state.value(kWidth);
  section.height = state.value(kHeight);
  const double corner = state.value(kCorner);
  section.cornerRadius = std::isfinite(corner) ? corner : 0.0;
  section.hollow = state.checked(kHollow);
  section.wall = state.value(kWall);
  section.appearance = state.choice(kAppearance);
  return section;
}

bool roundIn(const CommandState& state) { return state.choice(kSection) == kRound; }

// --- The section of an item started at an end port (owner 13.11.2026):
// the size of the pipe or Flex item there (flexSectionAtPort), shown in the
// panel, which can still change it; started anywhere else, the type's, as
// before. A section or size the user changed in this run stays (no later
// start at a port changes it); an edited item keeps its own.

// The end port a node's `port` is, with its sizes.
std::optional<FlexPort> portOf(const Shared& shared, const QJsonObject& ref) {
  for (const FlexPort& port : shared.ports) {
    if (port.ref == ref) {
      return port;
    }
  }
  for (auto placed = shared.placed.constBegin(); placed != shared.placed.constEnd(); ++placed) {
    for (const FlexPort& port : placed.value()) {
      if (port.ref == ref) {
        return port;
      }
    }
  }
  return std::nullopt;
}

// The `feature` query's answer for a port's feature: an empty object when it
// failed (the section's shape is then guessed from the cap's sizes).
QJsonObject portFeature(Shared& shared, const QString& uid, const CommandContext& model) {
  if (uid.isEmpty()) {
    return {};
  }
  const auto known = shared.portFeatures.constFind(uid);
  if (known != shared.portFeatures.constEnd()) {
    return known.value();
  }
  QJsonObject answer;
  try {
    answer = model.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}});
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Flex: no definition of %1: %2").arg(uid, modelText(error));
  }
  shared.portFeatures.insert(uid, answer);
  return answer;
}

QString sectionText(const FlexSection& section) {
  if (section.type == kRound) {
    return QStringLiteral("round %1 mm").arg(number(section.diameter));
  }
  return QStringLiteral("rectangular %1 x %2 mm").arg(number(section.width), number(section.height));
}

// The first node placed: the section it gives the item.
void takeStartSection(CommandState& state, Shared& shared, const QJsonObject& node, const CommandContext& model) {
  if (state.checked(kSizeTyped) || !state.object(kKept).isEmpty()) {
    return;
  }
  if (flexIsPort(node)) {
    const QJsonObject ref = node.value(QStringLiteral("port")).toObject();
    if (const std::optional<FlexPort> port = portOf(shared, ref)) {
      const QJsonObject feature = portFeature(shared, port->feature, model);
      if (const std::optional<FlexSection> section = flexSectionAtPort(*port, feature, sectionIn(state))) {
        const bool wasRound = roundIn(state);
        showSection(state, *section, model);
        if (wasRound && section->type == kRectangular && state.text(kBendEdge).trimmed().isEmpty()) {
          // A round item had no limit over the edge: a Flex item's end gives
          // its own; a square pipe's bends over either side alike.
          if (feature.value(QStringLiteral("type")).toString(port->kind) == QStringLiteral("flex")) {
            const QJsonObject def = feature.value(QStringLiteral("def")).toObject();
            showLength(state, kBendEdge, def.value(QStringLiteral("item")).toObject().value(kBendEdge), model);
          } else if (std::isfinite(state.value(kBend)) && state.value(kBend) > 0.0) {
            showLength(state, kBendEdge, state.value(kBend), model);
          }
        }
        state.setObject(kPortSection, ref);
        qInfo().noquote() << QStringLiteral("Flex: section of the end port of %1: %2%3")
                                 .arg(port->feature, sectionText(*section),
                                      section->hollow ? QStringLiteral(", hollow") : QString());
        return;
      }
    }
  }
  if (!state.object(kPortSection).isEmpty()) {
    // A start at a port was taken back: the type's section again.
    showSection(state, flexSectionOf(baseItem(state, shared)), model);
    state.setObject(kPortSection, {});
    qInfo().noquote() << QStringLiteral("Flex: section of the type again");
  }
}

// The item of the panel and its own bend radii: the type's item while the
// panel's values are its own, a bend radius that differs as the item's own
// (`radii`: the definition's `bend_radius` and `bend_radius_edge`); with
// `folded` every radius in the item itself (a type to save). An error says
// what is missing.
QJsonObject itemIn(const CommandState& state, const Shared& shared, QJsonObject* radii, QString* error, QString* input,
                   bool folded = false) {
  const FlexSection section = sectionIn(state);
  const auto missing = [&](const QString& key, const QString& message) {
    if (error != nullptr && error->isEmpty()) {
      *error = message;
      if (input != nullptr) {
        *input = key;
      }
    }
  };
  const auto positive = [&state](const QString& key) {
    const double value = state.value(key);
    return std::isfinite(value) && value > 0.0;
  };
  if (section.type == kRound && !positive(kDiameter)) {
    missing(kDiameter, QObject::tr("Give the item's diameter."));
  } else if (section.type == kRectangular && (!positive(kWidth) || !positive(kHeight))) {
    missing(positive(kWidth) ? kHeight : kWidth, QObject::tr("Give the section's width and height."));
  }
  if (section.type != kOutline && section.hollow && !positive(kWall)) {
    missing(kWall, QObject::tr("Give the wall's thickness."));
  }
  QJsonObject item = flexItemWith(baseItem(state, shared), section, QString());
  bool changed = false;
  for (const QString& key : {kBend, kBendEdge}) {
    if (key == kBendEdge && section.type == kRound) {
      continue;
    }
    const bool given = !state.text(key).trimmed().isEmpty();
    const bool has = item.value(key).isDouble();
    if (!given) {
      if (key == kBend && section.type == kRound) {
        missing(kBend, QObject::tr("Give the bend radius: the smallest radius the item bends to."));
      } else if (has) {
        item.remove(key); // it does not bend this way
        changed = true;
      }
      continue;
    }
    if (!positive(key)) {
      missing(key, QObject::tr("The bend radius must be greater than zero."));
      continue;
    }
    const double value = state.value(key);
    if (!has || folded) {
      changed = changed || !has || std::abs(item.value(key).toDouble() - value) > 1e-9;
      item.insert(key, value);
    } else if (std::abs(item.value(key).toDouble() - value) > 1e-9 && radii != nullptr) {
      radii->insert(key, state.expression(key));
    }
  }
  if (changed) {
    item.remove(QStringLiteral("origin"));
  }
  return item;
}

// --- Links of nodes on other components' geometry (as a construction
// feature's, mitcad#1029, #1035, #1039): the body, or the sketch or
// construction feature, linked from the occurrence it was picked in.
bool addLinks(const SelectionItem& item, const Shared& shared, const CommandContext& context, QJsonObject& links,
              QJsonObject& datumLinks) {
  if (item.occurrence.isEmpty() && shared.homeOccurrence.isEmpty()) {
    return true;
  }
  const QString component = occurrenceComponent(item.occurrence, context);
  if (component.isEmpty() || component == shared.homeComponent) {
    return true;
  }
  QJsonObject link;
  if (!item.occurrence.isEmpty()) {
    link.insert(QStringLiteral("source"), item.occurrence);
  }
  if (!shared.homeOccurrence.isEmpty()) {
    link.insert(QStringLiteral("target"), shared.homeOccurrence);
  }
  if (onBody(item)) {
    if (links.contains(item.owner) && links.value(item.owner).toObject() != link) {
      return false; // one link per body
    }
    links.insert(item.owner, link);
  } else if (item.owner != QStringLiteral("origin")) {
    datumLinks.insert(item.owner, link);
  }
  return true;
}

// The `flex` definition of the panel and the route: the nodes placed and,
// with `following`, the node under the pointer; `routeOnly` for a preview
// of the route without its solid.
Built definitionOf(const CommandState& state, const Shared& shared, const CommandContext& model, bool routeOnly) {
  QString error;
  QString input;
  QJsonObject radii;
  const QJsonObject item = itemIn(state, shared, &radii, &error, &input);
  if (!error.isEmpty()) {
    return Built::failure(error, input);
  }
  QJsonArray nodes = withPanelTwist(state, nodesOf(state));
  // The route alone runs on to the pointer; the item's own preview, the
  // solid, is of the nodes placed (owner 14.11.2026).
  const QJsonObject hover = state.object(kHover);
  if (routeOnly && !nodes.isEmpty() && !hover.isEmpty() && !ended(nodes) &&
      !flexSamePlace(hover, nodes.last().toObject(), 1e-6)) {
    nodes.append(hover);
  }
  if (nodes.isEmpty()) {
    return Built::asking(QObject::tr("Click where the item starts: an end of a pipe, a face or a point."), kNodes);
  }
  if (nodes.size() < 2) {
    return Built::asking(QObject::tr("Click where the item goes next: a face, an edge or a point. Alt places a node "
                                     "in space; Shift shows the route as the pointer moves."),
                         kNodes);
  }
  if (flexPortInside(nodes)) {
    return Built::failure(QObject::tr("An item meets an end of a pipe or of another item only with its own ends."),
                          kNodes);
  }
  QJsonObject links;
  QJsonObject datumLinks;
  for (const QJsonValue& value : nodes) {
    const QJsonObject pick = value.toObject().value(QStringLiteral("pick")).toObject();
    if (!pick.isEmpty() && !addLinks(itemOfPick(pick), shared, model, links, datumLinks)) {
      return Built::failure(QObject::tr("Click the faces of a component in one occurrence of it."), kNodes);
    }
  }
  // What an edited item has beyond the panel (a sketch profile, a fixed
  // length, its length parameter) stays.
  const QJsonObject kept = state.object(kKept);
  QJsonObject definition;
  for (const char* own : {"profile", "fixed_length", "length"}) {
    const QString key = QString::fromLatin1(own);
    if (kept.contains(key) && !kept.value(key).isNull()) {
      definition.insert(key, kept.value(key));
    }
  }
  definition.insert(QStringLiteral("type"), QStringLiteral("flex"));
  definition.insert(QStringLiteral("item"), item);
  definition.insert(QStringLiteral("nodes"), flexDefinitionNodes(nodes));
  for (const QString& key : {kBend, kBendEdge}) {
    if (radii.contains(key)) {
      definition.insert(key, radii.value(key));
    }
  }
  if (!links.isEmpty()) {
    definition.insert(QStringLiteral("links"), links);
  }
  if (!datumLinks.isEmpty()) {
    definition.insert(QStringLiteral("datum_links"), datumLinks);
  }
  if (routeOnly) {
    definition.insert(QStringLiteral("route_only"), true);
  }
  return Built::of(definition);
}

// --- The marks drawn over the view: a small cross at each node, and the
// end ports of the body under the pointer, each a ring round its cap with
// an arrow the way an item leaves it.

void addCross(std::vector<std::vector<gp_Pnt>>& lines, const gp_Pnt& at, double arm) {
  for (const gp_Vec& way : {gp_Vec(arm, 0.0, 0.0), gp_Vec(0.0, arm, 0.0), gp_Vec(0.0, 0.0, arm)}) {
    lines.push_back({at.Translated(-way), at.Translated(way)});
  }
}

void addPortMark(std::vector<std::vector<gp_Pnt>>& lines, const FlexPort& port, const gp_Trsf& world, double arm) {
  const gp_Ax3 frame(port.position, port.direction);
  const gp_Vec x(frame.XDirection());
  const gp_Vec y(frame.YDirection());
  const gp_Vec along(port.direction);
  const double radius = std::max(port.reach * 0.6, arm);
  std::vector<gp_Pnt> ring;
  constexpr int kSides = 32;
  for (int i = 0; i <= kSides; ++i) {
    const double angle = 2.0 * kPi * i / kSides;
    ring.push_back(port.position.Translated(x * (radius * std::cos(angle)) + y * (radius * std::sin(angle)))
                       .Transformed(world));
  }
  lines.push_back(ring);
  const double length = std::max(2.0 * radius, 4.0 * arm);
  const gp_Pnt tip = port.position.Translated(along * length);
  const gp_Pnt neck = port.position.Translated(along * (length - 1.5 * arm));
  lines.push_back({port.position.Transformed(world), tip.Transformed(world)});
  lines.push_back({neck.Translated(x * (0.6 * arm)).Transformed(world), tip.Transformed(world),
                   neck.Translated(x * (-0.6 * arm)).Transformed(world)});
}

// --- The Type choice

QString sizeTag(const QJsonObject& item) {
  const FlexSection section = flexSectionOf(item);
  if (section.type == kRound) {
    return QStringLiteral("Ø%1 mm").arg(number(section.diameter));
  }
  if (section.type == kRectangular) {
    return QStringLiteral("%1 × %2 mm").arg(number(section.width), number(section.height));
  }
  return QString();
}

QString libraryHeading(const QString& library) {
  if (library == QStringLiteral("user")) {
    return QObject::tr("My library");
  }
  if (library == QStringLiteral("project")) {
    return QObject::tr("This project");
  }
  return QObject::tr("Built-in");
}

// A new item's type: the one the last item was made of (kept between
// starts, as the panels' remembered values are), else the library's first;
// without a library the built-in stand-in.
void chooseStartType(CommandState& state, const Shared& shared, const CommandContext& context) {
  QString value = panelValuesRemembered() ? rememberedValue(kCommandId, kType) : QString();
  if (!typeOf(shared, value)) {
    value = typeOf(shared, shared.lastType) ? shared.lastType : QString();
  }
  if (value.isEmpty() && !shared.types.isEmpty()) {
    value = typeValue(shared.types.first().toObject());
  }
  if (value.isEmpty()) {
    state.setObject(kCustom, fallbackItem());
    value = kCustomType;
  }
  state.setChoice(kType, value);
  showItem(state, baseItem(state, shared), context);
}

void hideRope(const Shared& shared) {
  if (shared.rope) {
    shared.rope->showLine(std::nullopt);
    shared.rope->show(std::nullopt, gp_Trsf(), QString());
  }
}

// The keys held with the pointer (owner 14.11.2026): Shift, the route that
// follows the pointer live; Alt, a node free in space where a click hits
// nothing.
bool shiftHeld(const CommandContext& model) { return model.pointerModifiers().testFlag(Qt::ShiftModifier); }
bool altHeld(const CommandContext& model) { return model.pointerModifiers().testFlag(Qt::AltModifier); }

// The panel's state of an item to edit (the `feature` query's def with its
// uid), its type looked up in `library`'s types: the panel's own for its
// edit session, a state of its own for the item's hover text (mitcad#1971).
void loadItem(const QJsonObject& feature, CommandState& state, const Shared& library, const CommandContext& model) {
  state.setObject(kKept, feature);
  const QJsonObject item = feature.value(QStringLiteral("item")).toObject();
  // Its type while the library still has it as it was copied; else the
  // item itself.
  const QJsonObject origin = item.value(QStringLiteral("origin")).toObject();
  const QString value = str(origin, "library") + QLatin1Char('/') + str(origin, "id");
  const std::optional<QJsonObject> type = typeOf(library, value);
  if (type && flexItemOfType(*type) == item) {
    state.setChoice(kType, value);
  } else {
    state.setObject(kCustom, item);
    state.setChoice(kType, kCustomType);
  }
  showItem(state, item, model);
  for (const QString& key : {kBend, kBendEdge}) {
    if (feature.contains(key) && !feature.value(key).isNull()) {
      loadValue(state, key, feature.value(key), feature, model);
    }
  }
  // The nodes where their links saw them.
  const QJsonObject links = feature.value(QStringLiteral("links")).toObject();
  const QJsonObject datumLinks = feature.value(QStringLiteral("datum_links")).toObject();
  QJsonArray nodes;
  for (const QJsonValue& entry : feature.value(QStringLiteral("nodes")).toArray()) {
    QJsonObject node = entry.toObject();
    const QJsonValue ref = node.contains(QStringLiteral("port")) ? node.value(QStringLiteral("port"))
                                                                  : node.value(QStringLiteral("on"));
    if (!ref.isUndefined() && !ref.isNull()) {
      SelectionItem picked = itemOf(ref, model);
      if (picked.isValid()) {
        const QJsonObject link = onBody(picked) ? links.value(picked.owner).toObject()
                                                : datumLinks.value(picked.owner).toObject();
        picked.occurrence = str(link, "source");
        node.insert(QStringLiteral("pick"), pickOf(picked));
      }
    }
    nodes.append(node);
  }
  setNodes(state, nodes);
  state.setItems(kNodes, picksOf(nodes));
  showTwist(state, nodes.isEmpty() ? 0.0 : nodes.last().toObject().value(QStringLiteral("twist")).toDouble());
}

} // namespace

CommandDef flexCommand(const CommandContext& /*context*/, DocumentHost& host) {
  const auto shared = std::make_shared<Shared>();
  CommandDef def;
  def.id = kCommandId;
  def.name = QObject::tr("Flex");
  def.icon = QStringLiteral("flex");
  def.tooltip = QObject::tr("Lays a wire, cable, hose or strip through the model by clicks");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("flex");
  def.keywords = {QStringLiteral("wire"),  QStringLiteral("cable"),   QStringLiteral("hose"),
                  QStringLiteral("tube"),  QStringLiteral("cord"),    QStringLiteral("harness"),
                  QStringLiteral("strip"), QStringLiteral("ribbon"),  QStringLiteral("belt")};
  def.featureType = QStringLiteral("flex");

  // The panel's Twist becomes the twist of node `index`: the node it was of
  // keeps what was shown.
  const auto chooseTwistNode = [](CommandState& state, QJsonArray& nodes, int index) {
    nodes = withPanelTwist(state, nodes);
    if (index < 0 || index >= nodes.size() - 1) {
      state.setObject(kTwistNode, {}); // the last node, as each new one
    } else {
      state.setObject(kTwistNode, QJsonObject{{QStringLiteral("index"), index}});
    }
    const int now = index < 0 ? static_cast<int>(nodes.size()) - 1 : index;
    showTwist(state, now >= 0 && now < nodes.size() ? nodes.at(now).toObject().value(QStringLiteral("twist")).toDouble() : 0.0);
  };

  const auto place = [shared, chooseTwistNode, &host](const QJsonObject& node, bool free, CommandState& state,
                                                       const CommandContext& model) {
    if (editing(state)) {
      // An edit session: a click in the view places nothing (owner
      // 14.11.2026); the item's handles move, add and delete nodes.
      qInfo().noquote() << QStringLiteral("Flex: click not taken: the item's handles edit its nodes");
      return;
    }
    QJsonArray nodes = nodesOf(state);
    const gp_Pnt design = homeToDesign(*shared, flexPoint(node.value(QStringLiteral("at"))));
    // (Each click comes here once, by the view's pick or on the plane:
    // CommandSession::clickedAt, mitcad#1713.)
    hideRope(*shared);
    const auto designOf = [&shared, &nodes](int index) {
      return homeToDesign(*shared, flexPoint(nodes.at(index).toObject().value(QStringLiteral("at"))));
    };
    // A click on the node placed last, as a double click's second one: the
    // end. Not by the clock: under load the second click may come late.
    if (!nodes.isEmpty() && (flexSamePlace(node, nodes.last().toObject(), pixels(model, design, 5.0)) ||
                             nearOnScreen(model, designOf(static_cast<int>(nodes.size()) - 1), design, 5.0))) {
      if (nodes.size() >= 2) {
        state.setChecked(kFinish, true);
        qInfo().noquote() << QStringLiteral("Flex: a click on the last node ends the item at node %1").arg(nodes.size());
      }
      setNodes(state, nodes);
      return;
    }
    // A click on an earlier node of a section that is not round: the
    // panel's Twist is that node's.
    if (!roundIn(state)) {
      for (int i = 0; i + 1 < nodes.size(); ++i) {
        if (nearOnScreen(model, designOf(i), design, 6.0)) {
          chooseTwistNode(state, nodes, i);
          setNodes(state, nodes);
          qInfo().noquote() << QStringLiteral("Flex: node %1 chosen for its twist").arg(i + 1);
          return;
        }
      }
    }
    if (ended(nodes)) {
      // OK was refused after the click on the end port: say what is open.
      qInfo().noquote() << QStringLiteral("Flex: click not taken: the item ends at an end port");
      host.showStatus(QObject::tr("The item ends where it was attached. Press Enter, or Backspace to take that end back."),
                      NoticeLevel::Hint);
      return;
    }
    // The node the panel's Twist was of keeps it; the new one starts
    // without one and is the panel's.
    nodes = withPanelTwist(state, nodes);
    nodes.append(node);
    state.setObject(kTwistNode, {});
    showTwist(state, 0.0);
    setNodes(state, nodes);
    state.setObject(kHover, {});
    if (nodes.size() == 1) {
      takeStartSection(state, *shared, node, model); // the size of a pipe's end the item starts at
    }
    if (ended(nodes)) {
      state.setChecked(kFinish, true); // an end port ends the item: the click is OK too
    }
    updatePlane(state, model, *shared);
    QString where = QStringLiteral("free");
    if (flexIsPort(node)) {
      where = QStringLiteral("at an end port");
    } else if (!free) {
      where = QStringLiteral("on ");
      where += itemOfPick(node.value(QStringLiteral("pick")).toObject()).logDescription();
    }
    qInfo().noquote() << QStringLiteral("Flex: node %1 %2 at %3, %4, %5")
                             .arg(nodes.size())
                             .arg(where)
                             .arg(number(design.X()), number(design.Y()), number(design.Z()));
  };

  // The node under the pointer, which the preview runs to: true when the
  // preview changes with it.
  const auto follow = [shared](const QJsonObject& node, CommandState& state, const CommandContext& model) {
    const QJsonArray nodes = nodesOf(state);
    const QJsonObject before = state.object(kHover);
    if (node.isEmpty() || ended(nodes) || editing(state)) {
      // (An edit session: nothing follows the pointer, owner 14.11.2026.)
      state.setObject(kHover, {});
      return !before.isEmpty() && !nodes.isEmpty();
    }
    const gp_Pnt design = homeToDesign(*shared, flexPoint(node.value(QStringLiteral("at"))));
    if (!before.isEmpty() && flexSamePlace(before, node, pixels(model, design, 2.0))) {
      return false;
    }
    state.setObject(kHover, node);
    return !nodes.isEmpty();
  };

  // A size the user changed: no later start at an end port changes it.
  const auto sizeTyped = [](CommandState& state, const CommandContext&) { state.setChecked(kSizeTyped, true); };

  def.inputs = {
      choiceInput(kType, QObject::tr("Type"), {}, QString())
          .withTooltip(QObject::tr("The kind of item: a type of the library, whose values can be changed below"))
          .withChoices([shared](const CommandState& state) {
            Choices options;
            for (const QJsonValue& row : shared->types) {
              const QJsonObject type = row.toObject();
              options.append(qMakePair(typeValue(type), str(type, "name")));
            }
            const QJsonObject custom = state.object(kCustom);
            if (!custom.isEmpty() || options.isEmpty()) {
              options.append(qMakePair(kCustomType, str(custom.isEmpty() ? fallbackItem() : custom, "name")));
            }
            return options;
          })
          .withChoicePicker([shared](const CommandState& state) {
            QVector<ChoicePickerRow> rows;
            for (const QJsonValue& row : shared->types) {
              const QJsonObject type = row.toObject();
              const QString library = type.value(QStringLiteral("library")).toString(QStringLiteral("built_in"));
              rows.append(ChoicePickerRow{typeValue(type), libraryHeading(library), str(type, "name"), sizeTag(type),
                                          str(type, "id")});
            }
            const QJsonObject custom = state.object(kCustom);
            if (!custom.isEmpty()) {
              rows.append(
                  ChoicePickerRow{kCustomType, QObject::tr("This design"), str(custom, "name"), sizeTag(custom), QString()});
            }
            return rows;
          })
          .withOnChange([shared](CommandState& state, const CommandContext& model) {
            showItem(state, baseItem(state, *shared), model);
            // The type's own sizes now: a start at an end port takes its size again.
            state.setChecked(kSizeTyped, false);
            state.setObject(kPortSection, {});
            qInfo().noquote() << QStringLiteral("Flex: type %1").arg(state.choice(kType));
          }),
      selectionInput(kNodes, QObject::tr("Route"), kNodeKinds, 0, 0)
          .withTooltip(QObject::tr("Click the route's nodes: pipe ends, faces, edges, points or free space"))
          .withOnPick(
              [shared, place](const SelectionItem& item, CommandState& state, const CommandContext& model) {
                if (!item.at) {
                  return; // picked in the Browser: no place on it
                }
                place(nodeOn(item, nodesOf(state), *shared), false, state, model);
                state.setItems(kNodes, picksOf(nodesOf(state)));
              },
              true)
          .withItemsText([](const Selection&, const CommandState& state, const CommandContext&) {
            const int count = static_cast<int>(nodesOf(state).size());
            return count == 0 ? QString() : QObject::tr("%n node(s)", nullptr, count);
          })
          // (Not the selection rows' "None (all)", mitcad#1713.)
          .withEmptyText(QObject::tr("No nodes yet"))
          .withOnChange([shared](CommandState& state, const CommandContext&) {
            // A pick taken out in the panel takes its nodes; the row
            // cleared starts the route again.
            const Selection items = state.items(kNodes);
            const QJsonArray nodes = nodesOf(state);
            if (picksOf(nodes) == items) {
              return;
            }
            QJsonArray left;
            if (!items.isEmpty()) {
              for (const QJsonValue& value : nodes) {
                const QJsonObject pick = value.toObject().value(QStringLiteral("pick")).toObject();
                if (pick.isEmpty() || items.contains(itemOfPick(pick))) {
                  left.append(value);
                }
              }
            }
            qInfo().noquote() << QStringLiteral("Flex: %1 of %2 node(s) left").arg(left.size()).arg(nodes.size());
            setNodes(state, left);
            state.setObject(kHover, {});
            state.setObject(kTwistNode, {});
            hideRope(*shared);
          }),
      // Where clicks off the model land (mitcad#1713): as a 3D sketch's
      // drawing tools choose their placement plane, Tab steps it.
      choiceInput(kPlaneChoice, QObject::tr("Free Nodes On"),
                  {{kPlaneAuto, QObject::tr("Automatic")},
                   {kPlaneXy, QObject::tr("XY Plane")},
                   {kPlaneYz, QObject::tr("YZ Plane")},
                   {kPlaneZx, QObject::tr("ZX Plane")}},
                  kPlaneAuto)
          .withTooltip(QObject::tr("The plane, through the last node, that clicks beside the model land on (Tab)"))
          .withTabCycle({kPlaneAuto, kPlaneXy, kPlaneYz, kPlaneZx})
          .withOnChange([shared](CommandState& state, const CommandContext& model) {
            updatePlane(state, model, *shared);
            qInfo().noquote() << QStringLiteral("Flex: free nodes on %1").arg(state.choice(kPlaneChoice));
          }),
      valueInput(kTwist, QObject::tr("Twist"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("Turns the section about the route at the node placed last, or at a node clicked"))
          .withLabel([](const CommandState& state) {
            const QJsonArray nodes = nodesOf(state);
            const int index = twistNode(state, nodes);
            return index >= 0 && index + 1 < nodes.size() ? QObject::tr("Twist at Node %1").arg(index + 1)
                                                          : QObject::tr("Twist");
          })
          .withVisible([](const CommandState& state) { return !roundIn(state) && !nodesOf(state).isEmpty(); })
          .withManipulator([shared](const CommandState& state, const CommandContext&) -> std::optional<Manipulator> {
            const QJsonArray nodes = nodesOf(state);
            const int index = twistNode(state, nodes);
            if (nodes.size() < 2 || index < 0) {
              return std::nullopt;
            }
            // About the way the route runs at the node: from the node
            // before it, the first node towards the next.
            const int from = index == 0 ? 0 : index - 1;
            const int to = index == 0 ? 1 : index;
            const gp_Pnt a = homeToDesign(*shared, flexPoint(nodes.at(from).toObject().value(QStringLiteral("at"))));
            const gp_Pnt b = homeToDesign(*shared, flexPoint(nodes.at(to).toObject().value(QStringLiteral("at"))));
            if (a.Distance(b) < 1e-6) {
              return std::nullopt;
            }
            return ring(index == 0 ? a : b, gp_Dir(gp_Vec(a, b)));
          }),
      choiceInput(kSection, QObject::tr("Section"),
                  {{kRound, QObject::tr("Round")}, {kRectangular, QObject::tr("Rectangular")}}, kRound)
          .withChoices([shared](const CommandState& state) {
            Choices options{{kRound, QObject::tr("Round")}, {kRectangular, QObject::tr("Rectangular")}};
            if (flexSectionOf(baseItem(state, *shared)).type == kOutline) {
              options.append(qMakePair(kOutline, QObject::tr("Outline")));
            }
            return options;
          })
          .withOnChange([](CommandState& state, const CommandContext& model) {
            state.setChecked(kSizeTyped, true); // the user's: no end port changes it
            // A section chosen by hand starts with sizes of its own.
            const auto unset = [&state](const QString& key) {
              const double value = state.value(key);
              return !std::isfinite(value) || value <= 0.0;
            };
            if (state.choice(kSection) == kRectangular && (unset(kWidth) || unset(kHeight))) {
              showLength(state, kWidth, 10.0, model);
              showLength(state, kHeight, 2.0, model);
              showLength(state, kCorner, 0.0, model);
            } else if (state.choice(kSection) == kRound && unset(kDiameter)) {
              showLength(state, kDiameter, 10.0, model);
            }
          }),
      valueInput(kDiameter, QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("10 mm"))
          .withStepMinimum(0.0, true)
          .withVisible([](const CommandState& state) { return roundIn(state); })
          .withOnChange(sizeTyped),
      valueInput(kWidth, QObject::tr("Width"), ValueKind::Length, QStringLiteral("10 mm"))
          .withStepMinimum(0.0, true)
          .withVisible([](const CommandState& state) { return state.choice(kSection) == kRectangular; })
          .withOnChange(sizeTyped),
      valueInput(kHeight, QObject::tr("Height"), ValueKind::Length, QStringLiteral("2 mm"))
          .withStepMinimum(0.0, true)
          .withVisible([](const CommandState& state) { return state.choice(kSection) == kRectangular; })
          .withOnChange(sizeTyped),
      valueInput(kCorner, QObject::tr("Corner Radius"), ValueKind::Length, QStringLiteral("0 mm"))
          .withStepMinimum(0.0)
          .withVisible([](const CommandState& state) { return state.choice(kSection) == kRectangular; })
          .withOnChange(sizeTyped),
      checkInput(kHollow, QObject::tr("Hollow"), false)
          .withVisible([](const CommandState& state) { return state.choice(kSection) != kOutline; }),
      valueInput(kWall, QObject::tr("Wall Thickness"), ValueKind::Length, QStringLiteral("1 mm"))
          .withStepMinimum(0.0, true)
          .withVisible([](const CommandState& state) {
            return state.checked(kHollow) && state.choice(kSection) != kOutline;
          }),
      valueInput(kBend, QObject::tr("Bend Radius"), ValueKind::Length, QStringLiteral("30 mm"))
          .withTooltip(QObject::tr("The smallest radius the item bends to; over its flat side when not round"))
          .withStepMinimum(0.0, true)
          .withAuto([](const CommandState&, const CommandContext&) { return QObject::tr("Does not bend this way"); }),
      valueInput(kBendEdge, QObject::tr("Bend Radius on Edge"), ValueKind::Length, QString())
          .withTooltip(QObject::tr("The smallest radius the item bends to over its edge; empty: it does not"))
          .withStepMinimum(0.0, true)
          .withVisible([](const CommandState& state) { return !roundIn(state); })
          .withAuto([](const CommandState&, const CommandContext&) { return QObject::tr("Does not bend this way"); }),
      choiceInput(kAppearance, QObject::tr("Appearance"), {}, QString())
          .withChoices([shared](const CommandState&) {
            Choices options{{QString(), QObject::tr("Default")}};
            options += shared->appearances;
            return options;
          }),
      checkInput(kSaving, QObject::tr("Save as Type"), false).withDisclosure(),
      textInput(kTypeName, QObject::tr("Name"), QString())
          .withVisible([](const CommandState& state) { return state.checked(kSaving); }),
  };
  def.inputs << noteInput(
                    QStringLiteral("save"),
                    [](const CommandState& state) {
                      return state.checked(kSaving)
                                 ? InputNote{QString(), {QObject::tr("Save to My Library")}, {}}
                                 : InputNote{};
                    },
                    [shared, &host](int, const CommandState& state, const CommandContext&) {
                      const QString name = state.text(kTypeName).trimmed();
                      if (name.isEmpty()) {
                        host.showStatus(QObject::tr("Give the type a name."), NoticeLevel::Error);
                        return;
                      }
                      QString error;
                      QJsonObject type = itemIn(state, *shared, nullptr, &error, nullptr, true);
                      if (!error.isEmpty()) {
                        host.showStatus(error, NoticeLevel::Error);
                        return;
                      }
                      type.remove(QStringLiteral("origin"));
                      type.insert(QStringLiteral("name"), name);
                      type.insert(QStringLiteral("cmd"), QStringLiteral("flex.save_type"));
                      type.insert(QStringLiteral("library"), QStringLiteral("user"));
                      QJsonObject result;
                      if (!host.runModelCommand(type, &result)) {
                        return; // the host showed why
                      }
                      shared->savedType = QStringLiteral("user/") + str(result, "id");
                      qInfo().noquote() << QStringLiteral("Flex: saved type %1").arg(shared->savedType);
                      host.showStatus(QObject::tr("Saved %1 to My library.").arg(name), NoticeLevel::Success);
                    })
                    .withFieldColumn();

  // An edited item's home: where its links saw it, while that occurrence
  // still places a component.
  const auto keptHome = [shared](const CommandState& state, const CommandContext& model) {
    const QJsonObject kept = state.object(kKept);
    for (const char* key : {"links", "datum_links"}) {
      const QJsonObject all = kept.value(QString::fromLatin1(key)).toObject();
      for (auto link = all.constBegin(); link != all.constEnd(); ++link) {
        const QString target = str(link.value().toObject(), "target");
        const QString component = target.isEmpty() ? QString() : occurrenceComponent(target, model);
        if (!component.isEmpty()) {
          shared->homeOccurrence = target;
          shared->homeComponent = component;
          return;
        }
      }
    }
  };

  def.init = [shared, keptHome, &host](CommandState& state, const CommandContext& model) {
    readModel(*shared, model);
    keptHome(state, model);
    if (state.object(kKept).isEmpty()) {
      shared->history.clear();
      chooseStartType(state, *shared, model);
      host.showStatus(QObject::tr("Click where the item starts: an end of a pipe, a face or a point."),
                      NoticeLevel::Hint);
    } else {
      host.showStatus(QObject::tr("Drag a node, click + to make the item longer, or change the panel. Enter keeps "
                                  "the changes, Esc cancels them."),
                      NoticeLevel::Hint);
    }
    if (shared->typeProblems > 0) {
      host.showStatus(QObject::tr("Some Flex types of the library could not be read."), NoticeLevel::Warning);
    }
    updatePlane(state, model, *shared);
  };
  def.modelChanged = [shared, keptHome](CommandState& state, const CommandContext& model) {
    readModel(*shared, model);
    keptHome(state, model);
    if (!shared->savedType.isEmpty() && typeOf(*shared, shared->savedType)) {
      // The type just saved is the item's type now.
      state.setChoice(kType, shared->savedType);
      state.setObject(kCustom, {});
      showItem(state, baseItem(state, *shared), model);
    }
    shared->savedType.clear();
  };
  def.ended = [shared](bool) {
    shared->history.clear(); // the edit session's changes go with it
    if (shared->rope) {
      shared->rope->deleteLater();
    }
  };

  def.hover = [shared](const SelectionItem& item, CommandState& state, const CommandContext& model) {
    state.setChecked(kOver, item.isValid());
    QJsonObject portsOf;
    if (item.isValid() && onBody(item) && !flexPortsOf(shared->ports, item.owner).empty()) {
      portsOf = QJsonObject{{QStringLiteral("body"), item.owner}, {QStringLiteral("occurrence"), item.occurrence}};
      placedPorts(*shared, item, model);
    }
    state.setObject(kPortsOf, portsOf);
    updatePlane(state, model, *shared);
    return true;
  };
  def.hoverPreview = [shared, follow](const SelectionItem& item, CommandState& state, const CommandContext& model) {
    state.setChecked(kOver, item.isValid());
    if (editing(state)) {
      return false; // nothing follows the pointer in an edit session (owner 14.11.2026)
    }
    if (!item.isValid() || !item.at || !kNodeKinds.testFlag(item.kind)) {
      // Off the model: the placement plane's point follows (cursorPlaneMoved).
      return false;
    }
    const bool was = state.checked(kHoverTarget);
    state.setChecked(kHoverTarget, true); // a click here makes a node
    return follow(nodeOn(item, nodesOf(state), *shared), state, model) || (!was && !nodesOf(state).isEmpty());
  };
  def.cursorPlane = [](const CommandState& state) -> std::optional<gp_Ax3> {
    const QJsonObject plane = state.object(kPlane);
    if (state.checked(kOver) || plane.isEmpty() || editing(state)) {
      return std::nullopt; // the click picks what is under the pointer
    }
    const gp_Pnt normal = flexPoint(plane.value(QStringLiteral("normal")));
    return gp_Ax3(flexPoint(plane.value(QStringLiteral("origin"))), gp_Dir(normal.X(), normal.Y(), normal.Z()));
  };
  def.cursorPlaneMoved = [shared, follow](const std::optional<gp_Pnt>& point, CommandState& state,
                                          const CommandContext& model) {
    // In space a click makes a node only with Alt held (owner 14.11.2026);
    // without it the pointer still has its dashed line.
    const bool was = state.checked(kHoverTarget);
    const bool target = point.has_value() && altHeld(model);
    state.setChecked(kHoverTarget, target);
    return follow(point ? flexFreeNode(designToHome(*shared, *point)) : QJsonObject(), state, model) ||
           (was != target && !nodesOf(state).isEmpty());
  };
  def.cursorPlaneClicked = [shared, place, &host](const gp_Pnt& point, CommandState& state,
                                                  const CommandContext& model) {
    // A click on empty space places nothing unless Alt is held (owner
    // 14.11.2026: a node on the grid by a stray click was odd); a click on
    // a node placed already (the last one: the end; an earlier one: its
    // twist) is still taken.
    bool onNode = false;
    for (const QJsonValue& value : nodesOf(state)) {
      const gp_Pnt at = homeToDesign(*shared, flexPoint(value.toObject().value(QStringLiteral("at"))));
      onNode = onNode || nearOnScreen(model, at, point, 6.0);
    }
    if (!altHeld(model) && !onNode) {
      qInfo().noquote() << QStringLiteral("Flex: click in empty space not taken (Alt places a free node)");
      host.showStatus(QObject::tr("Click a face, an edge or a point; hold Alt to place a node in space."),
                      NoticeLevel::Hint);
      return false;
    }
    place(flexFreeNode(designToHome(*shared, point)), true, state, model);
    return true;
  };
  def.clickConfirms = [](CommandState& state) {
    if (!state.checked(kFinish)) {
      return false;
    }
    state.setChecked(kFinish, false);
    return true;
  };
  def.confirming = [shared](CommandState& state, const CommandContext&) {
    hideRope(*shared);
    // The item ends at the last click, not at the pointer; its preview was
    // of the nodes placed already (owner 14.11.2026), so it stands.
    state.setObject(kHover, {});
    return false;
  };
  def.backspace = [shared](CommandState& state, const CommandContext& model) {
    QJsonArray nodes = nodesOf(state);
    if (nodes.isEmpty()) {
      return false;
    }
    if (editing(state)) {
      return false; // the handles' Delete takes a node out in an edit session
    }
    nodes = withPanelTwist(state, nodes); // an earlier node keeps the twist shown
    nodes.removeLast();
    state.setObject(kTwistNode, {});
    showTwist(state, nodes.isEmpty() ? 0.0 : nodes.last().toObject().value(QStringLiteral("twist")).toDouble());
    setNodes(state, nodes);
    state.setItems(kNodes, picksOf(nodes));
    state.setObject(kHover, {});
    state.setChecked(kFinish, false);
    hideRope(*shared);
    updatePlane(state, model, *shared);
    qInfo().noquote() << QStringLiteral("Flex: last node taken back, %1 left").arg(nodes.size());
    return true;
  };

  // While the pointer moves (owner 14.11.2026): a thin dashed line from the
  // last node to the pointer, no model call; the route alone (mitcad#1713),
  // drawn as the item's rope, once the pointer rests on a place a click
  // makes a node of; with Shift held, the route on every move. The
  // feature's preview, the solid, is of the nodes placed only.
  def.lightPreviewLive = [](const CommandState& state, const CommandContext& model) {
    return !editing(state) && shiftHeld(model);
  };
  def.lightPreviewMoved = [shared](CommandState& state, CommandHost& model) {
    const QJsonArray nodes = nodesOf(state);
    const QJsonObject hover = state.object(kHover);
    std::optional<std::pair<gp_Pnt, gp_Pnt>> line;
    if (!nodes.isEmpty() && !hover.isEmpty() && !ended(nodes) && !shiftHeld(model) && !editing(state)) {
      line = std::make_pair(homeToDesign(*shared, flexPoint(nodes.last().toObject().value(QStringLiteral("at")))),
                            homeToDesign(*shared, flexPoint(hover.value(QStringLiteral("at")))));
    }
    if (!shared->rope && line) {
      CommandHost* host = &model;
      shared->rope = new flex::FlexRopeOverlay(model.viewer(), [host](const gp_Pnt& point) { return host->pixelSize(point); });
    }
    if (shared->rope) {
      // The route worked out before is of where the pointer was.
      if (!shiftHeld(model)) {
        shared->rope->show(std::nullopt, gp_Trsf(), QString());
      }
      shared->rope->showLine(line);
    }
  };
  def.lightPreview = [shared](const CommandState& state, const CommandContext& model) {
    if (editing(state)) {
      // (An edit session: no route follows the pointer, owner 14.11.2026.)
      return Built::failure(QStringLiteral("an edit session: the handles change the nodes"));
    }
    if (!state.checked(kHoverTarget) && !shiftHeld(model)) {
      // (In empty space without Alt: the dashed line alone, no model call.)
      return Built::failure(QStringLiteral("no place for a node under the pointer"));
    }
    return definitionOf(state, *shared, model, true);
  };
  def.lightPreviewShown = [shared](bool ok, const QString& error, CommandState& state, CommandHost& model) {
    std::optional<flex::FlexItem> routed;
    QString length;
    if (ok) {
      try {
        QJsonObject query{{QStringLiteral("query"), QStringLiteral("flex")}, {QStringLiteral("preview"), true}};
        if (!roundIn(state)) {
          query.insert(QStringLiteral("frames"), true); // how the section lies, for the strip
        }
        // The previewed route has no item: the section is the panel's.
        flex::FlexItem section;
        const FlexSection shown = sectionIn(state);
        section.round = shown.type == kRound;
        section.width = section.round ? shown.diameter : shown.width;
        section.height = section.round ? shown.diameter : shown.height;
        routed = flex::FlexItem::read(model.queryObject(query), &section);
        if (!routed->valid || !routed->hasRoute()) {
          routed.reset();
        } else {
          length = lengthText(std::round(routed->length * 10.0) / 10.0, model);
        }
      } catch (const std::exception& failure) {
        qWarning().noquote() << QStringLiteral("Flex: the route under the pointer was not read: %1").arg(modelText(failure));
      }
    }
    if (!shared->rope && routed) {
      CommandHost* host = &model;
      shared->rope = new flex::FlexRopeOverlay(model.viewer(), [host](const gp_Pnt& point) { return host->pixelSize(point); });
    }
    if (shared->rope) {
      shared->rope->show(routed, worldOf(*shared, shared->homeOccurrence), length);
      if (routed) {
        shared->rope->showLine(std::nullopt); // the route is where the dashed line was
      }
    }
    const QString line = routed ? QStringLiteral("Flex route under the pointer: %1 node(s), %2, %3 warning(s)")
                                      .arg(routed->nodes.size())
                                      .arg(length)
                                      .arg(routed->problems.size())
                                : QStringLiteral("Flex route under the pointer: none%1")
                                      .arg(error.isEmpty() ? QString() : QStringLiteral(" (") + error + QLatin1Char(')'));
    if (line != shared->loggedRope) {
      shared->loggedRope = line;
      qInfo().noquote() << line;
    }
  };
  // The feature's own preview came: its solid is the item now.
  def.previewReported = [shared](QJsonObject&, CommandState&) {
    hideRope(*shared);
    shared->loggedRope.clear();
    return false;
  };

  def.viewLines = [shared](const CommandState& state, const CommandContext& model) {
    std::vector<std::vector<gp_Pnt>> lines;
    if (editing(state)) {
      return lines; // the item's handles show its nodes (flex/FlexEditController.hpp)
    }
    const QJsonArray placed = nodesOf(state);
    QJsonArray nodes = placed;
    if (!state.object(kHover).isEmpty()) {
      nodes.append(state.object(kHover));
    }
    const int chosen = roundIn(state) ? -1 : twistNode(state, placed);
    for (int i = 0; i < nodes.size(); ++i) {
      const gp_Pnt at = homeToDesign(*shared, flexPoint(nodes.at(i).toObject().value(QStringLiteral("at"))));
      // The node the panel's Twist is of stands out.
      addCross(lines, at, pixels(model, at, i == chosen && i + 1 < placed.size() ? 10.0 : 5.0));
    }
    const QJsonObject portsOf = state.object(kPortsOf);
    if (!portsOf.isEmpty() && !ended(placed)) {
      const gp_Trsf world = worldOf(*shared, str(portsOf, "occurrence"));
      for (const FlexPort& port : flexPortsOf(shared->ports, str(portsOf, "body"))) {
        addPortMark(lines, port, world, pixels(model, port.position.Transformed(world), 8.0));
      }
    }
    return lines;
  };

  def.build = [shared](const CommandState& state, const CommandContext& model) {
    Built built = definitionOf(state, *shared, model, false);
    // An edit session's changes, for its Undo (owner 14.11.2026): each new
    // definition with the panel's state that gave it.
    if (editing(state) && built.error.isEmpty() && !built.def.isEmpty() &&
        (shared->history.empty() || shared->history.back().first != built.def)) {
      shared->history.emplace_back(built.def, state);
    }
    return built;
  };
  // Undo in an edit session (owner 14.11.2026, as in sketch editing): the
  // panel and the nodes as they were before the session's last change; the
  // model's history is not touched (OK makes the session one undo step).
  def.undo = [shared, &host](CommandState& state, const CommandContext&) {
    if (!editing(state)) {
      return false;
    }
    if (shared->history.size() < 2) {
      host.showStatus(QObject::tr("Nothing to undo in this edit. Esc cancels the edit."), NoticeLevel::Hint);
      qInfo().noquote() << QStringLiteral("Flex: undo in the edit: nothing to undo");
      return false;
    }
    shared->history.pop_back();
    state = shared->history.back().second;
    hideRope(*shared);
    qInfo().noquote() << QStringLiteral("Flex: undo in the edit: %1 change(s) left, %2 node(s)")
                             .arg(shared->history.size() - 1)
                             .arg(nodesOf(state).size());
    return true;
  };

  // The panel opened on an item: its edit session, with the command's
  // shared state read again.
  def.load = [shared, keptHome](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    readModel(*shared, model);
    shared->history.clear(); // a new edit session
    loadItem(feature, state, *shared, model);
    keptHome(state, model);
    qInfo().noquote() << QStringLiteral("Flex: editing an item of %1 node(s), type %2")
                             .arg(nodesOf(state).size())
                             .arg(state.choice(kType));
  };
  // A Flex item's hover text and selection card (mitcad#1971): the item
  // loaded with the library's types read into state of its own, so that an
  // edit session running beside it keeps its types, end ports, home and
  // Undo as they are (the hover text of an item is built also while the
  // panel edits another, or this one).
  def.hoverLoad = [shared](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    Shared own;
    readTypes(own, model, false);
    loadItem(feature, state, own, model);
    qInfo().noquote() << QStringLiteral("Flex: hover text of %1 read on its own; the panel keeps %2 type(s), %3 end "
                                        "port(s)")
                             .arg(str(feature, "uid"))
                             .arg(shared->types.size())
                             .arg(shared->ports.size());
  };

  // The routed length of the nodes placed, and how the item ends (mitcad#1713: only the status line said Enter,
  // and nothing said a double click).
  def.note = [](const CommandState& state, const CommandContext& model) -> QString {
    try {
      const QJsonObject routed =
          model.queryObject({{QStringLiteral("query"), QStringLiteral("flex")}, {QStringLiteral("preview"), true}});
      const QJsonValue length = routed.value(QStringLiteral("length"));
      // (An edit session: the handles, OK keeps every change as one step.)
      const QString how = editing(state)
                              ? QObject::tr("Drag a node or click + in the view; OK keeps the changes as one step.")
                              : QObject::tr("Click to add a node; double-click or press Enter to finish.");
      if (!length.isDouble()) {
        return QString();
      }
      QString text = QObject::tr("Length: %1").arg(lengthText(std::round(length.toDouble() * 10.0) / 10.0, model));
      // Longer than its route with no direction of gravity in the design:
      // where the extra length goes (mitcad#1713, owner 15.11.2026).
      const QJsonValue slack = routed.value(QStringLiteral("slack"));
      if (slack.isDouble() && slack.toDouble() > 1e-6) {
        bool down = true;
        try {
          down = model.queryObject({{QStringLiteral("query"), QStringLiteral("gravity")}})
                     .value(QStringLiteral("direction"))
                     .isArray();
        } catch (const std::exception&) {
          // Said as without a direction.
          down = false;
        }
        if (!down) {
          text += QLatin1Char('\n') + QObject::tr("The extra length hangs down once gravity is set in Let It Hang.");
        }
      }
      return text + QLatin1Char('\n') + how;
    } catch (const std::exception&) {
      return QString(); // no preview to read
    }
  };

  def.keyText = [shared](const CommandState& state, const CommandContext&) {
    // A loaded item's own name: its hover text's state has the types of
    // its own load, not the panel's (hoverLoad).
    if (editing(state)) {
      return str(state.object(kKept).value(QStringLiteral("item")).toObject(), "name");
    }
    return str(baseItem(state, *shared), "name");
  };
  def.describe = [shared](const CommandState& state, const CommandContext&) {
    // The next item starts with this type, also after a restart.
    shared->lastType = state.choice(kType) == kCustomType ? QString() : state.choice(kType);
    if (!shared->lastType.isEmpty()) {
      rememberPanelValues(kCommandId, {qMakePair(kType, shared->lastType)}, true, false);
    }
    return QStringLiteral("Added flex item %1 through %2 node(s)")
        .arg(str(baseItem(state, *shared), "name"))
        .arg(nodesOf(state).size());
  };
  return def;
}

} // namespace cmd

void registerFlexCommands(CommandRegistry& registry, const CommandContext& context, DocumentHost& host) {
  registry.add(cmd::flexCommand(context, host));
}

bool isFlexEditSession(const CommandState& state) { return cmd::editing(state); }

QJsonObject flexSessionDefinition(const CommandState& state) {
  if (!cmd::editing(state)) {
    return QJsonObject();
  }
  return QJsonObject{{QStringLiteral("type"), QStringLiteral("flex")},
                     {QStringLiteral("nodes"),
                      cmd::flexDefinitionNodes(cmd::withPanelTwist(state, cmd::nodesOf(state)))}};
}

bool setFlexSessionNodes(CommandState& state, const CommandContext& model, const QJsonArray& nodes) {
  if (!cmd::editing(state) || nodes.size() < 2) {
    return false;
  }
  // Where each node's geometry was picked: the occurrence the handles give
  // with it, else the pick the node had, else the edited item's links.
  QHash<QString, QJsonObject> before;
  for (const QJsonValue& value : cmd::nodesOf(state)) {
    const QJsonObject node = value.toObject();
    before.insert(node.value(QStringLiteral("id")).toString(), node);
  }
  const QJsonObject kept = state.object(cmd::kKept);
  const QJsonObject links = kept.value(QStringLiteral("links")).toObject();
  const QJsonObject datumLinks = kept.value(QStringLiteral("datum_links")).toObject();
  QJsonArray placed;
  for (const QJsonValue& value : nodes) {
    QJsonObject node = value.toObject();
    const QString key = node.contains(QStringLiteral("port")) ? QStringLiteral("port") : QStringLiteral("on");
    const QJsonValue given = node.value(key);
    if (given.isObject()) {
      // The reference as stored; where it was picked goes with the pick.
      QJsonObject ref = given.toObject();
      const QString occurrence = ref.value(QStringLiteral("occurrence")).toString();
      ref.remove(QStringLiteral("occurrence"));
      ref.remove(QStringLiteral("context"));
      node.insert(key, ref);
      SelectionItem picked = cmd::itemOf(ref, model);
      if (picked.isValid()) {
        const QJsonObject old = before.value(node.value(QStringLiteral("id")).toString());
        const QJsonObject oldPick = old.value(QStringLiteral("pick")).toObject();
        if (!occurrence.isEmpty()) {
          picked.occurrence = occurrence;
        } else if (!oldPick.isEmpty() && old.value(key) == QJsonValue(ref)) {
          picked.occurrence = cmd::itemOfPick(oldPick).occurrence;
        } else {
          const QJsonObject link = cmd::onBody(picked) ? links.value(picked.owner).toObject()
                                                       : datumLinks.value(picked.owner).toObject();
          picked.occurrence = link.value(QStringLiteral("source")).toString();
        }
        node.insert(QStringLiteral("pick"), cmd::pickOf(picked));
      }
    } else if (given.isString()) {
      SelectionItem picked = cmd::itemOf(given, model); // the origin and its datums
      if (picked.isValid()) {
        node.insert(QStringLiteral("pick"), cmd::pickOf(picked));
      }
    }
    placed.append(node);
  }
  cmd::setNodes(state, placed);
  state.setItems(cmd::kNodes, cmd::picksOf(placed));
  state.setObject(cmd::kHover, {});
  state.setObject(cmd::kTwistNode, {});
  // The panel's Twist is the last node's (withPanelTwist), as the handles left it.
  cmd::showTwist(state, placed.last().toObject().value(QStringLiteral("twist")).toDouble());
  return true;
}

} // namespace mitcad
