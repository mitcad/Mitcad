// SPDX-License-Identifier: MIT
// Plastic Feature (mitcad#459; commands.md, "Plastic part features"): one
// panel for Boss, Lip, Groove, Snap Fit, Rest and Plastic Rule, chosen by
// its Type. A boss, snap fit or rest stands where the face was clicked (its
// point, which can be typed); a lip or groove follows the picked rim face.
// Values left at 0 are left out, so that the model's defaults apply (the
// plastic rule's draft and clearance).
#include <cmath>
#include <limits>

#include <QJsonArray>

#include "../framework/LengthUnits.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const Choices kTypes = {{QStringLiteral("boss"), QStringLiteral("Boss")},
                        {QStringLiteral("lip"), QStringLiteral("Lip")},
                        {QStringLiteral("groove"), QStringLiteral("Groove")},
                        {QStringLiteral("snap_fit"), QStringLiteral("Snap Fit")},
                        {QStringLiteral("rest"), QStringLiteral("Rest")},
                        {QStringLiteral("plastic_rule"), QStringLiteral("Plastic Rule")}};

const Choices kDirections = {{QStringLiteral("x"), QStringLiteral("+X")}, {QStringLiteral("-x"), QStringLiteral("-X")},
                             {QStringLiteral("y"), QStringLiteral("+Y")}, {QStringLiteral("-y"), QStringLiteral("-Y")},
                             {QStringLiteral("z"), QStringLiteral("+Z")}, {QStringLiteral("-z"), QStringLiteral("-Z")}};

QJsonArray directionOf(const QString& choice) {
  const double sign = choice.startsWith(QLatin1Char('-')) ? -1.0 : 1.0;
  const QChar axis = choice.back();
  return QJsonArray{axis == QLatin1Char('x') ? sign : 0.0, axis == QLatin1Char('y') ? sign : 0.0,
                    axis == QLatin1Char('z') ? sign : 0.0};
}

QString choiceOf(const QJsonArray& v) {
  for (int i = 0; i < 3; ++i) {
    const double c = v.at(i).toDouble();
    if (std::abs(std::abs(c) - 1.0) < 1e-9) {
      const QString axis = QString(QLatin1Char("xyz"[i]));
      return c < 0.0 ? QStringLiteral("-") + axis : axis;
    }
  }
  return QString();
}

std::function<bool(const CommandState&)> typeIn(QStringList types) {
  return [types](const CommandState& state) { return types.contains(state.choice(QStringLiteral("type"))); };
}

const char* const kRuleFields[] = {"thickness", "nominal_radius", "clearance", "knife_edge", "reveal_height",
                                   "thickness_variation", "draft_angle", "min_draft_angle", "min_thickness",
                                   "max_thickness"};

// The plastic rules before the feature: the timeline's plastic_rule
// features not rolled back, by uid.
Choices rulesOf(const CommandContext& context) {
  Choices rules = {{QString(), QStringLiteral("None")}};
  const QJsonObject timeline = context.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
  for (const QJsonValue& value : timeline.value(QStringLiteral("features")).toArray()) {
    const QJsonObject f = value.toObject();
    if (f.value(QStringLiteral("type")).toString() == QStringLiteral("plastic_rule") &&
        f.value(QStringLiteral("status")).toString() != QStringLiteral("rolled_back")) {
      rules.append({f.value(QStringLiteral("uid")).toString(), f.value(QStringLiteral("name")).toString()});
    }
  }
  return rules;
}

// The sizes a new feature of the chosen type starts with, in the design's
// unit (mitcad#1165).
void typeDefaults(CommandState& state, const CommandContext& context) {
  const QString unit = context.lengthUnit();
  const QString type = state.choice(QStringLiteral("type"));
  QVector<QPair<QString, QString>> values;
  if (type == QStringLiteral("boss")) {
    values = {{QStringLiteral("height"), QStringLiteral("10 mm")}};
  } else if (type == QStringLiteral("lip")) {
    values = {{QStringLiteral("width"), QStringLiteral("1 mm")}, {QStringLiteral("height"), QStringLiteral("2 mm")}};
  } else if (type == QStringLiteral("groove")) {
    values = {{QStringLiteral("width"), QStringLiteral("1 mm")}, {QStringLiteral("depth"), QStringLiteral("2 mm")}};
  } else if (type == QStringLiteral("snap_fit")) {
    values = {{QStringLiteral("width"), QStringLiteral("6 mm")}, {QStringLiteral("length"), QStringLiteral("12 mm")}};
  } else if (type == QStringLiteral("rest")) {
    values = {{QStringLiteral("width"), QStringLiteral("8 mm")},
              {QStringLiteral("length"), QStringLiteral("12 mm")},
              {QStringLiteral("height"), QStringLiteral("3 mm")}};
  }
  for (const auto& [key, text] : values) {
    state.setText(key, defaultLengthText(text, unit));
    state.setValue(key, std::numeric_limits<double>::quiet_NaN());
  }
}

} // namespace

