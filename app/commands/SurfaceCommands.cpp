// SPDX-License-Identifier: MIT
// The SURFACE group (mitcad#455; commands.md, "Surface modelling" and
// "Patch, Stitch and Thicken"): surface bodies made by Surface Extrude,
// Surface Revolve, Surface Sweep and Surface Loft (the solid features with
// "surface" set), filled, sewn and made solid by Patch, Stitch and Thicken
// (mitcad#456), and the surface features of the later families.
#include <algorithm>
#include <array>
#include <cmath>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <QJsonArray>

// Surface Loft's chains of edges (mitcad#2395).
#include <BRep_Tool.hxx>
#include <TopExp.hxx>
#include <TopoDS_Vertex.hxx>

#include "../framework/CommandRegistry.hpp"
#include "../framework/OpeningEdges.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "Commands.hpp"

namespace mitcad::cmd {
namespace {

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

// --- Surface Helix (mitcad#1777): Helix's sizes and Growth (mitcad#1797,
// helixSizeInputs) and its flat ends (mitcad#1798, helixEndInputs: the
// definition's `start` and `end`).

CommandDef surfaceHelixCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.helix", QObject::tr("Surface Helix"), "helix",
                                  QObject::tr("Turns sketch curves about an axis as they rise along it, into surfaces"));
  def.featureType = QStringLiteral("helix");
  def.inputs = sheetInputs(context);
  def.inputs.append(
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"),
                     SelectKind::SketchCurve | SelectKind::Axis | SelectKind::Edge, 1, 1)
          .withTooltip(QObject::tr("A sketch line, an origin or construction axis, or a straight edge"))
          .withAccepts([](const SelectionItem& item) { return item.kind == SelectKind::Axis || isLine(item); }));
  def.inputs += helixSizeInputs({QStringLiteral("4"), QStringLiteral("20 mm"), QStringLiteral("5 mm"),
                                 QObject::tr("How far the curves rise along the axis"),
                                 QObject::tr("How far the curves move out from the axis per turn, a conical "
                                             "helicoid; negative narrows it")});
  def.inputs += helixEndInputs();
  def.inputs += {
      choiceInput(QStringLiteral("hand"), QObject::tr("Handedness"),
                  {{QStringLiteral("right"), QStringLiteral("Right Hand")},
                   {QStringLiteral("left"), QStringLiteral("Left Hand")}},
                  QStringLiteral("right"))
          .withTooltip(QObject::tr("The way the curves turn about the way they rise")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withTooltip(QObject::tr("Rise against the axis direction")),
  };
  def.enabled = [&context] { return hasSketches(context, true); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("helix")}};
    if (!addSheetInputs(definition, state, result)) {
      return result;
    }
    const Selection& axis = state.items(QStringLiteral("axis"));
    if (axis.isEmpty()) {
      return Built::failure(QObject::tr("Select the axis."), QStringLiteral("axis"));
    }
    definition.insert(QStringLiteral("axis"), axis.first().reference());
    addHelixSizes(definition, state);
    if (helixHasGrowth(state)) {
      definition.insert(QStringLiteral("growth"), state.expression(QStringLiteral("growth")));
    }
    addHelixEnds(definition, state);
    if (state.choice(QStringLiteral("hand")) == QStringLiteral("left")) {
      definition.insert(QStringLiteral("left_handed"), true);
    }
    if (state.checked(QStringLiteral("flip"))) {
      definition.insert(QStringLiteral("flip"), true);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadSheetInputs(feature, state, context);
    const SelectionItem axis = itemOf(feature.value(QStringLiteral("axis")), context);
    if (axis.isValid()) {
      state.setItems(QStringLiteral("axis"), {axis});
    }
    loadHelixSizes(feature, state, context);
    loadHelixEnds(feature, state, context);
    state.setChoice(QStringLiteral("hand"), feature.value(QStringLiteral("left_handed")).toBool()
                                                ? QStringLiteral("left")
                                                : QStringLiteral("right"));
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
  };
  def.canEdit = [&context](const QJsonObject& feature) {
    return sheetEditable(feature, context) && isShowable(feature.value(QStringLiteral("axis")), context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const auto [turns, pitch] = helixTurnsAndPitch(state);
    QString text =
        QStringLiteral("Added surface helix of %1, %2 turns of %3").arg(sheetCount(state), number(turns), number(pitch));
    if (helixHasGrowth(state)) {
      text += QStringLiteral(", growing %1 per turn").arg(number(state.value(QStringLiteral("growth"))));
    }
    if (helixHasEnds(state)) {
      text += QStringLiteral(", with flat ends");
    }
    return text;
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

// Tangent Edges (mitcad#2162), as the solid Loft's.
const QString kLoftTangentEdges = QStringLiteral("tangent_edges");

// --- Surface Loft's sections of edges (mitcad#2395)

// Whole Opening and the `openings` entries, as the surface features' free
// edges (below, "Free edges").
InputDef wholeOpeningInput();
QJsonArray openingEntries(const Selection& items);
bool pickedOpeningEntry(const QJsonObject& entry);
Selection openingEntryItems(const QString& body, const QJsonObject& entry);
bool wholeOpeningsIn(const QJsonArray& entries);

// The Whole Opening check of the Sections input (wholeOpeningInput's id).
const QString kLoftWholeOpening = QStringLiteral("whole_opening");

// Ends of edges this close meet (mm), as the model's paths join them.
constexpr double kLoftEdgesMeet = 1.0e-4;

// The body's edges (indices) an edge pick stands for: a named edge's, a
// single free edge's, a whole opening's.
std::vector<int> loftPickedEdges(const geometry::Shape& body, const SelectionItem& item) {
  if (const auto at = freeEdgePoint(item)) {
    const std::optional<int> edge = freeEdgeAt(body, openingOf(item), *at);
    return edge ? std::vector<int>{*edge} : std::vector<int>{};
  }
  if (isOpeningEdge(item)) {
    return openingEdges(body, openingOf(item));
  }
  return body.find_edges(item.name.toStdString());
}

// Whether two sets of a body's edges meet at an end.
bool loftEdgesMeet(const geometry::Shape& body, const std::vector<int>& a, const std::vector<int>& b) {
  const auto ends = [&body](int edge) {
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(body.edge(edge), first, last);
    return std::array<TopoDS_Vertex, 2>{first, last};
  };
  for (const int i : a) {
    for (const TopoDS_Vertex& v : ends(i)) {
      for (const int j : b) {
        for (const TopoDS_Vertex& w : ends(j)) {
          if (!v.IsNull() && !w.IsNull() &&
              (v.IsSame(w) || BRep_Tool::Pnt(v).Distance(BRep_Tool::Pnt(w)) <= kLoftEdgesMeet)) {
            return true;
          }
        }
      }
    }
  }
  return false;
}

// The sections of the Sections input's picks: each pick a section of its
// own, but an edge that meets the edges of the section before it, of the
// same body, joins that section, a chain of the body's edges
// (`{"type": "edges"}`), as a path's edges make one path.
QVector<Selection> loftSectionGroups(const Selection& picks, const CommandContext& context) {
  QVector<Selection> groups;
  std::shared_ptr<geometry::Shape> shape; // the last section's body, for edges
  std::vector<int> chain;                 // the last section's edges
  for (const SelectionItem& item : picks) {
    if (item.kind == SelectKind::Edge && shape && !groups.isEmpty()) {
      const SelectionItem& before = groups.last().last();
      if (before.kind == SelectKind::Edge && before.owner == item.owner && before.occurrence == item.occurrence) {
        const std::vector<int> edges = loftPickedEdges(*shape, item);
        if (loftEdgesMeet(*shape, chain, edges)) {
          groups.last().append(item);
          chain.insert(chain.end(), edges.begin(), edges.end());
          continue;
        }
      }
    }
    groups.append(Selection{item});
    chain.clear();
    shape.reset();
    if (item.kind == SelectKind::Edge) {
      shape = context.bodyShape(item.owner);
      if (shape) {
        chain = loftPickedEdges(*shape, item);
      }
    }
  }
  return groups;
}

// The picks of an `edges` section as the Sections input lists them: its
// named edges and free edges in the order of the chain (each meeting one
// before it), so that loftSectionGroups makes them one section again; none
// when the panel cannot show it (free edges other than picked ones).
std::optional<Selection> loftEdgeSectionItems(const QJsonObject& section, const CommandContext& context) {
  const QString body = str(section, "body");
  Selection items;
  if (section.contains(QStringLiteral("edges"))) {
    const auto named = pathItems(
        QJsonObject{{QStringLiteral("body"), body}, {QStringLiteral("edges"), section.value(QStringLiteral("edges"))}},
        context);
    if (!named) {
      return std::nullopt;
    }
    items += *named;
  }
  for (const QJsonValue& value : section.value(QStringLiteral("openings")).toArray()) {
    if (!pickedOpeningEntry(value.toObject())) {
      return std::nullopt;
    }
    items += openingEntryItems(body, value.toObject());
  }
  if (items.isEmpty()) {
    return std::nullopt;
  }
  const std::shared_ptr<geometry::Shape> shape = context.bodyShape(body);
  if (!shape) {
    return items;
  }
  // In chain order: each next pick one that meets those before it.
  Selection ordered{items.takeFirst()};
  std::vector<int> chain = loftPickedEdges(*shape, ordered.first());
  while (!items.isEmpty()) {
    int next = 0;
    for (int i = 0; i < items.size(); ++i) {
      if (loftEdgesMeet(*shape, chain, loftPickedEdges(*shape, items[i]))) {
        next = i;
        break;
      }
    }
    const std::vector<int> edges = loftPickedEdges(*shape, items[next]);
    chain.insert(chain.end(), edges.begin(), edges.end());
    ordered.append(items.takeAt(next));
  }
  return ordered;
}

// The picks of the definition's sections, a section of curves that cannot
// be shown left out; none when a chain of edges cannot be shown.
std::optional<QVector<Selection>> loftSectionItems(const QJsonObject& feature, const CommandContext& context) {
  QVector<Selection> sections;
  for (const QJsonValue& value : feature.value(QStringLiteral("sections")).toArray()) {
    const QJsonObject section = value.toObject();
    const QString type = str(section, "type");
    if (type == QStringLiteral("profile")) {
      sections.append(Selection{
          {SelectKind::Profile, str(section, "sketch"), str(section, "region"), QStringLiteral("plane")}});
    } else if (type == QStringLiteral("face")) {
      // Faces and points (mitcad#2345).
      sections.append(Selection{itemOf(QJsonObject{{QStringLiteral("body"), section.value(QStringLiteral("body"))},
                                                   {QStringLiteral("face"), section.value(QStringLiteral("face"))}},
                                       context)});
    } else if (type == QStringLiteral("point")) {
      sections.append(Selection{itemOf(section.value(QStringLiteral("point")), context)});
    } else if (type == QStringLiteral("edges")) {
      // A chain of a body's edges (mitcad#2395).
      const auto items = loftEdgeSectionItems(section, context);
      if (!items) {
        return std::nullopt;
      }
      sections.append(*items);
    } else if (const auto items = pathItems(section, context)) {
      sections.append(*items);
    }
  }
  return sections;
}

// Whether the Sections input holds an open edge (Whole Opening shown).
bool loftPicksOpenEdges(const CommandState& state) {
  const Selection& items = state.items(QStringLiteral("sections"));
  return std::any_of(items.begin(), items.end(), [](const SelectionItem& item) { return isOpeningEdge(item); });
}

CommandDef surfaceLoftCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.loft", QObject::tr("Surface Loft"), "surface-loft",
                                  QObject::tr("A surface through sections in order: curves, profiles, faces or "
                                              "points"));
  def.featureType = QStringLiteral("loft");
  def.inputs = {
      // A 3D sketch's curves anywhere in space and its profiles (closed
      // loops in its sketch plane) too (mitcad#1550); faces of bodies and
      // surface bodies (their edges) and points at the ends as the solid
      // Loft's (mitcad#2345). Edges of bodies and free edges of surface
      // bodies too, those that meet one section's chain (mitcad#2395,
      // loftSectionGroups).
      selectionInput(QStringLiteral("sections"), QObject::tr("Sections"),
                     SelectKind::Profile | SelectKind::SketchCurve | SelectKind::Edge | SelectKind::Face | kPointKinds,
                     2, 0)
          .withTooltip(QObject::tr("Sections in order: sketch curves or chains of edges, or profiles and faces; "
                                   "points first or last"))
          .withAccepts([&context](const SelectionItem& item) { return spatialPathTakes(item, context); })
          .withRefusedHint([&context](const SelectionItem& item, const CommandState&) {
            return spatialPathRefusal(item, context);
          })
          .withOpenings(kLoftWholeOpening)
          .withOrdered(),
      wholeOpeningInput().withVisible(loftPicksOpenEdges),
  };
  // Start and End Condition as the solid Loft's (mitcad#2345).
  def.inputs += loftEndInputs(false);
  def.inputs += loftEndInputs(true);
  def.inputs += {
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
      choiceInput(kLoftTangentEdges, QObject::tr("Tangent Edges"),
                  {{QStringLiteral("merge"), QStringLiteral("Merge")}, {QStringLiteral("keep"), QStringLiteral("Keep")}},
                  QStringLiteral("merge"))
          .withTooltip(QObject::tr("Merge: a section's smoothly joined curves make one face; Keep: a face per curve")),
  };
  // A 3D sketch alone enables it (mitcad#1550), and bodies, whose faces it
  // takes (mitcad#2345).
  def.enabled = [&context] { return hasSketches(context, true) || hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext& model) {
    Built result;
    QJsonArray sections;
    bool curves = false;
    bool profiles = false;
    for (const Selection& group : loftSectionGroups(state.items(QStringLiteral("sections")), model)) {
      const SelectionItem& item = group.first();
      if (item.kind == SelectKind::Edge) {
        // A chain of a body's edges, named and free (mitcad#2395).
        curves = true;
        Selection named;
        Selection open;
        for (const SelectionItem& edge : group) {
          (isOpeningEdge(edge) ? open : named).append(edge);
        }
        QJsonObject section{{QStringLiteral("type"), QStringLiteral("edges")}, {QStringLiteral("body"), item.owner}};
        if (!named.isEmpty()) {
          section.insert(QStringLiteral("edges"), names(named, SelectKind::Edge));
        }
        if (!open.isEmpty()) {
          section.insert(QStringLiteral("openings"), openingEntries(open));
        }
        sections.append(section);
      } else if (item.kind == SelectKind::Profile) {
        profiles = true;
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("profile")},
                                    {QStringLiteral("sketch"), item.owner},
                                    {QStringLiteral("region"), item.name}});
      } else if (item.kind == SelectKind::Face) {
        // A face of a body or surface body: its outer edges (mitcad#2345).
        profiles = true;
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("face")},
                                    {QStringLiteral("body"), item.owner},
                                    {QStringLiteral("face"), item.name}});
      } else if (item.kind == SelectKind::SketchCurve) {
        // A planar or a 3D sketch's curve, its chain the section.
        curves = true;
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("curves")},
                                    {QStringLiteral("sketch"), item.owner},
                                    {QStringLiteral("curves"), QJsonArray{item.name}},
                                    {QStringLiteral("chain"), true}});
      } else {
        // A point at the start or the end (mitcad#2345).
        profiles = true;
        sections.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("point")},
                                    {QStringLiteral("point"), item.reference()}});
      }
    }
    if (curves && profiles) {
      return Built::failure(QObject::tr("Select sketch curves and edges as sections, or profiles, faces and "
                                        "points; not both kinds."),
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
      return Built::failure(QObject::tr("Rails guide a loft of profiles or faces: select those as sections, or "
                                        "remove the rails."),
                            kLoftRails);
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("loft")},
                           {QStringLiteral("sections"), sections},
                           {QStringLiteral("surface"), true},
                           {QStringLiteral("operation"), QStringLiteral("new_body")}};
    // Start and End Condition (mitcad#2345).
    addLoftEnds(definition, state);
    if (!rails.isEmpty()) {
      definition.insert(kLoftRails, rails);
    }
    if (state.checked(QStringLiteral("closed"))) {
      definition.insert(QStringLiteral("closed"), true);
    }
    if (state.checked(QStringLiteral("ruled"))) {
      definition.insert(QStringLiteral("ruled"), true);
    }
    // Keep is the field left out (mitcad#2162).
    if (state.choice(kLoftTangentEdges) == QStringLiteral("merge")) {
      definition.insert(kLoftTangentEdges, QStringLiteral("merge"));
    }
    result.def = definition;
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    // Chains of edges as their picks in chain order (mitcad#2395).
    Selection sections;
    if (const auto groups = loftSectionItems(feature, context)) {
      for (const Selection& group : *groups) {
        sections += group;
      }
    }
    state.setItems(QStringLiteral("sections"), sections);
    QJsonArray entries;
    for (const QJsonValue& value : feature.value(QStringLiteral("sections")).toArray()) {
      for (const QJsonValue& entry : value.toObject().value(QStringLiteral("openings")).toArray()) {
        entries.append(entry);
      }
    }
    state.setChecked(kLoftWholeOpening, wholeOpeningsIn(entries));
    // Start and End Condition (mitcad#2345).
    loadLoftEnds(feature, state, context);
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
    // A surface loft without the field keeps its tangent edges (mitcad#2162).
    state.setChoice(kLoftTangentEdges, feature.value(kLoftTangentEdges).toString() == QStringLiteral("merge")
                                           ? QStringLiteral("merge")
                                           : QStringLiteral("keep"));
  };
  // Sections of one curve each (chained), or profiles, faces and points
  // that can be shown, and rails of curves or edges chained alike (one
  // Chain Selection holds them), with end conditions (mitcad#2345) and
  // without a centre line.
  def.canEdit = [&context](const QJsonObject& feature) {
    if (!feature.value(QStringLiteral("surface")).toBool() || feature.contains(QStringLiteral("centerline"))) {
      return false;
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
    const QJsonArray all = feature.value(QStringLiteral("sections")).toArray();
    bool edges = false;
    for (const QJsonValue& value : all) {
      const QJsonObject section = value.toObject();
      const QString type = str(section, "type");
      if (type == QStringLiteral("curves")) {
        if (section.value(QStringLiteral("curves")).toArray().size() != 1 ||
            !section.value(QStringLiteral("chain")).toBool()) {
          return false;
        }
      } else if (type == QStringLiteral("point")) {
        if (!isShowable(section.value(QStringLiteral("point")), context)) {
          return false;
        }
      } else if (type == QStringLiteral("edges")) {
        edges = true; // a chain of a body's edges (mitcad#2395)
      } else if (type != QStringLiteral("profile") && type != QStringLiteral("face")) {
        return false;
      }
    }
    if (!edges) {
      return true;
    }
    // Chains of edges the panel shows as picks that make the same sections
    // again: no two neighbouring chains of one body that meet.
    const auto groups = loftSectionItems(feature, context);
    if (!groups) {
      return false;
    }
    Selection picks;
    for (const Selection& group : *groups) {
      picks += group;
    }
    return loftSectionGroups(picks, context).size() == all.size();
  };
  // The sections, a chain of edges one (mitcad#2395).
  def.describe = [](const CommandState& state, const CommandContext& context) {
    return QStringLiteral("Added surface loft through %1 sections")
        .arg(loftSectionGroups(state.items(QStringLiteral("sections")), context).size());
  };
  return def;
}

