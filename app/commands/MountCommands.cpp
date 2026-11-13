// SPDX-License-Identifier: MIT
// Place Part (mitcad#525; app/COMMANDS.md, "Component libraries"; the
// model's side in commands.md, "Mount frames"). Insert from Library starts
// it for a part whose manifest lists mount frames: a click on a face, a
// hole (its round face or rim) or a grid snaps the chosen frame to it, face
// to face, and the preview shows the part there (`mount_preview`, its shape
// `analysis_shape` `mount_preview`); Place (Enter) or Shift + click makes
// it (`insert_component` with `mount`: the part and the joint its frame
// asks for, in one undo step), and the panel stays for the next one. A
// selection made before Insert from Library that Place On takes is the
// pick (mitcad#1197: a hole's rim selected, the screw shown in it). Enter
// without a pick drops the part in at the origin as Insert from Library
// always did, to be joined later with Joint (its frames are joint origins
// of its component).
#include "MountCommands.hpp"
#include "framework/ModelMessages.hpp"

#include <cmath>
#include <exception>
#include <functional>
#include <optional>
#include <vector>

#include <QObject>
#include <QStringList>
#include <QtLogging>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../browser/DocumentHost.hpp"
#include "../browser/DocumentSnapshot.hpp"
#include "../build/BuildModel.hpp"
#include "../framework/Command.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Json.hpp"
#include "../framework/ModelShapes.hpp"
#include "../library/MountFrames.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace {

const QString kRoot = QStringLiteral("C0");
const QString kAt = QStringLiteral("at");
const QString kFrame = QStringLiteral("frame");
const QString kToward = QStringLiteral("at_toward");

// The active component: where insert_component puts the part. Its path
// from the root ("" for the root), its placement in the design and name.
struct Active {
  QString component = kRoot;
  QString path;
  gp_Trsf world;
  QString name = shownComponentName(kRoot, QStringLiteral("Root"));
};

Active activeOf(const CommandContext& context) {
  Active active;
  QJsonObject components;
  try {
    components = context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
  } catch (const std::exception&) {
    return active;
  }
  active.component = components.value(QStringLiteral("active")).toString(kRoot);
  for (const QJsonValue& value : components.value(QStringLiteral("components")).toArray()) {
    if (str(value.toObject(), "uid") == active.component) {
      active.name = shownComponentName(active.component, str(value.toObject(), "name"));
    }
  }
  if (active.component == kRoot) {
    return active;
  }
  // Its first placement, depth first as the tree lists them.
  bool found = false;
  std::function<void(const QJsonArray&, const QString&)> walk = [&](const QJsonArray& level, const QString& parent) {
    for (const QJsonValue& value : level) {
      if (found) {
        return;
      }
      const QJsonObject node = value.toObject();
      const QString path = parent.isEmpty() ? str(node, "uid") : parent + QLatin1Char('/') + str(node, "uid");
      if (str(node, "component") == active.component) {
        active.path = path;
        active.world = trsfOf(node.value(QStringLiteral("world")).toArray());
        found = true;
        return;
      }
      walk(node.value(QStringLiteral("children")).toArray(), path);
    }
  };
  walk(components.value(QStringLiteral("occurrences")).toArray(), QString());
  return active;
}

// An item's occurrence path from the active component; none when the item
// is not inside it.
std::optional<QString> pathFromActive(const SelectionItem& item, const Active& active) {
  if (active.path.isEmpty()) {
    return item.occurrence;
  }
  if (item.occurrence == active.path) {
    return QString();
  }
  if (item.occurrence.startsWith(active.path + QLatin1Char('/'))) {
    return item.occurrence.mid(active.path.size() + 1);
  }
  return std::nullopt;
}

QJsonArray xyzJson(double x, double y, double z) { return QJsonArray{x, y, z}; }

// The part's frames as the manifest lists them.
QVector<MountFrameInfo> framesOf(const MountPart& part) { return mountFramesOf(part.frames); }

// The frame chosen (the first when none is).
MountFrameInfo chosenFrame(const MountPart& part, const CommandState& state) {
  const QVector<MountFrameInfo> frames = framesOf(part);
  for (const MountFrameInfo& frame : frames) {
    if (frame.name == state.choice(kFrame)) {
      return frame;
    }
  }
  return frames.isEmpty() ? MountFrameInfo() : frames.first();
}

