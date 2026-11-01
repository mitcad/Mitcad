// SPDX-License-Identifier: MIT
// Bearing (mitcad#577; commands.md, "Printable bearings"): a ball bearing
// printed in one piece (dumbbell, roller or printed ball elements) or one
// for bought airsoft balls in a snap-in cage, of a standard or custom size,
// on a plane or with its raceway cut into a round face of a body (Groove
// Into). The panel shows the model's `bearing_values`: the pitch diameter,
// the elements, the walls and the least gap, or why the values do not
// print. The sizes come from the model's `bearing_sizes`. Place On
// (mitcad#656, PlaceInputs.hpp) puts it where a pick says; a shaft's end
// edge or side face, or a hole's rim or wall (mitcad#578) puts it on the
// shaft or in the hole, Offset along it (in a hole: how deep), and picks
// the size whose bore fits the shaft (whose outside diameter fits the
// hole) within the Fit Clearance; another size may be chosen after.
#include <cmath>
#include <functional>
#include <memory>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QString>
#include <QStringList>

#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"
#include "PlaceInputs.hpp"

namespace mitcad::cmd {
namespace {

const Choices kElements = {{QStringLiteral("dumbbell"), QStringLiteral("Dumbbell")},
                           {QStringLiteral("roller"), QStringLiteral("Roller")},
                           {QStringLiteral("ball"), QStringLiteral("Ball")},
                           {QStringLiteral("airsoft"), QStringLiteral("Airsoft")}};

const Choices kOperationsMade = {{QStringLiteral("new_component"), QStringLiteral("New Component")},
                                 {QStringLiteral("new_body"), QStringLiteral("New Body")}};

const QString kCustom = QStringLiteral("custom");

bool isChoice(const Choices& choices, const QString& value) {
  for (const auto& choice : choices) {
    if (choice.first == value) {
      return true;
    }
  }
  return false;
}

bool grooved(const CommandState& state) { return !state.items(QStringLiteral("groove")).isEmpty(); }
bool custom(const CommandState& state) { return state.choice(QStringLiteral("size")) == kCustom; }
bool flanged(const CommandState& state) { return state.checked(QStringLiteral("flange")); }

// An empty field leaves the model's choice (the default gap, the most
// elements, the largest element; mitcad#656: Auto). A typed 0 is sent as
// it is, and the model says why it does not print.
bool modelChoice(const CommandState& state, const QString& key) {
  return state.text(key).trimmed().isEmpty() || !std::isfinite(state.value(key));
}

// Placed by a pick (mitcad#656); not with Groove Into. No bore clearance: the size's bore is the bore.
PlacePanel placePanel() {
  PlacePanel panel;
  panel.command = QObject::tr("Bearing");
  panel.offered = [](const CommandState& state) { return !grooved(state); };
  return panel;
}
bool onSeat(const CommandState& state) { return onShaft(state, placePanel()); }

// The size that fits the picked shaft (by its bore) or hole (by its
// outside diameter) within the Fit Clearance, the nearest; the size stays
// when none does or the size is custom.
void fitSize(CommandState& state, const CommandContext& model) {
  if (!onSeat(state) || custom(state)) {
    return;
  }
  try {
    const QJsonObject frame = shaftSeat(state, model, placePanel());
    if (frame.isEmpty() || !frame.value(QStringLiteral("diameter")).isDouble()) {
      return;
    }
    const bool hole = frame.value(QStringLiteral("hole")).toBool();
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("bearing_sizes")},
                      {QStringLiteral("fit"), hole ? QStringLiteral("outside") : QStringLiteral("bore")},
                      {QStringLiteral("diameter"), frame.value(QStringLiteral("diameter"))}};
    if (nonZero(state, QStringLiteral("fit_clearance"))) {
      query.insert(QStringLiteral("clearance"), state.value(QStringLiteral("fit_clearance")));
    }
    const QJsonArray sizes = model.queryObject(query).value(QStringLiteral("sizes")).toArray();
    if (!sizes.isEmpty()) {
      state.setChoice(QStringLiteral("size"), sizes.first().toObject().value(QStringLiteral("designation")).toString());
    }
  } catch (...) {
  }
}

