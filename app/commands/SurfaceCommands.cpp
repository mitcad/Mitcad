// SPDX-License-Identifier: MIT
// The SURFACE group (mitcad#455; commands.md, "Surface modelling" and
// "Patch, Stitch and Thicken"): surface bodies made by Surface Extrude,
// Surface Revolve, Surface Sweep and Surface Loft (the solid features with
// "surface" set), filled, sewn and made solid by Patch, Stitch and Thicken
// (mitcad#456), and the surface features of the later families.
#include <cmath>
#include <optional>

#include <QJsonArray>

#include "../framework/CommandRegistry.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"

namespace mitcad::cmd {
namespace {

const QString kGroup = QStringLiteral("SURFACE");
const QString kOneSide = QStringLiteral("one_side");
const QString kTwoSides = QStringLiteral("two_sides");
const QString kSymmetric = QStringLiteral("symmetric");

const Choices kDirections = {{QStringLiteral("one_side"), QStringLiteral("One Side")},
                             {QStringLiteral("two_sides"), QStringLiteral("Two Sides")},
                             {QStringLiteral("symmetric"), QStringLiteral("Symmetric")}};
const Choices kContinuity = {{QStringLiteral("g0"), QStringLiteral("G0 (through the edges)")},
                             {QStringLiteral("g1"), QStringLiteral("G1 (tangent)")},
                             {QStringLiteral("g2"), QStringLiteral("G2 (curvature)")}};

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

CommandDef surfaceCommand(const char* id, const QString& name, const char* icon, const QString& tooltip) {
  CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = name;
  def.icon = QString::fromLatin1(icon);
  def.tooltip = tooltip;
  def.tab = QStringLiteral("SOLID");
  def.group = kGroup;
  def.keywords = {QStringLiteral("surface"), QStringLiteral("sheet")};
  return def;
}

// --- Profiles and curves of a surface sweep

// The input of the profiles and sketch curves a surface sweep takes, and
// Chain Selection for the curves. A 3D sketch's profiles and curves in its
// sketch plane too (mitcad#1550).
QVector<InputDef> sheetInputs(const CommandContext& context) {
  return {
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"),
                     SelectKind::Profile | SelectKind::SketchCurve, 1, 0)
          .withTooltip(QObject::tr("Profiles (their boundaries) and sketch curves, open or closed, "
                                   "of one sketch"))
          .withAccepts([&context](const SelectionItem& item) { return planeCurveTakes(item, context); })
          .withRefusedHint([&context](const SelectionItem& item, const CommandState&) {
            return planeCurveRefusal(item, context);
          }),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true)
          .withTooltip(QObject::tr("Also take the curves joined to the selected ones")),
  };
}

// The profiles and curves picked into the definition; false (with the
// error) when they are not of one sketch.
bool addSheetInputs(QJsonObject& def, const CommandState& state, Built& result) {
  const Selection& picked = state.items(QStringLiteral("profiles"));
  const QString sketch = oneSketch(picked, result, QStringLiteral("profiles"));
  if (sketch.isEmpty()) {
    return false;
  }
  Selection profiles;
  Selection curves;
  for (const SelectionItem& item : picked) {
    (item.kind == SelectKind::Profile ? profiles : curves).append(item);
  }
  if (!profiles.isEmpty()) {
    def.insert(QStringLiteral("profiles"), refsOf(profiles));
  }
  if (!curves.isEmpty()) {
    def.insert(QStringLiteral("curves"),
               pathOf(curves, state.checked(QStringLiteral("chain")), result, QStringLiteral("profiles")));
  }
  def.insert(QStringLiteral("surface"), true);
  def.insert(QStringLiteral("operation"), QStringLiteral("new_body"));
  return result.error.isEmpty();
}

void loadSheetInputs(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  Selection picked = profilesOf(feature.value(QStringLiteral("profiles")).toArray());
  bool chain = true;
  if (feature.contains(QStringLiteral("curves"))) {
    if (const auto curves = pathItems(feature.value(QStringLiteral("curves")), context, &chain)) {
      picked += *curves;
    }
  }
  state.setItems(QStringLiteral("profiles"), picked);
  state.setChecked(QStringLiteral("chain"), chain);
}

// A surface feature (surface set) whose curves the panel can show.
bool sheetEditable(const QJsonObject& feature, const CommandContext& context) {
  if (!feature.value(QStringLiteral("surface")).toBool() ||
      feature.value(QStringLiteral("operation")).toString() != QStringLiteral("new_body")) {
    return false;
  }
  return !feature.contains(QStringLiteral("curves")) ||
         pathItems(feature.value(QStringLiteral("curves")), context).has_value();
}

QString sheetCount(const CommandState& state) {
  int profiles = 0;
  int curves = 0;
  for (const SelectionItem& item : state.items(QStringLiteral("profiles"))) {
    ++(item.kind == SelectKind::Profile ? profiles : curves);
  }
  return QStringLiteral("%1 profiles and %2 curves").arg(profiles).arg(curves);
}

// --- Surface Extrude

CommandDef surfaceExtrudeCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.extrude", QObject::tr("Surface Extrude"), "surface-extrude",
                                  QObject::tr("Pushes profiles' boundaries and sketch curves into surfaces, "
                                              "without caps"));
  def.pinned = true;
  def.featureType = QStringLiteral("extrude");
  def.inputs = sheetInputs(context);
  def.inputs += {
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kDirections, kOneSide),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("10 mm")),
      valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length, QStringLiteral("10 mm"))
          .withVisible(choiceIs(QStringLiteral("direction"), kTwoSides)),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withVisible(choiceIs(QStringLiteral("direction"), kOneSide)),
  };
  def.init = newestProfile;
  def.enabled = [&context] { return hasSketches(context, true); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("extrude")}};
    if (!addSheetInputs(definition, state, result)) {
      return result;
    }
    const QString direction = state.choice(QStringLiteral("direction"));
    const QJsonObject distance{{QStringLiteral("type"), QStringLiteral("distance")},
                               {QStringLiteral("distance"), state.expression(QStringLiteral("distance"))}};
    if (direction == kTwoSides) {
      definition.insert(QStringLiteral("extent"),
                        QJsonObject{{QStringLiteral("type"), kTwoSides},
                                    {QStringLiteral("side1"), distance},
                                    {QStringLiteral("side2"),
                                     QJsonObject{{QStringLiteral("type"), QStringLiteral("distance")},
                                                 {QStringLiteral("distance"),
                                                  state.expression(QStringLiteral("distance2"))}}}});
    } else if (direction == kSymmetric) {
      definition.insert(QStringLiteral("extent"),
                        QJsonObject{{QStringLiteral("type"), kSymmetric},
                                    {QStringLiteral("distance"), state.expression(QStringLiteral("distance"))}});
    } else {
      definition.insert(QStringLiteral("extent"), distance);
      if (state.checked(QStringLiteral("flip"))) {
        definition.insert(QStringLiteral("flip"), true);
      }
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadSheetInputs(feature, state, context);
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == kTwoSides) {
      state.setChoice(QStringLiteral("direction"), kTwoSides);
      loadValue(state, QStringLiteral("distance"),
                extent.value(QStringLiteral("side1")).toObject().value(QStringLiteral("distance")), feature, context);
      loadValue(state, QStringLiteral("distance2"),
                extent.value(QStringLiteral("side2")).toObject().value(QStringLiteral("distance")), feature, context);
    } else {
      state.setChoice(QStringLiteral("direction"), type == kSymmetric ? kSymmetric : kOneSide);
      loadValue(state, QStringLiteral("distance"), extent.value(QStringLiteral("distance")), feature, context);
    }
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
  };
  // Distances only, from the profile plane; the solid Extrude's panel has
  // the other extents.
  def.canEdit = [&context](const QJsonObject& feature) {
    if (!sheetEditable(feature, context) || feature.contains(QStringLiteral("start"))) {
      return false;
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    const auto plain = [](const QJsonObject& side) {
      return str(side, "type") == QStringLiteral("distance") && !side.contains(QStringLiteral("taper"));
    };
    if (type == kTwoSides) {
      return plain(extent.value(QStringLiteral("side1")).toObject()) &&
             plain(extent.value(QStringLiteral("side2")).toObject());
    }
    return (type == QStringLiteral("distance") || type == kSymmetric) && !extent.contains(QStringLiteral("taper")) &&
           !extent.value(QStringLiteral("full_length")).toBool();
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added surface extrude of %1").arg(sheetCount(state));
  };
  return def;
}

// --- Surface Revolve

