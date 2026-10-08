// SPDX-License-Identifier: MIT
// The ASSEMBLE group of the SOLID tab (mitcad#55; commands.md, "Joints"):
// Joint, As-Built Joint, Joint Origin, Rigid Group and Drive Joint, in menu
// order. A joint goes into the active component (an edit keeps the
// joint's own): its origins are geometry of occurrences inside that
// component, named by occurrence paths from it, while picks carry paths
// from the root. The handles of the offset and the angle stand on the
// frame the model resolves an origin to (the `joint_frame` query), so a
// pick shows where its origin snaps; the preview shows the occurrences
// where the joint puts them (the preview's `placements`).
#include "Commands.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <limits>
#include <optional>

#include <QJsonArray>
#include <QJsonDocument>
#include <QKeySequence>
#include <QStringList>

#include <gp_Trsf.hxx>

#include "../framework/CommandRegistry.hpp"
#include "../framework/ModelShapes.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace cmd {
namespace {

const QString kRoot = QStringLiteral("C0");

const Choices kJointKinds = {{QStringLiteral("rigid"), QStringLiteral("Rigid")},
                             {QStringLiteral("revolute"), QStringLiteral("Revolute")},
                             {QStringLiteral("slider"), QStringLiteral("Slider")},
                             {QStringLiteral("cylindrical"), QStringLiteral("Cylindrical")},
                             {QStringLiteral("pin_slot"), QStringLiteral("Pin-Slot")},
                             {QStringLiteral("planar"), QStringLiteral("Planar")},
                             {QStringLiteral("ball"), QStringLiteral("Ball")}};
const Choices kSlideAxes = {{QStringLiteral("x"), QStringLiteral("X")},
                            {QStringLiteral("y"), QStringLiteral("Y")},
                            {QStringLiteral("z"), QStringLiteral("Z")}};
// Every free motion, in the order the panels list their limits.
const QStringList kMotions = {QStringLiteral("tx"), QStringLiteral("ty"), QStringLiteral("tz"),
                              QStringLiteral("rx"), QStringLiteral("ry"), QStringLiteral("rz")};

std::function<bool(const CommandState&)> never() {
  return [](const CommandState&) { return false; };
}

// A text input the panel does not show: what an edit keeps of the
// definition beyond the inputs.
InputDef hidden(const QString& id) { return textInput(id, id).withVisible(never()); }

bool isTurn(const QString& motion) { return motion.startsWith(QLatin1Char('r')); }

QString motionLabel(const QString& motion) {
  const QString axis = motion.right(1).toUpper();
  return isTurn(motion) ? QObject::tr("Turn %1").arg(axis) : QObject::tr("Slide %1").arg(axis);
}

// A kind's free motions (commands.md, "Kinds and frames").
QStringList motionsOf(const QString& kind, const QString& slideAxis) {
  if (kind == QStringLiteral("revolute")) {
    return {QStringLiteral("rz")};
  }
  if (kind == QStringLiteral("slider")) {
    return {QStringLiteral("t") + (slideAxis.isEmpty() ? QStringLiteral("z") : slideAxis)};
  }
  if (kind == QStringLiteral("cylindrical")) {
    return {QStringLiteral("tz"), QStringLiteral("rz")};
  }
  if (kind == QStringLiteral("pin_slot")) {
    return {QStringLiteral("tx"), QStringLiteral("rz")};
  }
  if (kind == QStringLiteral("planar")) {
    return {QStringLiteral("tx"), QStringLiteral("ty"), QStringLiteral("rz")};
  }
  if (kind == QStringLiteral("ball")) {
    return {QStringLiteral("rz"), QStringLiteral("ry"), QStringLiteral("rx")};
  }
  return {};
}

QStringList motionsIn(const CommandState& state) {
  return motionsOf(state.choice(QStringLiteral("kind")), state.choice(QStringLiteral("slide_axis")));
}

// A motion's value as a value input shows it.
QString valueText(const QString& motion, double value, const CommandContext& context) {
  return isTurn(motion) ? degrees(value) + QStringLiteral(" deg") : lengthText(value, context);
}

// A value for the log: degrees or millimetres, rounded.
QString loggedValue(const QString& motion, double value) {
  return isTurn(motion) ? degrees(value) + QStringLiteral(" deg")
                        : number(std::round(value * 1e6) / 1e6) + QStringLiteral(" mm");
}

int uidNumber(const QString& uid) { return uid.mid(1).toInt(); }

// --- Components and occurrence paths

QJsonObject componentsOf(const CommandContext& context) {
  return context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
}

QString activeComponent(const CommandContext& context) {
  try {
    return componentsOf(context).value(QStringLiteral("active")).toString(kRoot);
  } catch (const std::exception&) {
    return kRoot;
  }
}

QString componentName(const QString& component, const CommandContext& context) {
  try {
    for (const QJsonValue& value : componentsOf(context).value(QStringLiteral("components")).toArray()) {
      if (str(value.toObject(), "uid") == component) {
        return str(value.toObject(), "name");
      }
    }
  } catch (const std::exception&) {
    // The uid then.
  }
  return component;
}

// Calls `visit` with each occurrence of the `components` query's tree and
// its path of uids from the root.
void eachOccurrence(const CommandContext& context,
                    const std::function<void(const QJsonObject&, const QString&)>& visit) {
  std::function<void(const QJsonArray&, const QString&)> walk = [&](const QJsonArray& level, const QString& parent) {
    for (const QJsonValue& value : level) {
      const QJsonObject node = value.toObject();
      const QString path = parent.isEmpty() ? str(node, "uid") : parent + QLatin1Char('/') + str(node, "uid");
      visit(node, path);
      walk(node.value(QStringLiteral("children")).toArray(), path);
    }
  };
  try {
    walk(componentsOf(context).value(QStringLiteral("occurrences")).toArray(), QString());
  } catch (const std::exception&) {
    // No occurrences then.
  }
}

int occurrenceCount(const CommandContext& context) {
  int count = 0;
  eachOccurrence(context, [&count](const QJsonObject&, const QString&) { ++count; });
  return count;
}

// The paths (occurrence uids from the root) that place a component; the
// root's is the empty path.
QStringList placingPaths(const QString& component, const CommandContext& context) {
  if (component.isEmpty() || component == kRoot) {
    return {QString()};
  }
  QStringList paths;
  eachOccurrence(context, [&](const QJsonObject& node, const QString& path) {
    if (str(node, "component") == component) {
      paths << path;
    }
  });
  return paths;
}

// Where an occurrence path from the root places its component in the design.
std::optional<gp_Trsf> worldOf(const QString& path, const CommandContext& context) {
  if (path.isEmpty()) {
    return gp_Trsf();
  }
  std::optional<gp_Trsf> found;
  eachOccurrence(context, [&](const QJsonObject& node, const QString& at) {
    if (at == path) {
      found = trsfOf(node.value(QStringLiteral("world")).toArray());
    }
  });
  return found;
}

QString joinPath(const QString& prefix, const QString& path) {
  if (prefix.isEmpty()) {
    return path;
  }
  return path.isEmpty() ? prefix : prefix + QLatin1Char('/') + path;
}

// The component of a feature (an edit's), from the `feature` query.
QString featureComponent(const QString& uid, const CommandContext& context) {
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}})
        .value(QStringLiteral("component"))
        .toString(kRoot);
  } catch (const std::exception&) {
    return kRoot;
  }
}

