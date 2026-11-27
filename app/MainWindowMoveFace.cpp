// SPDX-License-Identifier: MIT
// Move Face in the selection (mitcad#531; app/COMMANDS.md, "Move Face"): a
// Move Face's timeline cell selects the faces it moved (they keep their
// names, so no face carries the feature's own), and the selection card
// names it: "MoveFace1: 2 faces of Body1, translate (-1.856, 0, -0.361) mm"
// (commands/MoveFaceText). Logged "Move Face card: ..." when it changes.
#include "MainWindow.hpp"

#include <exception>
#include <optional>

#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>

#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "commands/MoveFaceText.hpp"

namespace mitcad {

namespace {

// The definition of a Move Face, or an empty object for other features.
QJsonObject moveFaceDef(const QString& uid, const DocumentSnapshot& snapshot, const CommandContext& model) {
  const DocumentSnapshot::Feature* feature = snapshot.feature(uid);
  if (feature == nullptr || feature->type != QStringLiteral("move_face")) {
    return QJsonObject();
  }
  try {
    return model.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}})
        .value(QStringLiteral("def"))
        .toObject();
  } catch (const std::exception&) {
    return QJsonObject(); // gone from the model
  }
}

} // namespace

TopoDS_Shape MainWindow::movedFacesShape(const QString& uid) const {
  const QJsonObject def = moveFaceDef(uid, m_snapshot, model());
  if (def.isEmpty()) {
    return TopoDS_Shape();
  }
  TopoDS_Compound compound;
  BRep_Builder builder;
  builder.MakeCompound(compound);
  bool any = false;
  for (const QJsonValue& face : def.value(QStringLiteral("faces")).toArray()) {
    const TopoDS_Shape shape = itemShape(
        {SelectKind::Face, def.value(QStringLiteral("body")).toString(), face.toString(), QString()});
    if (!shape.IsNull()) {
      builder.Add(compound, shape);
      any = true;
    }
  }
  return any ? TopoDS_Shape(compound) : TopoDS_Shape();
}

QString MainWindow::moveFaceCard(const SelectionItem& item) const {
  if (item.kind != SelectKind::Feature) {
    return QString();
  }
  const QJsonObject def = moveFaceDef(item.owner, m_snapshot, model());
  if (def.isEmpty()) {
    return QString();
  }
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(item.owner);
  const QString bodyUid = def.value(QStringLiteral("body")).toString();
  const auto body = m_snapshot.bodies.constFind(bodyUid);
  // A parameter the motion names, by its value.
  QJsonArray parameters;
  try {
    parameters = queryArray(QStringLiteral("parameters"));
  } catch (const std::exception&) {
    // Shown by name.
  }
  const cmd::SlotValue value = [parameters](const QJsonValue& slot) -> std::optional<double> {
    for (const QJsonValue& parameter : parameters) {
      const QJsonObject p = parameter.toObject();
      if (p.value(QStringLiteral("name")).toString() == slot.toString()) {
        return p.value(QStringLiteral("value")).toDouble();
      }
    }
    return std::nullopt;
  };
  const QString summary = cmd::moveFaceSummary(
      feature != nullptr ? feature->name : item.owner,
      static_cast<int>(def.value(QStringLiteral("faces")).toArray().size()),
      body == m_snapshot.bodies.cend() || body->name.isEmpty() ? bodyUid : body->name,
      cmd::moveFaceMotion(def), value);
  if (summary != m_loggedMoveFaceCard) {
    m_loggedMoveFaceCard = summary;
    qDebug().noquote() << QStringLiteral("Move Face card: %1").arg(summary);
  }
  return QStringLiteral("<b>%1</b><br>%2").arg(tr("Move Face"), summary.toHtmlEscaped());
}

} // namespace mitcad
