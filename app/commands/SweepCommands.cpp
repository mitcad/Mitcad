// SPDX-License-Identifier: MIT
// Sweep, Loft, Rib, Web, Coil and Pipe (commands.md, "sweep",
// "loft", "pipe", "coil", "rib, web"), with all of their panel options, and
// Helix (mitcad#27; commands.md, "helix").
#include <algorithm>
#include <cmath>
#include <optional>

#include <QJsonArray>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

const Choices kPathExtents = {{QStringLiteral("full"), QStringLiteral("Full")},
                              {QStringLiteral("partial"), QStringLiteral("Partial")}};

// A path input: sketch curves or edges, with Chain Selection.
QVector<InputDef> pathInputs(const QString& id, const QString& name,
                             std::function<bool(const CommandState&)> shown = {}) {
  QVector<InputDef> inputs = {
      selectionInput(id, name, kPathKinds, 1, 0)
          .withTooltip(QObject::tr("Sketch curves or edges, joined end to end")),
  };
  if (shown) {
    inputs.first().withVisible(shown);
  }
  return inputs;
}

// The partial extent of a path: fractions beyond the profile and before it.
QJsonObject pathExtent(const CommandState& state, bool second) {
  if (state.choice(QStringLiteral("extent")) != QStringLiteral("partial")) {
    return {{QStringLiteral("type"), QStringLiteral("full")}};
  }
  QJsonObject extent{{QStringLiteral("type"), QStringLiteral("partial")},
                     {QStringLiteral("fraction"), state.expression(QStringLiteral("fraction"))}};
  if (second) {
    extent.insert(QStringLiteral("fraction2"), state.expression(QStringLiteral("fraction2")));
  }
  return extent;
}

void loadPathExtent(const QJsonObject& extent, CommandState& state, const QJsonObject& feature,
                    const CommandContext& context) {
  if (str(extent, "type") != QStringLiteral("partial")) {
    state.setChoice(QStringLiteral("extent"), QStringLiteral("full"));
    return;
  }
  state.setChoice(QStringLiteral("extent"), QStringLiteral("partial"));
  loadValue(state, QStringLiteral("fraction"), extent.value(QStringLiteral("fraction")), feature, context);
  loadValue(state, QStringLiteral("fraction2"), extent.value(QStringLiteral("fraction2")), feature, context);
}

bool pathEditable(const QJsonValue& path) {
  const QJsonObject o = path.toObject();
  return o.contains(QStringLiteral("curves")) || o.contains(QStringLiteral("curve")) ||
         o.contains(QStringLiteral("edges"));
}

bool loadPath(const QJsonValue& path, CommandState& state, const QString& id,
              const CommandContext& context) {
  bool chain = false;
  const auto items = pathItems(path, context, &chain);
  if (!items) {
    return false;
  }
  state.setItems(id, *items);
  state.setChecked(QStringLiteral("chain"), chain);
  return true;
}

} // namespace

// ---------------------------------------------------------------------------
// Sweep

