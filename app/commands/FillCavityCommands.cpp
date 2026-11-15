// SPDX-License-Identifier: MIT
// Fill Cavity (mitcad#1818; commands.md, "Fill Cavity"): a closed pocket or
// slit of a body filled with material up to a plane, the cap. In the SOLID
// tab's MODIFY group, beside Replace Face.
#include <QJsonObject>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {

CommandDef fillCavityCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.fill_cavity");
  def.name = QObject::tr("Fill Cavity");
  def.icon = QStringLiteral("fill-cavity");
  def.tooltip = QObject::tr("Fills a closed pocket or slit of a body with material up to a plane");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("fill"), QStringLiteral("pocket"), QStringLiteral("slit"), QStringLiteral("cavity"),
                  QStringLiteral("close"), QStringLiteral("plug")};
  def.featureType = QStringLiteral("fill_cavity");
  def.inputs = {
      selectionInput(QStringLiteral("body"), QObject::tr("Body"), SelectKind::Body, 1, 1).withoutMeshes(),
      selectionInput(QStringLiteral("cap"), QObject::tr("Cap"), kPlaneKinds, 1, 1)
          .withTooltip(QObject::tr("The plane the cavity is filled up to: a plane or a planar face"))
          .withAccepts(isPlanar),
      selectionInput(QStringLiteral("seed"), QObject::tr("Seed Face"), SelectKind::Face, 0, 1)
          .withoutMeshes()
          .withTooltip(QObject::tr("A face inside the cavity: only its pockets are filled. "
                                   "Without one, every pocket the cap closes")),
      valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("Moves the cap along its normal")),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
          .withTooltip(QObject::tr("Fills on the other side of the cap")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = oneBody(state.items(QStringLiteral("body")), result, QStringLiteral("body"));
    if (body.isEmpty()) {
      return result;
    }
    const Selection& seed = state.items(QStringLiteral("seed"));
    if (!seed.isEmpty() && seed.first().owner != body) {
      return Built::failure(QObject::tr("Pick the seed face on the body to fill."), QStringLiteral("seed"));
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("fill_cavity")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("cap"), state.items(QStringLiteral("cap")).first().reference()}};
    if (!seed.isEmpty()) {
      result.def.insert(QStringLiteral("seed"), seed.first().name);
    }
    if (nonZero(state, QStringLiteral("offset"))) {
      result.def.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
    }
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString body = str(feature, "body");
    Selection bodies;
    bodies.append({SelectKind::Body, body, QString(), QString()});
    state.setItems(QStringLiteral("body"), bodies);
    state.setItems(QStringLiteral("cap"), {itemOf(feature.value(QStringLiteral("cap")), context)});
    if (feature.contains(QStringLiteral("seed"))) {
      state.setItems(QStringLiteral("seed"),
                     {itemOf(QJsonObject{{QStringLiteral("body"), body},
                                         {QStringLiteral("face"), feature.value(QStringLiteral("seed"))}},
                             context)});
    }
    loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
  };

  // A fixed plane (an import's) has no input.
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("cap")), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return state.items(QStringLiteral("seed")).isEmpty() ? QStringLiteral("Added fill cavity up to the cap")
                                                         : QStringLiteral("Added fill cavity of the seed's pocket");
  };
  return def;
}

} // namespace mitcad::cmd
