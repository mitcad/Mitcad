// SPDX-License-Identifier: MIT
// Panelize (mitcad#2005; commands.md, "Panelize"): the picked planar faces
// of a body split off as panels of one thickness, bodies or components,
// step by step; Remove Remainder drops what is left. In the SOLID tab's
// MODIFY group, after Split Body.
#include <QJsonArray>
#include <QJsonObject>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {

CommandDef panelizeCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.panelize");
  def.name = QObject::tr("Panelize");
  def.icon = QStringLiteral("panelize");
  def.tooltip = QObject::tr("Splits planar faces off a body as panels of a set thickness, one step at a time");
  def.keywords = {QStringLiteral("panel"), QStringLiteral("cabinet"), QStringLiteral("board"),
                  QStringLiteral("plate"), QStringLiteral("carcass"), QStringLiteral("thickness")};
  def.featureType = QStringLiteral("panelize");
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
          .withoutMeshes()
          .withAccepts(isPlanar)
          .withTooltip(QObject::tr("Planar faces of one body, one panel each; faces that meet at an "
                                   "edge go in separate steps")),
      valueInput(QStringLiteral("thickness"), QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("18 mm"))
          .withStepMinimum(0.0)
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            const Selection& faces = state.items(QStringLiteral("faces"));
            if (faces.isEmpty()) {
              return std::nullopt;
            }
            const auto frame = frameOf(faces.first(), model);
            const auto center = centerOf({faces.first()}, model);
            if (!frame || !center) {
              return std::nullopt;
            }
            // Into the material, as the panel runs.
            Manipulator handle = arrow(*center, frame->normal.Reversed());
            handle.scale = itemsScale(faces, model);
            return handle;
          }),
      choiceInput(QStringLiteral("result"), QObject::tr("Result"),
                  {{QStringLiteral("bodies"), QStringLiteral("Bodies")},
                   {QStringLiteral("components"), QStringLiteral("Components")}},
                  QStringLiteral("bodies"))
          .withTooltip(QObject::tr("Panels as bodies, or each in a component of its own")),
      checkInput(QStringLiteral("remove_remainder"), QObject::tr("Remove Remainder"), false)
          .withTooltip(QObject::tr("Removes what is left of the body after this step")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& items = state.items(QStringLiteral("faces"));
    const QString body = oneBody(items, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("panelize")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("faces"), names(items, SelectKind::Face)},
                  {QStringLiteral("thickness"), state.expression(QStringLiteral("thickness"))}};
    if (state.choice(QStringLiteral("result")) == QStringLiteral("components")) {
      result.def.insert(QStringLiteral("result"), QStringLiteral("components"));
    }
    if (state.checked(QStringLiteral("remove_remainder"))) {
      result.def.insert(QStringLiteral("remove_remainder"), true);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    Selection items;
    for (const QJsonValue& face : feature.value(QStringLiteral("faces")).toArray()) {
      items.append(itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                      {QStringLiteral("face"), face}},
                          context));
    }
    state.setItems(QStringLiteral("faces"), items);
    loadValue(state, QStringLiteral("thickness"), feature.value(QStringLiteral("thickness")), feature, context);
    state.setChoice(QStringLiteral("result"), feature.value(QStringLiteral("result")).toString() ==
                                                      QStringLiteral("components")
                                                  ? QStringLiteral("components")
                                                  : QStringLiteral("bodies"));
    state.setChecked(QStringLiteral("remove_remainder"), feature.value(QStringLiteral("remove_remainder")).toBool());
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added panelize of %1 face(s)")
        .arg(names(state.items(QStringLiteral("faces")), SelectKind::Face).size());
  };
  return def;
}

} // namespace mitcad::cmd
