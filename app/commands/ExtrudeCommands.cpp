// SPDX-License-Identifier: MIT
// Extrude and Revolve with all of the model's options (U4; commands.md,
// "extrude" and "revolve"), laid out in panel sections: Type, Profiles,
// Start, Direction, Extent, Taper, Thin walls, Operation and Objects.
#include <cmath>

#include <QJsonArray>
#include <QKeySequence>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const QString kDistance = QStringLiteral("distance");
const QString kToObject = QStringLiteral("to_object");
const QString kThroughAll = QStringLiteral("through_all");
const QString kOneSide = QStringLiteral("one_side");
const QString kTwoSides = QStringLiteral("two_sides");
const QString kSymmetric = QStringLiteral("symmetric");

const Choices kDirections = {{QStringLiteral("one_side"), QStringLiteral("One Side")},
                             {QStringLiteral("two_sides"), QStringLiteral("Two Sides")},
                             {QStringLiteral("symmetric"), QStringLiteral("Symmetric")}};
const Choices kExtents = {{QStringLiteral("distance"), QStringLiteral("Distance")},
                          {QStringLiteral("to_object"), QStringLiteral("To Object")},
                          {QStringLiteral("through_all"), QStringLiteral("All")}};
const Choices kWalls = {{QStringLiteral("side1"), QStringLiteral("Side 1")},
                        {QStringLiteral("center"), QStringLiteral("Center")},
                        {QStringLiteral("side2"), QStringLiteral("Side 2")}};

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

// --- Extrude

// Where the extrusion starts: the profiles' middle, moved by a start offset.
std::optional<std::pair<gp_Pnt, gp_Dir>> extrudeBase(const CommandState& state,
                                                     const CommandContext& context) {
  auto base = profileNormal(state.items(QStringLiteral("profiles")), context);
  if (!base) {
    return std::nullopt;
  }
  if (state.choice(QStringLiteral("start")) == QStringLiteral("offset")) {
    const double offset = state.value(QStringLiteral("start_offset"));
    if (std::isfinite(offset)) {
      base->first.Translate(gp_Vec(base->second) * offset);
    }
  }
  return base;
}

// One side's extent, as a two-sided extent's side or a one-sided extent.
QJsonObject sideOf(const CommandState& state, const QString& suffix, Built& error) {
  const QString kind = state.choice(QStringLiteral("extent") + suffix);
  QJsonObject side{{QStringLiteral("type"), kind}};
  if (kind == kDistance) {
    side.insert(QStringLiteral("distance"), state.expression(QStringLiteral("distance") + suffix));
  } else if (kind == kToObject) {
    if (state.items(QStringLiteral("object") + suffix).isEmpty()) {
      error = Built::failure(QObject::tr("Select the object the extent ends at."),
                             QStringLiteral("object") + suffix);
      return side;
    }
    side.insert(QStringLiteral("object"), objectOf(state, suffix));
    if (nonZero(state, QStringLiteral("offset") + suffix)) {
      side.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset") + suffix));
    }
  }
  if (nonZero(state, QStringLiteral("taper") + suffix)) {
    side.insert(QStringLiteral("taper"), state.expression(QStringLiteral("taper") + suffix));
  }
  return side;
}

bool loadSide(const QJsonObject& side, CommandState& state, const QString& suffix,
              const QJsonObject& feature, const CommandContext& context) {
  const QString type = str(side, "type");
  state.setChoice(QStringLiteral("extent") + suffix, type);
  loadValue(state, QStringLiteral("distance") + suffix, side.value(QStringLiteral("distance")), feature, context);
  loadValue(state, QStringLiteral("offset") + suffix, side.value(QStringLiteral("offset")), feature, context);
  loadValue(state, QStringLiteral("taper") + suffix, side.value(QStringLiteral("taper")), feature, context);
  if (type == kToObject) {
    return loadObject(side.value(QStringLiteral("object")).toObject(), state, suffix, context);
  }
  return type == kDistance || type == kThroughAll;
}

bool sideEditable(const QJsonObject& side, const CommandContext* context) {
  const QString type = str(side, "type");
  if (type == kToObject) {
    return context == nullptr || objectShowable(side.value(QStringLiteral("object")).toObject(), *context);
  }
  return type == kDistance || type == kThroughAll;
}

