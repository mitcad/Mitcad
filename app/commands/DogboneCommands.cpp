// SPDX-License-Identifier: MIT
// Dogbone (mitcad#1011; commands.md, "Dogbone"): relief cuts at a body's
// inside corners, so that a router bit clears the sharp corner a square
// part fits into. In the SOLID tab's MODIFY group.
#include <QJsonArray>

#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {
namespace {

const Choices kDogboneTypes = {
    {QStringLiteral("diagonal"), QStringLiteral("Diagonal")},
    {QStringLiteral("along_side"), QStringLiteral("Along a Side")},
};

const Choices kToolAxes = {
    {QStringLiteral("z"), QStringLiteral("Z")},
    {QStringLiteral("y"), QStringLiteral("Y")},
    {QStringLiteral("x"), QStringLiteral("X")},
};

// Whether the picks take corners by a face or the body, whose edges must
// run along the tool axis (picked edges are taken whatever their direction).
bool byAxis(const CommandState& state) {
  for (const SelectionItem& item : state.items(QStringLiteral("picks"))) {
    if (item.kind != SelectKind::Edge) {
      return true;
    }
  }
  return false;
}

// The tool axis choice of a definition's `axis`; empty for one that is no
// axis of the component.
QString axisChoice(const QJsonValue& axis) {
  if (axis.isUndefined() || axis.isNull()) {
    return QStringLiteral("z");
  }
  const QJsonArray v = axis.toArray();
  if (v.size() != 3) {
    return QString();
  }
  const double x = v.at(0).toDouble();
  const double y = v.at(1).toDouble();
  const double z = v.at(2).toDouble();
  if (x == 0.0 && y == 0.0 && z > 0.0) {
    return QStringLiteral("z");
  }
  if (x == 0.0 && z == 0.0 && y > 0.0) {
    return QStringLiteral("y");
  }
  if (y == 0.0 && z == 0.0 && x > 0.0) {
    return QStringLiteral("x");
  }
  return QString();
}

} // namespace

CommandDef dogboneCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.dogbone");
  def.name = QObject::tr("Dogbone");
  def.icon = QStringLiteral("dogbone");
  def.tooltip = QObject::tr("Relief cuts at inside corners, so that a router bit clears them");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("cnc"), QStringLiteral("router"), QStringLiteral("relief"),
                  QStringLiteral("t-bone"), QStringLiteral("corner")};
  def.featureType = QStringLiteral("dogbone");
  def.inputs = {
      selectionInput(QStringLiteral("picks"), QObject::tr("Corners"),
                     SelectKind::Edge | SelectKind::Face | SelectKind::Body, 1, 0)
          .withoutMeshes()
          .withTooltip(QObject::tr("Inside corner edges, faces whose inside corners run along the tool axis, "
                                   "or a body for all of them"))
          .withAccepts([](const SelectionItem& item) { return item.kind != SelectKind::Edge || isLine(item); }),
      valueInput(QStringLiteral("diameter"), QObject::tr("Tool Diameter"), ValueKind::Length,
                 QStringLiteral("6.35 mm")),
      valueInput(QStringLiteral("extra"), QObject::tr("Extra"), ValueKind::Length, QStringLiteral("0.1 mm"))
          .withTooltip(QObject::tr("How far the relief reaches beyond the sharp corner")),
      choiceInput(QStringLiteral("dogbone_type"), QObject::tr("Type"), kDogboneTypes, QStringLiteral("diagonal"))
          .withTooltip(QObject::tr("Diagonal: along the corner's bisector. Along a Side: along the longer face "
                                   "or the face picked")),
      choiceInput(QStringLiteral("axis"), QObject::tr("Tool Axis"), kToolAxes, QStringLiteral("z"))
          .withTooltip(QObject::tr("The router bit's axis: faces and bodies give the corners along it"))
          .withVisible(byAxis),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const Selection& items = state.items(QStringLiteral("picks"));
    const QString body = oneBody(items, result, QStringLiteral("picks"));
    if (body.isEmpty()) {
      return result;
    }
    bool whole = false;
    for (const SelectionItem& item : items) {
      whole = whole || item.kind == SelectKind::Body;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("dogbone")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("diameter"), state.expression(QStringLiteral("diameter"))},
                  {QStringLiteral("extra"), state.expression(QStringLiteral("extra"))}};
    // A picked body takes every corner; its edges and faces add nothing.
    if (!whole) {
      const QJsonArray edges = names(items, SelectKind::Edge);
      const QJsonArray faces = names(items, SelectKind::Face);
      if (!edges.isEmpty()) {
        result.def.insert(QStringLiteral("edges"), edges);
      }
      if (!faces.isEmpty()) {
        result.def.insert(QStringLiteral("faces"), faces);
      }
    }
    if (state.choice(QStringLiteral("dogbone_type")) == QStringLiteral("along_side")) {
      result.def.insert(QStringLiteral("dogbone_type"), QStringLiteral("along_side"));
    }
    const QString axis = state.choice(QStringLiteral("axis"));
    if (byAxis(state) && axis != QStringLiteral("z")) {
      result.def.insert(QStringLiteral("axis"), axis == QStringLiteral("x") ? QJsonArray{1, 0, 0}
                                                                              : QJsonArray{0, 1, 0});
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString body = str(feature, "body");
    Selection items;
    for (const QJsonValue& edge : feature.value(QStringLiteral("edges")).toArray()) {
      items.append({SelectKind::Edge, body, edge.toString(), QString()});
    }
    for (const QJsonValue& face : feature.value(QStringLiteral("faces")).toArray()) {
      items.append(itemOf(QJsonObject{{QStringLiteral("body"), body}, {QStringLiteral("face"), face}}, context));
    }
    if (items.isEmpty()) {
      items.append({SelectKind::Body, body, QString(), QString()});
    }
    state.setItems(QStringLiteral("picks"), items);
    // Without a value the model's defaults, which the inputs start at.
    loadValue(state, QStringLiteral("diameter"), feature.value(QStringLiteral("diameter")), feature, context);
    loadValue(state, QStringLiteral("extra"), feature.value(QStringLiteral("extra")), feature, context);
    state.setChoice(QStringLiteral("dogbone_type"), feature.value(QStringLiteral("dogbone_type")).toString() ==
                                                            QStringLiteral("along_side")
                                                        ? QStringLiteral("along_side")
                                                        : QStringLiteral("diagonal"));
    state.setChoice(QStringLiteral("axis"), axisChoice(feature.value(QStringLiteral("axis"))));
  };

  // A tool axis that is no axis of the component has no input.
  def.canEdit = [](const QJsonObject& feature) {
    return !axisChoice(feature.value(QStringLiteral("axis"))).isEmpty();
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added dogbone (%1), tool diameter %2")
        .arg(label(kDogboneTypes, state.choice(QStringLiteral("dogbone_type"))))
        .arg(number(state.value(QStringLiteral("diameter"))));
  };
  return def;
}

} // namespace mitcad::cmd
