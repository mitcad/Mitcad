// SPDX-License-Identifier: MIT
// Mesh tools that change a mesh's shape (mitcad#2515; commands.md, "Mesh
// tools: Smooth, Combine, Offset and Shell, Align, Plane Cut"), in the SOLID
// tab's MODIFY group under Mesh, after the reverse engineering tools:
// - Smooth Mesh: a mesh body, or the triangles a brush touches, smoothed;
// - Combine Meshes: mesh bodies joined, cut, intersected or merged;
// - Offset Mesh: a mesh body offset, or shelled into a closed wall with
//   openings a brush picks;
// - Align Mesh: a mesh body moved onto another body by picked point pairs
//   and a best fit;
// - Plane Cut: a mesh body cut by a plane into two mesh bodies.
// Each is a feature that makes new mesh bodies and hides the mesh bodies it
// starts from.
#include <functional>

#include <QDebug>
#include <QJsonArray>
#include <QJsonObject>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "MeshToolsSupport.hpp"

namespace mitcad::cmd {

using namespace meshtools;

namespace {

// A mesh body input other than "body" (Combine's target and tools).
InputDef meshInput(const QString& id, const QString& label, const QString& tool, int min, int max) {
  return selectionInput(id, label, SelectKind::Body | SelectKind::Face, min, max)
      .withAccepts(isMeshPick)
      .withConvert(meshBodyOf)
      .withRefusedHint([tool](const SelectionItem& item, const CommandState&) {
        return QObject::tr("%1 works on mesh bodies; pick a mesh body (the picked %2 is not one).")
            .arg(tool, SelectionItem::kindText(item.kind));
      });
}

// Any body, a face picking its body (Align's target: a mesh, a solid or a
// surface body).
SelectionItem bodyOfPick(const SelectionItem& item) {
  SelectionItem body{SelectKind::Body, item.owner, QString(),
                     item.geometry == QStringLiteral("mesh") ? QStringLiteral("mesh") : QString()};
  body.occurrence = item.occurrence;
  return body;
}

// A brush's clicks on the mesh, kept in the state object `key` as
// {"points": [[x, y, z], ...]}: the input that takes them, shown while
// `shown` holds, and the radius beside it.
QJsonArray brushPointsOf(const CommandState& state, const QString& key) {
  return state.object(key).value(QStringLiteral("points")).toArray();
}

InputDef brushInput(const QString& key, const QString& tool, std::function<bool(const CommandState&)> shown) {
  return selectionInput(QStringLiteral("brush"), QObject::tr("Brush"), SelectKind::Face, 0, 1)
      .withTooltip(QObject::tr("Click on the mesh where triangles are taken; click again for more"))
      .withAccepts(isMeshPick)
      .withVisible(shown)
      .withOnPick([key, tool](const SelectionItem& item, CommandState& state, const CommandContext&) {
        if (!item.at) {
          return;
        }
        QJsonArray points = brushPointsOf(state, key);
        points.append(QJsonArray{(*item.at)[0], (*item.at)[1], (*item.at)[2]});
        state.setObject(key, QJsonObject{{QStringLiteral("points"), points}});
        qDebug().noquote() << QStringLiteral("%1 brush: %2 points").arg(tool).arg(points.size());
      });
}

InputDef brushRadiusInput(std::function<bool(const CommandState&)> shown) {
  return valueInput(QStringLiteral("radius"), QObject::tr("Brush Radius"), ValueKind::Length, QStringLiteral("2 mm"))
      .withTooltip(QObject::tr("The triangles within this of a brush click are taken"))
      .withVisible(shown);
}

// The brush's region {"brush": [...], "radius": ...}; empty without clicks.
QJsonObject brushRegion(const CommandState& state, const QString& key) {
  const QJsonArray points = brushPointsOf(state, key);
  if (points.isEmpty()) {
    return QJsonObject();
  }
  return {{QStringLiteral("brush"), points}, {QStringLiteral("radius"), state.expression(QStringLiteral("radius"))}};
}

void loadBrushRegion(const QJsonObject& region, CommandState& state, const QString& key, const QJsonObject& feature,
                     const CommandContext& context) {
  state.setObject(key, QJsonObject{{QStringLiteral("points"), region.value(QStringLiteral("brush")).toArray()}});
  if (region.contains(QStringLiteral("radius"))) {
    loadValue(state, QStringLiteral("radius"), region.value(QStringLiteral("radius")), feature, context);
  }
}

// A region the panel can show: a brush's clicks only.
bool brushOnly(const QJsonValue& region) {
  const QJsonObject object = region.toObject();
  return object.value(QStringLiteral("lassos")).toArray().isEmpty() &&
         object.value(QStringLiteral("triangles")).toArray().isEmpty();
}

bool backspaceBrush(CommandState& state, const QString& key) {
  QJsonArray points = brushPointsOf(state, key);
  if (points.isEmpty()) {
    return false;
  }
  points.removeLast();
  state.setObject(key, QJsonObject{{QStringLiteral("points"), points}});
  return true;
}

} // namespace

// Smooth Mesh --------------------------------------------------------------

namespace {

const Choices kSmoothRegions = {
    {QStringLiteral("whole"), QStringLiteral("Whole Mesh")},
    {QStringLiteral("brush"), QStringLiteral("Brush")},
};

bool smoothByBrush(const CommandState& state) {
  return state.choice(QStringLiteral("region")) == QStringLiteral("brush");
}

} // namespace

CommandDef meshSmoothCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_smooth");
  def.name = QObject::tr("Smooth Mesh");
  def.icon = QStringLiteral("mesh-smooth");
  def.tooltip = QObject::tr("Takes the noise out of a mesh body, keeping sharp edges and its volume");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("smooth"), QStringLiteral("noise"), QStringLiteral("scan"),
                  QStringLiteral("fair"), QStringLiteral("stl")};
  def.featureType = QStringLiteral("mesh_smooth");
  def.inputs = {
      meshBodyInput(QObject::tr("Smooth Mesh")),
      valueInput(QStringLiteral("amount"), QObject::tr("Amount"), ValueKind::Unitless, QStringLiteral("0.5"))
          .withTooltip(QObject::tr("How far each pass moves a vertex towards its neighbors, above 0 and at most 1")),
      valueInput(QStringLiteral("passes"), QObject::tr("Passes"), ValueKind::Unitless, QStringLiteral("5"))
          .withTooltip(QObject::tr("How many times the mesh is smoothed")),
      checkInput(QStringLiteral("sharp"), QObject::tr("Keep Sharp Edges"), true)
          .withTooltip(QObject::tr("Edges sharper than the angle below stay sharp")),
      valueInput(QStringLiteral("sharp_angle"), QObject::tr("Sharp Edge Angle"), ValueKind::Angle,
                 QStringLiteral("30 deg"))
          .withTooltip(QObject::tr("Edges whose triangles meet at more than this are kept"))
          .withVisible([](const CommandState& state) { return state.checked(QStringLiteral("sharp")); }),
      checkInput(QStringLiteral("keep_volume"), QObject::tr("Keep Volume"), true)
          .withTooltip(QObject::tr("A closed mesh keeps the volume it encloses")),
      choiceInput(QStringLiteral("region"), QObject::tr("Region"), kSmoothRegions, QStringLiteral("whole"))
          .withTooltip(QObject::tr("Whole Mesh, or only the triangles a brush touches")),
      brushInput(QStringLiteral("brush_points"), QObject::tr("Smooth Mesh"), smoothByBrush),
      brushRadiusInput(smoothByBrush),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.offeredAtTop = allMeshOfOneBody;
  def.backspace = [](CommandState& state, const CommandContext&) {
    return smoothByBrush(state) && backspaceBrush(state, QStringLiteral("brush_points"));
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_smooth")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("amount"), state.expression(QStringLiteral("amount"))},
                  {QStringLiteral("passes"), state.expression(QStringLiteral("passes"))}};
    if (state.checked(QStringLiteral("sharp"))) {
      result.def.insert(QStringLiteral("sharp_angle"), state.expression(QStringLiteral("sharp_angle")));
    }
    if (!state.checked(QStringLiteral("keep_volume"))) {
      result.def.insert(QStringLiteral("keep_volume"), false);
    }
    if (smoothByBrush(state)) {
      const QJsonObject region = brushRegion(state, QStringLiteral("brush_points"));
      if (region.isEmpty()) {
        return Built::asking(QObject::tr("Click on the mesh where it is to be smoothed."), QStringLiteral("brush"));
      }
      result.def.insert(QStringLiteral("region"), region);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    loadValue(state, QStringLiteral("amount"), feature.value(QStringLiteral("amount")), feature, context);
    loadValue(state, QStringLiteral("passes"), feature.value(QStringLiteral("passes")), feature, context);
    const bool sharp = feature.contains(QStringLiteral("sharp_angle"));
    state.setChecked(QStringLiteral("sharp"), sharp);
    if (sharp) {
      loadValue(state, QStringLiteral("sharp_angle"), feature.value(QStringLiteral("sharp_angle")), feature, context);
    }
    state.setChecked(QStringLiteral("keep_volume"), feature.value(QStringLiteral("keep_volume")).toBool(true));
    const QJsonObject region = feature.value(QStringLiteral("region")).toObject();
    state.setChoice(QStringLiteral("region"), region.isEmpty() ? QStringLiteral("whole") : QStringLiteral("brush"));
    if (!region.isEmpty()) {
      loadBrushRegion(region, state, QStringLiteral("brush_points"), feature, context);
    }
  };
  def.canEdit = [](const QJsonObject& feature) {
    return !feature.contains(QStringLiteral("region")) || brushOnly(feature.value(QStringLiteral("region")));
  };
  def.editRefusal = [](const QJsonObject&) {
    return QObject::tr("This region was picked by lassos or triangle numbers, which the panel cannot show; change "
                       "it through its definition.");
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added mesh smooth of %1, %2 passes")
        .arg(meshBody(state), number(state.value(QStringLiteral("passes"))));
  };
  return def;
}