CommandDef plasticCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.plastic");
  def.name = QObject::tr("Plastic Feature");
  def.icon = QStringLiteral("plastic");
  def.tooltip = QObject::tr("A boss, lip, groove, snap fit or rest on a face, or a plastic rule");
  def.group = QStringLiteral("CREATE");
  def.section = QStringLiteral("faces");
  def.keywords = {QStringLiteral("boss"),     QStringLiteral("lip"),  QStringLiteral("groove"),
                  QStringLiteral("snap fit"), QStringLiteral("rest"), QStringLiteral("molded"),
                  QStringLiteral("plastic rule")};
  def.featureType = QStringLiteral("boss");
  def.editsAlso = {QStringLiteral("lip"), QStringLiteral("groove"), QStringLiteral("snap_fit"),
                   QStringLiteral("rest"), QStringLiteral("plastic_rule")};
  const auto onFace = typeIn({QStringLiteral("boss"), QStringLiteral("lip"), QStringLiteral("groove"),
                              QStringLiteral("snap_fit"), QStringLiteral("rest")});
  const auto atPoint = typeIn({QStringLiteral("boss"), QStringLiteral("snap_fit"), QStringLiteral("rest")});
  const auto boss = typeIn({QStringLiteral("boss")});
  const auto band = typeIn({QStringLiteral("lip"), QStringLiteral("groove")});
  const auto groove = typeIn({QStringLiteral("groove")});
  const auto snap = typeIn({QStringLiteral("snap_fit")});
  const auto rest = typeIn({QStringLiteral("rest")});
  const auto rule = typeIn({QStringLiteral("plastic_rule")});
  const auto ruled = typeIn({QStringLiteral("boss"), QStringLiteral("groove"), QStringLiteral("rest")});
  const auto height = typeIn({QStringLiteral("boss"), QStringLiteral("lip"), QStringLiteral("rest")});
  const auto width = typeIn({QStringLiteral("lip"), QStringLiteral("groove"), QStringLiteral("snap_fit"),
                             QStringLiteral("rest")});
  const auto draft = typeIn({QStringLiteral("boss"), QStringLiteral("rest")});
  const auto more = [rule](const CommandState& s) { return rule(s) && s.checked(QStringLiteral("more")); };
  const auto rectangle = [rest](const CommandState& s) {
    return rest(s) && s.choice(QStringLiteral("shape")) == QStringLiteral("rectangle");
  };
  const auto length = [](const char* text) { return QString::fromLatin1(text); };
  def.inputs = {
      choiceInput(QStringLiteral("type"), QObject::tr("Type"), kTypes, QStringLiteral("boss"))
          .withOnChange([](CommandState& state, const CommandContext& context) { typeDefaults(state, context); }),
      selectionInput(QStringLiteral("face"), QObject::tr("Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The face it stands on (a click places it); for a lip or groove the rim face "
                                   "round the opening"))
          .withoutMeshes()
          .withVisible(onFace)
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            if (!item.at) {
              return;
            }
            const char* axes[] = {"x", "y", "z"};
            for (int i = 0; i < 3; ++i) {
              const QString key = QString::fromLatin1(axes[i]);
              state.setText(key, lengthText(std::round((*item.at)[static_cast<std::size_t>(i)] * 100.0) / 100.0, model));
              state.setValue(key, std::numeric_limits<double>::quiet_NaN());
            }
          }),
      valueInput(QStringLiteral("x"), QObject::tr("Point X"), ValueKind::Length, length("0 mm")).withVisible(atPoint),
      valueInput(QStringLiteral("y"), QObject::tr("Point Y"), ValueKind::Length, length("0 mm")).withVisible(atPoint),
      valueInput(QStringLiteral("z"), QObject::tr("Point Z"), ValueKind::Length, length("0 mm")).withVisible(atPoint),
      // Boss.
      valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, length("6 mm")).withStepMinimum(0.0, true)
          .withVisible(boss),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, length("10 mm")).withStepMinimum(0.0, true)
          .withVisible(height),
      valueInput(QStringLiteral("hole_diameter"), QObject::tr("Hole Diameter"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("0: no hole"))
          .withVisible(boss),
      valueInput(QStringLiteral("hole_depth"), QObject::tr("Hole Depth"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("From the top; 0: down to the face"))
          .withVisible(boss),
      valueInput(QStringLiteral("fillet"), QObject::tr("Root Fillet"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withVisible(boss),
      valueInput(QStringLiteral("ribs"), QObject::tr("Ribs"), ValueKind::Unitless, length("0")).withVisible(boss).withStepMinimum(0.0),
      valueInput(QStringLiteral("rib_thickness"), QObject::tr("Rib Thickness"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("0: 0.3 times the boss's radius"))
          .withVisible(boss),
      valueInput(QStringLiteral("rib_length"), QObject::tr("Rib Length"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("Out of the boss's wall; 0: its radius"))
          .withVisible(boss),
      valueInput(QStringLiteral("rib_offset"), QObject::tr("Rib Offset"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("The ribs' tops below the boss's top"))
          .withVisible(boss),
      // Rest.
      choiceInput(QStringLiteral("shape"), QObject::tr("Shape"),
                  {{QStringLiteral("round"), QStringLiteral("Round")},
                   {QStringLiteral("rectangle"), QStringLiteral("Rectangle")}},
                  QStringLiteral("round"))
          .withVisible(rest),
      // Lip, groove, snap fit and rest.
      valueInput(QStringLiteral("width"), QObject::tr("Width"), ValueKind::Length, length("1 mm")).withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("A round rest's diameter"))
          .withVisible(width),
      valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length, length("12 mm")).withStepMinimum(0.0, true)
          .withVisible([snap, rectangle](const CommandState& s) { return snap(s) || rectangle(s); }),
      choiceInput(QStringLiteral("along"), QObject::tr("Length Along"), kDirections, QStringLiteral("x"))
          .withVisible(rectangle),
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, length("2 mm")).withVisible(groove).withStepMinimum(0.0, true),
      valueInput(QStringLiteral("gap"), QObject::tr("Gap"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("Added to the width and depth; 0: the plastic rule's clearance"))
          .withVisible(groove),
      choiceInput(QStringLiteral("side"), QObject::tr("Side"),
                  {{QStringLiteral("inside"), QStringLiteral("Along the Opening")},
                   {QStringLiteral("outside"), QStringLiteral("Along the Outer Edge")}},
                  QStringLiteral("inside"))
          .withVisible(band),
      // Snap fit.
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, length("1.5 mm")).withStepMinimum(0.0, true)
          .withVisible(snap),
      valueInput(QStringLiteral("wall"), QObject::tr("Wall Thickness"), ValueKind::Length, length("0 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("0: a solid rest"))
          .withVisible(rest),
      valueInput(QStringLiteral("catch_depth"), QObject::tr("Catch Depth"), ValueKind::Length, length("1 mm")).withStepMinimum(0.0, true)
          .withVisible(snap),
      valueInput(QStringLiteral("catch_length"), QObject::tr("Catch Length"), ValueKind::Length, length("2 mm")).withStepMinimum(0.0, true)
          .withVisible(snap),
      choiceInput(QStringLiteral("hook"), QObject::tr("Catch Toward"), kDirections, QStringLiteral("x"))
          .withVisible(snap),
      valueInput(QStringLiteral("draft"), QObject::tr("Draft Angle"), ValueKind::Angle, length("0 deg"))
          .withTooltip(QObject::tr("0: the plastic rule's draft"))
          .withVisible(draft),
      choiceInput(QStringLiteral("rule"), QObject::tr("Plastic Rule"), {{QString(), QStringLiteral("None")}}, QString())
          .withChoices([&context](const CommandState&) { return rulesOf(context); })
          .withVisible(ruled),
      // Plastic rule.
      textInput(QStringLiteral("rule_name"), QObject::tr("Name"), QStringLiteral("ABS (1.5mm)")).withVisible(rule),
      textInput(QStringLiteral("material"), QObject::tr("Material"), QStringLiteral("ABS Plastic")).withVisible(rule),
      valueInput(QStringLiteral("thickness_rule"), QObject::tr("Nominal Wall"), ValueKind::Length, length("1.5 mm")).withStepMinimum(0.0, true)
          .withVisible(rule),
      valueInput(QStringLiteral("nominal_radius"), QObject::tr("Nominal Radius"), ValueKind::Length, length("0.75 mm"))
          .withVisible(rule),
      valueInput(QStringLiteral("clearance"), QObject::tr("Clearance"), ValueKind::Length, length("0.1 mm"))
          .withVisible(rule),
      valueInput(QStringLiteral("draft_angle"), QObject::tr("Draft"), ValueKind::Angle, length("2 deg"))
          .withVisible(rule),
      valueInput(QStringLiteral("min_draft_angle"), QObject::tr("Smallest Draft"), ValueKind::Angle, length("0.5 deg"))
          .withVisible(rule),
      valueInput(QStringLiteral("min_thickness"), QObject::tr("Smallest Wall"), ValueKind::Length, length("1.2 mm"))
          .withVisible(rule),
      valueInput(QStringLiteral("max_thickness"), QObject::tr("Largest Wall"), ValueKind::Length, length("3.5 mm"))
          .withVisible(rule),
      checkInput(QStringLiteral("more"), QObject::tr("More"), false).withDisclosure().withVisible(rule),
      valueInput(QStringLiteral("knife_edge"), QObject::tr("Knife Edge"), ValueKind::Length, length("1 mm"))
          .withVisible(more),
      valueInput(QStringLiteral("reveal_height"), QObject::tr("Reveal Height"), ValueKind::Length, length("0.5 mm"))
          .withVisible(more),
      valueInput(QStringLiteral("thickness_variation"), QObject::tr("Thickness Variation"), ValueKind::Length,
                 length("0.225 mm"))
          .withVisible(more),
  };
  def.enabled = [&context] { return hasBodies(context); };

  // The newest plastic rule before the feature.
  def.init = [](CommandState& state, const CommandContext& model) {
    const Choices rules = rulesOf(model);
    if (rules.size() > 1) {
      state.setChoice(QStringLiteral("rule"), rules.last().first);
    }
  };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString type = state.choice(QStringLiteral("type"));
    QJsonObject d{{QStringLiteral("type"), type}};
    const auto value = [&](const char* slot, const char* input) {
      d.insert(QString::fromLatin1(slot), state.expression(QString::fromLatin1(input)));
    };
    const auto optional = [&](const char* slot, const char* input) {
      if (nonZero(state, QString::fromLatin1(input))) {
        value(slot, input);
      }
    };
    if (type == QStringLiteral("plastic_rule")) {
      const QString name = state.text(QStringLiteral("rule_name")).trimmed();
      if (name.isEmpty()) {
        return Built::failure(QObject::tr("The rule needs a name."), QStringLiteral("rule_name"));
      }
      d.insert(QStringLiteral("rule_name"), name);
      const QString material = state.text(QStringLiteral("material")).trimmed();
      if (!material.isEmpty()) {
        d.insert(QStringLiteral("material"), material);
      }
      value("thickness", "thickness_rule");
      for (const char* field : kRuleFields) {
        if (QLatin1String(field) != QLatin1String("thickness")) {
          value(field, field);
        }
      }
      result.def = d;
      return result;
    }
    const SelectionItem& face = state.items(QStringLiteral("face")).first();
    const QJsonObject ref = face.reference().toObject();
    d.insert(QStringLiteral("body"), ref.value(QStringLiteral("body")));
    d.insert(QStringLiteral("face"), ref.value(QStringLiteral("face")));
    if (type != QStringLiteral("lip") && type != QStringLiteral("groove")) {
      d.insert(QStringLiteral("point"), QJsonArray{state.value(QStringLiteral("x")), state.value(QStringLiteral("y")),
                                                   state.value(QStringLiteral("z"))});
    }
    if (type == QStringLiteral("boss")) {
      value("diameter", "diameter");
      value("height", "height");
      optional("hole_diameter", "hole_diameter");
      optional("hole_depth", "hole_depth");
      optional("draft_angle", "draft");
      optional("fillet", "fillet");
      if (nonZero(state, QStringLiteral("ribs"))) {
        d.insert(QStringLiteral("ribs"), state.expression(QStringLiteral("ribs")));
      }
      optional("rib_thickness", "rib_thickness");
      optional("rib_length", "rib_length");
      optional("rib_offset", "rib_offset");
    } else if (type == QStringLiteral("lip") || type == QStringLiteral("groove")) {
      value("width", "width");
      if (type == QStringLiteral("lip")) {
        value("height", "height");
      } else {
        value("depth", "depth");
        optional("gap", "gap");
      }
      if (state.choice(QStringLiteral("side")) == QStringLiteral("outside")) {
        d.insert(QStringLiteral("outside"), true);
      }
    } else if (type == QStringLiteral("snap_fit")) {
      d.insert(QStringLiteral("hook"), directionOf(state.choice(QStringLiteral("hook"))));
      value("length", "length");
      value("thickness", "thickness");
      value("width", "width");
      value("catch_depth", "catch_depth");
      value("catch_length", "catch_length");
    } else {
      value("width", "width");
      value("height", "height");
      if (state.choice(QStringLiteral("shape")) == QStringLiteral("rectangle")) {
        value("length", "length");
        d.insert(QStringLiteral("along"), directionOf(state.choice(QStringLiteral("along"))));
      }
      optional("draft_angle", "draft");
      optional("thickness", "wall");
    }
    const QString rule = state.choice(QStringLiteral("rule"));
    if (!rule.isEmpty() &&
        (type == QStringLiteral("boss") || type == QStringLiteral("groove") || type == QStringLiteral("rest"))) {
      d.insert(QStringLiteral("rule"), rule);
    }
    result.def = d;
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString type = feature.value(QStringLiteral("type")).toString();
    state.setChoice(QStringLiteral("type"), type);
    const auto load = [&](const char* input, const char* slot) {
      loadValue(state, QString::fromLatin1(input), feature.value(QLatin1String(slot)), feature, context);
    };
    if (type == QStringLiteral("plastic_rule")) {
      state.setText(QStringLiteral("rule_name"), feature.value(QStringLiteral("rule_name")).toString());
      state.setText(QStringLiteral("material"), feature.value(QStringLiteral("material")).toString());
      load("thickness_rule", "thickness");
      for (const char* field : kRuleFields) {
        if (QLatin1String(field) != QLatin1String("thickness")) {
          load(field, field);
        }
      }
      return;
    }
    state.setItems(QStringLiteral("face"),
                   {itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                       {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                           context)});
    const QJsonArray point = feature.value(QStringLiteral("point")).toArray();
    if (point.size() == 3) {
      const char* axes[] = {"x", "y", "z"};
      for (int i = 0; i < 3; ++i) {
        state.setText(QString::fromLatin1(axes[i]), lengthText(point.at(i).toDouble(), context));
      }
    }
    for (const char* key : {"diameter", "height", "hole_diameter", "hole_depth", "fillet", "rib_thickness",
                            "rib_length", "rib_offset", "width", "length", "depth", "gap", "thickness",
                            "catch_depth", "catch_length"}) {
      load(key, key);
    }
    load("draft", "draft_angle");
    loadCount(state, QStringLiteral("ribs"), feature.value(QStringLiteral("ribs")), feature, context);
    state.setChoice(QStringLiteral("side"), feature.value(QStringLiteral("outside")).toBool() ? QStringLiteral("outside")
                                                                                               : QStringLiteral("inside"));
    if (type == QStringLiteral("rest")) {
      state.setChoice(QStringLiteral("shape"), feature.contains(QStringLiteral("length")) ? QStringLiteral("rectangle")
                                                                                        : QStringLiteral("round"));
      load("wall", "thickness");
    }
    if (const QString along = choiceOf(feature.value(QStringLiteral("along")).toArray()); !along.isEmpty()) {
      state.setChoice(QStringLiteral("along"), along);
    }
    if (const QString hook = choiceOf(feature.value(QStringLiteral("hook")).toArray()); !hook.isEmpty()) {
      state.setChoice(QStringLiteral("hook"), hook);
    }
    state.setChoice(QStringLiteral("rule"), feature.value(QStringLiteral("rule")).toString());
  };

  // What the panel cannot hold: a direction of its own, a direction across
  // the face or a hook that is not along an axis, a face it cannot show.
  def.canEdit = [&context](const QJsonObject& feature) {
    if (feature.value(QStringLiteral("type")).toString() == QStringLiteral("plastic_rule")) {
      return true;
    }
    if (feature.contains(QStringLiteral("direction"))) {
      return false;
    }
    for (const char* key : {"along", "hook"}) {
      if (feature.contains(QLatin1String(key)) && choiceOf(feature.value(QLatin1String(key)).toArray()).isEmpty()) {
        return false;
      }
    }
    return isShowable(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                  {QStringLiteral("face"), feature.value(QStringLiteral("face"))}},
                      context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added plastic feature (%1)").arg(label(kTypes, state.choice(QStringLiteral("type"))));
  };
  return def;
}

} // namespace mitcad::cmd