// The component a joint goes into: an edit keeps its own (the hidden input
// "component"), else the active one.
QString jointComponent(const CommandState& state, const CommandContext& context) {
  const QString loaded = state.text(QStringLiteral("component"));
  return loaded.isEmpty() ? activeComponent(context) : loaded;
}

// An item's occurrence path from a component that `paths` place; none when
// the item is not inside it. The component's own geometry has the empty
// path.
std::optional<QString> pathFrom(const SelectionItem& item, const QStringList& paths) {
  for (const QString& prefix : paths) {
    if (prefix.isEmpty()) {
      return item.occurrence;
    }
    if (item.occurrence == prefix) {
      return QString();
    }
    if (item.occurrence.startsWith(prefix + QLatin1Char('/'))) {
      return item.occurrence.mid(prefix.size() + 1);
    }
  }
  return std::nullopt;
}

// An occurrence, as the inputs of components keep it: a component picked in
// the browser, or the occurrence that places a body clicked in the view.
SelectionItem occurrenceItem(const SelectionItem& item) {
  SelectionItem occurrence{SelectKind::Component, QString(), QString(), QString()};
  occurrence.occurrence = item.occurrence;
  if (item.kind == SelectKind::Component) {
    occurrence.owner = item.owner;
  }
  return occurrence;
}

bool isPlaced(const SelectionItem& item) { return !item.occurrence.isEmpty(); }

// --- Origins

constexpr SelectFilter kOriginKinds = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex |
                                      SelectKind::SketchCurve | SelectKind::SketchPoint |
                                      kConstructionGeometry;

// Geometry that gives a joint a frame (commands.md, "Kinds and frames"):
// planar, cylindrical, conical, spherical and toroidal faces; straight and
// circular edges and sketch curves; points, axes and planes.
bool givesFrame(const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Face:
    return item.geometry != QStringLiteral("other");
  case SelectKind::Edge:
    return item.geometry == QStringLiteral("line") || item.geometry == QStringLiteral("circle");
  case SelectKind::SketchCurve:
    return item.geometry == QStringLiteral("line") || item.geometry == QStringLiteral("circle") ||
           item.geometry == QStringLiteral("arc");
  default:
    return true;
  }
}

// An input of an origin. The origin's planes and axes stay hidden (they
// would cover the components); a shown origin and construction geometry
// still count.
InputDef originInput(const QString& id, const QString& label, const QString& tip, int min = 1) {
  InputDef input = selectionInput(id, label, kOriginKinds, min, 1).withTooltip(tip).withAccepts(givesFrame);
  input.showsOrigin = false;
  return input;
}

QJsonObject originOf(const SelectionItem& item, const QString& path) {
  QJsonObject origin{{QStringLiteral("geometry"), item.reference()}};
  if (!path.isEmpty()) {
    origin.insert(QStringLiteral("occurrence"), path);
  }
  return origin;
}

// The item of a definition's origin, with its path from the root; invalid
// when no input can show it (fixed geometry, a frame given directly).
SelectionItem originItem(const QJsonValue& value, const QString& prefix, const CommandContext& context) {
  const QJsonObject origin = value.toObject();
  if (origin.contains(QStringLiteral("frame_override"))) {
    return SelectionItem();
  }
  SelectionItem item = itemOf(origin.value(QStringLiteral("geometry")), context);
  if (item.isValid()) {
    item.occurrence = joinPath(prefix, str(origin, "occurrence"));
  }
  return item;
}

struct Frame {
  gp_Pnt origin;
  gp_Dir x = gp_Dir(1, 0, 0);
  gp_Dir y = gp_Dir(0, 1, 0);
  gp_Dir z = gp_Dir(0, 0, 1);
};

Frame frameOf(const QJsonObject& frame) {
  Frame result;
  result.origin = pointOf(frame.value(QStringLiteral("origin")));
  result.x = gp_Dir(vectorOf(frame.value(QStringLiteral("x_axis"))));
  result.y = gp_Dir(vectorOf(frame.value(QStringLiteral("y_axis"))));
  result.z = gp_Dir(vectorOf(frame.value(QStringLiteral("z_axis"))));
  return result;
}