// Combine Meshes -----------------------------------------------------------

namespace {

const Choices kMeshCombineOperations = {
    {QStringLiteral("join"), QStringLiteral("Join")},
    {QStringLiteral("cut"), QStringLiteral("Cut")},
    {QStringLiteral("intersect"), QStringLiteral("Intersect")},
    {QStringLiteral("merge"), QStringLiteral("Merge")},
};

} // namespace

CommandDef meshCombineCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_combine");
  def.name = QObject::tr("Combine Meshes");
  def.icon = QStringLiteral("mesh-combine");
  def.tooltip = QObject::tr("Joins, cuts or intersects closed mesh bodies, or merges mesh bodies into one");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("combine"), QStringLiteral("boolean"),
                  QStringLiteral("union"), QStringLiteral("subtract"), QStringLiteral("merge"),
                  QStringLiteral("stl")};
  def.featureType = QStringLiteral("mesh_combine");
  def.inputs = {
      meshInput(QStringLiteral("target"), QObject::tr("Target Mesh"), QObject::tr("Combine Meshes"), 1, 1)
          .withTooltip(QObject::tr("The mesh body the tools are joined to, cut from or intersected with")),
      meshInput(QStringLiteral("tools"), QObject::tr("Tool Meshes"), QObject::tr("Combine Meshes"), 1, 0)
          .withTooltip(QObject::tr("The other mesh bodies")),
      choiceInput(QStringLiteral("operation"), QObject::tr("Operation"), kMeshCombineOperations,
                  QStringLiteral("join"))
          .withTooltip(QObject::tr("Merge puts the meshes into one body as they are, without a boolean")),
  };
  def.enabled = [&context] { return context.queryArray(QStringLiteral("bodies")).size() > 1; };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& targets = state.items(QStringLiteral("target"));
    const Selection& picked = state.items(QStringLiteral("tools"));
    if (targets.isEmpty() || picked.isEmpty()) {
      return result;
    }
    const QString target = targets.first().owner;
    QJsonArray tools;
    for (const SelectionItem& tool : picked) {
      if (tool.owner == target) {
        return Built::failure(QObject::tr("The target cannot be a tool too."), QStringLiteral("tools"));
      }
      if (!tools.contains(tool.owner)) {
        tools.append(tool.owner);
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_combine")},
                  {QStringLiteral("target"), target},
                  {QStringLiteral("tools"), tools},
                  {QStringLiteral("operation"), state.choice(QStringLiteral("operation"))}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    state.setItems(QStringLiteral("target"), {SelectionItem{SelectKind::Body, str(feature, "target"), QString(),
                                                            QStringLiteral("mesh")}});
    Selection tools;
    for (const QJsonValue& tool : feature.value(QStringLiteral("tools")).toArray()) {
      tools.append(SelectionItem{SelectKind::Body, tool.toString(), QString(), QStringLiteral("mesh")});
    }
    state.setItems(QStringLiteral("tools"), tools);
    state.setChoice(QStringLiteral("operation"), feature.value(QStringLiteral("operation")).toString());
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const Selection& targets = state.items(QStringLiteral("target"));
    return QStringLiteral("Added mesh combine (%1) of %2 with %3 tool(s)")
        .arg(label(kMeshCombineOperations, state.choice(QStringLiteral("operation"))),
             targets.isEmpty() ? QString() : targets.first().owner)
        .arg(state.items(QStringLiteral("tools")).size());
  };
  return def;
}