// --- Free edges (mitcad#1927, #1955)

const QString kWholeOpening = QStringLiteral("whole_opening");

// Whole Opening of the inputs that take openings (InputDef::withOpenings
// with this check): on, a click on a free edge takes its whole opening; off,
// the single edge.
InputDef wholeOpeningInput() {
  return checkInput(kWholeOpening, QObject::tr("Whole Opening"), true)
      .withTooltip(QObject::tr("On: a click on an open edge takes its whole opening. Off: only that edge"));
}

// The `openings` entries of the picked openings and free edges: a whole
// opening `{"opening": ...}`, the single free edges of one opening
// `{"opening": ..., "at": [[x, y, z], ...]}` (a point on each edge), in
// the order first picked.
QJsonArray openingEntries(const Selection& items) {
  QJsonArray entries;
  QVector<QJsonObject> singles; // the opening of each entry of single edges
  QVector<int> where;           // its place in `entries`
  for (const SelectionItem& item : items) {
    if (!isOpeningEdge(item)) {
      continue;
    }
    const QJsonObject opening = openingOf(item);
    const auto at = freeEdgePoint(item);
    if (!at) {
      entries.append(QJsonObject{{QStringLiteral("opening"), opening}});
      continue;
    }
    QJsonArray point;
    for (const double v : *at) {
      point.append(v);
    }
    const int known = static_cast<int>(singles.indexOf(opening));
    if (known < 0) {
      QJsonArray points;
      points.append(point);
      singles.append(opening);
      where.append(static_cast<int>(entries.size()));
      entries.append(QJsonObject{{QStringLiteral("opening"), opening}, {QStringLiteral("at"), points}});
    } else {
      QJsonObject entry = entries[where[known]].toObject();
      QJsonArray points = entry.value(QStringLiteral("at")).toArray();
      points.append(point);
      entry.insert(QStringLiteral("at"), points);
      entries[where[known]] = entry;
    }
  }
  return entries;
}

