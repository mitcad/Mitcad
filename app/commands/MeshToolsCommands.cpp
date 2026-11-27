// SPDX-License-Identifier: MIT
// Mesh tools for reverse engineering (mitcad#1871; commands.md, "Mesh tools
// for reverse engineering"), in the SOLID tab's MODIFY group under Mesh,
// after Convert Mesh:
// - Mesh Section: a mesh body's sections into sketches on a plane, one or
//   a parallel series (the `mesh_section` command, previewed as it is
//   applied and taken back);
// - Repair Mesh, Decimate Mesh, Remove Region, Segment Mesh and Fit
//   Surface: features that make new bodies of a mesh body, the mesh body
//   hidden. Remove Region picks triangles with a lasso clicked round them
//   on a plane facing the view, or a brush clicked on the mesh.
#include <algorithm>
#include <cmath>

#include <QDebug>
#include <QJsonArray>
#include <QJsonObject>
#include <QtMath>

#include <gp_Ax3.hxx>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "MeshToolsSupport.hpp"

namespace mitcad::cmd {
// Shared with MeshShapeCommands.cpp (mitcad#2515, MeshToolsSupport.hpp).
namespace meshtools {

// A mesh body, or a face of one (the view picks a mesh body's triangles as
// its face).
bool isMeshPick(const SelectionItem& item) {
  return (item.kind == SelectKind::Body || item.kind == SelectKind::Face) &&
         item.geometry == QStringLiteral("mesh");
}

SelectionItem meshBodyOf(const SelectionItem& item) {
  SelectionItem body{SelectKind::Body, item.owner, QString(), QStringLiteral("mesh")};
  body.occurrence = item.occurrence;
  return body;
}

// The mesh body input every mesh tool starts with.
InputDef meshBodyInput(const QString& tool) {
  return selectionInput(QStringLiteral("body"), QObject::tr("Mesh Body"), SelectKind::Body | SelectKind::Face, 1, 1)
      .withTooltip(QObject::tr("The mesh body, picked in the view or the browser"))
      .withAccepts(isMeshPick)
      .withConvert(meshBodyOf)
      .withRefusedHint([tool](const SelectionItem& item, const CommandState&) {
        return QObject::tr("%1 works on mesh bodies; pick a mesh body (the picked %2 is not one).")
            .arg(tool, SelectionItem::kindText(item.kind));
      });
}

QString meshBody(const CommandState& state) {
  const Selection& items = state.items(QStringLiteral("body"));
  return items.isEmpty() ? QString() : items.first().owner;
}

void loadMeshBody(const QJsonObject& feature, CommandState& state) {
  state.setItems(QStringLiteral("body"),
                 {SelectionItem{SelectKind::Body, str(feature, "body"), QString(), QStringLiteral("mesh")}});
}

// At the top of a mesh body's context menus.
bool allMeshOfOneBody(const Selection& items) {
  if (items.isEmpty()) {
    return false;
  }
  const QString owner = items.first().owner;
  for (const SelectionItem& item : items) {
    if (!isMeshPick(item) || item.owner != owner) {
      return false;
    }
  }
  return true;
}

// A mesh body's triangle count (the mesh_stats query); 0 when unknown.
int triangleCount(const QString& body, const CommandContext& context) {
  if (body.isEmpty()) {
    return 0;
  }
  const QJsonObject stats =
      context.queryObject({{QStringLiteral("query"), QStringLiteral("mesh_stats")}, {QStringLiteral("body"), body}});
  return stats.value(QStringLiteral("triangles")).toInt();
}

QJsonArray pointJson(const gp_Pnt& p) {
  return QJsonArray{p.X(), p.Y(), p.Z()};
}

} // namespace meshtools

using namespace meshtools;

// Mesh Section -------------------------------------------------------------

CommandDef meshSectionCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_section");
  def.name = QObject::tr("Mesh Section");
  def.icon = QStringLiteral("mesh-section");
  def.tooltip = QObject::tr("Cuts a mesh body by a plane into a sketch of lines and arcs");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("section"), QStringLiteral("scan"),
                  QStringLiteral("reverse"), QStringLiteral("stl"), QStringLiteral("slice")};
  def.sketchCommand = true;
  def.inputs = {
      meshBodyInput(QObject::tr("Mesh Section")),
      selectionInput(QStringLiteral("plane"), QObject::tr("Plane"), kPlaneKinds, 1, 1)
          .withTooltip(QObject::tr("The plane of the first section: a plane or a planar face; a new sketch goes "
                                   "on it"))
          .withAccepts(isPlanar)
          .withoutMeshes(),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Tolerance"), ValueKind::Length, QStringLiteral("0.05 mm"))
          .withTooltip(QObject::tr("How far the lines and arcs may lie from the section's points")),
      valueInput(QStringLiteral("count"), QObject::tr("Sections"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The number of parallel sections, each in a sketch of its own")),
      valueInput(QStringLiteral("spacing"), QObject::tr("Spacing"), ValueKind::Length, QStringLiteral("5 mm"))
          .withTooltip(QObject::tr("The distance between neighboring sections, along the plane's normal"))
          .withVisible([](const CommandState& state) { return state.value(QStringLiteral("count")) > 1.0; }),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    const Selection& plane = state.items(QStringLiteral("plane"));
    if (body.isEmpty() || plane.isEmpty()) {
      return result;
    }
    const double tolerance = state.value(QStringLiteral("tolerance"));
    const double count = state.value(QStringLiteral("count"));
    const double spacing = state.value(QStringLiteral("spacing"));
    if (qIsNaN(tolerance) || qIsNaN(count)) {
      return result;
    }
    if (count < 1.0 || count != std::floor(count)) {
      return Built::failure(QObject::tr("The number of sections is a whole number, 1 or more."),
                            QStringLiteral("count"));
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("mesh_section")},
                        {QStringLiteral("body"), body},
                        {QStringLiteral("plane"), plane.first().reference()},
                        {QStringLiteral("tolerance"), tolerance}};
    if (count > 1.0) {
      if (qIsNaN(spacing)) {
        return result;
      }
      command.insert(QStringLiteral("count"), static_cast<int>(count));
      command.insert(QStringLiteral("spacing"), spacing);
    }
    result.def = {{QStringLiteral("commands"), QJsonArray{command}}};
    return result;
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added mesh section of %1, %2 section(s)")
        .arg(meshBody(state))
        .arg(number(state.value(QStringLiteral("count"))));
  };
  return def;
}