CommandDef sweepCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.sweep");
  def.name = QObject::tr("Sweep");
  def.icon = QStringLiteral("sweep");
  def.tooltip = QObject::tr("Sweeps profiles along a path, optionally guided by a rail");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("path"), QStringLiteral("rail")};
  def.featureType = QStringLiteral("sweep");
  const auto rail = choiceIs(QStringLiteral("type"), QStringLiteral("rail"));
  const auto single = choiceIs(QStringLiteral("type"), QStringLiteral("single"));
  const auto partial = choiceIs(QStringLiteral("extent"), QStringLiteral("partial"));
  def.inputs = {
      choiceInput(QStringLiteral("type"), QObject::tr("Type"),
                  {{QStringLiteral("single"), QStringLiteral("Single Path")},
                   {QStringLiteral("rail"), QStringLiteral("Path + Guide Rail")}},
                  QStringLiteral("single")),
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"), SelectKind::Profile, 1, 0),
  };
  def.inputs += pathInputs(QStringLiteral("path"), QObject::tr("Path"));
  def.inputs += pathInputs(QStringLiteral("rail"), QObject::tr("Guide Rail"), rail);
  def.inputs += {
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true)
          .withTooltip(QObject::tr("Also take the curves joined to the selected ones")),
      choiceInput(QStringLiteral("extent"), QObject::tr("Distance"), kPathExtents, QStringLiteral("full")),
      valueInput(QStringLiteral("fraction"), QObject::tr("Distance 1"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The part of the path beyond the profile, 0 to 1"))
          .withVisible(partial),
      valueInput(QStringLiteral("fraction2"), QObject::tr("Distance 2"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The part of the path before the profile, 0 to 1"))
          .withVisible(partial),
      choiceInput(QStringLiteral("orientation"), QObject::tr("Orientation"),
                  {{QStringLiteral("perpendicular"), QStringLiteral("Perpendicular")},
                   {QStringLiteral("parallel"), QStringLiteral("Parallel")}},
                  QStringLiteral("perpendicular"))
          .withVisible(single),
      valueInput(QStringLiteral("taper"), QObject::tr("Taper Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(single),
      valueInput(QStringLiteral("twist"), QObject::tr("Twist Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(single),
      choiceInput(QStringLiteral("scaling"), QObject::tr("Profile Scaling"),
                  {{QStringLiteral("scale"), QStringLiteral("Scale")},
                   {QStringLiteral("stretch"), QStringLiteral("Stretch")},
                   {QStringLiteral("none"), QStringLiteral("None")}},
                  QStringLiteral("scale"))
          .withVisible(rail),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withTooltip(QObject::tr("Run the path the other way")),
  };
  def.inputs += operationInputs();
  def.init = newestProfile;
  def.enabled = [&context] { return hasProfiles(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    if (oneSketch(profiles, result, QStringLiteral("profiles")).isEmpty()) {
      return result;
    }
    const bool chain = state.checked(QStringLiteral("chain"));
    const QJsonValue path = pathOf(state.items(QStringLiteral("path")), chain, result, QStringLiteral("path"));
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("sweep")},
                  {QStringLiteral("profiles"), refsOf(profiles)},
                  {QStringLiteral("path"), path},
                  {QStringLiteral("extent"), pathExtent(state, true)}};
    if (state.choice(QStringLiteral("type")) == QStringLiteral("rail")) {
      const QJsonValue rail = pathOf(state.items(QStringLiteral("rail")), chain, result, QStringLiteral("rail"));
      if (!result.error.isEmpty()) {
        return result;
      }
      result.def.insert(QStringLiteral("guide_rail"), rail);
      result.def.insert(QStringLiteral("profile_scaling"), state.choice(QStringLiteral("scaling")));
    } else {
      result.def.insert(QStringLiteral("orientation"), state.choice(QStringLiteral("orientation")));
      if (nonZero(state, QStringLiteral("taper"))) {
        result.def.insert(QStringLiteral("taper_angle"), state.expression(QStringLiteral("taper")));
      }
      if (nonZero(state, QStringLiteral("twist"))) {
        result.def.insert(QStringLiteral("twist_angle"), state.expression(QStringLiteral("twist")));
      }
    }
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    addOperation(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    loadPath(feature.value(QStringLiteral("path")), state, QStringLiteral("path"), context);
    if (feature.contains(QStringLiteral("guide_rail"))) {
      state.setChoice(QStringLiteral("type"), QStringLiteral("rail"));
      const bool chain = state.checked(QStringLiteral("chain"));
      loadPath(feature.value(QStringLiteral("guide_rail")), state, QStringLiteral("rail"), context);
      state.setChecked(QStringLiteral("chain"), chain || state.checked(QStringLiteral("chain")));
      state.setChoice(QStringLiteral("scaling"),
                      feature.value(QStringLiteral("profile_scaling")).toString(QStringLiteral("scale")));
    }
    state.setChoice(QStringLiteral("orientation"),
                    feature.value(QStringLiteral("orientation")).toString(QStringLiteral("perpendicular")));
    loadValue(state, QStringLiteral("taper"), feature.value(QStringLiteral("taper_angle")), feature, context);
    loadValue(state, QStringLiteral("twist"), feature.value(QStringLiteral("twist_angle")), feature, context);
    loadPathExtent(feature.value(QStringLiteral("extent")).toObject(), state, feature, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    loadOperation(feature, state);
  };

  // A rail with twist or taper (ignored by the model) and a chained path
  // with an unchained rail cannot be shown with one Chain Selection.
  def.canEdit = [](const QJsonObject& feature) {
    if (!pathEditable(feature.value(QStringLiteral("path")))) {
      return false;
    }
    if (feature.contains(QStringLiteral("guide_rail"))) {
      const bool a = feature.value(QStringLiteral("path")).toObject().value(QStringLiteral("chain")).toBool();
      const bool b = feature.value(QStringLiteral("guide_rail")).toObject().value(QStringLiteral("chain")).toBool();
      return pathEditable(feature.value(QStringLiteral("guide_rail"))) && a == b &&
             !feature.contains(QStringLiteral("twist_angle")) && !feature.contains(QStringLiteral("taper_angle"));
    }
    return true;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added sweep (%1) along %2 curve(s)")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))))
        .arg(state.items(QStringLiteral("path")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Loft

namespace {

// End conditions as the first or last section takes them: a direction at a
// sketch profile, tangent and smooth at a face, a sharp or rounded tip at a
// point.
const Choices kProfileEnds = {{QStringLiteral("free"), QStringLiteral("Free")},
                              {QStringLiteral("direction"), QStringLiteral("Direction")}};
const Choices kFaceEnds = {{QStringLiteral("free"), QStringLiteral("Free")},
                           {QStringLiteral("tangent"), QStringLiteral("Tangent")},
                           {QStringLiteral("smooth"), QStringLiteral("Smooth")}};
const Choices kPointEnds = {{QStringLiteral("point_sharp"), QStringLiteral("Point Sharp")},
                            {QStringLiteral("point_tangent"), QStringLiteral("Point Tangent")}};

const QString kRails = QStringLiteral("rails");

QString endKey(bool last, const char* field) {
  return QStringLiteral("%1_%2").arg(last ? QStringLiteral("end") : QStringLiteral("start"),
                                     QLatin1String(field));
}

// The conditions the first (or the last) section takes.
const Choices& endChoices(const CommandState& state, bool last) {
  const Selection& sections = state.items(QStringLiteral("sections"));
  if (sections.isEmpty()) {
    return kProfileEnds;
  }
  const SelectionItem& section = last ? sections.last() : sections.first();
  switch (section.kind) {
  case SelectKind::Profile:
    return kProfileEnds;
  case SelectKind::Face:
    return kFaceEnds;
  default:
    return kPointEnds;
  }
}

// The condition chosen at an end if its section takes it, else empty.
QString endCondition(const CommandState& state, bool last) {
  const QString chosen = state.choice(endKey(last, "condition"));
  for (const auto& [value, label] : endChoices(state, last)) {
    if (value == chosen) {
      return chosen;
    }
  }
  return {};
}

bool imposes(const QString& condition) {
  return !condition.isEmpty() && condition != QStringLiteral("free") && condition != QStringLiteral("point_sharp");
}

// The condition, its takeoff angle and weight at one end.
QVector<InputDef> endInputs(bool last) {
  const auto is = [last](auto test) {
    return [last, test](const CommandState& state) { return test(endCondition(state, last)); };
  };
  return {
      choiceInput(endKey(last, "condition"), last ? QObject::tr("End Condition") : QObject::tr("Start Condition"),
                  kProfileEnds, QStringLiteral("free"))
          .withChoices([last](const CommandState& state) { return endChoices(state, last); })
          .withTooltip(QObject::tr("How the loft leaves the section: a direction from a profile, tangent or "
                                   "smooth to the faces next to a face, a rounded tip at a point")),
      valueInput(endKey(last, "angle"), QObject::tr("Takeoff Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("From the profile's normal, outwards for a positive angle"))
          .withVisible(is([](const QString& c) { return c == QStringLiteral("direction"); })),
      valueInput(endKey(last, "weight"), QObject::tr("Takeoff Weight"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("How far the takeoff reaches: 1 leaves at about the pace the sections are "
                                   "apart (0 to 10)"))
          .withVisible(is([](const QString& c) { return imposes(c); })),
  };
}

} // namespace

CommandDef loftCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.loft");
  def.name = QObject::tr("Loft");
  def.icon = QStringLiteral("loft");
  def.tooltip = QObject::tr("A solid through sections in order: profiles, faces, and points at "
                            "the ends");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("blend"), QStringLiteral("sections"), QStringLiteral("transition")};
  def.featureType = QStringLiteral("loft");
  const auto guide = [](const char* type) { return choiceIs(QStringLiteral("guide"), QString::fromLatin1(type)); };
  def.inputs = {
      selectionInput(QStringLiteral("sections"), QObject::tr("Profiles"),
                     SelectKind::Profile | SelectKind::Face | kPointKinds, 2, 0)
          .withTooltip(QObject::tr("Sections in the order the loft goes through them; points "
                                   "only first or last"))
          .withAccepts([](const SelectionItem& item) {
            return item.kind != SelectKind::Face || item.geometry == QStringLiteral("plane");
          })
          .withOrdered(),
  };
  def.inputs += endInputs(false);
  def.inputs += endInputs(true);
  def.inputs += {
      choiceInput(QStringLiteral("guide"), QObject::tr("Guide Type"),
                  {{QStringLiteral("none"), QStringLiteral("None")},
                   {QStringLiteral("rails"), QStringLiteral("Rails")},
                   {QStringLiteral("centerline"), QStringLiteral("Centerline")}},
                  QStringLiteral("none")),
      listInput(kRails, QObject::tr("Rails"),
                {selectionInput(QStringLiteral("path"), QObject::tr("Rail"), kPathKinds, 1, 0)
                     .withTooltip(QObject::tr("Sketch curves or edges joined end to end, meeting every "
                                              "section"))},
                QObject::tr("Add Rail"), QObject::tr("Rail %1"))
          .withVisible(guide("rails")),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true)
          .withTooltip(QObject::tr("Also take the curves joined to the selected ones"))
          .withVisible(guide("rails")),
      selectionInput(QStringLiteral("centerline"), QObject::tr("Centerline"), kPathKinds, 1, 0)
          .withTooltip(QObject::tr("Sketch curves or edges joined end to end"))
          .withVisible(guide("centerline")),
      checkInput(QStringLiteral("closed"), QObject::tr("Closed"), false)
          .withTooltip(QObject::tr("From the last section back to the first")),
      checkInput(QStringLiteral("ruled"), QObject::tr("Ruled"), false)
          .withTooltip(QObject::tr("Straight between neighbouring sections")),
  };
  def.inputs += operationInputs();
  def.enabled = [&context] { return hasProfiles(context) || hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonArray sections;
    for (const SelectionItem& item : state.items(QStringLiteral("sections"))) {
      switch (item.kind) {
      case SelectKind::Profile:
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("profile")},
                                    {QStringLiteral("sketch"), item.owner},
                                    {QStringLiteral("region"), item.name}});
        break;
      case SelectKind::Face:
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("face")},
                                    {QStringLiteral("body"), item.owner},
                                    {QStringLiteral("face"), item.name}});
        break;
      default:
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("point")},
                                    {QStringLiteral("point"), item.reference()}});
        break;
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("loft")}, {QStringLiteral("sections"), sections}};
    for (const bool last : {false, true}) {
      const QString condition = endCondition(state, last);
      if (!imposes(condition)) {
        continue;
      }
      QJsonObject end{{QStringLiteral("type"), condition},
                      {QStringLiteral("weight"), state.expression(endKey(last, "weight"))}};
      if (condition == QStringLiteral("direction")) {
        end.insert(QStringLiteral("angle"), state.expression(endKey(last, "angle")));
      }
      result.def.insert(endKey(last, "condition"), end);
    }
    const QString guide = state.choice(QStringLiteral("guide"));
    if (guide == QStringLiteral("rails")) {
      const bool chain = state.checked(QStringLiteral("chain"));
      QJsonArray rails;
      for (int i = 0; i < state.rows(kRails); ++i) {
        const QString key = CommandState::rowKey(kRails, i, QStringLiteral("path"));
        rails.append(pathOf(state.row(kRails, i).items(QStringLiteral("path")), chain, result, key));
        if (!result.error.isEmpty()) {
          return result;
        }
      }
      result.def.insert(kRails, rails);
    } else if (guide == QStringLiteral("centerline")) {
      const QJsonValue line = pathOf(state.items(QStringLiteral("centerline")), false, result,
                                     QStringLiteral("centerline"));
      if (!result.error.isEmpty()) {
        return result;
      }
      result.def.insert(QStringLiteral("centerline"), line);
    }
    if (state.checked(QStringLiteral("closed"))) {
      result.def.insert(QStringLiteral("closed"), true);
    }
    if (state.checked(QStringLiteral("ruled"))) {
      result.def.insert(QStringLiteral("ruled"), true);
    }
    addOperation(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    Selection sections;
    for (const QJsonValue& value : feature.value(QStringLiteral("sections")).toArray()) {
      const QJsonObject section = value.toObject();
      const QString type = str(section, "type");
      if (type == QStringLiteral("profile")) {
        sections.append({SelectKind::Profile, str(section, "sketch"), str(section, "region"), QStringLiteral("plane")});
      } else if (type == QStringLiteral("face")) {
        sections.append(itemOf(QJsonObject{{QStringLiteral("body"), section.value(QStringLiteral("body"))},
                                           {QStringLiteral("face"), section.value(QStringLiteral("face"))}},
                               context));
      } else {
        sections.append(itemOf(section.value(QStringLiteral("point")), context));
      }
    }
    state.setItems(QStringLiteral("sections"), sections);
    for (const bool last : {false, true}) {
      const QJsonObject end = feature.value(endKey(last, "condition")).toObject();
      const QString type = str(end, "type");
      if (type.isEmpty()) {
        continue;
      }
      state.setChoice(endKey(last, "condition"), type);
      loadValue(state, endKey(last, "weight"), end.value(QStringLiteral("weight")), feature, context);
      loadValue(state, endKey(last, "angle"), end.value(QStringLiteral("angle")), feature, context);
    }
    const QJsonArray rails = feature.value(kRails).toArray();
    if (!rails.isEmpty()) {
      state.setChoice(QStringLiteral("guide"), QStringLiteral("rails"));
      state.setRows(kRails, static_cast<int>(rails.size()));
      bool chain = false;
      for (int i = 0; i < rails.size(); ++i) {
        bool chained = false;
        if (const auto path = pathItems(rails[i], context, &chained)) {
          state.setItems(CommandState::rowKey(kRails, i, QStringLiteral("path")), *path);
        }
        chain = chain || chained;
      }
      state.setChecked(QStringLiteral("chain"), chain);
    } else if (feature.contains(QStringLiteral("centerline"))) {
      state.setChoice(QStringLiteral("guide"), QStringLiteral("centerline"));
      if (const auto path = pathItems(feature.value(QStringLiteral("centerline")), context)) {
        state.setItems(QStringLiteral("centerline"), *path);
      }
    }
    state.setChecked(QStringLiteral("closed"), feature.value(QStringLiteral("closed")).toBool());
    state.setChecked(QStringLiteral("ruled"), feature.value(QStringLiteral("ruled")).toBool());
    loadOperation(feature, state);
  };

  // Fixed points or paths, and rails chained and not (one Chain Selection
  // holds them), have no inputs.
  def.canEdit = [&context](const QJsonObject& feature) {
    std::optional<bool> chained;
    for (const QJsonValue& rail : feature.value(kRails).toArray()) {
      const bool chain = rail.toObject().value(QStringLiteral("chain")).toBool();
      if (!pathEditable(rail) || !pathItems(rail, context) || (chained && *chained != chain)) {
        return false;
      }
      chained = chain;
    }
    if (feature.contains(QStringLiteral("centerline")) && !pathEditable(feature.value(QStringLiteral("centerline")))) {
      return false;
    }
    for (const QJsonValue& value : feature.value(QStringLiteral("sections")).toArray()) {
      const QJsonObject section = value.toObject();
      if (str(section, "type") == QStringLiteral("point") &&
          !isShowable(section.value(QStringLiteral("point")), context)) {
        return false;
      }
    }
    return true;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added loft (%1) through %2 sections")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))))
        .arg(state.items(QStringLiteral("sections")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Rib and Web

namespace {

CommandDef ribOrWeb(const CommandContext& context, bool web) {
  CommandDef def;
  def.id = web ? QStringLiteral("solid.web") : QStringLiteral("solid.rib");
  def.name = web ? QObject::tr("Web") : QObject::tr("Rib");
  def.icon = web ? QStringLiteral("web") : QStringLiteral("rib");
  def.tooltip = web ? QObject::tr("Thin walls along open sketch curves, grown along the sketch normal")
                    : QObject::tr("A thin wall from an open sketch curve to the faces it reaches");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("stiffener"), QStringLiteral("wall"), QStringLiteral("gusset")};
  def.featureType = web ? QStringLiteral("web") : QStringLiteral("rib");
  def.inputs = {
      selectionInput(QStringLiteral("curves"), QObject::tr("Curves"), SelectKind::SketchCurve, 1, 0)
          .withTooltip(QObject::tr("Open sketch curves of one sketch")),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), false),
      choiceInput(QStringLiteral("location"), QObject::tr("Thickness Options"),
                  {{QStringLiteral("symmetric"), QStringLiteral("Symmetric")},
                   {QStringLiteral("side1"), QStringLiteral("Side 1")},
                   {QStringLiteral("side2"), QStringLiteral("Side 2")}},
                  QStringLiteral("symmetric")),
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("2 mm")),
      choiceInput(QStringLiteral("depth_type"), QObject::tr("Depth Options"),
                  {{QStringLiteral("to_next"), QStringLiteral("To Next")},
                   {QStringLiteral("depth"), QStringLiteral("Depth")}},
                  QStringLiteral("to_next")),
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(choiceIs(QStringLiteral("depth_type"), QStringLiteral("depth"))),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      selectionInput(QStringLiteral("objects"), QObject::tr("Objects"), SelectKind::Body, 0, 0)
          .withTooltip(QObject::tr("The bodies it joins and stops at; none: all")),
  };
  def.enabled = [&context] { return hasSketches(context); };
  const QString type = def.featureType;

  def.build = [type](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& curves = state.items(QStringLiteral("curves"));
    const QString sketch = oneSketch(curves, result, QStringLiteral("curves"));
    if (sketch.isEmpty()) {
      return result;
    }
    QJsonObject path{{QStringLiteral("sketch"), sketch},
                     {QStringLiteral("curves"), names(curves, SelectKind::SketchCurve)}};
    if (state.checked(QStringLiteral("chain"))) {
      path.insert(QStringLiteral("chain"), true);
    }
    QJsonObject extent{{QStringLiteral("type"), state.choice(QStringLiteral("depth_type"))}};
    if (state.choice(QStringLiteral("depth_type")) == QStringLiteral("depth")) {
      extent.insert(QStringLiteral("depth"), state.expression(QStringLiteral("depth")));
    }
    result.def = {{QStringLiteral("type"), type},
                  {QStringLiteral("curves"), path},
                  {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness"))},
                  {QStringLiteral("thickness_location"), state.choice(QStringLiteral("location"))},
                  {QStringLiteral("extent"), extent}};
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    const Selection& objects = state.items(QStringLiteral("objects"));
    if (!objects.isEmpty()) {
      result.def.insert(QStringLiteral("participants"), bodyUids(objects));
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadPath(feature.value(QStringLiteral("curves")), state, QStringLiteral("curves"), context);
    loadValue(state, QStringLiteral("thickness"), feature.value(QStringLiteral("thickness")), feature, context);
    state.setChoice(QStringLiteral("location"),
                    feature.value(QStringLiteral("thickness_location")).toString(QStringLiteral("symmetric")));
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    state.setChoice(QStringLiteral("depth_type"), str(extent, "type"));
    loadValue(state, QStringLiteral("depth"), extent.value(QStringLiteral("depth")), feature, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    state.setItems(QStringLiteral("objects"), bodiesOf(feature.value(QStringLiteral("participants")).toArray()));
  };

  def.describe = [type](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added %1 on %2 curve(s), thickness %3")
        .arg(type)
        .arg(state.items(QStringLiteral("curves")).size())
        .arg(number(state.value(QStringLiteral("thickness"))));
  };
  return def;
}

} // namespace

CommandDef ribCommand(const CommandContext& context) { return ribOrWeb(context, false); }
CommandDef webCommand(const CommandContext& context) { return ribOrWeb(context, true); }

// ---------------------------------------------------------------------------
// Coil

CommandDef coilCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.coil");
  def.name = QObject::tr("Coil");
  def.icon = QStringLiteral("coil");
  def.tooltip = QObject::tr("A helical or spiral coil on a plane");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("spring"), QStringLiteral("helix"), QStringLiteral("spiral")};
  def.featureType = QStringLiteral("coil");
  const auto typeHas = [](const char* field) {
    return [field = QString::fromLatin1(field)](const CommandState& state) {
      return state.choice(QStringLiteral("type")).contains(field) ||
             (field == QStringLiteral("revolutions") && state.choice(QStringLiteral("type")) == QStringLiteral("spiral")) ||
             (field == QStringLiteral("pitch") && state.choice(QStringLiteral("type")) == QStringLiteral("spiral"));
    };
  };
  def.inputs = {
      selectionInput(QStringLiteral("plane"), QObject::tr("Plane"), kPlaneKinds, 1, 1)
          .withTooltip(QObject::tr("A plane or planar face; a click places the centre"))
          .withAccepts(isPlanar)
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext& model) {
            placeAt(item, state, model, QStringLiteral("x"), QStringLiteral("y"));
          }),
      valueInput(QStringLiteral("x"), QObject::tr("Center X"), ValueKind::Length, QStringLiteral("0 mm")),
      valueInput(QStringLiteral("y"), QObject::tr("Center Y"), ValueKind::Length, QStringLiteral("0 mm")),
      choiceInput(QStringLiteral("type"), QObject::tr("Type"),
                  {{QStringLiteral("revolutions_and_height"), QStringLiteral("Revolution and Height")},
                   {QStringLiteral("revolutions_and_pitch"), QStringLiteral("Revolution and Pitch")},
                   {QStringLiteral("height_and_pitch"), QStringLiteral("Height and Pitch")},
                   {QStringLiteral("spiral"), QStringLiteral("Spiral")}},
                  QStringLiteral("revolutions_and_height")),
      valueInput(QStringLiteral("diameter"), QObject::tr("Diameter"), ValueKind::Length, QStringLiteral("40 mm")),
      choiceInput(QStringLiteral("rotation"), QObject::tr("Rotation"),
                  {{QStringLiteral("ccw"), QStringLiteral("Counterclockwise")},
                   {QStringLiteral("cw"), QStringLiteral("Clockwise")}},
                  QStringLiteral("ccw")),
      valueInput(QStringLiteral("revolutions"), QObject::tr("Revolutions"), ValueKind::Unitless, QStringLiteral("3"))
          .withVisible(typeHas("revolutions")),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("30 mm"))
          .withVisible(typeHas("height"))
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            const auto at = pointOnPlane(state, model, QStringLiteral("plane"), QStringLiteral("x"), QStringLiteral("y"));
            if (!at) {
              return std::nullopt;
            }
            const auto frame = frameOf(state.items(QStringLiteral("plane")).first(), model);
            if (!frame) {
              return std::nullopt;
            }
            return arrow(*at, frame->normal);
          }),
      valueInput(QStringLiteral("pitch"), QObject::tr("Pitch"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(typeHas("pitch")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("A cone: positive widens the coil as it rises"))
          .withVisible([](const CommandState& s) { return s.choice(QStringLiteral("type")) != QStringLiteral("spiral"); }),
      choiceInput(QStringLiteral("section"), QObject::tr("Section"),
                  {{QStringLiteral("circular"), QStringLiteral("Circular")},
                   {QStringLiteral("square"), QStringLiteral("Square")},
                   {QStringLiteral("triangular_external"), QStringLiteral("Triangular (External)")},
                   {QStringLiteral("triangular_internal"), QStringLiteral("Triangular (Internal)")}},
                  QStringLiteral("circular")),
      choiceInput(QStringLiteral("position"), QObject::tr("Section Position"),
                  {{QStringLiteral("inside"), QStringLiteral("Inside")},
                   {QStringLiteral("on_center"), QStringLiteral("On Center")},
                   {QStringLiteral("outside"), QStringLiteral("Outside")}},
                  QStringLiteral("on_center")),
      valueInput(QStringLiteral("size"), QObject::tr("Section Size"), ValueKind::Length, QStringLiteral("4 mm")),
  };
  def.inputs += operationInputs();

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString type = state.choice(QStringLiteral("type"));
    QJsonObject helix{{QStringLiteral("type"), type}};
    if (type.contains(QStringLiteral("revolutions")) || type == QStringLiteral("spiral")) {
      helix.insert(QStringLiteral("revolutions"), state.expression(QStringLiteral("revolutions")));
    }
    if (type.contains(QStringLiteral("height"))) {
      helix.insert(QStringLiteral("height"), state.expression(QStringLiteral("height")));
    }
    if (type.contains(QStringLiteral("pitch")) || type == QStringLiteral("spiral")) {
      helix.insert(QStringLiteral("pitch"), state.expression(QStringLiteral("pitch")));
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("coil")},
                  {QStringLiteral("plane"), state.items(QStringLiteral("plane")).first().reference()},
                  {QStringLiteral("center"), QJsonArray{state.value(QStringLiteral("x")), state.value(QStringLiteral("y"))}},
                  {QStringLiteral("diameter"), state.expression(QStringLiteral("diameter"))},
                  {QStringLiteral("helix"), helix},
                  {QStringLiteral("section"), state.choice(QStringLiteral("section"))},
                  {QStringLiteral("section_position"), state.choice(QStringLiteral("position"))},
                  {QStringLiteral("section_size"), state.expression(QStringLiteral("size"))}};
    if (type != QStringLiteral("spiral") && nonZero(state, QStringLiteral("angle"))) {
      result.def.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
    }
    if (state.choice(QStringLiteral("rotation")) == QStringLiteral("cw")) {
      result.def.insert(QStringLiteral("clockwise"), true);
    }
    addOperation(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("plane"), {itemOf(feature.value(QStringLiteral("plane")), context)});
    const QJsonArray center = feature.value(QStringLiteral("center")).toArray();
    state.setText(QStringLiteral("x"), lengthText(center.at(0).toDouble(), context));
    state.setText(QStringLiteral("y"), lengthText(center.at(1).toDouble(), context));
    loadValue(state, QStringLiteral("diameter"), feature.value(QStringLiteral("diameter")), feature, context);
    const QJsonObject helix = feature.value(QStringLiteral("helix")).toObject();
    state.setChoice(QStringLiteral("type"), str(helix, "type"));
    loadValue(state, QStringLiteral("revolutions"), helix.value(QStringLiteral("revolutions")), feature, context);
    loadValue(state, QStringLiteral("height"), helix.value(QStringLiteral("height")), feature, context);
    loadValue(state, QStringLiteral("pitch"), helix.value(QStringLiteral("pitch")), feature, context);
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
    state.setChoice(QStringLiteral("rotation"),
                    feature.value(QStringLiteral("clockwise")).toBool() ? QStringLiteral("cw") : QStringLiteral("ccw"));
    state.setChoice(QStringLiteral("section"), feature.value(QStringLiteral("section")).toString(QStringLiteral("circular")));
    state.setChoice(QStringLiteral("position"),
                    feature.value(QStringLiteral("section_position")).toString(QStringLiteral("on_center")));
    loadValue(state, QStringLiteral("size"), feature.value(QStringLiteral("section_size")), feature, context);
    loadOperation(feature, state);
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("plane")), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added coil (%1), diameter %2")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))),
             number(state.value(QStringLiteral("diameter"))));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Pipe