// An `openings` entry the panel shows as picks: a whole opening, or single
// free edges of one opening by their points.
bool pickedOpeningEntry(const QJsonObject& entry) {
  if (!entry.value(QStringLiteral("opening")).isObject() || entry.contains(QStringLiteral("faces"))) {
    return false;
  }
  return entry.size() == 1 || (entry.size() == 2 && !entry.value(QStringLiteral("at")).toArray().isEmpty());
}

// The items of such an entry of `body`: the whole opening, or a single
// free edge for each point.
Selection openingEntryItems(const QString& body, const QJsonObject& entry) {
  const QJsonObject opening = entry.value(QStringLiteral("opening")).toObject();
  const QJsonArray points = entry.value(QStringLiteral("at")).toArray();
  if (points.isEmpty()) {
    return {openingEdgeItem(body, opening)};
  }
  Selection items;
  for (const QJsonValue& value : points) {
    const QJsonArray p = value.toArray();
    if (p.size() == 3) {
      items.append(freeEdgeItem(body, opening, {p[0].toDouble(), p[1].toDouble(), p[2].toDouble()}));
    }
  }
  return items;
}

// Whole Opening as a definition's entries show it: off when an entry
// picks single free edges.
bool wholeOpeningsIn(const QJsonArray& entries) {
  return std::none_of(entries.begin(), entries.end(), [](const QJsonValue& value) {
    return !value.toObject().value(QStringLiteral("at")).toArray().isEmpty();
  });
}