CommandDef surfaceRevolveCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.revolve", QObject::tr("Surface Revolve"), "surface-revolve",
                                  QObject::tr("Turns profiles' boundaries and sketch curves about an axis into "
                                              "surfaces"));
  def.featureType = QStringLiteral("revolve");
  const auto partial = choiceIs(QStringLiteral("type"), QStringLiteral("angle"));
  def.inputs = sheetInputs(context);
  def.inputs += {
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"),
                     SelectKind::SketchCurve | SelectKind::Axis | SelectKind::Edge, 1, 1)
          .withTooltip(QObject::tr("A sketch line, an origin or construction axis, or a straight edge"))
          .withAccepts([](const SelectionItem& item) { return item.kind == SelectKind::Axis || isLine(item); }),
      choiceInput(QStringLiteral("type"), QObject::tr("Extent"),
                  {{QStringLiteral("full"), QObject::tr("Full")}, {QStringLiteral("angle"), QObject::tr("Angle")}},
                  QStringLiteral("full")),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kDirections, kOneSide).withVisible(partial),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible(partial),
      valueInput(QStringLiteral("angle2"), QObject::tr("Angle 2"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withVisible([partial](const CommandState& s) {
            return partial(s) && s.choice(QStringLiteral("direction")) == kTwoSides;
          }),
  };
  def.enabled = [&context] { return hasSketches(context, true); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("revolve")}};
    if (!addSheetInputs(definition, state, result)) {
      return result;
    }
    const Selection& axis = state.items(QStringLiteral("axis"));
    if (axis.isEmpty()) {
      return Built::failure(QObject::tr("Select the axis."), QStringLiteral("axis"));
    }
    definition.insert(QStringLiteral("axis"), axis.first().reference());
    QJsonObject extent{{QStringLiteral("type"), QStringLiteral("full")}};
    if (state.choice(QStringLiteral("type")) == QStringLiteral("angle")) {
      const QString direction = state.choice(QStringLiteral("direction"));
      if (direction == kTwoSides) {
        extent = {{QStringLiteral("type"), kTwoSides},
                  {QStringLiteral("angle1"), state.expression(QStringLiteral("angle"))},
                  {QStringLiteral("angle2"), state.expression(QStringLiteral("angle2"))}};
      } else {
        extent = {{QStringLiteral("type"), direction == kSymmetric ? kSymmetric : QStringLiteral("angle")},
                  {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
      }
    }
    definition.insert(QStringLiteral("extent"), extent);
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadSheetInputs(feature, state, context);
    const SelectionItem axis = itemOf(feature.value(QStringLiteral("axis")), context);
    if (axis.isValid()) {
      state.setItems(QStringLiteral("axis"), {axis});
    }
    const QJsonObject extent = feature.value(QStringLiteral("extent")).toObject();
    const QString type = str(extent, "type");
    if (type == QStringLiteral("full")) {
      state.setChoice(QStringLiteral("type"), type);
      return;
    }
    state.setChoice(QStringLiteral("type"), QStringLiteral("angle"));
    state.setChoice(QStringLiteral("direction"), type == kTwoSides   ? kTwoSides
                                                 : type == kSymmetric ? kSymmetric
                                                                      : kOneSide);
    loadValue(state, QStringLiteral("angle"),
              type == kTwoSides ? extent.value(QStringLiteral("angle1")) : extent.value(QStringLiteral("angle")),
              feature, context);
    loadValue(state, QStringLiteral("angle2"), extent.value(QStringLiteral("angle2")), feature, context);
  };
  def.canEdit = [&context](const QJsonObject& feature) {
    const QString type = str(feature.value(QStringLiteral("extent")).toObject(), "type");
    return sheetEditable(feature, context) && type != QStringLiteral("to_object") &&
           !feature.value(QStringLiteral("project_axis")).toBool() &&
           isShowable(feature.value(QStringLiteral("axis")), context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added surface revolve of %1").arg(sheetCount(state));
  };
  return def;
}

// --- Surface Sweep

CommandDef surfaceSweepCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.sweep", QObject::tr("Surface Sweep"), "surface-sweep",
                                  QObject::tr("Sweeps profiles' boundaries and sketch curves along a path into "
                                              "surfaces"));
  def.featureType = QStringLiteral("sweep");
  def.inputs = sheetInputs(context);
  def.inputs += {
      // A 3D sketch's curves too, as the typed path a Pipe's is
      // (mitcad#1550).
      selectionInput(QStringLiteral("path"), QObject::tr("Path"), kPathKinds, 1, 0)
          .withTooltip(QObject::tr("Sketch curves or edges, joined end to end"))
          .withAccepts([&context](const SelectionItem& item) { return spatialPathTakes(item, context); })
          .withRefusedHint([&context](const SelectionItem& item, const CommandState&) {
            return spatialPathRefusal(item, context);
          }),
      choiceInput(QStringLiteral("orientation"), QObject::tr("Orientation"),
                  {{QStringLiteral("perpendicular"), QStringLiteral("Perpendicular")},
                   {QStringLiteral("parallel"), QStringLiteral("Parallel")}},
                  QStringLiteral("perpendicular")),
  };
  def.enabled = [&context] { return hasSketches(context, true); };
  def.build = [](const CommandState& state, const CommandContext& model) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("sweep")}};
    if (!addSheetInputs(definition, state, result)) {
      return result;
    }
    // An edited 3D sketch's path keeps its direction (mitcad#1550).
    const QJsonValue path = pathOf(state.items(QStringLiteral("path")), false, result, QStringLiteral("path"), model,
                                   keptPathReversed(state, QStringLiteral("path")));
    if (!result.error.isEmpty()) {
      return result;
    }
    if (path.isNull()) {
      return Built::failure(QObject::tr("Select the path."), QStringLiteral("path"));
    }
    definition.insert(QStringLiteral("path"), path);
    if (state.choice(QStringLiteral("orientation")) == QStringLiteral("parallel")) {
      definition.insert(QStringLiteral("orientation"), QStringLiteral("parallel"));
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadSheetInputs(feature, state, context);
    if (const auto path = pathItems(feature.value(QStringLiteral("path")), context)) {
      state.setItems(QStringLiteral("path"), *path);
    }
    keepPathDirection(feature.value(QStringLiteral("path")), state, QStringLiteral("path"));
    state.setChoice(QStringLiteral("orientation"), feature.value(QStringLiteral("orientation")).toString() ==
                                                           QStringLiteral("parallel")
                                                       ? QStringLiteral("parallel")
                                                       : QStringLiteral("perpendicular"));
  };
  // A plain path, as the panel shows it: the solid Sweep's options stay
  // with its panel.
  def.canEdit = [&context](const QJsonObject& feature) {
    for (const char* key : {"guide_rail", "guide_rail2", "guide_surfaces", "twist_angle", "taper_angle",
                            "end_scale", "extent", "binormal", "flip"}) {
      if (feature.contains(QString::fromLatin1(key))) {
        return false;
      }
    }
    const QJsonObject path = feature.value(QStringLiteral("path")).toObject();
    return sheetEditable(feature, context) && !path.value(QStringLiteral("chain")).toBool() &&
           pathItems(path, context).has_value();
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added surface sweep of %1").arg(sheetCount(state));
  };
  return def;
}

// --- Surface Loft

const QString kLoftRails = QStringLiteral("rails");