// Offset Mesh --------------------------------------------------------------

namespace {

const Choices kMeshOffsetModes = {
    {QStringLiteral("offset"), QStringLiteral("Offset Surface")},
    {QStringLiteral("shell"), QStringLiteral("Shell")},
};

const Choices kMeshOffsetSides = {
    {QStringLiteral("outward"), QStringLiteral("Outward")},
    {QStringLiteral("inward"), QStringLiteral("Inward")},
    {QStringLiteral("both"), QStringLiteral("Both Ways")},
};

const Choices kMeshOpenings = {
    {QStringLiteral("none"), QStringLiteral("None")},
    {QStringLiteral("brush"), QStringLiteral("Brush")},
};

bool shellMode(const CommandState& state) {
  return state.choice(QStringLiteral("mode")) == QStringLiteral("shell");
}

bool openingsByBrush(const CommandState& state) {
  return shellMode(state) && state.choice(QStringLiteral("openings")) == QStringLiteral("brush");
}

} // namespace

CommandDef meshOffsetCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_offset");
  def.name = QObject::tr("Offset Mesh");
  def.icon = QStringLiteral("mesh-offset");
  def.tooltip = QObject::tr("Offsets a mesh body by a distance, or shells it into a closed wall");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("offset"), QStringLiteral("shell"),
                  QStringLiteral("thicken"), QStringLiteral("hollow"), QStringLiteral("wall"),
                  QStringLiteral("stl")};
  def.featureType = QStringLiteral("mesh_offset");
  def.inputs = {
      meshBodyInput(QObject::tr("Offset Mesh")),
      choiceInput(QStringLiteral("mode"), QObject::tr("Type"), kMeshOffsetModes, QStringLiteral("offset"))
          .withTooltip(QObject::tr("Offset Surface: the surface moved. Shell: a closed wall of a thickness")),
      choiceInput(QStringLiteral("side"), QObject::tr("Direction"), kMeshOffsetSides, QStringLiteral("outward"))
          .withTooltip(QObject::tr("Outward: out of a closed mesh. Both Ways: a wall on either side")),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("1 mm"))
          .withTooltip(QObject::tr("How far each face of the mesh moves"))
          .withVisible([](const CommandState& state) { return !shellMode(state); }),
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("2 mm"))
          .withTooltip(QObject::tr("The thickness of the wall"))
          .withVisible(shellMode),
      choiceInput(QStringLiteral("openings"), QObject::tr("Openings"), kMeshOpenings, QStringLiteral("none"))
          .withTooltip(QObject::tr("None, or the triangles a brush touches taken out and the wall left open there"))
          .withVisible(shellMode),
      brushInput(QStringLiteral("opening_points"), QObject::tr("Offset Mesh"), openingsByBrush),
      brushRadiusInput(openingsByBrush),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.offeredAtTop = allMeshOfOneBody;
  def.backspace = [](CommandState& state, const CommandContext&) {
    return openingsByBrush(state) && backspaceBrush(state, QStringLiteral("opening_points"));
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    if (body.isEmpty()) {
      return result;
    }
    const bool shell = shellMode(state);
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_offset")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("distance"),
                   state.expression(shell ? QStringLiteral("thickness") : QStringLiteral("distance"))}};
    if (shell) {
      result.def.insert(QStringLiteral("mode"), QStringLiteral("shell"));
    }
    const QString side = state.choice(QStringLiteral("side"));
    if (!side.isEmpty() && side != QStringLiteral("outward")) {
      result.def.insert(QStringLiteral("side"), side);
    }
    if (openingsByBrush(state)) {
      const QJsonObject region = brushRegion(state, QStringLiteral("opening_points"));
      if (region.isEmpty()) {
        return Built::asking(QObject::tr("Click on the mesh where the wall is to be open."), QStringLiteral("brush"));
      }
      result.def.insert(QStringLiteral("openings"), region);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    const bool shell = feature.value(QStringLiteral("mode")).toString() == QStringLiteral("shell");
    state.setChoice(QStringLiteral("mode"), shell ? QStringLiteral("shell") : QStringLiteral("offset"));
    const QString side = feature.value(QStringLiteral("side")).toString();
    state.setChoice(QStringLiteral("side"), side.isEmpty() ? QStringLiteral("outward") : side);
    loadValue(state, shell ? QStringLiteral("thickness") : QStringLiteral("distance"),
              feature.value(QStringLiteral("distance")), feature, context);
    const QJsonObject openings = feature.value(QStringLiteral("openings")).toObject();
    state.setChoice(QStringLiteral("openings"), openings.isEmpty() ? QStringLiteral("none") : QStringLiteral("brush"));
    if (!openings.isEmpty()) {
      loadBrushRegion(openings, state, QStringLiteral("opening_points"), feature, context);
    }
  };
  def.canEdit = [](const QJsonObject& feature) {
    return !feature.contains(QStringLiteral("openings")) || brushOnly(feature.value(QStringLiteral("openings")));
  };
  def.editRefusal = [](const QJsonObject&) {
    return QObject::tr("These openings were picked by lassos or triangle numbers, which the panel cannot show; "
                       "change them through the definition.");
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return shellMode(state) ? QStringLiteral("Added mesh shell of %1, %2, thickness %3")
                                  .arg(meshBody(state), state.choice(QStringLiteral("side")),
                                       number(state.value(QStringLiteral("thickness"))))
                            : QStringLiteral("Added mesh offset of %1, %2, distance %3")
                                  .arg(meshBody(state), state.choice(QStringLiteral("side")),
                                       number(state.value(QStringLiteral("distance"))));
  };
  return def;
}