// --- Patch, Stitch and Thicken (mitcad#456)

// Whether an opening of a surface body is picked (by one of its free edges,
// mitcad#1927).
bool picksOpening(const CommandState& state) {
  const Selection& items = state.items(QStringLiteral("boundary"));
  return std::any_of(items.begin(), items.end(), [](const SelectionItem& item) { return isOpeningEdge(item); });
}

// The `sketch_curves` entries of a patch's loop (mitcad#1972): the picked
// sketch curves, one entry per sketch in the order first picked.
QJsonArray sketchCurveEntries(const Selection& curves) {
  QStringList sketches;
  QVector<QJsonArray> picked; // the curves of each sketch
  for (const SelectionItem& item : curves) {
    int at = static_cast<int>(sketches.indexOf(item.owner));
    if (at < 0) {
      at = static_cast<int>(sketches.size());
      sketches.append(item.owner);
      picked.append(QJsonArray());
    }
    picked[at].append(item.name);
  }
  QJsonArray entries;
  for (int i = 0; i < static_cast<int>(sketches.size()); ++i) {
    entries.append(QJsonObject{{QStringLiteral("sketch"), sketches[i]}, {QStringLiteral("curves"), picked[i]}});
  }
  return entries;
}

// Whether the panel shows a `sketch_curves` entry: curves of a sketch,
// without chain or direction.
bool shownSketchCurves(const QJsonValue& value) {
  const QJsonObject entry = value.toObject();
  return entry.size() == 2 && entry.value(QStringLiteral("sketch")).isString() &&
         !entry.value(QStringLiteral("curves")).toArray().isEmpty();
}