// Repair Mesh --------------------------------------------------------------

CommandDef meshRepairCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_repair");
  def.name = QObject::tr("Repair Mesh");
  def.icon = QStringLiteral("mesh-repair");
  def.tooltip = QObject::tr("Welds, orients and closes the holes of a mesh body, as a new body");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("repair"), QStringLiteral("weld"), QStringLiteral("holes"),
                  QStringLiteral("manifold"), QStringLiteral("orient"), QStringLiteral("stl")};
  def.featureType = QStringLiteral("mesh_repair");
  def.inputs = {
      meshBodyInput(QObject::tr("Repair Mesh")),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Weld Tolerance"), ValueKind::Length,
                 QStringLiteral("0.01 mm"))
          .withTooltip(QObject::tr("Corners closer than this become one")),
      checkInput(QStringLiteral("fill"), QObject::tr("Fill Holes"), true)
          .withTooltip(QObject::tr("Closes the holes up to the size below with new triangles")),
      valueInput(QStringLiteral("max_hole"), QObject::tr("Largest Hole"), ValueKind::Length, QStringLiteral("10 mm"))
          .withTooltip(QObject::tr("Holes up to this across are filled; larger ones stay open"))
          .withVisible([](const CommandState& state) { return state.checked(QStringLiteral("fill")); }),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.offeredAtTop = allMeshOfOneBody;
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_repair")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("tolerance"), state.expression(QStringLiteral("tolerance"))}};
    if (state.checked(QStringLiteral("fill"))) {
      result.def.insert(QStringLiteral("max_hole"), state.expression(QStringLiteral("max_hole")));
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    loadValue(state, QStringLiteral("tolerance"), feature.value(QStringLiteral("tolerance")), feature, context);
    const bool fill = feature.contains(QStringLiteral("max_hole"));
    state.setChecked(QStringLiteral("fill"), fill);
    if (fill) {
      loadValue(state, QStringLiteral("max_hole"), feature.value(QStringLiteral("max_hole")), feature, context);
    }
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added mesh repair of %1, weld tolerance %2")
        .arg(meshBody(state))
        .arg(number(state.value(QStringLiteral("tolerance"))));
  };
  return def;
}