CommandDef surfaceLoftCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.loft", QObject::tr("Surface Loft"), "surface-loft",
                                  QObject::tr("A surface through sections in order: sketch curves or profiles"));
  def.featureType = QStringLiteral("loft");
  def.inputs = {
      // A 3D sketch's curves anywhere in space and its profiles (closed
      // loops in its sketch plane) too (mitcad#1550).
      selectionInput(QStringLiteral("sections"), QObject::tr("Sections"),
                     SelectKind::Profile | SelectKind::SketchCurve, 2, 0)
          .withTooltip(QObject::tr("Sections in the order the surface goes through them: sketch or 3D sketch "
                                   "curves, or profiles"))
          .withAccepts([&context](const SelectionItem& item) { return spatialPathTakes(item, context); })
          .withRefusedHint([&context](const SelectionItem& item, const CommandState&) {
            return spatialPathRefusal(item, context);
          })
          .withOrdered(),
      // Rails guide a loft of profiles; a 3D sketch's curves in any plane
      // too, typed as a Pipe's path (mitcad#1550).
      listInput(kLoftRails, QObject::tr("Rails"),
                {selectionInput(QStringLiteral("path"), QObject::tr("Rail"), kPathKinds | SelectKind::Sketch, 1, 0)
                     .withTooltip(QObject::tr("Sketch or 3D sketch curves or edges joined end to end, meeting "
                                              "every section"))
                     .withAccepts([&context](const SelectionItem& item) { return spatialPathTakes(item, context); })
                     .withRefusedHint([&context](const SelectionItem& item, const CommandState&) {
                       return spatialPathRefusal(item, context);
                     })},
                QObject::tr("Add Rail"), QObject::tr("Rail %1"), 0),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true)
          .withTooltip(QObject::tr("Also take the curves joined to the selected ones"))
          .withVisible([](const CommandState& state) { return state.rows(kLoftRails) > 0; }),
      checkInput(QStringLiteral("closed"), QObject::tr("Closed"), false)
          .withTooltip(QObject::tr("From the last section back to the first")),
      checkInput(QStringLiteral("ruled"), QObject::tr("Ruled"), false)
          .withTooltip(QObject::tr("Straight between neighboring sections")),
  };
  // A 3D sketch alone enables it (mitcad#1550).
  def.enabled = [&context] { return hasSketches(context, true); };
  def.build = [](const CommandState& state, const CommandContext& model) {
    Built result;
    QJsonArray sections;
    bool curves = false;
    bool profiles = false;
    for (const SelectionItem& item : state.items(QStringLiteral("sections"))) {
      if (item.kind == SelectKind::Profile) {
        profiles = true;
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("profile")},
                                    {QStringLiteral("sketch"), item.owner},
                                    {QStringLiteral("region"), item.name}});
      } else {
        // A planar or a 3D sketch's curve, its chain the section.
        curves = true;
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("curves")},
                                    {QStringLiteral("sketch"), item.owner},
                                    {QStringLiteral("curves"), QJsonArray{item.name}},
                                    {QStringLiteral("chain"), true}});
      }
    }
    if (curves && profiles) {
      return Built::failure(QObject::tr("Select sketch curves or profiles as sections, not both."),
                            QStringLiteral("sections"));
    }
    QJsonArray rails;
    const bool chain = state.checked(QStringLiteral("chain"));
    for (int i = 0; i < state.rows(kLoftRails); ++i) {
      const QString key = CommandState::rowKey(kLoftRails, i, QStringLiteral("path"));
      const Selection picked = state.row(kLoftRails, i).items(QStringLiteral("path"));
      if (picked.isEmpty()) {
        continue;
      }
      // A 3D sketch's curves in their typed form, running as an edited
      // rail ran.
      rails.append(pathOf(picked, chain, result, key, model, keptPathReversed(state, key)));
      if (!result.error.isEmpty()) {
        return result;
      }
    }
    if (curves && !rails.isEmpty()) {
      return Built::failure(QObject::tr("Rails guide a loft of profiles: select profiles as sections, or "
                                        "remove the rails."),
                            kLoftRails);
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("loft")},
                           {QStringLiteral("sections"), sections},
                           {QStringLiteral("surface"), true},
                           {QStringLiteral("operation"), QStringLiteral("new_body")}};
    if (!rails.isEmpty()) {
      definition.insert(kLoftRails, rails);
    }
    if (state.checked(QStringLiteral("closed"))) {
      definition.insert(QStringLiteral("closed"), true);
    }
    if (state.checked(QStringLiteral("ruled"))) {
      definition.insert(QStringLiteral("ruled"), true);
    }
    result.def = definition;
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    Selection sections;
    for (const QJsonValue& value : feature.value(QStringLiteral("sections")).toArray()) {
      const QJsonObject section = value.toObject();
      if (str(section, "type") == QStringLiteral("profile")) {
        sections.append({SelectKind::Profile, str(section, "sketch"), str(section, "region"), QStringLiteral("plane")});
      } else if (const auto items = pathItems(section, context)) {
        sections += *items;
      }
    }
    state.setItems(QStringLiteral("sections"), sections);
    const QJsonArray rails = feature.value(kLoftRails).toArray();
    state.setRows(kLoftRails, static_cast<int>(rails.size()));
    bool chain = rails.isEmpty();
    for (int i = 0; i < rails.size(); ++i) {
      bool chained = false;
      const QString key = CommandState::rowKey(kLoftRails, i, QStringLiteral("path"));
      if (const auto path = pathItems(rails[i], context, &chained)) {
        state.setItems(key, *path);
      }
      // A 3D sketch's rail keeps its direction (mitcad#1550).
      keepPathDirection(rails[i], state, key);
      chain = chain || chained;
    }
    state.setChecked(QStringLiteral("chain"), chain);
    state.setChecked(QStringLiteral("closed"), feature.value(QStringLiteral("closed")).toBool());
    state.setChecked(QStringLiteral("ruled"), feature.value(QStringLiteral("ruled")).toBool());
  };
  // Sections of one curve each (chained) or profiles, and rails of curves
  // or edges chained alike (one Chain Selection holds them), without end
  // conditions or a centre line.
  def.canEdit = [&context](const QJsonObject& feature) {
    if (!feature.value(QStringLiteral("surface")).toBool()) {
      return false;
    }
    for (const char* key : {"start_condition", "end_condition", "centerline"}) {
      if (feature.contains(QString::fromLatin1(key))) {
        return false;
      }
    }
    std::optional<bool> chained;
    for (const QJsonValue& rail : feature.value(kLoftRails).toArray()) {
      const QJsonObject o = rail.toObject();
      const bool chain = o.value(QStringLiteral("chain")).toBool();
      const bool shown = o.contains(QStringLiteral("curves")) || o.contains(QStringLiteral("curve")) ||
                         o.contains(QStringLiteral("edges")) || o.contains(QStringLiteral("feature"));
      if (!shown || !pathItems(rail, context) || (chained && *chained != chain)) {
        return false;
      }
      chained = chain;
    }
    for (const QJsonValue& value : feature.value(QStringLiteral("sections")).toArray()) {
      const QJsonObject section = value.toObject();
      const QString type = str(section, "type");
      if (type == QStringLiteral("curves")) {
        if (section.value(QStringLiteral("curves")).toArray().size() != 1 ||
            !section.value(QStringLiteral("chain")).toBool()) {
          return false;
        }
      } else if (type != QStringLiteral("profile")) {
        return false;
      }
    }
    return true;
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added surface loft through %1 sections").arg(state.items(QStringLiteral("sections")).size());
  };
  return def;
}

// --- Patch, Stitch and Thicken (mitcad#456)

CommandDef patchCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.patch", QObject::tr("Patch"), "patch",
                                  QObject::tr("Fills sketch profiles, or a closed loop of a body's edges, with "
                                              "surfaces"));
  def.pinned = true;
  def.keywords << QStringLiteral("fill") << QStringLiteral("cap");
  def.featureType = QStringLiteral("patch");
  def.inputs = {
      selectionInput(QStringLiteral("boundary"), QObject::tr("Profiles or Edges"),
                     SelectKind::Profile | SelectKind::Edge, 1, 0)
          .withTooltip(QObject::tr("Profiles of one sketch, or edges of one body that close up into a loop"))
          .withoutMeshes(),
      choiceInput(QStringLiteral("continuity"), QObject::tr("Continuity"), kContinuity, QStringLiteral("g0"))
          .withTooltip(QObject::tr("How a patch of edges meets the faces beside them")),
  };
  def.enabled = [&context] { return hasProfiles(context) || hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    Selection profiles;
    Selection edges;
    for (const SelectionItem& item : state.items(QStringLiteral("boundary"))) {
      (item.kind == SelectKind::Profile ? profiles : edges).append(item);
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("patch")}};
    if (!profiles.isEmpty()) {
      if (oneSketch(profiles, result, QStringLiteral("boundary")).isEmpty()) {
        return result;
      }
      definition.insert(QStringLiteral("profiles"), refsOf(profiles));
    }
    if (!edges.isEmpty()) {
      const QString body = oneBody(edges, result, QStringLiteral("boundary"));
      if (body.isEmpty()) {
        return result;
      }
      QJsonObject loop{{QStringLiteral("body"), body}, {QStringLiteral("edges"), names(edges, SelectKind::Edge)}};
      if (state.choice(QStringLiteral("continuity")) != QStringLiteral("g0")) {
        loop.insert(QStringLiteral("continuity"), state.choice(QStringLiteral("continuity")));
      }
      definition.insert(QStringLiteral("boundary"), loop);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    Selection items = profilesOf(feature.value(QStringLiteral("profiles")).toArray());
    const QJsonObject loop = feature.value(QStringLiteral("boundary")).toObject();
    if (!loop.isEmpty()) {
      if (const auto edges = pathItems(loop, context)) {
        items += *edges;
      }
      state.setChoice(QStringLiteral("continuity"),
                      loop.value(QStringLiteral("continuity")).toString(QStringLiteral("g0")));
    }
    state.setItems(QStringLiteral("boundary"), items);
  };
  // Named edges with one continuity, as the panel shows them.
  def.canEdit = [](const QJsonObject& feature) {
    const QJsonObject loop = feature.value(QStringLiteral("boundary")).toObject();
    return loop.isEmpty() ||
           (loop.contains(QStringLiteral("edges")) && !loop.contains(QStringLiteral("edge_continuity")) &&
            !loop.contains(QStringLiteral("points")));
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added patch of %1 items").arg(state.items(QStringLiteral("boundary")).size());
  };
  return def;
}

