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
#include "framework/ModelMessages.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <initializer_list>
#include <limits>
#include <optional>

#include <QJsonArray>
#include <QJsonDocument>
#include <QKeySequence>
#include <QRegularExpression>
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

QString activeComponent(const CommandContext& context) {
  try {
    return componentsOf(context).value(QStringLiteral("active")).toString(kRoot);
  } catch (const std::exception&) {
    return kRoot;
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

// The occurrence that moves with an item: the one placed directly in the
// joint's component that the item lies in, as a path from the root; none
// for the component's own geometry or an item outside it.
QStringList movingOccurrence(const SelectionItem& item, const CommandState& state, const CommandContext& context) {
  const auto path = pathFrom(item, placingPaths(jointComponent(state, context), context));
  if (!path || path->isEmpty()) {
    return {};
  }
  // The item's path from the root ends with its path from the component.
  const QString top = path->section(QLatin1Char('/'), 0, 0);
  return {item.occurrence.left(item.occurrence.size() - path->size()) + top};
}

// The ghost of side A (mitcad#147): while `other` waits for its pick, A's
// occurrence is see-through and not pickable, so that geometry behind it
// can be picked. Once both are picked the preview shows A where the joint
// puts it, normally.
std::function<QStringList(const SelectionItem&, const CommandState&, const CommandContext&)>
ghostUntilPicked(const QString& other) {
  return [other](const SelectionItem& item, const CommandState& state, const CommandContext& context) {
    return state.items(other).isEmpty() ? movingOccurrence(item, state, context) : QStringList();
  };
}

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

// An input of an origin: construction geometry counts, the origin's as its
// light bulb shows it or from the browser (mitcad#1030).
InputDef originInput(const QString& id, const QString& label, const QString& tip, int min = 1) {
  return selectionInput(id, label, kOriginKinds, min, 1).withTooltip(tip).withAccepts(givesFrame);
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

// --- Where the preview puts the joint's sides (mitcad#1745)

const QString kPreviewPlacements = QStringLiteral("preview_placements");

// A preview's report came (CommandDef::previewReported): the occurrences it
// places elsewhere, by their paths from the root, kept for the handles.
// Nothing to compute again.
bool keepPlacements(QJsonObject& report, CommandState& state) {
  QJsonObject placed;
  if (report.value(QStringLiteral("status")).toString() == QStringLiteral("ok")) {
    for (const QJsonValue& value : report.value(QStringLiteral("placements")).toArray()) {
      const QJsonObject placement = value.toObject();
      placed.insert(str(placement, "path"), placement.value(QStringLiteral("transform")));
    }
  }
  state.setObject(kPreviewPlacements, placed);
  return false;
}

// Whether the shown preview places the occurrence of `item` elsewhere: it,
// or an occurrence that holds it.
bool movedInPreview(const SelectionItem& item, const CommandState& state) {
  const QJsonObject placed = state.object(kPreviewPlacements);
  for (auto it = placed.begin(); it != placed.end(); ++it) {
    if (item.occurrence == it.key() || item.occurrence.startsWith(it.key() + QLatin1Char('/'))) {
      return true;
    }
  }
  return false;
}

// Where the shown preview moves a pick (CommandDef::pickMoved): an origin
// on a component the preview places elsewhere is drawn there, as the move
// from where the design has the component.
std::optional<gp_Trsf> pickMovedInPreview(const SelectionItem& item, const CommandState& state,
                                          const CommandContext& context) {
  const QJsonObject placed = state.object(kPreviewPlacements);
  for (auto it = placed.begin(); it != placed.end(); ++it) {
    if (item.occurrence != it.key() && !item.occurrence.startsWith(it.key() + QLatin1Char('/'))) {
      continue;
    }
    const auto before = worldOf(it.key(), context);
    if (!before) {
      return std::nullopt;
    }
    return trsfOf(it.value().toArray()) * before->Inverted();
  }
  return std::nullopt;
}

// The frame Joint's handles stand on, at the joint's origin: origin B's,
// where A moves onto it (the offset along its Z axis, the angle about it
// from its X axis). When the preview moves B's side instead (A's is held
// by ground or by other joints), origin A's as Flip turns it, its Z axis
// reversed: the offset's arrow then reaches where B's origin goes, and the
// angle's ring turns as B turns.
std::optional<Frame> jointHandleFrame(const CommandState& state, const CommandContext& context) {
  const Selection& a = state.items(QStringLiteral("a"));
  const Selection& b = state.items(QStringLiteral("b"));
  if (!a.isEmpty() && !b.isEmpty() && movedInPreview(b.first(), state) && !movedInPreview(a.first(), state)) {
    if (auto frame = originFrame(a.first(), context)) {
      if (!state.checked(QStringLiteral("flip"))) {
        // Half a turn about X: Z and Y reversed.
        frame->z.Reverse();
        frame->y.Reverse();
      }
      return frame;
    }
  }
  return pickedFrame(state, context, {QStringLiteral("b"), QStringLiteral("a")});
}

std::optional<Manipulator> jointOffsetHandle(const CommandState& state, const CommandContext& context) {
  const auto frame = jointHandleFrame(state, context);
  if (!frame) {
    return std::nullopt;
  }
  return arrow(frame->origin, frame->z);
}

std::optional<Manipulator> jointAngleHandle(const CommandState& state, const CommandContext& context) {
  const auto frame = jointHandleFrame(state, context);
  if (!frame) {
    return std::nullopt;
  }
  Manipulator handle = ring(frame->origin, frame->z);
  handle.reference = frame->x;
  return handle;
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

// Where the free motions start when the joint places its moving side
// (mitcad#1745, the model's `start`): a new joint's at 0, so that its
// origins meet at the offset and angle; an edit keeps the joint's own, else
// takes them where the joint is (so that a new offset or angle moves the
// side by its change), and leaves out those it cannot tell.
const QString kStart = QStringLiteral("start");
const QString kStartLoaded = QStringLiteral("start_loaded");

void loadStart(const QJsonObject& feature, CommandState& state, const QJsonObject& joints) {
  QJsonObject start = feature.value(kStart).toObject();
  const QString uid = str(feature, "uid");
  const QJsonObject position = feature.value(QStringLiteral("position")).toObject();
  const QJsonObject limits = feature.value(QStringLiteral("limits")).toObject();
  for (const QJsonValue& value : joints.value(QStringLiteral("joints")).toArray()) {
    const QJsonObject joint = value.toObject();
    if (str(joint, "uid") != uid) {
      continue;
    }
    const QJsonObject values = joint.value(QStringLiteral("values")).toObject();
    for (auto it = values.begin(); it != values.end(); ++it) {
      const bool driven = position.contains(it.key()) ||
                          limits.value(it.key()).toObject().contains(QStringLiteral("rest"));
      if (!driven && !start.contains(it.key()) && it.value().isDouble()) {
        start.insert(it.key(), it.value());
      }
    }
  }
  state.setObject(kStart, start);
  state.setObject(kStartLoaded, {{QStringLiteral("loaded"), true}});
}

QJsonObject startOf(const CommandState& state) {
  const QJsonObject kept = state.object(kStart);
  const bool loaded = state.object(kStartLoaded).value(QStringLiteral("loaded")).toBool();
  const QJsonObject position = positionOf(state);
  QJsonObject start;
  for (const QString& motion : motionsIn(state)) {
    if (position.contains(motion) || state.checked(QStringLiteral("rest_on_") + motion)) {
      continue; // driven: held at its value
    }
    if (kept.contains(motion)) {
      start.insert(motion, kept.value(motion));
    } else if (!loaded) {
      start.insert(motion, 0.0);
    }
  }
  return start;
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
                  QObject::tr("A face, edge, point, axis or plane of the component that moves onto B"))
          .withGhosts(ghostUntilPicked(QStringLiteral("b"))),
      originInput(QStringLiteral("b"), QObject::tr("Origin B"),
                  QObject::tr("A face, edge, point, axis or plane of another component")),
  };
  def.inputs += kindInputs();
  def.inputs += {
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the joint's Z axis"))
          .withManipulator(jointOffsetHandle),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("About the joint's Z axis"))
          .withManipulator(jointAngleHandle),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Turns origin A over: a face onto a face")),
  };
  def.inputs += limitInputs();
  def.inputs += {hidden(QStringLiteral("component")), hidden(QStringLiteral("position"))};
  def.enabled = [&context] { return occurrenceCount(context) > 0; };
  // The handles stand on the joint's origin where the preview puts it.
  def.previewReported = keepPlacements;
  // An origin on the component the preview moves is drawn where it goes.
  def.pickMoved = pickMovedInPreview;

  def.build = [](const CommandState& state, const CommandContext& context) {
    QJsonObject joint{{QStringLiteral("type"), QStringLiteral("joint")}};
    Built result = originsOf(state, context, {QStringLiteral("a"), QStringLiteral("b")}, joint);
    if (!result.error.isEmpty()) {
      return result;
    }
    addMotions(joint, state);
    const QJsonObject start = startOf(state);
    if (!start.isEmpty()) {
      joint.insert(kStart, start);
    }
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
    loadStart(feature, state, jointsOf(context));
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
                     QObject::tr("A component in the browser, or a body it places"))
          .withGhosts(ghostUntilPicked(QStringLiteral("b"))),
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
// Joint Origin: a frame on geometry of the active component, optionally a
// mount frame of the part (mitcad#525; commands.md, "Mount frames").

namespace {

const Choices kMountKinds ={{QStringLiteral("face"), QStringLiteral("On a face")},
                             {QStringLiteral("shaft"), QStringLiteral("Into a hole (its shaft)")},
                             {QStringLiteral("hole"), QStringLiteral("Onto a shaft (its hole)")},
                             {QStringLiteral("grid"), QStringLiteral("Grid of cells")},
                             {QStringLiteral("foot"), QStringLiteral("Foot on a grid")}};

// How a part placed by a frame of a kind may move (the joint it gets).
Choices mountJoints(const QString& kind) {
  if (kind == QStringLiteral("grid") || kind == QStringLiteral("foot")) {
    return {{QStringLiteral("rigid"), QObject::tr("Fixed")}};
  }
  if (kind == QStringLiteral("hole") || kind == QStringLiteral("shaft")) {
    return {{QStringLiteral("rigid"), QObject::tr("Fixed")},
            {QStringLiteral("revolute"), QObject::tr("Turns")},
            {QStringLiteral("cylindrical"), QObject::tr("Turns and slides")},
            {QStringLiteral("slider"), QObject::tr("Slides")}};
  }
  return {{QStringLiteral("rigid"), QObject::tr("Fixed")},
          {QStringLiteral("planar"), QObject::tr("Free on the face")},
          {QStringLiteral("revolute"), QObject::tr("Turns")}};
}

bool mountOn(const CommandState& state) { return state.checked(QStringLiteral("mount")); }

bool mountKindIs(const CommandState& state, std::initializer_list<const char*> kinds) {
  const QString kind = state.choice(QStringLiteral("mount_kind"));
  return mountOn(state) && std::any_of(kinds.begin(), kinds.end(), [&kind](const char* k) {
           return kind == QLatin1String(k);
         });
}

// The mount's two counts (cells, a footprint): whole numbers of 1 or more.
std::optional<QJsonArray> countsOf(const CommandState& state, const QString& x, const QString& y) {
  QJsonArray counts;
  for (const QString& id : {x, y}) {
    const double value = state.value(id);
    if (!std::isfinite(value) || value < 1.0 || std::abs(value - std::round(value)) > 1e-9) {
      return std::nullopt;
    }
    counts.append(static_cast<int>(std::lround(value)));
  }
  return counts;
}

// The `mount` of a joint origin from the panel's inputs; an error when a
// value is not one.
std::optional<QJsonObject> mountOf(const CommandState& state, QString& error) {
  const QString name = state.text(QStringLiteral("mount_name")).trimmed();
  static const QRegularExpression valid(QStringLiteral("^[A-Za-z0-9_+.-]{1,64}$"));
  if (!valid.match(name).hasMatch()) {
    error = QObject::tr("A mount frame's name is letters, digits, _ - + and . (1 to 64).");
    return std::nullopt;
  }
  const QString kind = state.choice(QStringLiteral("mount_kind"));
  QJsonObject mount{{QStringLiteral("name"), name}, {QStringLiteral("kind"), kind}};
  const Choices joints = mountJoints(kind);
  QString joint = state.choice(QStringLiteral("mount_joint"));
  if (std::none_of(joints.begin(), joints.end(), [&joint](const auto& j) { return j.first == joint; })) {
    joint = joints.first().first;
  }
  mount.insert(QStringLiteral("joint"), joint);
  if (kind == QStringLiteral("grid") || kind == QStringLiteral("foot")) {
    const double px = state.value(QStringLiteral("pitch_x"));
    const double py = state.value(QStringLiteral("pitch_y"));
    if (!(std::isfinite(px) && px > 0.0 && std::isfinite(py) && py > 0.0)) {
      error = QObject::tr("The pitch is two lengths above 0.");
      return std::nullopt;
    }
    mount.insert(QStringLiteral("pitch"), QJsonArray{px, py});
    const bool grid = kind == QStringLiteral("grid");
    const auto counts = grid ? countsOf(state, QStringLiteral("cells_x"), QStringLiteral("cells_y"))
                             : countsOf(state, QStringLiteral("footprint_x"), QStringLiteral("footprint_y"));
    if (!counts) {
      error = grid ? QObject::tr("A grid has 1 or more cells each way.")
                   : QObject::tr("A foot covers 1 or more cells each way.");
      return std::nullopt;
    }
    mount.insert(grid ? QStringLiteral("cells") : QStringLiteral("footprint"), *counts);
  }
  return mount;
}

} // namespace

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
      // A mount frame of the part (mitcad#525): where it mounts when placed
      // from a library, and how it may move there.
      checkInput(QStringLiteral("mount"), QObject::tr("Mount Frame"))
          .withTooltip(QObject::tr("Where the component mounts when it is placed from a library: a face, a hole, a grid")),
      textInput(QStringLiteral("mount_name"), QObject::tr("Name"), QStringLiteral("base"))
          .withTooltip(QObject::tr("The frame's name in the library: letters, digits, _ - + ."))
          .withVisible(mountOn),
      choiceInput(QStringLiteral("mount_kind"), QObject::tr("Mounts"), kMountKinds, QStringLiteral("face"))
          .withTooltip(QObject::tr("What the frame mounts on: z points out of the component towards it"))
          .withVisible(mountOn),
      choiceInput(QStringLiteral("mount_joint"), QObject::tr("Moves"), mountJoints(QStringLiteral("face")),
                  QStringLiteral("rigid"))
          .withChoices([](const CommandState& state) { return mountJoints(state.choice(QStringLiteral("mount_kind"))); })
          .withTooltip(QObject::tr("How the placed component may move: the joint its placement makes"))
          .withVisible([](const CommandState& state) { return mountOn(state) && !mountKindIs(state, {"grid", "foot"}); }),
      valueInput(QStringLiteral("pitch_x"), QObject::tr("Pitch X"), ValueKind::Length, QStringLiteral("42 mm"))
          .withExactDefault()
          .withTooltip(QObject::tr("A cell's size along the frame's X"))
          .withVisible([](const CommandState& state) { return mountKindIs(state, {"grid", "foot"}); }),
      valueInput(QStringLiteral("pitch_y"), QObject::tr("Pitch Y"), ValueKind::Length, QStringLiteral("42 mm"))
          .withExactDefault()
          .withTooltip(QObject::tr("A cell's size along the frame's Y"))
          .withVisible([](const CommandState& state) { return mountKindIs(state, {"grid", "foot"}); }),
      valueInput(QStringLiteral("cells_x"), QObject::tr("Cells X"), ValueKind::Unitless, QStringLiteral("3"))
          .withTooltip(QObject::tr("The grid's cells along X, from the frame's origin"))
          .withVisible([](const CommandState& state) { return mountKindIs(state, {"grid"}); }),
      valueInput(QStringLiteral("cells_y"), QObject::tr("Cells Y"), ValueKind::Unitless, QStringLiteral("2"))
          .withTooltip(QObject::tr("The grid's cells along Y, from the frame's origin"))
          .withVisible([](const CommandState& state) { return mountKindIs(state, {"grid"}); }),
      valueInput(QStringLiteral("footprint_x"), QObject::tr("Footprint X"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The cells the foot covers along X; the frame in their middle"))
          .withVisible([](const CommandState& state) { return mountKindIs(state, {"foot"}); }),
      valueInput(QStringLiteral("footprint_y"), QObject::tr("Footprint Y"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The cells the foot covers along Y"))
          .withVisible([](const CommandState& state) { return mountKindIs(state, {"foot"}); }),
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
    if (mountOn(state)) {
      QString error;
      const std::optional<QJsonObject> mount = mountOf(state, error);
      if (!mount) {
        return Built::failure(error, QStringLiteral("mount_name"));
      }
      origin.insert(QStringLiteral("mount"), *mount);
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
    // A mount frame (mitcad#525).
    const QJsonObject mount = feature.value(QStringLiteral("mount")).toObject();
    state.setChecked(QStringLiteral("mount"), !mount.isEmpty());
    if (!mount.isEmpty()) {
      state.setText(QStringLiteral("mount_name"), str(mount, "name"));
      state.setChoice(QStringLiteral("mount_kind"), str(mount, "kind").isEmpty() ? QStringLiteral("face") : str(mount, "kind"));
      state.setChoice(QStringLiteral("mount_joint"), str(mount, "joint").isEmpty() ? QStringLiteral("rigid") : str(mount, "joint"));
      const auto pair = [&state, &context](const QJsonValue& value, const QString& x, const QString& y, bool length) {
        const QJsonArray numbers = value.toArray();
        if (numbers.size() != 2) {
          return;
        }
        const QString ids[2] = {x, y};
        for (int i = 0; i < 2; ++i) {
          const double v = numbers[i].toDouble();
          state.setText(ids[i], length ? lengthText(v, context) : number(v));
        }
      };
      pair(mount.value(QStringLiteral("pitch")), QStringLiteral("pitch_x"), QStringLiteral("pitch_y"), true);
      pair(mount.value(QStringLiteral("cells")), QStringLiteral("cells_x"), QStringLiteral("cells_y"), false);
      pair(mount.value(QStringLiteral("footprint")), QStringLiteral("footprint_x"), QStringLiteral("footprint_y"), false);
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
      return QStringLiteral("Added joint origin %1: %2").arg(str(made, "name"), mitcad::modelMessage(e.what()));
    }
    const auto xyz = [&datum](const char* key) {
      QStringList parts;
      for (const QJsonValue& c : datum.value(QLatin1String(key)).toArray()) {
        parts << number(std::abs(c.toDouble()) < 1e-9 ? 0.0 : std::round(c.toDouble() * 1e6) / 1e6);
      }
      return QStringLiteral("(%1)").arg(parts.join(QStringLiteral(", ")));
    };
    QString text =
        QStringLiteral("Added joint origin %1: origin %2, normal %3").arg(str(made, "name"), xyz("origin"), xyz("normal"));
    // A mount frame (mitcad#525): "mount foot (foot, rigid)".
    QJsonObject mount;
    try {
      mount = context.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), str(made, "uid")}})
                  .value(QStringLiteral("def"))
                  .toObject()
                  .value(QStringLiteral("mount"))
                  .toObject();
    } catch (const std::exception&) {
      // No mount then.
    }
    if (!mount.isEmpty()) {
      text += QStringLiteral(", mount %1 (%2, %3)")
                  .arg(str(mount, "name"), str(mount, "kind"),
                       str(mount, "joint").isEmpty() ? QStringLiteral("rigid") : str(mount, "joint"));
    }
    return text;
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

// ---------------------------------------------------------------------------
// Constraint (mitcad#148): rows of assembly constraints in one panel, each
// geometry of one component held against geometry of another; one
// `constraint_set` feature. A row's type is inferred from its picks (the
// model's `constraint_row` query) unless chosen.

namespace {

const QString kRows = QStringLiteral("rows");
const QString kAuto = QStringLiteral("auto");

const Choices kConstraintTypes = {{QStringLiteral("align"), QStringLiteral("Align")},
                                  {QStringLiteral("angle"), QStringLiteral("Angle")},
                                  {QStringLiteral("center"), QStringLiteral("Center")},
                                  {QStringLiteral("tangent"), QStringLiteral("Tangent")},
                                  {QStringLiteral("insert"), QStringLiteral("Insert")},
                                  {QStringLiteral("insert_axis"), QStringLiteral("Insert (Axis)")}};

QString typeLabel(const QString& type) {
  for (const auto& [value, label] : kConstraintTypes) {
    if (value == type) {
      return label;
    }
  }
  return type;
}

// A row's reference as the model takes it: a path from the component.
std::optional<QJsonObject> rowOrigin(const CommandState& row, const QString& input, const QStringList& paths) {
  const Selection& items = row.items(input);
  if (items.isEmpty()) {
    return std::nullopt;
  }
  const auto path = pathFrom(items.first(), paths);
  if (!path) {
    return std::nullopt;
  }
  return originOf(items.first(), *path);
}

// The `constraint_row` query of a row's picks; empty without both.
QJsonObject rowQuery(const CommandState& row, const CommandContext& context) {
  const QStringList paths = placingPaths(activeComponent(context), context);
  const auto a = rowOrigin(row, QStringLiteral("a"), paths);
  const auto b = rowOrigin(row, QStringLiteral("b"), paths);
  if (!a || !b) {
    return QJsonObject();
  }
  try {
    return context.queryObject(
        {{QStringLiteral("query"), QStringLiteral("constraint_row")}, {QStringLiteral("a"), *a}, {QStringLiteral("b"), *b}});
  } catch (const std::exception&) {
    return QJsonObject();
  }
}

// The type the row's picks fit best, remembered per pair of picks (the
// kinds of geometry do not change while the panel is open).
QString inferredType(const CommandState& row, const CommandContext& context) {
  static QHash<QString, QString> known;
  const Selection& a = row.items(QStringLiteral("a"));
  const Selection& b = row.items(QStringLiteral("b"));
  if (a.isEmpty() || b.isEmpty()) {
    return QString();
  }
  const QString key = a.first().occurrence + QLatin1Char('|') +
                      QString::fromUtf8(QJsonDocument(QJsonObject{{QStringLiteral("a"), a.first().reference()},
                                                                  {QStringLiteral("b"), b.first().reference()}})
                                            .toJson(QJsonDocument::Compact)) +
                      QLatin1Char('|') + b.first().occurrence;
  const auto found = known.constFind(key);
  if (found != known.constEnd()) {
    return *found;
  }
  const QString type = str(rowQuery(row, context), "type");
  if (known.size() > 256) {
    known.clear();
  }
  known.insert(key, type);
  return type;
}

// The row's type: the chosen one, else the inferred one.
QString rowType(const CommandState& row, const CommandContext& context) {
  const QString chosen = row.choice(QStringLiteral("type"));
  return chosen.isEmpty() || chosen == kAuto ? inferredType(row, context) : chosen;
}

std::function<bool(const CommandState&)> typeIn(const CommandContext& context, const QStringList& types) {
  return [&context, types](const CommandState& row) { return types.contains(rowType(row, context)); };
}

bool limited(const CommandState& row) { return row.checked(QStringLiteral("limits")); }

// "Constraint1: insert placed, moved O2; DOF 1", as the `joints` query has
// the set (a failure: its error).
QString constraintLine(const QJsonObject& set, const QJsonObject& joints) {
  QStringList rows;
  for (const QJsonValue& value : set.value(QStringLiteral("rows")).toArray()) {
    const QJsonObject row = value.toObject();
    QString text = str(row, "type");
    if (row.contains(QStringLiteral("holds")) && !row.value(QStringLiteral("holds")).toBool()) {
      text += QStringLiteral(" (does not hold)");
    }
    rows << text;
  }
  QString line = QStringLiteral("%1: %2 %3").arg(str(set, "name"), rows.join(QStringLiteral(", ")), str(set, "state"));
  const QString error = str(set, "error");
  if (!error.isEmpty()) {
    line += QStringLiteral(": ") + error;
  }
  QStringList moved;
  for (const QJsonValue& uid : set.value(QStringLiteral("moved")).toArray()) {
    moved << uid.toString();
  }
  if (!moved.isEmpty()) {
    line += QStringLiteral(", moved ") + moved.join(QStringLiteral(" "));
  }
  for (const QJsonValue& value : joints.value(QStringLiteral("dof")).toArray()) {
    const QJsonObject dof = value.toObject();
    if (str(dof, "component") == str(set, "component")) {
      line += QStringLiteral("; DOF %1").arg(dof.value(QStringLiteral("total")).toInt());
    }
  }
  return line;
}

} // namespace

CommandDef constraintCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.constraint");
  def.name = QObject::tr("Assembly Constraint");
  def.icon = QStringLiteral("constraint");
  def.tooltip = QObject::tr("Holds components against each other: align, angle, center, tangent, insert");
  def.group = QStringLiteral("ASSEMBLE");
  def.pinned = true;
  def.keywords = {QStringLiteral("assembly"), QStringLiteral("mate"),    QStringLiteral("align"),
                  QStringLiteral("insert"),   QStringLiteral("tangent"), QStringLiteral("concentric"),
                  QStringLiteral("flush"),    QStringLiteral("angle")};
  def.featureType = QStringLiteral("constraint_set");
  const auto lengthLimits = [&context](const CommandState& row) {
    return limited(row) && rowType(row, context) == QStringLiteral("align");
  };
  const auto angleLimits = [&context](const CommandState& row) {
    return limited(row) && rowType(row, context) == QStringLiteral("angle");
  };
  const auto resting = [&context](const CommandState& row) {
    return limited(row) && row.checked(QStringLiteral("rest_on")) &&
           QStringList{QStringLiteral("align"), QStringLiteral("angle")}.contains(rowType(row, context));
  };
  def.inputs = {
      listInput(
          kRows, QObject::tr("Constraints"),
          {originInput(QStringLiteral("a"), QObject::tr("Geometry 1"),
                       QObject::tr("A face, edge, point, axis or plane of the component that moves"))
               .withGhosts(ghostUntilPicked(QStringLiteral("b"))),
           originInput(QStringLiteral("b"), QObject::tr("Geometry 2"),
                       QObject::tr("Geometry of another component that geometry 1 is held against")),
           choiceInput(QStringLiteral("type"), QObject::tr("Type"), {}, kAuto)
               .withTooltip(QObject::tr("What the row holds; Automatic follows the picks"))
               .withChoices([&context](const CommandState& row) {
                 Choices choices;
                 const QString inferred = inferredType(row, context);
                 choices.append({kAuto, inferred.isEmpty() ? QObject::tr("Automatic")
                                                           : QObject::tr("Automatic (%1)").arg(typeLabel(inferred))});
                 choices += kConstraintTypes;
                 return choices;
               }),
           selectionInput(QStringLiteral("b2"), QObject::tr("Second Face"), SelectKind::Face, 0, 1)
               .withTooltip(QObject::tr("Geometry 1 is centered between geometry 2 and this face"))
               .withVisible(typeIn(context, {QStringLiteral("center")})),
           valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
               .withTooltip(QObject::tr("Along geometry 2's normal or axis"))
               .withVisible([&context](const CommandState& row) {
                 const QString type = rowType(row, context);
                 return type == QStringLiteral("insert") || (type == QStringLiteral("align") && !limited(row));
               }),
           valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
               .withTooltip(QObject::tr("Between the normals or axes, 0 to 180 degrees"))
               .withVisible([&context](const CommandState& row) {
                 return rowType(row, context) == QStringLiteral("angle") && !limited(row);
               }),
           flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
               .withTooltip(QObject::tr("Planes and circles the same way instead of facing; a tangent inside"))
               .withVisible(typeIn(context, {QStringLiteral("align"), QStringLiteral("angle"),
                                             QStringLiteral("tangent"), QStringLiteral("insert")})),
           checkInput(QStringLiteral("lock_rotation"), QObject::tr("Lock Rotation"), false)
               .withTooltip(QObject::tr("Holds the turn about the axis where it is now"))
               .withVisible(typeIn(context, {QStringLiteral("insert"), QStringLiteral("insert_axis")})),
           checkInput(QStringLiteral("limits"), QObject::tr("Limits"), false)
               .withTooltip(QObject::tr("The offset or angle stays between a minimum and a maximum"))
               .withVisible(typeIn(context, {QStringLiteral("align"), QStringLiteral("angle")})),
           valueInput(QStringLiteral("min"), QObject::tr("Minimum"), ValueKind::Length, QStringLiteral("0 mm"))
               .withVisible(lengthLimits),
           valueInput(QStringLiteral("max"), QObject::tr("Maximum"), ValueKind::Length, QStringLiteral("10 mm"))
               .withVisible(lengthLimits),
           valueInput(QStringLiteral("min_angle"), QObject::tr("Minimum"), ValueKind::Angle, QStringLiteral("0 deg"))
               .withVisible(angleLimits),
           valueInput(QStringLiteral("max_angle"), QObject::tr("Maximum"), ValueKind::Angle, QStringLiteral("90 deg"))
               .withVisible(angleLimits),
           checkInput(QStringLiteral("rest_on"), QObject::tr("Rest"), false)
               .withTooltip(QObject::tr("The value it is held at unless dragged"))
               .withVisible([&context](const CommandState& row) {
                 return limited(row) &&
                        QStringList{QStringLiteral("align"), QStringLiteral("angle")}.contains(rowType(row, context));
               }),
           valueInput(QStringLiteral("rest"), QObject::tr("Rest Value"), ValueKind::Length, QStringLiteral("0 mm"))
               .withVisible([resting, &context](const CommandState& row) {
                 return resting(row) && rowType(row, context) == QStringLiteral("align");
               }),
           valueInput(QStringLiteral("rest_angle"), QObject::tr("Rest Value"), ValueKind::Angle, QStringLiteral("0 deg"))
               .withVisible([resting, &context](const CommandState& row) {
                 return resting(row) && rowType(row, context) == QStringLiteral("angle");
               }),
           // A locked insert's turn as loaded: an edit keeps it.
           hidden(QStringLiteral("lock_angle"))},
          QObject::tr("Add Constraint"), QObject::tr("Constraint %1")),
      hidden(QStringLiteral("component")),
  };
  def.enabled = [&context] { return occurrenceCount(context) > 1; };

  def.build = [](const CommandState& state, const CommandContext& context) {
    const QString component = jointComponent(state, context);
    const QStringList paths = placingPaths(component, context);
    QJsonArray rows;
    for (int i = 0; i < state.rows(kRows); ++i) {
      const CommandState row = state.row(kRows, i);
      const auto key = [i](const char* id) { return CommandState::rowKey(kRows, i, QString::fromLatin1(id)); };
      QJsonObject out;
      for (const char* id : {"a", "b"}) {
        const auto origin = rowOrigin(row, QString::fromLatin1(id), paths);
        if (!origin) {
          return Built::failure(QObject::tr("Select geometry inside %1, the component the constraint goes into.")
                                    .arg(componentName(component, context)),
                                key(id));
        }
        out.insert(QString::fromLatin1(id), *origin);
      }
      const QString type = rowType(row, context);
      if (type.isEmpty()) {
        return Built::failure(QObject::tr("No constraint holds this geometry: choose another."), key("b"));
      }
      out.insert(QStringLiteral("type"), type);
      if (type == QStringLiteral("center")) {
        if (const auto b2 = rowOrigin(row, QStringLiteral("b2"), paths)) {
          out.insert(QStringLiteral("b2"), *b2);
        }
      }
      const bool withLimits = row.checked(QStringLiteral("limits")) &&
                              (type == QStringLiteral("align") || type == QStringLiteral("angle"));
      const bool isAngle = type == QStringLiteral("angle");
      if (withLimits) {
        QJsonObject limits{
            {QStringLiteral("min"), row.expression(isAngle ? QStringLiteral("min_angle") : QStringLiteral("min"))},
            {QStringLiteral("max"), row.expression(isAngle ? QStringLiteral("max_angle") : QStringLiteral("max"))}};
        if (row.checked(QStringLiteral("rest_on"))) {
          limits.insert(QStringLiteral("rest"),
                        row.expression(isAngle ? QStringLiteral("rest_angle") : QStringLiteral("rest")));
        }
        out.insert(QStringLiteral("limits"), limits);
      } else if ((type == QStringLiteral("align") || type == QStringLiteral("insert")) &&
                 nonZero(row, QStringLiteral("offset"))) {
        out.insert(QStringLiteral("offset"), row.expression(QStringLiteral("offset")));
      } else if (isAngle) {
        out.insert(QStringLiteral("angle"), row.expression(QStringLiteral("angle")));
      }
      if (row.checked(QStringLiteral("flip")) &&
          QStringList{QStringLiteral("align"), QStringLiteral("angle"), QStringLiteral("tangent"), QStringLiteral("insert")}
              .contains(type)) {
        out.insert(QStringLiteral("flip"), true);
      }
      if (row.checked(QStringLiteral("lock_rotation")) && type.startsWith(QStringLiteral("insert"))) {
        out.insert(QStringLiteral("lock_rotation"), true);
        // The turn where it is now, unless an edit keeps the recorded one.
        QString angle = row.text(QStringLiteral("lock_angle"));
        if (angle.isEmpty()) {
          const QJsonValue twist =
              rowQuery(row, context).value(QStringLiteral("measures")).toObject().value(QStringLiteral("twist"));
          angle = degrees(twist.toDouble()) + QStringLiteral(" deg");
        }
        out.insert(QStringLiteral("angle"), angle);
      }
      rows.append(out);
    }
    return Built::of({{QStringLiteral("type"), QStringLiteral("constraint_set")}, {kRows, rows}});
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString component = featureComponent(str(feature, "uid"), context);
    state.setText(QStringLiteral("component"), component);
    const QString prefix = placingPaths(component, context).value(0);
    const QJsonArray rows = feature.value(kRows).toArray();
    state.setRows(kRows, static_cast<int>(rows.size()));
    for (int i = 0; i < rows.size(); ++i) {
      const QJsonObject row = rows[i].toObject();
      const auto key = [i](const char* id) { return CommandState::rowKey(kRows, i, QString::fromLatin1(id)); };
      for (const char* id : {"a", "b", "b2"}) {
        if (row.contains(QLatin1String(id))) {
          state.setItems(key(id), {originItem(row.value(QLatin1String(id)), prefix, context)});
        }
      }
      const QString type = str(row, "type");
      state.setChoice(key("type"), type);
      const bool isAngle = type == QStringLiteral("angle");
      const QJsonObject limits = row.value(QStringLiteral("limits")).toObject();
      if (limits.contains(QStringLiteral("min")) || limits.contains(QStringLiteral("max"))) {
        state.setChecked(key("limits"), true);
        loadValue(state, key(isAngle ? "min_angle" : "min"), limits.value(QStringLiteral("min")), feature, context);
        loadValue(state, key(isAngle ? "max_angle" : "max"), limits.value(QStringLiteral("max")), feature, context);
        if (limits.contains(QStringLiteral("rest"))) {
          state.setChecked(key("rest_on"), true);
          loadValue(state, key(isAngle ? "rest_angle" : "rest"), limits.value(QStringLiteral("rest")), feature, context);
        }
      }
      loadValue(state, key("offset"), row.value(QStringLiteral("offset")), feature, context);
      state.setChecked(key("flip"), row.value(QStringLiteral("flip")).toBool());
      const bool locked = row.value(QStringLiteral("lock_rotation")).toBool();
      state.setChecked(key("lock_rotation"), locked);
      if (locked) {
        state.setText(key("lock_angle"), context.valueText(row.value(QStringLiteral("angle")), str(feature, "uid")));
      } else if (isAngle) {
        loadValue(state, key("angle"), row.value(QStringLiteral("angle")), feature, context);
      }
    }
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonArray rows = feature.value(kRows).toArray();
    return std::all_of(rows.begin(), rows.end(), [&context](const QJsonValue& value) {
      const QJsonObject row = value.toObject();
      const QJsonObject limits = row.value(QStringLiteral("limits")).toObject();
      return !row.contains(QStringLiteral("a2")) &&
             limits.contains(QStringLiteral("min")) == limits.contains(QStringLiteral("max")) &&
             (!limits.contains(QStringLiteral("rest")) || limits.contains(QStringLiteral("min"))) &&
             originItem(row.value(QStringLiteral("a")), QString(), context).isValid() &&
             originItem(row.value(QStringLiteral("b")), QString(), context).isValid() &&
             (!row.contains(QStringLiteral("b2")) ||
              originItem(row.value(QStringLiteral("b2")), QString(), context).isValid());
    });
  };

  def.describe = [](const CommandState&, const CommandContext& context) {
    const QJsonObject joints = jointsOf(context);
    const QJsonObject set = newest(joints.value(QStringLiteral("constraints")).toArray(),
                                   [](const QJsonObject&) { return true; });
    if (set.isEmpty()) {
      return QStringLiteral("Added constraint");
    }
    return QStringLiteral("Added constraint %1").arg(constraintLine(set, joints));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Contact Set (mitcad#454): components whose bodies may touch but not pass
// through each other while joints move. Not a feature: OK runs
// `add_contact_set` (or `edit_contact_set` for the one edited, the hidden
// input "set"), one undo step that recomputes nothing.

CommandDef contactSetCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.contact_set");
  def.name = QObject::tr("Contact Set");
  def.icon = QStringLiteral("contact-set");
  def.tooltip = QObject::tr("Components that stop each other where they touch while joints move");
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("contact"), QStringLiteral("collision"), QStringLiteral("stop"),
                  QStringLiteral("interference")};
  def.modelCommand = true;
  def.inputs = {
      selectionInput(QStringLiteral("occurrences"), QObject::tr("Components"),
                     SelectKind::Component | SelectKind::Body, 2, 0)
          .withTooltip(QObject::tr("Components in the browser, or bodies they place"))
          .withAccepts(isPlaced)
          .withConvert(occurrenceItem),
      checkInput(QStringLiteral("suppressed"), QObject::tr("Suppressed"), false)
          .withTooltip(QObject::tr("A suppressed set stops nothing")),
      hidden(QStringLiteral("set")),
  };
  def.enabled = [&context] { return occurrenceCount(context) > 1; };
  def.build = [](const CommandState& state, const CommandContext&) {
    QJsonArray occurrences;
    for (const SelectionItem& item : state.items(QStringLiteral("occurrences"))) {
      if (!item.occurrence.isEmpty() && !occurrences.contains(item.occurrence)) {
        occurrences.append(item.occurrence);
      }
    }
    if (occurrences.size() < 2) {
      return Built::failure(QObject::tr("Select two or more components."), QStringLiteral("occurrences"));
    }
    const QString set = state.text(QStringLiteral("set"));
    const bool suppressed = state.checked(QStringLiteral("suppressed"));
    QJsonObject command = set.isEmpty()
                              ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("add_contact_set")}}
                              : QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_contact_set")},
                                            {QStringLiteral("name"), set}};
    command.insert(QStringLiteral("occurrences"), occurrences);
    command.insert(QStringLiteral("suppressed"), suppressed);
    Built built;
    built.def = {{QStringLiteral("commands"), QJsonArray{command}},
                 {QStringLiteral("label"), set.isEmpty() ? QStringLiteral("Add Contact Set")
                                                         : QStringLiteral("Edit %1").arg(set)}};
    return built;
  };
  // An entry of the `contact_sets` query, to edit.
  def.load = [](const QJsonObject& set, CommandState& state, const CommandContext&) {
    state.setText(QStringLiteral("set"), str(set, "name"));
    state.setChecked(QStringLiteral("suppressed"), set.value(QStringLiteral("suppressed")).toBool());
    Selection items;
    for (const QJsonValue& value : set.value(QStringLiteral("occurrences")).toArray()) {
      SelectionItem item{SelectKind::Component, QString(), QString(), QString()};
      item.occurrence = str(value.toObject(), "path");
      items.append(item);
    }
    state.setItems(QStringLiteral("occurrences"), items);
  };
  def.describe = [](const CommandState& state, const CommandContext& context) {
    QJsonArray sets;
    try {
      sets = context.queryArray(QStringLiteral("contact_sets"));
    } catch (const std::exception&) {
    }
    const QString edited = state.text(QStringLiteral("set"));
    if (!edited.isEmpty()) {
      return QStringLiteral("Edited contact set %1").arg(edited);
    }
    const QJsonObject set = sets.isEmpty() ? QJsonObject() : sets.last().toObject();
    QStringList names;
    for (const QJsonValue& value : set.value(QStringLiteral("occurrences")).toArray()) {
      names << str(value.toObject(), "name");
    }
    return QStringLiteral("Added contact set %1: %2").arg(str(set, "name"), names.join(QStringLiteral(", ")));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Motion Link (mitcad#466): one joint's free motion follows another's (the
// driver, A): gears, a rack and pinion, a screw, a plain ratio, or a cam
// with its follower; one `motion_link` feature. A size is a value, or the
// radius of round geometry picked beside it. "Keep where they are" adds the
// offset that leaves the joints where they are (the `motion_link_offset`
// query); a cam's follower otherwise moves to touch the cam.

namespace {

const Choices kCouplings = {{QStringLiteral("gear"), QStringLiteral("Gears")},
                            {QStringLiteral("rack_pinion"), QStringLiteral("Rack and Pinion")},
                            {QStringLiteral("screw"), QStringLiteral("Screw")},
                            {QStringLiteral("ratio"), QStringLiteral("Ratio")},
                            {QStringLiteral("cam"), QStringLiteral("Cam")}};
const Choices kFollowers = {{QStringLiteral("knife"), QStringLiteral("Knife Edge")},
                            {QStringLiteral("roller"), QStringLiteral("Roller")},
                            {QStringLiteral("flat"), QStringLiteral("Flat Face")}};

std::function<bool(const CommandState&)> couplingIs(const QStringList& types) {
  return [types](const CommandState& state) { return types.contains(state.choice(QStringLiteral("coupling"))); };
}

QString linkedJoint(const CommandState& state, const QString& side) {
  const Selection& items = state.items(side);
  return items.isEmpty() ? QString() : items.first().owner;
}

// The chosen motion of a side's joint (its first when the choice is not one
// of its motions).
QString linkedMotion(const CommandState& state, const QString& side, const CommandContext& context) {
  const QStringList motions = jointMotions(jointEntry(linkedJoint(state, side), context));
  const QString chosen = state.choice(side + QStringLiteral("_motion"));
  return motions.contains(chosen) ? chosen : motions.value(0);
}

InputDef motionChoice(const QString& side, const QString& label, const CommandContext& context) {
  return choiceInput(side + QStringLiteral("_motion"), label, {}, QString())
      .withChoices([&context, side](const CommandState& state) {
        Choices choices;
        for (const QString& motion : jointMotions(jointEntry(linkedJoint(state, side), context))) {
          choices.append({motion, motionLabel(motion)});
        }
        return choices;
      });
}

// Round geometry for a size: a cylindrical, conical or spherical face, a
// circular edge, a sketch circle or arc.
bool isRound(const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Face:
    return item.geometry == QStringLiteral("cylinder") || item.geometry == QStringLiteral("cone") ||
           item.geometry == QStringLiteral("sphere");
  case SelectKind::Edge:
  case SelectKind::SketchCurve:
    return item.geometry == QStringLiteral("circle") || item.geometry == QStringLiteral("arc");
  default:
    return false;
  }
}

// A size's inputs: its value, and round geometry that gives it instead.
QVector<InputDef> sizeInputs(const QString& id, const QString& label, const QString& defaultText,
                             const QString& tip, std::function<bool(const CommandState&)> shown) {
  InputDef round = selectionInput(id + QStringLiteral("_round"), QObject::tr("%1 From").arg(label),
                                  SelectKind::Face | SelectKind::Edge | SelectKind::SketchCurve, 0, 1)
                       .withTooltip(QObject::tr("A round face, circular edge or circle whose radius it is"))
                       .withAccepts(isRound)
                       .withVisible(shown);
  return {valueInput(id, label, ValueKind::Length, defaultText)
              .withTooltip(tip)
              .withVisible([shown, id](const CommandState& state) {
                return shown(state) && state.items(id + QStringLiteral("_round")).isEmpty();
              }),
          round};
}

// A size of the definition: its round geometry's origin, else its value.
std::optional<QJsonValue> sizeOf(const CommandState& state, const QString& id, const QStringList& paths) {
  const Selection& round = state.items(id + QStringLiteral("_round"));
  if (round.isEmpty()) {
    return QJsonValue(state.expression(id));
  }
  const auto path = pathFrom(round.first(), paths);
  if (!path) {
    return std::nullopt;
  }
  return QJsonValue(originOf(round.first(), *path));
}

void loadSize(CommandState& state, const QString& id, const QJsonValue& size, const QJsonObject& feature,
              const QString& prefix, const CommandContext& context) {
  if (size.isObject()) {
    state.setItems(id + QStringLiteral("_round"), {originItem(size, prefix, context)});
  } else {
    loadValue(state, id, size, feature, context);
  }
}

QJsonObject linkOf(const QJsonObject& joints, const std::function<bool(const QJsonObject&)>& fits) {
  return newest(joints.value(QStringLiteral("links")).toArray(), fits);
}

// "MotionLink1: gear, Joint1 rz to Joint2 rz, placed".
QString linkLine(const QJsonObject& link) {
  const auto side = [&link](const char* key) {
    const QJsonObject s = link.value(QLatin1String(key)).toObject();
    return QStringLiteral("%1 %2").arg(str(s, "name"), str(s, "motion"));
  };
  QString line = QStringLiteral("%1: %2, %3 to %4, %5")
                     .arg(str(link, "name"), str(link, "coupling"), side("a"), side("b"), str(link, "state"));
  const QString error = str(link, "error");
  if (!error.isEmpty()) {
    line += QStringLiteral(": ") + error;
  }
  return line;
}

} // namespace

CommandDef motionLinkCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.motion_link");
  def.name = QObject::tr("Motion Link");
  def.icon = QStringLiteral("motion-link");
  def.tooltip = QObject::tr("Makes one joint's motion follow another's: gears, rack and pinion, screw, cam");
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("gear"),  QStringLiteral("rack"),     QStringLiteral("pinion"),
                  QStringLiteral("screw"), QStringLiteral("cam"),      QStringLiteral("coupling"),
                  QStringLiteral("ratio"), QStringLiteral("follower"), QStringLiteral("motion")};
  def.featureType = QStringLiteral("motion_link");
  const auto jointPick = [&context](const QString& id, const QString& label, const QString& tip) {
    return selectionInput(id, label, SelectKind::Feature, 1, 1)
        .withTooltip(tip)
        .withAccepts([&context](const SelectionItem& item) { return drivable(item, context); });
  };
  const auto cam = couplingIs({QStringLiteral("cam")});
  const auto roller = [cam](const CommandState& state) {
    return cam(state) && state.choice(QStringLiteral("follower")) == QStringLiteral("roller");
  };
  const auto followerTurns = [&context](const CommandState& state) {
    return isTurn(linkedMotion(state, QStringLiteral("b"), context));
  };
  // A cam's follower touches the cam: its offset is only what is given.
  const auto keep = [cam](const CommandState& state) { return !cam(state) && state.checked(QStringLiteral("keep")); };
  InputDef profile = selectionInput(QStringLiteral("profile"), QObject::tr("Cam Profile"),
                                    SelectKind::Face | SelectKind::SketchCurve, 0, 1)
                         .withTooltip(QObject::tr("A flat face of the cam, or a curve of the sketch profile it is cut from"))
                         .withVisible(cam);
  InputDef tip = selectionInput(QStringLiteral("tip"), QObject::tr("Follower Tip"), kOriginKinds, 0, 1)
                     .withTooltip(QObject::tr("The knife's point or the roller's center on the follower"))
                     .withVisible(cam);
  def.inputs = {
      choiceInput(QStringLiteral("coupling"), QObject::tr("Type"), kCouplings, QStringLiteral("gear"))
          .withTooltip(QObject::tr("How the follower follows the driver")),
      jointPick(QStringLiteral("a"), QObject::tr("Driver"), QObject::tr("The joint that drives (a cam turns on a revolute joint)")),
      motionChoice(QStringLiteral("a"), QObject::tr("Driver Motion"), context),
      jointPick(QStringLiteral("b"), QObject::tr("Follower"), QObject::tr("The joint that follows")),
      motionChoice(QStringLiteral("b"), QObject::tr("Follower Motion"), context),
  };
  def.inputs += sizeInputs(QStringLiteral("ra"), QObject::tr("Driver Gear"), QStringLiteral("10 mm"),
                           QObject::tr("Its pitch radius, or any size in proportion (its tooth count)"),
                           couplingIs({QStringLiteral("gear")}));
  def.inputs += sizeInputs(QStringLiteral("rb"), QObject::tr("Follower Gear"), QStringLiteral("20 mm"),
                           QObject::tr("Its pitch radius, or any size in proportion (its tooth count)"),
                           couplingIs({QStringLiteral("gear")}));
  def.inputs += {checkInput(QStringLiteral("internal"), QObject::tr("Internal Gear"), false)
                     .withTooltip(QObject::tr("A ring gear: both turn the same way"))
                     .withVisible(couplingIs({QStringLiteral("gear")}))};
  def.inputs += sizeInputs(QStringLiteral("radius"), QObject::tr("Pinion Radius"), QStringLiteral("10 mm"),
                           QObject::tr("The rack travels this far per radian of the pinion"),
                           couplingIs({QStringLiteral("rack_pinion")}));
  def.inputs += {
      valueInput(QStringLiteral("lead"), QObject::tr("Lead"), ValueKind::Length, QStringLiteral("1 mm"))
          .withTooltip(QObject::tr("The travel per turn"))
          .withVisible(couplingIs({QStringLiteral("screw")})),
      valueInput(QStringLiteral("ratio"), QObject::tr("Ratio"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The follower's value per the driver's (mm or radians each)"))
          .withVisible(couplingIs({QStringLiteral("ratio")})),
      profile,
      choiceInput(QStringLiteral("follower"), QObject::tr("Follower"), kFollowers, QStringLiteral("knife"))
          .withVisible(cam),
  };
  def.inputs += sizeInputs(QStringLiteral("roller"), QObject::tr("Roller Radius"), QStringLiteral("2 mm"),
                           QObject::tr("The roller's radius"), roller);
  def.inputs += {
      tip,
      checkInput(QStringLiteral("reverse"), QObject::tr("Reverse"), false)
          .withTooltip(QObject::tr("The follower the other way (a cam's: from its other side)")),
      checkInput(QStringLiteral("keep"), QObject::tr("Keep Where They Are"), true)
          .withTooltip(QObject::tr("The offset that leaves the joints where they are now"))
          .withVisible([cam](const CommandState& state) { return !cam(state); }),
      valueInput(QStringLiteral("offset_angle"), QObject::tr("Offset"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("The follower's value when the driver is at zero"))
          .withVisible([keep, followerTurns](const CommandState& state) { return !keep(state) && followerTurns(state); }),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("The follower's value when the driver is at zero"))
          .withVisible([keep, followerTurns](const CommandState& state) { return !keep(state) && !followerTurns(state); }),
      hidden(QStringLiteral("component")),
  };
  def.enabled = [&context] {
    const QJsonArray joints = jointsOf(context).value(QStringLiteral("joints")).toArray();
    return std::count_if(joints.begin(), joints.end(), [](const QJsonValue& joint) {
             return !jointMotions(joint.toObject()).isEmpty();
           }) > 0;
  };

  def.build = [](const CommandState& state, const CommandContext& context) {
    const QString component = jointComponent(state, context);
    const QStringList paths = placingPaths(component, context);
    QJsonObject link{{QStringLiteral("type"), QStringLiteral("motion_link")}};
    for (const QString& side : {QStringLiteral("a"), QStringLiteral("b")}) {
      const QString joint = linkedJoint(state, side);
      const QString motion = linkedMotion(state, side, context);
      if (joint.isEmpty() || motion.isEmpty()) {
        return Built::failure(side == QStringLiteral("a") ? QObject::tr("Select the driving joint.")
                                                          : QObject::tr("Select the following joint."),
                              side);
      }
      link.insert(side, QJsonObject{{QStringLiteral("joint"), joint}, {QStringLiteral("motion"), motion}});
    }
    const QString type = state.choice(QStringLiteral("coupling"));
    QJsonObject coupling{{QStringLiteral("type"), type}};
    const auto size = [&](const QString& id, const QString& key) -> bool {
      const auto value = sizeOf(state, id, paths);
      if (value) {
        coupling.insert(key, *value);
      }
      return value.has_value();
    };
    const Built outside = Built::failure(
        QObject::tr("Select geometry inside %1, the component the link goes into.").arg(componentName(component, context)),
        QString());
    if (type == QStringLiteral("gear")) {
      if (!size(QStringLiteral("ra"), QStringLiteral("ra")) || !size(QStringLiteral("rb"), QStringLiteral("rb"))) {
        return outside;
      }
      if (state.checked(QStringLiteral("internal"))) {
        coupling.insert(QStringLiteral("internal"), true);
      }
    } else if (type == QStringLiteral("rack_pinion")) {
      if (!size(QStringLiteral("radius"), QStringLiteral("radius"))) {
        return outside;
      }
    } else if (type == QStringLiteral("screw")) {
      coupling.insert(QStringLiteral("lead"), state.expression(QStringLiteral("lead")));
    } else if (type == QStringLiteral("ratio")) {
      coupling.insert(QStringLiteral("ratio"), state.expression(QStringLiteral("ratio")));
    } else {
      const Selection& profile = state.items(QStringLiteral("profile"));
      if (profile.isEmpty()) {
        return Built::failure(QObject::tr("Select the cam's profile."), QStringLiteral("profile"));
      }
      const auto profilePath = pathFrom(profile.first(), paths);
      if (!profilePath) {
        return outside;
      }
      coupling.insert(QStringLiteral("profile"), originOf(profile.first(), *profilePath));
      const QString follower = state.choice(QStringLiteral("follower"));
      QJsonObject followerDef{{QStringLiteral("type"), follower}};
      if (follower == QStringLiteral("roller")) {
        const auto radius = sizeOf(state, QStringLiteral("roller"), paths);
        if (!radius) {
          return outside;
        }
        followerDef.insert(QStringLiteral("radius"), *radius);
      }
      coupling.insert(QStringLiteral("follower"), followerDef);
      const Selection& tip = state.items(QStringLiteral("tip"));
      if (!tip.isEmpty()) {
        const auto tipPath = pathFrom(tip.first(), paths);
        if (!tipPath) {
          return outside;
        }
        coupling.insert(QStringLiteral("tip"), originOf(tip.first(), *tipPath));
      }
    }
    link.insert(QStringLiteral("coupling"), coupling);
    const bool reverse = state.checked(QStringLiteral("reverse"));
    if (reverse) {
      link.insert(QStringLiteral("reverse"), true);
    }
    const QString bMotion = link.value(QStringLiteral("b")).toObject().value(QStringLiteral("motion")).toString();
    if (type != QStringLiteral("cam") && state.checked(QStringLiteral("keep"))) {
      // The offset that keeps the joints where they are.
      try {
        const QJsonObject found = context.queryObject(
            {{QStringLiteral("query"), QStringLiteral("motion_link_offset")},
             {QStringLiteral("a"), link.value(QStringLiteral("a"))},
             {QStringLiteral("b"), link.value(QStringLiteral("b"))},
             {QStringLiteral("coupling"), coupling},
             {QStringLiteral("reverse"), reverse},
             {QStringLiteral("component"), component}});
        const double offset = found.value(QStringLiteral("offset")).toDouble();
        if (std::abs(offset) > 1e-12) {
          link.insert(QStringLiteral("offset"), valueText(bMotion, offset, context));
        }
      } catch (const std::exception& e) {
        return Built::failure(mitcad::modelMessage(e.what()), QStringLiteral("b"));
      }
    } else {
      const QString input = isTurn(bMotion) ? QStringLiteral("offset_angle") : QStringLiteral("offset");
      if (nonZero(state, input)) {
        link.insert(QStringLiteral("offset"), state.expression(input));
      }
    }
    return Built::of(link);
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString component = featureComponent(str(feature, "uid"), context);
    state.setText(QStringLiteral("component"), component);
    const QString prefix = placingPaths(component, context).value(0);
    for (const QString& side : {QStringLiteral("a"), QStringLiteral("b")}) {
      const QJsonObject s = feature.value(side).toObject();
      state.setItems(side, {{SelectKind::Feature, str(s, "joint"), QString(), QString()}});
      state.setChoice(side + QStringLiteral("_motion"), str(s, "motion"));
    }
    const QJsonObject coupling = feature.value(QStringLiteral("coupling")).toObject();
    const QString type = str(coupling, "type");
    state.setChoice(QStringLiteral("coupling"), type);
    if (type == QStringLiteral("gear")) {
      loadSize(state, QStringLiteral("ra"), coupling.value(QStringLiteral("ra")), feature, prefix, context);
      loadSize(state, QStringLiteral("rb"), coupling.value(QStringLiteral("rb")), feature, prefix, context);
      state.setChecked(QStringLiteral("internal"), coupling.value(QStringLiteral("internal")).toBool());
    } else if (type == QStringLiteral("rack_pinion")) {
      loadSize(state, QStringLiteral("radius"), coupling.value(QStringLiteral("radius")), feature, prefix, context);
    } else if (type == QStringLiteral("screw")) {
      loadValue(state, QStringLiteral("lead"), coupling.value(QStringLiteral("lead")), feature, context);
    } else if (type == QStringLiteral("ratio")) {
      loadValue(state, QStringLiteral("ratio"), coupling.value(QStringLiteral("ratio")), feature, context);
    } else if (type == QStringLiteral("cam")) {
      state.setItems(QStringLiteral("profile"), {originItem(coupling.value(QStringLiteral("profile")), prefix, context)});
      const QJsonObject follower = coupling.value(QStringLiteral("follower")).toObject();
      state.setChoice(QStringLiteral("follower"), str(follower, "type").isEmpty() ? QStringLiteral("knife")
                                                                                  : str(follower, "type"));
      if (follower.contains(QStringLiteral("radius"))) {
        loadSize(state, QStringLiteral("roller"), follower.value(QStringLiteral("radius")), feature, prefix, context);
      }
      if (coupling.contains(QStringLiteral("tip"))) {
        state.setItems(QStringLiteral("tip"), {originItem(coupling.value(QStringLiteral("tip")), prefix, context)});
      }
    }
    state.setChecked(QStringLiteral("reverse"), feature.value(QStringLiteral("reverse")).toBool());
    // An edit keeps its offset as it is.
    state.setChecked(QStringLiteral("keep"), false);
    const bool turns = isTurn(str(feature.value(QStringLiteral("b")).toObject(), "motion"));
    loadValue(state, turns ? QStringLiteral("offset_angle") : QStringLiteral("offset"),
              feature.value(QStringLiteral("offset")), feature, context);
  };

  def.describe = [](const CommandState&, const CommandContext& context) {
    const QJsonObject link = linkOf(jointsOf(context), [](const QJsonObject&) { return true; });
    return link.isEmpty() ? QStringLiteral("Added motion link") : QStringLiteral("Added motion link %1").arg(linkLine(link));
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
  // Assembly constraints (mitcad#148).
  registry.add(constraintCommand(context));
  // Contact sets (mitcad#454).
  registry.add(contactSetCommand(context));
  // Motion links (mitcad#466).
  registry.add(motionLinkCommand(context));
}

} // namespace mitcad
