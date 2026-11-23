// SPDX-License-Identifier: MIT
// Cell Grid (mitcad#2238; commands.md, "Cell Grid"): the closed profiles of
// a sketch filled with cells of one shape, kept out of the loops inside
// them, and cut from bodies in one operation (vents, grilles). The preview
// shows the cells' tool; its remark (the panel's message and the log) gives
// the cell count and the open area.
#include <QJsonArray>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "ProfilePicks.hpp"

namespace mitcad::cmd {
namespace {

const QString kPolygon = QStringLiteral("polygon");
const QString kCircle = QStringLiteral("circle");
const QString kSymmetric = QStringLiteral("symmetric");
const QString kFree = QStringLiteral("free");

const Choices kShapes = {{QStringLiteral("hexagon"), QStringLiteral("Hexagon")},
                         {QStringLiteral("square"), QStringLiteral("Square")},
                         {QStringLiteral("triangle"), QStringLiteral("Triangle")},
                         {kCircle, QStringLiteral("Circle")},
                         {kPolygon, QStringLiteral("Polygon")}};

// How circles and polygons are laid out.
const Choices kRows = {{QStringLiteral("staggered"), QStringLiteral("Staggered")},
                       {QStringLiteral("square"), QStringLiteral("Square")}};

// One side (into the face a sketch lies on, or with Flip along the sketch
// normal) or both.
const Choices kDirections = {{QStringLiteral("one_side"), QStringLiteral("One Side")},
                             {kSymmetric, QStringLiteral("Symmetric")}};

const Choices kExtents = {{QStringLiteral("distance"), QStringLiteral("Distance")},
                          {QStringLiteral("through_all"), QStringLiteral("Through All")}};

// The cells an edge crosses: left out, snapped along their own lines, or
// trimmed to the edge.
const Choices kEdgeCells = {{QStringLiteral("whole"), QStringLiteral("Whole")},
                            {QStringLiteral("snap"), QStringLiteral("Snapped")},
                            {kFree, QStringLiteral("Trimmed")}};

bool throughAll(const CommandState& state) {
  return state.choice(QStringLiteral("extent")) == QStringLiteral("through_all");
}

bool symmetric(const CommandState& state) {
  return state.choice(QStringLiteral("direction")) == kSymmetric;
}

// The definition's direction: positive (along the sketch normal),
// negative (the model's default) or symmetric.
QString directionOf(const CommandState& state) {
  if (symmetric(state)) {
    return kSymmetric;
  }
  return state.checked(QStringLiteral("flip")) ? QStringLiteral("positive") : QStringLiteral("negative");
}

} // namespace

CommandDef cellGridCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.cell_grid");
  def.name = QObject::tr("Cell Grid");
  def.icon = QStringLiteral("cell-grid");
  def.tooltip = QObject::tr("Fills sketch profiles with cells and cuts them from bodies");
  def.keywords = {QStringLiteral("vent"),    QStringLiteral("grille"), QStringLiteral("grill"),
                  QStringLiteral("honeycomb"), QStringLiteral("hexagon"), QStringLiteral("perforate"),
                  QStringLiteral("lightening"), QStringLiteral("holes"), QStringLiteral("pattern")};
  def.featureType = QStringLiteral("cell_grid");
  def.inputs = {
      selectionInput(QStringLiteral("profiles"), QObject::tr("Profiles"), SelectKind::Profile, 1, 0)
          .withTooltip(QObject::tr("The field: profiles of one sketch; a loop inside one is kept clear")),
      choiceInput(QStringLiteral("shape"), QObject::tr("Shape"), kShapes, QStringLiteral("hexagon")),
      valueInput(QStringLiteral("sides"), QObject::tr("Sides"), ValueKind::Unitless, QStringLiteral("8"))
          .withTooltip(QObject::tr("A polygon cell's sides, 3 to 64"))
          .withVisible([](const CommandState& s) { return s.choice(QStringLiteral("shape")) == kPolygon; }),
      choiceInput(QStringLiteral("rows"), QObject::tr("Rows"), kRows, QStringLiteral("staggered"))
          .withTooltip(QObject::tr("How circles and polygons are laid out"))
          .withVisible([](const CommandState& s) {
            const QString shape = s.choice(QStringLiteral("shape"));
            return shape == kCircle || shape == kPolygon;
          }),
      valueInput(QStringLiteral("size"), QObject::tr("Size"), ValueKind::Length, QStringLiteral("5 mm"))
          .withTooltip(QObject::tr("A cell's inscribed diameter"))
          .withStepMinimum(0.0, true),
      // Its own disambiguation: the wall between cells, not the shared
      // "Web" of other commands (mitcad#2288).
      valueInput(QStringLiteral("web"), QObject::tr("Web", "Cell Grid: the wall between neighboring cells"),
                 ValueKind::Length, QStringLiteral("1.5 mm"))
          .withTooltip(QObject::tr("The wall between neighboring cells"))
          .withStepMinimum(0.0, true),
      choiceInput(QStringLiteral("extent"), QObject::tr("Extent"), kExtents, QStringLiteral("distance")),
      valueInput(QStringLiteral("depth"), QObject::tr("Depth"), ValueKind::Length, QStringLiteral("2 mm"))
          .withStepMinimum(0.0, true)
          .withVisible([](const CommandState& s) { return !throughAll(s); }),
      choiceInput(QStringLiteral("direction"), QObject::tr("Direction"), kDirections, QStringLiteral("one_side")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Cut along the sketch normal instead of into the face"))
          .withVisible([](const CommandState& s) { return !symmetric(s); }),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("Turns the grid about its anchor")),
      selectionInput(QStringLiteral("anchor"), QObject::tr("Anchor"),
                     SelectKind::Point | SelectKind::Vertex | SelectKind::SketchPoint, 0, 1)
          .withTooltip(QObject::tr("A cell's center sits here; none: the sketch's origin"))
          .withAccepts(isPointLike),
      choiceInput(QStringLiteral("clip"), QObject::tr("Edge Cells"), kEdgeCells, QStringLiteral("whole"))
          .withTooltip(QObject::tr("Cells an edge crosses: left out, cut along their own lines, or trimmed")),
      valueInput(QStringLiteral("margin"), QObject::tr("Margin"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("The least clearance from a cell to the edges"))
          .withStepMinimum(0.0)
          .withVisible([](const CommandState& s) { return s.choice(QStringLiteral("clip")) != kFree; }),
      selectionInput(QStringLiteral("targets"), QObject::tr("Bodies"), SelectKind::Body, 0, 0)
          .withTooltip(QObject::tr("The bodies cut; none: every body the cells reach"))
          .withoutMeshes(),
  };
  def.init = [](CommandState& state, const CommandContext& model) { newestProfile(state, model); };
  def.enabled = [&context] { return hasProfiles(context) && hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& profiles = state.items(QStringLiteral("profiles"));
    if (oneSketch(profiles, result, QStringLiteral("profiles")).isEmpty()) {
      return result;
    }
    const QString shape = state.choice(QStringLiteral("shape"));
    result.def = {{QStringLiteral("type"), QStringLiteral("cell_grid")},
                  {QStringLiteral("profiles"), refsOf(profiles)},
                  {QStringLiteral("shape"), shape},
                  {QStringLiteral("size"), state.expression(QStringLiteral("size"))},
                  {QStringLiteral("web"), state.expression(QStringLiteral("web"))},
                  {QStringLiteral("direction"), directionOf(state)},
                  {QStringLiteral("clip"), state.choice(QStringLiteral("clip"))}};
    if (shape == kPolygon) {
      result.def.insert(QStringLiteral("sides"), state.expression(QStringLiteral("sides")));
    }
    if ((shape == kPolygon || shape == kCircle) && state.choice(QStringLiteral("rows")) == QStringLiteral("square")) {
      result.def.insert(QStringLiteral("rows"), QStringLiteral("square"));
    }
    if (throughAll(state)) {
      result.def.insert(QStringLiteral("through_all"), true);
    } else {
      result.def.insert(QStringLiteral("depth"), state.expression(QStringLiteral("depth")));
    }
    // An angle and a margin only when they are not 0 (a parameter that is
    // 0 now stays linked: nonZero reads the value).
    if (nonZero(state, QStringLiteral("angle"))) {
      result.def.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
    }
    if (state.choice(QStringLiteral("clip")) != kFree && nonZero(state, QStringLiteral("margin"))) {
      result.def.insert(QStringLiteral("margin"), state.expression(QStringLiteral("margin")));
    }
    const Selection& anchor = state.items(QStringLiteral("anchor"));
    if (!anchor.isEmpty()) {
      result.def.insert(QStringLiteral("anchor"), anchor.first().reference());
    }
    const Selection& targets = state.items(QStringLiteral("targets"));
    if (!targets.isEmpty()) {
      result.def.insert(QStringLiteral("targets"), bodyUids(targets));
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("profiles"), profilesOf(feature.value(QStringLiteral("profiles")).toArray()));
    state.setChoice(QStringLiteral("shape"), feature.value(QStringLiteral("shape")).toString(QStringLiteral("hexagon")));
    loadCount(state, QStringLiteral("sides"), feature.value(QStringLiteral("sides")), feature, context);
    state.setChoice(QStringLiteral("rows"), feature.value(QStringLiteral("rows")).toString(QStringLiteral("staggered")));
    for (const QString& key : {QStringLiteral("size"), QStringLiteral("web"), QStringLiteral("depth"),
                               QStringLiteral("angle"), QStringLiteral("margin")}) {
      loadValue(state, key, feature.value(key), feature, context);
    }
    state.setChoice(QStringLiteral("extent"), feature.value(QStringLiteral("through_all")).toBool(false)
                                                  ? QStringLiteral("through_all")
                                                  : QStringLiteral("distance"));
    const QString direction = feature.value(QStringLiteral("direction")).toString(QStringLiteral("negative"));
    state.setChoice(QStringLiteral("direction"), direction == kSymmetric ? kSymmetric : QStringLiteral("one_side"));
    state.setChecked(QStringLiteral("flip"), direction == QStringLiteral("positive"));
    state.setChoice(QStringLiteral("clip"), feature.value(QStringLiteral("clip")).toString(QStringLiteral("whole")));
    const SelectionItem anchor = itemOf(feature.value(QStringLiteral("anchor")), context);
    if (anchor.isValid()) {
      state.setItems(QStringLiteral("anchor"), {anchor});
    }
    state.setItems(QStringLiteral("targets"), bodiesOf(feature.value(QStringLiteral("targets")).toArray()));
  };

  // An anchor the panel cannot show (none of the bodies before the feature
  // has it) would be dropped by OK.
  def.canEdit = [&context](const QJsonObject& feature) {
    const QJsonValue anchor = feature.value(QStringLiteral("anchor"));
    return anchor.isUndefined() || anchor.isNull() || isShowable(anchor, context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const QString extent = throughAll(state) ? QStringLiteral("through all")
                                             : QStringLiteral("depth %1").arg(number(state.value(QStringLiteral("depth"))));
    return QStringLiteral("Added cell grid (%1, %2 edge cells) of %3 profile(s): size %4, web %5, %6, %7")
        .arg(label(kShapes, state.choice(QStringLiteral("shape"))).toLower(),
             label(kEdgeCells, state.choice(QStringLiteral("clip"))).toLower())
        .arg(state.items(QStringLiteral("profiles")).size())
        .arg(number(state.value(QStringLiteral("size"))), number(state.value(QStringLiteral("web"))), extent,
             directionOf(state));
  };
  return def;
}

} // namespace mitcad::cmd