CommandDef stitchCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.stitch", QObject::tr("Stitch"), "stitch",
                                  QObject::tr("Sews bodies' faces together where their edges meet; a solid "
                                              "where they close up"));
  def.keywords << QStringLiteral("sew") << QStringLiteral("knit");
  def.featureType = QStringLiteral("stitch");
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("The first keeps its name and becomes the sewn body"))
          .withoutMeshes(),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Tolerance"), ValueKind::Length, QStringLiteral("0.01 mm")).withStepMinimum(0.0)
          .withTooltip(QObject::tr("Edges this close are sewn; 0 for the kernel's own precision")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    return Built::of({{QStringLiteral("type"), QStringLiteral("stitch")},
                      {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))},
                      {QStringLiteral("tolerance"), state.expression(QStringLiteral("tolerance"))}});
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("bodies"), bodiesOf(feature.value(QStringLiteral("bodies")).toArray()));
    loadValue(state, QStringLiteral("tolerance"), feature.value(QStringLiteral("tolerance")), feature, context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added stitch of %1 bodies").arg(state.items(QStringLiteral("bodies")).size());
  };
  return def;
}

CommandDef thickenCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.thicken", QObject::tr("Thicken"), "thicken",
                                  QObject::tr("Makes surface bodies solid by a wall along their normals"));
  def.pinned = true;
  def.featureType = QStringLiteral("thicken");
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Surfaces"), SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("Surface bodies; they stay as they are"))
          .withoutMeshes(),
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("1 mm"))
          .withTooltip(QObject::tr("Along the faces' normals; negative against them")),
      checkInput(QStringLiteral("symmetric"), QObject::tr("Symmetric"), false)
          .withTooltip(QObject::tr("Half of the thickness to each side")),
  };
  def.inputs += operationInputs();
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("thicken")},
                           {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))},
                           {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness"))}};
    if (state.checked(QStringLiteral("symmetric"))) {
      definition.insert(QStringLiteral("symmetric"), true);
    }
    addOperation(definition, state);
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("bodies"), bodiesOf(feature.value(QStringLiteral("bodies")).toArray()));
    loadValue(state, QStringLiteral("thickness"), feature.value(QStringLiteral("thickness")), feature, context);
    state.setChecked(QStringLiteral("symmetric"), feature.value(QStringLiteral("symmetric")).toBool());
    loadOperation(feature, state);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added thicken (%1) of %2 bodies")
        .arg(label(kOperations, state.choice(QStringLiteral("operation"))))
        .arg(state.items(QStringLiteral("bodies")).size());
  };
  return def;
}

// --- Offset Surface, Ruled Surface and Extend

// Face items for face names of a body.
Selection facesOf(const QString& body, const QJsonArray& faces, const CommandContext& context) {
  Selection items;
  for (const QJsonValue& face : faces) {
    const SelectionItem item = itemOf(QJsonObject{{QStringLiteral("body"), body}, {QStringLiteral("face"), face}}, context);
    if (item.isValid()) {
      items.append(item);
    }
  }
  return items;
}

// The edges picked (named) and the faces whose open edges are meant, into
// `edges` and `openings`; the body they are all of, empty (with the error)
// when they are not of one.
QString addEdgesOf(QJsonObject& def, const CommandState& state, Built& result, const QString& edgesKey,
                   const QString& facesKey) {
  Selection all = state.items(edgesKey);
  all += state.items(facesKey);
  if (all.isEmpty()) {
    result = Built::failure(QObject::tr("Select edges, or faces whose open edges are meant."), edgesKey);
    return QString();
  }
  const QString body = oneBody(all, result, edgesKey);
  if (body.isEmpty()) {
    return body;
  }
  def.insert(QStringLiteral("body"), body);
  const QJsonArray edges = names(state.items(edgesKey), SelectKind::Edge);
  if (!edges.isEmpty()) {
    def.insert(QStringLiteral("edges"), edges);
  }
  const QJsonArray faces = names(state.items(facesKey), SelectKind::Face);
  if (!faces.isEmpty()) {
    def.insert(QStringLiteral("openings"), QJsonArray{QJsonObject{{QStringLiteral("faces"), faces}}});
  }
  return body;
}

QString addSheetEdges(QJsonObject& def, const CommandState& state, Built& result) {
  return addEdgesOf(def, state, result, QStringLiteral("edges"), QStringLiteral("open_faces"));
}

void loadEdgesOf(const QJsonObject& feature, CommandState& state, const CommandContext& context,
                 const QString& edgesKey, const QString& facesKey) {
  const QString body = str(feature, "body");
  if (const auto edges = pathItems(QJsonObject{{QStringLiteral("body"), body},
                                               {QStringLiteral("edges"), feature.value(QStringLiteral("edges"))}},
                                   context)) {
    state.setItems(edgesKey, *edges);
  }
  const QJsonArray openings = feature.value(QStringLiteral("openings")).toArray();
  if (!openings.isEmpty()) {
    state.setItems(facesKey,
                   facesOf(body, openings.first().toObject().value(QStringLiteral("faces")).toArray(), context));
  }
}

void loadSheetEdges(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  loadEdgesOf(feature, state, context, QStringLiteral("edges"), QStringLiteral("open_faces"));
}

// Openings as the panel shows them: one, every opening of the faces.
bool sheetEdgesEditable(const QJsonObject& feature) {
  const QJsonArray openings = feature.value(QStringLiteral("openings")).toArray();
  if (openings.isEmpty()) {
    return true;
  }
  const QJsonObject opening = openings.first().toObject();
  return openings.size() == 1 && !opening.contains(QStringLiteral("opening")) &&
         !opening.contains(QStringLiteral("at")) && !opening.value(QStringLiteral("faces")).toArray().isEmpty();
}

QVector<InputDef> sheetEdgeInputs() {
  return {
      selectionInput(QStringLiteral("edges"), QObject::tr("Edges"), SelectKind::Edge, 0, 0)
          .withTooltip(QObject::tr("Edges of one body"))
          .withoutMeshes(),
      selectionInput(QStringLiteral("open_faces"), QObject::tr("Open Edges Of"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Faces of a surface body: their open edges (which cannot be picked "
                                   "themselves) are taken"))
          .withoutMeshes(),
  };
}

CommandDef offsetSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.offset", QObject::tr("Offset Surface"), "offset-surface",
                                  QObject::tr("Copies faces out along their normals as a new surface"));
  def.featureType = QStringLiteral("offset_surface");
  def.keywords << QStringLiteral("offset") << QStringLiteral("copy faces");
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
          .withTooltip(QObject::tr("Faces of one body"))
          .withoutMeshes(),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("1 mm"))
          .withTooltip(QObject::tr("Along the faces' normals; negative against them, 0 copies them")),
      checkInput(QStringLiteral("chain"), QObject::tr("Tangent Chain"), false)
          .withTooltip(QObject::tr("Also the faces joined to them through smooth edges")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("faces"));
    const QString body = oneBody(faces, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("offset_surface")},
                           {QStringLiteral("body"), body},
                           {QStringLiteral("faces"), names(faces, SelectKind::Face)},
                           {QStringLiteral("distance"), state.expression(QStringLiteral("distance"))}};
    if (state.checked(QStringLiteral("chain"))) {
      definition.insert(QStringLiteral("chain"), true);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"),
                   facesOf(str(feature, "body"), feature.value(QStringLiteral("faces")).toArray(), context));
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, context);
    state.setChecked(QStringLiteral("chain"), feature.value(QStringLiteral("chain")).toBool());
  };
  // Every face of the body (no faces named) has no pick to show.
  def.canEdit = [](const QJsonObject& feature) {
    return !feature.value(QStringLiteral("faces")).toArray().isEmpty();
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added offset surface of %1 faces").arg(state.items(QStringLiteral("faces")).size());
  };
  return def;
}