CommandDef patchCommand(const CommandContext& context) {
  CommandDef def = surfaceCommand("surface.patch", QObject::tr("Patch"), "patch",
                                  QObject::tr("Fills sketch profiles, or a closed loop of a body's edges, with "
                                              "surfaces"));
  def.keywords << QStringLiteral("fill") << QStringLiteral("cap");
  def.featureType = QStringLiteral("patch");
  def.inputs = {
      selectionInput(QStringLiteral("boundary"), QObject::tr("Profiles or Edges"),
                     SelectKind::Profile | SelectKind::Edge | SelectKind::SketchCurve, 1, 0)
          .withTooltip(QObject::tr("Sketch profiles, or edges of one body closing a loop: open edges and "
                                   "sketch curves too"))
          .withoutMeshes()
          .withOpenings(kWholeOpening),
      wholeOpeningInput(),
      choiceInput(QStringLiteral("continuity"), QObject::tr("Continuity"), kContinuity, QStringLiteral("g0"))
          .withTooltip(QObject::tr("How a patch of edges meets the faces beside them")),
      checkInput(QStringLiteral("stitch"), QObject::tr("Stitch"), true)
          .withTooltip(QObject::tr("Sews the patch onto the body of the opening; a solid where it closes it"))
          .withVisible(picksOpening),
  };
  def.enabled = [&context] { return hasProfiles(context) || hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    Selection profiles;
    Selection edges;
    Selection openings;
    Selection curves; // sketch curves in the loop (mitcad#1972)
    for (const SelectionItem& item : state.items(QStringLiteral("boundary"))) {
      (item.kind == SelectKind::Profile       ? profiles
       : item.kind == SelectKind::SketchCurve ? curves
       : isOpeningEdge(item)                  ? openings
                                              : edges)
          .append(item);
    }
    if (!curves.isEmpty() && edges.isEmpty() && openings.isEmpty()) {
      return Built::failure(QObject::tr("Select the edges the sketch curves close a loop with: sketch curves "
                                        "only join a patch's loop of edges."),
                            QStringLiteral("boundary"));
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("patch")}};
    if (!profiles.isEmpty()) {
      if (oneSketch(profiles, result, QStringLiteral("boundary")).isEmpty()) {
        return result;
      }
      definition.insert(QStringLiteral("profiles"), refsOf(profiles));
    }
    if (!edges.isEmpty() || !openings.isEmpty()) {
      // Whole openings (picked by a free edge with Whole Opening on) and
      // single free edges (with it off, mitcad#1955).
      const auto whole = std::count_if(openings.begin(), openings.end(),
                                       [](const SelectionItem& item) { return !isFreeEdge(item); });
      if (whole > 0 && (whole > 1 || openings.size() > 1 || !edges.isEmpty() || !curves.isEmpty())) {
        return Built::failure(QObject::tr("Select one whole opening, or edges, single open edges and sketch "
                                          "curves that close up into a loop: a patch fills one loop."),
                              QStringLiteral("boundary"));
      }
      const QString body = oneBody(edges + openings, result, QStringLiteral("boundary"));
      if (body.isEmpty()) {
        return result;
      }
      QJsonObject loop{{QStringLiteral("body"), body}};
      if (!edges.isEmpty()) {
        loop.insert(QStringLiteral("edges"), names(edges, SelectKind::Edge));
      }
      if (whole > 0) {
        loop.insert(QStringLiteral("opening"), openingOf(openings.first()));
      } else if (!openings.isEmpty()) {
        loop.insert(QStringLiteral("openings"), openingEntries(openings));
      }
      if (!curves.isEmpty()) {
        loop.insert(QStringLiteral("sketch_curves"), sketchCurveEntries(curves));
      }
      if (!openings.isEmpty() && state.checked(QStringLiteral("stitch"))) {
        loop.insert(QStringLiteral("stitch"), true);
      }
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
      const QString body = str(loop, "body");
      if (loop.contains(QStringLiteral("opening"))) {
        items.append(openingEdgeItem(body, loop.value(QStringLiteral("opening")).toObject()));
      } else {
        if (loop.contains(QStringLiteral("edges"))) {
          if (const auto edges = pathItems(QJsonObject{{QStringLiteral("body"), body},
                                                       {QStringLiteral("edges"), loop.value(QStringLiteral("edges"))}},
                                           context)) {
            items += *edges;
          }
        }
        // Single free edges beside them (mitcad#1955).
        for (const QJsonValue& entry : loop.value(QStringLiteral("openings")).toArray()) {
          items += openingEntryItems(body, entry.toObject());
        }
        // Sketch curves in the loop (mitcad#1972).
        for (const QJsonValue& entry : loop.value(QStringLiteral("sketch_curves")).toArray()) {
          if (const auto curves = pathItems(entry, context)) {
            items += *curves;
          }
        }
      }
      state.setChecked(kWholeOpening, wholeOpeningsIn(loop.value(QStringLiteral("openings")).toArray()));
      state.setChoice(QStringLiteral("continuity"),
                      loop.value(QStringLiteral("continuity")).toString(QStringLiteral("g0")));
      state.setChecked(QStringLiteral("stitch"), loop.value(QStringLiteral("stitch")).toBool());
    }
    state.setItems(QStringLiteral("boundary"), items);
  };
  // Named edges, single free edges and sketch curves, or an opening, with
  // one continuity, as the panel shows them; only a loop with free edges is
  // stitched there.
  def.canEdit = [](const QJsonObject& feature) {
    const QJsonObject loop = feature.value(QStringLiteral("boundary")).toObject();
    const QJsonArray entries = loop.value(QStringLiteral("openings")).toArray();
    const QJsonArray curves = loop.value(QStringLiteral("sketch_curves")).toArray();
    const bool openEdges = loop.contains(QStringLiteral("opening")) || !entries.isEmpty();
    const bool shown = std::all_of(entries.begin(), entries.end(),
                                   [](const QJsonValue& value) {
                                     const QJsonObject entry = value.toObject();
                                     return pickedOpeningEntry(entry) &&
                                            !entry.value(QStringLiteral("at")).toArray().isEmpty();
                                   }) &&
                       std::all_of(curves.begin(), curves.end(), shownSketchCurves);
    return loop.isEmpty() ||
           ((loop.contains(QStringLiteral("edges")) || openEdges) && shown &&
            !loop.contains(QStringLiteral("edge_continuity")) && !loop.contains(QStringLiteral("points")) &&
            (openEdges || !loop.contains(QStringLiteral("stitch"))));
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString described = QStringLiteral("Added patch of %1 items").arg(state.items(QStringLiteral("boundary")).size());
    return picksOpening(state) && state.checked(QStringLiteral("stitch")) ? described + QStringLiteral(", stitched")
                                                                           : described;
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

// The edges picked (named, or openings picked by a free edge, mitcad#1927)
// and the faces whose open edges are meant, into `edges` and `openings`
// (the faces' entry first, then one entry of each opening); the body they
// are all of, empty (with the error) when they are not of one.
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
  Selection named;
  QJsonArray openings;
  const QJsonArray faces = names(state.items(facesKey), SelectKind::Face);
  if (!faces.isEmpty()) {
    openings.append(QJsonObject{{QStringLiteral("faces"), faces}});
  }
  for (const SelectionItem& item : state.items(edgesKey)) {
    if (!isOpeningEdge(item)) {
      named.append(item);
    }
  }
  // Whole openings, and single free edges by their points (mitcad#1955).
  for (const QJsonValue& entry : openingEntries(state.items(edgesKey))) {
    openings.append(entry);
  }
  const QJsonArray edges = names(named, SelectKind::Edge);
  if (!edges.isEmpty()) {
    def.insert(QStringLiteral("edges"), edges);
  }
  if (!openings.isEmpty()) {
    def.insert(QStringLiteral("openings"), openings);
  }
  return body;
}