QVector<InputDef> sideInputs(const QString& suffix, std::function<bool(const CommandState&)> shown,
                             const QString& name) {
  const QString extent = QStringLiteral("extent") + suffix;
  const auto when = [shown, extent](const QString& kind) {
    return [shown, extent, kind](const CommandState& s) { return shown(s) && s.choice(extent) == kind; };
  };
  QVector<InputDef> inputs = {
      choiceInput(extent, name.isEmpty() ? QObject::tr("Extent") : QObject::tr("Extent %1").arg(name),
                  kExtents, kDistance)
          .withVisible(shown),
      valueInput(QStringLiteral("distance") + suffix,
                 name.isEmpty() ? QObject::tr("Distance") : QObject::tr("Distance %1").arg(name),
                 ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(when(kDistance))
          .withManipulator([suffix](const CommandState& state,
                                    const CommandContext& context) -> std::optional<Manipulator> {
            auto base = extrudeBase(state, context);
            if (!base) {
              return std::nullopt;
            }
            gp_Dir direction = base->second;
            const bool one = state.choice(QStringLiteral("direction")) == kOneSide;
            if ((one && state.checked(QStringLiteral("flip"))) || !suffix.isEmpty()) {
              direction.Reverse(); // side two goes against the normal
            }
            return arrow(base->first, direction);
          }),
  };
  inputs += objectInputs(suffix, when(kToObject));
  inputs.append(valueInput(QStringLiteral("offset") + suffix, QObject::tr("Offset"), ValueKind::Length,
                           QStringLiteral("0 mm"))
                    .withTooltip(QObject::tr("Beyond the object (negative: short of it)"))
                    .withVisible(when(kToObject)));
  inputs.append(valueInput(QStringLiteral("taper") + suffix,
                           name.isEmpty() ? QObject::tr("Taper Angle")
                                          : QObject::tr("Taper Angle %1").arg(name),
                           ValueKind::Angle, QStringLiteral("0 deg"))
                    .withTooltip(QObject::tr("Positive widens the profile away from the start"))
                    .withVisible(shown));
  return inputs;
}

} // namespace