// Align Mesh ---------------------------------------------------------------

namespace {

// The picked points, in turn on the mesh and on the target:
// {"points": [[x, y, z], ...], "owners": ["F2.b0", ...]}.
const QString kPairPicks = QStringLiteral("pair_points");

QJsonArray pairPoints(const CommandState& state) {
  return state.object(kPairPicks).value(QStringLiteral("points")).toArray();
}

} // namespace

CommandDef meshAlignCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_align");
  def.name = QObject::tr("Align Mesh");
  def.icon = QStringLiteral("mesh-align");
  def.tooltip = QObject::tr("Moves a mesh body onto another body by point pairs and a best fit");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("align"), QStringLiteral("register"),
                  QStringLiteral("best fit"), QStringLiteral("icp"), QStringLiteral("scan")};
  def.featureType = QStringLiteral("mesh_align");
  def.inputs = {
      meshBodyInput(QObject::tr("Align Mesh")),
      selectionInput(QStringLiteral("target"), QObject::tr("Target"), SelectKind::Body | SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The body the mesh is moved onto: a mesh, a solid or a surface body"))
          .withConvert(bodyOfPick),
      selectionInput(QStringLiteral("pairs"), QObject::tr("Point Pairs"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("A point on the mesh, then where it belongs on the target; three pairs or more"))
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext&) {
            if (!item.at) {
              return;
            }
            // In the clicked body's component's coordinates, as the click
            // gives it: a feature works on its own component's bodies, and
            // mesh_align's pairs are in that component's coordinates, also
            // where an occurrence places it moved and turned (mitcad#2566;
            // Align Point Cloud's pairs are the design's instead).
            QJsonObject picks = state.object(kPairPicks);
            QJsonArray points = picks.value(QStringLiteral("points")).toArray();
            QJsonArray owners = picks.value(QStringLiteral("owners")).toArray();
            points.append(QJsonArray{(*item.at)[0], (*item.at)[1], (*item.at)[2]});
            owners.append(item.owner);
            state.setObject(kPairPicks,
                            QJsonObject{{QStringLiteral("points"), points}, {QStringLiteral("owners"), owners}});
            qDebug().noquote() << QStringLiteral("Align Mesh points: %1").arg(points.size());
          }),
      checkInput(QStringLiteral("best_fit"), QObject::tr("Best Fit"), true)
          .withTooltip(QObject::tr("Then moves the mesh to lie as close to the target as it can")),
  };
  def.enabled = [&context] { return context.queryArray(QStringLiteral("bodies")).size() > 1; };
  // Backspace takes the last picked point back.
  def.backspace = [](CommandState& state, const CommandContext&) {
    QJsonObject picks = state.object(kPairPicks);
    QJsonArray points = picks.value(QStringLiteral("points")).toArray();
    QJsonArray owners = picks.value(QStringLiteral("owners")).toArray();
    if (points.isEmpty()) {
      return false;
    }
    points.removeLast();
    if (!owners.isEmpty()) {
      owners.removeLast();
    }
    state.setObject(kPairPicks, QJsonObject{{QStringLiteral("points"), points}, {QStringLiteral("owners"), owners}});
    return true;
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    const Selection& targets = state.items(QStringLiteral("target"));
    if (body.isEmpty() || targets.isEmpty()) {
      return result;
    }
    const QString target = targets.first().owner;
    if (target == body) {
      return Built::failure(QObject::tr("Pick another body as the target: the mesh is moved onto it."),
                            QStringLiteral("target"));
    }
    const QJsonArray points = pairPoints(state);
    const QJsonArray owners = state.object(kPairPicks).value(QStringLiteral("owners")).toArray();
    QJsonArray pairs;
    for (int k = 0; k + 1 < points.size(); k += 2) {
      if (owners.at(k).toString() != body || owners.at(k + 1).toString() != target) {
        return Built::failure(
            QObject::tr("Pick each pair's first point on the mesh and its second on the target; Backspace takes "
                        "points back."),
            QStringLiteral("pairs"));
      }
      pairs.append(QJsonObject{{QStringLiteral("from"), points.at(k)}, {QStringLiteral("to"), points.at(k + 1)}});
    }
    if (points.size() % 2 == 1) {
      return Built::asking(QObject::tr("Click where the last point belongs on the target."), QStringLiteral("pairs"));
    }
    if (!pairs.isEmpty() && pairs.size() < 3) {
      return Built::asking(QObject::tr("Pick three point pairs or more, or none for a best fit only."),
                           QStringLiteral("pairs"));
    }
    const bool bestFit = state.checked(QStringLiteral("best_fit"));
    if (pairs.isEmpty() && !bestFit) {
      return Built::asking(QObject::tr("Pick three point pairs, or turn Best Fit on."), QStringLiteral("pairs"));
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_align")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("target"), target}};
    if (!pairs.isEmpty()) {
      result.def.insert(QStringLiteral("pairs"), pairs);
    }
    if (!bestFit) {
      result.def.insert(QStringLiteral("best_fit"), false);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    loadMeshBody(feature, state);
    const QString target = str(feature, "target");
    state.setItems(QStringLiteral("target"), {SelectionItem{SelectKind::Body, target, QString(), QString()}});
    QJsonArray points;
    QJsonArray owners;
    for (const QJsonValue& pair : feature.value(QStringLiteral("pairs")).toArray()) {
      points.append(pair.toObject().value(QStringLiteral("from")));
      owners.append(str(feature, "body"));
      points.append(pair.toObject().value(QStringLiteral("to")));
      owners.append(target);
    }
    state.setObject(kPairPicks, QJsonObject{{QStringLiteral("points"), points}, {QStringLiteral("owners"), owners}});
    state.setChecked(QStringLiteral("best_fit"), feature.value(QStringLiteral("best_fit")).toBool(true));
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    const Selection& targets = state.items(QStringLiteral("target"));
    return QStringLiteral("Added mesh align of %1 onto %2, %3 point pairs%4")
        .arg(meshBody(state), targets.isEmpty() ? QString() : targets.first().owner)
        .arg(pairPoints(state).size() / 2)
        .arg(state.checked(QStringLiteral("best_fit")) ? QStringLiteral(", best fit") : QString());
  };
  return def;
}

// Plane Cut ----------------------------------------------------------------

namespace {

const Choices kMeshCutKeep = {
    {QStringLiteral("both"), QStringLiteral("Both Pieces")},
    {QStringLiteral("above"), QStringLiteral("Above the Plane")},
    {QStringLiteral("below"), QStringLiteral("Below the Plane")},
};

} // namespace

CommandDef meshPlaneCutCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_plane_cut");
  def.name = QObject::tr("Plane Cut");
  def.icon = QStringLiteral("mesh-plane-cut");
  def.tooltip = QObject::tr("Cuts a mesh body by a plane into two mesh bodies, the cut capped or open");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("cut"), QStringLiteral("plane"), QStringLiteral("split"),
                  QStringLiteral("trim"), QStringLiteral("cap"), QStringLiteral("stl")};
  def.featureType = QStringLiteral("mesh_plane_cut");
  def.inputs = {
      meshBodyInput(QObject::tr("Plane Cut")),
      selectionInput(QStringLiteral("plane"), QObject::tr("Plane"), kPlaneKinds, 1, 1)
          .withTooltip(QObject::tr("The cutting plane: a plane or a planar face"))
          .withAccepts(isPlanar)
          .withoutMeshes(),
      choiceInput(QStringLiteral("keep"), QObject::tr("Keep"), kMeshCutKeep, QStringLiteral("both"))
          .withTooltip(QObject::tr("Above: the side the plane's normal points to")),
      checkInput(QStringLiteral("cap"), QObject::tr("Cap the Cut"), true)
          .withTooltip(QObject::tr("Closes each piece where it was cut, so that a closed mesh stays closed")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.offeredAtTop = allMeshOfOneBody;
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    const Selection& plane = state.items(QStringLiteral("plane"));
    if (body.isEmpty() || plane.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_plane_cut")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("plane"), plane.first().reference()}};
    const QString keep = state.choice(QStringLiteral("keep"));
    if (!keep.isEmpty() && keep != QStringLiteral("both")) {
      result.def.insert(QStringLiteral("keep"), keep);
    }
    if (!state.checked(QStringLiteral("cap"))) {
      result.def.insert(QStringLiteral("cap"), false);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    state.setItems(QStringLiteral("plane"), {itemOf(feature.value(QStringLiteral("plane")), context)});
    const QString keep = feature.value(QStringLiteral("keep")).toString();
    state.setChoice(QStringLiteral("keep"), keep.isEmpty() ? QStringLiteral("both") : keep);
    state.setChecked(QStringLiteral("cap"), feature.value(QStringLiteral("cap")).toBool(true));
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added mesh plane cut of %1, keep %2%3")
        .arg(meshBody(state), state.choice(QStringLiteral("keep")),
             state.checked(QStringLiteral("cap")) ? QStringLiteral(", capped") : QString());
  };
  return def;
}

} // namespace mitcad::cmd