// The frame an origin gives at the marker, in the design.
std::optional<Frame> originFrame(const SelectionItem& item, const CommandContext& context) {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("joint_frame")},
                      {QStringLiteral("geometry"), item.reference()}};
  if (!item.occurrence.isEmpty()) {
    request.insert(QStringLiteral("occurrence"), item.occurrence);
  }
  try {
    return frameOf(context.queryObject(request));
  } catch (const std::exception&) {
    return std::nullopt;
  }
}

// The frame of the first of `inputs` that has a pick: the joint's handles
// stand on origin B once it is picked, else on A.
std::optional<Frame> pickedFrame(const CommandState& state, const CommandContext& context,
                                 const QStringList& inputs) {
  for (const QString& input : inputs) {
    const Selection& items = state.items(input);
    if (!items.isEmpty()) {
      if (auto frame = originFrame(items.first(), context)) {
        return frame;
      }
    }
  }
  return std::nullopt;
}

using HandleMaker = std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>;

// The offset: an arrow along the frame's Z axis.
HandleMaker offsetHandle(const QStringList& inputs) {
  return [inputs](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    const auto frame = pickedFrame(state, context, inputs);
    if (!frame) {
      return std::nullopt;
    }
    return arrow(frame->origin, frame->z);
  };
}

// The angle: a ring about the frame's Z axis, 0 along its X axis.
HandleMaker angleHandle(const QStringList& inputs) {
  return [inputs](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    const auto frame = pickedFrame(state, context, inputs);
    if (!frame) {
      return std::nullopt;
    }
    Manipulator handle = ring(frame->origin, frame->z);
    handle.reference = frame->x;
    return handle;
  };
}

// --- Limits and positions

QVector<InputDef> limitInputs() {
  QVector<InputDef> inputs;
  for (const QString& motion : kMotions) {
    const ValueKind kind = isTurn(motion) ? ValueKind::Angle : ValueKind::Length;
    const auto has = [motion](const CommandState& state) { return motionsIn(state).contains(motion); };
    const QString limit = QStringLiteral("limit_") + motion;
    const QString rest = QStringLiteral("rest_on_") + motion;
    const auto limited = [has, limit](const CommandState& state) { return has(state) && state.checked(limit); };
    const auto resting = [has, rest](const CommandState& state) { return has(state) && state.checked(rest); };
    inputs << checkInput(limit, QObject::tr("%1 Limits").arg(motionLabel(motion)), false)
                  .withTooltip(QObject::tr("The motion stays between a minimum and a maximum"))
                  .withVisible(has)
           << valueInput(QStringLiteral("min_") + motion, QObject::tr("Minimum"), kind,
                         isTurn(motion) ? QStringLiteral("-180 deg") : QStringLiteral("0 mm"))
                  .withVisible(limited)
           << valueInput(QStringLiteral("max_") + motion, QObject::tr("Maximum"), kind,
                         isTurn(motion) ? QStringLiteral("180 deg") : QStringLiteral("100 mm"))
                  .withVisible(limited)
           << checkInput(rest, QObject::tr("%1 Rest").arg(motionLabel(motion)), false)
                  .withTooltip(QObject::tr("The value the motion is held at unless driven elsewhere"))
                  .withVisible(has)
           << valueInput(QStringLiteral("rest_") + motion, QObject::tr("Rest Value"), kind,
                         isTurn(motion) ? QStringLiteral("0 deg") : QStringLiteral("0 mm"))
                  .withVisible(resting);
  }
  return inputs;
}

QJsonObject limitsOf(const CommandState& state) {
  QJsonObject limits;
  for (const QString& motion : motionsIn(state)) {
    QJsonObject limit;
    if (state.checked(QStringLiteral("limit_") + motion)) {
      limit.insert(QStringLiteral("min"), state.expression(QStringLiteral("min_") + motion));
      limit.insert(QStringLiteral("max"), state.expression(QStringLiteral("max_") + motion));
    }
    if (state.checked(QStringLiteral("rest_on_") + motion)) {
      limit.insert(QStringLiteral("rest"), state.expression(QStringLiteral("rest_") + motion));
    }
    if (!limit.isEmpty()) {
      limits.insert(motion, limit);
    }
  }
  return limits;
}

void loadLimits(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  const QJsonObject limits = feature.value(QStringLiteral("limits")).toObject();
  for (auto it = limits.begin(); it != limits.end(); ++it) {
    const QString motion = it.key();
    const QJsonObject limit = it.value().toObject();
    if (limit.contains(QStringLiteral("min")) || limit.contains(QStringLiteral("max"))) {
      state.setChecked(QStringLiteral("limit_") + motion, true);
      loadValue(state, QStringLiteral("min_") + motion, limit.value(QStringLiteral("min")), feature, context);
      loadValue(state, QStringLiteral("max_") + motion, limit.value(QStringLiteral("max")), feature, context);
    }
    if (limit.contains(QStringLiteral("rest"))) {
      state.setChecked(QStringLiteral("rest_on_") + motion, true);
      loadValue(state, QStringLiteral("rest_") + motion, limit.value(QStringLiteral("rest")), feature, context);
    }
  }
}

// Limits the panel can hold: a minimum with a maximum.
bool limitsEditable(const QJsonObject& feature) {
  const QJsonObject limits = feature.value(QStringLiteral("limits")).toObject();
  return std::all_of(limits.begin(), limits.end(), [](const QJsonValue& value) {
    const QJsonObject limit = value.toObject();
    return limit.contains(QStringLiteral("min")) == limit.contains(QStringLiteral("max"));
  });
}