cmd::Choices frameChoices(const MountPart& part) {
  cmd::Choices choices;
  for (const MountFrameInfo& frame : framesOf(part)) {
    choices.append({frame.name, mountFrameLabel(frame)});
  }
  return choices;
}

// What a click takes for the frame chosen: an axis (a hole's round face or
// rim, an axis) for a hole or shaft frame; for a foot any face of the part
// with the grid (the cell under the click), so that the planes of the
// frames of the parts already placed, which lie in the cells' faces, and
// the origin's do not take the clicks; else a plane (a flat face, a
// construction or origin plane).
SelectFilter takesFor(const MountPart& part, const CommandState& state) {
  const QString kind = chosenFrame(part, state).kind;
  if (mountTakesAxes(kind)) {
    return SelectFilter(SelectKind::Face | SelectKind::Edge | SelectKind::Axis);
  }
  if (kind == QStringLiteral("foot")) {
    return SelectFilter(SelectKind::Face);
  }
  return SelectFilter(SelectKind::Face | SelectKind::Plane);
}

bool takes(const MountPart& part, const SelectionItem& item, const CommandState& state) {
  const QString kind = chosenFrame(part, state).kind;
  if (mountTakesAxes(kind)) {
    return item.kind == SelectKind::Axis || (item.kind == SelectKind::Edge && cmd::isCircular(item)) ||
           (item.kind == SelectKind::Face && cmd::isAxial(item));
  }
  if (kind == QStringLiteral("foot")) {
    return item.kind == SelectKind::Face;
  }
  return cmd::isPlanar(item);
}

// `mount.at` of the pick: the item's path from the active component, the
// geometry, where it was clicked and the way towards the viewer then, both
// in the active component's coordinates. None without a pick (a drop-in);
// with `problem` set when the pick is not inside the active component.
std::optional<QJsonObject> atOf(const CommandState& state, const CommandContext& context, QString& problem) {
  const Selection& items = state.items(kAt);
  if (items.isEmpty()) {
    return std::nullopt;
  }
  const SelectionItem& item = items.first();
  const Active active = activeOf(context);
  const std::optional<QString> path = pathFromActive(item, active);
  if (!path) {
    problem = QObject::tr("%1 is not in %2, where the component goes.").arg(context.itemText(item), active.name);
    return std::nullopt;
  }
  QJsonObject at{{QStringLiteral("geometry"), item.reference()}};
  if (!path->isEmpty()) {
    at.insert(QStringLiteral("occurrence"), *path);
  }
  const gp_Trsf back = active.world.Inverted();
  if (const std::optional<gp_Pnt> point = build::pickPoint(kAt, state, context)) {
    const gp_Pnt local = point->Transformed(back);
    at.insert(QStringLiteral("point"), xyzJson(local.X(), local.Y(), local.Z()));
  }
  const QJsonObject toward = state.object(kToward);
  const QJsonArray xyz = toward.value(QStringLiteral("toward")).toArray();
  if (str(toward, "item") == item.logDescription() && xyz.size() == 3) {
    const gp_Vec way = gp_Vec(xyz[0].toDouble(), xyz[1].toDouble(), xyz[2].toDouble()).Transformed(back);
    if (way.Magnitude() > 1e-12) {
      at.insert(QStringLiteral("toward"), xyzJson(way.X(), way.Y(), way.Z()));
    }
  }
  return at;
}

// insert_component's fields (also mount_preview's): the part, and with a
// pick its placement by the frame chosen.
QJsonObject fieldsOf(const MountPart& part, const CommandState& state, const std::optional<QJsonObject>& at) {
  QJsonObject fields{{QStringLiteral("library"), part.library}, {QStringLiteral("link"), part.link}};
  if (!part.parameters.isEmpty()) {
    fields.insert(QStringLiteral("parameters"), part.parameters); // mitcad#810
  }
  if (!at) {
    return fields;
  }
  QJsonObject mount{{QStringLiteral("at"), *at}};
  const MountFrameInfo frame = chosenFrame(part, state);
  if (!frame.name.isEmpty()) {
    mount.insert(QStringLiteral("frame"), frame.name);
  }
  if (cmd::nonZero(state, QStringLiteral("rotate"))) {
    mount.insert(QStringLiteral("rotate"), state.expression(QStringLiteral("rotate")));
  }
  if (cmd::nonZero(state, QStringLiteral("offset"))) {
    mount.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
  }
  if (state.checked(QStringLiteral("flip"))) {
    mount.insert(QStringLiteral("flip"), true);
  }
  fields.insert(QStringLiteral("mount"), mount);
  return fields;
}

