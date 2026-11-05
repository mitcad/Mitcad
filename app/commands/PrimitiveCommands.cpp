// SPDX-License-Identifier: MIT
// New Component (the ASSEMBLE group) and the primitives Box, Cylinder,
// Sphere and Torus (the CREATE group; U4;
// commands.md, "box, cylinder, sphere, torus" and "Components and
// occurrences (F6)"). A primitive is placed by a pick (mitcad#656,
// PlaceInputs.hpp): on an axis, at a point, or on a face or a plane at
// distances from two of its edges; a box also turns about its axis.
#include <cmath>

#include <QJsonArray>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "PlaceInputs.hpp"

namespace mitcad::cmd {
namespace {

enum class Primitive { Box, Cylinder, Sphere, Torus };

PlacePanel placePanel(Primitive primitive, const QString& name) {
  PlacePanel panel;
  panel.command = name;
  // A box's turn about its axis matters; a round part's does not.
  if (primitive == Primitive::Box) {
    panel.turns = [](const CommandState&) { return true; };
  }
  return panel;
}

// An arrow from where the part is placed, with its value box (mitcad#656):
// along its axis (heights), its x axis (diameters, at half their value,
// a box's length) or its y axis (a box's width); a box's height from
// the middle of its base, its width from the middle of its length (`dx`,
// `dy`: fractions of its length and width).
enum class Along { Normal, X, Y };
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
handle(const PlacePanel& panel, Along along, double factor, double dx = 0.0, double dy = 0.0) {
  return [=](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
    const auto frame = placeFrame(state, model, panel);
    if (!frame) {
      return std::nullopt;
    }
    gp_Pnt at = frame->origin;
    const double length = state.value(QStringLiteral("length"));
    const double width = state.value(QStringLiteral("width"));
    if ((dx != 0.0 || dy != 0.0) && std::isfinite(length) && std::isfinite(width)) {
      at.Translate(gp_Vec(frame->x) * length * dx + gp_Vec(frame->y) * width * dy);
    }
    Manipulator m = arrow(at, along == Along::Normal ? frame->normal : along == Along::X ? frame->x : frame->y);
    m.factor = factor;
    m.valueBox = true;
    m.boxAtTip = true; // a size reads at its end (mitcad#656)
    return m;
  };
}

CommandDef primitive(const CommandContext& context, Primitive kind) {
  CommandDef def;
  const QString type = kind == Primitive::Box        ? QStringLiteral("box")
                       : kind == Primitive::Cylinder ? QStringLiteral("cylinder")
                       : kind == Primitive::Sphere   ? QStringLiteral("sphere")
                                                     : QStringLiteral("torus");
  def.id = QStringLiteral("solid.") + type;
  def.icon = type;
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("primitives");
  def.featureType = type;
  def.keywords = {QStringLiteral("primitive")};
  def.name = kind == Primitive::Box        ? QObject::tr("Box")
             : kind == Primitive::Cylinder ? QObject::tr("Cylinder")
             : kind == Primitive::Sphere   ? QObject::tr("Sphere")
                                           : QObject::tr("Torus");
  const PlacePanel place = placePanel(kind, def.name);
  def.inputs = placeInputs(place);
  const auto direction = choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                                     {{QStringLiteral("one_side"), QStringLiteral("One Side")},
                                      {QStringLiteral("symmetric"), QStringLiteral("Symmetric")}},
                                     QStringLiteral("one_side"));
  switch (kind) {
  case Primitive::Box:
    def.tooltip = QObject::tr("A box: corner, length, width and height");
    def.keywords << QStringLiteral("cube") << QStringLiteral("block");
    def.inputs += {
        valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("30 mm")).withStepMinimum(0.0, true)
            .withManipulator(handle(place, Along::X, 1.0)),
        valueInput(QStringLiteral("width"), QObject::tr("Width"), ValueKind::Length, QStringLiteral("20 mm")).withStepMinimum(0.0, true)
            .withManipulator(handle(place, Along::Y, 1.0, 0.5, 0.0)),
        valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("10 mm"))
            .withManipulator(handle(place, Along::Normal, 1.0, 0.5, 0.5)),
        direction,
    };
    break;
  case Primitive::Cylinder:
    def.tooltip = QObject::tr("A cylinder standing on a face, an axis or a point");
    def.inputs += {
        valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("20 mm")).withStepMinimum(0.0, true)
            .withManipulator(handle(place, Along::X, 0.5)),
        valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("30 mm"))
            .withManipulator(handle(place, Along::Normal, 1.0)),
        direction,
    };
    break;
  case Primitive::Sphere:
    def.tooltip = QObject::tr("A sphere centered where it is placed");
    def.inputs += {
        valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("30 mm")).withStepMinimum(0.0, true)
            .withManipulator(handle(place, Along::X, 0.5)),
    };
    break;
  case Primitive::Torus:
    def.tooltip = QObject::tr("A ring about where it is placed");
    def.keywords << QStringLiteral("ring") << QStringLiteral("donut");
    def.inputs += {
        valueInput(QStringLiteral("diameter"), QObject::tr("Inner Diameter"), ValueKind::Length, QStringLiteral("40 mm")).withStepMinimum(0.0, true)
            .withManipulator(handle(place, Along::X, 0.5)),
        valueInput(QStringLiteral("section"), QObject::tr("Torus Diameter"), ValueKind::Length, QStringLiteral("10 mm")).withStepMinimum(0.0, true),
        choiceInput(QStringLiteral("position"), QObject::tr("Torus Position"),
                    {{QStringLiteral("inside"), QStringLiteral("Inside")},
                     {QStringLiteral("on_center"), QStringLiteral("On Center")},
                     {QStringLiteral("outside"), QStringLiteral("Outside")}},
                    QStringLiteral("on_center")),
    };
    break;
  }
  def.inputs += operationInputs();
  def.init = [place](CommandState& state, const CommandContext& model) { initPlace(state, model, place); };

  def.build = [kind, type, place](const CommandState& state, const CommandContext& model) {
    Built result;
    result.def = {{QStringLiteral("type"), type}};
    if (!addPlace(result.def, state, model, place, result)) {
      return result;
    }
    switch (kind) {
    case Primitive::Box:
      result.def.insert(QStringLiteral("length"), state.expression(QStringLiteral("length")));
      result.def.insert(QStringLiteral("width"), state.expression(QStringLiteral("width")));
      result.def.insert(QStringLiteral("height"), state.expression(QStringLiteral("height")));
      break;
    case Primitive::Cylinder:
      result.def.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("diameter")));
      result.def.insert(QStringLiteral("height"), state.expression(QStringLiteral("height")));
      break;
    case Primitive::Sphere:
      result.def.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("diameter")));
      break;
    case Primitive::Torus:
      result.def.insert(QStringLiteral("diameter"), state.expression(QStringLiteral("diameter")));
      result.def.insert(QStringLiteral("section_diameter"), state.expression(QStringLiteral("section")));
      result.def.insert(QStringLiteral("position"), state.choice(QStringLiteral("position")));
      break;
    }
    if ((kind == Primitive::Box || kind == Primitive::Cylinder) &&
        state.choice(QStringLiteral("direction")) == QStringLiteral("symmetric")) {
      result.def.insert(QStringLiteral("symmetric"), true);
    }
    addOperation(result.def, state);
    return result;
  };

  def.load = [kind, place](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    // An earlier box's `corner`, the others' `center`, on their plane.
    loadPlace(feature, state, context, place,
              kind == Primitive::Box ? QStringLiteral("corner") : QStringLiteral("center"));
    for (const char* key : {"length", "width", "height", "diameter"}) {
      loadValue(state, QString::fromLatin1(key), feature.value(QLatin1String(key)), feature, context);
    }
    loadValue(state, QStringLiteral("section"), feature.value(QStringLiteral("section_diameter")), feature, context);
    state.setChoice(QStringLiteral("position"), feature.value(QStringLiteral("position")).toString(QStringLiteral("on_center")));
    state.setChoice(QStringLiteral("direction"), feature.value(QStringLiteral("symmetric")).toBool()
                                                     ? QStringLiteral("symmetric")
                                                     : QStringLiteral("one_side"));
    loadOperation(feature, state);
  };

  // A fixed plane (an importer's or an agent's placement) shows as Place
  // On's item, kept until a face or a plane is picked in its place
  // (mitcad#1137); a fixed axis or point still has no input.
  def.canEdit = [&context](const QJsonObject& feature) { return placeShowable(feature, context); };

  def.describe = [type](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added %1 (%2)").arg(type, label(kOperations, state.choice(QStringLiteral("operation"))));
  };
  return def;
}

} // namespace