// The standard sizes as the model lists them ("608 (8 x 22 x 7)"), then
// Custom; read once.
const Choices& sizeChoices(const CommandContext& context, const std::shared_ptr<Choices>& cache) {
  if (cache->isEmpty()) {
    QJsonArray sizes;
    try {
      sizes = context.queryObject({{QStringLiteral("query"), QStringLiteral("bearing_sizes")}})
                  .value(QStringLiteral("sizes"))
                  .toArray();
    } catch (...) {
    }
    for (const QJsonValue& value : sizes) {
      const QJsonObject size = value.toObject();
      const QString designation = size.value(QStringLiteral("designation")).toString();
      cache->append({designation, QStringLiteral("%1 (%2 × %3 × %4)")
                                      .arg(designation, number(size.value(QStringLiteral("bore")).toDouble()),
                                           number(size.value(QStringLiteral("outside_diameter")).toDouble()),
                                           number(size.value(QStringLiteral("width")).toDouble()))});
    }
    if (cache->isEmpty()) {
      cache->append({QStringLiteral("608"), QStringLiteral("608 (8 x 22 x 7)")});
    }
    cache->append({kCustom, QObject::tr("Custom")});
  }
  return *cache;
}

// A cylindrical face: the raceway is cut into it.
bool isRound(const SelectionItem& item) {
  return item.kind == SelectKind::Face && item.geometry == QStringLiteral("cylinder");
}

} // namespace