// The position a joint is driven to (Drive Joint, a drag) is kept through an
// edit, for the motions the kind still has.
void loadPosition(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  QJsonObject position;
  const QJsonObject stored = feature.value(QStringLiteral("position")).toObject();
  for (auto it = stored.begin(); it != stored.end(); ++it) {
    position.insert(it.key(), context.valueText(it.value(), str(feature, "uid")));
  }
  state.setText(QStringLiteral("position"), QString::fromUtf8(QJsonDocument(position).toJson(QJsonDocument::Compact)));
}

QJsonObject positionOf(const CommandState& state) {
  const QJsonObject stored = QJsonDocument::fromJson(state.text(QStringLiteral("position")).toUtf8()).object();
  QJsonObject position;
  for (const QString& motion : motionsIn(state)) {
    if (stored.contains(motion)) {
      position.insert(motion, stored.value(motion));
    }
  }
  return position;
}

void addMotions(QJsonObject& def, const CommandState& state) {
  const QString kind = state.choice(QStringLiteral("kind"));
  def.insert(QStringLiteral("kind"), kind);
  if (kind == QStringLiteral("slider") && state.choice(QStringLiteral("slide_axis")) != QStringLiteral("z")) {
    def.insert(QStringLiteral("slide_axis"), state.choice(QStringLiteral("slide_axis")));
  }
  const QJsonObject limits = limitsOf(state);
  if (!limits.isEmpty()) {
    def.insert(QStringLiteral("limits"), limits);
  }
  const QJsonObject position = positionOf(state);
  if (!position.isEmpty()) {
    def.insert(QStringLiteral("position"), position);
  }
}

void loadMotions(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  state.setChoice(QStringLiteral("kind"), str(feature, "kind"));
  state.setChoice(QStringLiteral("slide_axis"), feature.value(QStringLiteral("slide_axis")).toString(QStringLiteral("z")));
  loadLimits(feature, state, context);
  loadPosition(feature, state, context);
}

QVector<InputDef> kindInputs() {
  return {
      choiceInput(QStringLiteral("kind"), QObject::tr("Type"), kJointKinds, QStringLiteral("rigid"))
          .withTooltip(QObject::tr("How the components may move against each other")),
      choiceInput(QStringLiteral("slide_axis"), QObject::tr("Slide Along"), kSlideAxes, QStringLiteral("z"))
          .withTooltip(QObject::tr("The axis of the joint's frame the slider moves along"))
          .withVisible([](const CommandState& state) { return state.choice(QStringLiteral("kind")) == QStringLiteral("slider"); }),
  };
}

// --- What the log says after OK

QJsonObject jointsOf(const CommandContext& context) {
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("joints")}});
  } catch (const std::exception&) {
    return QJsonObject();
  }
}

// The entry of a list with the newest uid that `fits`.
QJsonObject newest(const QJsonArray& entries, const std::function<bool(const QJsonObject&)>& fits) {
  QJsonObject found;
  for (const QJsonValue& value : entries) {
    const QJsonObject entry = value.toObject();
    if (fits(entry) && (found.isEmpty() || uidNumber(str(entry, "uid")) > uidNumber(str(found, "uid")))) {
      found = entry;
    }
  }
  return found;
}

QString valuesText(const QJsonObject& values) {
  QStringList parts;
  for (const QString& motion : kMotions) {
    if (values.contains(motion)) {
      parts << QStringLiteral("%1 %2").arg(motion, loggedValue(motion, values.value(motion).toDouble()));
    }
  }
  return parts.join(QStringLiteral(", "));
}

// "Joint1 (revolute): placed, rz 30 deg, moved O2; DOF 1" (a failure: its
// error), as the `joints` query has a joint; with the degrees of freedom of
// its component.
QString jointLine(const QJsonObject& joint, const QJsonObject& joints) {
  QString line = QStringLiteral("%1 (%2): %3").arg(str(joint, "name"), str(joint, "kind"), str(joint, "state"));
  const QString error = str(joint, "error");
  if (!error.isEmpty()) {
    line += QStringLiteral(": ") + error;
  }
  const QString values = valuesText(joint.value(QStringLiteral("values")).toObject());
  if (!values.isEmpty()) {
    line += QStringLiteral(", ") + values;
  }
  QStringList moved;
  for (const QJsonValue& uid : joint.value(QStringLiteral("moved")).toArray()) {
    moved << uid.toString();
  }
  if (!moved.isEmpty()) {
    line += QStringLiteral(", moved ") + moved.join(QStringLiteral(" "));
  }
  for (const QJsonValue& value : joints.value(QStringLiteral("dof")).toArray()) {
    const QJsonObject dof = value.toObject();
    if (str(dof, "component") == str(joint, "component")) {
      line += QStringLiteral("; DOF %1").arg(dof.value(QStringLiteral("total")).toInt());
    }
  }
  return line;
}

QString addedJoint(const CommandContext& context, const QString& type, const QString& what) {
  const QJsonObject joints = jointsOf(context);
  const QJsonObject joint = newest(joints.value(QStringLiteral("joints")).toArray(),
                                   [&type](const QJsonObject& entry) { return str(entry, "type") == type; });
  if (joint.isEmpty()) {
    return QStringLiteral("Added %1").arg(what);
  }
  return QStringLiteral("Added %1 %2").arg(what, jointLine(joint, joints));
}

// --- Joint

Built originsOf(const CommandState& state, const CommandContext& context, const QStringList& inputs,
                QJsonObject& def) {
  const QString component = jointComponent(state, context);
  const QStringList paths = placingPaths(component, context);
  for (const QString& input : inputs) {
    const Selection& items = state.items(input);
    if (items.isEmpty()) {
      continue;
    }
    const auto path = pathFrom(items.first(), paths);
    if (!path) {
      return Built::failure(QObject::tr("Select geometry inside %1, the component the joint goes into.")
                                .arg(componentName(component, context)),
                            input);
    }
    def.insert(input, originOf(items.first(), *path));
  }
  return Built::of(def);
}

} // namespace

