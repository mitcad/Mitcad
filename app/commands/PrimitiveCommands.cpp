// SPDX-License-Identifier: MIT
// New Component and the primitives Box, Cylinder, Sphere and Torus (U4;
// commands.md, "box, cylinder, sphere, torus" and "Components and
// occurrences (F6)"). A primitive is placed on a plane or planar face, at
// the point clicked on it or typed in its coordinates.
#include <cmath>

#include <QJsonArray>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

enum class Primitive { Box, Cylinder, Sphere, Torus };

// The plane and the point on it: a box's corner, the others' centre.
QVector<InputDef> placementInputs(Primitive primitive) {
  const bool box = primitive == Primitive::Box;
  return {
      selectionInput(QStringLiteral("plane"), QObject::tr("Plane"), kPlaneKinds, 1, 1)
          .withTooltip(box ? QObject::tr("A plane or planar face; a click places the corner")
                           : QObject::tr("A plane or planar face; a click places the centre"))
          .withAccepts(isPlanar)
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            placeAt(item, state, model, QStringLiteral("x"), QStringLiteral("y"));
          }),
      valueInput(QStringLiteral("x"), box ? QObject::tr("Corner X") : QObject::tr("Center X"),
                 ValueKind::Length, QStringLiteral("0 mm")),
      valueInput(QStringLiteral("y"), box ? QObject::tr("Corner Y") : QObject::tr("Center Y"),
                 ValueKind::Length, QStringLiteral("0 mm")),
  };
}

// An arrow on the plane from the placed point: along the normal (heights)
// or along the plane's x axis (diameters, at half their value).
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
handle(bool alongNormal, double factor, double dx = 0.0, double dy = 0.0) {
  return [=](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
    auto at = pointOnPlane(state, model, QStringLiteral("plane"), QStringLiteral("x"), QStringLiteral("y"));
    if (!at) {
      return std::nullopt;
    }
    const auto frame = frameOf(state.items(QStringLiteral("plane")).first(), model);
    if (!frame) {
      return std::nullopt;
    }
    // A box's height arrow stands on the middle of its base.
    const double length = state.value(QStringLiteral("length"));
    const double width = state.value(QStringLiteral("width"));
    if (dx != 0.0 && std::isfinite(length) && std::isfinite(width)) {
      at->Translate(gp_Vec(frame->x) * length * dx + gp_Vec(frame->y) * width * dy);
    }
    Manipulator m = arrow(*at, alongNormal ? frame->normal : frame->x);
    m.factor = factor;
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
  def.featureType = type;
  def.keywords = {QStringLiteral("primitive")};
  def.inputs = placementInputs(kind);
  const auto direction = choiceInput(QStringLiteral("direction"), QObject::tr("Direction"),
                                     {{QStringLiteral("one_side"), QStringLiteral("One Side")},
                                      {QStringLiteral("symmetric"), QStringLiteral("Symmetric")}},
                                     QStringLiteral("one_side"));
  switch (kind) {
  case Primitive::Box:
    def.name = QObject::tr("Box");
    def.tooltip = QObject::tr("A box on a plane: corner, length, width and height");
    def.keywords << QStringLiteral("cube") << QStringLiteral("block");
    def.inputs += {
        valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, QStringLiteral("30 mm"))
            .withManipulator(handle(false, 1.0)),
        valueInput(QStringLiteral("width"), QObject::tr("Width"), ValueKind::Length, QStringLiteral("20 mm")),
        valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("10 mm"))
            .withManipulator(handle(true, 1.0, 0.5, 0.5)),
        direction,
    };
    break;
  case Primitive::Cylinder:
    def.name = QObject::tr("Cylinder");
    def.tooltip = QObject::tr("A cylinder standing on a plane");
    def.inputs += {
        valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("20 mm"))
            .withManipulator(handle(false, 0.5)),
        valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("30 mm"))
            .withManipulator(handle(true, 1.0)),
        direction,
    };
    break;
  case Primitive::Sphere:
    def.name = QObject::tr("Sphere");
    def.tooltip = QObject::tr("A sphere centred on a plane");
    def.inputs += {
        valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("30 mm"))
            .withManipulator(handle(false, 0.5)),
    };
    break;
  case Primitive::Torus:
    def.name = QObject::tr("Torus");
    def.tooltip = QObject::tr("A ring on a plane");
    def.keywords << QStringLiteral("ring") << QStringLiteral("donut");
    def.inputs += {
        valueInput(QStringLiteral("diameter"), QObject::tr("Inner Diameter"), ValueKind::Length, QStringLiteral("40 mm"))
            .withTooltip(QObject::tr("The diameter the section lies on, as Torus Position says"))
            .withManipulator(handle(false, 0.5)),
        valueInput(QStringLiteral("section"), QObject::tr("Torus Diameter"), ValueKind::Length, QStringLiteral("10 mm")),
        choiceInput(QStringLiteral("position"), QObject::tr("Torus Position"),
                    {{QStringLiteral("inside"), QStringLiteral("Inside")},
                     {QStringLiteral("on_center"), QStringLiteral("On Center")},
                     {QStringLiteral("outside"), QStringLiteral("Outside")}},
                    QStringLiteral("on_center")),
    };
    break;
  }
  def.inputs += operationInputs();

  def.build = [kind, type](const CommandState& state, const CommandContext&) {
    Built result;
    const QJsonArray at{state.value(QStringLiteral("x")), state.value(QStringLiteral("y"))};
    result.def = {{QStringLiteral("type"), type},
                  {QStringLiteral("plane"), state.items(QStringLiteral("plane")).first().reference()},
                  {kind == Primitive::Box ? QStringLiteral("corner") : QStringLiteral("center"), at}};
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

  def.load = [kind](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("plane"), {itemOf(feature.value(QStringLiteral("plane")), context)});
    const QJsonArray at = feature.value(kind == Primitive::Box ? QStringLiteral("corner") : QStringLiteral("center")).toArray();
    state.setText(QStringLiteral("x"), lengthText(at.at(0).toDouble(), context));
    state.setText(QStringLiteral("y"), lengthText(at.at(1).toDouble(), context));
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

  // A fixed plane (the importer's placement) has no input.
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("plane")), context);
  };

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
  def.group = QStringLiteral("CREATE");
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
          .withTooltip(QObject::tr("Empty: Component1, Component2, ..."))
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
