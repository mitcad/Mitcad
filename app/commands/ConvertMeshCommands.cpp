// SPDX-License-Identifier: MIT
// Convert Mesh (mitcad#1082; commands.md, "Convert Mesh"): a mesh body made
// a solid, in the SOLID tab's MODIFY group and at the top of a mesh body's
// context menus. Faceted Solid (the default) welds the mesh's vertices,
// merges its triangles into planar faces and makes a new solid body, the
// mesh body hidden; Analytic rebuilds one profile extruded straight as
// planes and cylinders in place (mitcad#462).

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const Choices kConvertModes = {
    {QStringLiteral("faceted"), QStringLiteral("Faceted Solid")},
    {QStringLiteral("analytic"), QStringLiteral("Analytic")},
};

bool faceted(const CommandState& state) {
  return state.choice(QStringLiteral("mode")) != QStringLiteral("analytic");
}

// A mesh body, or a face of one (the view picks a mesh body's triangles as
// its face).
bool isMeshItem(const SelectionItem& item) {
  return (item.kind == SelectKind::Body || item.kind == SelectKind::Face) &&
         item.geometry == QStringLiteral("mesh");
}

} // namespace

CommandDef convertMeshCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.convert_mesh");
  def.name = QObject::tr("Convert Mesh");
  def.icon = QStringLiteral("convert-mesh");
  def.tooltip = QObject::tr("Makes a mesh body from an STL or OBJ file a solid that features can change");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("stl"), QStringLiteral("obj"), QStringLiteral("mesh"),
                  QStringLiteral("solid"), QStringLiteral("remix"), QStringLiteral("facets")};
  def.featureType = QStringLiteral("mesh_convert");
  def.inputs = {
      selectionInput(QStringLiteral("body"), QObject::tr("Mesh Body"), SelectKind::Body | SelectKind::Face, 1, 1)
          .withTooltip(QObject::tr("The mesh body to convert, picked in the view or the browser"))
          .withAccepts(isMeshItem)
          .withConvert([](const SelectionItem& item) {
            SelectionItem body{SelectKind::Body, item.owner, QString(), QStringLiteral("mesh")};
            body.occurrence = item.occurrence;
            return body;
          })
          .withRefusedHint([](const SelectionItem& item, const CommandState&) {
            return QObject::tr("Convert Mesh works on mesh bodies; pick a mesh body (the picked %1 is not one).")
                .arg(SelectionItem::kindText(item.kind));
          }),
      choiceInput(QStringLiteral("mode"), QObject::tr("Type"), kConvertModes, QStringLiteral("faceted"))
          .withTooltip(QObject::tr("Faceted Solid: any closed mesh, flat faces. Analytic: one straight extrusion")),
      valueInput(QStringLiteral("tolerance"), QObject::tr("Weld Tolerance"), ValueKind::Length,
                 QStringLiteral("0.01 mm"))
          .withLabel([](const CommandState& state) {
            return faceted(state) ? QObject::tr("Weld Tolerance") : QObject::tr("Tolerance");
          })
          .withTooltip(
              QObject::tr("Corners closer than this become one (Analytic: the faces' distance from the mesh)")),
      valueInput(QStringLiteral("angle"), QObject::tr("Merge Angle"), ValueKind::Angle, QStringLiteral("0.5 deg"))
          .withTooltip(QObject::tr("Neighbouring triangles whose normals lie within this angle become one flat "
                                   "face"))
          .withVisible(faceted),
  };
  def.enabled = [&context] { return hasBodies(context); };
  // At the top of a mesh body's context menus.
  def.offeredAtTop = [](const Selection& items) {
    if (items.isEmpty()) {
      return false;
    }
    const QString owner = items.first().owner;
    for (const SelectionItem& item : items) {
      if (!isMeshItem(item) || item.owner != owner) {
        return false;
      }
    }
    return true;
  };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& items = state.items(QStringLiteral("body"));
    if (items.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("mesh_convert")},
                  {QStringLiteral("body"), items.first().owner},
                  {QStringLiteral("tolerance"), state.expression(QStringLiteral("tolerance"))}};
    if (faceted(state)) {
      result.def.insert(QStringLiteral("mode"), QStringLiteral("faceted"));
      result.def.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("body"),
                   {SelectionItem{SelectKind::Body, str(feature, "body"), QString(), QStringLiteral("mesh")}});
    const bool facetedMode = feature.value(QStringLiteral("mode")).toString() == QStringLiteral("faceted");
    state.setChoice(QStringLiteral("mode"), facetedMode ? QStringLiteral("faceted") : QStringLiteral("analytic"));
    loadValue(state, QStringLiteral("tolerance"), feature.value(QStringLiteral("tolerance")), feature, context);
    // Without an angle the model's 0.5 degrees, which the input starts at.
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added convert mesh (%1) of %2, tolerance %3")
        .arg(label(kConvertModes, faceted(state) ? QStringLiteral("faceted") : QStringLiteral("analytic")))
        .arg(state.items(QStringLiteral("body")).value(0).owner)
        .arg(number(state.value(QStringLiteral("tolerance"))));
  };
  return def;
}

} // namespace mitcad::cmd