CommandDef pipeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.pipe");
  def.name = QObject::tr("Pipe");
  def.icon = QStringLiteral("pipe");
  def.tooltip = QObject::tr("A round, square or triangular section swept along a path, solid or hollow");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("tube"), QStringLiteral("wire")};
  def.featureType = QStringLiteral("pipe");
  def.inputs = pathInputs(QStringLiteral("path"), QObject::tr("Path"));
  def.inputs += {
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true),
      choiceInput(QStringLiteral("extent"), QObject::tr("Distance"), kPathExtents, QStringLiteral("full")),
      valueInput(QStringLiteral("fraction"), QObject::tr("Distance 1"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The part of the path from its start, 0 to 1"))
          .withVisible(choiceIs(QStringLiteral("extent"), QStringLiteral("partial"))),
      choiceInput(QStringLiteral("section"), QObject::tr("Section"),
                  {{QStringLiteral("circular"), QStringLiteral("Circular")},
                   {QStringLiteral("square"), QStringLiteral("Square")},
                   {QStringLiteral("triangular"), QStringLiteral("Triangular")}},
                  QStringLiteral("circular")),
      valueInput(QStringLiteral("size"), QObject::tr("Section Size"), ValueKind::Length, QStringLiteral("8 mm")),
      checkInput(QStringLiteral("hollow"), QObject::tr("Hollow"), false),
      valueInput(QStringLiteral("thickness"), QObject::tr("Section Thickness"), ValueKind::Length, QStringLiteral("1 mm"))
          .withVisible([](const CommandState& s) { return s.checked(QStringLiteral("hollow")); }),
  };
  def.inputs += operationInputs();
  def.enabled = [&context] { return hasSketches(context) || hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QJsonValue path =
        pathOf(state.items(QStringLiteral("path")), state.checked(QStringLiteral("chain")), result, QStringLiteral("path"));
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("pipe")},
                  {QStringLiteral("path"), path},
                  {QStringLiteral("section"), state.choice(QStringLiteral("section"))},
                  {QStringLiteral("size"), state.expression(QStringLiteral("size"))},
                  {QStringLiteral("extent"), pathExtent(state, false)}};
    if (state.checked(QStringLiteral("hollow"))) {
      result.def.insert(QStringLiteral("thickness"), state.expression(QStringLiteral("thickness")));
    }
    addOperation(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadPath(feature.value(QStringLiteral("path")), state, QStringLiteral("path"), context);
    state.setChoice(QStringLiteral("section"), feature.value(QStringLiteral("section")).toString(QStringLiteral("circular")));
    loadValue(state, QStringLiteral("size"), feature.value(QStringLiteral("size")), feature, context);
    if (feature.contains(QStringLiteral("thickness"))) {
      state.setChecked(QStringLiteral("hollow"), true);
      loadValue(state, QStringLiteral("thickness"), feature.value(QStringLiteral("thickness")), feature, context);
    }
    loadPathExtent(feature.value(QStringLiteral("extent")).toObject(), state, feature, context);
    loadOperation(feature, state);
  };

  // The part before the start of a closed path has no input.
  def.canEdit = [](const QJsonObject& feature) {
    return pathEditable(feature.value(QStringLiteral("path"))) &&
           !feature.value(QStringLiteral("extent")).toObject().contains(QStringLiteral("fraction2"));
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added pipe (%1), size %2")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))),
             number(state.value(QStringLiteral("size"))));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Helix (mitcad#27): the model's helix holds a pitch and revolutions; a
// height goes in as their ratio ("30 mm / 10 mm" turns, "30 mm / 3" a
// pitch), as the FreeCAD import writes it, and is read back from it.

namespace {

const QString kRevolutionsAndPitch = QStringLiteral("revolutions_and_pitch");
const QString kRevolutionsAndHeight = QStringLiteral("revolutions_and_height");
const QString kHeightAndPitch = QStringLiteral("height_and_pitch");

// Whether "(…)" closes only at its end.
bool closesAtEnd(const QString& text) {
  int depth = 0;
  for (int i = 0; i < text.size(); ++i) {
    if (text[i] == QLatin1Char('(')) {
      ++depth;
    } else if (text[i] == QLatin1Char(')') && --depth == 0 && i + 1 < text.size()) {
      return false;
    }
  }
  return depth == 0;
}

// An expression as an operand of a division: as it is when it is a name, a
// number with its unit or a call, else in parentheses.
QString operand(const QString& expression) {
  const QString t = expression.trimmed();
  const auto word = [](const QString& w) {
    return !w.isEmpty() && std::all_of(w.begin(), w.end(), [](QChar c) {
             return c.isLetterOrNumber() || c == QLatin1Char('_') || c == QLatin1Char('.');
           });
  };
  const QStringList parts = t.split(QLatin1Char(' '), Qt::SkipEmptyParts);
  bool number = false;
  if (parts.size() == 2) {
    const double value = parts[0].toDouble(&number);
    number = number && std::isfinite(value);
  }
  const bool simple = word(t) || (number && (word(parts[1]) || parts[1] == QString(QChar(0x00B0))));
  const int open = t.indexOf(QLatin1Char('('));
  const bool call = t.endsWith(QLatin1Char(')')) && open > 0 && word(t.left(open)) &&
                    !t.left(open).contains(QLatin1Char('.')) && closesAtEnd(t.mid(open));
  return simple || call ? t : QStringLiteral("(%1)").arg(t);
}

QString ratio(const QString& dividend, const QString& divisor) {
  return operand(dividend) + QStringLiteral(" / ") + operand(divisor);
}

// The dividend of an expression that is ratio(dividend, divisor); empty
// when it is not.
QString dividendOf(const QString& expression, const QString& divisor) {
  const QString suffix = QStringLiteral(" / ") + operand(divisor);
  const QString t = expression.trimmed();
  if (divisor.trimmed().isEmpty() || !t.endsWith(suffix)) {
    return QString();
  }
  const QString head = t.left(t.size() - suffix.size()).trimmed();
  QString dividend = head;
  if (head.startsWith(QLatin1Char('(')) && head.endsWith(QLatin1Char(')')) && closesAtEnd(head)) {
    dividend = head.mid(1, head.size() - 2).trimmed();
  }
  return !dividend.isEmpty() && operand(dividend) == head ? dividend : QString();
}

// Where the helix's handles start: the profiles' middle, the way they rise
// along the axis.
std::optional<std::pair<gp_Pnt, gp_Dir>> helixRise(const CommandState& state, const CommandContext& context) {
  const Selection& axisItems = state.items(QStringLiteral("axis"));
  const auto center = centerOf(state.items(QStringLiteral("profiles")), context);
  if (axisItems.isEmpty() || !center) {
    return std::nullopt;
  }
  const auto axis = axisOf(axisItems.first(), context);
  if (!axis) {
    return std::nullopt;
  }
  return std::make_pair(*center, state.checked(QStringLiteral("flip")) ? axis->second.Reversed() : axis->second);
}

// Whether the helix has a growth: anything but a plain zero.
bool helixGrows(const CommandState& state) {
  const QString growth = state.expression(QStringLiteral("growth")).trimmed();
  if (growth.isEmpty()) {
    return false;
  }
  bool number = false;
  const double value = growth.section(QLatin1Char(' '), 0, 0).toDouble(&number);
  return !(number && value == 0.0 && growth.count(QLatin1Char(' ')) <= 1);
}

} // namespace

CommandDef helixCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.helix");
  def.name = QObject::tr("Helix");
  def.icon = QStringLiteral("helix");
  def.tooltip = QObject::tr("Turns sketch profiles about an axis while they rise along it, as a screw moves");
  def.group = QStringLiteral("CREATE");
  def.keywords = {QStringLiteral("screw"), QStringLiteral("thread"), QStringLiteral("spiral"),
                  QStringLiteral("twist")};
  def.featureType = QStringLiteral("helix");
  const auto typeHas = [](const char* field) {
    return [field = QString::fromLatin1(field)](const CommandState& state) {
      return state.choice(QStringLiteral("type")).contains(field);
    };
  };
  def.inputs = {
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"), SelectKind::Profile, 1, 0),
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"),
                     SelectKind::SketchCurve | SelectKind::Axis | SelectKind::Edge | SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("A sketch line, an origin or construction axis, a straight "
                                   "edge, or a cylindrical face"))
          .withAccepts([](const SelectionItem& item) {
            return item.kind == SelectKind::Axis || isLine(item) ||
                   (item.kind == SelectKind::Face && isAxial(item));
          }),
      choiceInput(QStringLiteral("type"), QObject::tr("Type"),
                  {{kRevolutionsAndHeight, QStringLiteral("Revolution and Height")},
                   {kRevolutionsAndPitch, QStringLiteral("Revolution and Pitch")},
                   {kHeightAndPitch, QStringLiteral("Height and Pitch")}},
                  kRevolutionsAndPitch),
      valueInput(QStringLiteral("revolutions"), QObject::tr("Revolutions"), ValueKind::Unitless, QStringLiteral("3"))
          .withTooltip(QObject::tr("The turns; need not be whole"))
          .withVisible(typeHas("revolutions")),
      valueInput(QStringLiteral("height"), QObject::tr("Height"), ValueKind::Length, QStringLiteral("30 mm"))
          .withTooltip(QObject::tr("How far the profiles rise along the axis"))
          .withVisible(typeHas("height"))
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            const auto rise = helixRise(state, model);
            return rise ? std::optional<Manipulator>(arrow(rise->first, rise->second)) : std::nullopt;
          }),
      valueInput(QStringLiteral("pitch"), QObject::tr("Pitch"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("The rise per turn"))
          .withVisible(typeHas("pitch"))
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            // With the turns given, its knob is at the top: pitch times turns.
            const auto rise = helixRise(state, model);
            const double turns = state.value(QStringLiteral("revolutions"));
            if (!rise || state.choice(QStringLiteral("type")) != kRevolutionsAndPitch || !(turns > 0.0)) {
              return std::nullopt;
            }
            Manipulator handle = arrow(rise->first, rise->second);
            handle.factor = turns;
            return handle;
          }),
      valueInput(QStringLiteral("growth"), QObject::tr("Growth"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("How far the profiles move out from the axis per turn; negative narrows "
                                   "the helix")),
      choiceInput(QStringLiteral("hand"), QObject::tr("Handedness"),
                  {{QStringLiteral("right"), QStringLiteral("Right Hand")},
                   {QStringLiteral("left"), QStringLiteral("Left Hand")}},
                  QStringLiteral("right"))
          .withTooltip(QObject::tr("The way the profiles turn about the way they rise")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Rise against the axis direction")),
  };
  def.inputs += operationInputs();
  def.init = newestProfile;
  def.enabled = [&context] { return hasProfiles(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    if (oneSketch(profiles, result, QStringLiteral("profiles")).isEmpty()) {
      return result;
    }
    const QString type = state.choice(QStringLiteral("type"));
    QString pitch = state.expression(QStringLiteral("pitch"));
    QString revolutions = state.expression(QStringLiteral("revolutions"));
    if (type == kHeightAndPitch) {
      revolutions = ratio(state.expression(QStringLiteral("height")), pitch);
    } else if (type == kRevolutionsAndHeight) {
      pitch = ratio(state.expression(QStringLiteral("height")), revolutions);
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("helix")},
                  {QStringLiteral("profiles"), refsOf(profiles)},
                  {QStringLiteral("axis"), state.items(QStringLiteral("axis")).first().reference()},
                  {QStringLiteral("pitch"), pitch},
                  {QStringLiteral("revolutions"), revolutions}};
    if (helixGrows(state)) {
      result.def.insert(QStringLiteral("growth"), state.expression(QStringLiteral("growth")));
      // An edited helix keeps its construction (FreeCAD's for the FreeCAD
      // import's); a new one is Mitcad's.
      const QString construction = state.choice(QStringLiteral("construction"));
      if (!construction.isEmpty()) {
        result.def.insert(QStringLiteral("construction"), construction);
      }
    }
    if (state.choice(QStringLiteral("hand")) == QStringLiteral("left")) {
      result.def.insert(QStringLiteral("left_handed"), true);
    }
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    addOperation(result.def, state);
    return result;
  };

  // A height comes back from the ratio it went in as (also the FreeCAD
  // import's): revolutions "<height> / <pitch>" or a pitch
  // "<height> / <revolutions>".
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    const SelectionItem axis = itemOf(feature.value(QStringLiteral("axis")), context);
    if (axis.isValid()) {
      state.setItems(QStringLiteral("axis"), {axis});
    }
    const QString uid = feature.value(QStringLiteral("uid")).toString();
    const QString pitch = context.valueText(feature.value(QStringLiteral("pitch")), uid);
    const QString revolutions = context.valueText(feature.value(QStringLiteral("revolutions")), uid);
    if (const QString height = dividendOf(revolutions, pitch); !height.isEmpty()) {
      state.setChoice(QStringLiteral("type"), kHeightAndPitch);
      state.setText(QStringLiteral("height"), height);
      state.setText(QStringLiteral("pitch"), pitch);
    } else if (const QString rise = dividendOf(pitch, revolutions); !rise.isEmpty()) {
      state.setChoice(QStringLiteral("type"), kRevolutionsAndHeight);
      state.setText(QStringLiteral("height"), rise);
      state.setText(QStringLiteral("revolutions"), revolutions);
    } else {
      state.setChoice(QStringLiteral("type"), kRevolutionsAndPitch);
      state.setText(QStringLiteral("pitch"), pitch);
      state.setText(QStringLiteral("revolutions"), revolutions);
    }
    if (feature.contains(QStringLiteral("growth"))) {
      state.setText(QStringLiteral("growth"), context.valueText(feature.value(QStringLiteral("growth")), uid));
    }
    state.setChoice(QStringLiteral("construction"), feature.value(QStringLiteral("construction")).toString());
    state.setChoice(QStringLiteral("hand"), feature.value(QStringLiteral("left_handed")).toBool()
                                                ? QStringLiteral("left")
                                                : QStringLiteral("right"));
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    loadOperation(feature, state);
  };

  // A fixed axis (the importer's) cannot be shown.
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("axis")), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString type = state.choice(QStringLiteral("type"));
    const double height = state.value(QStringLiteral("height"));
    double pitch = state.value(QStringLiteral("pitch"));
    double turns = state.value(QStringLiteral("revolutions"));
    if (type == kHeightAndPitch) {
      turns = height / pitch;
    } else if (type == kRevolutionsAndHeight) {
      pitch = height / turns;
    }
    QString text = QStringLiteral("Added helix (%1), %2 turns of %3, %4")
                       .arg(label(kOperations, state.choice(QStringLiteral("operation"))), number(turns),
                            number(pitch),
                            state.choice(QStringLiteral("hand")) == QStringLiteral("left")
                                ? QStringLiteral("left-handed")
                                : QStringLiteral("right-handed"));
    if (helixGrows(state)) {
      text += QStringLiteral(", growing %1 per turn").arg(number(state.value(QStringLiteral("growth"))));
    }
    return text;
  };
  return def;
}

} // namespace mitcad::cmd