CommandDef jointCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.joint");
  def.name = QObject::tr("Joint");
  def.icon = QStringLiteral("joint");
  def.tooltip = QObject::tr("Joins two components at origins on their geometry, and says how they may move");
  def.shortcut = QKeySequence(Qt::Key_J);
  def.group = QStringLiteral("ASSEMBLE");
  def.pinned = true;
  def.keywords = {QStringLiteral("assembly"), QStringLiteral("hinge"), QStringLiteral("pivot"),
                  QStringLiteral("connect"), QStringLiteral("revolute"), QStringLiteral("slider")};
  def.featureType = QStringLiteral("joint");
  def.inputs = {
      originInput(QStringLiteral("a"), QObject::tr("Origin A"),
                  QObject::tr("A face, edge, point, axis or plane of the component that moves onto B")),
      originInput(QStringLiteral("b"), QObject::tr("Origin B"),
                  QObject::tr("A face, edge, point, axis or plane of another component")),
  };
  def.inputs += kindInputs();
  def.inputs += {
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the joint's Z axis"))
          .withManipulator(offsetHandle({QStringLiteral("b"), QStringLiteral("a")})),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("About the joint's Z axis"))
          .withManipulator(angleHandle({QStringLiteral("b"), QStringLiteral("a")})),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Turns origin A over: a face onto a face")),
  };
  def.inputs += limitInputs();
  def.inputs += {hidden(QStringLiteral("component")), hidden(QStringLiteral("position"))};
  def.enabled = [&context] { return occurrenceCount(context) > 0; };

  def.build = [](const CommandState& state, const CommandContext& context) {
    QJsonObject joint{{QStringLiteral("type"), QStringLiteral("joint")}};
    Built result = originsOf(state, context, {QStringLiteral("a"), QStringLiteral("b")}, joint);
    if (!result.error.isEmpty()) {
      return result;
    }
    addMotions(joint, state);
    if (nonZero(state, QStringLiteral("offset"))) {
      joint.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
    }
    if (nonZero(state, QStringLiteral("angle"))) {
      joint.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
    }
    if (state.checked(QStringLiteral("flip"))) {
      joint.insert(QStringLiteral("flip"), true);
    }
    return Built::of(joint);
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString component = featureComponent(str(feature, "uid"), context);
    state.setText(QStringLiteral("component"), component);
    const QString prefix = placingPaths(component, context).value(0);
    state.setItems(QStringLiteral("a"), {originItem(feature.value(QStringLiteral("a")), prefix, context)});
    state.setItems(QStringLiteral("b"), {originItem(feature.value(QStringLiteral("b")), prefix, context)});
    loadMotions(feature, state, context);
    loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    return originItem(feature.value(QStringLiteral("a")), QString(), context).isValid() &&
           originItem(feature.value(QStringLiteral("b")), QString(), context).isValid() &&
           limitsEditable(feature);
  };

  def.describe = [](const CommandState&, const CommandContext& context) {
    return addedJoint(context, QStringLiteral("joint"), QStringLiteral("joint"));
  };
  return def;
}

// ---------------------------------------------------------------------------
// As-Built Joint: two components joined where they are.