QString problemsOf(const QJsonObject& answer) {
  QStringList texts;
  for (const QJsonValue& problem : answer.value(QStringLiteral("problems")).toArray()) {
    texts << mitcad::modelMessage(problem.toString());
  }
  return texts.join(QLatin1Char(' '));
}

// Where a placement goes, for the panel and the log: "Baseplate:1, cells
// (1, 0)" (`target.name`, a grid placement's cells).
QString whereText(const QJsonObject& plan) {
  QString text = str(plan.value(QStringLiteral("target")).toObject(), "name");
  const QJsonObject grid = plan.value(QStringLiteral("grid")).toObject();
  if (!grid.isEmpty()) {
    const QJsonArray covers = grid.value(QStringLiteral("covers")).toArray();
    text += covers.size() == 1 ? QStringLiteral(", cell ") : QStringLiteral(", cells ");
    text += mountCellsText(covers);
  }
  return text;
}

QJsonObject marker(const QString& label, const QJsonValue& point) {
  return {{QStringLiteral("label"), label}, {QStringLiteral("point"), point}};
}

std::vector<PreviewMarker> markersOf(const QJsonObject& report) {
  std::vector<PreviewMarker> markers;
  for (const QJsonValue& value : report.value(QStringLiteral("markers")).toArray()) {
    const QJsonObject entry = value.toObject();
    if (const std::optional<gp_Pnt> point = pointFrom(entry.value(QStringLiteral("point")))) {
      markers.push_back({str(entry, "label"), *point});
    }
  }
  return markers;
}

QString noPart() { return QObject::tr("Choose a component with mount frames in Insert from Library first."); }

Inspection inspectPlacement(const MountPart& part, const CommandState& state, const CommandContext& context) {
  Inspection result;
  if (part.library.isEmpty()) {
    result.error = noPart();
    return result;
  }
  QString problem;
  const std::optional<QJsonObject> at = atOf(state, context, problem);
  if (!problem.isEmpty()) {
    result.error = problem;
    return result;
  }
  const QString name = part.name.toHtmlEscaped();
  if (!at) {
    result.viewHint =
        QObject::tr("Click where %1 mounts: a face, a hole or a grid; Enter drops it in; Shift+click places at once")
            .arg(part.name);
    result.text = QObject::tr("<b>%1</b><br>Click where it mounts, or press Enter to drop it in at the origin and "
                              "join it later with Joint.")
                      .arg(name);
    result.log = QStringLiteral("place part %1: no pick, Enter drops it in").arg(part.name);
    return result;
  }
  QJsonObject request = fieldsOf(part, state, at);
  QJsonObject shapeRequest = request;
  request.insert(QStringLiteral("query"), QStringLiteral("mount_preview"));
  QJsonObject answer;
  try {
    answer = context.queryObject(request);
  } catch (const std::exception& error) {
    result.error = errorText(error);
    return result;
  }
  const QString refused = problemsOf(answer);
  if (!refused.isEmpty()) {
    result.error = refused;
    result.log = QStringLiteral("place part %1: refused: %2").arg(part.name, refused);
    return result;
  }
  shapeRequest.insert(QStringLiteral("shape"), QStringLiteral("mount_preview"));
  result.shape = context.analysisShape(shapeRequest);
  const QString where = whereText(answer);
  const QString joint = str(answer, "joint");
  QStringList lines{QStringLiteral("<b>%1</b>").arg(name)};
  lines << QObject::tr("By its frame %1, %2").arg(str(answer, "frame").toHtmlEscaped(), mountKindText(str(answer, "kind")))
        << QObject::tr("On %1").arg(where.toHtmlEscaped()) << QObject::tr("Once placed: %1").arg(mountJointText(joint));
  result.text = lines.join(QStringLiteral("<br>"));
  result.log = QStringLiteral("place part %1: frame %2 (%3), %4 joint, on %5")
                   .arg(part.name, str(answer, "frame"), str(answer, "kind"), joint, where);
  const QJsonValue origin = answer.value(QStringLiteral("at")).toObject().value(QStringLiteral("origin"));
  result.report = {{QStringLiteral("markers"), QJsonArray{marker(str(answer, "frame"), origin)}}};
  return result;
}