CommandDef bearingCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.bearing");
  def.name = QObject::tr("Bearing");
  def.icon = QStringLiteral("bearing");
  def.tooltip = QObject::tr("A ball bearing printed in one piece, or one for bought airsoft balls");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("machine");
  def.subgroup = QStringLiteral("Machine Elements");
  def.keywords = {QStringLiteral("printable bearing"), QStringLiteral("print in place"),
                  QStringLiteral("ball bearing"),      QStringLiteral("roller bearing"),
                  QStringLiteral("airsoft"),           QStringLiteral("608"),
                  QStringLiteral("race")};
  def.featureType = QStringLiteral("printable_bearing");
  const auto sizes = std::make_shared<Choices>();
  const auto ungrooved = [](const CommandState& state) { return !grooved(state); };
  // The model's values of the definition the panel makes (set once the
  // build is known, below), for the Auto fields.
  const auto worked = std::make_shared<std::function<QJsonObject(const CommandState&, const CommandContext&)>>();
  // What an empty field gets: the model's value of that input.
  const auto workedOut = [worked](const QString& key, bool length) {
    return [worked, key, length](const CommandState& state, const CommandContext& model) -> QString {
      if (!*worked) {
        return QString();
      }
      const QJsonValue value = (*worked)(state, model).value(key);
      if (!value.isDouble()) {
        return QString();
      }
      return length ? lengthText(value.toDouble(), model) : number(value.toDouble());
    };
  };
  def.inputs = {
      choiceInput(QStringLiteral("element"), QObject::tr("Element"), kElements, QStringLiteral("dumbbell")),
      selectionInput(QStringLiteral("groove"), QObject::tr("Groove Into"), SelectKind::Face, 0, 1)
          .withAccepts(isRound)
          .withVisible([](const CommandState& state) { return !onSeat(state); }),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(grooved),
      valueInput(QStringLiteral("fit_clearance"), QObject::tr("Fit Clearance"), ValueKind::Length,
                 QStringLiteral("0.5 mm"))
          .withPrintedFit(QStringLiteral("printable_bearing"))
          .withVisible(onSeat),
      choiceInput(QStringLiteral("size"), QObject::tr("Size"), {}, QStringLiteral("608"))
          .withChoices([&context, sizes](const CommandState&) { return sizeChoices(context, sizes); })
          .withHeading(QObject::tr("Size")),
      valueInput(QStringLiteral("bore"), QObject::tr("Bore"), ValueKind::Length, QStringLiteral("8 mm"))
          .withVisible(custom),
      valueInput(QStringLiteral("outside_diameter"), QObject::tr("Outside Diameter"), ValueKind::Length,
                 QStringLiteral("22 mm"))
          .withVisible(custom),
      valueInput(QStringLiteral("width"), QObject::tr("Width"), ValueKind::Length, QStringLiteral("7 mm"))
          .withVisible(custom),
      checkInput(QStringLiteral("flange"), QObject::tr("Flange"), false),
      valueInput(QStringLiteral("flange_diameter"), QObject::tr("Flange Diameter"), ValueKind::Length,
                 QStringLiteral("25 mm"))
          .withVisible(flanged),
      valueInput(QStringLiteral("flange_thickness"), QObject::tr("Flange Thickness"), ValueKind::Length,
                 QStringLiteral("1.5 mm"))
          .withVisible(flanged),
      valueInput(QStringLiteral("radial_clearance"), QObject::tr("Radial Clearance"), ValueKind::Length,
                 QStringLiteral("0.4 mm"))
          .withPrintedFit(QStringLiteral("printable_bearing"))
          .withHeading(QObject::tr("Printing")),
      valueInput(QStringLiteral("axial_clearance"), QObject::tr("Axial Clearance"), ValueKind::Length,
                 QStringLiteral("0.4 mm"))
          .withPrintedFit(QStringLiteral("printable_bearing")),
      // Left empty, the model's choice (mitcad#656: Auto with the value it
      // works out, from the preview's bearing_values).
      valueInput(QStringLiteral("element_gap"), QObject::tr("Element Gap"), ValueKind::Length, QString())
          .withPrintedFit(QStringLiteral("printable_bearing"))
          .withAuto(workedOut(QStringLiteral("element_gap"), true)),
      valueInput(QStringLiteral("element_count"), QObject::tr("Elements"), ValueKind::Unitless, QString())
          .withAuto(workedOut(QStringLiteral("element_count"), false)),
      valueInput(QStringLiteral("element_diameter"), QObject::tr("Element Diameter"), ValueKind::Length, QString())
          .withAuto(workedOut(QStringLiteral("element_diameter"), true)),
      valueInput(QStringLiteral("chamfer"), QObject::tr("Chamfer"), ValueKind::Length, QStringLiteral("0.4 mm")),
      valueInput(QStringLiteral("min_wall"), QObject::tr("Smallest Wall"), ValueKind::Length, QStringLiteral("1.2 mm")),
      valueInput(QStringLiteral("flank_angle"), QObject::tr("Flank Angle"), ValueKind::Angle, QStringLiteral("45") + QChar(0x00B0)),
      choiceInput(QStringLiteral("operation"), QObject::tr("Operation"), kOperationsMade,
                  QStringLiteral("new_component"))
          .withVisible(ungrooved),
  };
  // Place On and the inputs it brings (mitcad#656) first; a picked shaft
  // or hole also takes the size that fits it.
  {
    QVector<InputDef> placeList = placeInputs(placePanel());
    for (InputDef& input : placeList) {
      if (input.id == QStringLiteral("at")) {
        const auto picked = input.onChange;
        input.onChange = [picked](CommandState& state, const CommandContext& model) {
          if (picked) {
            picked(state, model);
          }
          fitSize(state, model);
        };
      }
    }
    def.inputs = placeList + def.inputs;
  }

  def.init = [](CommandState& state, const CommandContext& model) {
    if (!grooved(state)) {
      initPlace(state, model, placePanel());
    }
  };

  const auto buildBearing = [](const CommandState& state, const CommandContext& model) {
    Built result;
    QJsonObject bearing{{QStringLiteral("type"), QStringLiteral("printable_bearing")},
                        {QStringLiteral("element"), state.choice(QStringLiteral("element"))}};
    if (grooved(state)) {
      bearing.insert(QStringLiteral("groove"), state.items(QStringLiteral("groove")).first().reference());
      if (nonZero(state, QStringLiteral("offset"))) {
        bearing.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
      }
    } else {
      if (!addPlace(bearing, state, model, placePanel(), result)) {
        return result;
      }
      if (onSeat(state)) {
        bearing.insert(QStringLiteral("fit_clearance"), state.expression(QStringLiteral("fit_clearance")));
      }
    }
    if (custom(state)) {
      for (const char* key : {"bore", "outside_diameter", "width"}) {
        bearing.insert(QLatin1String(key), state.expression(QLatin1String(key)));
      }
    } else {
      bearing.insert(QStringLiteral("size"), state.choice(QStringLiteral("size")));
    }
    if (flanged(state)) {
      bearing.insert(QStringLiteral("flange"),
                     QJsonObject{{QStringLiteral("diameter"), state.expression(QStringLiteral("flange_diameter"))},
                                 {QStringLiteral("thickness"), state.expression(QStringLiteral("flange_thickness"))}});
    }
    for (const char* key : {"radial_clearance", "axial_clearance", "chamfer", "min_wall", "flank_angle"}) {
      bearing.insert(QLatin1String(key), state.expression(QLatin1String(key)));
    }
    // Empty leaves the model's choice: the default gap, the most
    // elements, the largest element.
    for (const char* key : {"element_gap", "element_diameter"}) {
      if (!modelChoice(state, QLatin1String(key))) {
        bearing.insert(QLatin1String(key), state.expression(QLatin1String(key)));
      }
    }
    if (!modelChoice(state, QStringLiteral("element_count"))) {
      bearing.insert(QStringLiteral("element_count"), state.expression(QStringLiteral("element_count")));
    }
    // A raceway cut into a body stays in the body's component.
    bearing.insert(QStringLiteral("operation"),
                   grooved(state) ? QStringLiteral("new_body") : state.choice(QStringLiteral("operation")));
    result.def = bearing;
    return result;
  };
  def.build = buildBearing;
  *worked = [buildBearing](const CommandState& state, const CommandContext& model) -> QJsonObject {
    Built built = buildBearing(state, model);
    if (!built.error.isEmpty()) {
      return QJsonObject();
    }
    built.def.remove(QStringLiteral("type"));
    try {
      return model.queryObject({{QStringLiteral("query"), QStringLiteral("bearing_values")},
                                {QStringLiteral("def"), built.def}});
    } catch (...) {
      return QJsonObject();
    }
  };

  // The derived values of the definition the preview showed, or why it
  // does not print.
  def.note = [buildBearing](const CommandState& state, const CommandContext& model) -> QString {
    Built built = buildBearing(state, model);
    if (!built.error.isEmpty()) {
      return QString();
    }
    built.def.remove(QStringLiteral("type"));
    QJsonObject values;
    try {
      values = model.queryObject({{QStringLiteral("query"), QStringLiteral("bearing_values")},
                                  {QStringLiteral("def"), built.def}});
    } catch (...) {
      return QString();
    }
    const QString error = values.value(QStringLiteral("error")).toString();
    if (!error.isEmpty()) {
      QString text = error;
      text[0] = text[0].toUpper();
      return text;
    }
    const auto length = [&](const char* key) { return lengthText(values.value(QLatin1String(key)).toDouble(), model); };
    QStringList parts;
    parts << QObject::tr("Pitch diameter %1").arg(length("pitch_diameter"));
    parts << QObject::tr("%1 elements of %2")
                 .arg(values.value(QStringLiteral("element_count")).toInt())
                 .arg(length("element_diameter"));
    QStringList walls;
    for (const char* key : {"inner_wall", "outer_wall"}) {
      if (values.value(QLatin1String(key)).isDouble()) {
        walls << length(key);
      }
    }
    if (!walls.isEmpty()) {
      parts << QObject::tr("walls %1").arg(walls.join(QStringLiteral(" and ")));
    }
    parts << QObject::tr("least gap %1").arg(length("min_gap"));
    // On a shaft or in a hole (mitcad#578), and whether a size fits it.
    if (values.contains(QStringLiteral("seat"))) {
      const bool hole = values.value(QStringLiteral("seat")).toObject().value(QStringLiteral("hole")).toBool();
      parts << (hole ? QObject::tr("in a %1 hole") : QObject::tr("on a %1 shaft")).arg(length("shaft_diameter"));
      if (values.value(QStringLiteral("fits")).toArray().isEmpty()) {
        parts << QObject::tr("no standard size fits it");
      }
    }
    for (const QJsonValue& value : values.value(QStringLiteral("purchased")).toArray()) {
      const QJsonObject item = value.toObject();
      parts << QObject::tr("buy %1 × %2")
                   .arg(item.value(QStringLiteral("quantity")).toInt())
                   .arg(item.value(QStringLiteral("designation")).toString());
    }
    return parts.join(QStringLiteral(", "));
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setChoice(QStringLiteral("element"), feature.value(QStringLiteral("element")).toString(QStringLiteral("dumbbell")));
    const QJsonValue groove = feature.value(QStringLiteral("groove"));
    // Its placement: `at`, a seat, or an earlier plane and centre.
    const bool placed = loadPlace(feature, state, context, placePanel());
    if (placed) {
      state.setItems(QStringLiteral("groove"), {});
      loadValue(state, QStringLiteral("fit_clearance"), feature.value(QStringLiteral("fit_clearance")), feature, context);
    } else if (!groove.isUndefined() && !groove.isNull()) {
      state.setItems(QStringLiteral("groove"), {itemOf(groove, context)});
      loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
    }
    const QString size = feature.value(QStringLiteral("size")).toString();
    state.setChoice(QStringLiteral("size"), size.isEmpty() ? kCustom : size);
    for (const char* key : {"bore", "outside_diameter", "width", "radial_clearance", "axial_clearance", "element_gap",
                            "element_diameter", "chamfer", "min_wall", "flank_angle"}) {
      loadValue(state, QLatin1String(key), feature.value(QLatin1String(key)), feature, context);
    }
    loadCount(state, QStringLiteral("element_count"), feature.value(QStringLiteral("element_count")), feature, context);
    // Left out, or 0 as earlier versions wrote it: Auto (mitcad#656).
    for (const char* key : {"element_gap", "element_diameter", "element_count"}) {
      if (zeroOrNone(feature.value(QLatin1String(key)))) {
        state.setText(QLatin1String(key), QString());
      }
    }
    const QJsonObject flange = feature.value(QStringLiteral("flange")).toObject();
    state.setChecked(QStringLiteral("flange"), !flange.isEmpty());
    loadValue(state, QStringLiteral("flange_diameter"), flange.value(QStringLiteral("diameter")), feature, context);
    loadValue(state, QStringLiteral("flange_thickness"), flange.value(QStringLiteral("thickness")), feature, context);
    state.setChoice(QStringLiteral("operation"),
                    feature.value(QStringLiteral("operation")).toString(QStringLiteral("new_component")));
  };

  // Only what the panel can show: its elements and operations, a standard
  // size it lists, a plane or a face it shows.
  def.canEdit = [&context, sizes](const QJsonObject& feature) {
    if (!isChoice(kElements, feature.value(QStringLiteral("element")).toString(QStringLiteral("dumbbell")))) {
      return false;
    }
    if (!isChoice(kOperationsMade, feature.value(QStringLiteral("operation")).toString(QStringLiteral("new_component")))) {
      return false;
    }
    const QString size = feature.value(QStringLiteral("size")).toString();
    if (!size.isEmpty() && !isChoice(sizeChoices(context, sizes), size)) {
      return false;
    }
    const QJsonValue groove = feature.value(QStringLiteral("groove"));
    if (!groove.isUndefined() && !groove.isNull() && !isShowable(groove, context)) {
      return false;
    }
    // A size the shaft fits is given by the panel (mitcad#578).
    const bool onShaftDef = !feature.value(QStringLiteral("seat")).isUndefined() ||
                            feature.value(QStringLiteral("at")).toObject().contains(QStringLiteral("axis"));
    return placeShowable(feature, context) &&
           (!onShaftDef || !feature.value(QStringLiteral("size")).isUndefined() ||
            !feature.value(QStringLiteral("width")).isUndefined());
  };

  def.describe = [](const CommandState& state, const CommandContext& model) {
    const QString element = label(kElements, state.choice(QStringLiteral("element")));
    if (custom(state)) {
      return QStringLiteral("Added bearing (%1), %2 x %3 x %4")
          .arg(element, lengthText(state.value(QStringLiteral("bore")), model),
               lengthText(state.value(QStringLiteral("outside_diameter")), model),
               lengthText(state.value(QStringLiteral("width")), model));
    }
    QString where;
    if (onSeat(state)) {
      where = QStringLiteral(", on a shaft or in a hole");
    } else if (grooved(state)) {
      where = QStringLiteral(", groove into a face");
    }
    return QStringLiteral("Added bearing (%1), %2%3").arg(element, state.choice(QStringLiteral("size")), where);
  };
  return def;
}

} // namespace mitcad::cmd

namespace mitcad {

void registerBearingCommands(CommandRegistry& registry, const CommandContext& context) {
  registry.add(cmd::bearingCommand(context));
}

} // namespace mitcad