// Decimate Mesh ------------------------------------------------------------

namespace {

const Choices kReduceTo = {
    {QStringLiteral("count"), QStringLiteral("Triangle Count")},
    {QStringLiteral("tolerance"), QStringLiteral("Tolerance")},
};

bool byCount(const CommandState& state) {
  return state.choice(QStringLiteral("reduce")) != QStringLiteral("tolerance");
}

} // namespace

CommandDef meshDecimateCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_decimate");
  def.name = QObject::tr("Decimate Mesh");
  def.icon = QStringLiteral("mesh-decimate");
  def.tooltip = QObject::tr("Makes a mesh body of fewer triangles, keeping its outline and sharp edges");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("decimate"), QStringLiteral("reduce"),
                  QStringLiteral("simplify"), QStringLiteral("triangles"), QStringLiteral("stl")};
  def.featureType = QStringLiteral("mesh_decimate");
  def.inputs = {
      meshBodyInput(QObject::tr("Decimate Mesh")),
      choiceInput(QStringLiteral("reduce"), QObject::tr("Reduce To"), kReduceTo, QStringLiteral("count"))
          .withTooltip(QObject::tr("Triangle Count: a share of the triangles. Tolerance: as few as stay within it")),
      valueInput(QStringLiteral("keep"), QObject::tr("Keep (%)"), ValueKind::Unitless, QStringLiteral("50"))
          .withTooltip(QObject::tr("The share of the mesh's triangles to keep, in percent"))
          .withVisible(byCount),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Tolerance"), ValueKind::Length, QStringLiteral("0.05 mm"))
          .withTooltip(QObject::tr("How far the new surface may move from the mesh"))
          .withVisible([](const CommandState& state) { return !byCount(state); }),
      valueInput(QStringLiteral("sharp_angle"), QObject::tr("Sharp Edge Angle"), ValueKind::Angle,
                 QStringLiteral("30 deg"))
          .withTooltip(QObject::tr("Edges whose triangles meet at more than this are kept as they are")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext& context) {
    Built result;
    const QString body = meshBody(state);
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_decimate")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("sharp_angle"), state.expression(QStringLiteral("sharp_angle"))}};
    if (byCount(state)) {
      const double keep = state.value(QStringLiteral("keep"));
      if (qIsNaN(keep)) {
        return result;
      }
      if (keep <= 0.0 || keep > 100.0) {
        return Built::failure(QObject::tr("Keep a share of the triangles above 0 and at most 100 percent."),
                              QStringLiteral("keep"));
      }
      const int total = triangleCount(body, context);
      if (total <= 0) {
        return Built::failure(QObject::tr("The mesh body's triangles could not be counted."),
                              QStringLiteral("body"));
      }
      result.def.insert(QStringLiteral("triangles"), std::max(4, static_cast<int>(std::lround(total * keep / 100.0))));
    } else {
      result.def.insert(QStringLiteral("tolerance"), state.expression(QStringLiteral("tolerance")));
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    if (feature.contains(QStringLiteral("triangles"))) {
      state.setChoice(QStringLiteral("reduce"), QStringLiteral("count"));
      const int total = triangleCount(str(feature, "body"), context);
      const double count = feature.value(QStringLiteral("triangles")).toDouble();
      if (total > 0 && count > 0) {
        state.setText(QStringLiteral("keep"), number(std::min(100.0, 100.0 * count / total)));
      }
    } else {
      state.setChoice(QStringLiteral("reduce"), QStringLiteral("tolerance"));
      loadValue(state, QStringLiteral("tolerance"), feature.value(QStringLiteral("tolerance")), feature, context);
    }
    if (feature.contains(QStringLiteral("sharp_angle"))) {
      loadValue(state, QStringLiteral("sharp_angle"), feature.value(QStringLiteral("sharp_angle")), feature, context);
    }
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return byCount(state) ? QStringLiteral("Added mesh decimate of %1, keep %2 percent")
                                .arg(meshBody(state), number(state.value(QStringLiteral("keep"))))
                          : QStringLiteral("Added mesh decimate of %1, tolerance %2")
                                .arg(meshBody(state), number(state.value(QStringLiteral("tolerance"))));
  };
  return def;
}

// Remove Region ------------------------------------------------------------

namespace {

const Choices kRemoveTools = {
    {QStringLiteral("lasso"), QStringLiteral("Lasso")},
    {QStringLiteral("brush"), QStringLiteral("Brush")},
};

bool lassoTool(const CommandState& state) {
  return state.choice(QStringLiteral("tool")) != QStringLiteral("brush");
}

// The lasso's state: {"normal": [x, y, z], "points": [[x, y, z], ...]}.
QJsonArray lassoPoints(const CommandState& state) {
  return state.object(QStringLiteral("lasso")).value(QStringLiteral("points")).toArray();
}

QJsonArray brushPoints(const CommandState& state) {
  return state.object(QStringLiteral("brush_points")).value(QStringLiteral("points")).toArray();
}

} // namespace

CommandDef meshRemoveCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_remove");
  def.name = QObject::tr("Remove Region");
  def.icon = QStringLiteral("mesh-remove");
  def.tooltip = QObject::tr("Removes the triangles of a mesh body inside a lasso or under a brush");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("remove"), QStringLiteral("delete"),
                  QStringLiteral("lasso"), QStringLiteral("brush"), QStringLiteral("triangles"),
                  QStringLiteral("clean")};
  def.featureType = QStringLiteral("mesh_remove");
  def.inputs = {
      meshBodyInput(QObject::tr("Remove Region")),
      choiceInput(QStringLiteral("tool"), QObject::tr("Select With"), kRemoveTools, QStringLiteral("lasso"))
          .withTooltip(QObject::tr("Lasso: click round the triangles in the view. Brush: click on the mesh where "
                                   "triangles go")),
      selectionInput(QStringLiteral("brush"), QObject::tr("Brush"), SelectKind::Face, 0, 1)
          .withTooltip(QObject::tr("Click on the mesh: the triangles within the radius go; click again for more"))
          .withAccepts(isMeshPick)
          .withVisible([](const CommandState& state) { return !lassoTool(state); })
          .withOnPick([](const SelectionItem& item, CommandState& state, const CommandContext&) {
            if (!item.at) {
              return;
            }
            QJsonArray points = brushPoints(state);
            points.append(QJsonArray{(*item.at)[0], (*item.at)[1], (*item.at)[2]});
            state.setObject(QStringLiteral("brush_points"), QJsonObject{{QStringLiteral("points"), points}});
            qDebug().noquote() << QStringLiteral("Remove Region brush: %1 points").arg(points.size());
          }),
      valueInput(QStringLiteral("radius"), QObject::tr("Brush Radius"), ValueKind::Length, QStringLiteral("2 mm"))
          .withTooltip(QObject::tr("The triangles within this of a brush click go"))
          .withVisible([](const CommandState& state) { return !lassoTool(state); }),
      checkInput(QStringLiteral("through"), QObject::tr("Through"), false)
          .withTooltip(QObject::tr("Also the triangles inside the lasso that face away from the view"))
          .withVisible(lassoTool),
      checkInput(QStringLiteral("fill"), QObject::tr("Fill Hole"), false)
          .withTooltip(QObject::tr("Closes the hole the triangles leave with new triangles")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  // The lasso: clicks on a plane through the mesh's middle, facing the view
  // when the lasso began.
  def.cursorPlane = [&context](const CommandState& state) -> std::optional<gp_Ax3> {
    if (!lassoTool(state) || state.items(QStringLiteral("body")).isEmpty()) {
      return std::nullopt;
    }
    const std::optional<gp_Pnt> centre = centerOf(state.items(QStringLiteral("body")), context);
    if (!centre) {
      return std::nullopt;
    }
    const QJsonArray normal = state.object(QStringLiteral("lasso")).value(QStringLiteral("normal")).toArray();
    if (normal.size() == 3) {
      const gp_Vec n(normal.at(0).toDouble(), normal.at(1).toDouble(), normal.at(2).toDouble());
      if (n.Magnitude() > 1e-12) {
        return gp_Ax3(*centre, gp_Dir(n));
      }
    }
    const std::optional<gp_Dir> toward = context.towardViewer(*centre);
    if (!toward) {
      return std::nullopt;
    }
    return gp_Ax3(*centre, *toward);
  };
  def.cursorPlaneMoved = [](const std::optional<gp_Pnt>&, CommandState&, const CommandContext&) { return false; };
  def.cursorPlaneClicked = [&context](const gp_Pnt& point, CommandState& state, const CommandContext&) {
    QJsonObject lasso = state.object(QStringLiteral("lasso"));
    QJsonArray points = lasso.value(QStringLiteral("points")).toArray();
    if (points.isEmpty()) {
      const std::optional<gp_Pnt> centre = centerOf(state.items(QStringLiteral("body")), context);
      const std::optional<gp_Dir> toward = centre ? context.towardViewer(*centre) : std::nullopt;
      if (!toward) {
        return false;
      }
      lasso.insert(QStringLiteral("normal"), QJsonArray{toward->X(), toward->Y(), toward->Z()});
    }
    points.append(pointJson(point));
    lasso.insert(QStringLiteral("points"), points);
    state.setObject(QStringLiteral("lasso"), lasso);
    qDebug().noquote() << QStringLiteral("Remove Region lasso: %1 points").arg(points.size());
    return true;
  };
  // Backspace takes the last lasso point or brush click back.
  def.backspace = [](CommandState& state, const CommandContext&) {
    if (lassoTool(state)) {
      QJsonObject lasso = state.object(QStringLiteral("lasso"));
      QJsonArray points = lasso.value(QStringLiteral("points")).toArray();
      if (points.isEmpty()) {
        return false;
      }
      points.removeLast();
      if (points.isEmpty()) {
        lasso = QJsonObject();
      } else {
        lasso.insert(QStringLiteral("points"), points);
      }
      state.setObject(QStringLiteral("lasso"), lasso);
      return true;
    }
    QJsonArray points = brushPoints(state);
    if (points.isEmpty()) {
      return false;
    }
    points.removeLast();
    state.setObject(QStringLiteral("brush_points"), QJsonObject{{QStringLiteral("points"), points}});
    return true;
  };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_remove")}, {QStringLiteral("body"), body}};
    if (lassoTool(state)) {
      const QJsonObject lasso = state.object(QStringLiteral("lasso"));
      if (lassoPoints(state).size() < 3) {
        return Built::asking(QObject::tr("Click round the triangles to remove in the view, three points or more."));
      }
      result.def.insert(QStringLiteral("lassos"), QJsonArray{lasso});
      if (state.checked(QStringLiteral("through"))) {
        result.def.insert(QStringLiteral("through"), true);
      }
    } else {
      const QJsonArray points = brushPoints(state);
      if (points.isEmpty()) {
        return Built::asking(QObject::tr("Click on the mesh where triangles are to go."), QStringLiteral("brush"));
      }
      result.def.insert(QStringLiteral("brush"), points);
      result.def.insert(QStringLiteral("radius"), state.expression(QStringLiteral("radius")));
    }
    if (state.checked(QStringLiteral("fill"))) {
      result.def.insert(QStringLiteral("fill"), true);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    const QJsonArray lassos = feature.value(QStringLiteral("lassos")).toArray();
    if (!lassos.isEmpty()) {
      state.setChoice(QStringLiteral("tool"), QStringLiteral("lasso"));
      state.setObject(QStringLiteral("lasso"), lassos.first().toObject());
    } else {
      state.setChoice(QStringLiteral("tool"), QStringLiteral("brush"));
      state.setObject(QStringLiteral("brush_points"),
                      QJsonObject{{QStringLiteral("points"), feature.value(QStringLiteral("brush")).toArray()}});
      loadValue(state, QStringLiteral("radius"), feature.value(QStringLiteral("radius")), feature, context);
    }
    state.setChecked(QStringLiteral("through"), feature.value(QStringLiteral("through")).toBool());
    state.setChecked(QStringLiteral("fill"), feature.value(QStringLiteral("fill")).toBool());
  };
  // The panel holds one lasso or one brush's clicks, not picked triangles.
  def.canEdit = [](const QJsonObject& feature) {
    return feature.value(QStringLiteral("lassos")).toArray().size() <= 1 &&
           feature.value(QStringLiteral("triangles")).toArray().isEmpty() &&
           (feature.value(QStringLiteral("lassos")).toArray().isEmpty() ||
            feature.value(QStringLiteral("brush")).toArray().isEmpty());
  };
  def.editRefusal = [](const QJsonObject&) {
    return QObject::tr("This region was picked by several lassos or by triangle numbers, which the panel cannot "
                       "show; change it through its definition.");
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return lassoTool(state) ? QStringLiteral("Added mesh remove of %1 by a lasso of %2 points%3")
                                  .arg(meshBody(state))
                                  .arg(lassoPoints(state).size())
                                  .arg(state.checked(QStringLiteral("fill")) ? QStringLiteral(", filled") : QString())
                            : QStringLiteral("Added mesh remove of %1 by %2 brush clicks%3")
                                  .arg(meshBody(state))
                                  .arg(brushPoints(state).size())
                                  .arg(state.checked(QStringLiteral("fill")) ? QStringLiteral(", filled") : QString());
  };
  return def;
}

// Segment Mesh -------------------------------------------------------------

CommandDef meshSegmentCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_segment");
  def.name = QObject::tr("Segment Mesh");
  def.icon = QStringLiteral("mesh-segment");
  def.tooltip = QObject::tr("Splits a mesh body into planes, cylinders and free-form regions");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("segment"), QStringLiteral("regions"),
                  QStringLiteral("curvature"), QStringLiteral("split"), QStringLiteral("reverse")};
  def.featureType = QStringLiteral("mesh_segment");
  def.inputs = {
      meshBodyInput(QObject::tr("Segment Mesh")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("20 deg"))
          .withTooltip(QObject::tr("Neighboring triangles whose normals differ by more than this part regions")),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Tolerance"), ValueKind::Length, QStringLiteral("0.05 mm"))
          .withTooltip(QObject::tr("How far a region may lie from the plane or cylinder it is taken for")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_segment")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))},
                  {QStringLiteral("tolerance"), state.expression(QStringLiteral("tolerance"))}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
    loadValue(state, QStringLiteral("tolerance"), feature.value(QStringLiteral("tolerance")), feature, context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added mesh segment of %1, angle %2")
        .arg(meshBody(state), degrees(state.value(QStringLiteral("angle"))));
  };
  return def;
}

// Fit Surface --------------------------------------------------------------

namespace {

const Choices kFitSurfaces = {
    {QStringLiteral("auto"), QStringLiteral("Auto")},          {QStringLiteral("plane"), QStringLiteral("Plane")},
    {QStringLiteral("cylinder"), QStringLiteral("Cylinder")}, {QStringLiteral("cone"), QStringLiteral("Cone")},
    {QStringLiteral("sphere"), QStringLiteral("Sphere")},     {QStringLiteral("bspline"), QStringLiteral("B-Spline")},
};

} // namespace

CommandDef meshFitSurfaceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.mesh_fit_surface");
  def.name = QObject::tr("Fit Surface");
  def.icon = QStringLiteral("mesh-fit-surface");
  def.tooltip = QObject::tr("Fits a plane, cylinder, cone, sphere or B-spline surface to a mesh");
  def.keywords = {QStringLiteral("mesh"), QStringLiteral("fit"), QStringLiteral("surface"), QStringLiteral("reverse"),
                  QStringLiteral("plane"), QStringLiteral("cylinder"), QStringLiteral("bspline")};
  def.featureType = QStringLiteral("mesh_fit_surface");
  def.inputs = {
      meshBodyInput(QObject::tr("Fit Surface")),
      choiceInput(QStringLiteral("surface"), QObject::tr("Surface"), kFitSurfaces, QStringLiteral("auto"))
          .withTooltip(QObject::tr("Auto: the simplest that lies within the tolerance")),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Tolerance"), ValueKind::Length, QStringLiteral("0.05 mm"))
          .withTooltip(QObject::tr("How far the surface may lie from the mesh")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = meshBody(state);
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_fit_surface")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("tolerance"), state.expression(QStringLiteral("tolerance"))}};
    const QString surface = state.choice(QStringLiteral("surface"));
    if (!surface.isEmpty() && surface != QStringLiteral("auto")) {
      result.def.insert(QStringLiteral("surface"), surface);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    loadMeshBody(feature, state);
    const QString surface = feature.value(QStringLiteral("surface")).toString();
    state.setChoice(QStringLiteral("surface"), surface.isEmpty() ? QStringLiteral("auto") : surface);
    loadValue(state, QStringLiteral("tolerance"), feature.value(QStringLiteral("tolerance")), feature, context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added mesh fit surface (%1) of %2")
        .arg(label(kFitSurfaces, state.choice(QStringLiteral("surface"))), meshBody(state));
  };
  return def;
}

} // namespace mitcad::cmd