CommandDef ruledSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.ruled", QObject::tr("Ruled Surface"), "ruled-surface",
                                  QObject::tr("A strip of surface off a chain of edges"));
  def.featureType = QStringLiteral("ruled_surface");
  def.keywords << QStringLiteral("flange surface") << QStringLiteral("strip");
  const auto angled = choiceIs(QStringLiteral("mode"), QStringLiteral("angle"));
  def.inputs = sheetEdgeInputs();
  def.inputs += {
      selectionInput(QStringLiteral("face"), QObject::tr("Face Beside"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("For edges between two faces: the face the strip leaves"))
          .withoutMeshes(),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("5 mm")).withStepMinimum(0.0, true),
      choiceInput(QStringLiteral("mode"), QObject::tr("Direction"),
                  {{QStringLiteral("normal"), QStringLiteral("Normal")},
                   {QStringLiteral("tangent"), QStringLiteral("Tangent")},
                   {QStringLiteral("angle"), QStringLiteral("Angle")}},
                  QStringLiteral("normal"))
          .withTooltip(QObject::tr("Along the face's normal, continuing the face, or at an angle from the "
                                   "normal")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("45 deg"))
          .withVisible(angled),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      choiceInput(QStringLiteral("corners"), QObject::tr("Corners"),
                  {{QStringLiteral("mitre"), QStringLiteral("Miter")}, {QStringLiteral("round"), QStringLiteral("Round")}},
                  QStringLiteral("mitre")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("ruled_surface")}};
    const QString body = addSheetEdges(definition, state, result);
    if (body.isEmpty()) {
      return result;
    }
    const Selection& face = state.items(QStringLiteral("face"));
    if (!face.isEmpty()) {
      if (face.first().owner != body) {
        return Built::failure(QObject::tr("The face beside must be of the edges' body."), QStringLiteral("face"));
      }
      definition.insert(QStringLiteral("face"), face.first().name);
    }
    definition.insert(QStringLiteral("distance"), state.expression(QStringLiteral("distance")));
    const QString mode = state.choice(QStringLiteral("mode"));
    if (mode != QStringLiteral("normal")) {
      definition.insert(QStringLiteral("mode"), mode);
    }
    if (mode == QStringLiteral("angle")) {
      definition.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
    }
    if (state.checked(QStringLiteral("flip"))) {
      definition.insert(QStringLiteral("flip"), true);
    }
    if (state.choice(QStringLiteral("corners")) == QStringLiteral("round")) {
      definition.insert(QStringLiteral("corners"), QStringLiteral("round"));
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadSheetEdges(feature, state, context);
    if (feature.contains(QStringLiteral("face"))) {
      state.setItems(QStringLiteral("face"),
                     facesOf(str(feature, "body"), QJsonArray{feature.value(QStringLiteral("face"))}, context));
    }
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, context);
    const QString mode = str(feature, "mode");
    state.setChoice(QStringLiteral("mode"), mode.isEmpty() ? QStringLiteral("normal") : mode);
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    const QString corners = str(feature, "corners");
    state.setChoice(QStringLiteral("corners"), corners.isEmpty() ? QStringLiteral("mitre") : corners);
  };
  // A fixed direction has no input.
  def.canEdit = [](const QJsonObject& feature) {
    return !feature.contains(QStringLiteral("direction")) && sheetEdgesEditable(feature);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added ruled surface off %1 edges and the open edges of %2 faces")
        .arg(state.items(QStringLiteral("edges")).size())
        .arg(state.items(QStringLiteral("open_faces")).size());
  };
  return def;
}

CommandDef extendSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.extend", QObject::tr("Extend"), "surface-extend",
                                  QObject::tr("Extends a surface body past its open edges"));
  def.featureType = QStringLiteral("extend_surface");
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.keywords << QStringLiteral("extend surface") << QStringLiteral("lengthen");
  def.inputs = sheetEdgeInputs();
  def.inputs += {
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("5 mm")).withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("Along the surface, past the edges")),
      choiceInput(QStringLiteral("mode"), QObject::tr("Extension"),
                  {{QStringLiteral("natural"), QStringLiteral("Natural")},
                   {QStringLiteral("tangent"), QStringLiteral("Tangent")},
                   {QStringLiteral("curvature"), QStringLiteral("Curvature")}},
                  QStringLiteral("natural"))
          .withTooltip(QObject::tr("The surface continued, or leaving the edge tangent to it or also with "
                                   "its curvature")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("extend_surface")}};
    if (addSheetEdges(definition, state, result).isEmpty()) {
      return result;
    }
    definition.insert(QStringLiteral("distance"), state.expression(QStringLiteral("distance")));
    if (state.choice(QStringLiteral("mode")) != QStringLiteral("natural")) {
      definition.insert(QStringLiteral("mode"), state.choice(QStringLiteral("mode")));
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadSheetEdges(feature, state, context);
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, context);
    const QString mode = str(feature, "mode");
    state.setChoice(QStringLiteral("mode"), mode.isEmpty() ? QStringLiteral("natural") : mode);
  };
  def.canEdit = [](const QJsonObject& feature) { return sheetEdgesEditable(feature); };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added extend of %1 edges and the open edges of %2 faces")
        .arg(state.items(QStringLiteral("edges")).size())
        .arg(state.items(QStringLiteral("open_faces")).size());
  };
  return def;
}

// --- Trim, Untrim, Unstitch and Reverse Normal

// The state's record of a loaded trim's keep points, used while no new
// pieces are clicked.
const QString kKept = QStringLiteral("kept_points");

CommandDef trimSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.trim", QObject::tr("Trim"), "surface-trim",
                                  QObject::tr("Cuts a surface body and keeps the pieces clicked"));
  def.featureType = QStringLiteral("trim_surface");
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.extendedToolPreview = true; // the trimming tool as it is extended (mitcad#712)
  def.keywords << QStringLiteral("trim surface") << QStringLiteral("cut");
  def.inputs = {
      selectionInput(QStringLiteral("body"), QObject::tr("Surface"), SelectKind::Body, 1, 1)
          .withTooltip(QObject::tr("A surface body; a solid is cut by Split Body"))
          .withoutMeshes(),
      toolInput(QObject::tr("Trimming Tool")),
      checkInput(QStringLiteral("extend"), QObject::tr("Extend Trimming Tool"), true),
      toolOffsetInput(),
  };
  def.inputs += directionInputs();
  def.inputs += {
      selectionInput(QStringLiteral("keep"), QObject::tr("Keep"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Click the surface on each piece to keep"))
          .withoutMeshes(),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& body = state.items(QStringLiteral("body"));
    if (body.isEmpty()) {
      return Built::failure(QObject::tr("Select a surface body."), QStringLiteral("body"));
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("trim_surface")},
                           {QStringLiteral("body"), body.first().owner}};
    if (!addTool(definition, state, result)) {
      return result;
    }
    QJsonArray keep;
    for (const SelectionItem& item : state.items(QStringLiteral("keep"))) {
      if (item.owner != body.first().owner || !item.at) {
        return Built::failure(QObject::tr("Click the surface itself on the pieces to keep."), QStringLiteral("keep"));
      }
      keep.append(QJsonArray{(*item.at)[0], (*item.at)[1], (*item.at)[2]});
    }
    if (keep.isEmpty()) {
      keep = state.object(kKept).value(QStringLiteral("points")).toArray();
    }
    if (keep.isEmpty()) {
      return Built::failure(QObject::tr("Click the surface on the pieces to keep."), QStringLiteral("keep"));
    }
    definition.insert(QStringLiteral("keep"), keep);
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("body"), bodiesOf(QJsonArray{feature.value(QStringLiteral("body"))}));
    loadTool(feature, state, context);
    state.setObject(kKept, {{QStringLiteral("points"), feature.value(QStringLiteral("keep"))}});
  };
  def.canEdit = [&context](const QJsonObject& feature) { return toolEditable(feature, context); };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const qsizetype clicked = state.items(QStringLiteral("keep")).size();
    return QStringLiteral("Added trim keeping %1 pieces")
        .arg(clicked > 0 ? clicked : state.object(kKept).value(QStringLiteral("points")).toArray().size());
  };
  return def;
}

CommandDef untrimSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.untrim", QObject::tr("Untrim"), "surface-untrim",
                                  QObject::tr("Restores a face towards its surface: holes closed, outline put back"));
  def.featureType = QStringLiteral("untrim_surface");
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.keywords << QStringLiteral("untrim surface") << QStringLiteral("fill holes");
  def.inputs = {
      selectionInput(QStringLiteral("face"), QObject::tr("Face"), SelectKind::Face, 1, 1).withoutMeshes(),
      checkInput(QStringLiteral("holes"), QObject::tr("Close Holes"), true)
          .withTooltip(QObject::tr("Takes the face's holes away")),
      checkInput(QStringLiteral("outer"), QObject::tr("Outer Boundary"), false)
          .withTooltip(QObject::tr("Puts the outer boundary back at the surface's own bounds")),
      checkInput(QStringLiteral("in_place"), QObject::tr("In Place"), false)
          .withTooltip(QObject::tr("Changes the face of a surface body where it is, instead of a new surface")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    const Selection& face = state.items(QStringLiteral("face"));
    if (face.isEmpty()) {
      return Built::failure(QObject::tr("Select a face."), QStringLiteral("face"));
    }
    const bool holes = state.checked(QStringLiteral("holes"));
    const bool outer = state.checked(QStringLiteral("outer"));
    if (!holes && !outer) {
      return Built::failure(QObject::tr("Close the holes, put the outer boundary back, or both."),
                            QStringLiteral("holes"));
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("untrim_surface")},
                           {QStringLiteral("body"), face.first().owner},
                           {QStringLiteral("face"), face.first().name}};
    if (!holes) {
      definition.insert(QStringLiteral("holes"), false);
    }
    if (outer) {
      definition.insert(QStringLiteral("outer"), true);
    }
    if (state.checked(QStringLiteral("in_place"))) {
      definition.insert(QStringLiteral("in_place"), true);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("face"),
                   facesOf(str(feature, "body"), QJsonArray{feature.value(QStringLiteral("face"))}, context));
    state.setChecked(QStringLiteral("holes"), feature.value(QStringLiteral("holes")).toBool(true));
    state.setChecked(QStringLiteral("outer"), feature.value(QStringLiteral("outer")).toBool());
    state.setChecked(QStringLiteral("in_place"), feature.value(QStringLiteral("in_place")).toBool());
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added untrim (holes %1, outer %2)")
        .arg(state.checked(QStringLiteral("holes")) ? QStringLiteral("closed") : QStringLiteral("kept"))
        .arg(state.checked(QStringLiteral("outer")) ? QStringLiteral("restored") : QStringLiteral("kept"));
  };
  return def;
}