CommandDef extrudeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.extrude");
  def.name = QObject::tr("Extrude");
  def.icon = QStringLiteral("extrude");
  def.tooltip = QObject::tr("Pushes sketch profiles into a solid");
  def.shortcut = QKeySequence(Qt::Key_E);
  def.group = QStringLiteral("CREATE");
  def.pinned = true;
  def.keywords = {QStringLiteral("pad"), QStringLiteral("boss"), QStringLiteral("pocket"),
                  QStringLiteral("thin")};
  def.featureType = QStringLiteral("extrude");
  const auto always = [](const CommandState&) { return true; };
  const auto thin = choiceIs(QStringLiteral("type"), QStringLiteral("thin"));
  const auto twoSides = choiceIs(QStringLiteral("direction"), kTwoSides);
  def.inputs = {
      choiceInput(QStringLiteral("type"), QObject::tr("Type"),
                  {{QStringLiteral("solid"), QStringLiteral("Extrude")},
                   {QStringLiteral("thin"), QStringLiteral("Thin Extrude")}},
                  QStringLiteral("solid"))
          .withTooltip(QObject::tr("A solid of the profiles, or walls along their curves")),
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"), SelectKind::Profile, 1, 0),
      choiceInput(QStringLiteral("start"), QObject::tr("Start"),
                  {{QStringLiteral("profile_plane"), QStringLiteral("Profile Plane")},
                   {QStringLiteral("offset"), QStringLiteral("Offset")},
                   {QStringLiteral("object"), QStringLiteral("Object")}},
                  QStringLiteral("profile_plane")),
      valueInput(QStringLiteral("start_offset"), QObject::tr("Offset"), ValueKind::Length,
                 QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Along the sketch normal from the start plane"))
          .withVisible([](const CommandState& s) {
            return s.choice(QStringLiteral("start")) != QStringLiteral("profile_plane");
          })
          .withManipulator([](const CommandState& state,
                              const CommandContext& model) -> std::optional<Manipulator> {
            if (state.choice(QStringLiteral("start")) != QStringLiteral("offset")) {
              return std::nullopt;
            }
            const auto base = profileNormal(state.items(QStringLiteral("profiles")), model);
            return base ? std::optional<Manipulator>(arrow(base->first, base->second)) : std::nullopt;
          }),
      selectionInput(QStringLiteral("start_object"), QObject::tr("Start Object"), kPlaneKinds, 1, 1)
          .withTooltip(QObject::tr("The plane or planar face the profiles are projected onto"))
          .withAccepts(isPlanar)
          .withVisible(choiceIs(QStringLiteral("start"), QStringLiteral("object"))),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kDirections, kOneSide),
  };
  def.inputs += sideInputs(QString(), always, QString());
  def.inputs.append(choiceInput(QStringLiteral("measurement"), QObject::tr("Measurement"),
                                {{QStringLiteral("half"), QStringLiteral("Half Length")},
                                 {QStringLiteral("whole"), QStringLiteral("Whole Length")}},
                                QStringLiteral("half"))
                        .withVisible([](const CommandState& s) {
                          return s.choice(QStringLiteral("direction")) == kSymmetric &&
                                 s.choice(QStringLiteral("extent")) == kDistance;
                        }));
  def.inputs.append(flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
                        .withVisible(choiceIs(QStringLiteral("direction"), kOneSide)));
  def.inputs += sideInputs(QStringLiteral("2"), twoSides, QStringLiteral("2"));
  def.inputs.append(choiceInput(QStringLiteral("wall"), QObject::tr("Wall Location"), kWalls,
                                QStringLiteral("side1"))
                        .withTooltip(QObject::tr("Side 1 lies away from the profile's material"))
                        .withVisible(thin));
  def.inputs.append(valueInput(QStringLiteral("thickness"), QObject::tr("Wall Thickness"),
                               ValueKind::Length, QStringLiteral("1 mm"))
                        .withVisible(thin));
  const auto thinTwo = [thin, twoSides](const CommandState& s) { return thin(s) && twoSides(s); };
  def.inputs.append(choiceInput(QStringLiteral("wall2"), QObject::tr("Wall Location 2"), kWalls,
                                QStringLiteral("side1"))
                        .withVisible(thinTwo));
  def.inputs.append(valueInput(QStringLiteral("thickness2"), QObject::tr("Wall Thickness 2"),
                               ValueKind::Length, QStringLiteral("1 mm"))
                        .withVisible(thinTwo));
  def.inputs += operationInputs();
  def.init = newestProfile;
  def.enabled = [&context] { return hasProfiles(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    if (oneSketch(profiles, result, QStringLiteral("profiles")).isEmpty()) {
      return result;
    }
    const QString direction = state.choice(QStringLiteral("direction"));
    QJsonObject extent;
    if (direction == kTwoSides) {
      const QJsonObject side1 = sideOf(state, QString(), result);
      const QJsonObject side2 = sideOf(state, QStringLiteral("2"), result);
      if (!result.error.isEmpty()) {
        return result;
      }
      extent = {{QStringLiteral("type"), kTwoSides},
                {QStringLiteral("side1"), side1},
                {QStringLiteral("side2"), side2}};
    } else {
      extent = sideOf(state, QString(), result);
      if (!result.error.isEmpty()) {
        return result;
      }
      if (direction == kSymmetric) {
        const QString kind = state.choice(QStringLiteral("extent"));
        if (kind == kToObject) {
          return Built::failure(QObject::tr("A symmetric extrusion goes a distance or through all."),
                                QStringLiteral("extent"));
        }
        if (kind == kDistance) {
          extent.insert(QStringLiteral("type"), kSymmetric);
          if (state.choice(QStringLiteral("measurement")) == QStringLiteral("whole")) {
            extent.insert(QStringLiteral("full_length"), true);
          }
        } else {
          extent.insert(QStringLiteral("both_sides"), true);
        }
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("extrude")},
                  {QStringLiteral("profiles"), refsOf(profiles)},
                  {QStringLiteral("extent"), extent}};
    const QString start = state.choice(QStringLiteral("start"));
    if (start == QStringLiteral("offset")) {
      result.def.insert(QStringLiteral("start"),
                        QJsonObject{{QStringLiteral("type"), QStringLiteral("offset")},
                                    {QStringLiteral("offset"), state.expression(QStringLiteral("start_offset"))}});
    } else if (start == QStringLiteral("object")) {
      const Selection& object = state.items(QStringLiteral("start_object"));
      if (object.isEmpty()) {
        return Built::failure(QObject::tr("Select the plane the extrusion starts at."),
                              QStringLiteral("start_object"));
      }
      QJsonObject begin{{QStringLiteral("type"), QStringLiteral("object")},
                        {QStringLiteral("object"),
                         QJsonObject{{QStringLiteral("type"), QStringLiteral("plane")},
                                     {QStringLiteral("plane"), object.first().reference()}}}};
      if (nonZero(state, QStringLiteral("start_offset"))) {
        begin.insert(QStringLiteral("offset"), state.expression(QStringLiteral("start_offset")));
      }
      result.def.insert(QStringLiteral("start"), begin);
    }
    if (state.choice(QStringLiteral("type")) == QStringLiteral("thin")) {
      QJsonObject walls{{QStringLiteral("location"), state.choice(QStringLiteral("wall"))},
                        {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness"))}};
      if (direction == kTwoSides &&
          (state.choice(QStringLiteral("wall2")) != state.choice(QStringLiteral("wall")) ||
           state.expression(QStringLiteral("thickness2")) != state.expression(QStringLiteral("thickness")))) {
        walls.insert(QStringLiteral("side2"),
                     QJsonObject{{QStringLiteral("location"), state.choice(QStringLiteral("wall2"))},
                                 {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness2"))}});
      }
      result.def.insert(QStringLiteral("thin"), walls);
    }
    if (direction == kOneSide && state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    addOperation(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == kTwoSides) {
      state.setChoice(QStringLiteral("direction"), kTwoSides);
      loadSide(extent.value(QStringLiteral("side1")).toObject(), state, QString(), feature, context);
      loadSide(extent.value(QStringLiteral("side2")).toObject(), state, QStringLiteral("2"), feature, context);
    } else if (type == kSymmetric) {
      state.setChoice(QStringLiteral("direction"), kSymmetric);
      state.setChoice(QStringLiteral("extent"), kDistance);
      loadValue(state, QStringLiteral("distance"), extent.value(QStringLiteral("distance")), feature, context);
      loadValue(state, QStringLiteral("taper"), extent.value(QStringLiteral("taper")), feature, context);
      state.setChoice(QStringLiteral("measurement"), extent.value(QStringLiteral("full_length")).toBool()
                                                         ? QStringLiteral("whole")
                                                         : QStringLiteral("half"));
    } else {
      state.setChoice(QStringLiteral("direction"),
                      extent.value(QStringLiteral("both_sides")).toBool() ? kSymmetric : kOneSide);
      loadSide(extent, state, QString(), feature, context);
    }
    const QJsonObject start = feature.value(QStringLiteral("start")).toObject();
    const QString startType = str(start, "type");
    if (startType == QStringLiteral("offset") || startType == QStringLiteral("object")) {
      state.setChoice(QStringLiteral("start"), startType);
      loadValue(state, QStringLiteral("start_offset"), start.value(QStringLiteral("offset")), feature, context);
      if (startType == QStringLiteral("object")) {
        const QJsonObject object = start.value(QStringLiteral("object")).toObject();
        const SelectionItem item = itemOf(object.value(QStringLiteral("plane")), context);
        if (item.isValid()) {
          state.setItems(QStringLiteral("start_object"), {item});
        }
      }
    }
    const QJsonObject thin = feature.value(QStringLiteral("thin")).toObject();
    if (!thin.isEmpty()) {
      state.setChoice(QStringLiteral("type"), QStringLiteral("thin"));
      state.setChoice(QStringLiteral("wall"), str(thin, "location"));
      loadValue(state, QStringLiteral("thickness"), thin.value(QStringLiteral("thickness")), feature, context);
      const QJsonObject side2 = thin.value(QStringLiteral("side2")).toObject();
      state.setChoice(QStringLiteral("wall2"), side2.isEmpty() ? str(thin, "location") : str(side2, "location"));
      loadValue(state, QStringLiteral("thickness2"),
                side2.isEmpty() ? thin.value(QStringLiteral("thickness")) : side2.value(QStringLiteral("thickness")),
                feature, context);
    }
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    loadOperation(feature, state);
  };

  // Everything the model's extrusions have can be shown, but for fixed
  // planes (the importer's) as objects, a flipped two-sided or symmetric
  // extrusion and thin walls of side two of a one-sided one.
  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    bool ok = true;
    if (type == kTwoSides) {
      ok = sideEditable(extent.value(QStringLiteral("side1")).toObject(), &context) &&
           sideEditable(extent.value(QStringLiteral("side2")).toObject(), &context);
    } else if (type == kSymmetric) {
      ok = true;
    } else {
      ok = sideEditable(extent, &context);
      if (type == kToObject && extent.value(QStringLiteral("both_sides")).toBool()) {
        ok = false;
      }
    }
    const bool oneSided = type != kTwoSides && type != kSymmetric &&
                          !extent.value(QStringLiteral("both_sides")).toBool();
    if (feature.value(QStringLiteral("flip")).toBool() && !oneSided) {
      ok = false;
    }
    const QJsonObject thin = feature.value(QStringLiteral("thin")).toObject();
    if (thin.contains(QStringLiteral("side2")) && type != kTwoSides) {
      ok = false;
    }
    const QJsonObject start = feature.value(QStringLiteral("start")).toObject();
    if (str(start, "type") == QStringLiteral("object")) {
      const QJsonObject object = start.value(QStringLiteral("object")).toObject();
      ok = ok && str(object, "type") == QStringLiteral("plane") &&
           isShowable(object.value(QStringLiteral("plane")), context);
    }
    return ok;
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString kind = state.choice(QStringLiteral("extent"));
    QString extent = number(state.value(QStringLiteral("distance")));
    if (kind == kThroughAll) {
      extent = QStringLiteral("all");
    } else if (kind == kToObject) {
      extent = QStringLiteral("to object");
    }
    return QStringLiteral("Added %1extrude (%2), distance %3")
        .arg(state.choice(QStringLiteral("type")) == QStringLiteral("thin") ? QStringLiteral("thin ") : QString(),
             label(kOperations, state.choice(QStringLiteral("operation"))), extent);
  };
  return def;
}

// ---------------------------------------------------------------------------
// Revolve

CommandDef revolveCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.revolve");
  def.name = QObject::tr("Revolve");
  def.icon = QStringLiteral("revolve");
  def.tooltip = QObject::tr("Turns sketch profiles about an axis into a solid");
  def.group = QStringLiteral("CREATE");
  def.pinned = true;
  def.keywords = {QStringLiteral("rotate"), QStringLiteral("turn"), QStringLiteral("lathe")};
  def.featureType = QStringLiteral("revolve");
  const auto partial = choiceIs(QStringLiteral("type"), QStringLiteral("angle"));
  const auto toObject = choiceIs(QStringLiteral("type"), kToObject);
  // A ring about the axis through the profiles' middle.
  const auto angleRing = [](bool second) {
    return [second](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
      const Selection& axisItems = state.items(QStringLiteral("axis"));
      const auto center = centerOf(state.items(QStringLiteral("profiles")), model);
      if (axisItems.isEmpty() || !center) {
        return std::nullopt;
      }
      const auto axis = axisOf(axisItems.first(), model);
      if (!axis) {
        return std::nullopt;
      }
      const gp_Vec along(axis->second);
      const gp_Vec out(axis->first, *center);
      const gp_Pnt foot = axis->first.Translated(along * out.Dot(along));
      const gp_Vec radial(foot, *center);
      Manipulator handle = ring(foot, second ? axis->second.Reversed() : axis->second);
      if (radial.Magnitude() > 1e-9) {
        handle.reference = gp_Dir(radial);
      }
      return handle;
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
      choiceInput(QStringLiteral("type"), QObject::tr("Extent"),
                  {{QStringLiteral("full"), QObject::tr("Full")},
                   {QStringLiteral("angle"), QObject::tr("Angle")},
                   {QStringLiteral("to_object"), QObject::tr("To Object")}},
                  QStringLiteral("full")),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kDirections, kOneSide)
          .withVisible(partial),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withTooltip(QObject::tr("Right-handed about the axis; negative turns the other way"))
          .withVisible(partial)
          .withManipulator(angleRing(false)),
      valueInput(QStringLiteral("angle2"), QObject::tr("Angle 2"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible([partial](const CommandState& s) {
            return partial(s) && s.choice(QStringLiteral("direction")) == kTwoSides;
          })
          .withManipulator(angleRing(true)),
  };
  def.inputs += objectInputs(QString(), toObject);
  def.inputs.append(checkInput(QStringLiteral("project_axis"), QObject::tr("Project Axis"), false)
                        .withTooltip(QObject::tr("Revolve about the axis projected into the sketch plane")));
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
    QJsonObject extent{{QStringLiteral("type"), type}};
    if (type == QStringLiteral("angle")) {
      const QString direction = state.choice(QStringLiteral("direction"));
      if (direction == kTwoSides) {
        extent = {{QStringLiteral("type"), kTwoSides},
                  {QStringLiteral("angle1"), state.expression(QStringLiteral("angle"))},
                  {QStringLiteral("angle2"), state.expression(QStringLiteral("angle2"))}};
      } else {
        extent = {{QStringLiteral("type"), direction == kSymmetric ? kSymmetric : QStringLiteral("angle")},
                  {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
      }
    } else if (type == kToObject) {
      if (state.items(QStringLiteral("object")).isEmpty()) {
        return Built::failure(QObject::tr("Select the object the revolution ends at."), QStringLiteral("object"));
      }
      extent.insert(QStringLiteral("object"), objectOf(state, QString()));
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("revolve")},
                  {QStringLiteral("profiles"), refsOf(profiles)},
                  {QStringLiteral("axis"), state.items(QStringLiteral("axis")).first().reference()},
                  {QStringLiteral("extent"), extent}};
    if (state.checked(QStringLiteral("project_axis"))) {
      result.def.insert(QStringLiteral("project_axis"), true);
    }
    addOperation(result.def, state);
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    const SelectionItem axis = itemOf(feature.value(QStringLiteral("axis")), context);
    if (axis.isValid()) {
      state.setItems(QStringLiteral("axis"), {axis});
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == QStringLiteral("full") || type == kToObject) {
      state.setChoice(QStringLiteral("type"), type);
      if (type == kToObject) {
        loadObject(extent.value(QStringLiteral("object")).toObject(), state, QString(), context);
      }
    } else {
      state.setChoice(QStringLiteral("type"), QStringLiteral("angle"));
      state.setChoice(QStringLiteral("direction"), type == kTwoSides   ? kTwoSides
                                                   : type == kSymmetric ? kSymmetric
                                                                        : kOneSide);
      loadValue(state, QStringLiteral("angle"),
                type == kTwoSides ? extent.value(QStringLiteral("angle1")) : extent.value(QStringLiteral("angle")),
                feature, context);
      loadValue(state, QStringLiteral("angle2"), extent.value(QStringLiteral("angle2")), feature, context);
    }
    state.setChecked(QStringLiteral("project_axis"), feature.value(QStringLiteral("project_axis")).toBool());
    loadOperation(feature, state);
  };

  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == kToObject && !objectShowable(extent.value(QStringLiteral("object")).toObject(), context)) {
      return false;
    }
    return isShowable(feature.value(QStringLiteral("axis")), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString type = state.choice(QStringLiteral("type"));
    QString extent = QStringLiteral("full turn");
    if (type == QStringLiteral("angle")) {
      extent = QStringLiteral("angle %1 deg").arg(degrees(state.value(QStringLiteral("angle"))));
    } else if (type == kToObject) {
      extent = QStringLiteral("to object");
    }
    return QStringLiteral("Added revolve (%1), %2")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))), extent);
  };
  return def;
}

} // namespace mitcad::cmd