QString addSheetEdges(QJsonObject& def, const CommandState& state, Built& result) {
  return addEdgesOf(def, state, result, QStringLiteral("edges"), QStringLiteral("open_faces"));
}

void loadEdgesOf(const QJsonObject& feature, CommandState& state, const CommandContext& context,
                 const QString& edgesKey, const QString& facesKey) {
  const QString body = str(feature, "body");
  Selection edges;
  if (const auto named = pathItems(QJsonObject{{QStringLiteral("body"), body},
                                               {QStringLiteral("edges"), feature.value(QStringLiteral("edges"))}},
                                   context)) {
    edges = *named;
  }
  const QJsonArray openings = feature.value(QStringLiteral("openings")).toArray();
  for (const QJsonValue& value : openings) {
    const QJsonObject entry = value.toObject();
    if (pickedOpeningEntry(entry)) {
      // A whole opening, or single free edges (mitcad#1955).
      edges += openingEntryItems(body, entry);
    } else if (state.items(facesKey).isEmpty()) {
      state.setItems(facesKey, facesOf(body, entry.value(QStringLiteral("faces")).toArray(), context));
    }
  }
  state.setItems(edgesKey, edges);
  // Whole Opening (on at first) off for single free edges; Blend
  // Surface's ends share it.
  if (!wholeOpeningsIn(openings)) {
    state.setChecked(kWholeOpening, false);
  }
}