// Unstitch and Reverse Normal: faces of one body, or a body (every face).
CommandDef bodyFacesCommand(const char* id, const QString& name, const char* icon, const QString& tooltip,
                            const QString& type, const CommandContext& context) {
  CommandDef def = surfaceCommand(id, name, icon, tooltip);
  def.featureType = type;
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces/Body"), SelectKind::Face | SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("Faces of one body, or a body for all of its faces"))
          .withoutMeshes(),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [type](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& items = state.items(QStringLiteral("faces"));
    const QString body = oneBody(items, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result.error.isEmpty() ? Built::failure(QObject::tr("Select faces or a body."), QStringLiteral("faces"))
                                    : result;
    }
    QJsonObject definition{{QStringLiteral("type"), type}, {QStringLiteral("body"), body}};
    const QJsonArray faces = names(items, SelectKind::Face);
    if (!faces.isEmpty()) {
      definition.insert(QStringLiteral("faces"), faces);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QJsonArray faces = feature.value(QStringLiteral("faces")).toArray();
    state.setItems(QStringLiteral("faces"), faces.isEmpty()
                                                ? bodiesOf(QJsonArray{feature.value(QStringLiteral("body"))})
                                                : facesOf(str(feature, "body"), faces, context));
  };
  def.describe = [name](const CommandState& state, const CommandContext&) {
    const Selection& items = state.items(QStringLiteral("faces"));
    const qsizetype faces = names(items, SelectKind::Face).size();
    return faces > 0 ? QStringLiteral("Added %1 of %2 faces").arg(name.toLower()).arg(faces)
                     : QStringLiteral("Added %1 of a body").arg(name.toLower());
  };
  return def;
}

CommandDef unstitchCommand(const CommandContext& context) {
  CommandDef def = bodyFacesCommand("surface.unstitch", QObject::tr("Unstitch"), "unstitch",
                                    QObject::tr("Takes faces out of a body as surface bodies of their own"),
                                    QStringLiteral("unstitch"), context);
  def.keywords << QStringLiteral("explode") << QStringLiteral("separate faces");
  return def;
}

CommandDef reverseNormalCommand(const CommandContext& context) {
  CommandDef def = bodyFacesCommand("surface.reverse_normal", QObject::tr("Reverse Normal"), "reverse-normal",
                                    QObject::tr("Turns a surface body's faces over"),
                                    QStringLiteral("reverse_normal"), context);
  def.keywords << QStringLiteral("flip normal") << QStringLiteral("orientation");
  return def;
}

// --- Blend Surface, Boundary Surface, Match Surface and Fair Surface

const QString kG1 = QStringLiteral("g1");
const QString kG2 = QStringLiteral("g2");

// "<prefix>_<name>".
QString key(const QString& prefix, const char* name) { return prefix + QLatin1Char('_') + QLatin1String(name); }

// A face beside a chain into `def`; false (with the error) when it is not
// of the chain's body.
bool addFaceBeside(QJsonObject& def, const CommandState& state, const QString& input, const QString& body,
                   Built& result, const char* field = "face") {
  const Selection& face = state.items(input);
  if (face.isEmpty()) {
    return true;
  }
  if (face.first().owner != body) {
    result = Built::failure(QObject::tr("The face beside must be of the edges' body."), input);
    return false;
  }
  def.insert(QLatin1String(field), face.first().name);
  return true;
}

void loadFaceBeside(const QJsonObject& def, CommandState& state, const QString& input, const QString& body,
                    const CommandContext& context, const char* field = "face") {
  if (def.contains(QLatin1String(field))) {
    state.setItems(input, facesOf(body, QJsonArray{def.value(QLatin1String(field))}, context));
  }
}

// One end of a blend: its edges, open edges, face beside, continuity and
// bulge, the inputs "<prefix>_...".
QVector<InputDef> blendSideInputs(const QString& prefix, const QString& which) {
  return {
      selectionInput(key(prefix, "edges"), QObject::tr("%1 Edges").arg(which), SelectKind::Edge, 0, 0)
          .withTooltip(QObject::tr("Edges of one body that join into a chain"))
          .withoutMeshes(),
      selectionInput(key(prefix, "open_faces"), QObject::tr("%1 Open Edges Of").arg(which), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Faces of a surface body: their open edges are taken"))
          .withoutMeshes(),
      selectionInput(key(prefix, "face"), QObject::tr("%1 Face Beside").arg(which), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("For edges between two faces: the face the blend continues"))
          .withoutMeshes(),
      choiceInput(key(prefix, "continuity"), QObject::tr("%1 Continuity").arg(which), kContinuity, kG1),
      valueInput(key(prefix, "bulge"), QObject::tr("%1 Bulge").arg(which), ValueKind::Unitless, QStringLiteral("0"))
          .withTooltip(QObject::tr("How full the blend leaves: 0 plain, more fuller, down to -0.9 flatter")),
  };
}

bool addBlendSide(QJsonObject& def, const CommandState& state, const QString& prefix, Built& result) {
  QJsonObject side;
  const QString body = addEdgesOf(side, state, result, key(prefix, "edges"), key(prefix, "open_faces"));
  if (body.isEmpty() || !addFaceBeside(side, state, key(prefix, "face"), body, result)) {
    return false;
  }
  const QString continuity = state.choice(key(prefix, "continuity"));
  if (continuity != kG1) {
    side.insert(QStringLiteral("continuity"), continuity);
  }
  const double bulge = state.value(key(prefix, "bulge"));
  if (!std::isfinite(bulge) || bulge < -0.9) {
    result = Built::failure(QObject::tr("The bulge must be -0.9 or more."), key(prefix, "bulge"));
    return false;
  }
  if (bulge != 0.0) {
    side.insert(QStringLiteral("bulge"), bulge);
  }
  def.insert(prefix, side);
  return true;
}

void loadBlendSide(const QJsonObject& feature, CommandState& state, const QString& prefix,
                   const CommandContext& context) {
  const QJsonObject side = feature.value(prefix).toObject();
  loadEdgesOf(side, state, context, key(prefix, "edges"), key(prefix, "open_faces"));
  loadFaceBeside(side, state, key(prefix, "face"), str(side, "body"), context);
  const QString continuity = str(side, "continuity");
  state.setChoice(key(prefix, "continuity"), continuity.isEmpty() ? kG1 : continuity);
  state.setText(key(prefix, "bulge"), number(side.value(QStringLiteral("bulge")).toDouble(0.0)));
}

CommandDef blendSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.blend", QObject::tr("Blend Surface"), "blend-surface",
                                  QObject::tr("A surface bridging two chains of edges, tangent or curved as the "
                                              "faces it leaves"));
  def.featureType = QStringLiteral("blend_surface");
  def.keywords << QStringLiteral("bridge") << QStringLiteral("blend");
  def.inputs = blendSideInputs(QStringLiteral("from"), QObject::tr("First"));
  def.inputs += blendSideInputs(QStringLiteral("to"), QObject::tr("Second"));
  def.inputs += {
      checkInput(QStringLiteral("stitch"), QObject::tr("Stitch"), false)
          .withTooltip(QObject::tr("Sews the blend onto both bodies, into the first")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("blend_surface")}};
    if (!addBlendSide(definition, state, QStringLiteral("from"), result) ||
        !addBlendSide(definition, state, QStringLiteral("to"), result)) {
      return result;
    }
    if (state.checked(QStringLiteral("stitch"))) {
      definition.insert(QStringLiteral("stitch"), true);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadBlendSide(feature, state, QStringLiteral("from"), context);
    loadBlendSide(feature, state, QStringLiteral("to"), context);
    state.setChecked(QStringLiteral("stitch"), feature.value(QStringLiteral("stitch")).toBool());
  };
  def.canEdit = [](const QJsonObject& feature) {
    return sheetEdgesEditable(feature.value(QStringLiteral("from")).toObject()) &&
           sheetEdgesEditable(feature.value(QStringLiteral("to")).toObject());
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added blend surface (%1, %2)")
        .arg(state.choice(QStringLiteral("from_continuity")), state.choice(QStringLiteral("to_continuity")));
  };
  return def;
}