bool place(MountPart& part, DocumentHost& host, CommandState& state, const CommandContext& context) {
  if (part.library.isEmpty()) {
    host.showStatus(noPart(), NoticeLevel::Error);
    return false;
  }
  QString problem;
  const std::optional<QJsonObject> at = atOf(state, context, problem);
  if (!problem.isEmpty()) {
    host.showStatus(problem, NoticeLevel::Error);
    return false;
  }
  QJsonObject command = fieldsOf(part, state, at);
  command.insert(QStringLiteral("cmd"), QStringLiteral("insert_component"));
  QJsonObject result;
  if (!host.runModelCommand(command, &result)) {
    return false;
  }
  const QString how = part.link ? QStringLiteral("linked") : QStringLiteral("copy");
  QStringList bodies;
  for (const QJsonValue& body : result.value(QStringLiteral("bodies")).toArray()) {
    bodies << body.toString();
  }
  QStringList frames;
  for (const MountFrameInfo& frame : mountFramesOf(result.value(QStringLiteral("frames")))) {
    frames << QStringLiteral("%1 (%2)").arg(frame.name, frame.kind);
  }
  const QString component = str(result, "name");
  const QString occurrence = str(result, "occurrence");
  if (at) {
    const QJsonObject plan = result.value(QStringLiteral("mount")).toObject();
    // Where the frame went, in the design (for UI tests: the part's place).
    QStringList origin;
    for (const QJsonValue& c : plan.value(QStringLiteral("at")).toObject().value(QStringLiteral("origin")).toArray()) {
      const double rounded = std::round(c.toDouble() * 1000.0) / 1000.0;
      origin << QString::number(rounded == 0.0 ? 0.0 : rounded, 'g', 12);
    }
    qInfo().noquote() << QStringLiteral("Placed part %1 (%2) from %3 %4 as %5 (%6) by its frame %7 on %8: %9 joint %10 "
                                        "at (%11); bodies %12")
                             .arg(part.name, how, str(part.library, "id"), part.version, component, occurrence,
                                  str(plan, "frame"), whereText(plan), str(plan, "joint"))
                             .arg(str(result, "joint_name"), origin.join(QStringLiteral(", ")),
                                  bodies.join(QStringLiteral(", ")));
    host.showStatus(QObject::tr("Placed %1 on %2.").arg(component, whereText(plan)), NoticeLevel::Information);
  } else {
    qInfo().noquote() << QStringLiteral("Inserted component %1 (%2) from %3 %4, licence %5, dropped in; bodies %6; "
                                        "mount frames %7")
                             .arg(component, how, str(part.library, "id"), part.version,
                                  part.license.isEmpty() ? QStringLiteral("none") : part.license,
                                  bodies.join(QStringLiteral(", ")), frames.join(QStringLiteral(", ")));
    host.showStatus(QObject::tr("Inserted %1; join it with Joint, its mount frames are joint origins.").arg(component),
                    NoticeLevel::Information);
  }
  // The next one starts from a new click.
  state.setItems(kAt, {});
  state.setObject(QStringLiteral("at_pick"), {});
  state.setObject(kToward, {});
  return true;
}