void loadSheetEdges(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  loadEdgesOf(feature, state, context, QStringLiteral("edges"), QStringLiteral("open_faces"));
}

// Openings as the panel shows them: whole openings and single free edges
// picked by a free edge, and one entry of every opening of the faces.
bool sheetEdgesEditable(const QJsonObject& feature) {
  int faces = 0;
  for (const QJsonValue& value : feature.value(QStringLiteral("openings")).toArray()) {
    const QJsonObject opening = value.toObject();
    if (pickedOpeningEntry(opening)) {
      continue;
    }
    if (++faces > 1 || opening.contains(QStringLiteral("opening")) || opening.contains(QStringLiteral("at")) ||
        opening.value(QStringLiteral("faces")).toArray().isEmpty()) {
      return false;
    }
  }
  return true;
}

QVector<InputDef> sheetEdgeInputs() {
  return {
      selectionInput(QStringLiteral("edges"), QObject::tr("Edges"), SelectKind::Edge, 0, 0)
          .withTooltip(QObject::tr("Edges of one body, open edges of a surface body too"))
          .withoutMeshes()
          .withOpenings(kWholeOpening),
      wholeOpeningInput(),
      selectionInput(QStringLiteral("open_faces"), QObject::tr("Open Edges Of"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Faces of a surface body: their open edges are taken"))
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
          .withoutMeshes()
          .withOpenings(kWholeOpening),
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
      wholeOpeningInput(), // both ends' (mitcad#1955)
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

// The sketches of the sketch curves, in the order their first curves were
// picked (mitcad#2198: a loop may run through several sketches).
QStringList curveSketches(const Selection& items) {
  QStringList sketches;
  for (const SelectionItem& item : items) {
    if (item.kind == SelectKind::SketchCurve && !sketches.contains(item.owner)) {
      sketches << item.owner;
    }
  }
  return sketches;
}

// The sketch curves of one sketch among the items.
Selection curvesOf(const Selection& items, const QString& sketch) {
  Selection own;
  for (const SelectionItem& item : items) {
    if (item.kind == SelectKind::SketchCurve && item.owner == sketch) {
      own.append(item);
    }
  }
  return own;
}

// Whether a planar sketch's curves are among the items: Tangent to Sketch
// Plane is for those (a 3D sketch has no one plane).
bool hasPlanarCurves(const Selection& items, const CommandContext& context) {
  const QStringList sketches = curveSketches(items);
  return std::any_of(sketches.begin(), sketches.end(),
                     [&context](const QString& sketch) { return !isSpatialSketch(sketch, context); });
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
          .withTooltip(QObject::tr("Sketch curves round the loop, of one sketch or several, 3D sketches too"))
          .withVisible(loop),
      checkInput(QStringLiteral("chain"), QObject::tr("Chain Selection"), true)
          .withTooltip(QObject::tr("Also take the curves joined to the selected ones"))
          .withVisible(loop),
      // Only while a planar sketch's curves are picked (mitcad#2198).
      checkInput(QStringLiteral("tangent"), QObject::tr("Tangent to Sketch Plane"), false)
          .withTooltip(QObject::tr("The surface leaves the curves of planar sketches along their sketch's plane"))
          .withVisible([loop, &context](const CommandState& state) {
            return loop(state) && hasPlanarCurves(state.items(QStringLiteral("curves")), context);
          }),
      selectionInput(QStringLiteral("edges"), QObject::tr("Boundary Edges"), SelectKind::Edge, 0, 0)
          .withTooltip(QObject::tr("Edges of one body round the loop, open edges of a surface body too"))
          .withoutMeshes()
          .withOpenings(kWholeOpening)
          .withVisible(loop),
      wholeOpeningInput().withVisible(loop),
      selectionInput(QStringLiteral("open_faces"), QObject::tr("Open Edges Of"), SelectKind::Face, 0, 0)
          .withTooltip(QObject::tr("Faces of a surface body: their open edges close the loop"))
          .withoutMeshes()
          .withVisible(loop),
      // How the surface meets the faces beside the edges: only while edges
      // are picked (mitcad#2198); sketch curves have Tangent to Sketch Plane.
      choiceInput(QStringLiteral("continuity"), QObject::tr("Edge Continuity"), kContinuity,
                  QStringLiteral("g0"))
          .withVisible([loop](const CommandState& state) {
            return loop(state) && (!state.items(QStringLiteral("edges")).isEmpty() ||
                                   !state.items(QStringLiteral("open_faces")).isEmpty());
          }),
      selectionInput(QStringLiteral("u"), QObject::tr("Curves One Way"), SelectKind::SketchCurve, 0, 0)
          .withTooltip(QObject::tr("Two or more, the surface lofted through them in order"))
          .withVisible(network),
      selectionInput(QStringLiteral("v"), QObject::tr("Crossing Curves"), SelectKind::SketchCurve, 0, 0)
          .withTooltip(QObject::tr("None, or two or more crossing every curve one way"))
          .withVisible(network),
  };
  // Any sketch, 3D sketches too (mitcad#2198), or a body.
  def.enabled = [&context] { return hasSketches(context, true) || hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext& context) {
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
    // One entry for each sketch's curves (mitcad#2198), planar or 3D; the
    // model joins them end to end in the loop's order and refuses a loop
    // that does not close. Tangent to Sketch Plane for the planar ones.
    QJsonArray boundary;
    const Selection& curves = state.items(QStringLiteral("curves"));
    for (const QString& sketch : curveSketches(curves)) {
      QJsonObject entry{{QStringLiteral("sketch"), sketch},
                        {QStringLiteral("curves"), names(curvesOf(curves, sketch), SelectKind::SketchCurve)}};
      if (state.checked(QStringLiteral("chain"))) {
        entry.insert(QStringLiteral("chain"), true);
      }
      if (state.checked(QStringLiteral("tangent")) && !isSpatialSketch(sketch, context)) {
        entry.insert(QStringLiteral("continuity"), kG1);
      }
      boundary.append(entry);
    }
    if (!state.items(QStringLiteral("edges")).isEmpty() || !state.items(QStringLiteral("open_faces")).isEmpty()) {
      // The body's edges: named, whole openings picked by a free edge
      // (mitcad#1927) and the open edges of faces, one chain.
      QJsonObject entry;
      if (addEdgesOf(entry, state, result, QStringLiteral("edges"), QStringLiteral("open_faces")).isEmpty()) {
        return result;
      }
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
    // Every sketch's curves into Boundary Curves (mitcad#2198); canEdit
    // holds their entries to one Chain Selection and one Tangent to Sketch
    // Plane (a 3D sketch's are never tangent).
    Selection curves;
    bool tangent = false;
    for (const QJsonValue& value : feature.value(QStringLiteral("boundary")).toArray()) {
      const QJsonObject entry = value.toObject();
      if (entry.contains(QStringLiteral("sketch"))) {
        if (const auto items = pathItems(entry, context)) {
          curves += *items;
        }
        state.setChecked(QStringLiteral("chain"), entry.value(QStringLiteral("chain")).toBool());
        tangent = tangent || str(entry, "continuity") == kG1;
      } else {
        loadEdgesOf(entry, state, context, QStringLiteral("edges"), QStringLiteral("open_faces"));
        const QString continuity = str(entry, "continuity");
        state.setChoice(QStringLiteral("continuity"), continuity.isEmpty() ? QStringLiteral("g0") : continuity);
      }
    }
    if (!curves.isEmpty()) {
      state.setItems(QStringLiteral("curves"), curves);
    }
    state.setChecked(QStringLiteral("tangent"), tangent);
  };
  // The panel shows the curves of one sketch or several (each sketch's in
  // one entry, all chained or none, the planar ones all tangent to their
  // planes or none, a 3D sketch's never) and one chain of a body's edges,
  // open edges of its faces and its openings (loop), or single curves
  // (network).
  def.canEdit = [&context](const QJsonObject& feature) {
    if (feature.contains(QStringLiteral("u"))) {
      return singleCurveItems(feature.value(QStringLiteral("u")).toArray(), context).has_value() &&
             singleCurveItems(feature.value(QStringLiteral("v")).toArray(), context).has_value();
    }
    QStringList sketches;
    std::optional<bool> chain;
    std::optional<bool> tangent;
    int bodies = 0;
    for (const QJsonValue& value : feature.value(QStringLiteral("boundary")).toArray()) {
      const QJsonObject entry = value.toObject();
      if (entry.contains(QStringLiteral("sketch"))) {
        const QString sketch = str(entry, "sketch");
        const QString continuity = str(entry, "continuity");
        const bool chained = entry.value(QStringLiteral("chain")).toBool();
        const bool tangential = continuity == kG1;
        if (sketches.contains(sketch) || !(continuity.isEmpty() || tangential) || (chain && *chain != chained)) {
          return false;
        }
        sketches << sketch;
        chain = chained;
        if (isSpatialSketch(sketch, context)) {
          if (tangential) {
            return false;
          }
        } else if (tangent && *tangent != tangential) {
          return false;
        } else {
          tangent = tangential;
        }
      } else if (entry.contains(QStringLiteral("point")) || entry.contains(QStringLiteral("face")) ||
                 ++bodies > 1 || !sheetEdgesEditable(entry)) {
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
          .withoutMeshes()
          .withOpenings(kWholeOpening),
      wholeOpeningInput(),
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
  // Their places on the toolbar are the layout's (mitcad#1951).
  // Surfaces from sketch profiles and curves.
  registry.add(surfaceExtrudeCommand(context));
  registry.add(surfaceRevolveCommand(context));
  registry.add(surfaceSweepCommand(context));
  registry.add(surfaceLoftCommand(context));
  // A surface helix of profiles and sketch curves (mitcad#1777).
  registry.add(surfaceHelixCommand(context));
  // Surfaces from edges.
  registry.add(patchCommand(context));
  registry.add(ruledSurfaceCommand(context));
  registry.add(boundarySurfaceCommand(context));
  registry.add(blendSurfaceCommand(context));
  // Changing a surface.
  registry.add(extendSurfaceCommand(context));
  registry.add(trimSurfaceCommand(context));
  registry.add(untrimSurfaceCommand(context));
  registry.add(offsetSurfaceCommand(context));
  // Its quality and orientation.
  registry.add(matchSurfaceCommand(context));
  registry.add(fairSurfaceCommand(context));
  registry.add(reverseNormalCommand(context));
  // Surface bodies and solids.
  registry.add(stitchCommand(context));
  registry.add(unstitchCommand(context));
  registry.add(thickenCommand(context));
}

} // namespace mitcad