const QString kLoop = QStringLiteral("loop");
const QString kNetwork = QStringLiteral("network");

// Each sketch curve picked as a curve of its own.
QJsonArray singleCurves(const Selection& items) {
  QJsonArray curves;
  for (const SelectionItem& item : items) {
    if (item.kind == SelectKind::SketchCurve) {
      curves.append(QJsonObject{{QStringLiteral("sketch"), item.owner},
                                {QStringLiteral("curves"), QJsonArray{item.name}}});
    }
  }
  return curves;
}

// The sketch curves of single-curve entries; none when an entry is other.
std::optional<Selection> singleCurveItems(const QJsonArray& entries, const CommandContext& context) {
  Selection items;
  for (const QJsonValue& entry : entries) {
    const QJsonObject o = entry.toObject();
    const QJsonArray curves = o.value(QStringLiteral("curves")).toArray();
    if (!o.contains(QStringLiteral("sketch")) || curves.size() != 1 || o.value(QStringLiteral("chain")).toBool()) {
      return std::nullopt;
    }
    const auto found = pathItems(o, context);
    if (!found || found->size() != 1) {
      return std::nullopt;
    }
    items += *found;
  }
  return items;
}

CommandDef boundarySurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.boundary", QObject::tr("Boundary Surface"), "boundary-surface",
                                  QObject::tr("A surface filling a loop of curves, or through a network of curves"));
  def.featureType = QStringLiteral("boundary_surface");
  def.keywords << QStringLiteral("network") << QStringLiteral("fill") << QStringLiteral("coons");
  const auto loop = choiceIs(QStringLiteral("mode"), kLoop);
  const auto network = choiceIs(QStringLiteral("mode"), kNetwork);
  def.inputs = {
      choiceInput(QStringLiteral("mode"), QObject::tr("Type"),
                  {{kLoop, QStringLiteral("Fill a Loop")}, {kNetwork, QStringLiteral("Network")}}, kLoop),
      selectionInput(QStringLiteral("curves"), QObject::tr("Boundary Curves"), SelectKind::SketchCurve, 0, 0)
          .withTooltip(QObject::tr("Sketch curves of one sketch round the loop"))
          .withVisible(loop),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true)
          .withTooltip(QObject::tr("Also take the curves joined to the selected ones"))
          .withVisible(loop),
      checkInput(QStringLiteral("tangent"), QObject::tr("Tangent to Sketch Plane"), false)
          .withTooltip(QObject::tr("The surface leaves the curves along their sketch's plane"))
          .withVisible(loop),
      selectionInput(QStringLiteral("open_faces"), QObject::tr("Open Edges Of"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Faces of a surface body: their open edges close the loop"))
          .withoutMeshes()
          .withVisible(loop),
      choiceInput(QStringLiteral("continuity"), QObject::tr("Edge Continuity"), kContinuity,
                  QStringLiteral("g0"))
          .withVisible(loop),
      selectionInput(QStringLiteral("u"), QObject::tr("Curves One Way"), SelectKind::SketchCurve, 0, 0)
          .withTooltip(QObject::tr("Two or more, the surface lofted through them in order"))
          .withVisible(network),
      selectionInput(QStringLiteral("v"), QObject::tr("Crossing Curves"), SelectKind::SketchCurve, 0, 0)
          .withTooltip(QObject::tr("None, or two or more crossing every curve one way"))
          .withVisible(network),
  };
  def.enabled = [&context] { return hasSketches(context) || hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("boundary_surface")}};
    if (state.choice(QStringLiteral("mode")) == kNetwork) {
      const QJsonArray u = singleCurves(state.items(QStringLiteral("u")));
      const QJsonArray v = singleCurves(state.items(QStringLiteral("v")));
      if (u.size() < 2) {
        return Built::failure(QObject::tr("Select two curves or more one way."), QStringLiteral("u"));
      }
      if (v.size() == 1) {
        return Built::failure(QObject::tr("Select two crossing curves or more, or none."), QStringLiteral("v"));
      }
      definition.insert(QStringLiteral("u"), u);
      if (!v.isEmpty()) {
        definition.insert(QStringLiteral("v"), v);
      }
      return Built::of(definition);
    }
    QJsonArray boundary;
    const Selection& curves = state.items(QStringLiteral("curves"));
    if (!curves.isEmpty()) {
      const QString sketch = oneSketch(curves, result, QStringLiteral("curves"));
      if (sketch.isEmpty()) {
        return result;
      }
      QJsonObject entry{{QStringLiteral("sketch"), sketch},
                        {QStringLiteral("curves"), names(curves, SelectKind::SketchCurve)}};
      if (state.checked(QStringLiteral("chain"))) {
        entry.insert(QStringLiteral("chain"), true);
      }
      if (state.checked(QStringLiteral("tangent"))) {
        entry.insert(QStringLiteral("continuity"), kG1);
      }
      boundary.append(entry);
    }
    const Selection& faces = state.items(QStringLiteral("open_faces"));
    if (!faces.isEmpty()) {
      const QString body = oneBody(faces, result, QStringLiteral("open_faces"));
      if (body.isEmpty()) {
        return result;
      }
      QJsonObject entry{{QStringLiteral("body"), body},
                        {QStringLiteral("openings"),
                         QJsonArray{QJsonObject{{QStringLiteral("faces"), names(faces, SelectKind::Face)}}}}};
      if (state.choice(QStringLiteral("continuity")) != QStringLiteral("g0")) {
        entry.insert(QStringLiteral("continuity"), state.choice(QStringLiteral("continuity")));
      }
      boundary.append(entry);
    }
    if (boundary.isEmpty()) {
      return Built::failure(QObject::tr("Select the curves or open edges round the loop."), QStringLiteral("curves"));
    }
    definition.insert(QStringLiteral("boundary"), boundary);
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    if (feature.contains(QStringLiteral("u"))) {
      state.setChoice(QStringLiteral("mode"), kNetwork);
      if (const auto u = singleCurveItems(feature.value(QStringLiteral("u")).toArray(), context)) {
        state.setItems(QStringLiteral("u"), *u);
      }
      if (const auto v = singleCurveItems(feature.value(QStringLiteral("v")).toArray(), context)) {
        state.setItems(QStringLiteral("v"), *v);
      }
      return;
    }
    state.setChoice(QStringLiteral("mode"), kLoop);
    for (const QJsonValue& value : feature.value(QStringLiteral("boundary")).toArray()) {
      const QJsonObject entry = value.toObject();
      if (entry.contains(QStringLiteral("sketch"))) {
        bool chain = false;
        if (const auto items = pathItems(entry, context, &chain)) {
          state.setItems(QStringLiteral("curves"), *items);
        }
        state.setChecked(QStringLiteral("chain"), entry.value(QStringLiteral("chain")).toBool());
        state.setChecked(QStringLiteral("tangent"), str(entry, "continuity") == kG1);
      } else {
        const QJsonArray openings = entry.value(QStringLiteral("openings")).toArray();
        if (!openings.isEmpty()) {
          state.setItems(QStringLiteral("open_faces"),
                         facesOf(str(entry, "body"),
                                 openings.first().toObject().value(QStringLiteral("faces")).toArray(), context));
        }
        const QString continuity = str(entry, "continuity");
        state.setChoice(QStringLiteral("continuity"), continuity.isEmpty() ? QStringLiteral("g0") : continuity);
      }
    }
  };
  // The panel shows one chain of one sketch's curves and the open edges of
  // faces of one body (loop), or single curves (network).
  def.canEdit = [&context](const QJsonObject& feature) {
    if (feature.contains(QStringLiteral("u"))) {
      return singleCurveItems(feature.value(QStringLiteral("u")).toArray(), context).has_value() &&
             singleCurveItems(feature.value(QStringLiteral("v")).toArray(), context).has_value();
    }
    int sketches = 0;
    int bodies = 0;
    for (const QJsonValue& value : feature.value(QStringLiteral("boundary")).toArray()) {
      const QJsonObject entry = value.toObject();
      if (entry.contains(QStringLiteral("sketch"))) {
        const QString continuity = str(entry, "continuity");
        if (++sketches > 1 || !(continuity.isEmpty() || continuity == kG1)) {
          return false;
        }
      } else if (entry.contains(QStringLiteral("point")) || entry.contains(QStringLiteral("edges")) ||
                 entry.contains(QStringLiteral("face")) || ++bodies > 1 || !sheetEdgesEditable(entry)) {
        return false;
      }
    }
    return true;
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    if (state.choice(QStringLiteral("mode")) == kNetwork) {
      return QStringLiteral("Added boundary surface through %1 by %2 curves")
          .arg(state.items(QStringLiteral("u")).size())
          .arg(state.items(QStringLiteral("v")).size());
    }
    return QStringLiteral("Added boundary surface filling %1 curves and the open edges of %2 faces")
        .arg(state.items(QStringLiteral("curves")).size())
        .arg(state.items(QStringLiteral("open_faces")).size());
  };
  return def;
}