CommandDef boxCommand(const CommandContext& context) { return primitive(context, Primitive::Box); }
CommandDef cylinderCommand(const CommandContext& context) { return primitive(context, Primitive::Cylinder); }
CommandDef sphereCommand(const CommandContext& context) { return primitive(context, Primitive::Sphere); }
CommandDef torusCommand(const CommandContext& context) { return primitive(context, Primitive::Torus); }

// ---------------------------------------------------------------------------
// New Component

CommandDef newComponentCommand(const CommandContext& context) {
  Q_UNUSED(context);
  CommandDef def;
  def.id = QStringLiteral("solid.new_component");
  def.name = QObject::tr("New Component");
  def.icon = QStringLiteral("component");
  def.tooltip = QObject::tr("An empty component in the active one, or components made of bodies");
  // Registered with CREATE, so that it leads the ASSEMBLE group.
  def.group = QStringLiteral("ASSEMBLE");
  def.pinned = true;
  def.keywords = {QStringLiteral("part"), QStringLiteral("assembly"), QStringLiteral("occurrence")};
  def.modelCommand = true;
  const auto fromBodies = [](const CommandState& s) {
    return s.choice(QStringLiteral("type")) == QStringLiteral("bodies");
  };
  def.inputs = {
      choiceInput(QStringLiteral("type"), QObject::tr("Type"),
                  {{QStringLiteral("empty"), QStringLiteral("Empty Component")},
                   {QStringLiteral("bodies"), QStringLiteral("From Bodies")}},
                  QStringLiteral("empty")),
      textInput(QStringLiteral("name"), QObject::tr("Name"))
          .withTooltip(QObject::tr("Empty: Component1, Component2, …"))
          .withVisible([fromBodies](const CommandState& s) { return !fromBodies(s); }),
      checkInput(QStringLiteral("activate"), QObject::tr("Activate"), true)
          .withTooltip(QObject::tr("New features go into the new component"))
          .withVisible([fromBodies](const CommandState& s) { return !fromBodies(s); }),
      selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("Each body becomes a component of its own, where it is"))
          .withVisible(fromBodies),
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    if (state.choice(QStringLiteral("type")) == QStringLiteral("bodies")) {
      result.def = {{QStringLiteral("commands"),
                     QJsonArray{QJsonObject{{QStringLiteral("cmd"), QStringLiteral("components_from_bodies")},
                                            {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))}}}},
                    {QStringLiteral("label"), QStringLiteral("Create Components from Bodies")}};
      return result;
    }
    // A new empty component has the general defaults, everything allowed
    // (mitcad#421).
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("create_component")},
                        {QStringLiteral("activate"), state.checked(QStringLiteral("activate"))}};
    const QString name = state.text(QStringLiteral("name")).trimmed();
    if (!name.isEmpty()) {
      command.insert(QStringLiteral("name"), name);
    }
    result.def = {{QStringLiteral("commands"), QJsonArray{command}},
                  {QStringLiteral("label"), QStringLiteral("New Component")}};
    return result;
  };
  def.describe = [](const CommandState& state, const CommandContext& model) {
    if (state.choice(QStringLiteral("type")) == QStringLiteral("bodies")) {
      return QStringLiteral("Components from %1 bodies").arg(state.items(QStringLiteral("bodies")).size());
    }
    const QJsonObject components = model.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
    const QJsonArray all = components.value(QStringLiteral("components")).toArray();
    return QStringLiteral("New component %1")
        .arg(all.isEmpty() ? QString() : all.last().toObject().value(QStringLiteral("name")).toString());
  };
  return def;
}

} // namespace mitcad::cmd