CommandDef asBuiltJointCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.as_built_joint");
  def.name = QObject::tr("As-Built Joint");
  def.icon = QStringLiteral("as-built-joint");
  def.tooltip = QObject::tr("Joins two components where they are now, and says how they may move");
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("assembly"), QStringLiteral("in place"), QStringLiteral("connect")};
  def.featureType = QStringLiteral("as_built_joint");
  const auto componentInput = [](const QString& id, const QString& label, const QString& tip) {
    return selectionInput(id, label, SelectKind::Component | SelectKind::Body, 1, 1)
        .withTooltip(tip)
        .withAccepts(isPlaced)
        .withConvert(occurrenceItem);
  };
  def.inputs = {
      componentInput(QStringLiteral("a"), QObject::tr("Component A"),
                     QObject::tr("A component in the browser, or a body it places")),
      componentInput(QStringLiteral("b"), QObject::tr("Component B"),
                     QObject::tr("The component A moves against")),
  };
  def.inputs += kindInputs();
  def.inputs += {
      originInput(QStringLiteral("origin"), QObject::tr("Motion Origin"),
                  QObject::tr("Where the free motions turn and slide (B's coordinates when none)"), 0),
  };
  def.inputs += limitInputs();
  // The relation recorded (rows), and the pair it was recorded for: an edit
  // keeps it while A and B stay.
  def.inputs += {hidden(QStringLiteral("component")), hidden(QStringLiteral("position")),
                 hidden(QStringLiteral("relative")), hidden(QStringLiteral("pair"))};
  def.enabled = [&context] { return occurrenceCount(context) > 1; };

  def.build = [](const CommandState& state, const CommandContext& context) {
    const QString component = jointComponent(state, context);
    const QStringList paths = placingPaths(component, context);
    QJsonObject joint{{QStringLiteral("type"), QStringLiteral("as_built_joint")}};
    QString sides[2];
    const QString inputs[2] = {QStringLiteral("a"), QStringLiteral("b")};
    for (int i = 0; i < 2; ++i) {
      const auto path = pathFrom(state.items(inputs[i]).first(), paths);
      if (!path || path->isEmpty()) {
        return Built::failure(QObject::tr("Select a component inside %1.").arg(componentName(component, context)),
                              inputs[i]);
      }
      sides[i] = state.items(inputs[i]).first().occurrence;
      joint.insert(inputs[i], *path);
    }
    if (sides[0] == sides[1]) {
      return Built::failure(QObject::tr("Select two different components."), QStringLiteral("b"));
    }
    if (!state.items(QStringLiteral("origin")).isEmpty()) {
      const SelectionItem& origin = state.items(QStringLiteral("origin")).first();
      const auto path = pathFrom(origin, paths);
      if (!path) {
        return Built::failure(QObject::tr("Select geometry inside %1.").arg(componentName(component, context)),
                              QStringLiteral("origin"));
      }
      joint.insert(QStringLiteral("origin"), originOf(origin, *path));
    }
    // A's placement in B's coordinates where they are now; an edit keeps
    // the recorded one while A and B stay.
    const QString pair = sides[0] + QLatin1Char('|') + sides[1];
    QJsonArray relative;
    if (state.text(QStringLiteral("pair")) == pair) {
      relative = QJsonDocument::fromJson(state.text(QStringLiteral("relative")).toUtf8()).array();
    }
    if (relative.isEmpty()) {
      const auto a = worldOf(sides[0], context);
      const auto b = worldOf(sides[1], context);
      if (!a || !b) {
        return Built::failure(QObject::tr("The components are not placed."), QStringLiteral("a"));
      }
      const gp_Trsf t = b->Inverted() * *a;
      for (int r = 1; r <= 3; ++r) {
        relative.append(QJsonArray{t.Value(r, 1), t.Value(r, 2), t.Value(r, 3), t.Value(r, 4)});
      }
    }
    joint.insert(QStringLiteral("relative"), relative);
    addMotions(joint, state);
    return Built::of(joint);
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString component = featureComponent(str(feature, "uid"), context);
    state.setText(QStringLiteral("component"), component);
    const QString prefix = placingPaths(component, context).value(0);
    QString sides[2];
    const char* const keys[2] = {"a", "b"};
    for (int i = 0; i < 2; ++i) {
      SelectionItem item{SelectKind::Component, QString(), QString(), QString()};
      item.occurrence = joinPath(prefix, str(feature, keys[i]));
      sides[i] = item.occurrence;
      state.setItems(QLatin1String(keys[i]), {item});
    }
    if (feature.contains(QStringLiteral("origin"))) {
      state.setItems(QStringLiteral("origin"), {originItem(feature.value(QStringLiteral("origin")), prefix, context)});
    }
    const QJsonArray relative = feature.value(QStringLiteral("relative")).toArray();
    if (!relative.isEmpty()) {
      state.setText(QStringLiteral("relative"), QString::fromUtf8(QJsonDocument(relative).toJson(QJsonDocument::Compact)));
      state.setText(QStringLiteral("pair"), sides[0] + QLatin1Char('|') + sides[1]);
    }
    loadMotions(feature, state, context);
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    return (!feature.contains(QStringLiteral("origin")) ||
            originItem(feature.value(QStringLiteral("origin")), QString(), context).isValid()) &&
           limitsEditable(feature);
  };

  def.describe = [](const CommandState&, const CommandContext& context) {
    return addedJoint(context, QStringLiteral("as_built_joint"), QStringLiteral("as-built joint"));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Joint Origin: a frame on geometry of the active component.

CommandDef jointOriginCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.joint_origin");
  def.name = QObject::tr("Joint Origin");
  def.icon = QStringLiteral("joint-origin");
  def.tooltip = QObject::tr("A frame on geometry of the active component for joints to use");
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("frame"), QStringLiteral("assembly")};
  def.featureType = QStringLiteral("joint_origin");
  def.inputs = {
      originInput(QStringLiteral("geometry"), QObject::tr("Geometry"),
                  QObject::tr("A face, edge, point, axis or plane of the active component itself")),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the frame's Z axis"))
          .withManipulator(offsetHandle({QStringLiteral("geometry")})),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("About the frame's Z axis"))
          .withManipulator(angleHandle({QStringLiteral("geometry")})),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withTooltip(QObject::tr("Turns the frame over")),
      hidden(QStringLiteral("component")),
      hidden(QStringLiteral("override")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext& context) {
    const QString component = jointComponent(state, context);
    const SelectionItem& item = state.items(QStringLiteral("geometry")).first();
    const auto path = pathFrom(item, placingPaths(component, context));
    if (!path || !path->isEmpty()) {
      return Built::failure(QObject::tr("Select geometry of %1 itself.").arg(componentName(component, context)),
                            QStringLiteral("geometry"));
    }
    QJsonObject origin{{QStringLiteral("type"), QStringLiteral("joint_origin")},
                       {QStringLiteral("geometry"), item.reference()}};
    if (nonZero(state, QStringLiteral("offset"))) {
      origin.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
    }
    if (nonZero(state, QStringLiteral("angle"))) {
      origin.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
    }
    if (state.checked(QStringLiteral("flip"))) {
      origin.insert(QStringLiteral("flip"), true);
    }
    const QJsonObject frameOverride = QJsonDocument::fromJson(state.text(QStringLiteral("override")).toUtf8()).object();
    if (!frameOverride.isEmpty()) {
      origin.insert(QStringLiteral("frame_override"), frameOverride);
    }
    return Built::of(origin);
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString component = featureComponent(str(feature, "uid"), context);
    state.setText(QStringLiteral("component"), component);
    SelectionItem item = itemOf(feature.value(QStringLiteral("geometry")), context);
    item.occurrence = placingPaths(component, context).value(0);
    state.setItems(QStringLiteral("geometry"), {item});
    loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    const QJsonObject frameOverride = feature.value(QStringLiteral("frame_override")).toObject();
    if (!frameOverride.isEmpty()) {
      state.setText(QStringLiteral("override"),
                    QString::fromUtf8(QJsonDocument(frameOverride).toJson(QJsonDocument::Compact)));
    }
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("geometry")), context);
  };

  def.describe = [](const CommandState&, const CommandContext& context) {
    // The newest joint origin's datum: a plane with its frame.
    const QJsonArray features = context.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}})
                                    .value(QStringLiteral("features"))
                                    .toArray();
    const QJsonObject made = newest(features, [](const QJsonObject& feature) {
      return str(feature, "type") == QStringLiteral("joint_origin");
    });
    if (made.isEmpty()) {
      return QStringLiteral("Added joint origin");
    }
    QJsonObject datum;
    try {
      datum = context.queryObject({{QStringLiteral("query"), QStringLiteral("datum")}, {QStringLiteral("uid"), str(made, "uid")}});
    } catch (const std::exception& e) {
      return QStringLiteral("Added joint origin %1: %2").arg(str(made, "name"), QString::fromUtf8(e.what()));
    }
    const auto xyz = [&datum](const char* key) {
      QStringList parts;
      for (const QJsonValue& c : datum.value(QLatin1String(key)).toArray()) {
        parts << number(std::abs(c.toDouble()) < 1e-9 ? 0.0 : std::round(c.toDouble() * 1e6) / 1e6);
      }
      return QStringLiteral("(%1)").arg(parts.join(QStringLiteral(", ")));
    };
    return QStringLiteral("Added joint origin %1: origin %2, normal %3").arg(str(made, "name"), xyz("origin"), xyz("normal"));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Rigid Group: components that move as one.