CommandDef matchSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.match", QObject::tr("Match Surface"), "match-surface",
                                  QObject::tr("Rebuilds a surface's face so that a side meets another body's edges"));
  def.featureType = QStringLiteral("match_surface");
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.keywords << QStringLiteral("match") << QStringLiteral("align surface");
  def.inputs = {
      selectionInput(QStringLiteral("face"), QObject::tr("Face"), SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("A face of a surface body bounded by its surface's four sides"))
          .withoutMeshes(),
      selectionInput(QStringLiteral("target_edges"), QObject::tr("Target Edges"), SelectKind::Edge, 0, 0)
          .withTooltip(QObject::tr("Edges of another body that join into a chain"))
          .withoutMeshes(),
      selectionInput(QStringLiteral("target_open_faces"), QObject::tr("Target Open Edges Of"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Faces of the target surface body: their open edges are taken"))
          .withoutMeshes(),
      selectionInput(QStringLiteral("target_face"), QObject::tr("Target Face Beside"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("For edges between two faces: the face the match continues"))
          .withoutMeshes(),
      choiceInput(QStringLiteral("continuity"), QObject::tr("Continuity"), kContinuity, kG2),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& face = state.items(QStringLiteral("face"));
    if (face.isEmpty()) {
      return Built::failure(QObject::tr("Select the face to match."), QStringLiteral("face"));
    }
    QJsonObject target;
    const QString body =
        addEdgesOf(target, state, result, QStringLiteral("target_edges"), QStringLiteral("target_open_faces"));
    if (body.isEmpty()) {
      return result;
    }
    if (body == face.first().owner) {
      return Built::failure(QObject::tr("The target edges must be of another body."), QStringLiteral("target_edges"));
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("match_surface")},
                           {QStringLiteral("body"), face.first().owner},
                           {QStringLiteral("face"), face.first().name},
                           {QStringLiteral("target"), body}};
    for (const char* field : {"edges", "openings"}) {
      if (target.contains(QLatin1String(field))) {
        definition.insert(QLatin1String(field), target.value(QLatin1String(field)));
      }
    }
    if (!addFaceBeside(definition, state, QStringLiteral("target_face"), body, result, "target_face")) {
      return result;
    }
    if (state.choice(QStringLiteral("continuity")) != kG2) {
      definition.insert(QStringLiteral("continuity"), state.choice(QStringLiteral("continuity")));
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("face"),
                   facesOf(str(feature, "body"), QJsonArray{feature.value(QStringLiteral("face"))}, context));
    QJsonObject target{{QStringLiteral("body"), feature.value(QStringLiteral("target"))}};
    for (const char* field : {"edges", "openings"}) {
      if (feature.contains(QLatin1String(field))) {
        target.insert(QLatin1String(field), feature.value(QLatin1String(field)));
      }
    }
    loadEdgesOf(target, state, context, QStringLiteral("target_edges"), QStringLiteral("target_open_faces"));
    loadFaceBeside(feature, state, QStringLiteral("target_face"), str(feature, "target"), context, "target_face");
    const QString continuity = str(feature, "continuity");
    state.setChoice(QStringLiteral("continuity"), continuity.isEmpty() ? kG2 : continuity);
  };
  // A match of the body's only face (none named) has no face to show.
  def.canEdit = [](const QJsonObject& feature) {
    return feature.contains(QStringLiteral("face")) && sheetEdgesEditable(feature);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added match surface (%1)").arg(state.choice(QStringLiteral("continuity")));
  };
  return def;
}

CommandDef fairSurfaceCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.fair", QObject::tr("Fair Surface"), "fair-surface",
                                  QObject::tr("Smooths B-spline faces within a tolerance, their edges kept"));
  def.featureType = QStringLiteral("fair_surface");
  def.surfaceBodiesOnly = true; // the view's context menu (mitcad#494)
  def.keywords << QStringLiteral("fair") << QStringLiteral("smooth");
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces/Body"), SelectKind::Face | SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("Faces of one surface body, or a body for all of its faces"))
          .withoutMeshes(),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Tolerance"), ValueKind::Length, QStringLiteral("0.1 mm")).withStepMinimum(0.0, true)
          .withTooltip(QObject::tr("The farthest a control point moves")),
      valueInput(QStringLiteral("strength"), QObject::tr("Strength"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The share of a full smoothing step each iteration takes, up to 1")),
      valueInput(QStringLiteral("iterations"), QObject::tr("Iterations"), ValueKind::Unitless, QStringLiteral("1")),
      checkInput(QStringLiteral("keep_edges"), QObject::tr("Keep Tangents at Edges"), true)
          .withTooltip(QObject::tr("Also holds the control points next to the edges")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& items = state.items(QStringLiteral("faces"));
    const QString body = oneBody(items, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result.error.isEmpty() ? Built::failure(QObject::tr("Select faces or a body."), QStringLiteral("faces"))
                                    : result;
    }
    const double strength = state.value(QStringLiteral("strength"));
    if (!(strength > 0.0 && strength <= 1.0)) {
      return Built::failure(QObject::tr("The strength must be more than 0 and at most 1."), QStringLiteral("strength"));
    }
    const double iterations = state.value(QStringLiteral("iterations"));
    if (!(iterations >= 1.0 && iterations <= 1000.0) || iterations != std::floor(iterations)) {
      return Built::failure(QObject::tr("The iterations are a whole number from 1 to 1000."),
                            QStringLiteral("iterations"));
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("fair_surface")},
                           {QStringLiteral("body"), body},
                           {QStringLiteral("tolerance"), state.expression(QStringLiteral("tolerance"))},
                           {QStringLiteral("strength"), strength},
                           {QStringLiteral("iterations"), static_cast<int>(iterations)}};
    const QJsonArray faces = names(items, SelectKind::Face);
    if (!faces.isEmpty()) {
      definition.insert(QStringLiteral("faces"), faces);
    }
    if (!state.checked(QStringLiteral("keep_edges"))) {
      definition.insert(QStringLiteral("keep_edges"), false);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QJsonArray faces = feature.value(QStringLiteral("faces")).toArray();
    state.setItems(QStringLiteral("faces"), faces.isEmpty()
                                                ? bodiesOf(QJsonArray{feature.value(QStringLiteral("body"))})
                                                : facesOf(str(feature, "body"), faces, context));
    loadValue(state, QStringLiteral("tolerance"), feature.value(QStringLiteral("tolerance")), feature, context);
    state.setText(QStringLiteral("strength"), number(feature.value(QStringLiteral("strength")).toDouble(1.0)));
    state.setText(QStringLiteral("iterations"), number(feature.value(QStringLiteral("iterations")).toDouble(1.0)));
    state.setChecked(QStringLiteral("keep_edges"), feature.value(QStringLiteral("keep_edges")).toBool(true));
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const qsizetype faces = names(state.items(QStringLiteral("faces")), SelectKind::Face).size();
    return faces > 0 ? QStringLiteral("Added fair surface of %1 faces").arg(faces)
                     : QStringLiteral("Added fair surface of a body");
  };
  return def;
}

} // namespace
} // namespace mitcad::cmd

namespace mitcad {

void registerSurfaceCommands(CommandRegistry& registry, const CommandContext& context) {
  using namespace cmd;
  registry.add(surfaceExtrudeCommand(context));
  registry.add(surfaceRevolveCommand(context));
  registry.add(surfaceSweepCommand(context));
  registry.add(surfaceLoftCommand(context));
  registry.add(patchCommand(context));
  registry.add(stitchCommand(context));
  registry.add(thickenCommand(context));
  registry.add(offsetSurfaceCommand(context));
  registry.add(ruledSurfaceCommand(context));
  registry.add(extendSurfaceCommand(context));
  registry.add(trimSurfaceCommand(context));
  registry.add(untrimSurfaceCommand(context));
  registry.add(unstitchCommand(context));
  registry.add(reverseNormalCommand(context));
  registry.add(blendSurfaceCommand(context));
  registry.add(boundarySurfaceCommand(context));
  registry.add(matchSurfaceCommand(context));
  registry.add(fairSurfaceCommand(context));
}

} // namespace mitcad