CommandDef mountPartCommand(const std::shared_ptr<MountPart>& part, DocumentHost& host) {
  CommandDef def;
  def.id = QString::fromLatin1(kMountPartCommand);
  def.name = QObject::tr("Place Component");
  def.icon = QStringLiteral("insert-library");
  def.tooltip = QObject::tr("Places a library component by its mount frame on a face, a hole or a grid");
  def.tab.clear(); // Insert from Library starts it
  def.group = QStringLiteral("ASSEMBLE");
  def.keywords = {QStringLiteral("mount"), QStringLiteral("snap"), QStringLiteral("library"), QStringLiteral("grid")};
  def.okLabel = QObject::tr("Place");
  def.repeats = true;
  def.enabled = [part] { return !part->library.isEmpty(); };
  def.inputs = {
      selectionInput(kAt, QObject::tr("Place On"), SelectKind::Face | SelectKind::Edge | SelectKind::Plane | SelectKind::Axis,
                     0, 1)
          .withPickRule([part](const CommandState& state) { return takesFor(*part, state); },
                        [part](const SelectionItem& item, const CommandState& state) { return takes(*part, item, state); })
          .withOnPick(
              [](const SelectionItem& item, CommandState& state, const CommandContext& context) {
                build::recordPick(kAt, item, state, context);
                // The way towards the viewer at the click: which end of a
                // hole the part goes in from.
                const std::optional<gp_Pnt> point = build::pickPoint(kAt, state, context);
                if (point) {
                  if (const std::optional<gp_Dir> toward = context.towardViewer(*point)) {
                    state.setObject(kToward, {{QStringLiteral("item"), item.logDescription()},
                                              {QStringLiteral("toward"), xyzJson(toward->X(), toward->Y(), toward->Z())}});
                  }
                }
              },
              false)
          // Shift + click: the pick and Place in one (as BUILD, mitcad#451).
          .withShiftClickApplies()
          .withTooltip(QObject::tr("A face, a hole's round face or rim, or a grid; none drops the component in")),
      choiceInput(kFrame, QObject::tr("Mount Frame"), {}, QString())
          .withChoices([part](const CommandState&) { return frameChoices(*part); })
          .withTooltip(QObject::tr("The component's frame that goes on the pick (a lamp's ceiling or wall frame)")),
      valueInput(QStringLiteral("rotate"), QObject::tr("Rotate"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("About the frame's Z; on a grid in quarter turns")),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the frame's Z: a gap from what it is on")),
      checkInput(QStringLiteral("flip"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("The plane's other side, or the hole's other end")),
  };
  // A selection made before Insert from Library that Place On takes (a
  // hole's rim or round face, an axis, a flat face or a plane) is the pick
  // (mitcad#1197): the part shows there at once, Place (Enter) places it.
  // Its click's place and the way towards the viewer then, as a click's.
  def.preselected = [](const Selection&, CommandState& state, const CommandContext& context) {
    const Selection& items = state.items(kAt);
    if (items.isEmpty()) {
      return;
    }
    const SelectionItem item = items.first();
    build::recordPick(kAt, item, state, context);
    if (const std::optional<gp_Pnt> point = build::pickPoint(kAt, state, context)) {
      if (const std::optional<gp_Dir> toward = context.towardViewer(*point)) {
        state.setObject(kToward, {{QStringLiteral("item"), item.logDescription()},
                                  {QStringLiteral("toward"), xyzJson(toward->X(), toward->Y(), toward->Z())}});
      }
    }
    qInfo().noquote() << QStringLiteral("Place Component: Place On %1 from the selection").arg(item.logDescription());
  };
  def.init = [part](CommandState& state, const CommandContext&) {
    const QVector<MountFrameInfo> frames = framesOf(*part);
    if (!frames.isEmpty() && state.choice(kFrame).isEmpty()) {
      state.setChoice(kFrame, frames.first().name);
    }
    QStringList names;
    for (const MountFrameInfo& frame : frames) {
      names << mountFrameLabel(frame);
    }
    qInfo().noquote() << QStringLiteral("Place Part %1 from %2 %3: frames %4")
                             .arg(part->name, str(part->library, "id"), part->version, names.join(QStringLiteral(", ")));
  };
  def.inspect = [part](const CommandState& state, const CommandContext& context) {
    return inspectPlacement(*part, state, context);
  };
  def.previewMarkers = markersOf;
  def.apply = [part, &host](CommandState& state, const CommandContext& context) {
    return place(*part, host, state, context);
  };
  return def;
}

} // namespace

void registerMountCommands(CommandRegistry& registry, DocumentHost& host, std::shared_ptr<MountPart> part) {
  registry.add(mountPartCommand(part, host));
}

} // namespace mitcad