CommandDef rigidGroupCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.rigid_group");
  def.name = QObject::tr("Rigid Group");
  def.icon = QStringLiteral("rigid-group");
  def.tooltip = QObject::tr("Components that move as one");
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("assembly"), QStringLiteral("lock"), QStringLiteral("fix together")};
  def.featureType = QStringLiteral("rigid_group");
  def.inputs = {
      selectionInput(QStringLiteral("occurrences"), QObject::tr("Components"), SelectKind::Component | SelectKind::Body, 2, 0)
          .withTooltip(QObject::tr("Components in the browser, or bodies they place"))
          .withAccepts(isPlaced)
          .withConvert(occurrenceItem),
      hidden(QStringLiteral("component")),
  };
  def.enabled = [&context] { return occurrenceCount(context) > 1; };

  def.build = [](const CommandState& state, const CommandContext& context) {
    const QString component = jointComponent(state, context);
    const QStringList paths = placingPaths(component, context);
    QJsonArray occurrences;
    for (const SelectionItem& item : state.items(QStringLiteral("occurrences"))) {
      const auto path = pathFrom(item, paths);
      if (!path || path->isEmpty()) {
        return Built::failure(QObject::tr("Select components inside %1.").arg(componentName(component, context)),
                              QStringLiteral("occurrences"));
      }
      // The occurrence placed in the component: a sub-assembly moves whole.
      const QString top = path->section(QLatin1Char('/'), 0, 0);
      if (!occurrences.contains(top)) {
        occurrences.append(top);
      }
    }
    if (occurrences.size() < 2) {
      return Built::failure(QObject::tr("Select two or more components."), QStringLiteral("occurrences"));
    }
    return Built::of({{QStringLiteral("type"), QStringLiteral("rigid_group")}, {QStringLiteral("occurrences"), occurrences}});
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString component = featureComponent(str(feature, "uid"), context);
    state.setText(QStringLiteral("component"), component);
    const QString prefix = placingPaths(component, context).value(0);
    Selection items;
    for (const QJsonValue& uid : feature.value(QStringLiteral("occurrences")).toArray()) {
      SelectionItem item{SelectKind::Component, QString(), QString(), QString()};
      item.occurrence = joinPath(prefix, uid.toString());
      items.append(item);
    }
    state.setItems(QStringLiteral("occurrences"), items);
  };

  def.describe = [](const CommandState&, const CommandContext& context) {
    const QJsonObject group = newest(jointsOf(context).value(QStringLiteral("rigid_groups")).toArray(),
                                     [](const QJsonObject&) { return true; });
    QStringList names;
    for (const QJsonValue& name : group.value(QStringLiteral("names")).toArray()) {
      names << name.toString();
    }
    return QStringLiteral("Added rigid group %1: %2").arg(str(group, "name"), names.join(QStringLiteral(", ")));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Drive Joint: a free motion of a joint driven to a value. The command runs
// while the inputs change (as sketch commands do) and OK keeps it: one undo
// step, `Drive Joint1`.

namespace {

QJsonObject jointEntry(const QString& uid, const CommandContext& context) {
  for (const QJsonValue& value : jointsOf(context).value(QStringLiteral("joints")).toArray()) {
    if (str(value.toObject(), "uid") == uid) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

QStringList jointMotions(const QJsonObject& joint) {
  QStringList motions;
  for (const QJsonValue& motion : joint.value(QStringLiteral("motions")).toArray()) {
    motions << motion.toString();
  }
  return motions;
}

QString drivenJoint(const CommandState& state) {
  const Selection& items = state.items(QStringLiteral("joint"));
  return items.isEmpty() ? QString() : items.first().owner;
}

QString valueInputOf(const QString& motion) {
  return isTurn(motion) ? QStringLiteral("angle") : QStringLiteral("distance");
}

// The motion's value where the joint is now (its driven position when it
// does not hold at the marker).
void takeCurrentValue(CommandState& state, const CommandContext& context) {
  const QJsonObject joint = jointEntry(drivenJoint(state), context);
  const QStringList motions = jointMotions(joint);
  QString motion = state.choice(QStringLiteral("motion"));
  if (!motions.contains(motion)) {
    motion = motions.value(0);
    state.setChoice(QStringLiteral("motion"), motion);
  }
  if (motion.isEmpty()) {
    return;
  }
  const QJsonValue at = joint.value(QStringLiteral("values")).toObject().value(motion);
  const double value = at.isDouble() ? at.toDouble()
                                     : joint.value(QStringLiteral("position")).toObject().value(motion).toDouble();
  const QString input = valueInputOf(motion);
  state.setText(input, valueText(motion, value, context));
  state.setValue(input, std::numeric_limits<double>::quiet_NaN());
}

// The driven motion's handle on the joint's frame B (where frame A is at
// the value 0), in the design.
HandleMaker driveHandle() {
  return [](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    const QJsonObject joint = jointEntry(drivenJoint(state), context);
    const QJsonObject frames = joint.value(QStringLiteral("frames")).toObject();
    const QString motion = state.choice(QStringLiteral("motion"));
    if (frames.isEmpty() || motion.isEmpty()) {
      return std::nullopt;
    }
    Frame frame = frameOf(frames.value(QStringLiteral("b")).toObject());
    // Frames are in the joint's component: placed by its first occurrence.
    if (const auto world = worldOf(placingPaths(str(joint, "component"), context).value(0), context)) {
      frame.origin.Transform(*world);
      frame.x.Transform(*world);
      frame.y.Transform(*world);
      frame.z.Transform(*world);
    }
    const QChar axis = motion.at(1);
    const gp_Dir along = axis == QLatin1Char('x') ? frame.x : axis == QLatin1Char('y') ? frame.y : frame.z;
    if (!isTurn(motion)) {
      return arrow(frame.origin, along);
    }
    Manipulator handle = ring(frame.origin, along);
    handle.reference = axis == QLatin1Char('x') ? frame.y : frame.x;
    return handle;
  };
}

bool drivable(const SelectionItem& item, const CommandContext& context) {
  return item.kind == SelectKind::Feature && !jointMotions(jointEntry(item.owner, context)).isEmpty();
}

} // namespace

CommandDef driveJointCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.drive_joint");
  def.name = QObject::tr("Drive Joint");
  def.icon = QStringLiteral("drive-joint");
  def.tooltip = QObject::tr("Moves a joint's free motion to a value: the components follow");
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("motion"), QStringLiteral("animate"), QStringLiteral("position"),
                  QStringLiteral("turn joint")};
  def.sketchCommand = true;
  const auto turning = [](const CommandState& state) { return isTurn(state.choice(QStringLiteral("motion"))); };
  const auto sliding = [](const CommandState& state) {
    const QString motion = state.choice(QStringLiteral("motion"));
    return !motion.isEmpty() && !isTurn(motion);
  };
  def.inputs = {
      selectionInput(QStringLiteral("joint"), QObject::tr("Joint"), SelectKind::Feature, 1, 1)
          .withTooltip(QObject::tr("A joint with free motions, in the timeline or the browser"))
          .withAccepts([&context](const SelectionItem& item) { return drivable(item, context); })
          .withOnPick([](const SelectionItem&, CommandState& state, const CommandContext& model) {
            takeCurrentValue(state, model);
          }, false),
      choiceInput(QStringLiteral("motion"), QObject::tr("Motion"), {}, QString())
          .withChoices([&context](const CommandState& state) {
            Choices choices;
            for (const QString& motion : jointMotions(jointEntry(drivenJoint(state), context))) {
              choices.append({motion, motionLabel(motion)});
            }
            return choices;
          })
          .withOnChange([](CommandState& state, const CommandContext& model) { takeCurrentValue(state, model); }),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(turning)
          .withManipulator(driveHandle()),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(sliding)
          .withManipulator(driveHandle()),
  };
  def.enabled = [&context] {
    const QJsonArray joints = jointsOf(context).value(QStringLiteral("joints")).toArray();
    return std::any_of(joints.begin(), joints.end(), [](const QJsonValue& joint) {
      return !jointMotions(joint.toObject()).isEmpty();
    });
  };
  // The joint selected before, else the newest one that has free motions.
  def.init = [](CommandState& state, const CommandContext& context) {
    if (state.items(QStringLiteral("joint")).isEmpty()) {
      const QJsonObject joint = newest(jointsOf(context).value(QStringLiteral("joints")).toArray(),
                                       [](const QJsonObject& entry) { return !jointMotions(entry).isEmpty(); });
      if (!joint.isEmpty()) {
        state.setItems(QStringLiteral("joint"), {{SelectKind::Feature, str(joint, "uid"), QString(), QString()}});
      }
    }
    takeCurrentValue(state, context);
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    const QString motion = state.choice(QStringLiteral("motion"));
    if (motion.isEmpty()) {
      return Built::failure(QObject::tr("The joint has no free motion."), QStringLiteral("joint"));
    }
    return Built::of({{QStringLiteral("cmd"), QStringLiteral("drive_joint")},
                      {QStringLiteral("joint"), drivenJoint(state)},
                      {QStringLiteral("values"), QJsonObject{{motion, state.expression(valueInputOf(motion))}}}});
  };
  def.describe = [](const CommandState& state, const CommandContext& context) {
    const QJsonObject joints = jointsOf(context);
    const QString uid = drivenJoint(state);
    for (const QJsonValue& value : joints.value(QStringLiteral("joints")).toArray()) {
      if (str(value.toObject(), "uid") == uid) {
        return QStringLiteral("Drove joint %1").arg(jointLine(value.toObject(), joints));
      }
    }
    return QStringLiteral("Drove joint %1").arg(uid);
  };
  return def;
}

} // namespace cmd

// The ASSEMBLE group (mitcad#55), in menu order.
void registerAssembleCommands(CommandRegistry& registry, const CommandContext& context) {
  using namespace cmd;
  registry.add(jointCommand(context));
  registry.add(asBuiltJointCommand(context));
  registry.add(jointOriginCommand(context));
  registry.add(rigidGroupCommand(context));
  registry.add(driveJointCommand(context));
}

} // namespace mitcad
