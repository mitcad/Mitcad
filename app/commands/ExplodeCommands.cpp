// SPDX-License-Identifier: MIT
// Explode Step (mitcad#2113; app/COMMANDS.md "Exploded views"): one step of
// an explode view, made or edited in a panel of its own from the Explode
// panel (motion/ExplodeViewDialog): the components (or bodies of the view's
// component) it moves, a Distance along a Direction and an Angle about an
// Axis, each dragged by its handle at the components' middle (an arrow, a
// ring) or typed, an axis of the component or picked geometry for each,
// Trail Lines and its Time in the animation. OK adds the step to the view
// (`edit_explode_view`'s `add_step`, or a new view with `add_explode_view`)
// or changes the step edited (`step` and `set`): one undo step that
// recomputes nothing. A picked direction or axis of the view's component is
// kept as its geometry reference (mitcad#2328: the occurrence's path from
// the component and the geometry, `toward` the panel's arrow so that the
// model keeps `flip` to match), so the step follows edits of the design;
// an axis of the component, or geometry outside it, as a vector. Edit Step
// shows a reference's pick again, its arrow the way the model resolves it.
#include <cmath>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QStringList>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../framework/CommandRegistry.hpp"
#include "../framework/Json.hpp"
#include "../framework/ModelShapes.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace cmd {
namespace {

const QString kRoot = QStringLiteral("C0");

const QVector<QPair<QString, QString>> kAxes = {
    {QStringLiteral("x"), QStringLiteral("X")},   {QStringLiteral("-x"), QStringLiteral("-X")},
    {QStringLiteral("y"), QStringLiteral("Y")},   {QStringLiteral("-y"), QStringLiteral("-Y")},
    {QStringLiteral("z"), QStringLiteral("Z")},   {QStringLiteral("-z"), QStringLiteral("-Z")},
    {QStringLiteral("picked"), QStringLiteral("Picked Geometry")}};

InputDef hiddenText(const QString& id) {
  return textInput(id, id).withVisible([](const CommandState&) { return false; });
}

// The explode views of the design (the `explode_views` query).
QJsonArray explodeViewsOf(const CommandContext& context) {
  try {
    return context.queryArray(QStringLiteral("explode_views"));
  } catch (const std::exception&) {
    return QJsonArray();
  }
}

QJsonObject explodeViewOf(const QString& name, const CommandContext& context) {
  for (const QJsonValue& value : explodeViewsOf(context)) {
    if (str(value.toObject(), "name") == name) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

// The path from the root that places a component (the first; empty for
// the root or a component placed nowhere) and where it puts it.
QString placingPath(const QString& component, const CommandContext& context) {
  if (component.isEmpty() || component == kRoot) {
    return QString();
  }
  QString found;
  eachOccurrence(context, [&](const QJsonObject& node, const QString& path) {
    if (found.isEmpty() && str(node, "component") == component) {
      found = path;
    }
  });
  return found;
}

gp_Trsf worldOfPath(const QString& path, const CommandContext& context) {
  gp_Trsf found;
  if (path.isEmpty()) {
    return found;
  }
  eachOccurrence(context, [&](const QJsonObject& node, const QString& at) {
    if (at == path) {
      found = trsfOf(node.value(QStringLiteral("world")).toArray());
    }
  });
  return found;
}

// The view's component (the root for a new view).
QString viewComponent(const CommandState& state, const CommandContext& context) {
  const QString view = state.text(QStringLiteral("view"));
  const QString component = view.isEmpty() ? QString() : str(explodeViewOf(view, context), "component");
  return component.isEmpty() ? kRoot : component;
}

// A component picked in the browser, or the occurrence that places a body
// clicked in the view; a body of the root kept as a body.
SelectionItem partItem(const SelectionItem& item) {
  if (item.kind == SelectKind::Body && item.occurrence.isEmpty()) {
    return item;
  }
  SelectionItem occurrence{SelectKind::Component, QString(), QString(), QString()};
  occurrence.occurrence = item.occurrence;
  if (item.kind == SelectKind::Component) {
    occurrence.owner = item.owner;
  }
  return occurrence;
}

bool isPart(const SelectionItem& item) { return !item.occurrence.isEmpty() || item.kind == SelectKind::Body; }

// An axis of the choices as a direction of the component.
std::optional<gp_Dir> namedAxis(const QString& choice) {
  const bool negative = choice.startsWith(QLatin1Char('-'));
  const QString axis = negative ? choice.mid(1) : choice;
  const double sign = negative ? -1.0 : 1.0;
  if (axis == QLatin1String("x")) {
    return gp_Dir(sign, 0, 0);
  }
  if (axis == QLatin1String("y")) {
    return gp_Dir(0, sign, 0);
  }
  if (axis == QLatin1String("z")) {
    return gp_Dir(0, 0, sign);
  }
  return std::nullopt;
}

// A direction (`direction` or `axis`) in the design's coordinates: an axis
// of the view's component placed by its occurrence, the picked geometry's
// axis or normal, or the vector the step kept.
std::optional<gp_Dir> designDirection(const CommandState& state, const QString& key, const CommandContext& context) {
  const QString choice = state.choice(key);
  if (const auto axis = namedAxis(choice)) {
    return axis->Transformed(worldOfPath(placingPath(viewComponent(state, context), context), context));
  }
  const Selection& picked = state.items(key + QStringLiteral("_geometry"));
  if (!picked.isEmpty()) {
    std::optional<gp_Dir> found;
    if (const auto axis = axisOf(picked.first(), context)) {
      found = axis->second;
    } else if (const auto frame = frameOf(picked.first(), context)) {
      found = frame->normal;
    } else {
      return std::nullopt;
    }
    // The way the step edited points (mitcad#2328: `<key>_toward`, its
    // geometry's direction as the model resolves it, flip applied).
    const QJsonArray toward = state.object(key + QStringLiteral("_toward")).value(QStringLiteral("vector")).toArray();
    if (toward.size() == 3) {
      const gp_Vec way(toward[0].toDouble(), toward[1].toDouble(), toward[2].toDouble());
      if (way.Magnitude() > 1e-12 && found->Dot(gp_Dir(way)) < 0) {
        found->Reverse();
      }
    }
    return found;
  }
  const QJsonArray kept = state.object(key + QStringLiteral("_kept")).value(QStringLiteral("vector")).toArray();
  if (kept.size() == 3) {
    const gp_Vec v(kept[0].toDouble(), kept[1].toDouble(), kept[2].toDouble());
    if (v.Magnitude() > 1e-12) {
      return gp_Dir(v).Transformed(worldOfPath(placingPath(viewComponent(state, context), context), context));
    }
  }
  // Geometry kept that no pick shows (mitcad#2328): the way it points now.
  const QJsonArray toward = state.object(key + QStringLiteral("_toward")).value(QStringLiteral("vector")).toArray();
  if (toward.size() == 3) {
    const gp_Vec v(toward[0].toDouble(), toward[1].toDouble(), toward[2].toDouble());
    if (v.Magnitude() > 1e-12) {
      return gp_Dir(v);
    }
  }
  return std::nullopt;
}

// That direction as the view's component has it, for the model.
std::optional<QJsonArray> componentVector(const CommandState& state, const QString& key,
                                          const CommandContext& context) {
  const QString choice = state.choice(key);
  if (const auto axis = namedAxis(choice)) {
    return QJsonArray{axis->X(), axis->Y(), axis->Z()};
  }
  auto design = designDirection(state, key, context);
  if (!design) {
    return std::nullopt;
  }
  const gp_Trsf back = worldOfPath(placingPath(viewComponent(state, context), context), context).Inverted();
  const gp_Dir local = design->Transformed(back);
  const auto clean = [](double v) { return std::abs(v) < 1e-12 ? 0.0 : std::round(v * 1e9) / 1e9; };
  return QJsonArray{clean(local.X()), clean(local.Y()), clean(local.Z())};
}

// A path from the root as a path from the view's component placed at
// `prefix` (its path from the root; empty for the root): empty for the
// component's own geometry; none for a path outside it.
std::optional<QString> pathInComponent(const QString& path, const QString& prefix) {
  if (prefix.isEmpty() || path == prefix) {
    return prefix.isEmpty() ? path : QString();
  }
  if (path.startsWith(prefix + QLatin1Char('/'))) {
    return path.mid(prefix.size() + 1);
  }
  return std::nullopt;
}

// The step's direction or axis for the model (mitcad#2328): picked geometry
// of the view's component as its reference (the occurrence's path from the
// component and the geometry), which follows edits of the design, with
// `toward` the panel's arrow so that the model keeps `flip` to point the
// same way; an axis of the component, geometry outside it, or the vector
// kept, as a vector in the component.
std::optional<QJsonValue> stepAxis(const CommandState& state, const QString& key, const CommandContext& context) {
  const Selection& picked = state.items(key + QStringLiteral("_geometry"));
  const bool named = namedAxis(state.choice(key)).has_value();
  if (!named && picked.isEmpty()) {
    // The reference of the step edited, while nothing else is picked: as
    // it is.
    const QJsonObject kept = state.object(key + QStringLiteral("_kept"));
    if (kept.contains(QStringLiteral("geometry"))) {
      QJsonObject axis{{QStringLiteral("geometry"), kept.value(QStringLiteral("geometry"))}};
      if (!str(kept, "occurrence").isEmpty()) {
        axis.insert(QStringLiteral("occurrence"), kept.value(QStringLiteral("occurrence")));
      }
      if (kept.value(QStringLiteral("flip")).toBool()) {
        axis.insert(QStringLiteral("flip"), true);
      }
      return QJsonValue(axis);
    }
  }
  const auto vector = componentVector(state, key, context);
  if (!vector) {
    return std::nullopt;
  }
  if (named || picked.isEmpty()) {
    return QJsonValue(*vector);
  }
  const SelectionItem& item = picked.first();
  const QJsonValue geometry = item.reference();
  const auto path = pathInComponent(item.occurrence, placingPath(viewComponent(state, context), context));
  if (!path || geometry.isNull() || geometry.isUndefined()) {
    return QJsonValue(*vector);
  }
  QJsonObject axis{{QStringLiteral("geometry"), geometry}, {QStringLiteral("toward"), *vector}};
  if (!path->isEmpty()) {
    axis.insert(QStringLiteral("occurrence"), *path);
  }
  return QJsonValue(axis);
}

// Where the handles stand: the middle of the picked components.
gp_Pnt partsMiddle(const CommandState& state, const CommandContext& context) {
  const Selection& parts = state.items(QStringLiteral("occurrences"));
  if (const auto middle = centerOf(parts, context)) {
    return *middle;
  }
  if (!parts.isEmpty() && !parts.first().occurrence.isEmpty()) {
    return gp_Pnt(0, 0, 0).Transformed(worldOfPath(parts.first().occurrence, context));
  }
  return gp_Pnt(0, 0, 0);
}

std::optional<Manipulator> moveHandle(const CommandState& state, const CommandContext& context) {
  if (state.items(QStringLiteral("occurrences")).isEmpty()) {
    return std::nullopt;
  }
  const auto along = designDirection(state, QStringLiteral("direction"), context);
  if (!along) {
    return std::nullopt;
  }
  return arrow(partsMiddle(state, context), *along);
}

std::optional<Manipulator> turnHandle(const CommandState& state, const CommandContext& context) {
  if (state.items(QStringLiteral("occurrences")).isEmpty()) {
    return std::nullopt;
  }
  const auto about = designDirection(state, QStringLiteral("axis"), context);
  if (!about) {
    return std::nullopt;
  }
  Manipulator handle = ring(partsMiddle(state, context), *about);
  // The ring's zero towards whichever axis is most across it.
  const gp_Dir x(1, 0, 0);
  handle.reference = std::abs(about->Dot(x)) < 0.9 ? x : gp_Dir(0, 1, 0);
  return handle;
}

// A path from the root as a path from the view's component.
QString pathFromComponent(const QString& path, const QString& prefix) {
  if (prefix.isEmpty()) {
    return path;
  }
  return path.startsWith(prefix + QLatin1Char('/')) ? path.mid(prefix.size() + 1) : path;
}

// The choice a kept direction shows: an axis, else Picked Geometry (the
// vector kept).
void loadDirection(const QJsonObject& kept, const QString& key, CommandState& state, const QString& prefix,
                   const CommandContext& context) {
  const QJsonArray v = kept.value(QStringLiteral("vector")).toArray();
  const bool flip = kept.value(QStringLiteral("flip")).toBool();
  if (v.size() == 3) {
    const double x = v[0].toDouble() * (flip ? -1.0 : 1.0);
    const double y = v[1].toDouble() * (flip ? -1.0 : 1.0);
    const double z = v[2].toDouble() * (flip ? -1.0 : 1.0);
    const double length = std::sqrt(x * x + y * y + z * z);
    for (const auto& [id, label] : kAxes) {
      const auto axis = namedAxis(id);
      if (axis && length > 0 &&
          std::abs(axis->X() - x / length) + std::abs(axis->Y() - y / length) + std::abs(axis->Z() - z / length) <
              1e-9) {
        state.setChoice(key, id);
        return;
      }
    }
    state.setChoice(key, QStringLiteral("picked"));
    state.setObject(key + QStringLiteral("_kept"), QJsonObject{{QStringLiteral("vector"), QJsonArray{x, y, z}}});
    return;
  }
  // Geometry kept by the model (a pick of this panel's, mitcad#2328, or an
  // agent's): shown picked where it can be, its arrow the way the model
  // resolves it now (`resolved`, flip applied); the reference itself kept
  // for OK while nothing else is picked.
  state.setChoice(key, QStringLiteral("picked"));
  state.setObject(key + QStringLiteral("_kept"), kept);
  const QJsonArray resolved = kept.value(QStringLiteral("resolved")).toArray();
  if (resolved.size() == 3) {
    const gp_Vec local(resolved[0].toDouble(), resolved[1].toDouble(), resolved[2].toDouble());
    if (local.Magnitude() > 1e-12) {
      const gp_Dir design = gp_Dir(local).Transformed(worldOfPath(prefix, context));
      state.setObject(key + QStringLiteral("_toward"),
                      QJsonObject{{QStringLiteral("vector"), QJsonArray{design.X(), design.Y(), design.Z()}}});
    }
  }
  SelectionItem item = itemOf(kept.value(QStringLiteral("geometry")), context);
  if (item.kind != SelectKind::None) {
    const QString own = str(kept, "occurrence");
    item.occurrence = prefix.isEmpty() ? own : own.isEmpty() ? prefix : prefix + QLatin1Char('/') + own;
    state.setItems(key + QStringLiteral("_geometry"), {item});
  }
}

} // namespace

CommandDef explodeStepCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("assemble.explode_step");
  def.name = QObject::tr("Explode Step");
  def.icon = QStringLiteral("explode");
  def.tooltip = QObject::tr("Moves components of an exploded view along a direction or about an axis");
  def.keywords = {QStringLiteral("explode"), QStringLiteral("exploded view"), QStringLiteral("trail")};
  def.modelCommand = true;
  // Opened with its view (`name`) from the Explode panel: logged "Editing
  // explode view Explode1 with Explode Step".
  def.editedKind = QStringLiteral("explode view");
  const auto picked = [](const QString& key) {
    return [key](const CommandState& state) { return state.choice(key) == QLatin1String("picked"); };
  };
  def.inputs = {
      selectionInput(QStringLiteral("occurrences"), QObject::tr("Components"),
                     SelectKind::Component | SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("Components in the browser, or bodies they place"))
          .withAccepts(isPart)
          .withConvert(partItem),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kAxes, QStringLiteral("z")),
      selectionInput(QStringLiteral("direction_geometry"), QObject::tr("Direction Geometry"),
                     SelectKind::Edge | SelectKind::Face | SelectKind::Axis | SelectKind::Plane, 0, 1)
          .withTooltip(QObject::tr("An edge, an axis, a face or a plane: along it, or along its normal"))
          .withVisible(picked(QStringLiteral("direction"))),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withManipulator(moveHandle),
      choiceInput(QStringLiteral("axis"), QObject::tr("Axis"), kAxes, QStringLiteral("z")),
      selectionInput(QStringLiteral("axis_geometry"), QObject::tr("Axis Geometry"),
                     SelectKind::Edge | SelectKind::Face | SelectKind::Axis | SelectKind::Plane, 0, 1)
          .withTooltip(QObject::tr("An edge, an axis, a round face or a plane to turn about"))
          .withVisible(picked(QStringLiteral("axis"))),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withManipulator(turnHandle),
      checkInput(QStringLiteral("trails"), QObject::tr("Trail Lines"), true)
          .withTooltip(QObject::tr("Thin lines from each component back to where the step found it")),
      valueInput(QStringLiteral("seconds"), QObject::tr("Time"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The step's seconds in the animation")),
      hiddenText(QStringLiteral("view")),
      hiddenText(QStringLiteral("step")),
      hiddenText(QStringLiteral("step_name")),
  };
  def.enabled = [&context] {
    int count = 0;
    eachOccurrence(context, [&count](const QJsonObject&, const QString&) { ++count; });
    return count > 0;
  };
  def.build = [](const CommandState& state, const CommandContext& context) {
    const QString view = state.text(QStringLiteral("view"));
    const QString prefix = placingPath(viewComponent(state, context), context);
    QJsonArray occurrences;
    QJsonArray bodies;
    for (const SelectionItem& item : state.items(QStringLiteral("occurrences"))) {
      if (item.kind == SelectKind::Body && item.occurrence.isEmpty()) {
        if (!bodies.contains(item.owner)) {
          bodies.append(item.owner);
        }
      } else if (!item.occurrence.isEmpty()) {
        const QString path = pathFromComponent(item.occurrence, prefix);
        if (!occurrences.contains(path)) {
          occurrences.append(path);
        }
      }
    }
    if (occurrences.isEmpty() && bodies.isEmpty()) {
      return Built::failure(QObject::tr("Select the components the step moves."), QStringLiteral("occurrences"));
    }
    const double distance = state.value(QStringLiteral("distance"));
    const double angle = state.value(QStringLiteral("angle"));
    const double seconds = state.value(QStringLiteral("seconds"));
    if (std::isnan(distance) || std::isnan(angle)) {
      return Built::asking(QObject::tr("Give a distance or an angle."), QStringLiteral("distance"));
    }
    if (!(seconds > 0)) {
      return Built::failure(QObject::tr("The time is a number of seconds more than 0."), QStringLiteral("seconds"));
    }
    const bool moves = std::abs(distance) > 1e-12;
    const bool turns = std::abs(angle) > 1e-12;
    if (!moves && !turns) {
      return Built::failure(QObject::tr("Give a distance or an angle."), QStringLiteral("distance"));
    }
    QJsonObject step{{QStringLiteral("occurrences"), occurrences},
                     {QStringLiteral("bodies"), bodies},
                     {QStringLiteral("trails"), state.checked(QStringLiteral("trails"))},
                     {QStringLiteral("seconds"), seconds}};
    if (moves) {
      const auto direction = stepAxis(state, QStringLiteral("direction"), context);
      if (!direction) {
        return Built::failure(QObject::tr("Pick the geometry it moves along."), QStringLiteral("direction_geometry"));
      }
      step.insert(QStringLiteral("distance"), state.expression(QStringLiteral("distance")));
      step.insert(QStringLiteral("direction"), *direction);
    } else {
      step.insert(QStringLiteral("move"), false);
    }
    if (turns) {
      const auto axis = stepAxis(state, QStringLiteral("axis"), context);
      if (!axis) {
        return Built::failure(QObject::tr("Pick the geometry it turns about."), QStringLiteral("axis_geometry"));
      }
      step.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
      step.insert(QStringLiteral("axis"), *axis);
    } else {
      step.insert(QStringLiteral("turn"), false);
    }
    const QString index = state.text(QStringLiteral("step"));
    QJsonObject command;
    QString label;
    if (view.isEmpty()) {
      step.remove(QStringLiteral("move"));
      step.remove(QStringLiteral("turn"));
      command = {{QStringLiteral("cmd"), QStringLiteral("add_explode_view")}, {QStringLiteral("steps"), QJsonArray{step}}};
      label = QStringLiteral("Add Exploded View");
    } else if (index.isEmpty() || index.toInt() < 0) {
      step.remove(QStringLiteral("move"));
      step.remove(QStringLiteral("turn"));
      command = {{QStringLiteral("cmd"), QStringLiteral("edit_explode_view")},
                 {QStringLiteral("name"), view},
                 {QStringLiteral("add_step"), step}};
      label = QStringLiteral("Edit %1").arg(view);
    } else {
      step.insert(QStringLiteral("name"), state.text(QStringLiteral("step_name")));
      command = {{QStringLiteral("cmd"), QStringLiteral("edit_explode_view")},
                 {QStringLiteral("name"), view},
                 {QStringLiteral("step"), index.toInt()},
                 {QStringLiteral("set"), step}};
      label = QStringLiteral("Edit %1").arg(view);
    }
    Built built;
    built.def = {{QStringLiteral("commands"), QJsonArray{command}}, {QStringLiteral("label"), label}};
    return built;
  };
  // `{"view": <name>, "step": <index or -1>}`, with the step as the
  // `explode_views` query gives it when one is edited.
  def.load = [](const QJsonObject& entry, CommandState& state, const CommandContext& context) {
    const QString view = str(entry, "view");
    state.setText(QStringLiteral("view"), view);
    const int index = entry.value(QStringLiteral("step")).toInt(-1);
    state.setText(QStringLiteral("step"), QString::number(index));
    const QJsonObject step = entry.value(QStringLiteral("entry")).toObject();
    if (index < 0 || step.isEmpty()) {
      return;
    }
    const QString prefix = placingPath(viewComponent(state, context), context);
    state.setText(QStringLiteral("step_name"), str(step, "own_name"));
    Selection items;
    for (const QJsonValue& value : step.value(QStringLiteral("occurrences")).toArray()) {
      SelectionItem item{SelectKind::Component, QString(), QString(), QString()};
      const QString path = str(value.toObject(), "path");
      item.occurrence = prefix.isEmpty() ? path : prefix + QLatin1Char('/') + path;
      items.append(item);
    }
    for (const QJsonValue& value : step.value(QStringLiteral("bodies")).toArray()) {
      items.append(SelectionItem{SelectKind::Body, str(value.toObject(), "body"), QString(), QString()});
    }
    state.setItems(QStringLiteral("occurrences"), items);
    const QJsonObject direction = step.value(QStringLiteral("direction")).toObject();
    if (!direction.isEmpty()) {
      loadDirection(direction, QStringLiteral("direction"), state, prefix, context);
      state.setText(QStringLiteral("distance"), str(step, "distance"));
    }
    const QJsonObject axis = step.value(QStringLiteral("axis")).toObject();
    if (!axis.isEmpty()) {
      loadDirection(axis, QStringLiteral("axis"), state, prefix, context);
      state.setText(QStringLiteral("angle"), str(step, "angle"));
    }
    state.setChecked(QStringLiteral("trails"), step.value(QStringLiteral("trails")).toBool(true));
    state.setText(QStringLiteral("seconds"), QString::number(step.value(QStringLiteral("seconds")).toDouble(1.0)));
  };
  def.describe = [](const CommandState& state, const CommandContext& context) {
    QString view = state.text(QStringLiteral("view"));
    const QJsonArray views = explodeViewsOf(context);
    if (view.isEmpty() && !views.isEmpty()) {
      view = str(views.last().toObject(), "name");
    }
    const QJsonObject entry = explodeViewOf(view, context);
    QStringList steps;
    for (const QJsonValue& value : entry.value(QStringLiteral("steps")).toArray()) {
      const QJsonObject step = value.toObject();
      QStringList names;
      for (const QJsonValue& occurrence : step.value(QStringLiteral("occurrences")).toArray()) {
        names << str(occurrence.toObject(), "name");
      }
      steps << QStringLiteral("%1 (%2)").arg(str(step, "name"), names.join(QStringLiteral(", ")));
    }
    return QStringLiteral("Explode view %1: %2").arg(view, steps.join(QStringLiteral("; ")));
  };
  return def;
}

} // namespace cmd
} // namespace mitcad
